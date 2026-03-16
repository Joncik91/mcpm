# S02: Config writer correctness — UAT

## Prerequisites
- A `.claude.json` with servers in both top-level `mcpServers` and `projects[path].mcpServers`

## Test Script

### 1. CC-Global remove scope
1. Run `mcpm`, select a CC-Global server, press `d`
2. Select CC-Global, confirm removal
3. **Expected:** Server removed from top-level `mcpServers` only
4. **Expected:** Same server still present in project-scoped entries (check `.claude.json` manually)

### 2. Backup naming
1. After any add/remove/sync operation, check the config file's directory
2. **Expected:** Backup file is `<original>.bak` (e.g. `.claude.json.bak`, `.mcp.json.bak`)
3. **Expected:** No stale `.claude.bak` or `.mcp.bak` files from old naming

### 3. Undo
1. After a remove operation, press `u` on the same server's client
2. **Expected:** Server reappears (restored from `.json.bak`)
3. **Expected:** A new `.json.bak` exists (the pre-undo state, for re-undo)
