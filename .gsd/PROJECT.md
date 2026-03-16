# mcpm

## What This Is

A terminal dashboard (Rust TUI) for managing MCP servers across 8 client configurations — Claude Code (global/project/plugin), Cursor (global/project), VS Code, Windsurf, and Claude Desktop. Users can discover, add, remove, sync, health check, and edit MCP server configs from a single interface without manually editing JSON files. ~2,600 lines of Rust using ratatui + crossterm.

## Core Value

See all your MCP servers in one place and manage them without touching JSON files.

## Current State

v1.2.2. All features (add/remove/sync/health check/edit/undo) are implemented and the TUI renders correctly. However, several bugs and edge cases exist: Windows Claude Desktop discovery is missing, UTF-8 truncation can panic, CC-Global remove is over-scoped, and various operations silently fail without user feedback.

## Architecture / Key Patterns

- Flat module structure: `main.rs`, `types.rs`, `discovery.rs`, `app.rs`, `ui.rs`, `health.rs`, `config_writer.rs`, `wizard.rs`
- State machine via `Mode` enum (Normal, AddWizard, RemoveConfirm, SyncSelect)
- No async runtime — `std::thread::spawn` + `mpsc::channel` for background health checks
- Manual JSON parsing via `serde_json::Value` — no derive macros on domain types
- Atomic config writes: backup → write .tmp → rename

## Capability Contract

See `.gsd/REQUIREMENTS.md` for the explicit capability contract, requirement status, and coverage mapping.

## Milestone Sequence

- [ ] M001: Reliability & correctness pass — Fix cross-platform bugs, config writer correctness, and UX edge cases
