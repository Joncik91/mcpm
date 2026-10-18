//! Shell-style word splitting/quoting for wizard input and $EDITOR, and
//! `${VAR}` expansion as performed by MCP clients when launching servers.

/// Split a command line into words, honoring '…' and "…" quoting.
///
/// Outside quotes a backslash only escapes whitespace, quotes and backslash,
/// so Windows paths like `C:\tools\srv.exe` pass through untouched.
pub fn split(input: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => cur.push(ch),
                        None => return Err("unterminated single quote".to_string()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') if matches!(chars.peek(), Some('"') | Some('\\')) => {
                            cur.push(chars.next().unwrap_or('\\'));
                        }
                        Some(ch) => cur.push(ch),
                        None => return Err("unterminated double quote".to_string()),
                    }
                }
            }
            '\\' => {
                in_word = true;
                match chars.peek() {
                    Some(&n) if n.is_whitespace() || matches!(n, '\'' | '"' | '\\') => {
                        cur.push(n);
                        chars.next();
                    }
                    _ => cur.push('\\'),
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    Ok(words)
}

/// Quote words so that `split(&join(w)) == w`.
pub fn join(words: &[String]) -> String {
    words
        .iter()
        .map(|w| {
            let plain = !w.is_empty()
                && !w
                    .chars()
                    .any(|c| c.is_whitespace() || matches!(c, '\'' | '"' | '\\'));
            if plain {
                w.clone()
            } else {
                format!("'{}'", w.replace('\'', r#"'"'"'"#))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Expand `${NAME}` and `${NAME:-default}` using `lookup`. Unknown variables
/// without a default are left verbatim, so the failure stays visible.
pub fn expand_vars(input: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let expr = &after[..end];
        let (name, default) = match expr.split_once(":-") {
            Some((n, d)) => (n, Some(d)),
            None => (expr, None),
        };
        match lookup(name)
            .filter(|v| !v.is_empty())
            .or(default.map(str::to_string))
        {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[start..start + 2 + end + 1]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn splits_with_quotes() {
        assert_eq!(
            split(r#"-y "@scope/pkg" --dir '/my docs' --h "a \"b\"""#).unwrap(),
            s(&["-y", "@scope/pkg", "--dir", "/my docs", "--h", "a \"b\""])
        );
        assert_eq!(split("  ").unwrap(), Vec::<String>::new());
        assert_eq!(split("a '' b").unwrap(), s(&["a", "", "b"]));
    }

    #[test]
    fn keeps_windows_paths() {
        assert_eq!(
            split(r"C:\tools\srv.exe a\ b").unwrap(),
            s(&[r"C:\tools\srv.exe", "a b"])
        );
    }

    #[test]
    fn unterminated_quote_is_error() {
        assert!(split("a 'b").is_err());
        assert!(split("a \"b").is_err());
    }

    #[test]
    fn join_round_trips() {
        for w in [
            s(&["npx", "-y", "pkg"]),
            s(&["a b", "it's", "", r"C:\x", "q\"q"]),
        ] {
            assert_eq!(split(&join(&w)).unwrap(), w);
        }
    }

    #[test]
    fn expands_vars_and_defaults() {
        let lookup = |k: &str| (k == "HOME").then(|| "/h".to_string());
        assert_eq!(expand_vars("${HOME}/x", &lookup), "/h/x");
        assert_eq!(expand_vars("${NOPE:-d}-${HOME:-z}", &lookup), "d-/h");
        assert_eq!(expand_vars("${NOPE} ${open", &lookup), "${NOPE} ${open");
        assert_eq!(expand_vars("plain $HOME", &lookup), "plain $HOME");
    }
}
