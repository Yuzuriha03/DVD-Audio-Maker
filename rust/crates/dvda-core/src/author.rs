use serde_json::{Value, json};

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "author.normalize_title_mode" => normalize_title_mode(request),
        "author.title_ends" => title_ends(request),
        "author.build_args" => build_arguments(request),
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

fn build_arguments(request: Value) -> Result<Value, String> {
    let object = request
        .as_object()
        .ok_or("Expected author argument request")?;
    let disc = object
        .get("Disc")
        .and_then(Value::as_object)
        .ok_or("Missing disc")?;
    let title_mode = object
        .get("TitleMode")
        .and_then(Value::as_str)
        .unwrap_or("album");
    let normalized = mode(title_mode);
    let numeric = normalized.parse::<usize>().unwrap_or(0);
    let mut arguments = Vec::new();
    let mut tracks_seen = 0usize;
    let groups = disc
        .get("Groups")
        .and_then(Value::as_array)
        .ok_or("Missing disc groups")?;
    for group in groups {
        arguments.push("-g".to_owned());
        let tracks = group
            .as_object()
            .and_then(|value| value.get("Tracks"))
            .and_then(Value::as_array)
            .ok_or("Missing group tracks")?;
        let mut previous_album: Option<&str> = None;
        for track in tracks {
            let track = track.as_object().ok_or("Invalid author track")?;
            let album = track.get("Album").and_then(Value::as_str).unwrap_or("");
            let boundary = normalized != "one"
                && if numeric > 0 {
                    tracks_seen > 0 && tracks_seen.is_multiple_of(numeric)
                } else {
                    previous_album.is_some_and(|previous| previous != album)
                };
            if boundary {
                arguments.push("-z".to_owned());
            }
            arguments.push(
                track
                    .get("MlpPath")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
            );
            previous_album = Some(album);
            tracks_seen += 1;
        }
    }
    arguments.extend([
        "-o".to_owned(),
        text(object, "OutputDirectory"),
        "-D".to_owned(),
        text(object, "TemporaryDirectory"),
        "-W".to_owned(),
        "-P0".to_owned(),
        "-n".to_owned(),
    ]);
    if let Some(menu) = object.get("MenuArguments").and_then(Value::as_array) {
        arguments.extend(menu.iter().filter_map(Value::as_str).map(str::to_owned));
    }
    if let Some(path) = object
        .get("IsoPath")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
    {
        arguments.push(format!("--iso={path}"));
        if let Some(volume) = object
            .get("IsoVolume")
            .and_then(Value::as_str)
            .filter(|volume| !volume.trim().is_empty())
        {
            arguments.push("--iso-volume".to_owned());
            arguments.push(volume.to_owned());
        }
    }
    Ok(json!(arguments))
}

fn text(object: &serde_json::Map<String, Value>, key: &str) -> String {
    object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
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

    #[test]
    fn builds_author_arguments_with_album_boundaries() {
        let request = json!({
            "Disc":{"Groups":[{"Tracks":[
                {"MlpPath":"a.mlp","Album":"A"},
                {"MlpPath":"b.mlp","Album":"B"}
            ]}]},
            "OutputDirectory":"out",
            "TemporaryDirectory":"tmp",
            "TitleMode":"album",
            "MenuArguments":[],
            "IsoPath":"disc.iso",
            "IsoVolume":"TEST_DISC"
        });
        assert_eq!(
            build_arguments(request).unwrap(),
            json!([
                "-g",
                "a.mlp",
                "-z",
                "b.mlp",
                "-o",
                "out",
                "-D",
                "tmp",
                "-W",
                "-P0",
                "-n",
                "--iso=disc.iso",
                "--iso-volume",
                "TEST_DISC"
            ])
        );
    }
}
