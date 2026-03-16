# S03: UX polish & edge case guards

**Goal:** Fix scroll bounds, add user feedback for no-op operations, and guard against syncing Unknown transport servers.
**Demo:** Scroll stops at content end; `h`/`e`/`u` on unsupported servers shows clear feedback; sync blocked for Unknown transport.

## Must-Haves
- Detail panel scroll offset capped at content length
- Pressing `h` on non-stdio server shows "Health checks only available for stdio servers"
- Pressing `e` on plugin server shows "Cannot edit plugin configs — they are read-only"
- Pressing `u` on plugin server shows "Cannot undo plugin config changes"
- Pressing `s` on Unknown transport server shows "Cannot sync server with unknown transport"

## Tasks

- [x] **T01: Scroll bounds + no-op feedback + sync guard**
  Cap scroll_offset, add status messages for unsupported operations, block sync for Unknown transport. All in one task since each is a few lines.

## Files Likely Touched
- `src/app.rs` — handle_normal() for feedback messages, scroll bounds
- `src/ui.rs` — track content height for scroll capping
