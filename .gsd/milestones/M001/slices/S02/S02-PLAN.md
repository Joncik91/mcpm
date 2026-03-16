# S02: Config writer correctness

**Goal:** Fix CC-Global remove scope, backup file naming, and backup/restore correctness.
**Demo:** CC-Global remove only affects top-level mcpServers; backup files named `.json.bak`; undo works with new naming.

## Must-Haves
- CC-Global remove only removes from top-level `mcpServers`, not project-scoped entries
- Backup files use `.json.bak` suffix (e.g. `.claude.json.bak`)
- `write_atomic()` uses `.json.tmp` for temp files
- `restore_backup()` works correctly with `.json.bak` naming
- Undo (`u` key) works correctly with new naming

## Tasks

- [x] **T01: CC-Global remove scope fix**
  Remove the loop that deletes from projects[path].mcpServers entries.

- [x] **T02: Backup naming and restore correctness**
  Change backup/tmp naming from `with_extension()` to string append. Update restore logic.

## Files Likely Touched
- `src/config_writer.rs` — remove_server_inner(), backup(), restore_backup(), write_atomic()
