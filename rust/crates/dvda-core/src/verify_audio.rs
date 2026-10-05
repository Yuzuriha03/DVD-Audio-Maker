//! Reconstruct the encoding input and compare full decoded PCM, without changing
//! either the source or the encoded file. Keep conversion policy identical to batch.
use crate::{media, pcm};
use dvda_native::{
    NativeFormats,
    media::{Callbacks, Operation, OutputFormat, Request},
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Workspace(PathBuf);
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn track(
    library: &Path,
    root: &Path,
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
    let builtin = mode == "surcode-batch";
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
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let directory = media::temporary_path(&root.join("track"));
    fs::create_dir(&directory).map_err(|e| e.to_string())?;
    let work = Workspace(directory);
    let expected = if builtin || lpcm {
        let converted = work.0.join("converted.wav");
        convert(
            library,
            &source,
            &converted,
            rate,
            bits,
            OutputFormat::Wave,
            caller,
        )?;
        let normalized = work.0.join("input.wav");
        pcm::normalize(
            pcm::Job {
                source: converted.to_string_lossy().into_owned(),
                destination: normalized.clone(),
                rate: rate as i32,
                bits: bits as i32,
            },
            caller,
        )
        .map_err(|e| e.message)?;
        normalized
    } else {
        source
    };
    let source_raw = work.0.join("source.raw");
    let decoded_raw = work.0.join("decoded.raw");
    convert(
        library,
        &expected,
        &source_raw,
        0,
        24,
        OutputFormat::S24,
        caller,
    )?;
    convert(
        library,
        &encoded,
        &decoded_raw,
        0,
        24,
        OutputFormat::S24,
        caller,
    )?;
    if caller.cancelled() {
        return Err("成品验证已取消。".into());
    }
    let comparison = NativeFormats::load()?.compare_pcm(
        &source_raw,
        &decoded_raw,
        3 * channels,
        if builtin { (rate - 1) / 1000 } else { 0 },
    )?;
    if caller.cancelled() {
        return Err("成品验证已取消。".into());
    }
    if !comparison.matches {
        return Err(format!(
            "PCM 不一致：原因 {}，偏移 {}，源 {} 字节，解码 {} 字节。",
            comparison.reason_code,
            comparison.first_mismatch_offset,
            comparison.source_bytes,
            comparison.decoded_bytes
        ));
    }
    Ok(())
}

fn convert(
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
