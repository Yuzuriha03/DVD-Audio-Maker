//! Audio inspection uses typed C requests, without command-line emulation.
use crate::media::{self, Failure};
use dvda_native::media::{Callbacks, Operation as MediaOperation, OutputFormat, Request};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone, Copy, Debug, Deserialize)]
pub enum Operation {
    Metadata,
    Parameters,
    Decode,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    pub library: PathBuf,
    pub input: String,
    pub operation: Operation,
    pub resample_to: Option<i32>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Parameters {
    pub sample_rate: i32,
    pub channels: i32,
    pub bits: i32,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Metadata {
    pub path: String,
    pub sample_rate: i32,
    pub bits: i32,
    pub channels: i32,
    pub date: String,
    pub track: String,
    pub title: String,
    pub album: String,
    pub duration: f64,
    pub source_sample_rate: i32,
    pub source_bits: i32,
    pub resample_to: Option<i32>,
}
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DecodeResult {
    pub samples: Option<i64>,
    pub error_count: i32,
    pub error_lines: Vec<String>,
    pub exit_code: i32,
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Data {
    Metadata(Metadata),
    Parameters(Parameters),
    Decode(DecodeResult),
}
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Outcome {
    pub exit_code: Option<i32>,
    pub failure: Option<Failure>,
    pub data: Option<Data>,
}

struct Capture<'a> {
    caller: &'a mut dyn Callbacks,
    output: String,
    errors: String,
}
impl Callbacks for Capture<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        let text = text.trim_end_matches(['\r', '\n']);
        if stream == 1 {
            self.output.push_str(text);
            self.output.push_str("\r\n");
        } else if stream == 2 {
            self.errors.push_str(text);
            self.errors.push_str("\r\n");
        }
        // Structured probe output belongs to this API, not the user log.
        if stream != 1 {
            self.caller.emit(stream, text);
        }
    }
    fn cancelled(&mut self) -> bool {
        self.caller.cancelled()
    }
}

fn run(
    job: &Job,
    operation: Operation,
    caller: &mut dyn Callbacks,
) -> Result<(i32, String, String), Failure> {
    if caller.cancelled() {
        return Err(Failure::new("Cancelled", "Audio inspection cancelled"));
    }
    if matches!(operation, Operation::Decode) && job.resample_to.is_some_and(|rate| rate < 0) {
        return Err(Failure::new("NotSupported", "Negative resampling rate"));
    }
    let mut capture = Capture {
        caller,
        output: String::new(),
        errors: String::new(),
    };
    let decode = matches!(operation, Operation::Decode);
    let result = media::execute(
        media::Job {
            library: job.library.clone(),
            replace: false,
            timeout_millis: None,
            request: Request {
                operation: if decode {
                    MediaOperation::Audio
                } else {
                    MediaOperation::Probe
                },
                rate: if decode {
                    job.resample_to.unwrap_or(0) as u32
                } else {
                    0
                },
                bits: 0,
                output_format: OutputFormat::None,
                soxr: decode && job.resample_to.is_some(),
                compression: 8,
                cover: false,
                input: job.input.clone(),
                output: None,
                tags: Vec::new(),
            },
        },
        &mut capture,
    );
    if let Some(failure) = result.failure {
        return Err(failure);
    }
    Ok((
        result.exit_code.unwrap_or(1),
        capture.output,
        capture.errors,
    ))
}

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
pub fn parameters_from_probe(document: &Value) -> Parameters {
    let stream = stream(document);
    Parameters {
        sample_rate: int(&stream["sample_rate"]),
        channels: int(&stream["channels"]),
        bits: int(&stream["bits_per_raw_sample"]),
    }
}
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

fn probe(job: &Job, operation: Operation, caller: &mut dyn Callbacks) -> Result<Value, Failure> {
    let (code, output, errors) = run(job, operation, caller)?;
    if code != 0 {
        let message = if matches!(operation, Operation::Parameters) {
            format!(
                "ffprobe 无法探测 {}（退出码 {code}）: {}",
                job.input,
                errors.trim()
            )
        } else {
            format!(
                "ffprobe 无法读取 {}（退出码 {code}）\r\n{errors}",
                job.input
            )
        };
        return Err(Failure::new("InvalidData", &message));
    }
    serde_json::from_str(&output).map_err(|error| Failure::new("InvalidData", &error.to_string()))
}
pub fn read_metadata(job: &Job, caller: &mut dyn Callbacks) -> Result<Metadata, Failure> {
    Ok(metadata_from_probe(
        &job.input,
        &probe(job, Operation::Metadata, caller)?,
    ))
}
pub fn read_parameters(job: &Job, caller: &mut dyn Callbacks) -> Result<Parameters, Failure> {
    Ok(parameters_from_probe(&probe(
        job,
        Operation::Parameters,
        caller,
    )?))
}
pub fn check_decode(job: &Job, caller: &mut dyn Callbacks) -> Result<DecodeResult, Failure> {
    let (code, _, errors) = run(job, Operation::Decode, caller)?;
    Ok(decode_from_log(&errors, code))
}
pub fn execute(job: Job, caller: &mut dyn Callbacks) -> Outcome {
    let result = match job.operation {
        Operation::Metadata => read_metadata(&job, caller).map(Data::Metadata),
        Operation::Parameters => read_parameters(&job, caller).map(Data::Parameters),
        Operation::Decode => check_decode(&job, caller).map(Data::Decode),
    };
    match result {
        Ok(data) => Outcome {
            exit_code: Some(0),
            failure: None,
            data: Some(data),
        },
        Err(failure) => Outcome {
            exit_code: None,
            failure: Some(failure),
            data: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn first_audio_stream_and_format_tags_keep_metadata_contract() {
        let probe = json!({"streams":[{"codec_type":"video","sample_rate":1},
            {"codec_type":"audio","sample_rate":48000,"channels":2,"bits_per_raw_sample":"24",
            "duration_ts":96000,"time_base":"1/48000","tags":{"title":"ignored"}},
            {"codec_type":"audio","sample_rate":44100}],
            "format":{"duration":9.0,"tags":{"TITLE":" 曲目 🎵  \nignored", "DATE":"", "releasetime":"2026", "album":" 日本語"}}});
        let value = metadata_from_probe("D:\\中文\\input.wav", &probe);
        assert_eq!(value.duration, 2.0);
        assert_eq!(value.title, " 曲目 🎵");
        assert_eq!(value.date, "");
        assert_eq!(value.album, " 日本語");
        assert_eq!(value.source_bits, 24);
        assert_eq!(value.channels, 2);
        assert_eq!(
            metadata_from_probe("D:/a.b/song.wav", &json!({})).title,
            "song"
        );
    }

    #[test]
    fn duration_fallback_and_integer_overflow_match_old_host() {
        for base in ["0/0", "bad", "1/0", "1/2/3"] {
            assert_eq!(
                metadata_from_probe(
                    "a",
                    &json!({"streams":[{"codec_type":"audio","duration_ts":4,"time_base":base}],
                "format":{"duration":3.25}})
                )
                .duration,
                3.25
            );
        }
        assert_eq!(
            metadata_from_probe(
                "a",
                &json!({"streams":[{"codec_type":"audio",
            "duration_ts":i64::MAX,"time_base":"2/1"}]})
            )
            .duration,
            -2.0
        );
    }

    #[test]
    fn decode_diagnostics_preserve_first_match_and_three_examples() {
        let result = decode_from_log(
            " CORRUPT \r\ninvalid element\ncorrupt\nnot implemented\nNumber of samples: bad\nNumber of samples: \r\n17\nNumber of samples: 99",
            1,
        );
        assert_eq!(result.samples, Some(17));
        assert_eq!(result.error_count, 4);
        assert_eq!(
            result.error_lines,
            ["CORRUPT", "invalid element", "corrupt"]
        );
        assert_eq!(
            decode_from_log(
                "Number of samples: 999999999999999999999\nNumber of samples: 2",
                0
            )
            .samples,
            None
        );
        assert_eq!(decode_from_log("native failure", 1).error_count, 1);
        assert_eq!(decode_from_log("", 0).samples, None);
    }
}
