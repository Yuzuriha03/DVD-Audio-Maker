use serde_json::{Value, json};

fn is_wide_unit(value: u16) -> bool {
    (0x2e80..=0x9fff).contains(&value) || value >= 0xff00
}
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn from_units(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

pub fn sanitize(request: Value) -> Result<Value, String> {
    let value = request.as_str().ok_or("Expected menu text")?;
    let input = units(value);
    if !input.iter().any(|unit| matches!(*unit, 0x2c | 0x3a | 0x3d)) {
        return Ok(json!({"Value":value,"Changed":false}));
    }
    let wide = input.iter().any(|unit| is_wide_unit(*unit));
    let result: Vec<u16> = input
        .into_iter()
        .map(|unit| match unit {
            0x2c if wide => 0xff0c,
            0x2c => 0x00b7,
            0x3a if wide => 0xff1a,
            0x3a => 0x00b7,
            0x3d if wide => 0xff1d,
            0x3d => 0x00b7,
            other => other,
        })
        .collect();
    Ok(json!({"Value":from_units(&result),"Changed":true}))
}

fn text_pixels(value: &str, font_size: i64) -> i64 {
    let wide = units(value)
        .iter()
        .filter(|unit| is_wide_unit(**unit))
        .count() as f64;
    let length = units(value).len() as f64;
    (wide * font_size as f64 + (length - wide) * font_size as f64 * 0.55) as i64
}
fn round_even(value: f64) -> f64 {
    let lower = value.floor();
    let fraction = value - lower;
    if fraction < 0.5 || (fraction == 0.5 && (lower as i64) % 2 == 0) {
        lower
    } else {
        lower + 1.0
    }
}
pub fn truncate(request: Value) -> Result<Value, String> {
    let value = text(&request, "Value");
    let font_size = request["FontSize"].as_i64().ok_or("Missing FontSize")?;
    let budget = request["Budget"].as_i64().unwrap_or(660);
    if text_pixels(value, font_size) <= budget {
        return Ok(Value::String(value.to_owned()));
    }
    let mut output = units(value);
    while !output.is_empty() && text_pixels(&(from_units(&output) + "~"), font_size) > budget {
        output.pop();
    }
    let result = if output.is_empty() {
        String::new()
    } else {
        format!("{}~", from_units(&output).trim_end())
    };
    Ok(Value::String(result))
}
pub fn font_size(request: Value) -> Result<Value, String> {
    let rows = request.as_i64().ok_or("Expected row count")?;
    let span = rows + 4;
    let label_height = (576 - 56 - 40 - span * 12) / span;
    Ok(json!((label_height + 2).clamp(7, 30)))
}
pub fn font_width(request: Value) -> Result<Value, String> {
    let texts = request.as_array().ok_or("Expected text array")?;
    let mut wide = 0f64;
    let mut narrow = 0f64;
    let mut bytes = 0f64;
    for item in texts {
        let value = item.as_str().ok_or("Expected text string")?;
        for unit in units(value) {
            if is_wide_unit(unit) {
                wide += 1.0;
                bytes += 3.0
            } else {
                narrow += 1.0;
                bytes += 1.0
            }
        }
    }
    if bytes == 0.0 {
        return Ok(json!(5));
    }
    Ok(json!(
        round_even(10.0 * (wide + 0.5 * narrow) / bytes).clamp(1.0, 10.0) as i64
    ))
}
pub fn short_album(request: Value) -> Result<Value, String> {
    let value = text(&request, "Value");
    let max = request["MaximumCharacters"].as_i64().unwrap_or(24).max(0) as usize;
    let index = value
        .char_indices()
        .find(|(_, c)| matches!(c, '(' | '[' | '（' | '【'))
        .map(|(i, _)| i)
        .unwrap_or(value.len());
    let mut output = value[..index]
        .trim()
        .trim_end_matches(['-', '–', '—'])
        .trim()
        .to_owned();
    if output.is_empty() {
        output = value.to_owned();
    }
    output = from_units(&units(&output).into_iter().take(max).collect::<Vec<_>>());
    Ok(Value::String(output))
}
pub fn normalize_path(request: Value) -> Result<Value, String> {
    Ok(Value::String(
        request.as_str().ok_or("Expected path")?.replace('\\', "/"),
    ))
}

pub fn pages(request: Value) -> Result<Value, String> {
    let tracks = request["Tracks"].as_array().ok_or("Missing Tracks")?;
    let albums = request["Albums"].as_array().ok_or("Missing Albums")?;
    let row_cap = request["RowCap"].as_i64().ok_or("Missing RowCap")?;
    if tracks.len() != albums.len() {
        return Err("Track and album lists must have equal lengths".into());
    }
    if row_cap < 1 {
        return Err("RowCap must be positive".into());
    }
    let mut result = Vec::new();
    let mut block_start = 0usize;
    while block_start < tracks.len() {
        let album = albums[block_start].as_str().unwrap_or("");
        let mut block_end = block_start + 1;
        while block_end < albums.len() && albums[block_end].as_str().unwrap_or("") == album {
            block_end += 1;
        }
        let mut page_start = block_start;
        let mut continuation = false;
        while page_start < block_end {
            let page_end = (page_start + row_cap as usize).min(block_end);
            result.push(json!({"Album":album,"StartTrackIndex":page_start,"EndTrackIndex":page_end,"Continuation":continuation,"Tracks":tracks[page_start..page_end].to_vec()}));
            continuation = true;
            page_start = page_end;
        }
        block_start = block_end;
    }
    Ok(Value::Array(result))
}
