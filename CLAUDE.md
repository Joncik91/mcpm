# mcpm

## Overview
MCP Server Manager — a Rust TUI/CLI for discovering, viewing, and managing MCP server configurations across 7 client applications (Claude Code, Cursor, VS Code, Windsurf, Claude Desktop).

## Tech Stack
- Rust (edition 2024)
- ratatui 0.29 + crossterm 0.28 (terminal UI)
- clap 4 (CLI parsing with derive)
- serde_json 1 (manual JSON parsing — no derive macros)
- dirs 6 (cross-platform home directory)
- anyhow 1 (error handling)

## Development Commands
```bash
cargo build                        # Debug build
cargo build --release              # Release build (~1.9MB binary)
cargo run                          # Run TUI (debug)
cargo clippy -- -D warnings        # Lint
cargo fmt --check                  # Check formatting
```

No tests, CI, or linting config currently exist.

## Architecture
Flat module structure, ~2,370 lines across 8 files:

| Module | Purpose |
|--------|---------|
| `main.rs` | CLI entry point, clap subcommands (list, check, TUI), event loop |
| `types.rs` | Core types: `ClientKind`, `Transport`, `HealthStatus`, `McpServer`, `DiscoveryResult` |
| `discovery.rs` | Scans 7 client config locations, parses JSON, returns unified server list |
| `app.rs` | Central `App` state machine, input handling, modal transitions |
| `ui.rs` | Ratatui rendering: header, server list, detail panel, client matrix, modals |
| `health.rs` | Stdio health checks via JSON-RPC initialize, background threads + mpsc |
| `config_writer.rs` | Safe config mutations: backup → write to .tmp → atomic rename |
| `wizard.rs` | Multi-step modal wizards: AddWizard, RemoveConfirm, SyncSelect |

**Data flow:** Keyboard → `handle_event()` → App state update → `render()` → Terminal

**Concurrency:** No async runtime. Uses `std::thread::spawn` + `mpsc::channel` for background health checks with 5s timeout.

## Conventions
- **Naming:** snake_case functions, PascalCase types/enums, SCREAMING_SNAKE_CASE constants
- **Errors:** `Result<T, String>` for config operations; non-fatal parse errors collected in `DiscoveryResult.errors`
- **JSON:** Manual parsing via `serde_json::Value` — no serde derive macros on domain types
- **Imports:** std first, then external crates, then `crate::` internal modules. Wildcard imports for `types` and `wizard`
- **File safety:** Every config write creates a `.bak` backup and uses atomic write (`.tmp` + rename)
- **UI modals:** State machine via `Mode` enum (Normal, AddWizard, RemoveConfirm, SyncSelect)
- **Visibility:** Public API per module is minimal; scanner/render helpers are private
- **Comments:** Section separators with `// ---` in ui.rs; doc comments (`///`) on public types/methods

## Key Files
- `src/main.rs` — start here to understand CLI routing and TUI bootstrap
- `src/types.rs` — all data structures and `ClientKind` config path logic
- `src/app.rs` — state management and keybinding dispatch
- `Cargo.toml` — dependencies and metadata

## Supported Clients
| Client | Config Path | Servers Key | Writable |
|--------|------------|-------------|----------|
| Claude Code Global | `~/.claude.json` | `mcpServers` | No (read-only) |
| Claude Code Project | `.mcp.json` | flat or `mcpServers` | Yes |
| Cursor Global | `~/.cursor/mcp.json` | `mcpServers` | Yes |
| Cursor Project | `.cursor/mcp.json` | `mcpServers` | Yes |
| VS Code Project | `.vscode/mcp.json` | `servers` | Yes |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `mcpServers` | Yes |
| Claude Desktop | platform-specific | `mcpServers` | Yes |

## Adding Features
- **New client:** Add variant to `ClientKind` in `types.rs`, implement `config_path()` and `servers_key()`, add scanner in `discovery.rs`
- **New transport:** Add variant to `Transport` enum in `types.rs`, handle in `parse_transport()` and UI detail rendering
- **New modal:** Add variant to `Mode` enum in `wizard.rs`, add handler in `app.rs`, add render function in `ui.rs`
