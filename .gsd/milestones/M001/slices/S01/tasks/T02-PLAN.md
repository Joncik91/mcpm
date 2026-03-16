# T02: UTF-8 safe truncation + Windows editor default

**Slice:** S01
**Milestone:** M001

## Goal
Fix the truncation panic on multi-byte UTF-8 and make the editor default platform-aware.

## Must-Haves

### Truths
- `truncate("日本語サーバー", 8)` does not panic and returns a truncated string with ellipsis
- `truncate("short", 10)` returns "short" unchanged
- On Windows, editor defaults to `notepad`; on other platforms, defaults to `vi`

### Artifacts
- `src/ui.rs` — `truncate()` rewritten to use char boundaries
- `src/main.rs` — editor default uses `cfg!(windows)`

### Key Links
- `truncate()` called from `render_server_list()` and `render_matrix()` in ui.rs

## Steps
1. Rewrite `truncate()` to use `.chars().count()` for length check and `.char_indices()` for safe slicing
2. Update editor default in `main.rs` to use `cfg!(windows)` conditional
3. Verify `cargo build` compiles clean

## Context
- Current code: `&s[..max - 1]` — byte slice on string, panics at non-char-boundary
- Rust's `char_indices()` gives (byte_offset, char) pairs — use nth char's byte offset for safe slicing
