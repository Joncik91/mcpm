# Decisions Register

<!-- Append-only. Never edit or remove existing rows.
     To reverse a decision, add a new row that supersedes it.
     Read this file at the start of any planning or research phase. -->

| # | When | Scope | Decision | Choice | Rationale | Revisable? |
|---|------|-------|----------|--------|-----------|------------|
| D001 | M001 | convention | Backup file naming | `.json.bak` suffix (e.g. `.claude.json.bak`) | Clearer association between backup and original file. User chose this over `.bak`. | No |
| D002 | M001 | arch | CC-Global remove scope | Top-level `mcpServers` only, leave project scopes untouched | Current behavior is destructive — nukes all project entries. User explicitly chose top-level only. | No |
| D003 | M001 | convention | Windows editor default | `notepad` when `$EDITOR` is unset on Windows | `vi` doesn't exist on Windows. `notepad` is universally available. | Yes — if a better default emerges |
| D004 | M002 | arch | `~/.claude.json` `projects[...]` servers | New `CC-Local` client for `projects[<cwd>]` only; other projects' servers are no longer listed | They were shown as CC-Global but could not be removed (D002), and other directories' local servers don't apply here. Matches Claude Code's user/local/project scopes. | Yes |
| D005 | M002 | arch | Undo semantics | Session journal of exact file changes (undo + redo), refusing if a file changed since; `.json.bak` still written on every change | Old `.bak` swap restored the selected row's client file, often the wrong or a days-old backup. | Yes |
| D006 | M002 | convention | JSONC configs | Read comments/trailing commas; refuse to rewrite files that contain comments | Rewriting would silently delete the user's comments. | Yes |
| D007 | M002 | arch | Sync payload | Copy the raw on-disk entry, not a rebuild from parsed fields | Rebuild dropped `cwd`, `timeout`, `disabled`, non-string env, SSE headers. | No |
