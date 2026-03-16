# S01: Cross-platform discovery & crash fixes

**Goal:** Fix the three cross-platform bugs that affect basic usability: Windows Claude Desktop discovery, UTF-8 panic in truncation, and Windows editor default.
**Demo:** Claude Desktop servers appear on Windows; non-ASCII server names don't crash the TUI; pressing `e` opens notepad on Windows.

## Must-Haves
- Claude Desktop config discovered at `%APPDATA%\Claude\claude_desktop_config.json` on Windows
- MSIX fallback path checked when standard APPDATA path doesn't exist
- `truncate()` handles multi-byte UTF-8 chars without panicking
- `$EDITOR` defaults to `notepad` on Windows, `vi` on other platforms

## Tasks

- [x] **T01: Windows Claude Desktop discovery + MSIX fallback**
  Add Windows APPDATA path to `ClientKind::config_path()` and `scan_claude_desktop()`. Include MSIX virtualized path as fallback.

- [x] **T02: UTF-8 safe truncation + Windows editor default**
  Fix `truncate()` to use char boundaries. Change editor default to platform-aware.

## Files Likely Touched
- `src/types.rs` — `config_path()` for ClaudeDesktop
- `src/discovery.rs` — `scan_claude_desktop()`
- `src/ui.rs` — `truncate()`
- `src/main.rs` — editor default
