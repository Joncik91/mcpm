# M001: Reliability & Correctness Pass — Context

**Gathered:** 2026-03-16
**Status:** Ready for planning

## Project Description

mcpm is an existing Rust TUI (~2,600 lines) for discovering and managing MCP server configurations across 8 client config locations. The app works but has cross-platform bugs, destructive edge cases, and silent failures that need fixing.

## Why This Milestone

The developer uses Windows as their primary platform, yet Claude Desktop discovery is completely broken on Windows. UTF-8 handling can crash the app. Config mutations have destructive edge cases. Users get no feedback when operations silently fail. These bugs undermine trust in the tool.

## User-Visible Outcome

### When this milestone is complete, the user can:

- See Claude Desktop servers discovered on Windows in the TUI
- Use non-ASCII server names without crashing
- Press `e` on Windows and have notepad open the config file
- Remove a server from CC-Global without nuking project-scoped entries
- See `.json.bak` backup files alongside their configs
- Press `u` to undo and have it work with the new naming
- Scroll the detail panel without going past the content
- Get clear feedback when pressing `h`/`e`/`u` on servers that don't support those operations

### Entry point / environment

- Entry point: `mcpm` CLI/TUI
- Environment: local dev terminal, Windows and macOS/Linux
- Live dependencies involved: none (local config file operations only)

## Completion Class

- Contract complete means: `cargo build` succeeds, all fixes are in the source code, edge cases handled
- Integration complete means: discovery finds Claude Desktop on Windows, config mutations produce correct JSON
- Operational complete means: none (no services or lifecycle to manage)

## Final Integrated Acceptance

To call this milestone complete, we must prove:

- All 9 bug fixes are implemented and the code compiles clean
- The `truncate()` function handles multi-byte UTF-8 without panicking
- Windows Claude Desktop path resolution includes both APPDATA and MSIX fallback
- CC-Global remove leaves project-scoped entries intact
- Backup/restore cycle works with `.json.bak` naming

## Risks and Unknowns

- MSIX virtualized path pattern may vary across Windows versions — mitigated by glob-based detection
- Scroll bounds require tracking content height during render, which could be tricky with ratatui's rendering model

## Existing Codebase / Prior Art

- `src/types.rs` — `ClientKind::config_path()` handles all path resolution, missing Windows Claude Desktop
- `src/config_writer.rs` — `backup()`, `restore_backup()`, `write_atomic()` use `with_extension()` for naming
- `src/ui.rs` — `truncate()` at line 727 uses byte slicing
- `src/main.rs` — `$EDITOR` default at line 158
- `src/discovery.rs` — `scan_claude_desktop()` only checks macOS and Linux paths
- `src/app.rs` — `handle_normal()` dispatches all keybindings, `scroll_detail_down()` is unbounded

> See `.gsd/DECISIONS.md` for all architectural and pattern decisions — it is an append-only register; read it during planning, append to it during execution.

## Relevant Requirements

- R001–R003 — Cross-platform fixes (S01)
- R004–R005, R009 — Config writer correctness (S02)
- R006–R008 — UX polish (S03)

## Scope

### In Scope

- Windows Claude Desktop discovery (APPDATA + MSIX fallback)
- UTF-8 safe truncation
- Windows editor default
- CC-Global remove scope fix
- Backup naming change to `.json.bak`
- Backup/restore correctness with new naming
- Scroll bounds
- User feedback for no-op operations
- Sync guard for Unknown transport

### Out of Scope / Non-Goals

- New client types or transport types
- Async runtime or major refactors
- New TUI features
- Test suite creation

## Technical Constraints

- Rust edition 2024, no clippy available (not installed)
- Must compile and work on Windows (primary dev platform)
- No async runtime — changes must stay synchronous
- Manual JSON parsing — no serde derive macros on domain types

## Integration Points

- Filesystem: reads/writes JSON config files at platform-specific paths
- Process spawning: `$EDITOR` for config editing, server processes for health checks

## Open Questions

- None — all decisions are locked from the discussion
