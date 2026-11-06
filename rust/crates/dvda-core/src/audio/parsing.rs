use super::{DecodeResult, Metadata, Parameters};
use serde_json::Value;
use std::collections::HashMap;

fn int(value: &Value) -> i32 {
    value
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .unwrap_or(0)
}
fn stream(document: &Value) -> &Value {
    document["streams"]
        .as_array()
        .and_then(|streams| {
            streams
                .iter()
                .find(|stream| stream["codec_type"] == "audio")
        })
        .unwrap_or(&Value::Null)
}
#[must_use]
pub fn parameters_from_probe(document: &Value) -> Parameters {
    let stream = stream(document);
    Parameters {
        sample_rate: int(&stream["sample_rate"]),
        channels: int(&stream["channels"]),
        bits: int(&stream["bits_per_raw_sample"]),
    }
}
#[must_use]
pub fn metadata_from_probe(path: &str, document: &Value) -> Metadata {
    let parameters = parameters_from_probe(document);
    let stream = stream(document);
    let duration = (|| {
        let timestamp = stream["duration_ts"].as_i64()?;
        let (numerator, denominator) = stream["time_base"].as_str()?.split_once('/')?;
        let numerator = numerator.parse::<i64>().ok()?;
        let denominator = denominator.parse::<i64>().ok()?;
        (denominator != 0).then(|| timestamp.wrapping_mul(numerator) as f64 / denominator as f64)
    })()
    .or_else(|| document["format"]["duration"].as_f64())
    .unwrap_or(0.0);
    let tags: HashMap<String, String> = document["format"]["tags"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| {
            value.as_str().map(|value| {
                // Preserve the established displayed-tag whitespace and first-line rules.
                (
                    key.to_ascii_lowercase(),
                    value.split('\n').next().unwrap_or("").trim_end().to_owned(),
                )
            })
        })
        .collect();
    let tag = |key: &str| tags.get(key).cloned();
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let title = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    Metadata {
        path: path.into(),
        sample_rate: parameters.sample_rate,
        bits: parameters.bits,
        channels: parameters.channels,
        date: tag("date")
            .or_else(|| tag("releasetime"))
            .unwrap_or_default(),
        track: tag("track").unwrap_or_default(),
        title: tag("title").unwrap_or_else(|| title.into()),
        album: tag("album").unwrap_or_default(),
        duration,
        source_sample_rate: parameters.sample_rate,
        source_bits: parameters.bits,
        resample_to: None,
    }
}

pub fn decode_from_log(stderr: &str, exit_code: i32) -> DecodeResult {
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
    let mut result = DecodeResult {
        samples: None,
        error_count: 0,
        error_lines: Vec::new(),
        exit_code,
    };
    for line in stderr.split('\n') {
        let lower = line.to_ascii_lowercase();
        if keywords.iter().any(|keyword| lower.contains(keyword)) {
            result.error_count += 1;
            if result.error_lines.len() < 3 {
                result.error_lines.push(line.trim().into());
            }
        }
    }
    let mut remaining = stderr;
    while let Some((_, suffix)) = remaining.split_once("Number of samples:") {
        remaining = suffix;
        let digits: String = suffix
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if !digits.is_empty() {
            result.samples = digits.parse().ok();
            break;
        }
    }
    if exit_code != 0 && result.error_count == 0 {
        result.error_count = 1;
        result
            .error_lines
            .push(format!("ffmpeg 退出码 {exit_code}"));
    }
    result
}
