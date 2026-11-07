//! Reconstruct the encoding input and compare full decoded PCM without changing
//! either the source or the encoded file, using bounded linked streams.
#[cfg(test)]
use crate::media;
use dvda_native::media::Callbacks;
#[cfg(test)]
use dvda_native::media::{Operation, OutputFormat, Request};
use serde_json::Value;
use std::path::{Path, PathBuf};
/// # Errors
/// Returns an error on cancellation, missing source/encoded files, invalid
/// sample rate, bit depth or channel count, or an unknown legacy external MLP
/// conversion policy. Decode/comparison errors, including unequal PCM, are
/// propagated from the streaming verifier.
pub fn track(
    library: &Path,
    _root: &Path,
    track: &Value,
    caller: &mut dyn Callbacks,
) -> Result<(), String> {
    if caller.cancelled() {
        return Err("成品验证已取消。".into());
    }
    let path = |key| {
        track
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| format!("Missing track {key}"))
    };
    let source = path("src")?;
    let encoded = path("mlp")?;
    if !source.is_file() || !encoded.is_file() {
        return Err("缺少源音频或编码文件，无法完成逐轨校验。".into());
    }
    let mode = track["mlp_source"].as_str().unwrap_or_default();
    let builtin = mode == "mlp-batch";
    let lpcm = mode == "lpcm";
    let rate = track["sr"]
        .as_i64()
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v > 1000)
        .ok_or("Invalid verification sample rate")?;
    let bits = track["bits"]
        .as_i64()
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| matches!(v, 16 | 20 | 24))
        .ok_or("Invalid verification bit depth")?;
    let channels = track["ch"]
        .as_i64()
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| (1..=6).contains(v))
        .ok_or("Invalid verification channel count")?;
    if !builtin
        && !lpcm
        && (track["ext_resampled"].as_bool() == Some(true) || !track["resample_to"].is_null())
    {
        return Err("旧外部 MLP 的转换策略未知，不能代替 PCM 一致性校验。".into());
    }
    crate::verify_audio_stream::compare(
        &crate::verify_audio_stream::CompareJob {
            library,
            source: &source,
            decoded: &encoded,
            rate,
            bits,
            channels,
            builtin,
        },
        caller,
    )
}

#[cfg(test)]
pub(crate) fn convert(
    library: &Path,
    input: &Path,
    output: &Path,
    rate: u32,
    bits: u32,
    output_format: OutputFormat,
    caller: &mut dyn Callbacks,
) -> Result<(), String> {
    let outcome = media::execute(
        media::Job {
            library: library.to_owned(),
            replace: false,
            timeout_millis: None,
            request: Request {
                operation: Operation::Audio,
                input: input.to_string_lossy().into_owned(),
                output: Some(output.to_string_lossy().into_owned()),
                rate,
                bits,
                output_format,
                soxr: false,
                dither: false,
                compression: 8,
                cover: false,
                tags: Vec::new(),
            },
        },
        caller,
    );
    if let Some(failure) = outcome.failure {
        return Err(failure.message);
    }
    if outcome.exit_code != Some(0) {
        return Err(format!("无法解码音频进行 PCM 校验：{}", input.display()));
    }
    Ok(())
}
