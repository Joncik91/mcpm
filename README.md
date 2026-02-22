# mcpm

A terminal dashboard for managing MCP servers across all your clients. See everything in one place. Add, remove, sync, health check — without manually editing JSON files.

## The Problem

MCP servers are configured differently across every client:

- Claude Code → `~/.claude.json` and `.mcp.json`
- Cursor → `~/.cursor/mcp.json`
- VS Code → `.vscode/mcp.json`
- Windsurf → `~/.codeium/windsurf/mcp_config.json`
- Claude Desktop → platform-specific path

If you have 5 servers across 3 clients, you're manually editing JSON files and hoping you got the structure right. There's no way to see "what's actually configured?" across your setup.

## What You Get

```
┌──────────────────────────────────────────────────────────────────┐
│ mcpm v1.2.0 — 5 servers                                         │
├──────────────────────┬───────────────────────────────────────────┤
│ Servers              │ Detail                                    │
│                      │                                           │
│ ▸ github     CC-Proj │  Name        github                       │
│   context7   VSCode  │  Client      CC-Project                   │
│   filesystem Cursor  │  Transport   http                         │
│   playwright VSCode ●│  URL         https://api.github...        │
│   memory     Desktop │  Health      ● healthy (github v1.2)      │
├──────────────────────┴───────────────────────────────────────────┤
│ Client Matrix                                                    │
│              CC-Proj  Cursor  VSCode  Desktop                    │
│ github          ✓                                                │
│ context7                        ✓                                │
│ filesystem               ✓      ✓                                │
│ playwright                      ✓                                │
│ memory                                  ✓                        │
└──────────────────────────────────────────────────────────────────┘
 a:add  d:remove  s:sync  e:edit  h:check  H:all  !:errors  q:quit
```

## Install

Requires Rust 1.75+.

```bash
git clone https://github.com/Joncik91/mcpm.git
cd mcpm
cargo build --release
# Binary at ./target/release/mcpm
```

Optionally copy to your PATH:

```bash
cp target/release/mcpm ~/.cargo/bin/
```

## Usage

```bash
mcpm              # Launch TUI
mcpm list         # Plain text server list (for scripting/SSH)
mcpm check        # Health check all stdio servers (CI-friendly, exit code 0/1)
mcpm --version
```

## Keybindings

### Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `PgUp` / `PgDn` | Scroll detail panel |
| `r` | Refresh (rescan all configs) |
| `q` / `Ctrl-C` | Quit |

### Server Management

| Key | Action |
|-----|--------|
| `a` | Add server — wizard for name, command, args, env, client selection |
| `d` | Remove server from selected clients |
| `s` | Sync server to clients that don't have it |
| `e` | Edit config file in `$EDITOR` |

### Health Checks

| Key | Action |
|-----|--------|
| `h` | Health check selected server (stdio only) |
| `H` | Health check all stdio servers |
| `!` | Toggle parse error overlay |

## How Health Checks Work

For stdio servers, mcpm spawns the server process, sends a JSON-RPC `initialize` message, and checks for a valid response within 5 seconds.

- `●` green — healthy, shows server name + version from response
- `⚠` yellow — timeout after 5s
- `✗` red — error (command not found, invalid response, etc.)

Health checks run in background threads so the TUI stays responsive.

## Config Files Discovered

| Client | Path | Format |
|--------|------|--------|
| Claude Code (global) | `~/.claude.json` | `projects[path].mcpServers` |
| Claude Code (project) | `.mcp.json` | flat or `mcpServers` wrapped |
| Cursor (global) | `~/.cursor/mcp.json` | `mcpServers` |
| Cursor (project) | `.cursor/mcp.json` | `mcpServers` |
| VS Code (project) | `.vscode/mcp.json` | `servers` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `mcpServers` |
| Claude Desktop (macOS) | `~/Library/Application Support/Claude/claude_desktop_config.json` | `mcpServers` |
| Claude Desktop (Linux) | `~/.config/Claude/claude_desktop_config.json` | `mcpServers` |

## Safety

- **Backup before every write** — `.bak` file created alongside the original
- **Atomic writes** — writes to `.tmp` then renames to prevent corruption
- **Read-modify-write** — preserves all existing config fields and other servers
- **`~/.claude.json` is read-only** — mcpm reads from it but writes to `.mcp.json` instead

## Tech

- Rust, ~1800 lines
- [ratatui](https://ratatui.rs) + crossterm for TUI
- serde_json for config parsing/writing
- No async runtime, no network calls (except spawning local server processes for health checks)
- 1.9MB release binary

## License

MIT
