//! Minimal JSONC support: `//` and `/* */` comments plus trailing commas,
//! as accepted by VS Code's `mcp.json`. Comments are blanked out with spaces
//! (newlines kept), so serde_json error line/column numbers stay accurate.

use serde_json::Value;

/// A parsed config document.
pub struct Parsed {
    pub value: Value,
    /// True if the source contained comments, which a rewrite would destroy.
    pub had_comments: bool,
}

/// Parse JSON, falling back to JSONC when plain JSON fails.
pub fn parse(text: &str) -> Result<Parsed, serde_json::Error> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    match serde_json::from_str(text) {
        Ok(value) => Ok(Parsed {
            value,
            had_comments: false,
        }),
        Err(plain_err) => {
            let (clean, had_comments) = strip(text);
            match serde_json::from_str(&clean) {
                Ok(value) => Ok(Parsed {
                    value,
                    had_comments,
                }),
                // Nothing JSONC-specific was found: the original error is the useful one.
                Err(_) if clean == text => Err(plain_err),
                Err(e) => Err(e),
            }
        }
    }
}

/// Remove comments and trailing commas. Returns the cleaned text and whether
/// any comment was present.
fn strip(text: &str) -> (String, bool) {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut had_comments = false;
    let mut i = 0;

    // Pass 1: comments.
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                let end = string_end(&chars, i);
                out.extend(&chars[i..end]);
                i = end;
            }
            '/' if chars.get(i + 1) == Some(&'/') => {
                had_comments = true;
                while i < chars.len() && chars[i] != '\n' {
                    out.push(' ');
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                had_comments = true;
                out.push_str("  ");
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    out.push(if chars[i] == '\n' { '\n' } else { ' ' });
                    i += 1;
                }
                if i < chars.len() {
                    out.push_str("  ");
                    i += 2;
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }

    // Pass 2: trailing commas (comments are gone, so only strings need care).
    let chars: Vec<char> = out.chars().collect();
    let mut result = String::with_capacity(out.len());
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '"' => {
                let end = string_end(&chars, i);
                result.extend(&chars[i..end]);
                i = end;
            }
            ',' => {
                let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
                result.push(if matches!(next, Some('}') | Some(']')) {
                    ' '
                } else {
                    ','
                });
                i += 1;
            }
            c => {
                result.push(c);
                i += 1;
            }
        }
    }
    (result, had_comments)
}

/// Index just past the closing quote of the string starting at `start`
/// (or the end of input if unterminated).
fn string_end(chars: &[char], start: usize) -> usize {
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '"' => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plain_json_has_no_comments() {
        let p = parse(r#"{"a": 1}"#).unwrap();
        assert_eq!(p.value, json!({"a": 1}));
        assert!(!p.had_comments);
    }

    #[test]
    fn comments_and_trailing_commas() {
        let src =
            "{\n  // line\n  \"servers\": { /* block */ \"x\": {\"url\": \"http://a//b\",}, },\n}";
        let p = parse(src).unwrap();
        assert_eq!(p.value, json!({"servers": {"x": {"url": "http://a//b"}}}));
        assert!(p.had_comments);
    }

    #[test]
    fn trailing_comma_only_is_not_a_comment() {
        let p = parse("{\"a\": [1, 2,],}").unwrap();
        assert_eq!(p.value, json!({"a": [1, 2]}));
        assert!(!p.had_comments);
    }

    #[test]
    fn strings_with_escaped_quotes_and_comment_markers_survive() {
        let p = parse(r#"{"a": "say \"/* hi */\" // ok", } // c"#).unwrap();
        assert_eq!(p.value, json!({"a": "say \"/* hi */\" // ok"}));
    }

    #[test]
    fn bom_is_accepted() {
        assert!(parse("\u{feff}{}").is_ok());
    }

    #[test]
    fn invalid_input_reports_error() {
        assert!(parse("{\"a\": }").is_err());
        assert!(parse("{\"a\": 1 /* unterminated").is_err());
    }
}
