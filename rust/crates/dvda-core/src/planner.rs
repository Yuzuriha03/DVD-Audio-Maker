use serde_json::{Value, json};

fn number(v: &Value, key: &str) -> Result<i64, String> {
    v[key]
        .as_i64()
        .ok_or_else(|| format!("Missing integer {key}"))
}
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn size(tracks: &[Value]) -> Result<i64, String> {
    tracks.iter().try_fold(0i64, |sum, t| {
        sum.checked_add(number(t, "MlpSize")?)
            .ok_or("MLP size overflow".into())
    })
}
pub fn estimate(bytes: i64) -> Result<i64, String> {
    let value = (bytes as f64 * 1.025).ceil();
    if value < i64::MIN as f64 || value >= 9223372036854775808.0 {
        return Err("AOB size overflow".into());
    }
    Ok(value as i64)
}
pub fn albums(tracks: &[Value]) -> Vec<(String, Vec<Value>)> {
    let mut albums: Vec<(String, Vec<Value>)> = Vec::new();
    for track in tracks {
        let name = text(track, "Album");
        if let Some((_, items)) = albums.iter_mut().find(|(n, _)| n == name) {
            items.push(track.clone());
        } else {
            albums.push((name.into(), vec![track.clone()]));
        }
    }
    albums
}
fn diagnostic(list: &mut Vec<Value>, severity: i32, code: &str, message: String) {
    list.push(json!({"Severity":severity,"Code":code,"Message":message}));
}
pub fn plan(request: Value) -> Result<Value, String> {
    let tracks = request["Tracks"].as_array().ok_or("Missing Tracks")?;
    let disc_bytes = number(&request, "DiscBytes")?;
    let maximum = number(&request, "MaxDiscs")?;
    let group_limit = number(&request, "GroupTrackLimit")?;
    let limit = disc_bytes.wrapping_sub(8 * 1024 * 1024).max(0);
    let mut diagnostics = Vec::new();
    for track in tracks {
        if number(track, "MlpSize")? <= 0 {
            diagnostic(
                &mut diagnostics,
                2,
                "MLP_MISSING",
                format!(
                    "找不到或无法读取 MLP: {}（{}）",
                    text(track, "MlpPath"),
                    text(track, "Title")
                ),
            );
        }
    }
    let mut disc_albums: Vec<Vec<(String, Vec<Value>)>> = Vec::new();
    let mut current = Vec::new();
    let mut bytes = 0i64;
    for album in albums(tracks) {
        let album_bytes = size(&album.1)?;
        if !current.is_empty() && estimate(bytes.wrapping_add(album_bytes))? > limit {
            disc_albums.push(std::mem::take(&mut current));
            bytes = 0;
        }
        bytes = bytes.wrapping_add(album_bytes);
        current.push(album);
    }
    if !current.is_empty() {
        disc_albums.push(current);
    }
    for (i, items) in disc_albums.iter().enumerate() {
        if items.len() == 1 && estimate(size(&items[0].1)?)? > limit {
            diagnostic(
                &mut diagnostics,
                1,
                "ALBUM_EXCEEDS_DISC",
                format!(
                    "第 {} 盘仅含专辑“{}”，估算 AOB 已超过容量上限；专辑不会被拆分。",
                    i + 1,
                    items[0].0
                ),
            );
        }
    }
    let mut discs = Vec::new();
    for (disc_index, items) in disc_albums.into_iter().enumerate() {
        type Key = (i64, i64, String, i64, i64);
        let mut buckets: Vec<(Key, Vec<Value>)> = Vec::new();
        for (_, tracks) in &items {
            for track in tracks {
                let lpcm = text(track, "MlpSource") == "lpcm";
                let key = (
                    number(track, "SampleRate")?,
                    number(track, "Bits")?,
                    if lpcm { "lpcm" } else { "mlp" }.into(),
                    if lpcm {
                        track["Channels"].as_i64().unwrap_or(0)
                    } else {
                        0
                    },
                    if lpcm {
                        track["ChannelMask"].as_i64().unwrap_or(0)
                    } else {
                        0
                    },
                );
                if let Some((_, bucket)) = buckets.iter_mut().find(|(k, _)| *k == key) {
                    bucket.push(track.clone());
                } else {
                    buckets.push((key, vec![track.clone()]));
                }
            }
        }
        let mut groups = Vec::new();
        for (key, bucket) in buckets {
            let mut chunks: Vec<Vec<Value>> = Vec::new();
            let mut chunk: Vec<Value> = Vec::new();
            for track in bucket {
                if !chunk.is_empty()
                    && text(&track, "Album") != text(chunk.last().unwrap(), "Album")
                    && chunk.len() as i64 >= group_limit
                {
                    chunks.push(std::mem::take(&mut chunk));
                }
                chunk.push(track);
            }
            if !chunk.is_empty() {
                chunks.push(chunk);
            }
            for chunk in chunks {
                if chunk.len() as i64 > group_limit {
                    diagnostic(
                        &mut diagnostics,
                        1,
                        "ALBUM_EXCEEDS_GROUP_LIMIT",
                        format!(
                            "专辑“{}”在 {}Hz/{}bit 组中有 {} 轨，超过组轨上限 {}；为保持专辑完整未拆分。",
                            text(&chunk[0], "Album"),
                            key.0,
                            key.1,
                            chunk.len(),
                            group_limit
                        ),
                    );
                }
                groups.push(
                    json!({"Number":groups.len()+1,"SampleRate":key.0,"Bits":key.1,"Tracks":chunk}),
                );
            }
        }
        let albums: Vec<Value> = items
            .into_iter()
            .map(|(name, tracks)| json!({"Name":name,"Tracks":tracks}))
            .collect();
        discs.push(json!({"Number":disc_index+1,"Albums":albums,"Groups":groups}));
    }
    if maximum > 0 && discs.len() as i64 > maximum {
        diagnostic(
            &mut diagnostics,
            1,
            "DISC_COUNT_EXCEEDED",
            format!("按容量需 {} 张盘，超出期望的 {} 张。", discs.len(), maximum),
        );
    }
    Ok(
        json!({"Tracks":tracks,"Discs":discs,"Diagnostics":diagnostics,"DiscContentLimitBytes":limit}),
    )
}
pub fn track_number(value: &str) -> i32 {
    let digits: String = value
        .trim_start_matches(crate::config::whitespace)
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().unwrap_or(9999)
}
pub fn track_number_value(request: Value) -> Result<Value, String> {
    Ok(json!(track_number(
        request.as_str().ok_or("Expected track text")?
    )))
}
pub fn estimate_value(request: Value) -> Result<Value, String> {
    Ok(json!(estimate(
        request.as_i64().ok_or("Expected MLP bytes")?
    )?))
}
pub fn content_limit_value(request: Value) -> Result<Value, String> {
    Ok(json!(
        request
            .as_i64()
            .ok_or("Expected disc bytes")?
            .saturating_sub(8 * 1024 * 1024)
            .max(0)
    ))
}
pub fn sort(request: Value) -> Result<Value, String> {
    let mut tracks = request.as_array().ok_or("Expected tracks")?.clone();
    // .NET ordinal compares UTF-16 code units, not Unicode scalar values.
    tracks.sort_by(|a, b| {
        text(a, "Date")
            .encode_utf16()
            .cmp(text(b, "Date").encode_utf16())
            .then(track_number(text(a, "Track")).cmp(&track_number(text(b, "Track"))))
            .then(
                text(a, "Title")
                    .encode_utf16()
                    .cmp(text(b, "Title").encode_utf16()),
            )
    });
    Ok(Value::Array(tracks))
}
