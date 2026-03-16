---
id: S01
milestone: M001
provides:
  - Windows Claude Desktop discovery (APPDATA + MSIX fallback)
  - UTF-8 safe truncation (no panics on multi-byte chars)
  - Platform-aware editor default (notepad on Windows)
  - cfg!(windows) gating pattern for platform-specific behavior
key_files:
  - src/types.rs — config_path() with Windows paths, find_msix_claude_config()
  - src/discovery.rs — scan_claude_desktop() with Windows candidates
  - src/ui.rs — truncate() using char_indices()
  - src/main.rs — platform-aware editor default
key_decisions:
  - MSIX path takes priority over standard APPDATA in config_path()
  - char_indices().nth() for safe truncation byte offset
  - notepad as Windows editor default
patterns_established:
  - cfg!(windows) gating for platform-specific paths and defaults
drill_down_paths:
  - .gsd/milestones/M001/slices/S01/tasks/T01-SUMMARY.md
  - .gsd/milestones/M001/slices/S01/tasks/T02-SUMMARY.md
verification_result: pass
completed_at: 2026-03-16T09:20:00Z
---

# S01: Cross-platform discovery & crash fixes

**Fixed Windows Claude Desktop discovery, UTF-8 truncation panic, and editor default**

## What Happened

Three cross-platform bugs fixed in 4 files:

1. **Windows Claude Desktop** — Added `%APPDATA%\Claude\` path via `dirs::data_dir()` and MSIX virtualized path detection via `find_msix_claude_config()` which scans `%LOCALAPPDATA%\Packages\Claude_*`. Both `config_path()` and `scan_claude_desktop()` now check Windows paths first.

2. **UTF-8 truncation** — Rewrote `truncate()` from byte-slicing (`&s[..max-1]`) to char-boundary-aware slicing via `char_indices().nth()`. Prevents panics on non-ASCII server names.

3. **Editor default** — Changed from hardcoded `"vi"` to `cfg!(windows)` conditional: `notepad` on Windows, `vi` elsewhere.

## Deviations
None.
