use crate::{
    audio::{self, Operation as AudioOperation},
    formats, hash, identity, media,
};
use dvda_native::media::{Callbacks, Operation, OutputFormat, Request};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn request_integer(request: &Value, key: &str) -> Result<i64, String> {
    request[key]
        .as_i64()
        .ok_or_else(|| format!("Missing {key}"))
}

#[must_use]
pub fn format_code(rate: i64, bits: i64, channels: i64) -> i64 {
    if !matches!(rate, 44100 | 48000 | 88200 | 96000 | 176_400 | 192_000)
        || !matches!(bits, 16 | 24)
        || !(1..=6).contains(&channels)
        || (rate > 96000 && channels > 2)
    {
        return 1;
    }
    if rate.saturating_mul(bits).saturating_mul(channels) > 9_600_000 {
        return 2;
    }
    0
}

#[must_use]
pub fn layout_code(rate: i64, bits: i64, channels: i64, mask: i64) -> i64 {
    let format = format_code(rate, bits, channels);
    if format != 0 {
        return format;
    }
    if matches!(
        mask,
        4 | 3 | 0x103 | 0x33 | 0xb | 0x10b | 0x3b | 7 | 0x107 | 0x37 | 0xf | 0x10f | 0x3f
    ) {
        0
    } else {
        3
    }
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    let rate = request_integer(&request, "Rate")?;
    let bits = request_integer(&request, "Bits")?;
    let channels = request_integer(&request, "Channels")?;
    let code = match operation {
        "lpcm.validate_format" => format_code(rate, bits, channels),
        "lpcm.validate_layout" => layout_code(
            rate,
            bits,
            channels,
            request_integer(&request, "ChannelMask")?,
        ),
        _ => return Err(format!("Unsupported Rust LPCM operation: {operation}")),
    };
    Ok(json!(code))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PrepareJob {
    pub media_library: PathBuf,
    pub build_directory: PathBuf,
    pub cache_path: PathBuf,
    pub rate: i32,
    pub bits: i32,
    #[serde(default = "dvda_native::media::default_dither")]
    pub dither: bool,
    pub identity: String,
    pub tracks: Vec<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PrepareResult {
    tracks: Vec<Value>,
    cache_hits: i32,
    cache_rebuilt: i32,
    diagnostics: Vec<Value>,
}

struct PreparedTrack {
    track: Value,
    cached: bool,
    cache_entry: Option<(String, Value)>,
}

pub fn execute(mut job: PrepareJob, caller: &mut dyn Callbacks) -> Result<PrepareResult, String> {
    job.identity = crate::signature::encoding_identity(
        &format!("lpcm-rust-pcm-v2|dither={}|{}", job.dither, job.identity),
        &job.media_library,
        None,
        "",
    )?;
    if format_code(job.rate.into(), job.bits.into(), 1) != 0 {
        return Err("LPCM 目标格式不符合 DVD-Audio 限制。".into());
    }
    if !crate::native_components::media_available(&job.media_library) {
        return Err("内置媒体组件缺失，请完整解压发布包。".into());
    }
    let root = job.build_directory.join("lpcm");
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let pcm_signature =
        audio::production_pcm_signature(&job.media_library, job.rate, job.bits, job.dither)?;
    let mut cache = load_cache(&job.cache_path);
    let mut output = Vec::with_capacity(job.tracks.len());
    let mut diagnostics = Vec::new();
    let mut hits = 0;
    let mut rebuilt = 0;
    let mut completed = 0;
    caller.progress(0, job.tracks.len() as u64);
    let mut destinations = std::collections::HashSet::new();
    let independent = job.tracks.iter().all(|track| {
        string(track, "SourcePath")
            .is_some_and(|source| destinations.insert(cache_path(&job.build_directory, source)))
    });
    let workers = if independent {
        crate::options::worker_count(job.tracks.len())
    } else {
        1
    };
    let prepared = crate::parallel::map_with_workers(
        &job.tracks,
        workers,
        caller,
        "LPCM 准备已取消。",
        |index, track, callbacks| {
            prepare_track(&job, &root, &cache, &pcm_signature, index, track, callbacks)
        },
        |_, _, callbacks| {
            completed += 1;
            callbacks.progress(completed, job.tracks.len() as u64);
        },
    )?;
    for (source, result) in job.tracks.iter().zip(prepared) {
        match result {
            Ok(prepared) => {
                if prepared.cached {
                    hits += 1;
                } else {
                    rebuilt += 1;
                }
                if let Some((path, entry)) = prepared.cache_entry {
                    cache.insert(path, entry);
                }
                output.push(prepared.track);
            }
            Err(error) => {
                let title = string(source, "Title").unwrap_or("unknown");
                diagnostics.push(json!({
                    "Severity": 2,
                    "Code": "LPCM_PREPARATION_FAILED",
                    "Message": format!("LPCM 音源准备失败：{title}（{error}）")
                }));
                output.push(source.clone());
            }
        }
    }
    save_cache(&job.cache_path, &cache)?;
    Ok(PrepareResult {
        tracks: output,
        cache_hits: hits,
        cache_rebuilt: rebuilt,
        diagnostics,
    })
}

fn prepare_track(
    job: &PrepareJob,
    root: &Path,
    cache: &Map<String, Value>,
    pcm_signature: &str,
    index: usize,
    track: &Value,
    caller: &mut dyn Callbacks,
) -> Result<PreparedTrack, String> {
    let source_path = string(track, "SourcePath").ok_or("缺少音源路径")?;
    let source_identity = identity::compute(source_path).map_err(|error| error.to_string())?;
    let source_parameters = match audio::parameters_from_track(track, &source_identity) {
        Some(parameters) => parameters,
        None => probe_parameters(&job.media_library, source_path, caller)?,
    };
    let format = format_code(
        i64::from(job.rate),
        i64::from(job.bits),
        i64::from(source_parameters.channels),
    );
    if format != 0 {
        return Err(if format == 2 {
            "LPCM 音频码率超过 9.6 Mb/s，请降低目标参数或选择 MLP 编码。".into()
        } else {
            "LPCM 支持 16/24 位、最多六声道；176.4/192 kHz 最多双声道。".into()
        });
    }
    let destination = cache_path(&job.build_directory, source_path);
    if let Some(entry) = cache.get(&destination)
        && cache_matches(entry, &source_identity, &job.identity, job.bits, job.rate)
        && identity_matches_file(&destination, entry.get("output"))
    {
        let layout = formats::read_wav_layout(&destination)?;
        validate_layout(&layout)?;
        return Ok(PreparedTrack {
            track: updated_track(track, &destination, &layout, &source_parameters),
            cached: true,
            cache_entry: None,
        });
    }
    let directory = root.join(format!(
        ".lpcm-{}-{}-{}",
        std::process::id(),
        index,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let decoded = directory.join("converted.wav");
    let normalized = directory.join("input.wav");
    let result = (|| -> Result<PreparedTrack, String> {
        let reused = audio::reusable_production_pcm(
            track,
            &source_identity,
            pcm_signature,
            job.rate,
            job.bits,
            caller,
        )
        .map_err(|error| error.message)?;
        let prepared = if let Some(path) = reused {
            path
        } else {
            let media_result = media::execute(
                media::Job {
                    library: job.media_library.clone(),
                    request: Request {
                        operation: Operation::Audio,
                        rate: job.rate as u32,
                        bits: job.bits as u32,
                        output_format: OutputFormat::Wave,
                        soxr: false,
                        dither: job.dither,
                        compression: 8,
                        cover: false,
                        input: source_path.into(),
                        output: Some(decoded.to_string_lossy().into_owned()),
                        tags: Vec::new(),
                    },
                    replace: false,
                    timeout_millis: None,
                },
                caller,
            );
            if let Some(error) = media_result.failure {
                return Err(error.message);
            }
            if media_result.exit_code != Some(0) {
                return Err("音源转换失败。".into());
            }
            crate::pcm::prepare_wave(&decoded, &normalized, job.rate, job.bits, caller)
                .map_err(|error| error.message)?
        };
        let layout = formats::read_wav_layout(&prepared.to_string_lossy())?;
        validate_layout(&layout)?;
        if layout.channels != source_parameters.channels {
            return Err("LPCM 转换改变了声道数。".into());
        }
        if !identity_matches_value(source_path, &source_identity)? {
            return Err(format!("编码过程中源文件发生变化: {source_path}"));
        }
        if let Some(parent) = Path::new(&destination).parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        if Path::new(&destination).exists() {
            fs::remove_file(&destination).map_err(|error| error.to_string())?;
        }
        fs::rename(&prepared, &destination).map_err(|error| error.to_string())?;
        let output_identity = identity::compute(&destination).map_err(|error| error.to_string())?;
        let entry = json!({
            "source": source_identity,
            "output": output_identity,
            "encoder": job.identity,
            "bits": job.bits,
            "resample_to": job.rate,
            "max_interval": 0
        });
        Ok(PreparedTrack {
            track: updated_track(track, &destination, &layout, &source_parameters),
            cached: false,
            cache_entry: Some((destination, entry)),
        })
    })();
    let _ = fs::remove_dir_all(&directory);
    result
}

fn probe_parameters(
    library: &Path,
    input: &str,
    caller: &mut dyn Callbacks,
) -> Result<audio::Parameters, String> {
    audio::read_parameters(
        &audio::Job {
            library: library.to_path_buf(),
            input: input.to_owned(),
            operation: AudioOperation::Parameters,
            resample_to: None,
            decode_output: None,
        },
        caller,
    )
    .map_err(|error| error.message)
}

fn updated_track(
    track: &Value,
    destination: &str,
    layout: &formats::WavLayout,
    source: &audio::Parameters,
) -> Value {
    let mut track = track.as_object().cloned().unwrap_or_default();
    track.insert("MlpPath".into(), Value::String(destination.into()));
    track.insert(
        "MlpSize".into(),
        Value::Number(fs::metadata(destination).map_or(0, |m| m.len()).into()),
    );
    track.insert("ChannelMask".into(), json!(layout.channel_mask));
    track.insert("MlpSource".into(), json!("lpcm"));
    track.insert("SampleRate".into(), json!(layout.sample_rate));
    track.insert("Bits".into(), json!(layout.valid_bits));
    track.insert("Channels".into(), json!(layout.channels));
    track.insert("SourceSampleRate".into(), json!(source.sample_rate));
    track.insert("SourceBits".into(), json!(source.bits));
    track.insert(
        "ResampleTo".into(),
        if source.sample_rate == layout.sample_rate {
            Value::Null
        } else {
            json!(layout.sample_rate)
        },
    );
    track.insert(
        "ParametersChanged".into(),
        json!(source.sample_rate != layout.sample_rate || source.bits != layout.valid_bits),
    );
    Value::Object(track)
}

#[must_use]
pub fn cache_path(build_directory: &Path, source_path: &str) -> String {
    let canonical = std::path::absolute(source_path)
        .unwrap_or_else(|_| PathBuf::from(source_path))
        .to_string_lossy()
        .to_uppercase();
    build_directory
        .join("lpcm")
        .join(format!("{}.wav", hash::hex_digest(canonical.as_bytes())))
        .to_string_lossy()
        .into_owned()
}

fn cache_matches(entry: &Value, source: &Value, identity: &str, bits: i32, rate: i32) -> bool {
    same_identity(entry.get("source"), Some(source))
        && string(entry, "encoder") == Some(identity)
        && integer(entry, "bits") == Some(i64::from(bits))
        && integer(entry, "resample_to") == Some(i64::from(rate))
        && integer(entry, "max_interval") == Some(0)
}

fn identity_matches_file(path: &str, expected: Option<&Value>) -> bool {
    let Some(expected) = expected else {
        return false;
    };
    identity::compute(path).is_ok_and(|actual| same_identity(Some(&actual), Some(expected)))
}

fn identity_matches_value(path: &str, expected: &Value) -> Result<bool, String> {
    let actual = identity::compute(path).map_err(|error| error.to_string())?;
    Ok(same_identity(Some(&actual), Some(expected)))
}

fn same_identity(left: Option<&Value>, right: Option<&Value>) -> bool {
    let Some(left) = left else { return false };
    let Some(right) = right else { return false };
    ["Size", "LastWriteUtcTicks", "HeadHash", "TailHash"]
        .iter()
        .all(|key| left.get(*key) == right.get(*key))
}

fn validate_layout(layout: &formats::WavLayout) -> Result<(), String> {
    let code = layout_code(
        i64::from(layout.sample_rate),
        i64::from(layout.valid_bits),
        i64::from(layout.channels),
        i64::from(layout.channel_mask),
    );
    match code {
        0 => Ok(()),
        1 => Err("LPCM 支持 16/24 位、最多六声道；176.4/192 kHz 最多双声道。".into()),
        2 => Err("LPCM 音频码率超过 9.6 Mb/s，请降低目标参数或选择 MLP 编码。".into()),
        _ => Err("LPCM 不支持此声道布局，请使用标准 DVD-Audio 声道布局。".into()),
    }
}

fn integer(object: &Value, key: &str) -> Option<i64> {
    object.get(key).and_then(Value::as_i64)
}

fn string<'a>(object: &'a Value, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str)
}

fn load_cache(path: &Path) -> Map<String, Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn save_cache(path: &Path, entries: &Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = PathBuf::from(format!("{}.tmp", path.to_string_lossy()));
    let text = serde_json::to_string_pretty(entries).map_err(|error| error.to_string())? + "\n";
    fs::write(&temporary, text).map_err(|error| error.to_string())?;
    if let Err(error) = fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    fs::rename(&temporary, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_dvd_audio_format_matrix_and_bitrate_limit() {
        assert_eq!(format_code(48000, 24, 6), 0);
        assert_eq!(format_code(192_000, 24, 3), 1);
        assert_eq!(format_code(96000, 24, 6), 2);
    }

    #[test]
    fn rejects_unknown_channel_masks() {
        assert_eq!(layout_code(48000, 24, 2, 0x1234), 3);
        assert_eq!(layout_code(48000, 24, 2, 3), 0);
    }
}
