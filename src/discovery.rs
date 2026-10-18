use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::jsonc;
use crate::scope;
use crate::types::*;

/// Scan all known MCP config locations and return discovered servers.
pub fn discover(ctx: &Ctx) -> DiscoveryResult {
    let mut result = DiscoveryResult::default();
    let mut cache: HashMap<PathBuf, Option<Value>> = HashMap::new();
    let mut scanned: Vec<(PathBuf, ClientKind)> = Vec::new();

    for &client in ClientKind::all() {
        if client == ClientKind::ClaudeCodePlugin {
            scan_claude_code_plugins(ctx, &mut result);
            continue;
        }
        let Some(path) = client.config_path(ctx) else {
            continue;
        };
        // Running from $HOME makes e.g. <cwd>/.cursor/mcp.json the same file as
        // ~/.cursor/mcp.json — list it once, under the global client.
        if client.is_project_scoped() && scanned.iter().any(|(p, _)| same_file(p, &path)) {
            continue;
        }
        scanned.push((path.clone(), client));

        let root = cache
            .entry(path.clone())
            .or_insert_with(|| read_config(&path, &mut result.errors));
        if let Some(root) = root {
            collect(root, client, &path, ctx, &mut result);
        }
    }

    let seen: HashSet<ClientKind> = result.servers.iter().map(|s| s.client).collect();
    result.active_clients = ClientKind::all()
        .iter()
        .filter(|c| seen.contains(c))
        .copied()
        .collect();

    result
}

/// True when two paths name the same file (by canonical path when both exist).
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read and parse a config file. Absent files are silent; anything else that
/// prevents reading is reported.
fn read_config(path: &Path, errors: &mut Vec<String>) -> Option<Value> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            errors.push(format!("{}: {}", path.display(), e));
            return None;
        }
    };
    if text.trim().is_empty() {
        return None;
    }
    match jsonc::parse(&text) {
        Ok(p) => Some(p.value),
        Err(e) => {
            errors.push(format!("{}: {}", path.display(), e));
            None
        }
    }
}

fn collect(root: &Value, client: ClientKind, path: &Path, ctx: &Ctx, result: &mut DiscoveryResult) {
    match scope::servers(root, client.layout(), &ctx.cwd) {
        Ok(Some(map)) => {
            // Flat files hold servers at the root, next to unrelated settings.
            let flat = root.as_object().is_some_and(|r| std::ptr::eq(r, map));
            parse_server_map(map, client, path, flat, result)
        }
        Ok(None) => {}
        Err(e) => result
            .errors
            .push(format!("{} ({}): {}", path.display(), client.label(), e)),
    }
}

fn parse_server_map(
    map: &Map<String, Value>,
    client: ClientKind,
    source: &Path,
    flat: bool,
    result: &mut DiscoveryResult,
) {
    for (name, obj) in map {
        if !obj.is_object() {
            if !flat {
                result.errors.push(format!(
                    "{}: server \"{}\" is not an object — skipped",
                    source.display(),
                    name
                ));
            }
            continue;
        }
        result.servers.push(McpServer {
            name: name.clone(),
            client,
            source_path: source.to_path_buf(),
            transport: parse_transport(obj),
            env: string_map(obj.get("env")),
            disabled: obj
                .get("disabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            raw: obj.clone(),
        });
    }
}

pub(crate) fn parse_transport(obj: &Value) -> Transport {
    let ttype = obj.get("type").and_then(Value::as_str).unwrap_or("");
    let url = obj
        .get("url")
        .or_else(|| obj.get("serverUrl")) // Windsurf's remote-server key
        .and_then(Value::as_str)
        .map(str::to_string);
    let headers = || string_map(obj.get("headers"));

    match ttype {
        "http" | "streamable-http" | "streamableHttp" => Transport::Http {
            url: url.unwrap_or_default(),
            headers: headers(),
        },
        "sse" => Transport::Sse {
            url: url.unwrap_or_default(),
            headers: headers(),
        },
        _ if obj.get("command").is_some() || ttype == "stdio" => Transport::Stdio {
            command: obj
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            args: obj
                .get("args")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(scalar_to_string).collect())
                .unwrap_or_default(),
        },
        // Has a URL but no explicit type — clients default to HTTP.
        _ if url.is_some() => Transport::Http {
            url: url.unwrap_or_default(),
            headers: headers(),
        },
        _ => Transport::Unknown,
    }
}

/// Map of string-ish values; numbers/bools are stringified rather than dropped.
fn string_map(v: Option<&Value>) -> BTreeMap<String, String> {
    v.and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), scalar_to_string(v)))
                .collect()
        })
        .unwrap_or_default()
}

fn scalar_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Plugins
// ---------------------------------------------------------------------------

/// ~/.claude/plugins/marketplaces/*/external_plugins/*/.mcp.json
/// Only scans plugins that are actually installed (have a matching plugins/ entry).
fn scan_claude_code_plugins(ctx: &Ctx, result: &mut DiscoveryResult) {
    let Some(home) = &ctx.home else {
        return;
    };
    let Ok(marketplaces) = std::fs::read_dir(home.join(".claude/plugins/marketplaces")) else {
        return;
    };
    let mut marketplaces: Vec<PathBuf> = marketplaces.flatten().map(|e| e.path()).collect();
    marketplaces.sort();
    for marketplace in marketplaces {
        let installed_dir = marketplace.join("plugins");
        let Ok(plugins) = std::fs::read_dir(marketplace.join("external_plugins")) else {
            continue;
        };
        let mut plugins: Vec<PathBuf> = plugins.flatten().map(|e| e.path()).collect();
        plugins.sort();
        for plugin in plugins {
            let Some(name) = plugin.file_name() else {
                continue;
            };
            if !installed_dir.join(name).is_dir() {
                continue;
            }
            let mcp_json = plugin.join(".mcp.json");
            if let Some(root) = read_config(&mcp_json, &mut result.errors) {
                collect(&root, ClientKind::ClaudeCodePlugin, &mcp_json, ctx, result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;
    use serde_json::json;

    fn write(path: &Path, v: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, v).unwrap();
    }

    fn names(r: &DiscoveryResult, c: ClientKind) -> Vec<String> {
        r.servers
            .iter()
            .filter(|s| s.client == c)
            .map(|s| s.name.clone())
            .collect()
    }

    #[test]
    fn discovers_all_scopes_with_correct_attribution() {
        let t = TempDir::new();
        let ctx = t.ctx();
        let cwd = ctx.cwd.to_string_lossy().into_owned();
        write(
            &t.home().join(".claude.json"),
            &json!({
                "mcpServers": {"g": {"command": "g"}},
                "projects": {
                    cwd.clone(): {"mcpServers": {"local": {"command": "l"}}},
                    "/elsewhere": {"mcpServers": {"foreign": {"command": "f"}}}
                }
            })
            .to_string(),
        );
        write(
            &ctx.cwd.join(".mcp.json"),
            r#"{"mcpServers":{"p":{"type":"http","url":"http://x"}}}"#,
        );
        write(
            &ctx.cwd.join(".vscode/mcp.json"),
            "{ // jsonc\n \"servers\": {\"v\": {\"type\": \"stdio\", \"command\": \"v\"},},}",
        );
        write(
            &t.home().join(".codeium/windsurf/mcp_config.json"),
            r#"{"mcpServers":{"w":{"serverUrl":"https://w"}}}"#,
        );

        let r = discover(&ctx);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(names(&r, ClientKind::ClaudeCodeGlobal), vec!["g"]);
        assert_eq!(names(&r, ClientKind::ClaudeCodeLocal), vec!["local"]);
        assert_eq!(names(&r, ClientKind::ClaudeCodeProject), vec!["p"]);
        assert_eq!(names(&r, ClientKind::VsCodeProject), vec!["v"]);
        let w = r.servers.iter().find(|s| s.name == "w").unwrap();
        assert_eq!(
            w.transport,
            Transport::Http {
                url: "https://w".into(),
                headers: BTreeMap::new()
            }
        );
        assert!(
            !r.servers.iter().any(|s| s.name == "foreign"),
            "other projects' servers must not leak in"
        );
    }

    #[test]
    fn reports_malformed_entries() {
        let t = TempDir::new();
        let ctx = t.ctx();
        write(
            &t.home().join(".cursor/mcp.json"),
            r#"{"mcpServers": {"bad": 3, "ok": {"command": "x"}}}"#,
        );
        write(&ctx.cwd.join(".cursor/mcp.json"), r#"{"mcpServers": ["#);
        let r = discover(&ctx);
        assert_eq!(names(&r, ClientKind::CursorGlobal), vec!["ok"]);
        assert_eq!(r.errors.len(), 2, "{:?}", r.errors);
    }

    #[test]
    fn preserves_file_order_and_unmodelled_fields() {
        let t = TempDir::new();
        let ctx = t.ctx();
        write(
            &ctx.cwd.join(".mcp.json"),
            r#"{"mcpServers":{"zeta":{"command":"z","cwd":"/srv","env":{"N":1}},"alpha":{"command":"a"}}}"#,
        );
        let r = discover(&ctx);
        assert_eq!(
            names(&r, ClientKind::ClaudeCodeProject),
            vec!["zeta", "alpha"]
        );
        assert_eq!(r.servers[0].raw["cwd"], "/srv");
        assert_eq!(r.servers[0].env["N"], "1");
        assert_eq!(r.servers[0].extra_fields(), vec!["cwd"]);
    }

    #[test]
    fn running_from_home_lists_shared_file_once() {
        let t = TempDir::new();
        let ctx = Ctx {
            home: Some(t.home()),
            cwd: t.home(),
        };
        write(
            &t.home().join(".cursor/mcp.json"),
            r#"{"mcpServers":{"c":{"command":"c"}}}"#,
        );
        let r = discover(&ctx);
        assert_eq!(r.servers.len(), 1);
        assert_eq!(r.servers[0].client, ClientKind::CursorGlobal);
    }

    #[test]
    fn plugin_discovery_requires_install_and_accepts_wrapped() {
        let t = TempDir::new();
        let ctx = t.ctx();
        let mp = t.home().join(".claude/plugins/marketplaces/m");
        write(
            &mp.join("external_plugins/a/.mcp.json"),
            r#"{"mcpServers":{"pa":{"command":"a"}}}"#,
        );
        write(
            &mp.join("external_plugins/b/.mcp.json"),
            r#"{"pb":{"command":"b"}}"#,
        );
        std::fs::create_dir_all(mp.join("plugins/a")).unwrap();
        let r = discover(&ctx);
        assert_eq!(names(&r, ClientKind::ClaudeCodePlugin), vec!["pa"]);
        assert_eq!(r.servers[0].origin_label(), "CC-Plugin (a)");
    }
}
