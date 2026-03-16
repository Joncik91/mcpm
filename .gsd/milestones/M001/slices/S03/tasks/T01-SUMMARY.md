---
id: T01
parent: S03
milestone: M001
provides:
  - Scroll offset bounded to content height
  - Status messages for h/e/u/s on unsupported servers
  - Unknown transport sync guard
requires:
  - slice: S01
    provides: Stable UI rendering
affects: []
key_files:
  - src/app.rs
  - src/ui.rs
key_decisions:
  - "Store content/visible height in App during render — avoids recomputing in scroll handler"
patterns_established:
  - "Early guard pattern: check preconditions before action, show status on failure"
drill_down_paths:
  - .gsd/milestones/M001/slices/S03/tasks/T01-PLAN.md
duration: 8min
verification_result: pass
completed_at: 2026-03-16T09:40:00Z
---

# T01: Scroll bounds + no-op feedback + sync guard

**Added scroll bounds, user feedback for unsupported operations, and Unknown transport sync guard**

## What Happened

Three categories of UX fixes:

1. **Scroll bounds** — Added `detail_content_height` and `detail_visible_height` fields to App, set during `render_detail()`. `scroll_detail_down()` now caps at `content_height - visible_height`.

2. **No-op feedback** — Four cases now show status messages: `h` on non-stdio, `e` on plugin (config_path returns None), `u` on plugin (early guard before restore_backup), `s` on Unknown transport.

3. **Sync guard** — Added `matches!(server.transport, Transport::Unknown)` check before entering SyncSelect mode.

## Deviations
None.

## Files Created/Modified
- `src/app.rs` — Scroll bounds, feedback messages, sync guard
- `src/ui.rs` — Content/visible height tracking in render_detail()
