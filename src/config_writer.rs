//! Safe config mutations.
//!
//! Every write is: read → modify in memory → verify the file on disk is still
//! byte-identical to what was read (so a concurrent edit by Claude Code or an
//! editor is never clobbered) → copy to `.bak` → write a temp file with the
//! original's permissions → fsync → atomic rename. Symlinked configs are
//! written through to their target. Each write returns a [`Change`] that can
//! later be reverted exactly.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::jsonc;
use crate::scope;
use crate::types::{ClientKind, Ctx, McpServer, Transport};

/// One file's before/after contents. `None` means "file did not exist".
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: Option<String>,
}

impl Change {
    /// The change that undoes this one.
    pub fn inverse(&self) -> Change {
        Change {
            path: self.path.clone(),
            before: self.after.clone(),
            after: self.before.clone(),
        }
    }
}

/// Outcome of adding a server to one client.
#[derive(Debug)]
pub struct Added {
    pub overwrote: bool,
    pub change: Change,
}

// ---------------------------------------------------------------------------
// Building server entries
// ---------------------------------------------------------------------------

/// Build a server entry from a transport (wizard input).
pub fn server_value(transport: &Transport, env: &BTreeMap<String, String>) -> Value {
    let mut obj = Map::new();
    let string_map = |m: &BTreeMap<String, String>| {
        Value::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect(),
        )
    };
    match transport {
        Transport::Stdio { command, args } => {
            obj.insert("command".into(), Value::String(command.clone()));
            if !args.is_empty() {
                obj.insert(
                    "args".into(),
                    Value::Array(args.iter().cloned().map(Value::String).collect()),
                );
            }
        }
        Transport::Http { url, headers } | Transport::Sse { url, headers } => {
            obj.insert("type".into(), Value::String(transport.kind_label().into()));
            obj.insert("url".into(), Value::String(url.clone()));
            if !headers.is_empty() {
                obj.insert("headers".into(), string_map(headers));
            }
        }
        Transport::Unknown => {}
    }
    if !env.is_empty() {
        obj.insert("env".into(), string_map(env));
    }
    Value::Object(obj)
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

/// True if `name` already exists in the exact scope `add_server` would write
/// to for this client. Read/parse failures count as absent (no false alarm).
pub fn server_exists(client: ClientKind, ctx: &Ctx, name: &str) -> bool {
    let Some(path) = client.config_path(ctx) else {
        return false;
    };
    let Ok(doc) = Doc::load(&path) else {
        return false;
    };
    scope::servers(&doc.root, client.layout(), &ctx.cwd)
        .ok()
        .flatten()
        .is_some_and(|m| m.contains_key(name))
}

/// Whether `add_server` can target this client right now (e.g. CC-Local needs
/// an existing project entry in ~/.claude.json).
pub fn can_add_to(client: ClientKind, ctx: &Ctx) -> bool {
    if client == ClientKind::ClaudeCodeLocal {
        let Some(path) = client.config_path(ctx) else {
            return false;
        };
        let Ok(mut doc) = Doc::load(&path) else {
            return false;
        };
        return scope::servers_mut(&mut doc.root, client.layout(), &ctx.cwd, true).is_ok();
    }
    client.config_path(ctx).is_some()
}

/// Add (or overwrite) a server in a client's config file.
pub fn add_server(
    client: ClientKind,
    ctx: &Ctx,
    name: &str,
    value: &Value,
) -> Result<Added, String> {
    let path = client
        .config_path(ctx)
        .ok_or_else(|| format!("{} is read-only or its location is unknown", client.label()))?;
    let mut doc = Doc::load(&path)?;
    let map = scope::servers_mut(&mut doc.root, client.layout(), &ctx.cwd, true)?
        .ok_or("could not create server container")?;
    let overwrote = map.insert(name.to_string(), value.clone()).is_some();
    Ok(Added {
        overwrote,
        change: doc.save()?,
    })
}

/// Remove a server from the file it was discovered in.
/// `Ok(None)` if it is no longer there (nothing written).
pub fn remove_server(server: &McpServer, ctx: &Ctx) -> Result<Option<Change>, String> {
    let mut doc = Doc::load(&server.source_path)?;
    let removed = scope::servers_mut(&mut doc.root, server.client.layout(), &ctx.cwd, false)?
        .is_some_and(|m| m.shift_remove(&server.name).is_some());
    if removed {
        doc.save().map(Some)
    } else {
        Ok(None)
    }
}

/// Revert a set of changes (newest last). All files are verified first, so
/// either every file is restored or — on a detected conflict — none is.
/// Returns the inverse changes in the order applied (newest last), which
/// `revert` accepts to redo the operation.
pub fn revert(changes: &[Change]) -> Result<Vec<Change>, String> {
    // One operation may touch a file more than once (CC-Global + CC-Local both
    // live in ~/.claude.json): only the latest change per file must match disk;
    // earlier ones are verified step by step as the later ones are undone.
    let mut latest: Vec<&Change> = Vec::new();
    for c in changes.iter().rev() {
        if !latest.iter().any(|l| l.path == c.path) {
            latest.push(c);
        }
    }
    for c in latest {
        if read_opt(&c.path)? != c.after {
            return Err(format!(
                "{} was modified after this change — not touching it",
                c.path.display()
            ));
        }
    }
    let mut done = Vec::new();
    for c in changes.iter().rev() {
        let inv = c.inverse();
        if let Err(e) = replace_file(&inv.path, inv.before.as_deref(), inv.after.as_deref()) {
            return Err(format!(
                "{} (restored {} of {} files)",
                e,
                done.len(),
                changes.len()
            ));
        }
        done.push(inv);
    }
    Ok(done)
}

// ---------------------------------------------------------------------------
// Document I/O
// ---------------------------------------------------------------------------

/// A config file loaded for modification.
struct Doc {
    /// Real path (symlinks resolved), so writes update the link target.
    path: PathBuf,
    original: Option<String>,
    root: Value,
    had_comments: bool,
}

impl Doc {
    fn load(path: &Path) -> Result<Doc, String> {
        let path = resolve(path)?;
        let original = read_opt(&path)?;
        let (root, had_comments) = match original.as_deref() {
            None => (Value::Object(Map::new()), false),
            Some(t) if t.trim().is_empty() => (Value::Object(Map::new()), false),
            Some(t) => {
                let p = jsonc::parse(t)
                    .map_err(|e| format!("invalid JSON in {}: {}", path.display(), e))?;
                (p.value, p.had_comments)
            }
        };
        Ok(Doc {
            path,
            original,
            root,
            had_comments,
        })
    }

    fn save(self) -> Result<Change, String> {
        if self.had_comments {
            return Err(format!(
                "{} contains comments that a rewrite would delete — edit it manually ([e])",
                self.path.display()
            ));
        }
        let text = serialize(&self.root, self.original.as_deref())?;
        replace_file(&self.path, self.original.as_deref(), Some(&text))?;
        Ok(Change {
            path: self.path,
            before: self.original,
            after: Some(text),
        })
    }
}

/// Serialize matching the original file's indentation, line endings and
/// trailing newline, so a one-server edit doesn't reformat the whole file.
fn serialize(value: &Value, original: Option<&str>) -> Result<String, String> {
    let indent = original
        .and_then(detect_indent)
        .unwrap_or_else(|| "  ".to_string());
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    serde::Serialize::serialize(value, &mut ser)
        .map_err(|e| format!("failed to serialize JSON: {}", e))?;
    let mut text = String::from_utf8(buf).map_err(|e| e.to_string())?;
    if original.is_none_or(|o| o.ends_with('\n')) {
        text.push('\n');
    }
    if original.is_some_and(|o| o.contains("\r\n")) {
        text = text.replace('\n', "\r\n");
    }
    Ok(text)
}

fn detect_indent(text: &str) -> Option<String> {
    text.lines()
        .skip(1)
        .map(|l| l.trim_end_matches('\r'))
        .find(|l| l.starts_with([' ', '\t']) && !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| *c == ' ' || *c == '\t').collect())
}

fn read_opt(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("failed to read {}: {}", path.display(), e)),
    }
}

/// Follow a symlink to the file it points at; non-links are returned as-is.
fn resolve(path: &Path) -> Result<PathBuf, String> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => std::fs::canonicalize(path)
            .map_err(|e| format!("broken symlink {}: {}", path.display(), e)),
        _ => Ok(path.to_path_buf()),
    }
}

/// Replace a file's contents, but only if it still holds `expected`
/// (`None` = must not exist). `new = None` deletes the file.
fn replace_file(path: &Path, expected: Option<&str>, new: Option<&str>) -> Result<(), String> {
    if read_opt(path)?.as_deref() != expected {
        return Err(format!(
            "{} changed on disk since it was read — refresh ([r]) and retry",
            path.display()
        ));
    }
    if expected.is_some() {
        let bak = with_suffix(path, ".bak");
        std::fs::copy(path, &bak)
            .map_err(|e| format!("failed to create backup {}: {}", bak.display(), e))?;
    }
    match new {
        Some(text) => write_atomic(path, text),
        None => std::fs::remove_file(path)
            .map_err(|e| format!("failed to delete {}: {}", path.display(), e)),
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directory {}: {}", parent.display(), e))?;
    }
    let tmp = with_suffix(path, &format!(".mcpm-{}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        // Apply the original's permissions before any content lands, so a
        // 0600 config holding API keys is never briefly world-readable.
        if let Ok(meta) = std::fs::metadata(path) {
            f.set_permissions(meta.permissions())?;
        }
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    result.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("failed to write {}: {}", path.display(), e)
    })
}

/// Append a suffix to a path's filename ("foo.json" + ".bak" → "foo.json.bak").
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::discover;
    use crate::testutil::TempDir;
    use serde_json::json;

    fn stdio_val() -> Value {
        server_value(
            &Transport::Stdio {
                command: "node".into(),
                args: vec!["server.js".into()],
            },
            &BTreeMap::new(),
        )
    }

    fn find(ctx: &Ctx, client: ClientKind, name: &str) -> McpServer {
        discover(ctx)
            .servers
            .into_iter()
            .find(|s| s.client == client && s.name == name)
            .expect("server discovered")
    }

    const P: ClientKind = ClientKind::ClaudeCodeProject;

    #[test]
    fn add_reports_fresh_then_overwrite() {
        let t = TempDir::new();
        let ctx = t.ctx();
        assert!(!add_server(P, &ctx, "foo", &stdio_val()).unwrap().overwrote);
        assert!(add_server(P, &ctx, "foo", &stdio_val()).unwrap().overwrote);
        let text = std::fs::read_to_string(ctx.cwd.join(".mcp.json")).unwrap();
        assert!(
            text.starts_with("{\n  \"mcpServers\""),
            "new .mcp.json is wrapped: {}",
            text
        );
    }

    #[test]
    fn exists_check_matches_what_add_writes() {
        let t = TempDir::new();
        let ctx = t.ctx();
        assert!(!server_exists(P, &ctx, "foo"));
        add_server(P, &ctx, "foo", &stdio_val()).unwrap();
        assert!(server_exists(P, &ctx, "foo"));
        assert!(!server_exists(P, &ctx, "bar"));
        let s = find(&ctx, P, "foo");
        remove_server(&s, &ctx).unwrap().unwrap();
        assert!(!server_exists(P, &ctx, "foo"));
    }

    #[test]
    fn remove_missing_is_honest_noop() {
        let t = TempDir::new();
        let ctx = t.ctx();
        add_server(P, &ctx, "foo", &stdio_val()).unwrap();
        let s = find(&ctx, P, "foo");
        let before = std::fs::read_to_string(&s.source_path).unwrap();
        let mut ghost = s.clone();
        ghost.name = "does-not-exist".into();
        assert_eq!(remove_server(&ghost, &ctx), Ok(None));
        assert_eq!(before, std::fs::read_to_string(&s.source_path).unwrap());
        assert!(remove_server(&s, &ctx).unwrap().is_some());
        assert_eq!(remove_server(&s, &ctx), Ok(None));
    }

    #[test]
    fn preserves_key_order_formatting_and_other_fields() {
        let t = TempDir::new();
        let ctx = t.ctx();
        let path = ctx.cwd.join(".cursor/mcp.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = "{\n\t\"zSetting\": true,\n\t\"mcpServers\": {\n\t\t\"zz\": {\"command\": \"z\"},\n\t\t\"aa\": {\"command\": \"a\"}\n\t},\n\t\"aSetting\": 1\n}";
        std::fs::write(&path, original).unwrap();
        add_server(ClientKind::CursorProject, &ctx, "mm", &stdio_val()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let keys: Vec<&str> = [
            "zSetting",
            "mcpServers",
            "aSetting",
            "\"zz\"",
            "\"aa\"",
            "\"mm\"",
        ]
        .into_iter()
        .collect();
        let pos: Vec<usize> = keys.iter().map(|k| text.find(k).unwrap()).collect();
        assert!(
            pos.windows(2).take(2).all(|w| w[0] < w[1]),
            "root order kept:\n{}",
            text
        );
        assert!(
            pos[3] < pos[4] && pos[4] < pos[5],
            "server order kept:\n{}",
            text
        );
        assert!(
            text.contains("\n\t\"zSetting\""),
            "tab indent kept:\n{}",
            text
        );
        assert!(!text.ends_with('\n'), "no trailing newline added");
    }

    #[test]
    fn refuses_to_clobber_concurrent_edit() {
        let t = TempDir::new();
        let ctx = t.ctx();
        let path = ctx.cwd.join(".mcp.json");
        std::fs::write(&path, "{}").unwrap();
        let mut doc = Doc::load(&path).unwrap();
        doc.root["mcpServers"] = json!({"x": {}});
        std::fs::write(&path, r#"{"someoneElse": true}"#).unwrap();
        let err = doc.save().unwrap_err();
        assert!(err.contains("changed on disk"), "{}", err);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"someoneElse": true}"#
        );
    }

    #[test]
    fn refuses_to_strip_comments() {
        let t = TempDir::new();
        let ctx = t.ctx();
        let path = ctx.cwd.join(".vscode/mcp.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ // keep me\n \"servers\": {}}").unwrap();
        let err = add_server(ClientKind::VsCodeProject, &ctx, "x", &stdio_val()).unwrap_err();
        assert!(err.contains("comments"), "{}", err);
    }

    #[test]
    fn vscode_writes_into_existing_mcpservers_key() {
        let t = TempDir::new();
        let ctx = t.ctx();
        let path = ctx.cwd.join(".vscode/mcp.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"mcpServers": {"a": {"command": "a"}}}"#).unwrap();
        add_server(ClientKind::VsCodeProject, &ctx, "b", &stdio_val()).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(v.get("servers").is_none());
        assert!(v["mcpServers"]["b"].is_object());
        let a = find(&ctx, ClientKind::VsCodeProject, "a");
        assert!(remove_server(&a, &ctx).unwrap().is_some());
    }

    #[test]
    fn revert_restores_exactly_and_detects_conflicts() {
        let t = TempDir::new();
        let ctx = t.ctx();
        let path = ctx.cwd.join(".mcp.json");
        // Undo of a create deletes the file again.
        let created = add_server(P, &ctx, "a", &stdio_val()).unwrap().change;
        let redo = revert(std::slice::from_ref(&created)).unwrap();
        assert!(!path.exists());
        revert(&redo).unwrap();
        assert!(server_exists(P, &ctx, "a"));

        // A file edited after the change is left alone.
        let ch = add_server(P, &ctx, "b", &stdio_val()).unwrap().change;
        std::fs::write(&path, "{}").unwrap();
        assert!(revert(&[ch]).unwrap_err().contains("modified after"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
    }

    #[test]
    fn revert_handles_several_changes_to_one_file() {
        let t = TempDir::new();
        let ctx = t.ctx();
        let cfg = t.home().join(".claude.json");
        let cwd = ctx.cwd.to_string_lossy().into_owned();
        let original = json!({"projects": {cwd: {}}}).to_string();
        std::fs::write(&cfg, &original).unwrap();
        let a = add_server(ClientKind::ClaudeCodeGlobal, &ctx, "x", &stdio_val())
            .unwrap()
            .change;
        let b = add_server(ClientKind::ClaudeCodeLocal, &ctx, "x", &stdio_val())
            .unwrap()
            .change;
        let redo = revert(&[a, b]).unwrap();
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), original);
        revert(&redo).unwrap();
        assert!(server_exists(ClientKind::ClaudeCodeGlobal, &ctx, "x"));
        assert!(server_exists(ClientKind::ClaudeCodeLocal, &ctx, "x"));
    }

    #[test]
    fn cc_local_add_and_remove_touch_only_this_project() {
        let t = TempDir::new();
        let ctx = t.ctx();
        let cfg = t.home().join(".claude.json");
        let cwd = ctx.cwd.to_string_lossy().into_owned();
        assert!(
            !can_add_to(ClientKind::ClaudeCodeLocal, &ctx),
            "no file yet"
        );
        std::fs::write(
            &cfg,
            json!({"mcpServers": {"g": {"command": "g"}}, "projects": {cwd.clone(): {"history": [1]}, "/o": {"mcpServers": {"x": {}}}}}).to_string(),
        )
        .unwrap();
        assert!(can_add_to(ClientKind::ClaudeCodeLocal, &ctx));
        add_server(ClientKind::ClaudeCodeLocal, &ctx, "x", &stdio_val()).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert_eq!(v["projects"][&cwd]["history"], json!([1]));
        assert!(v["projects"][&cwd]["mcpServers"]["x"].is_object());
        assert!(v["mcpServers"].get("x").is_none(), "global scope untouched");

        let s = find(&ctx, ClientKind::ClaudeCodeLocal, "x");
        remove_server(&s, &ctx).unwrap().unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert!(
            v["projects"]["/o"]["mcpServers"]["x"].is_object(),
            "other project untouched"
        );
    }

    #[cfg(unix)]
    #[test]
    fn keeps_permissions_and_writes_through_symlinks() {
        use std::os::unix::fs::PermissionsExt;
        let t = TempDir::new();
        let ctx = t.ctx();
        let real = t.home().join("dotfiles-claude.json");
        std::fs::write(&real, "{}").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&real, t.home().join(".claude.json")).unwrap();

        add_server(ClientKind::ClaudeCodeGlobal, &ctx, "s", &stdio_val()).unwrap();

        let link = std::fs::symlink_metadata(t.home().join(".claude.json")).unwrap();
        assert!(link.file_type().is_symlink(), "symlink preserved");
        let mode = std::fs::metadata(&real).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert!(std::fs::read_to_string(&real).unwrap().contains("\"s\""));
        assert!(t.home().join("dotfiles-claude.json.bak").exists());
    }
}
