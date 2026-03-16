# T02: Backup naming and restore correctness

**Slice:** S02
**Milestone:** M001

## Goal
Change backup file naming from `.bak` to `.json.bak`, temp files from `.tmp` to `.json.tmp`, and ensure restore/undo works correctly.

## Must-Haves

### Truths
- `backup(".claude.json")` creates `.claude.json.bak`
- `write_atomic(".claude.json")` uses `.claude.json.tmp` as temp
- `restore_backup()` swaps `.json.bak` ↔ current correctly
- Undo key (`u`) works end-to-end with new naming

### Artifacts
- `src/config_writer.rs` — `backup()`, `write_atomic()`, `restore_backup()` updated

### Key Links
- `backup()` called from `add_server()` and `remove_server_inner()`
- `restore_backup()` called from `handle_normal()` undo handler in app.rs

## Steps
1. Replace `path.with_extension("bak")` with string-based `.bak` append in `backup()`
2. Replace `path.with_extension("tmp")` with string-based `.tmp` append in `write_atomic()`
3. Update `restore_backup()` to use `.json.bak` naming and `.undo_tmp` → `.json.undo_tmp`
4. Verify `cargo build` compiles clean

## Context
- Decision D001: `.json.bak` naming per user choice
- `with_extension()` replaces the last extension, so `.claude.json` → `.claude.bak` — that's the bug
- String append: `format!("{}.bak", path.display())` gives `.claude.json.bak` — correct
