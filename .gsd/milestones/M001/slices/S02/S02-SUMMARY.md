---
id: S02
milestone: M001
provides:
  - CC-Global remove scoped to top-level mcpServers only
  - Backup files named .json.bak
  - Atomic write temp files named .json.tmp
  - Restore/undo correctness with new naming
  - path_with_suffix() helper
key_files:
  - src/config_writer.rs
key_decisions:
  - CC-Global remove: top-level only (D002)
  - Backup naming: .json.bak (D001)
  - OsString::push() for suffix appending
patterns_established:
  - path_with_suffix() for all backup/tmp/undo naming
drill_down_paths:
  - .gsd/milestones/M001/slices/S02/tasks/T01-SUMMARY.md
  - .gsd/milestones/M001/slices/S02/tasks/T02-SUMMARY.md
verification_result: pass
completed_at: 2026-03-16T09:30:00Z
---

# S02: Config writer correctness

**Fixed CC-Global remove scope and backup file naming**

## What Happened

Two config writer fixes in `config_writer.rs`:

1. **CC-Global remove** — Removed the loop that deleted servers from all `projects[path].mcpServers` entries. Now only the top-level `mcpServers` is affected.

2. **Backup naming** — Added `path_with_suffix()` helper that appends to the full filename (`.claude.json` → `.claude.json.bak`) instead of replacing the extension. Updated all three functions: `backup()`, `write_atomic()`, and `restore_backup()`.

## Deviations
None.
