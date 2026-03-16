---
id: T02
parent: S01
milestone: M001
provides:
  - UTF-8 safe truncation using char_indices()
  - Platform-aware editor default (notepad on Windows, vi elsewhere)
requires:
  - slice: none
    provides: n/a
affects: []
key_files:
  - src/ui.rs
  - src/main.rs
key_decisions:
  - "Used char_indices().nth() for safe byte offset — avoids allocating a new String"
patterns_established:
  - "cfg!(windows) for platform-specific defaults"
drill_down_paths:
  - .gsd/milestones/M001/slices/S01/tasks/T02-PLAN.md
duration: 3min
verification_result: pass
completed_at: 2026-03-16T09:18:00Z
---

# T02: UTF-8 safe truncation + Windows editor default

**Fixed panic-on-multibyte in truncate() and Windows editor default**

## What Happened

Rewrote `truncate()` to use `chars().count()` for length comparison and `char_indices().nth()` for safe byte-offset slicing. The old code used `&s[..max-1]` which panics on multi-byte UTF-8 characters.

Changed the `$EDITOR` fallback from hardcoded `"vi"` to `cfg!(windows)` conditional: `notepad` on Windows, `vi` elsewhere.

## Deviations
None.

## Files Created/Modified
- `src/ui.rs` — Rewrote `truncate()` function
- `src/main.rs` — Platform-aware editor default
