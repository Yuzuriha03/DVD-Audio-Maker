use serde_json::{Value, json};

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "author.normalize_title_mode" => normalize_title_mode(request),
        "author.title_ends" => title_ends(request),
        _ => Err(format!("Unsupported Rust author operation: {operation}")),
    }
}

fn mode(value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    if normalized == "album" || normalized == "one" {
        return normalized;
    }
    match normalized.parse::<i32>() {
        Ok(count) if count > 0 => count.to_string(),
        _ => "album".to_owned(),
    }
}

fn normalize_title_mode(request: Value) -> Result<Value, String> {
    Ok(json!(mode(request.as_str().unwrap_or("album"))))
}

fn title_ends(request: Value) -> Result<Value, String> {
    let object = request.as_object().ok_or("Expected author request")?;
    let disc = object.get("Disc").ok_or("Missing disc")?;
    let title_mode = object
        .get("TitleMode")
        .and_then(Value::as_str)
        .unwrap_or("album");
    let mode = mode(title_mode);
    let numeric = mode.parse::<usize>().unwrap_or(0);
    let mut seen = 0usize;
    let mut result = serde_json::Map::new();
    let groups = disc
        .as_object()
        .and_then(|value| value.get("Groups"))
        .and_then(Value::as_array)
        .ok_or("Missing disc groups")?;
    for group in groups {
        let tracks = group
            .as_object()
            .and_then(|value| value.get("Tracks"))
            .and_then(Value::as_array)
            .ok_or("Missing group tracks")?;
        for (index, track) in tracks.iter().enumerate() {
            let path = track
                .as_object()
                .and_then(|value| value.get("MlpPath"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let album = track
                .as_object()
                .and_then(|value| value.get("Album"))
                .and_then(Value::as_str)
                .unwrap_or("");
            seen += 1;
            let end = index + 1 == tracks.len()
                || (mode != "one"
                    && if numeric > 0 {
                        seen.is_multiple_of(numeric)
                    } else {
                        let next = tracks[index + 1]
                            .as_object()
                            .and_then(|value| value.get("Album"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        album != next
                    });
            result.insert(path.to_owned(), json!(end));
        }
    }
    Ok(Value::Object(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_mode_normalizes_like_csharp() {
        assert_eq!(mode(" invalid "), "album");
        assert_eq!(mode("2"), "2");
        assert_eq!(mode("ONE"), "one");
    }
}
