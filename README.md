<div align="center">

<img src="docs/logo.png" alt="mcpm" width="160" height="160">

# mcpm

[![Rust 1.75+](https://img.shields.io/badge/rust-1.75%2B-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-E8954A.svg)](LICENSE)
[![ratatui](https://img.shields.io/badge/TUI-ratatui-E8954A)](https://ratatui.rs/)
[![8 clients supported](https://img.shields.io/badge/clients-8%20supported-E8954A)](#config-files-discovered)
[![Binary size: 1.9 MB](https://img.shields.io/badge/release%20binary-1.9%20MB-E8954A)]()
[![No async runtime](https://img.shields.io/badge/no%20async%20runtime-✓-E8954A)]()

**A terminal dashboard for managing MCP servers across all your clients.**

See everything in one place. Add, remove, sync, health check — without manually editing JSON files.

</div>

## Table of Contents

- [The Problem](#the-problem)
- [What You Get](#what-you-get)
- [Install](#install)
- [Usage](#usage)
- [Keybindings](#keybindings)
- [How Health Checks Work](#how-health-checks-work)
- [Config Files Discovered](#config-files-discovered)
- [Safety](#safety)
- [Security](#security)
- [Tech](#tech)
- [License](#license)

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
│ mcpm v1.3.0 — 5 servers                                         │
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
 a:add  d:remove  s:sync  e:edit  u:undo  h:check  c:check-all  !:errors  q:quit
```

## Install

Requires Rust 1.75+.

```bash
git clone https://github.com/Joncik91/mcpm.git
cd mcpm
cargo build --release
```

Then add it to your PATH (pick one):

```bash
# Option A: install via cargo
cargo install --path .

# Option B: copy to a directory in your PATH
cp target/release/mcpm ~/.cargo/bin/       # Unix/macOS
copy target\release\mcpm.exe %USERPROFILE%\.cargo\bin\   # Windows
```

Or run directly without installing:

```bash
./target/release/mcpm          # Unix/macOS
.\target\release\mcpm.exe      # Windows
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
| `a` | Add server — wizard for name, transport (stdio/http/sse), config, client selection |
| `d` | Remove server from selected clients |
| `s` | Sync server to clients that don't have it |
| `e` | Edit config file in `$EDITOR` |
| `u` | Undo last config change (restore from `.json.bak`) |

### Health Checks

| Key | Action |
|-----|--------|
| `h` | Health check selected server (stdio only) |
| `c` | Health check all stdio servers |
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
| Claude Desktop (Windows) | `%APPDATA%\Claude\claude_desktop_config.json` | `mcpServers` |
| Claude Desktop (Windows MSIX) | `%LOCALAPPDATA%\Packages\Claude_*\LocalCache\Roaming\Claude\claude_desktop_config.json` | `mcpServers` |
| Claude Desktop (macOS) | `~/Library/Application Support/Claude/claude_desktop_config.json` | `mcpServers` |
| Claude Desktop (Linux) | `~/.config/Claude/claude_desktop_config.json` | `mcpServers` |
| Claude Code (plugins) | `~/.claude/plugins/**/external_plugins/**/.mcp.json` | flat (read-only discovery) |

## Safety

- **Backup before every write** — `.json.bak` file created alongside the original
- **Atomic writes** — writes to `.json.tmp` then renames to prevent corruption
- **Read-modify-write** — preserves all existing config fields and other servers
- **Undo** — press `u` to restore the previous config from the `.json.bak` file

## Security

mcpm reads and writes the actual config files your MCP clients use,
and spawns processes to do health checks. Three implications worth
being explicit about:

- **Health checks execute the server's `command`.** When you press
  `h` or `c`, mcpm spawns the configured stdio server's binary with
  the configured args and env. Any server you've added to a client
  config is already trusted to run; mcpm doesn't introduce new
  execution risk, but it also doesn't sandbox what the server does
  during `initialize`. Treat `mcpm check` like `claude` itself —
  don't run it against configs from untrusted sources.
- **Writes happen with your full filesystem permission.** mcpm edits
  files in `~/.claude.json`, `~/.cursor/`, `.vscode/`, and similar
  locations. The [Safety](#safety) primitives (atomic writes, `.bak`
  files, undo) reduce the blast radius of mistakes — but a bug in
  mcpm could still corrupt a config. Keep your dotfiles in version
  control.
- **No network listener.** mcpm has no server, no daemon, no exposed
  port. The only network-adjacent action is spawning local stdio
  servers for health checks, which read/write on the spawned
  process's stdio pipes only.

## Tech

- Rust, ~2750 lines
- [ratatui](https://ratatui.rs) + crossterm for TUI
- serde_json for config parsing/writing
- No async runtime, no network calls (except spawning local server processes for health checks)
- 1.9 MB release binary

## License

MIT — see [LICENSE](LICENSE).
