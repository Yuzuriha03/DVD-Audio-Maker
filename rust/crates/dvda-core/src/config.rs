use serde_json::Value;

// Match Char.IsWhiteSpace rather than Rust's broader Unicode White_Space set.
pub fn whitespace(c: char) -> bool {
    matches!(c, '\u{9}'..='\u{d}' | '\u{20}' | '\u{85}' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

pub fn trim(text: &str) -> &str {
    text.trim_matches(whitespace)
}

pub fn safe_basename(request: Value) -> Result<Value, String> {
    let path = request.as_str().ok_or("Expected path")?;
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = name
        .rsplit_once('.')
        .map(|(value, _)| value)
        .unwrap_or(name);
    let result: String = stem
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    Ok(Value::String(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_matches_windows_host_rules() {
        assert_eq!(trim("\u{3000}value\u{00a0}"), "value");
        assert_eq!(trim("\u{200b}value\u{200b}"), "\u{200b}value\u{200b}");
    }
}
