# Decisions Register

<!-- Append-only. Never edit or remove existing rows.
     To reverse a decision, add a new row that supersedes it.
     Read this file at the start of any planning or research phase. -->

| # | When | Scope | Decision | Choice | Rationale | Revisable? |
|---|------|-------|----------|--------|-----------|------------|
| D001 | M001 | convention | Backup file naming | `.json.bak` suffix (e.g. `.claude.json.bak`) | Clearer association between backup and original file. User chose this over `.bak`. | No |
| D002 | M001 | arch | CC-Global remove scope | Top-level `mcpServers` only, leave project scopes untouched | Current behavior is destructive — nukes all project entries. User explicitly chose top-level only. | No |
| D003 | M001 | convention | Windows editor default | `notepad` when `$EDITOR` is unset on Windows | `vi` doesn't exist on Windows. `notepad` is universally available. | Yes — if a better default emerges |
