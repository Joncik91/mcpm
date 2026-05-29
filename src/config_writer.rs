use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Map, Value};

use std::path::PathBuf;

use crate::types::ClientKind;

/// Build a stdio server JSON value from wizard inputs
pub fn build_server_value(
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
) -> Value {
    let mut obj = Map::new();
    obj.insert("command".to_string(), Value::String(command.to_string()));
    if !args.is_empty() {
        obj.insert(
            "args".to_string(),
            Value::Array(args.iter().map(|a| Value::String(a.clone())).collect()),
        );
    }
    if !env.is_empty() {
        let env_obj: Map<String, Value> = env
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        obj.insert("env".to_string(), Value::Object(env_obj));
    }
    Value::Object(obj)
}

/// Build an HTTP server JSON value
pub fn build_http_server_value(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    env: &HashMap<String, String>,
) -> Value {
    let mut obj = Map::new();
    obj.insert("type".to_string(), Value::String("http".to_string()));
    obj.insert("url".to_string(), Value::String(url.to_string()));
    if let Some(h) = headers {
        if !h.is_empty() {
            let hdr_obj: Map<String, Value> = h
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            obj.insert("headers".to_string(), Value::Object(hdr_obj));
        }
    }
    if !env.is_empty() {
        let env_obj: Map<String, Value> = env
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        obj.insert("env".to_string(), Value::Object(env_obj));
    }
    Value::Object(obj)
}

/// Build an SSE server JSON value
pub fn build_sse_server_value(
    url: &str,
    env: &HashMap<String, String>,
) -> Value {
    let mut obj = Map::new();
    obj.insert("type".to_string(), Value::String("sse".to_string()));
    obj.insert("url".to_string(), Value::String(url.to_string()));
    if !env.is_empty() {
        let env_obj: Map<String, Value> = env
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        obj.insert("env".to_string(), Value::Object(env_obj));
    }
    Value::Object(obj)
}

/// Returns true if `name` already exists in the exact scope `add_server` would
/// write to for this client — so callers can warn about a *real* clobber.
///
/// This deliberately mirrors `add_server`'s per-client scope. In particular,
/// CC-Global is checked against top-level `mcpServers` only: a server that lives
/// solely in a `projects[<path>].mcpServers` entry is surfaced by discovery as
/// CC-Global but would be a fresh top-level insert, so it must NOT be flagged as
/// an overwrite. Any read/parse failure is treated as "absent" (no false alarm).
pub fn server_exists_in_scope(client: &ClientKind, cwd: &Path, name: &str) -> bool {
    let Some(path) = client.config_path(cwd) else {
        return false;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(root) = serde_json::from_str::<Value>(&text) else {
        return false;
    };

    if *client == ClientKind::ClaudeCodeProject {
        // Wrapped takes precedence over flat, matching add_server.
        if let Some(map) = root.get("mcpServers").and_then(Value::as_object) {
            map.contains_key(name)
        } else {
            root.as_object().is_some_and(|m| m.contains_key(name))
        }
    } else {
        // CC-Global and all other clients write under their servers key
        // (top-level `mcpServers` for CC-Global — project scopes are not touched).
        root.get(client.servers_key())
            .and_then(Value::as_object)
            .is_some_and(|m| m.contains_key(name))
    }
}

/// Add a server to a client's config file.
/// Returns `Ok(true)` if an existing server of the same name was overwritten,
/// `Ok(false)` if it was a fresh insert.
pub fn add_server(
    client: &ClientKind,
    cwd: &Path,
    name: &str,
    server_value: &Value,
) -> Result<bool, String> {
    let path = client
        .config_path(cwd)
        .ok_or("could not determine config path")?;

    // Create parent dirs
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directory {}: {}", parent.display(), e))?;
    }

    // Read existing or start fresh
    let mut root = read_or_empty(&path)?;

    // Backup if file exists
    backup(&path)?;

    // Insert server at the right location, tracking whether we clobbered an entry
    let key = client.servers_key();
    let overwrote;

    if *client == ClientKind::ClaudeCodeGlobal {
        // Add to top-level mcpServers in ~/.claude.json
        if root.get(key).is_none() {
            root[key] = json!({});
        }
        overwrote = root[key].get(name).is_some();
        root[key][name] = server_value.clone();
    } else if *client == ClientKind::ClaudeCodeProject {
        if root.get("mcpServers").is_some() {
            // Wrapped format — insert under mcpServers
            overwrote = root["mcpServers"].get(name).is_some();
            root["mcpServers"][name] = server_value.clone();
        } else {
            // Flat format — insert at root
            overwrote = root.get(name).is_some();
            root[name] = server_value.clone();
        }
    } else {
        // All other clients: insert under their servers key
        if root.get(key).is_none() {
            root[key] = json!({});
        }
        overwrote = root[key].get(name).is_some();
        root[key][name] = server_value.clone();
    }

    write_atomic(&path, &root)?;
    Ok(overwrote)
}

/// Remove a server from a client's config file.
/// For ClaudeCodePlugin, pass the source_path as `plugin_source`.
/// Returns `Ok(true)` if a matching server was actually removed, `Ok(false)`
/// if no entry with that name existed in this client's scope (no write made).
pub fn remove_server(
    client: &ClientKind,
    cwd: &Path,
    name: &str,
) -> Result<bool, String> {
    remove_server_inner(client, cwd, name, None)
}

/// Remove a plugin server using its source path
pub fn remove_plugin_server(
    cwd: &Path,
    name: &str,
    source_path: &str,
) -> Result<bool, String> {
    remove_server_inner(&ClientKind::ClaudeCodePlugin, cwd, name, Some(source_path))
}

fn remove_server_inner(
    client: &ClientKind,
    cwd: &Path,
    name: &str,
    plugin_source: Option<&str>,
) -> Result<bool, String> {
    let path = if *client == ClientKind::ClaudeCodePlugin {
        PathBuf::from(plugin_source.ok_or("plugin source path required")?)
    } else {
        client.config_path(cwd).ok_or("could not determine config path")?
    };

    let mut root = read_or_empty(&path)?;

    let key = client.servers_key();

    // CC-Global is intentionally scoped to top-level mcpServers only — project
    // scopes are left untouched (decision D002). `removed` reflects reality so
    // callers can report honestly instead of claiming a phantom success.
    let removed = if *client == ClientKind::ClaudeCodeGlobal {
        root.get_mut("mcpServers")
            .and_then(Value::as_object_mut)
            .map(|obj| obj.remove(name).is_some())
            .unwrap_or(false)
    } else if *client == ClientKind::ClaudeCodePlugin {
        // Flat format — remove server key from root
        root.as_object_mut()
            .map(|obj| obj.remove(name).is_some())
            .unwrap_or(false)
    } else if *client == ClientKind::ClaudeCodeProject {
        // Check both wrapped and flat
        if let Some(obj) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
            obj.remove(name).is_some()
        } else if let Some(obj) = root.as_object_mut() {
            obj.remove(name).is_some()
        } else {
            false
        }
    } else {
        root.get_mut(key)
            .and_then(Value::as_object_mut)
            .map(|obj| obj.remove(name).is_some())
            .unwrap_or(false)
    };

    // Only touch the file (and create a backup) when we actually changed something.
    if removed {
        backup(&path)?;
        write_atomic(&path, &root)?;
    }
    Ok(removed)
}

/// Restore the most recent backup for a client's config file.
/// Swaps current ↔ backup so the undo is itself undoable.
pub fn restore_backup(client: &ClientKind, cwd: &Path) -> Result<(), String> {
    let path = client
        .config_path(cwd)
        .ok_or("could not determine config path")?;
    let bak = path_with_suffix(&path, ".bak");
    if !bak.exists() {
        return Err("no backup file found".to_string());
    }
    // Swap current and backup
    let tmp = path_with_suffix(&path, ".undo_tmp");
    if path.exists() {
        std::fs::copy(&path, &tmp)
            .map_err(|e| format!("failed to save current: {}", e))?;
    }
    std::fs::rename(&bak, &path)
        .map_err(|e| format!("failed to restore backup: {}", e))?;
    if tmp.exists() {
        std::fs::rename(&tmp, &bak)
            .map_err(|e| format!("failed to rotate backup: {}", e))?;
    }
    Ok(())
}

fn read_or_empty(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            serde_json::from_str(&text).map_err(|e| format!("invalid JSON in {}: {}", path.display(), e))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("failed to read {}: {}", path.display(), e)),
    }
}

fn backup(path: &Path) -> Result<(), String> {
    if path.exists() {
        let bak = path_with_suffix(path, ".bak");
        std::fs::copy(path, &bak)
            .map_err(|e| format!("failed to create backup {}: {}", bak.display(), e))?;
    }
    Ok(())
}

fn write_atomic(path: &Path, value: &Value) -> Result<(), String> {
    let json_str = serde_json::to_string_pretty(value)
        .map_err(|e| format!("failed to serialize JSON: {}", e))?;

    let tmp = path_with_suffix(path, ".tmp");
    std::fs::write(&tmp, json_str.as_bytes())
        .map_err(|e| format!("failed to write {}: {}", tmp.display(), e))?;

    std::fs::rename(&tmp, path)
        .map_err(|e| format!("failed to rename {} to {}: {}", tmp.display(), path.display(), e))
}

/// Append a suffix to a path's filename (e.g. "foo.json" + ".bak" → "foo.json.bak").
/// Unlike `with_extension()`, this preserves the original extension.
fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Unique temp dir used as a fake `cwd`; ClaudeCodeProject derives its
    /// config path from cwd (`<cwd>/.mcp.json`), so no home dir is touched.
    fn temp_cwd() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("mcpm_test_{}_{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn stdio_val() -> Value {
        build_server_value("node", &["server.js".to_string()], &HashMap::new())
    }

    #[test]
    fn add_reports_fresh_then_overwrite() {
        let cwd = temp_cwd();
        let client = ClientKind::ClaudeCodeProject;

        // First insert is fresh.
        assert_eq!(add_server(&client, &cwd, "foo", &stdio_val()), Ok(false));
        // Re-adding the same name clobbers — must report the overwrite.
        assert_eq!(add_server(&client, &cwd, "foo", &stdio_val()), Ok(true));
    }

    #[test]
    fn scope_check_matches_what_add_writes() {
        let cwd = temp_cwd();
        let client = ClientKind::ClaudeCodeProject;

        // Absent before any write.
        assert!(!server_exists_in_scope(&client, &cwd, "foo"));

        // Present in the exact scope add_server wrote to.
        add_server(&client, &cwd, "foo", &stdio_val()).unwrap();
        assert!(server_exists_in_scope(&client, &cwd, "foo"));
        // A different name is still absent (no false positive).
        assert!(!server_exists_in_scope(&client, &cwd, "bar"));

        // After removal it reads absent again.
        remove_server(&client, &cwd, "foo").unwrap();
        assert!(!server_exists_in_scope(&client, &cwd, "foo"));
    }

    #[test]
    fn remove_missing_is_honest_noop() {
        let cwd = temp_cwd();
        let client = ClientKind::ClaudeCodeProject;
        add_server(&client, &cwd, "foo", &stdio_val()).unwrap();

        let path = cwd.join(".mcp.json");
        let before = std::fs::read_to_string(&path).unwrap();

        // Removing a name that isn't there must report false and NOT rewrite.
        assert_eq!(remove_server(&client, &cwd, "does-not-exist"), Ok(false));
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(before, after, "no-op remove must not modify the file");

        // Removing the real entry reports true; removing it again reports false.
        assert_eq!(remove_server(&client, &cwd, "foo"), Ok(true));
        assert_eq!(remove_server(&client, &cwd, "foo"), Ok(false));
    }
}
