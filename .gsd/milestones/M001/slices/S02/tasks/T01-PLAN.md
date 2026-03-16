# T01: CC-Global remove scope fix

**Slice:** S02
**Milestone:** M001

## Goal
CC-Global remove only removes from top-level mcpServers, not project-scoped entries.

## Must-Haves

### Truths
- Removing a server from CC-Global only removes it from `root["mcpServers"]`
- Project-scoped entries in `root["projects"][path]["mcpServers"]` are untouched

### Artifacts
- `src/config_writer.rs` — `remove_server_inner()` CC-Global branch simplified

### Key Links
- `remove_server_inner()` called from `remove_server()` and `execute_remove()` in app.rs

## Steps
1. Read `remove_server_inner()` CC-Global branch
2. Remove the projects iteration loop
3. Verify `cargo build` compiles clean

## Context
- Decision D002: top-level only, per user's explicit choice
