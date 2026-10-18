<div align="center">

<img src="docs/logo.png" alt="mcpm" width="160" height="160">

# mcpm

[![Rust 1.85+](https://img.shields.io/badge/rust-1.85%2B-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-E8954A.svg)](LICENSE)
[![ratatui](https://img.shields.io/badge/TUI-ratatui-E8954A)](https://ratatui.rs/)
[![8 clients supported](https://img.shields.io/badge/clients-9%20supported-E8954A)](#config-files-discovered)
[![Binary size: 2.3 MB](https://img.shields.io/badge/release%20binary-2.3%20MB-E8954A)]()
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

- Claude Code → `~/.claude.json` (user + per-project local scope) and `.mcp.json`
- Cursor → `~/.cursor/mcp.json`
- VS Code → `.vscode/mcp.json`
- Windsurf → `~/.codeium/windsurf/mcp_config.json`
- Claude Desktop → platform-specific path

If you have 5 servers across 3 clients, you're manually editing JSON files and hoping you got the structure right. There's no way to see "what's actually configured?" across your setup.

## What You Get

```
┌──────────────────────────────────────────────────────────────────┐
│ mcpm v1.4.0 — 5 servers                                         │
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
 a:add  d:remove  s:sync  e:edit  u/U:undo/redo  h/c:check  r:refresh  ?:help  q:quit
```

## Install

Requires Rust 1.85+ (edition 2024).

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
mcpm check        # Health check all stdio servers in parallel (CI-friendly, exit code 0/1)
mcpm --timeout 20 check   # Longer timeout, e.g. for first-run `npx -y` downloads
mcpm --version
```

## Keybindings

### Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `g` / `G` | First / last server |
| `PgUp` / `PgDn` | Scroll detail panel |
| `r` | Refresh (rescan all configs) |
| `?` | Key help |
| `q` / `Ctrl-C` | Quit |

### Server Management

| Key | Action |
|-----|--------|
| `a` | Add server — wizard for name, transport (stdio/http/sse), command + quoted args or URL + headers, env, client selection. `Shift-Tab` goes back a step |
| `d` | Remove server — lists every config holding that name; only the selected row is pre-checked |
| `s` | Sync — copies the server's entry verbatim (incl. `cwd`, `timeout`, etc.) to clients that don't have it |
| `e` | Edit the server's config file in `$VISUAL` / `$EDITOR` (arguments allowed, e.g. `code -w`) |
| `u` / `U` | Undo / redo add, remove and sync operations from this session (all files of an operation at once) |

### Health Checks

| Key | Action |
|-----|--------|
| `h` | Health check selected server (stdio only) |
| `c` | Health check all stdio servers |
| `!` | Toggle parse error overlay |

## How Health Checks Work

For stdio servers, mcpm spawns the server process, sends a JSON-RPC `initialize` message, and waits for the matching response (default 5s, `--timeout` to change).

- `${VAR}` / `${VAR:-default}` (and VS Code's `${workspaceFolder}`, `${userHome}`, `${env:VAR}`) are expanded in command, args and env first, as clients do.
- Log lines, notifications and `Content-Length` framing before the response are tolerated.
- The server runs in its own process group, which is killed afterwards — `npx` → `node` grandchildren don't leak (Unix).
- On Windows, `.cmd` shims such as `npx` are retried through `cmd /C`.

Results:

- `●` green — healthy, shows server name, version and negotiated protocol version
- `⚠` yellow — timeout
- `✗` red — error (command not found, server error, or exit code plus the last stderr line)

Health checks run in background threads so the TUI stays responsive; results stay attached to the right server across rescans.

## Config Files Discovered

| Client | Path | Format |
|--------|------|--------|
| Claude Code (global) | `~/.claude.json` | top-level `mcpServers` |
| Claude Code (local) | `~/.claude.json` | `projects[<cwd>].mcpServers` — current directory only |
| Claude Code (project) | `.mcp.json` | flat or `mcpServers` wrapped |
| Cursor (global) | `~/.cursor/mcp.json` | `mcpServers` |
| Cursor (project) | `.cursor/mcp.json` | `mcpServers` |
| VS Code (project) | `.vscode/mcp.json` | `servers` (or `mcpServers`); JSONC comments and trailing commas accepted |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `mcpServers` (`serverUrl` read as the URL) |
| Claude Desktop (Windows) | `%APPDATA%\Claude\claude_desktop_config.json` | `mcpServers` |
| Claude Desktop (Windows MSIX) | `%LOCALAPPDATA%\Packages\Claude_*\LocalCache\Roaming\Claude\claude_desktop_config.json` | `mcpServers` |
| Claude Desktop (macOS) | `~/Library/Application Support/Claude/claude_desktop_config.json` | `mcpServers` |
| Claude Desktop (Linux) | `~/.config/Claude/claude_desktop_config.json` | `mcpServers` |
| Claude Code (plugins) | `~/.claude/plugins/marketplaces/*/external_plugins/*/.mcp.json` | flat or wrapped; installed plugins only; remove supported, add/sync/edit not |

## Safety

- **Backup before every write** — `.json.bak` file created alongside the original
- **Atomic writes** — temp file (with the original's permissions, fsynced) then rename
- **No clobbering** — if a config changed on disk since mcpm read it (e.g. Claude Code wrote `~/.claude.json`), the write is refused
- **Minimal diffs** — key order, indentation, line endings and trailing newline are preserved; other fields and servers untouched
- **Symlinks** — symlinked dotfiles are written through to their target, not replaced
- **Comments** — files with JSONC comments are never rewritten (comments would be lost); mcpm tells you to edit them with `e`
- **Undo** — `u` restores exactly what an operation changed, and refuses if the file was modified since

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

- Rust, ~5000 lines including ~50 unit/integration tests (`cargo test`)
- [ratatui](https://ratatui.rs) + crossterm for TUI
- serde_json for config parsing/writing
- No async runtime, no network calls (except spawning local server processes for health checks)
- 2.3 MB release binary

## License

MIT — see [LICENSE](LICENSE).
