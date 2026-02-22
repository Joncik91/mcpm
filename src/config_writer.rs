use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Map, Value};

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

/// Add a server to a client's config file
pub fn add_server(
    client: &ClientKind,
    cwd: &Path,
    name: &str,
    server_value: &Value,
) -> Result<(), String> {
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

    // Insert server at the right location
    let key = client.servers_key();

    if *client == ClientKind::ClaudeCodeGlobal {
        // Add to top-level mcpServers in ~/.claude.json
        if root.get(key).is_none() {
            root[key] = json!({});
        }
        root[key][name] = server_value.clone();
    } else if *client == ClientKind::ClaudeCodeProject {
        if root.get("mcpServers").is_some() {
            // Wrapped format — insert under mcpServers
            root["mcpServers"][name] = server_value.clone();
        } else {
            // Flat format — insert at root
            root[name] = server_value.clone();
        }
    } else {
        // All other clients: insert under their servers key
        if root.get(key).is_none() {
            root[key] = json!({});
        }
        root[key][name] = server_value.clone();
    }

    write_atomic(&path, &root)
}

/// Remove a server from a client's config file
pub fn remove_server(
    client: &ClientKind,
    cwd: &Path,
    name: &str,
) -> Result<(), String> {
    let path = client
        .config_path(cwd)
        .ok_or("could not determine config path")?;

    let mut root = read_or_empty(&path)?;

    backup(&path)?;

    let key = client.servers_key();

    if *client == ClientKind::ClaudeCodeGlobal {
        // Remove from top-level mcpServers
        if let Some(obj) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
            obj.remove(name);
        }
        // Remove from all project entries
        if let Some(projects) = root.get_mut("projects").and_then(Value::as_object_mut) {
            for (_project_path, project_val) in projects.iter_mut() {
                if let Some(mcp) = project_val.get_mut("mcpServers").and_then(Value::as_object_mut) {
                    mcp.remove(name);
                }
            }
        }
    } else if *client == ClientKind::ClaudeCodeProject {
        // Check both wrapped and flat
        if let Some(obj) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
            obj.remove(name);
        } else if let Some(obj) = root.as_object_mut() {
            obj.remove(name);
        }
    } else {
        if let Some(obj) = root.get_mut(key).and_then(Value::as_object_mut) {
            obj.remove(name);
        }
    }

    write_atomic(&path, &root)
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
        let bak = path.with_extension("bak");
        std::fs::copy(path, &bak)
            .map_err(|e| format!("failed to create backup {}: {}", bak.display(), e))?;
    }
    Ok(())
}

fn write_atomic(path: &Path, value: &Value) -> Result<(), String> {
    let json_str = serde_json::to_string_pretty(value)
        .map_err(|e| format!("failed to serialize JSON: {}", e))?;

    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, json_str.as_bytes())
        .map_err(|e| format!("failed to write {}: {}", tmp.display(), e))?;

    std::fs::rename(&tmp, path)
        .map_err(|e| format!("failed to rename {} to {}: {}", tmp.display(), path.display(), e))
}
