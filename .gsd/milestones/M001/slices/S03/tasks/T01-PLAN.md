# T01: Scroll bounds + no-op feedback + sync guard

**Slice:** S03
**Milestone:** M001

## Goal
Fix three UX issues: unbounded scroll, silent no-ops, and Unknown transport sync.

## Must-Haves

### Truths
- scroll_offset cannot exceed content height minus visible area
- `h` on non-stdio → status message
- `e` on plugin → status message
- `u` on plugin → status message
- `s` on Unknown transport → status message, no modal opens

### Artifacts
- `src/app.rs` — updated handle_normal() for all feedback cases, scroll bounds in scroll_detail_down()
- `src/ui.rs` — detail panel content height tracking

### Key Links
- app.rs handle_normal() → set_status() for feedback
- app.rs scroll_detail_down() → needs content height from build_detail_lines()

## Steps
1. Add feedback to `h` key handler when transport is not stdio
2. Add feedback to `e` key handler when config_path returns None (plugin)
3. Add feedback to `u` key handler when config_path returns None (plugin)
4. Add Unknown transport guard to `s` key handler
5. Track detail content height in App state, cap scroll_offset
6. Verify `cargo build` compiles clean

## Context
- build_detail_lines() in ui.rs produces a Vec<Line> — its len() is the content height
- The detail panel visible height is the render area height minus borders (2)
- Simplest approach: compute content height in scroll_detail_down() by checking selected server, or store it during render
