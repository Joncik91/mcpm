---
id: M001
title: Reliability & Correctness Pass
slices_completed: [S01, S02, S03]
verification_result: pass
completed_at: 2026-03-16T09:45:00Z
---

# M001: Reliability & Correctness Pass

**Fixed 9 cross-platform bugs, config writer issues, and UX edge cases across 3 slices**

## S01: Cross-platform discovery & crash fixes
- Windows Claude Desktop discovery (APPDATA + MSIX fallback)
- UTF-8 safe truncation (no panics on multi-byte chars)
- Windows editor default (notepad)

## S02: Config writer correctness
- CC-Global remove scoped to top-level mcpServers only
- Backup naming changed to .json.bak
- Restore/undo correctness with new naming
- `path_with_suffix()` helper for safe filename suffix appending

## S03: UX polish & edge case guards
- Detail panel scroll bounded to content height
- Status feedback for h/e/u on unsupported server types
- Unknown transport sync guard

## Key Files Changed
- `src/types.rs` — Windows Claude Desktop paths, MSIX detection
- `src/discovery.rs` — Platform-aware Claude Desktop scanning
- `src/ui.rs` — UTF-8 truncation, scroll height tracking
- `src/main.rs` — Platform-aware editor default
- `src/config_writer.rs` — Remove scope, backup naming, path_with_suffix()
- `src/app.rs` — Scroll bounds, feedback messages, sync guard

## Key Decisions
- D001: Backup naming → .json.bak
- D002: CC-Global remove → top-level only
- D003: Windows editor default → notepad
