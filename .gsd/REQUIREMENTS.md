# Requirements

This file is the explicit capability and coverage contract for the project.

## Active

### R001 — Windows Claude Desktop discovery
- Class: core-capability
- Status: active
- Description: Claude Desktop config on Windows (`%APPDATA%\Claude\claude_desktop_config.json`) must be discovered. MSIX virtualized path should also be checked as fallback.
- Why it matters: On Windows (the developer's own platform), Claude Desktop servers are invisible to mcpm.
- Source: user
- Primary owning slice: M001/S01
- Supporting slices: none
- Validation: unmapped
- Notes: Must check both standard APPDATA path and MSIX LocalCache path

### R002 — UTF-8 safe truncation
- Class: quality-attribute
- Status: active
- Description: The `truncate()` function in ui.rs must use char-boundary-aware slicing instead of byte slicing. No panics on multi-byte UTF-8 server names.
- Why it matters: Any non-ASCII server name (e.g. CJK, emoji, accented chars) causes a panic crash.
- Source: inferred
- Primary owning slice: M001/S01
- Supporting slices: none
- Validation: unmapped
- Notes: `&s[..max-1]` must become char-boundary-safe

### R003 — Windows editor default
- Class: core-capability
- Status: active
- Description: When `$EDITOR` is unset, default to `notepad` on Windows instead of `vi`.
- Why it matters: `vi` doesn't exist on Windows — the `e` key silently fails.
- Source: inferred
- Primary owning slice: M001/S01
- Supporting slices: none
- Validation: unmapped
- Notes: Use `cfg!(windows)` to select default

### R004 — CC-Global remove: top-level only
- Class: core-capability
- Status: active
- Description: Removing a server from CC-Global must only remove from `~/.claude.json` top-level `mcpServers`. Project-scoped entries (`projects[path].mcpServers`) must remain untouched.
- Why it matters: Current behavior is destructive — nukes the server from every project scope, which users don't expect.
- Source: user
- Primary owning slice: M001/S02
- Supporting slices: none
- Validation: unmapped
- Notes: User explicitly chose top-level only

### R005 — Backup file naming (.json.bak)
- Class: quality-attribute
- Status: active
- Description: Backup files should use `.json.bak` extension instead of `.bak`. E.g. `.claude.json` → `.claude.json.bak` instead of `.claude.bak`.
- Why it matters: Clearer association between backup and original file. Users can tell what was backed up.
- Source: user
- Primary owning slice: M001/S02
- Supporting slices: none
- Validation: unmapped
- Notes: User explicitly chose .json.bak. Affects backup(), restore_backup(), and write_atomic() tmp naming.

### R006 — Scroll offset bounds checking
- Class: quality-attribute
- Status: active
- Description: Detail panel scroll offset must be capped to actual content length. Cannot scroll past content into blank space.
- Why it matters: Currently `scroll_detail_down()` increments forever with no upper bound.
- Source: inferred
- Primary owning slice: M001/S03
- Supporting slices: none
- Validation: unmapped
- Notes: Need to track content height and cap scroll_offset accordingly

### R007 — User feedback for no-op operations
- Class: quality-attribute
- Status: active
- Description: Pressing `h` on non-stdio servers, `e` on plugin servers, and `u` on plugin servers must show clear status messages instead of silently doing nothing.
- Why it matters: Users press keys and nothing happens — they don't know if it worked or not.
- Source: inferred
- Primary owning slice: M001/S03
- Supporting slices: none
- Validation: unmapped
- Notes: Three distinct cases: h=non-stdio, e=plugin (no config_path), u=plugin (no config_path)

### R008 — Sync guard for Unknown transport
- Class: quality-attribute
- Status: active
- Description: Syncing a server with Unknown transport must be blocked with a status message instead of writing empty `{}` JSON to config files.
- Why it matters: Syncing an Unknown transport writes meaningless JSON that would break the server config.
- Source: inferred
- Primary owning slice: M001/S03
- Supporting slices: none
- Validation: unmapped
- Notes: Check transport type before entering SyncSelect mode

### R009 — Backup/restore correctness with new naming
- Class: core-capability
- Status: active
- Description: The backup, restore, and undo flow must work correctly with the new `.json.bak` naming scheme. Atomic writes must use `.json.tmp` instead of `.tmp`.
- Why it matters: Changing backup naming affects restore_backup() swap logic and write_atomic() temp file naming.
- Source: inferred
- Primary owning slice: M001/S02
- Supporting slices: none
- Validation: unmapped
- Notes: Must update backup(), restore_backup(), and write_atomic() in config_writer.rs

## Validated

(none yet)

## Deferred

(none)

## Out of Scope

### R010 — New features
- Class: anti-feature
- Status: out-of-scope
- Description: No new client types, transport types, or TUI features in this pass.
- Why it matters: Keeps focus on fixing what exists rather than expanding scope.
- Source: user
- Primary owning slice: none
- Supporting slices: none
- Validation: n/a
- Notes: This is a reliability pass, not a feature pass

### R011 — Architectural changes
- Class: anti-feature
- Status: out-of-scope
- Description: No async runtime, no module restructuring, no major refactors.
- Why it matters: The architecture works — bugs are in logic, not structure.
- Source: inferred
- Primary owning slice: none
- Supporting slices: none
- Validation: n/a
- Notes: Keep changes surgical and minimal

## Traceability

| ID | Class | Status | Primary owner | Supporting | Proof |
|---|---|---|---|---|---|
| R001 | core-capability | active | M001/S01 | none | unmapped |
| R002 | quality-attribute | active | M001/S01 | none | unmapped |
| R003 | core-capability | active | M001/S01 | none | unmapped |
| R004 | core-capability | active | M001/S02 | none | unmapped |
| R005 | quality-attribute | active | M001/S02 | none | unmapped |
| R006 | quality-attribute | active | M001/S03 | none | unmapped |
| R007 | quality-attribute | active | M001/S03 | none | unmapped |
| R008 | quality-attribute | active | M001/S03 | none | unmapped |
| R009 | core-capability | active | M001/S02 | none | unmapped |
| R010 | anti-feature | out-of-scope | none | none | n/a |
| R011 | anti-feature | out-of-scope | none | none | n/a |

## Coverage Summary

- Active requirements: 9
- Mapped to slices: 9
- Validated: 0
- Unmapped active requirements: 0
