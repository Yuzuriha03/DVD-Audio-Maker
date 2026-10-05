//! End-to-end MLP acquisition owned by Rust.
//!
//! The low-level encoder is still the project-built native codec. This module
//! owns the surrounding state machine: cache evidence, staging, validation,
//! source-change detection and track metadata updates.
use crate::{audio, batch, cache, identity};
use dvda_native::{NativeFormats, media::Callbacks};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const REQUIRED_MAJOR_SYNC_INTERVAL: i64 = 8;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    pub media_library: PathBuf,
    pub encoder_library: PathBuf,
    pub source_root: String,
    pub output_root: PathBuf,
    pub cache_path: PathBuf,
    pub temporary_directory: PathBuf,
    pub stage_directory: PathBuf,
    pub sample_rate: i32,
    pub bits: i32,
    pub jobs: i32,
    pub metadata_context: String,
    pub encoder_identity: String,
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

#[derive(Debug)]
struct Pending {
    index: usize,
    source: String,
    destination: PathBuf,
    source_identity: Value,
}

struct Forward<'a> {
    caller: &'a mut dyn Callbacks,
}

impl Callbacks for Forward<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        self.caller.emit(stream, text);
    }

    fn cancelled(&mut self) -> bool {
        self.caller.cancelled()
    }
}

pub fn execute(mut job: Job, caller: &mut dyn Callbacks) -> std::result::Result<Outcome, String> {
    validate(&job)?;
    job.encoder_identity = crate::signature::encoding_identity(
        &format!("mlp-swr-normalized-v1|{}", job.encoder_identity),
        &job.media_library,
        Some(&job.encoder_library),
        &job.metadata_context,
    )?;
    let native = NativeFormats::load()?;
    fs::create_dir_all(&job.output_root).map_err(|error| error.to_string())?;
    fs::create_dir_all(&job.temporary_directory).map_err(|error| error.to_string())?;
    fs::create_dir_all(&job.stage_directory).map_err(|error| error.to_string())?;
    let mut cache_entries = cache::load_mlp(&job.cache_path, caller);
    let mut tracks = job.tracks.clone();
    let mut pending = Vec::new();
    let mut hits = 0;
    let mut diagnostics = Vec::new();
    for (index, track) in job.tracks.iter().enumerate() {
        if caller.cancelled() {
            return Err("MLP 编码已被取消。".into());
        }
        let source = text(track, "SourcePath").ok_or("Missing source path")?;
        if !Path::new(source).is_file() {
            diagnostics.push(error("SOURCE_MISSING", format!("源文件不存在: {source}")));
            continue;
        }
        let source_identity = identity::compute(source).map_err(|error| error.to_string())?;
        let destination = destination(source, &job.source_root, &job.output_root);
        if cache_matches(&cache_entries, &destination, &source_identity, &job)
            && valid_mlp(&native, &destination)
        {
            hits += 1;
            tracks[index] = update_track(track, &destination, &job.media_library, caller)?;
            continue;
        }
        pending.push(Pending {
            index,
            source: source.into(),
            destination,
            source_identity,
        });
    }

    let mut rebuilt = 0;
    if !pending.is_empty() {
        let token = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        );
        let temp = job.temporary_directory.join(&token);
        let stage = job.stage_directory.join(&token);
        fs::create_dir_all(&temp).map_err(|error| error.to_string())?;
        fs::create_dir_all(&stage).map_err(|error| error.to_string())?;
        let result = (|| -> Result<(), String> {
            let batch_job = batch::Job {
                media_library: job.media_library.clone(),
                encoder_library: job.encoder_library.clone(),
                temporary_directory: temp.clone(),
                output_directory: stage.clone(),
                sample_rate: job.sample_rate,
                bits: job.bits,
                jobs: job.jobs,
                metadata_context: job.metadata_context.clone(),
                tracks: pending
                    .iter()
                    .enumerate()
                    .map(|(work_index, item)| batch::Track {
                        source_path: item.source.clone(),
                        work_name: format!("__surcode_{:04}", work_index + 1),
                        display_name: text(&job.tracks[item.index], "Title")
                            .unwrap_or("Track")
                            .into(),
                        duration_seconds: number(&job.tracks[item.index], "Duration"),
                    })
                    .collect(),
            };
            let mut forward = Forward { caller };
            let outcome = batch::execute(batch_job, &mut forward);
            if let Some(failure) = outcome.failure {
                return Err(failure.message);
            }
            for (work_index, item) in pending.iter().enumerate() {
                if caller.cancelled() {
                    return Err("MLP 编码已被取消。".into());
                }
                let staged = stage.join(format!("__surcode_{:04}.mlp", work_index + 1));
                if !valid_mlp(&native, &staged) {
                    diagnostics.push(error(
                        "MLP_OUTPUT_INVALID",
                        format!("MLP 编码结果无效: {}", item.source),
                    ));
                    continue;
                }
                if !same_identity(
                    &item.source_identity,
                    &identity::compute(&item.source).map_err(|e| e.to_string())?,
                ) {
                    return Err(format!("编码过程中源文件发生变化: {}", item.source));
                }
                if let Some(parent) = item.destination.parent() {
                    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                publish_encoded(&staged, &item.destination)?;
                let output_identity = identity::compute(&item.destination.to_string_lossy())
                    .map_err(|error| error.to_string())?;
                cache::record_mlp(
                    &mut cache_entries,
                    &item.destination,
                    json!({
                        "source": item.source_identity.clone(),
                        "output": output_identity,
                        "encoder": job.encoder_identity,
                        "bits": job.bits,
                        "resample_to": job.sample_rate,
                        "max_interval": REQUIRED_MAJOR_SYNC_INTERVAL
                    }),
                );
                tracks[item.index] = update_track(
                    &job.tracks[item.index],
                    &item.destination,
                    &job.media_library,
                    caller,
                )?;
                rebuilt += 1;
            }
            Ok(())
        })();
        let _ = fs::remove_dir_all(&temp);
        let _ = fs::remove_dir_all(&stage);
        result?;
    }
    cache::dispatch(
        "cache.save",
        json!({"Path":job.cache_path,"Entries":cache_entries}),
    )?;
    Ok(Outcome {
        tracks,
        cache_hits: hits,
        cache_rebuilt: rebuilt,
        diagnostics,
    })
}

fn validate(job: &Job) -> Result<(), String> {
    if !job.media_library.is_file() {
        return Err("内置媒体组件缺失，请完整解压发布包。".into());
    }
    if !job.encoder_library.is_file() {
        return Err("MLP 编码器组件缺失。".into());
    }
    if !matches!(
        job.sample_rate,
        44_100 | 48_000 | 88_200 | 96_000 | 176_400 | 192_000
    ) {
        return Err(format!("不支持的目标采样率: {}", job.sample_rate));
    }
    if !matches!(job.bits, 16 | 20 | 24) {
        return Err(format!("不支持的目标位深: {}", job.bits));
    }
    if job.tracks.is_empty() {
        return Err("编码任务不包含任何音轨。".into());
    }
    Ok(())
}

fn cache_matches(
    entries: &Map<String, Value>,
    destination: &Path,
    source: &Value,
    job: &Job,
) -> bool {
    let Some(entry) = cache::find_mlp(entries, destination) else {
        return false;
    };
    let request = json!({
        "Entry": entry,
        "Source": source,
        "Encoder": job.encoder_identity,
        "Bits": job.bits,
        "ResampleTo": job.sample_rate,
        "MaxInterval": REQUIRED_MAJOR_SYNC_INTERVAL
    });
    crate::workflow::dispatch("cache.mlp_match", request).ok() == Some(json!(true))
        && entry.get("output").is_some_and(|output| {
            identity::compute(&destination.to_string_lossy())
                .ok()
                .is_some_and(|current| same_identity(output, &current))
        })
}

fn publish_encoded(staged: &Path, destination: &Path) -> Result<(), String> {
    // File.Move(overwrite:true) semantics: preserve an existing destination on
    // same-volume failure and support configured staging on another volume.
    dvda_native::files::move_file(staged, destination, true).map_err(|error| error.to_string())
}

fn valid_mlp(native: &NativeFormats, path: &Path) -> bool {
    native
        .inspect_file(path)
        .map(|inspection| inspection.is_valid && inspection.has_end_of_stream)
        .unwrap_or(false)
}

fn update_track(
    track: &Value,
    destination: &Path,
    media_library: &Path,
    caller: &mut dyn Callbacks,
) -> Result<Value, String> {
    let source = text(track, "SourcePath").ok_or("Missing source path")?;
    let source_parameters = audio::read_parameters(
        &audio::Job {
            library: media_library.to_owned(),
            input: source.into(),
            operation: audio::Operation::Parameters,
            resample_to: None,
        },
        caller,
    )
    .map_err(|error| error.message)?;
    let mlp_parameters = audio::read_parameters(
        &audio::Job {
            library: media_library.to_owned(),
            input: destination.to_string_lossy().into_owned(),
            operation: audio::Operation::Parameters,
            resample_to: None,
        },
        caller,
    )
    .map_err(|error| error.message)?;
    let mut result = track
        .as_object()
        .cloned()
        .ok_or("Track must be an object")?;
    result.insert("MlpPath".into(), json!(destination.to_string_lossy()));
    result.insert(
        "MlpSize".into(),
        json!(fs::metadata(destination).map_err(|e| e.to_string())?.len()),
    );
    result.insert("SampleRate".into(), json!(mlp_parameters.sample_rate));
    result.insert("Bits".into(), json!(mlp_parameters.bits));
    result.insert("Channels".into(), json!(mlp_parameters.channels));
    result.insert(
        "SourceSampleRate".into(),
        json!(source_parameters.sample_rate),
    );
    result.insert("SourceBits".into(), json!(source_parameters.bits));
    result.insert("MlpSource".into(), json!("surcode-batch"));
    let changed_rate = source_parameters.sample_rate > 0
        && source_parameters.sample_rate != mlp_parameters.sample_rate;
    let changed_bits = source_parameters.bits > 0 && source_parameters.bits != mlp_parameters.bits;
    result.insert(
        "ResampleTo".into(),
        if changed_rate {
            json!(mlp_parameters.sample_rate)
        } else {
            Value::Null
        },
    );
    result.insert("ExternalResampled".into(), json!(changed_rate));
    result.insert("ExternalRebitded".into(), json!(changed_bits));
    result.insert(
        "ParametersChanged".into(),
        json!(changed_rate || changed_bits),
    );
    Ok(Value::Object(result))
}

fn destination(source: &str, source_root: &str, output_root: &Path) -> PathBuf {
    let source = Path::new(source);
    let root = Path::new(source_root);
    let relative = relative_or_name(source, root);
    output_root.join(relative).with_extension("mlp")
}

fn relative_or_name(source: &Path, root: &Path) -> PathBuf {
    let source_text = source.to_string_lossy().replace('/', "\\");
    let root_text = root
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_owned();
    let prefix = format!("{root_text}\\");
    if source_text.len() > prefix.len()
        && source_text
            .get(..prefix.len())
            .is_some_and(|head| dvda_native::files::ordinal_ignore_case(head, &prefix))
    {
        return PathBuf::from(&source_text[prefix.len()..]);
    }
    source.file_name().map(PathBuf::from).unwrap_or_default()
}

fn same_identity(left: &Value, right: &Value) -> bool {
    ["Size", "LastWriteUtcTicks", "HeadHash", "TailHash"]
        .iter()
        .all(|key| left.get(*key) == right.get(*key))
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .or_else(|| value.get(key.to_ascii_lowercase()))
        .and_then(Value::as_str)
}

fn number(value: &Value, key: &str) -> f64 {
    value
        .get(key)
        .or_else(|| value.get(key.to_ascii_lowercase()))
        .and_then(Value::as_f64)
        .unwrap_or(0.)
}

fn error(code: &str, message: String) -> Value {
    json!({"Severity":2,"Code":code,"Message":message})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;

    #[test]
    fn encoded_publication_preserves_old_output_when_source_missing_or_destination_locked() {
        let root = std::env::temp_dir().join(format!("dvda-mlp-publish-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let staged = root.join("staged.mlp");
        let destination = root.join("old.mlp");
        fs::write(&destination, b"old valid output").unwrap();
        assert!(publish_encoded(&staged, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"old valid output");
        fs::write(&staged, b"new validated output").unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&destination)
            .unwrap();
        assert!(publish_encoded(&staged, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"old valid output");
        assert_eq!(fs::read(&staged).unwrap(), b"new validated output");
        drop(lock);
        publish_encoded(&staged, &destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"new validated output");
        assert!(!staged.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires DVDA_TEST_OTHER_VOLUME pointing to a writable directory on another volume"]
    fn encoded_publication_supports_configured_staging_on_another_volume() {
        let root = std::env::temp_dir().join(format!("dvda-mlp-volume-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let other = PathBuf::from(
            std::env::var_os("DVDA_TEST_OTHER_VOLUME").expect("Set other-volume test directory"),
        )
        .join(format!("dvda-mlp-volume-{}", std::process::id()));
        assert_ne!(root.components().next(), other.components().next());
        fs::create_dir(&other).unwrap();
        let staged = other.join("staged.mlp");
        let destination = root.join("existing.mlp");
        fs::write(&staged, b"validated cross-volume output").unwrap();
        fs::write(&destination, b"old output").unwrap();
        publish_encoded(&staged, &destination).unwrap();
        assert_eq!(
            fs::read(&destination).unwrap(),
            b"validated cross-volume output"
        );
        assert!(!staged.exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(other).unwrap();
    }

    #[test]
    fn destination_matching_is_unicode_safe_and_windows_case_insensitive() {
        assert_eq!(
            destination(
                "C:/MUSIC/Été/track.flac",
                "c:/music/été",
                Path::new("C:/out")
            ),
            PathBuf::from("C:/out/track.mlp")
        );
        assert_eq!(
            destination("C:/其他目录/file.flac", "C:/abcdef", Path::new("C:/out")),
            PathBuf::from("C:/out/file.mlp")
        );
    }

    #[test]
    fn mlp_cache_criteria_require_source_output_encoder_parameters_and_ignore_key_case() {
        let root = std::env::temp_dir().join(format!("dvda-mlp-cache-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.flac");
        let destination = root.join("output.MLP");
        fs::write(&source, b"source").unwrap();
        fs::write(&destination, b"output").unwrap();
        let source_identity = identity::compute(&source.to_string_lossy()).unwrap();
        let output_identity = identity::compute(&destination.to_string_lossy()).unwrap();
        let mut entries = Map::new();
        let key = destination.to_string_lossy().to_uppercase();
        let entry = json!({"source":source_identity,"output":output_identity,"encoder":"encoder-v1","bits":24,"resample_to":48000,"max_interval":8});
        entries.insert(key.clone(), entry.clone());
        let job = Job {
            media_library: PathBuf::new(),
            encoder_library: PathBuf::new(),
            source_root: String::new(),
            output_root: PathBuf::new(),
            cache_path: PathBuf::new(),
            temporary_directory: PathBuf::new(),
            stage_directory: PathBuf::new(),
            sample_rate: 48000,
            bits: 24,
            jobs: 1,
            metadata_context: String::new(),
            encoder_identity: "encoder-v1".into(),
            tracks: vec![],
        };
        assert!(cache_matches(
            &entries,
            &destination,
            &source_identity,
            &job
        ));
        for (field, replacement) in [
            ("encoder", json!("other")),
            ("bits", json!(16)),
            ("resample_to", json!(96000)),
            ("max_interval", json!(16)),
            ("source", Value::Null),
            ("output", Value::Null),
        ] {
            entries[&key] = entry.clone();
            entries[&key][field] = replacement;
            assert!(
                !cache_matches(&entries, &destination, &source_identity, &job),
                "accepted changed {field}"
            );
        }
        entries[&key] = entry.clone();
        let time = fs::metadata(&destination).unwrap().modified().unwrap();
        fs::write(&destination, b"change").unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&destination)
            .unwrap()
            .set_modified(time)
            .unwrap();
        assert!(!cache_matches(
            &entries,
            &destination,
            &source_identity,
            &job
        ));
        fs::write(&destination, b"output").unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&destination)
            .unwrap()
            .set_modified(time)
            .unwrap();
        assert!(cache_matches(
            &entries,
            &destination,
            &source_identity,
            &job
        ));
        let mut changed_source = source_identity;
        changed_source["HeadHash"] = json!("changed");
        assert!(!cache_matches(
            &entries,
            &destination,
            &changed_source,
            &job
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires source-built x64 dvda-formats.dll"]
    fn acquisition_entrypoint_reports_corrupt_cache_and_revalidates_missing_source() {
        #[derive(Default)]
        struct Events(Vec<String>);
        impl Callbacks for Events {
            fn emit(&mut self, _: i32, text: &str) {
                self.0.push(text.into());
            }
            fn cancelled(&mut self) -> bool {
                false
            }
        }
        let root = std::env::temp_dir().join(format!("dvda-mlp-warning-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let component = root.join("identity-only-component.dll");
        fs::write(
            &component,
            b"identity input: source is intentionally missing before any media/codec call",
        )
        .unwrap();
        let cache_path = root.join("cache.json");
        fs::write(&cache_path, b"{ malformed").unwrap();
        let job = Job {
            media_library: component.clone(),
            encoder_library: component,
            source_root: root.to_string_lossy().into_owned(),
            output_root: root.join("output"),
            cache_path,
            temporary_directory: root.join("temporary"),
            stage_directory: root.join("stage"),
            sample_rate: 48000,
            bits: 24,
            jobs: 1,
            metadata_context: String::new(),
            encoder_identity: "encoder-v1".into(),
            tracks: vec![json!({"SourcePath":root.join("missing.flac"),"Title":"missing"})],
        };
        let mut events = Events::default();
        let result = execute(job, &mut events).unwrap();
        assert_eq!(result.cache_hits, 0);
        assert_eq!(result.cache_rebuilt, 0);
        assert_eq!(result.diagnostics[0]["Code"], "SOURCE_MISSING");
        assert!(
            events
                .0
                .iter()
                .any(|line| line.starts_with("[警告] MLP 缓存索引无法读取"))
        );
        fs::remove_dir_all(root).unwrap();
    }
}
