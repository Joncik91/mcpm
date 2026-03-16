---
id: T02
parent: S02
milestone: M001
provides:
  - Backup files named .json.bak (preserving original extension)
  - Atomic write temp files named .json.tmp
  - Restore/undo works with new .json.bak naming
  - path_with_suffix() helper for safe filename suffix appending
requires:
  - slice: none
    provides: n/a
affects: []
key_files:
  - src/config_writer.rs
key_decisions:
  - "Used OsString::push() for suffix appending — avoids format!() and handles non-UTF8 paths"
patterns_established:
  - "path_with_suffix() helper for all backup/tmp/undo file naming"
drill_down_paths:
  - .gsd/milestones/M001/slices/S02/tasks/T02-PLAN.md
duration: 5min
verification_result: pass
completed_at: 2026-03-16T09:28:00Z
---

# T02: Backup naming and restore correctness

**Changed backup naming from .bak to .json.bak using path_with_suffix() helper**

## What Happened

Added `path_with_suffix()` that appends to the full filename via `OsString::push()` — unlike `with_extension()` which replaces the last extension. Updated `backup()`, `write_atomic()`, and `restore_backup()` to use this helper. All `.bak`/`.tmp`/`.undo_tmp` references now go through `path_with_suffix()`.

## Deviations
None.

## Files Created/Modified
- `src/config_writer.rs` — Added `path_with_suffix()`, updated `backup()`, `write_atomic()`, `restore_backup()`
