# S03: UX polish & edge case guards — UAT

## Test Script

### 1. Scroll bounds
1. Run `mcpm`, select a server with enough detail to overflow the panel
2. Press `PgDn` repeatedly
3. **Expected:** Scroll stops when the last line of detail content is visible. No blank space below.
4. Press `PgUp` to scroll back up
5. **Expected:** Scrolls back smoothly, stops at top

### 2. Health check on non-stdio server
1. Select an HTTP or SSE server
2. Press `h`
3. **Expected:** Status bar shows "Health checks only available for stdio servers"

### 3. Edit on plugin server
1. Select a CC-Plugin server
2. Press `e`
3. **Expected:** Status bar shows "Cannot edit plugin configs — they are read-only"

### 4. Undo on plugin server
1. Select a CC-Plugin server
2. Press `u`
3. **Expected:** Status bar shows "Cannot undo plugin config changes"

### 5. Sync on Unknown transport server
1. If an Unknown transport server exists, select it and press `s`
2. **Expected:** Status bar shows "Cannot sync server with unknown transport"
