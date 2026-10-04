use serde_json::{Value, json};

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "audio.parameters" => audio_parameters(request),
        "metadata.parse" => metadata_parse(request),
        "decode.scan" => decode_scan(request),
        _ => Err(format!("Unsupported Rust probe operation: {operation}")),
    }
}

fn audio_parameters(request: Value) -> Result<Value, String> {
    let output = request.as_str().ok_or("Expected probe output text")?;
    let mut values = output
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.parse::<i32>().unwrap_or(0))
        .chain([0, 0, 0]);
    Ok(json!({
        "SampleRate": values.next().unwrap_or(0),
        "Channels": values.next().unwrap_or(0),
        "Bits": values.next().unwrap_or(0)
    }))
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct Metadata {
    path: String,
    sample_rate: i32,
    bits: i32,
    channels: i32,
    date: String,
    track: String,
    title: String,
    album: String,
    duration: f64,
    source_sample_rate: i32,
    source_bits: i32,
}

fn metadata_parse(request: Value) -> Result<Value, String> {
    let object = request.as_object().ok_or("Expected metadata request")?;
    let path = object
        .get("Path")
        .and_then(Value::as_str)
        .ok_or("Missing metadata path")?;
    let output = object
        .get("Output")
        .and_then(Value::as_str)
        .ok_or("Missing metadata output")?;
    let mut sample_rate = 0;
    let mut bits = 0;
    let mut channels = 0;
    let mut duration_timestamp = None;
    let mut time_base = None;
    let mut format_duration = None;
    let mut tags = std::collections::HashMap::<String, String>::new();
    for raw in output.split('\n') {
        let line = raw.trim();
        if line.starts_with("sample_rate=") {
            sample_rate = value(line).parse().unwrap_or(0);
        } else if line.starts_with("bits_per_raw_sample=") {
            bits = value(line).parse().unwrap_or(0);
        } else if line.starts_with("channels=") {
            channels = value(line).parse().unwrap_or(0);
        } else if line.starts_with("duration_ts=") {
            duration_timestamp = value(line).parse::<i64>().ok();
        } else if line.starts_with("time_base=") {
            time_base = Some(value(line).to_owned());
        } else if line.starts_with("duration=") {
            format_duration = value(line).parse::<f64>().ok();
        } else if line.starts_with("TAG:") {
            if let Some(separator) = line.find('=') {
                tags.insert(
                    line[4..separator].to_ascii_lowercase(),
                    line[separator + 1..].to_owned(),
                );
            }
        }
    }
    let duration = calculate_duration(duration_timestamp, time_base.as_deref())
        .or(format_duration)
        .unwrap_or(0.0);
    let fallback_title = file_stem(path);
    let tag = |key: &str| tags.get(key).cloned();
    serde_json::to_value(Metadata {
        path: path.to_owned(),
        sample_rate,
        bits,
        channels,
        date: tag("date")
            .or_else(|| tag("releasetime"))
            .unwrap_or_default(),
        track: tag("track").unwrap_or_default(),
        title: tag("title").unwrap_or(fallback_title),
        album: tag("album").unwrap_or_default(),
        duration,
        source_sample_rate: sample_rate,
        source_bits: bits,
    })
    .map_err(|error| error.to_string())
}

fn value(line: &str) -> &str {
    line.split_once('=').map(|(_, value)| value).unwrap_or("")
}

fn calculate_duration(timestamp: Option<i64>, time_base: Option<&str>) -> Option<f64> {
    let timestamp = timestamp?;
    let parts: Vec<_> = time_base?.split('/').collect();
    if parts.len() != 2 {
        return None;
    }
    let numerator = parts[0].parse::<i64>().ok()?;
    let denominator = parts[1].parse::<i64>().ok()?;
    if denominator == 0 {
        return None;
    }
    Some(timestamp as f64 * numerator as f64 / denominator as f64)
}

fn file_stem(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .to_owned()
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct DecodeScan {
    count: i32,
    lines: Vec<String>,
}

fn decode_scan(request: Value) -> Result<Value, String> {
    let stderr = request.as_str().ok_or("Expected decoder stderr text")?;
    let keywords = [
        "error submitting packet to decoder",
        "invalid element",
        "error while decoding",
        "invalid data found",
        "channel element",
        "crc mismatch",
        "corrupt",
        "not implemented",
        "not yet implemented",
    ];
    let mut count = 0;
    let mut lines = Vec::new();
    for line in stderr.split('\n') {
        let lower = line.to_ascii_lowercase();
        if !keywords.iter().any(|keyword| lower.contains(keyword)) {
            continue;
        }
        count += 1;
        if lines.len() < 3 {
            lines.push(line.trim().to_owned());
        }
    }
    serde_json::to_value(DecodeScan { count, lines }).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_prefers_timestamp_and_tags() {
        let value = metadata_parse(json!({
            "Path": "D:/album/song.flac",
            "Output": "sample_rate=48000\nbits_per_raw_sample=24\nchannels=2\nduration_ts=96000\ntime_base=1/48000\nTAG:TITLE=Song\nTAG:ALBUM=Album\n"
        }))
        .unwrap();
        assert_eq!(value["Duration"], 2.0);
        assert_eq!(value["Title"], "Song");
    }

    #[test]
    fn decode_scan_keeps_three_examples() {
        let value = decode_scan(json!("corrupt\nCORRUPT\ninvalid element\ncorrupt\n")).unwrap();
        assert_eq!(value["Count"], 4);
        assert_eq!(value["Lines"].as_array().unwrap().len(), 3);
    }
}
