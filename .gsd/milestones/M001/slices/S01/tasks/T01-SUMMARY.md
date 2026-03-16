---
id: T01
parent: S01
milestone: M001
provides:
  - Windows Claude Desktop config path resolution (APPDATA)
  - MSIX virtualized path detection via find_msix_claude_config()
  - Windows-first candidate ordering in scan_claude_desktop()
requires:
  - slice: none
    provides: n/a
affects: [S02]
key_files:
  - src/types.rs
  - src/discovery.rs
key_decisions:
  - "MSIX path takes priority over standard APPDATA in config_path() — it's more specific"
  - "Standard APPDATA path always returned on Windows even if dir doesn't exist — callers handle missing files"
patterns_established:
  - "cfg!(windows) gating for platform-specific paths in types.rs and discovery.rs"
drill_down_paths:
  - .gsd/milestones/M001/slices/S01/tasks/T01-PLAN.md
duration: 8min
verification_result: pass
completed_at: 2026-03-16T09:15:00Z
---

# T01: Windows Claude Desktop discovery + MSIX fallback

**Added Windows APPDATA and MSIX virtualized path resolution for Claude Desktop config discovery**

## What Happened

Added `cfg!(windows)` gated paths in both `config_path()` (types.rs) and `scan_claude_desktop()` (discovery.rs). On Windows, the standard path is `%APPDATA%\Claude\claude_desktop_config.json` via `dirs::data_dir()`. For MSIX installs, a new `find_msix_claude_config()` function scans `%LOCALAPPDATA%\Packages\Claude_*` for the virtualized config.

Fixed an issue where Windows would fall through to macOS/Linux paths — now `config_path()` returns early on Windows with the correct platform path.

## Deviations
None.

## Files Created/Modified
- `src/types.rs` — Added Windows path logic to `config_path()`, added `find_msix_claude_config()` helper
- `src/discovery.rs` — Updated `scan_claude_desktop()` to check Windows paths first
