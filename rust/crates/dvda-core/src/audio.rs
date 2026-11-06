//! Audio inspection uses typed C requests, without command-line emulation.
use crate::media::{self, Failure};
use dvda_native::media::{Callbacks, Operation as MediaOperation, OutputFormat, Request};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

mod parsing;
pub use parsing::{decode_from_log, metadata_from_probe, parameters_from_probe};

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
    #[serde(default)]
    pub decode_output: Option<DecodeOutput>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DecodeOutput {
    pub output: PathBuf,
    pub normalized: PathBuf,
    pub rate: i32,
    pub bits: i32,
    pub dither: bool,
    #[serde(default)]
    pub mlp_output: Option<PathBuf>,
}
impl DecodeOutput {
    #[must_use]
    pub fn prepared_path(&self) -> PathBuf {
        if self.normalized.is_file() {
            self.normalized.clone()
        } else {
            self.output.clone()
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Parameters {
    pub sample_rate: i32,
    pub channels: i32,
    pub bits: i32,
}
/// # Errors
/// Returns an error if executable or dependency discovery, file identity/hash
/// reads, or serialization of the encoding identity fails.
pub fn production_pcm_signature(
    library: &std::path::Path,
    rate: i32,
    bits: i32,
    dither: bool,
) -> Result<String, String> {
    crate::signature::encoding_identity(
        &format!("production-pcm-v1|rate={rate}|bits={bits}|dither={dither}"),
        library,
        None,
        "",
    )
}

/// # Errors
/// Returns an error if an identity-matched PCM file cannot be reopened or
/// validation is cancelled. Missing or stale cache evidence and other WAVE
/// validation failures return `None` instead.
pub fn reusable_production_pcm(
    track: &Value,
    current_source: &Value,
    signature: &str,
    rate: i32,
    bits: i32,
    caller: &mut dyn Callbacks,
) -> Result<Option<PathBuf>, Failure> {
    let Some(source) = track.get("SourceIdentity") else {
        return Ok(None);
    };
    if !crate::identity::equivalent(source, current_source)
        || track.get("PreparedPcmSignature").and_then(Value::as_str) != Some(signature)
    {
        return Ok(None);
    }
    let Some(path) = track
        .get("PreparedPcmPath")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
    else {
        return Ok(None);
    };
    let Some(expected) = track.get("PreparedPcmIdentity") else {
        return Ok(None);
    };
    let Ok(current) = crate::identity::compute(path) else {
        return Ok(None);
    };
    if !crate::identity::equivalent(expected, &current) {
        return Ok(None);
    }
    let mut input = crate::identity::open_read(path)?;
    match crate::pcm::reusable_wave(&mut input, rate, bits, caller) {
        Ok(true) => Ok(Some(PathBuf::from(path))),
        Ok(false) => Ok(None),
        Err(error) if error.kind == "Cancelled" => Err(error),
        Err(_) => Ok(None),
    }
}

#[must_use]
pub fn parameters_from_track(track: &Value, current_identity: &Value) -> Option<Parameters> {
    let evidence = track.get("SourceIdentity")?;
    if !crate::identity::equivalent(evidence, current_identity) {
        return None;
    }
    let parameters = Parameters {
        sample_rate: track.get("SourceSampleRate")?.as_i64()? as i32,
        channels: track.get("Channels")?.as_i64()? as i32,
        bits: track.get("SourceBits")?.as_i64()? as i32,
    };
    (parameters.sample_rate > 0
        && (1..=64).contains(&parameters.bits)
        && (1..=8).contains(&parameters.channels))
    .then_some(parameters)
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

    fn progress(&mut self, completed: u64, total: u64) {
        self.caller.progress(completed, total);
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
    let decode_output = job.decode_output.as_ref();
    #[cfg(feature = "rust-mlp")]
    let streamed = decode_output.and_then(|output| output.mlp_output.as_deref());
    #[cfg(not(feature = "rust-mlp"))]
    let streamed = None::<&std::path::Path>;
    let request = Request {
        operation: if decode {
            MediaOperation::Audio
        } else {
            MediaOperation::Probe
        },
        rate: decode_output.map_or(
            if decode {
                job.resample_to.unwrap_or(0) as u32
            } else {
                0
            },
            |output| output.rate as u32,
        ),
        bits: decode_output.map_or(0, |output| output.bits as u32),
        output_format: if decode_output.is_some() && streamed.is_none() {
            OutputFormat::Wave
        } else {
            OutputFormat::None
        },
        soxr: decode_output.is_none() && decode && job.resample_to.is_some(),
        dither: decode_output.is_some_and(|output| output.dither),
        compression: 8,
        cover: false,
        input: job.input.clone(),
        output: decode_output
            .filter(|_| streamed.is_none())
            .map(|output| output.output.to_string_lossy().into_owned()),
        tags: Vec::new(),
    };
    let result = if let Some(destination) = streamed {
        #[cfg(feature = "rust-mlp")]
        {
            crate::mlp_stream::execute(
                &job.library,
                &job.input,
                destination,
                decode_output.unwrap().rate,
                decode_output.unwrap().bits,
                decode_output.unwrap().dither,
                &mut capture,
            )
        }
        #[cfg(not(feature = "rust-mlp"))]
        {
            let _ = destination;
            unreachable!("streamed output is disabled without rust-mlp")
        }
    } else {
        media::execute(
            media::Job {
                library: job.library.clone(),
                replace: false,
                timeout_millis: None,
                request,
            },
            &mut capture,
        )
    };
    if let Some(failure) = result.failure {
        return Err(failure);
    }
    Ok((
        result.exit_code.unwrap_or(1),
        capture.output,
        capture.errors,
    ))
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
/// # Errors
/// Returns an error on cancellation, media-backend execution failure, a nonzero
/// probe exit code, or invalid JSON probe output.
pub fn read_metadata(job: &Job, caller: &mut dyn Callbacks) -> Result<Metadata, Failure> {
    Ok(metadata_from_probe(
        &job.input,
        &probe(job, Operation::Metadata, caller)?,
    ))
}
/// # Errors
/// Returns an error on cancellation, media-backend execution failure, a nonzero
/// probe exit code, or invalid JSON probe output.
pub fn read_parameters(job: &Job, caller: &mut dyn Callbacks) -> Result<Parameters, Failure> {
    Ok(parameters_from_probe(&probe(
        job,
        Operation::Parameters,
        caller,
    )?))
}
/// # Errors
/// Returns an error on cancellation, negative resampling rates, media/streaming
/// backend failure, or failure to validate and normalize a successful WAVE
/// decode. Decoder log errors and exit codes are otherwise represented in the
/// returned decode result.
pub fn check_decode(job: &Job, caller: &mut dyn Callbacks) -> Result<DecodeResult, Failure> {
    let (code, _, errors) = run(job, Operation::Decode, caller)?;
    let result = decode_from_log(&errors, code);
    if result.error_count == 0
        && result.samples.is_some()
        && let Some(output) = &job.decode_output
        && output.mlp_output.is_none()
    {
        crate::pcm::prepare_wave(
            &output.output,
            &output.normalized,
            output.rate,
            output.bits,
            caller,
        )?;
    }
    Ok(result)
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

    #[test]
    fn track_parameters_require_matching_current_source_identity() {
        let identity = json!({"Size":12,"LastWriteUtcTicks":34,"HeadHash":"a","TailHash":"b"});
        let track = json!({"SourceIdentity":identity,"SourceSampleRate":48000,"SourceBits":24,"Channels":2});
        assert_eq!(
            parameters_from_track(&track, &identity),
            Some(Parameters {
                sample_rate: 48000,
                bits: 24,
                channels: 2,
            })
        );
        let changed = json!({"Size":12,"LastWriteUtcTicks":34,"HeadHash":"changed","TailHash":"b"});
        assert_eq!(parameters_from_track(&track, &changed), None);
        assert_eq!(parameters_from_track(&json!({}), &identity), None);
    }
}
