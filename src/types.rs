use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::scope::Layout;

/// Where mcpm is looking: the user's home directory and the working directory.
/// Passed explicitly so discovery and writes are testable against fixture dirs.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub home: Option<PathBuf>,
    pub cwd: PathBuf,
}

impl Ctx {
    /// Build from the real process environment.
    pub fn from_env() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Ctx {
            home: dirs::home_dir(),
            cwd,
        }
    }
}

/// Which client configuration file a server was found in
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ClientKind {
    /// `~/.claude.json` top-level `mcpServers` (Claude Code "user" scope)
    ClaudeCodeGlobal,
    /// `~/.claude.json` `projects[<cwd>].mcpServers` (Claude Code "local" scope)
    ClaudeCodeLocal,
    /// `<cwd>/.mcp.json` (Claude Code "project" scope)
    ClaudeCodeProject,
    ClaudeCodePlugin,
    CursorGlobal,
    CursorProject,
    VsCodeProject,
    Windsurf,
    ClaudeDesktop,
}

impl ClientKind {
    /// Short display label for the client matrix columns
    pub fn label(&self) -> &'static str {
        match self {
            ClientKind::ClaudeCodeGlobal => "CC-Global",
            ClientKind::ClaudeCodeLocal => "CC-Local",
            ClientKind::ClaudeCodeProject => "CC-Project",
            ClientKind::ClaudeCodePlugin => "CC-Plugin",
            ClientKind::CursorGlobal => "Cursor",
            ClientKind::CursorProject => "Cur-Proj",
            ClientKind::VsCodeProject => "VSCode",
            ClientKind::Windsurf => "Windsurf",
            ClientKind::ClaudeDesktop => "Desktop",
        }
    }

    /// All variants in display order
    pub fn all() -> &'static [ClientKind] {
        &[
            ClientKind::ClaudeCodeGlobal,
            ClientKind::ClaudeCodeLocal,
            ClientKind::ClaudeCodeProject,
            ClientKind::ClaudeCodePlugin,
            ClientKind::CursorGlobal,
            ClientKind::CursorProject,
            ClientKind::VsCodeProject,
            ClientKind::Windsurf,
            ClientKind::ClaudeDesktop,
        ]
    }

    /// Clients mcpm can add servers to (plugins are managed by Claude Code itself).
    pub fn writable() -> &'static [ClientKind] {
        &[
            ClientKind::ClaudeCodeGlobal,
            ClientKind::ClaudeCodeLocal,
            ClientKind::ClaudeCodeProject,
            ClientKind::CursorGlobal,
            ClientKind::CursorProject,
            ClientKind::VsCodeProject,
            ClientKind::Windsurf,
            ClientKind::ClaudeDesktop,
        ]
    }

    /// True for clients whose config lives in the working directory.
    pub fn is_project_scoped(&self) -> bool {
        matches!(
            self,
            ClientKind::ClaudeCodeProject | ClientKind::CursorProject | ClientKind::VsCodeProject
        )
    }

    /// Filesystem path to this client's config file.
    /// `None` for plugins (each server carries its own path) or when the home
    /// directory is unknown for a home-based client.
    pub fn config_path(&self, ctx: &Ctx) -> Option<PathBuf> {
        let home = ctx.home.as_deref();
        match self {
            ClientKind::ClaudeCodeGlobal | ClientKind::ClaudeCodeLocal => {
                Some(home?.join(".claude.json"))
            }
            ClientKind::ClaudeCodeProject => Some(ctx.cwd.join(".mcp.json")),
            ClientKind::ClaudeCodePlugin => None,
            ClientKind::CursorGlobal => Some(home?.join(".cursor").join("mcp.json")),
            ClientKind::CursorProject => Some(ctx.cwd.join(".cursor").join("mcp.json")),
            ClientKind::VsCodeProject => Some(ctx.cwd.join(".vscode").join("mcp.json")),
            ClientKind::Windsurf => Some(
                home?
                    .join(".codeium")
                    .join("windsurf")
                    .join("mcp_config.json"),
            ),
            ClientKind::ClaudeDesktop => claude_desktop_config_path(home),
        }
    }

    /// Where servers live inside this client's config file.
    pub fn layout(&self) -> Layout {
        match self {
            ClientKind::ClaudeCodeLocal => Layout::ProjectEntry,
            ClientKind::ClaudeCodeProject | ClientKind::ClaudeCodePlugin => Layout::WrappedOrFlat,
            // VS Code's documented key is `servers`; some files use `mcpServers`.
            ClientKind::VsCodeProject => Layout::Keyed(&["servers", "mcpServers"]),
            _ => Layout::Keyed(&["mcpServers"]),
        }
    }
}

/// Claude Desktop's config path — the single source of truth for both reading
/// and writing. Returns the first candidate that exists, else the platform
/// default (where a new config would be created).
fn claude_desktop_config_path(home: Option<&Path>) -> Option<PathBuf> {
    const FILE: &str = "claude_desktop_config.json";
    let mut candidates: Vec<PathBuf> = Vec::new();

    if cfg!(windows) {
        // An MSIX install reads its virtualized copy, so prefer it when present.
        if let Some(p) = find_msix_claude_config() {
            candidates.push(p);
        }
        if let Some(data) = dirs::data_dir() {
            candidates.push(data.join("Claude").join(FILE));
        }
    } else if cfg!(target_os = "macos") {
        if let Some(h) = home {
            candidates.push(
                h.join("Library")
                    .join("Application Support")
                    .join("Claude")
                    .join(FILE),
            );
        }
    } else if let Some(h) = home {
        candidates.push(h.join(".config").join("Claude").join(FILE));
    }

    candidates
        .iter()
        .find(|p| p.is_file())
        .or_else(|| candidates.last())
        .cloned()
}

/// Search for Claude Desktop config inside the MSIX virtualized filesystem.
/// MSIX installs redirect %APPDATA% to %LOCALAPPDATA%\Packages\Claude_<hash>\LocalCache\Roaming\
fn find_msix_claude_config() -> Option<PathBuf> {
    let packages = dirs::data_local_dir()?.join("Packages");
    std::fs::read_dir(packages)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("Claude_"))
        .map(|e| {
            e.path()
                .join("LocalCache")
                .join("Roaming")
                .join("Claude")
                .join("claude_desktop_config.json")
        })
        .find(|p| p.is_file())
}

/// Transport type of an MCP server
#[derive(Debug, Clone, PartialEq)]
pub enum Transport {
    Http {
        url: String,
        headers: BTreeMap<String, String>,
    },
    Sse {
        url: String,
        headers: BTreeMap<String, String>,
    },
    Stdio {
        command: String,
        args: Vec<String>,
    },
    Unknown,
}

impl Transport {
    pub fn kind_label(&self) -> &'static str {
        match self {
            Transport::Http { .. } => "http",
            Transport::Sse { .. } => "sse",
            Transport::Stdio { .. } => "stdio",
            Transport::Unknown => "unknown",
        }
    }

    pub fn is_stdio(&self) -> bool {
        matches!(self, Transport::Stdio { .. })
    }
}

/// Stable identity of a discovered server — survives rescans, unlike a list index.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerId {
    pub client: ClientKind,
    pub source: PathBuf,
    pub name: String,
}

/// A single MCP server entry as found in a config file
#[derive(Debug, Clone)]
pub struct McpServer {
    pub name: String,
    pub client: ClientKind,
    pub source_path: PathBuf,
    pub transport: Transport,
    pub env: BTreeMap<String, String>,
    pub disabled: bool,
    /// The entry exactly as it appears on disk; sync copies this verbatim so
    /// fields mcpm doesn't model (cwd, timeout, autoApprove, ...) survive.
    pub raw: Value,
}

impl McpServer {
    pub fn id(&self) -> ServerId {
        ServerId {
            client: self.client,
            source: self.source_path.clone(),
            name: self.name.clone(),
        }
    }

    /// Short origin label: the client, plus the plugin name for plugin servers.
    pub fn origin_label(&self) -> String {
        if self.client == ClientKind::ClaudeCodePlugin {
            let plugin = self
                .source_path
                .parent()
                .and_then(Path::file_name)
                .map(|n| n.to_string_lossy().into_owned());
            if let Some(p) = plugin {
                return format!("{} ({})", self.client.label(), p);
            }
        }
        self.client.label().to_string()
    }

    /// Top-level keys of the raw entry that mcpm doesn't display elsewhere.
    pub fn extra_fields(&self) -> Vec<String> {
        const KNOWN: &[&str] = &[
            "type",
            "command",
            "args",
            "env",
            "url",
            "serverUrl",
            "headers",
            "disabled",
        ];
        self.raw
            .as_object()
            .map(|m| {
                m.keys()
                    .filter(|k| !KNOWN.contains(&k.as_str()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Health check status for a server (absence from the health map = unchecked)
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Checking,
    Healthy {
        server_name: String,
        server_version: String,
        protocol_version: Option<String>,
    },
    Timeout(Duration),
    Error(String),
}

impl HealthStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            HealthStatus::Checking => "⟳",
            HealthStatus::Healthy { .. } => "●",
            HealthStatus::Timeout(_) => "⚠",
            HealthStatus::Error(_) => "✗",
        }
    }

    pub fn label(&self) -> String {
        match self {
            HealthStatus::Checking => "checking...".to_string(),
            HealthStatus::Healthy {
                server_name,
                server_version,
                ..
            } => format!("healthy ({} v{})", server_name, server_version),
            HealthStatus::Timeout(d) => format!("timeout ({}s)", d.as_secs_f32()),
            HealthStatus::Error(e) => format!("error: {}", e),
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, HealthStatus::Healthy { .. })
    }
}

/// Result from a background health check thread
pub struct HealthResult {
    pub id: ServerId,
    pub status: HealthStatus,
    pub checked_at: Instant,
}

/// All discovered data, ready for the UI
#[derive(Debug, Default)]
pub struct DiscoveryResult {
    pub servers: Vec<McpServer>,
    /// Clients that actually had servers (for matrix columns)
    pub active_clients: Vec<ClientKind>,
    /// Non-fatal read/parse problems
    pub errors: Vec<String>,
}
