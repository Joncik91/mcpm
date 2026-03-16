# M001: Reliability & Correctness Pass

**Vision:** Fix cross-platform bugs, config writer correctness issues, and UX edge cases so every mcpm feature works reliably on Windows and macOS/Linux without panics or silent failures.

## Success Criteria

- Claude Desktop servers appear in the TUI when running on Windows
- Non-ASCII server names render correctly without panicking
- `e` key opens the platform-appropriate editor on Windows
- CC-Global remove only affects top-level `mcpServers`, not project-scoped entries
- Backup files use `.json.bak` naming and undo works correctly with it
- Detail panel scroll stops at content boundary
- `h`, `e`, `u` on unsupported servers show clear status messages
- Sync is blocked for Unknown transport servers

## Key Risks / Unknowns

- MSIX virtualized path pattern may vary — mitigated by glob-based detection
- Scroll bounds require knowing content height at render time

## Proof Strategy

- MSIX path detection → retire in S01 by proving Windows path resolution works for both standard and MSIX installs
- Scroll bounds → retire in S03 by proving scroll stops at content end

## Verification Classes

- Contract verification: `cargo build` succeeds, code review of each fix
- Integration verification: none (local filesystem only)
- Operational verification: none
- UAT / human verification: manual testing of each keybinding on Windows

## Milestone Definition of Done

This milestone is complete only when all are true:

- All 9 active requirements are implemented
- `cargo build` is clean
- No panics on multi-byte UTF-8 names
- Windows Claude Desktop path resolution covers both APPDATA and MSIX
- CC-Global remove preserves project-scoped entries
- Backup/restore cycle works with `.json.bak` naming
- All keybindings give feedback on unsupported operations

## Requirement Coverage

- Covers: R001, R002, R003, R004, R005, R006, R007, R008, R009
- Partially covers: none
- Leaves for later: none
- Orphan risks: none

## Slices

- [x] **S01: Cross-platform discovery & crash fixes** `risk:medium` `depends:[]`
  > After this: Claude Desktop servers appear on Windows; non-ASCII server names don't crash; `e` opens notepad on Windows

- [x] **S02: Config writer correctness** `risk:medium` `depends:[S01]`
  > After this: CC-Global remove only affects top-level mcpServers; backup files named `.json.bak`; undo works with new naming

- [x] **S03: UX polish & edge case guards** `risk:low` `depends:[S01]`
  > After this: Scroll stops at content end; `h`/`e`/`u` on unsupported servers shows clear feedback; sync blocked for Unknown transport

## Boundary Map

### S01 → S02

Produces:
- `src/types.rs` → `ClientKind::config_path()` with correct Windows Claude Desktop path (APPDATA + MSIX)
- `src/discovery.rs` → `scan_claude_desktop()` checks Windows path in addition to macOS/Linux
- `src/ui.rs` → `truncate()` is char-boundary-safe, no panics on multi-byte UTF-8
- `src/main.rs` → `$EDITOR` defaults to `notepad` on Windows via `cfg!(windows)`

Consumes:
- nothing (first slice)

### S01 → S03

Produces:
- Same as S01 → S02 (S03 depends on S01 for stable UI rendering)

Consumes:
- nothing (first slice)

### S02 → S03

Produces:
- `src/config_writer.rs` → `remove_server_inner()` for CC-Global only removes from top-level mcpServers
- `src/config_writer.rs` → `backup()` creates `.json.bak` files; `restore_backup()` works with new naming; `write_atomic()` uses `.json.tmp`

Consumes from S01:
- nothing (S02's changes are in config_writer.rs, independent of S01's UI/discovery fixes)
