---
id: T01
parent: S02
milestone: M001
provides:
  - CC-Global remove scoped to top-level mcpServers only
requires:
  - slice: none
    provides: n/a
affects: []
key_files:
  - src/config_writer.rs
key_decisions:
  - "Removed projects iteration loop per D002"
patterns_established: []
drill_down_paths:
  - .gsd/milestones/M001/slices/S02/tasks/T01-PLAN.md
duration: 2min
verification_result: pass
completed_at: 2026-03-16T09:25:00Z
---

# T01: CC-Global remove scope fix

**CC-Global remove now only removes from top-level mcpServers, not project-scoped entries**

## What Happened

Removed the 6-line loop in `remove_server_inner()` that iterated over `root["projects"]` and deleted the server from every project's `mcpServers`. Now only the top-level `root["mcpServers"]` entry is removed.

## Deviations
None.

## Files Created/Modified
- `src/config_writer.rs` — Simplified CC-Global branch in `remove_server_inner()`
