# T01: Windows Claude Desktop discovery + MSIX fallback

**Slice:** S01
**Milestone:** M001

## Goal
Make Claude Desktop config discoverable on Windows, including MSIX virtualized installs.

## Must-Haves

### Truths
- On Windows, `ClientKind::ClaudeDesktop.config_path()` returns the APPDATA path
- `scan_claude_desktop()` checks Windows APPDATA path alongside macOS/Linux paths
- MSIX fallback is attempted when standard paths don't exist

### Artifacts
- `src/types.rs` — `config_path()` includes Windows APPDATA path for ClaudeDesktop
- `src/discovery.rs` — `scan_claude_desktop()` includes Windows candidate path + MSIX glob

### Key Links
- `types.rs` config_path → used by `config_writer.rs` add/remove/restore operations
- `discovery.rs` scan_claude_desktop → uses `home()` helper + new APPDATA resolution

## Steps
1. Read `src/types.rs` config_path for ClaudeDesktop — understand current logic
2. Add Windows path: `dirs::data_dir()` returns `%APPDATA%` on Windows → join `Claude/claude_desktop_config.json`
3. Add MSIX fallback: glob `%LOCALAPPDATA%/Packages/Claude*/LocalCache/Roaming/Claude/claude_desktop_config.json`
4. Update `scan_claude_desktop()` in discovery.rs to include Windows candidate
5. Verify `cargo build` compiles clean

## Context
- `dirs::data_dir()` returns `%APPDATA%/Roaming` on Windows, `~/Library/Application Support` on macOS
- Wait — `dirs::data_dir()` returns `%APPDATA%/Roaming` which would give `%APPDATA%/Roaming/Claude/...` but the actual path is `%APPDATA%/Claude/...` where `%APPDATA%` = `C:\Users\X\AppData\Roaming`. So `dirs::data_dir()` IS the right function since it returns the Roaming dir.
- MSIX package name pattern: `Claude_*` under `%LOCALAPPDATA%/Packages/`
