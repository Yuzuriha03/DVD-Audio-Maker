use serde_json::Map;
use serde_json::Value;

// Match Char.IsWhiteSpace rather than Rust's broader Unicode White_Space set.
pub fn whitespace(c: char) -> bool {
    matches!(c, '\u{9}'..='\u{d}' | '\u{20}' | '\u{85}' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}
pub fn trim(text: &str) -> &str {
    text.trim_matches(whitespace)
}

fn looks_like_path(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with("\\\\")
        || (value.as_bytes().get(1) == Some(&b':')
            && matches!(value.as_bytes().get(2), Some(b'\\' | b'/')))
        || value.contains('\\')
}

pub fn parse(text: &str) -> Map<String, Value> {
    let mut result = Map::new();
    for raw in text.split(['\n', '\r']) {
        let line = trim(raw);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = trim(key);
        let mut chars = key.chars();
        if !chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        let mut value = trim(value);
        let quoted = |s: &str| {
            s.len() >= 2
                && ((s.starts_with('"') && s.ends_with('"'))
                    || (s.starts_with(char::from(39)) && s.ends_with(char::from(39))))
        };
        if !quoted(value)
            && let Some(index) = [value.find(" #"), value.find("\t#")]
                .into_iter()
                .flatten()
                .min()
        {
            value = trim(&value[..index]);
        }
        value = trim(value);
        if quoted(value) {
            value = &value[1..value.len() - 1];
        }
        let value = if looks_like_path(value) {
            value.replace('\\', "/")
        } else {
            value.to_owned()
        };
        result.insert(key.to_owned(), Value::String(value));
    }
    result
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
    fn literals_comments_and_duplicates() {
        let v = parse(
            "A='中 文 #x'\nB=C:\\music\\a # comment\nexport X=1\nA=last\nY=$(keep)\nZ=\"x #y\"",
        );
        assert_eq!(v["A"], "last");
        assert_eq!(v["B"], "C:/music/a");
        assert_eq!(v["Y"], "$(keep)");
        assert_eq!(v["Z"], "x #y");
        assert!(!v.contains_key("X"));
    }
}
