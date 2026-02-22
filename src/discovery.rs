use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::types::*;

/// Scan all known MCP config locations and return discovered servers
/// For CC-Global, also scans top-level mcpServers and deduplicates by name.

/// Scan all known MCP config locations and return discovered servers
pub fn discover(cwd: &Path) -> DiscoveryResult {
    let mut result = DiscoveryResult::default();

    scan_claude_code_global(&mut result);
    scan_mcp_json(cwd, &mut result);
    scan_wrapped(home(".cursor/mcp.json"), ClientKind::CursorGlobal, &mut result);
    scan_wrapped(cwd.join(".cursor/mcp.json"), ClientKind::CursorProject, &mut result);
    scan_vscode(cwd, &mut result);
    scan_wrapped(
        home(".codeium/windsurf/mcp_config.json"),
        ClientKind::Windsurf,
        &mut result,
    );
    scan_claude_desktop(&mut result);

    // Build active_clients: only clients that contributed at least one server
    let seen: HashSet<ClientKind> = result.servers.iter().map(|s| s.client.clone()).collect();
    result.active_clients = ClientKind::all()
        .iter()
        .filter(|c| seen.contains(c))
        .cloned()
        .collect();

    result
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn home(rel: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(rel)
}

fn read_json_with_errors(path: &Path, errors: &mut Vec<String>) -> Option<(Value, String)> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return None, // file absent — silent
    };
    let src = path.to_string_lossy().into_owned();
    match serde_json::from_str(&text) {
        Ok(val) => Some((val, src)),
        Err(e) => {
            errors.push(format!("{}: {}", src, e));
            None
        }
    }
}

fn parse_transport(obj: &Value) -> Transport {
    let ttype = obj.get("type").and_then(Value::as_str).unwrap_or("");

    match ttype {
        "http" => Transport::Http {
            url: obj["url"].as_str().unwrap_or("").to_string(),
            headers: parse_string_map(obj.get("headers")),
        },
        "sse" => Transport::Sse {
            url: obj["url"].as_str().unwrap_or("").to_string(),
        },
        _ if obj.get("command").is_some() || ttype == "stdio" => Transport::Stdio {
            command: obj["command"].as_str().unwrap_or("").to_string(),
            args: obj
                .get("args")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        },
        _ if obj.get("url").is_some() => {
            // Has URL but no explicit type — guess http
            Transport::Http {
                url: obj["url"].as_str().unwrap_or("").to_string(),
                headers: parse_string_map(obj.get("headers")),
            }
        }
        _ => Transport::Unknown,
    }
}

fn parse_string_map(v: Option<&Value>) -> Option<HashMap<String, String>> {
    v?.as_object().map(|m| {
        m.iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect()
    })
}

fn parse_server_map(
    map: &serde_json::Map<String, Value>,
    client: ClientKind,
    source: &str,
) -> Vec<McpServer> {
    map.iter()
        .filter(|(_, v)| v.is_object())
        .map(|(name, obj)| McpServer {
            name: name.clone(),
            client: client.clone(),
            source_path: source.to_string(),
            transport: parse_transport(obj),
            env: parse_string_map(obj.get("env")),
            health: HealthStatus::Unchecked,
            last_checked: None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Individual scanners
// ---------------------------------------------------------------------------

/// ~/.claude.json → top-level mcpServers + projects["<path>"].mcpServers (deduplicated)
fn scan_claude_code_global(result: &mut DiscoveryResult) {
    let path = home(".claude.json");
    let Some((root, src)) = read_json_with_errors(&path, &mut result.errors) else {
        return;
    };

    let mut seen: HashSet<String> = HashSet::new();

    // Top-level mcpServers (global servers)
    if let Some(mcp) = root["mcpServers"].as_object() {
        for server in parse_server_map(mcp, ClientKind::ClaudeCodeGlobal, &src) {
            seen.insert(server.name.clone());
            result.servers.push(server);
        }
    }

    // Per-project mcpServers (deduplicate by name)
    if let Some(projects) = root["projects"].as_object() {
        for (_project_path, project_val) in projects {
            if let Some(mcp) = project_val["mcpServers"].as_object() {
                for server in parse_server_map(mcp, ClientKind::ClaudeCodeGlobal, &src) {
                    if seen.insert(server.name.clone()) {
                        result.servers.push(server);
                    }
                }
            }
        }
    }
}

/// ./.mcp.json — supports both flat (top-level server keys) and wrapped (mcpServers key)
fn scan_mcp_json(cwd: &Path, result: &mut DiscoveryResult) {
    let path = cwd.join(".mcp.json");
    let Some((root, src)) = read_json_with_errors(&path, &mut result.errors) else {
        return;
    };

    // Try wrapped first
    if let Some(mcp) = root["mcpServers"].as_object() {
        result
            .servers
            .extend(parse_server_map(mcp, ClientKind::ClaudeCodeProject, &src));
    } else if let Some(obj) = root.as_object() {
        // Flat: every top-level key that has an object value is a server
        result
            .servers
            .extend(parse_server_map(obj, ClientKind::ClaudeCodeProject, &src));
    }
}

/// Generic scanner for configs that use { "mcpServers": { ... } }
fn scan_wrapped(path: PathBuf, client: ClientKind, result: &mut DiscoveryResult) {
    let Some((root, src)) = read_json_with_errors(&path, &mut result.errors) else {
        return;
    };

    if let Some(mcp) = root["mcpServers"].as_object() {
        result
            .servers
            .extend(parse_server_map(mcp, client, &src));
    }
}

/// VS Code uses "servers" key (not "mcpServers"), also check "mcpServers" as fallback
fn scan_vscode(cwd: &Path, result: &mut DiscoveryResult) {
    let path = cwd.join(".vscode/mcp.json");
    let Some((root, src)) = read_json_with_errors(&path, &mut result.errors) else {
        return;
    };

    let map = root["servers"]
        .as_object()
        .or_else(|| root["mcpServers"].as_object());

    if let Some(mcp) = map {
        result
            .servers
            .extend(parse_server_map(mcp, ClientKind::VsCodeProject, &src));
    }
}

/// Claude Desktop — try macOS path first, then Linux
fn scan_claude_desktop(result: &mut DiscoveryResult) {
    let candidates = [
        home("Library/Application Support/Claude/claude_desktop_config.json"),
        home(".config/Claude/claude_desktop_config.json"),
    ];
    for path in &candidates {
        if path.exists() {
            scan_wrapped(path.clone(), ClientKind::ClaudeDesktop, result);
            return;
        }
    }
}
