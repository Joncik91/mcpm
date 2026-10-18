# mcpm

## Overview
MCP Server Manager — a Rust TUI/CLI for discovering, viewing, and managing MCP server configurations across 9 client configurations (Claude Code global/local/project/plugin, Cursor, VS Code, Windsurf, Claude Desktop).

## Tech Stack
- Rust (edition 2024)
- ratatui 0.29 + crossterm 0.28 (terminal UI)
- clap 4 (CLI parsing with derive)
- serde_json 1 with `preserve_order` (manual JSON parsing — no derive macros; key order survives writes)
- dirs 6 (cross-platform home directory)
- libc 0.2 (Unix only: process-group kill for health checks)

## Development Commands
```bash
cargo build                        # Debug build
cargo build --release              # Release build (~2.3MB binary)
cargo test                         # Unit + integration tests (temp dirs, fake stdio servers)
cargo run                          # Run TUI (debug)
cargo clippy --all-targets -- -D warnings   # Lint
cargo fmt --check                  # Check formatting
```

Tests live in `#[cfg(test)]` modules per file; `testutil.rs` provides isolated temp home/cwd dirs via `Ctx`. No CI config.

## Architecture
Flat module structure, ~5,000 lines across 12 files:

| Module | Purpose |
|--------|---------|
| `main.rs` | CLI entry point, clap subcommands (list, check, TUI), terminal guard + panic hook, event loop, $EDITOR |
| `types.rs` | Core types: `Ctx` (home + cwd), `ClientKind`, `Transport`, `ServerId`, `McpServer` (keeps `raw` JSON), `HealthStatus` |
| `scope.rs` | `Layout`: where servers live in a file. Single source of truth for both reading and writing |
| `discovery.rs` | Scans all client configs via `scope`, returns unified server list + non-fatal errors |
| `jsonc.rs` | JSONC (comments, trailing commas) parsing for VS Code configs |
| `shell.rs` | Quote-aware arg splitting/joining, `${VAR}` expansion |
| `app.rs` | Central `App` state machine, input handling, health map keyed by `ServerId`, undo/redo journal |
| `ui.rs` | Ratatui rendering: header, server list, detail panel, client matrix, modals |
| `health.rs` | Stdio health checks via JSON-RPC initialize, background threads + mpsc, process-group cleanup |
| `config_writer.rs` | Safe config mutations: load → modify → verify unchanged on disk → .bak → atomic write; returns `Change` for undo |
| `wizard.rs` | `Mode` enum, `Checklist`, AddWizard, RemoveConfirm, SyncSelect |
| `testutil.rs` | Test-only temp dir helper |

**Data flow:** Keyboard → `handle_event()` → App state update → `render()` → Terminal

**Concurrency:** No async runtime. Uses `std::thread::spawn` + `mpsc::channel` for background health checks (default 5s, `--timeout`). Results are keyed by `ServerId`, never list index.

## Conventions
- **Naming:** snake_case functions, PascalCase types/enums, SCREAMING_SNAKE_CASE constants
- **Errors:** `Result<T, String>` for config operations; non-fatal parse errors collected in `DiscoveryResult.errors`
- **JSON:** Manual parsing via `serde_json::Value` — no serde derive macros on domain types
- **Imports:** std first, then external crates, then `crate::` internal modules. Wildcard imports for `types` and `wizard`
- **File safety:** Every write goes through `config_writer::replace_file`: refuses if the file changed since it was read, copies to `.bak`, writes a temp file with the original's permissions, renames. Symlinks are resolved. Files with comments are never rewritten. Undo replays the recorded `Change`s in reverse.
- **Versions:** Use `env!("CARGO_PKG_VERSION")` for version strings — never hardcode version numbers
- **UI modals:** State machine via `Mode` enum (Normal, AddWizard, RemoveConfirm, SyncSelect, Help, Errors)
- **Paths:** Never call `dirs::home_dir()` outside `Ctx::from_env()`; pass `&Ctx` so tests can use temp dirs
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
| Claude Code Global | `~/.claude.json` | `mcpServers` | Yes |
| Claude Code Local | `~/.claude.json` | `projects[<cwd>].mcpServers` | Yes (only if the project entry exists) |
| Claude Code Project | `.mcp.json` | flat or `mcpServers` | Yes |
| Claude Code Plugin | `~/.claude/plugins/marketplaces/*/external_plugins/*/.mcp.json` | flat or `mcpServers` | Remove only |
| Cursor Global | `~/.cursor/mcp.json` | `mcpServers` | Yes |
| Cursor Project | `.cursor/mcp.json` | `mcpServers` | Yes |
| VS Code Project | `.vscode/mcp.json` | `servers` or `mcpServers` | Yes (not if it has comments) |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `mcpServers` | Yes |
| Claude Desktop | platform-specific | `mcpServers` | Yes |

## Adding Features
- **New client:** Add variant to `ClientKind` in `types.rs` (`all()`, `writable()`, `label()`, `config_path()`, `layout()`); discovery and writes pick it up via `scope`
- **New transport:** Add variant to `Transport` enum in `types.rs`, handle in `discovery::parse_transport()`, `config_writer::server_value()` and UI detail rendering
- **New modal:** Add variant to `Mode` enum in `wizard.rs`, add handler in `app.rs`, add render function in `ui.rs`, add it to the render smoke test in `ui.rs`
