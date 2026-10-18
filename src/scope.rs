//! Where servers live inside a config file. Discovery (read) and config_writer
//! (write) both go through this module, so they can never disagree about scope.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// Shape of the server container inside a config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Servers live under the first of these root keys that holds an object;
    /// a new container is created under the first key.
    Keyed(&'static [&'static str]),
    /// `.mcp.json`: under `mcpServers` when present, otherwise flat at the root.
    WrappedOrFlat,
    /// `~/.claude.json` → `projects[<cwd>].mcpServers`.
    ProjectEntry,
}

const WRAPPER: &str = "mcpServers";

/// Read-only access to the server map. `Ok(None)` means "no container yet".
/// `Err` means the file has a container of the wrong type.
pub fn servers<'a>(
    root: &'a Value,
    layout: Layout,
    cwd: &Path,
) -> Result<Option<&'a Map<String, Value>>, String> {
    let Some(obj) = root.as_object() else {
        return Err("top-level value is not a JSON object".to_string());
    };
    match layout {
        Layout::Keyed(keys) => keyed(obj, keys),
        Layout::WrappedOrFlat => match obj.get(WRAPPER) {
            Some(Value::Object(m)) => Ok(Some(m)),
            Some(Value::Null) | None => Ok(Some(obj)),
            Some(_) => Err(format!("\"{}\" is not an object", WRAPPER)),
        },
        Layout::ProjectEntry => {
            let Some(projects) = obj.get("projects").and_then(Value::as_object) else {
                return Ok(None);
            };
            let Some(key) = find_project_key(projects, cwd) else {
                return Ok(None);
            };
            match projects[&key].get(WRAPPER) {
                Some(Value::Object(m)) => Ok(Some(m)),
                Some(Value::Null) | None => Ok(None),
                Some(_) => Err(format!("projects[{}].{} is not an object", key, WRAPPER)),
            }
        }
    }
}

fn keyed<'a>(
    obj: &'a Map<String, Value>,
    keys: &[&str],
) -> Result<Option<&'a Map<String, Value>>, String> {
    for key in keys {
        match obj.get(*key) {
            Some(Value::Object(m)) => return Ok(Some(m)),
            Some(Value::Null) | None => continue,
            Some(_) => return Err(format!("\"{}\" is not an object", key)),
        }
    }
    Ok(None)
}

/// Mutable access to the server map. With `create`, a missing container is
/// created; without it, `Ok(None)` is returned instead.
pub fn servers_mut<'a>(
    root: &'a mut Value,
    layout: Layout,
    cwd: &Path,
    create: bool,
) -> Result<Option<&'a mut Map<String, Value>>, String> {
    let Some(obj) = root.as_object_mut() else {
        return Err("top-level value is not a JSON object".to_string());
    };
    match layout {
        Layout::Keyed(keys) => {
            let existing = keys
                .iter()
                .find(|k| obj.get(**k).is_some_and(|v| !v.is_null()))
                .copied();
            let key = match existing {
                Some(k) => k,
                None if create => {
                    obj.insert(keys[0].to_string(), Value::Object(Map::new()));
                    keys[0]
                }
                None => return Ok(None),
            };
            match obj.get_mut(key) {
                Some(Value::Object(m)) => Ok(Some(m)),
                _ => Err(format!("\"{}\" is not an object", key)),
            }
        }
        Layout::WrappedOrFlat => {
            // A brand-new (empty) file gets the documented wrapped shape.
            if obj.is_empty() && create {
                obj.insert(WRAPPER.to_string(), Value::Object(Map::new()));
            }
            if obj.get(WRAPPER).is_some_and(|v| !v.is_null()) {
                match obj.get_mut(WRAPPER) {
                    Some(Value::Object(m)) => Ok(Some(m)),
                    _ => Err(format!("\"{}\" is not an object", WRAPPER)),
                }
            } else {
                obj.remove(WRAPPER);
                Ok(Some(obj))
            }
        }
        Layout::ProjectEntry => {
            let no_entry = || {
                format!(
                    "no Claude Code project entry for {} — run `claude` in this directory once first",
                    cwd.display()
                )
            };
            let Some(projects) = obj.get_mut("projects").and_then(Value::as_object_mut) else {
                return if create { Err(no_entry()) } else { Ok(None) };
            };
            let Some(key) = find_project_key(projects, cwd) else {
                return if create { Err(no_entry()) } else { Ok(None) };
            };
            let Some(entry) = projects.get_mut(&key).and_then(Value::as_object_mut) else {
                return Err(format!("projects[{}] is not an object", key));
            };
            if entry.get(WRAPPER).is_none_or(Value::is_null) {
                if !create {
                    return Ok(None);
                }
                entry.insert(WRAPPER.to_string(), Value::Object(Map::new()));
            }
            match entry.get_mut(WRAPPER) {
                Some(Value::Object(m)) => Ok(Some(m)),
                _ => Err(format!("projects[{}].{} is not an object", key, WRAPPER)),
            }
        }
    }
}

/// Find the `projects` key in `~/.claude.json` that refers to `cwd`.
/// Claude Code keys projects by absolute path; compare tolerantly (separator
/// style, trailing slash, case on Windows, symlinked vs. canonical cwd).
pub fn find_project_key(projects: &Map<String, Value>, cwd: &Path) -> Option<String> {
    let mut wanted = vec![normalize(&cwd.to_string_lossy())];
    if let Ok(canon) = std::fs::canonicalize(cwd) {
        wanted.push(normalize(&strip_verbatim(canon).to_string_lossy()));
    }
    projects
        .keys()
        .find(|k| wanted.contains(&normalize(k)))
        .cloned()
}

fn normalize(p: &str) -> String {
    let mut s = p.replace('\\', "/");
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    if cfg!(windows) { s.to_lowercase() } else { s }
}

/// `canonicalize` on Windows yields `\\?\C:\...`; Claude Code stores `C:\...`.
fn strip_verbatim(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CWD: &str = "/work/proj";

    #[test]
    fn keyed_prefers_first_existing_key() {
        let root = json!({"mcpServers": {"a": {}}});
        let m = servers(
            &root,
            Layout::Keyed(&["servers", "mcpServers"]),
            Path::new(CWD),
        )
        .unwrap()
        .unwrap();
        assert!(m.contains_key("a"));
    }

    #[test]
    fn keyed_rejects_wrong_container_type() {
        let root = json!({"mcpServers": []});
        assert!(servers(&root, Layout::Keyed(&["mcpServers"]), Path::new(CWD)).is_err());
        let mut root = root;
        assert!(
            servers_mut(
                &mut root,
                Layout::Keyed(&["mcpServers"]),
                Path::new(CWD),
                true
            )
            .is_err()
        );
        assert_eq!(root, json!({"mcpServers": []}), "must not clobber");
    }

    #[test]
    fn wrapped_or_flat_new_file_is_wrapped() {
        let mut root = json!({});
        servers_mut(&mut root, Layout::WrappedOrFlat, Path::new(CWD), true)
            .unwrap()
            .unwrap()
            .insert("a".into(), json!({"command": "x"}));
        assert_eq!(root, json!({"mcpServers": {"a": {"command": "x"}}}));
    }

    #[test]
    fn wrapped_or_flat_keeps_existing_flat_shape() {
        let mut root = json!({"old": {"command": "y"}});
        servers_mut(&mut root, Layout::WrappedOrFlat, Path::new(CWD), true)
            .unwrap()
            .unwrap()
            .insert("a".into(), json!({}));
        assert!(root.get("a").is_some());
        assert!(root.get("mcpServers").is_none());
    }

    #[test]
    fn project_entry_matches_tolerantly() {
        let root = json!({"projects": {"/work/proj/": {"mcpServers": {"a": {}}}, "/other": {"mcpServers": {"b": {}}}}});
        let m = servers(&root, Layout::ProjectEntry, Path::new(CWD))
            .unwrap()
            .unwrap();
        assert!(m.contains_key("a"));
        assert!(!m.contains_key("b"));
    }

    #[test]
    fn project_entry_add_requires_existing_entry() {
        let mut root = json!({"projects": {"/other": {}}});
        let err = servers_mut(&mut root, Layout::ProjectEntry, Path::new(CWD), true).unwrap_err();
        assert!(err.contains("no Claude Code project entry"));
        let mut root = json!({"projects": {"/work/proj": {"allowedTools": []}}});
        servers_mut(&mut root, Layout::ProjectEntry, Path::new(CWD), true)
            .unwrap()
            .unwrap()
            .insert("a".into(), json!({}));
        assert_eq!(root["projects"]["/work/proj"]["allowedTools"], json!([]));
        assert!(root["projects"]["/work/proj"]["mcpServers"]["a"].is_object());
    }
}
