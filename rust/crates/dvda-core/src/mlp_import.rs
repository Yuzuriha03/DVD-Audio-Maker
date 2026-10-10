//! Import and validate existing MLP files without the managed probe path.
use crate::{acquisition, audio};
use dvda_native::{NativeFormats, media::Callbacks};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    pub media_library: PathBuf,
    pub external_root: String,
    pub source_root: String,
    pub tracks: Vec<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Outcome {
    pub tracks: Vec<Value>,
    pub cache_hits: i32,
    pub cache_rebuilt: i32,
    pub diagnostics: Vec<Value>,
}

pub fn execute(job: Job, caller: &mut dyn Callbacks) -> Result<Outcome, String> {
    if !crate::native_components::media_available(&job.media_library) {
        return Err("内置媒体组件缺失，请完整解压发布包。".into());
    }
    if !Path::new(&job.external_root).is_dir() {
        return Ok(Outcome {
            tracks: job.tracks,
            cache_hits: 0,
            cache_rebuilt: 0,
            diagnostics: vec![error(
                "EXTERNAL_MLP_DIRECTORY_INVALID",
                format!("外部 MLP 目录无效: {}", job.external_root),
            )],
        });
    }
    let lookup = acquisition::dispatch("build.mlp_lookup", json!({"Root":job.external_root}))?;
    let index: HashMap<String, String> =
        serde_json::from_value(lookup["Index"].clone()).map_err(|error| error.to_string())?;
    let ambiguous: Vec<String> = serde_json::from_value(lookup["AmbiguousNames"].clone())
        .map_err(|error| error.to_string())?;
    let mut diagnostics = ambiguous
        .into_iter()
        .map(|name| {
            error(
                "EXTERNAL_MLP_BASENAME_AMBIGUOUS",
                format!("外部 MLP 文件名重复，无法安全回退匹配: {name}"),
            )
        })
        .collect::<Vec<_>>();
    let native = NativeFormats::load()?;
    let mut output = Vec::with_capacity(job.tracks.len());
    let mut channel_counts: HashMap<i32, i32> = HashMap::new();
    for track in &job.tracks {
        if caller.cancelled() {
            return Err("MLP 导入已被取消。".into());
        }
        let source = text(track, "SourcePath").ok_or("Missing source path")?;
        let hit = acquisition::dispatch(
            "build.mlp_resolve",
            json!({
                "SourcePath":source,
                "SourceRoot":job.source_root,
                "ExternalRoot":job.external_root,
                "BasenameIndex":index
            }),
        )?;
        let Some(hit) = hit.as_str() else {
            diagnostics.push(error(
                "EXTERNAL_MLP_MISSING",
                format!(
                    "找不到外部 MLP: {}",
                    text(track, "Title").unwrap_or("Track")
                ),
            ));
            output.push(track.clone());
            continue;
        };
        let path = Path::new(hit);
        if !path.is_file() || path.metadata().map(|m| m.len()).unwrap_or(0) == 0 {
            diagnostics.push(error(
                "EXTERNAL_MLP_EMPTY",
                format!("外部 MLP 是空文件: {hit}"),
            ));
            output.push(track.clone());
            continue;
        }
        let valid = native
            .inspect_file(path)
            .map(|inspection| inspection.is_valid && inspection.has_end_of_stream)
            .unwrap_or(false);
        if !valid {
            diagnostics.push(error(
                "EXTERNAL_MLP_INVALID",
                format!("外部 MLP 结构校验失败: {hit}"),
            ));
            output.push(track.clone());
            continue;
        }
        let mlp = parameters(&job.media_library, hit, caller)?;
        let source_parameters = parameters(&job.media_library, source, caller)?;
        if mlp.sample_rate <= 0 || mlp.bits <= 0 {
            diagnostics.push(error(
                "EXTERNAL_MLP_PROBE_FAILED",
                format!("无法探测外部 MLP 参数: {hit}"),
            ));
            output.push(track.clone());
            continue;
        }
        *channel_counts.entry(mlp.channels).or_default() += 1;
        let resampled =
            source_parameters.sample_rate > 0 && source_parameters.sample_rate != mlp.sample_rate;
        let rebitded = source_parameters.bits > 0 && source_parameters.bits != mlp.bits;
        let mut value = track
            .as_object()
            .cloned()
            .ok_or("Track must be an object")?;
        value.insert("SampleRate".into(), json!(mlp.sample_rate));
        value.insert("Bits".into(), json!(mlp.bits));
        value.insert("Channels".into(), json!(mlp.channels));
        value.insert(
            "SourceSampleRate".into(),
            json!(source_parameters.sample_rate),
        );
        value.insert("SourceBits".into(), json!(source_parameters.bits));
        value.insert(
            "ResampleTo".into(),
            if resampled {
                json!(mlp.sample_rate)
            } else {
                Value::Null
            },
        );
        value.insert("MlpPath".into(), json!(hit));
        value.insert(
            "MlpSize".into(),
            json!(path.metadata().map_err(|e| e.to_string())?.len()),
        );
        value.insert("MlpSource".into(), json!("external"));
        value.insert("ExternalResampled".into(), json!(resampled));
        value.insert("ExternalRebitded".into(), json!(rebitded));
        value.insert("ParametersChanged".into(), json!(resampled || rebitded));
        output.push(Value::Object(value));
    }
    if channel_counts.len() > 1 {
        diagnostics.push(error(
            "EXTERNAL_MLP_CHANNELS_MIXED",
            format!(
                "外部 MLP 声道数不一致: {}",
                channel_counts
                    .into_iter()
                    .map(|(channels, count)| format!("{channels} 声道 × {count}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    Ok(Outcome {
        tracks: output,
        cache_hits: 0,
        cache_rebuilt: 0,
        diagnostics,
    })
}

fn parameters(
    library: &Path,
    input: &str,
    caller: &mut dyn Callbacks,
) -> Result<audio::Parameters, String> {
    audio::read_parameters(
        &audio::Job {
            library: library.to_owned(),
            input: input.into(),
            operation: audio::Operation::Parameters,
            resample_to: None,
        },
        caller,
    )
    .map_err(|error| error.message)
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .or_else(|| value.get(key.to_ascii_lowercase()))
        .and_then(Value::as_str)
}

fn error(code: &str, message: String) -> Value {
    json!({"Severity":2,"Code":code,"Message":message})
}
