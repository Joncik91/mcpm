# S01: Cross-platform discovery & crash fixes — UAT

## Prerequisites
- Windows machine with Claude Desktop installed (standard or MSIX)
- At least one MCP server configured in Claude Desktop

## Test Script

### 1. Claude Desktop Discovery on Windows
1. Run `mcpm list`
2. **Expected:** Claude Desktop servers appear in the list with client label "Desktop"
3. **Expected:** Source path shows `%APPDATA%\Claude\claude_desktop_config.json` or MSIX path

### 2. TUI shows Claude Desktop servers
1. Run `mcpm` (TUI mode)
2. **Expected:** Claude Desktop servers appear in the server list and client matrix

### 3. UTF-8 truncation
1. Add an MCP server with a non-ASCII name (e.g. Japanese, emoji, accented chars)
2. Run `mcpm` (TUI mode)
3. **Expected:** Server name displays truncated with "…" if too long, no crash

### 4. Editor opens on Windows
1. Run `mcpm` (TUI mode)
2. Select a server, press `e`
3. **Expected:** Notepad opens with the config file (when $EDITOR is not set)
