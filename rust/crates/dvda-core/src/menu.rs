use std::collections::{HashMap, HashSet};

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

pub fn visual_near_solid(request: Value) -> Result<Value, String> {
    let standard_deviation = request["StandardDeviation"]
        .as_f64()
        .ok_or("Missing StandardDeviation")?;
    let colors = request["Colors"].as_f64().ok_or("Missing Colors")?;
    Ok(json!(colors <= 50.0 || standard_deviation <= 1.0))
}

pub fn visual_background_invalid(request: Value) -> Result<Value, String> {
    let mean = request.as_f64().ok_or("Expected background mean")?;
    Ok(json!(mean > 160.0))
}

pub fn visual_thumbnail_missing(request: Value) -> Result<Value, String> {
    let mean = request.as_f64().ok_or("Expected thumbnail mean")?;
    Ok(json!(mean <= 3.0))
}

pub fn visual_label_missing(request: Value) -> Result<Value, String> {
    let maximum = request["Maximum"].as_f64().ok_or("Missing label maximum")?;
    let mean = request["Mean"].as_f64().ok_or("Missing label mean")?;
    Ok(json!(maximum <= 200.0 || mean > 200.0))
}

pub fn visual_expected_index_cells(request: Value) -> Result<Value, String> {
    let album_count = request["AlbumCount"]
        .as_i64()
        .ok_or("Missing album count")?;
    let page_index = request["PageIndex"].as_i64().ok_or("Missing page index")?;
    let per_page = request["PerPage"]
        .as_i64()
        .ok_or("Missing per-page count")?;
    if per_page < 1 {
        return Err("Per-page count must be positive".into());
    }
    Ok(json!(
        (album_count - page_index * per_page).clamp(0, per_page)
    ))
}

fn finite_number(value: &str) -> Option<f64> {
    // Invariant NumberStyles.Float accepts ASCII numeric whitespace and trailing NULs.
    let parsed = value
        .trim_end_matches('\0')
        .trim_matches(|c: char| c == ' ' || ('\t'..='\r').contains(&c))
        .parse::<f64>()
        .ok()?;
    parsed.is_finite().then_some(parsed)
}

pub fn parse_batch_frame_stats(request: Value) -> Result<Value, String> {
    let output = request.as_str().ok_or("Expected ImageMagick output")?;
    let lines: Vec<&str> = output
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| line.starts_with("F|"))
        .collect();
    if lines.len() != 1 {
        return Ok(Value::Null);
    }
    let fields: Vec<&str> = lines[0].split('|').collect();
    if fields.len() != 4 {
        return Ok(Value::Null);
    }
    let Some(mean) = finite_number(fields[1]) else {
        return Ok(Value::Null);
    };
    let Some(deviation) = finite_number(fields[2]) else {
        return Ok(Value::Null);
    };
    let Some(colors) = finite_number(fields[3]) else {
        return Ok(Value::Null);
    };
    Ok(json!([mean, deviation, colors]))
}

pub fn parse_index_batch(request: Value) -> Result<Value, String> {
    let output = text(&request, "Output");
    let expected = request["Expected"]
        .as_i64()
        .ok_or("Missing expected cell count")?;
    let succeeded = request["Succeeded"]
        .as_bool()
        .ok_or("Missing success flag")?;
    let mut records = HashMap::<(String, i64), Vec<String>>::new();
    let mut duplicates = HashSet::<(String, i64)>::new();
    for line in output
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_end_matches('\r'))
    {
        let mut fields: Vec<String> = line.split('|').map(str::to_owned).collect();
        if fields.len() > 1 {
            fields[1] = fields[1].trim_end_matches('\0').to_owned();
        }
        if fields.len() < 3
            || !matches!(fields[0].as_str(), "B" | "T" | "L")
            || fields[1].is_empty()
            || !fields[1]
                .chars()
                .all(|character| character.is_ascii_digit())
        {
            continue;
        }
        let Ok(index) = fields[1].parse::<i32>().map(i64::from) else {
            continue;
        };
        if !(1..=expected).contains(&index) {
            continue;
        }
        let key = (fields[0].clone(), index);
        if records.insert(key.clone(), fields).is_some() {
            duplicates.insert(key);
        }
    }

    fn valid(
        records: &HashMap<(String, i64), Vec<String>>,
        duplicates: &HashSet<(String, i64)>,
        succeeded: bool,
        kind: &str,
        index: i64,
        count: usize,
    ) -> Option<(f64, f64)> {
        if !succeeded || duplicates.contains(&(kind.to_owned(), index)) {
            return None;
        }
        let fields = records.get(&(kind.to_owned(), index))?;
        if fields.len() != count {
            return None;
        }
        let first = finite_number(&fields[2])?;
        let second = if count == 3 {
            0.0
        } else {
            finite_number(&fields[3])?
        };
        Some((first, second))
    }

    let mut background = Vec::new();
    let mut thumbnail = Vec::new();
    let mut label = Vec::new();
    if expected > 0 {
        for index in 1..=expected {
            match valid(&records, &duplicates, succeeded, "B", index, 3) {
                Some((value, _)) if value <= 160.0 => {}
                _ => background.push(index),
            }
            match valid(&records, &duplicates, succeeded, "T", index, 3) {
                Some((value, _)) if value > 3.0 => {}
                _ => thumbnail.push(index),
            }
            match valid(&records, &duplicates, succeeded, "L", index, 4) {
                Some((maximum, mean)) if maximum > 200.0 && mean <= 200.0 => {}
                _ => label.push(index),
            }
        }
    }
    Ok(json!({
        "Background": background,
        "Thumbnail": thumbnail,
        "Label": label,
    }))
}

pub fn parse_overlay_batch(request: Value) -> Result<Value, String> {
    let output = request["Output"].as_str().ok_or("Missing overlay output")?;
    let succeeded = request["Succeeded"]
        .as_bool()
        .ok_or("Missing success flag")?;
    let needs_arrow = request["NeedsArrow"]
        .as_bool()
        .ok_or("Missing arrow flag")?;
    let mut values = HashMap::new();
    let mut invalid = HashSet::new();
    if succeeded {
        for line in output.split('\n').filter(|line| !line.is_empty()) {
            let parts: Vec<_> = line.trim_end_matches('\r').split('|').collect();
            let kind = parts[0];
            if !matches!(kind, "N" | "H" | "A") {
                continue;
            }
            let value = (parts.len() == 2)
                .then(|| finite_number(parts[1]))
                .flatten();
            match value {
                Some(value) if !values.contains_key(kind) => {
                    values.insert(kind, value);
                }
                _ => {
                    invalid.insert(kind);
                }
            }
        }
    }
    let get = |kind| {
        if invalid.contains(kind) {
            None
        } else {
            values.get(kind).copied()
        }
    };
    Ok(json!({
        "Normal": get("N"),
        "Highlighted": get("H"),
        "Arrow": if needs_arrow { get("A") } else { None },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_frame_statistics_and_rejects_non_finite_values() {
        assert_eq!(
            parse_batch_frame_stats(Value::String("F|1.5|2.5|3\r\n".into())).unwrap(),
            json!([1.5, 2.5, 3.0])
        );
        assert_eq!(
            parse_batch_frame_stats(Value::String("F|0.5|NaN|1234\n".into())).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn classifies_index_batch_records_and_duplicates() {
        let result = parse_index_batch(json!({
            "Output": "B|1|120\nT|1|4\nL|1|240|80\nB|2|120\nT|2|3\nL|2|240|80\nB|2|120\n",
            "Expected": 2,
            "Succeeded": true,
        }))
        .unwrap();
        assert_eq!(
            result,
            json!({
                "Background": [2],
                "Thumbnail": [2],
                "Label": [],
            })
        );
    }
}
