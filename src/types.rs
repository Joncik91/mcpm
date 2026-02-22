use std::collections::HashMap;
use std::time::Instant;

/// Which client configuration file a server was found in
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClientKind {
    ClaudeCodeGlobal,
    ClaudeCodeProject,
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
            ClientKind::ClaudeCodeProject => "CC-Project",
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
            ClientKind::ClaudeCodeProject,
            ClientKind::CursorGlobal,
            ClientKind::CursorProject,
            ClientKind::VsCodeProject,
            ClientKind::Windsurf,
            ClientKind::ClaudeDesktop,
        ]
    }
}

/// Transport type of an MCP server
#[derive(Debug, Clone)]
pub enum Transport {
    Http {
        url: String,
        headers: Option<HashMap<String, String>>,
    },
    Sse {
        url: String,
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

/// Health check status for a server
#[derive(Debug, Clone)]
pub enum HealthStatus {
    Unchecked,
    Checking,
    Healthy {
        server_name: String,
        server_version: String,
    },
    Timeout,
    Error(String),
}

impl HealthStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            HealthStatus::Unchecked => "",
            HealthStatus::Checking => "⟳",
            HealthStatus::Healthy { .. } => "●",
            HealthStatus::Timeout => "⚠",
            HealthStatus::Error(_) => "✗",
        }
    }

    pub fn label(&self) -> String {
        match self {
            HealthStatus::Unchecked => "unchecked".to_string(),
            HealthStatus::Checking => "checking...".to_string(),
            HealthStatus::Healthy {
                server_name,
                server_version,
            } => format!("healthy ({} v{})", server_name, server_version),
            HealthStatus::Timeout => "timeout (5s)".to_string(),
            HealthStatus::Error(e) => format!("error: {}", e),
        }
    }
}

/// Result from a background health check thread
pub struct HealthResult {
    pub server_index: usize,
    pub status: HealthStatus,
    pub checked_at: Instant,
}

/// A single MCP server entry as found in a config file
#[derive(Debug, Clone)]
pub struct McpServer {
    pub name: String,
    pub client: ClientKind,
    pub source_path: String,
    pub transport: Transport,
    pub env: Option<HashMap<String, String>>,
    pub health: HealthStatus,
    pub last_checked: Option<Instant>,
}

/// All discovered data, ready for the UI
#[derive(Debug, Default)]
pub struct DiscoveryResult {
    pub servers: Vec<McpServer>,
    /// Clients that actually had servers (for matrix columns)
    pub active_clients: Vec<ClientKind>,
    /// Non-fatal parse errors
    pub errors: Vec<String>,
}
