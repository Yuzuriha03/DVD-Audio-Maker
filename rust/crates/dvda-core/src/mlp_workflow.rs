//! End-to-end MLP acquisition owned by Rust.
//!
//! The low-level encoder is still the project-built native codec. This module
//! owns the surrounding state machine: cache evidence, staging, validation,
//! source-change detection and track metadata updates.
use crate::{audio, batch, cache, identity};
use dvda_native::{MlpInspection, NativeFormats, media::Callbacks};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
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
    #[serde(default, skip_deserializing)]
    pub jobs: i32, // Deprecated compatibility field; production ignores explicit worker counts.
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
    base_completed: usize,
    completed_tracks: usize,
    track_progress: Vec<u8>,
    total: usize,
}

impl Forward<'_> {
    fn track_progress(&mut self, text: &str) {
        let Ok(event) = serde_json::from_str::<Value>(text) else {
            return;
        };
        let kind = event["Kind"].as_str().unwrap_or_default();
        let track = event["Track"]
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < self.track_progress.len())
            .or_else(|| {
                (kind == "Encoded")
                    .then(|| {
                        self.track_progress
                            .iter()
                            .position(|&percent| percent < 100)
                    })
                    .flatten()
            });
        if let Some(track) = track {
            let percent = if kind == "Encoded" {
                100
            } else if kind == "PcmProgress" {
                event["Value"].as_u64().unwrap_or(0).min(100) as u8
            } else {
                return;
            };
            let previous = self.track_progress[track];
            self.track_progress[track] = previous.max(percent);
            if kind == "Encoded" && previous < 100 {
                self.completed_tracks += 1;
                self.caller.emit(
                    1,
                    &format!("[MLP-PROGRESS] {}/{}", self.completed_tracks, self.total),
                );
            }
        }
        let done_hundredths = self.base_completed as u64 * 100
            + self
                .track_progress
                .iter()
                .map(|&percent| u64::from(percent))
                .sum::<u64>();
        self.caller
            .progress(done_hundredths, self.total.max(1) as u64 * 100);
    }
}

impl Callbacks for Forward<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        self.track_progress(text);
        self.caller.emit(stream, text);
    }

    fn cancelled(&mut self) -> bool {
        self.caller.cancelled()
    }

    fn progress(&mut self, completed: u64, total: u64) {
        self.caller.progress(completed, total);
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
    let mut diagnostics = Vec::new();
    caller.progress(0, 100);
    let (pending, hits) = {
        let mut progress = dvda_native::media::ProgressScope::new(caller, 0, 10);
        check_tracks(
            &job,
            &mut cache_entries,
            &mut tracks,
            &mut diagnostics,
            &mut progress,
        )?
    };

    let mut rebuilt = 0;
    if !pending.is_empty() {
        caller.emit(1, &format!("[MLP-PROGRESS] {hits}/{}", job.tracks.len()));
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
                jobs: 0,
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
            let mut encode_progress = dvda_native::media::ProgressScope::new(caller, 10, 85);
            let mut forward = Forward {
                caller: &mut encode_progress,
                base_completed: hits as usize,
                completed_tracks: hits as usize,
                track_progress: vec![0; pending.len()],
                total: job.tracks.len(),
            };
            let outcome = batch::execute(batch_job, &mut forward);
            drop(forward);
            if let Some(failure) = outcome.failure {
                return Err(failure.message);
            }
            let mut finalize_progress = dvda_native::media::ProgressScope::new(caller, 85, 100);
            finalize_progress.emit(1, &format!("[MLP-FINALIZE] start {}", pending.len()));
            for (work_index, item) in pending.iter().enumerate() {
                if finalize_progress.cancelled() {
                    return Err("MLP 编码已被取消。".into());
                }
                let display_name = text(&job.tracks[item.index], "Title").unwrap_or("Track");
                finalize_progress.emit(
                    1,
                    &format!(
                        "[MLP-FINALIZE] progress {}/{} {}",
                        hits as usize + work_index + 1,
                        job.tracks.len(),
                        display_name
                    ),
                );
                let staged = stage.join(format!("__surcode_{:04}.mlp", work_index + 1));
                let inspection = native.inspect_file(&staged).ok().filter(valid_inspection);
                let Some(inspection) = inspection else {
                    diagnostics.push(error(
                        "MLP_OUTPUT_INVALID",
                        format!("MLP 编码结果无效: {}", item.source),
                    ));
                    continue;
                };
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
                let updated = update_track(
                    &job.tracks[item.index],
                    &item.destination,
                    &inspection,
                    None,
                    &job.media_library,
                    &mut finalize_progress,
                )?;
                cache::record_mlp(
                    &mut cache_entries,
                    &item.destination,
                    json!({
                        "source": item.source_identity.clone(),
                        "output": output_identity,
                        "encoder": job.encoder_identity,
                        "bits": job.bits,
                        "resample_to": job.sample_rate,
                        "max_interval": REQUIRED_MAJOR_SYNC_INTERVAL,
                        "parameters_version": 1,
                        "source_parameters": updated.source_parameters,
                        "output_parameters": updated.output_parameters
                    }),
                );
                tracks[item.index] = updated.track;
                rebuilt += 1;
                finalize_progress.progress((work_index + 1) as u64, pending.len() as u64);
            }
            finalize_progress.emit(1, &format!("[MLP-FINALIZE] complete {}", pending.len()));
            Ok(())
        })();
        let _ = fs::remove_dir_all(&temp);
        let _ = fs::remove_dir_all(&stage);
        result?;
    } else {
        caller.progress(100, 100);
    }
    caller.progress(100, 100);
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

enum CheckedTrack {
    Cached {
        index: usize,
        destination: PathBuf,
        track: Value,
        entry: Value,
    },
    Pending(Pending),
    Missing(Value),
}

enum CheckEvent {
    Log(i32, String),
    Track(usize, Result<CheckedTrack, String>),
    Failed(String),
}

struct CheckCallbacks<'a> {
    send: SyncSender<CheckEvent>,
    stop: &'a AtomicBool,
}

impl Callbacks for CheckCallbacks<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        if self
            .send
            .send(CheckEvent::Log(stream, text.into()))
            .is_err()
        {
            self.stop.store(true, Ordering::Release);
        }
    }

    fn cancelled(&mut self) -> bool {
        self.stop.load(Ordering::Acquire)
    }
}

fn check_worker_count(tracks: usize) -> usize {
    crate::options::worker_count(tracks)
}

fn check_tracks(
    job: &Job,
    entries: &mut Map<String, Value>,
    tracks: &mut [Value],
    diagnostics: &mut Vec<Value>,
    caller: &mut dyn Callbacks,
) -> Result<(Vec<Pending>, i32), String> {
    caller.emit(1, &format!("[MLP-CHECK] start {}", job.tracks.len()));
    if caller.cancelled() {
        return Err("MLP 编码已被取消。".into());
    }
    let count = check_worker_count(job.tracks.len());
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let (send, receive) = mpsc::sync_channel(count * 2);
    // Workers only read evidence; update it on the coordinator after all
    // readers finish so completion order cannot change cache or track order.
    let mut updates = Vec::new();
    let mut issues = Vec::new();
    let mut pending = Vec::new();
    let mut hits = 0;
    let mut cancelled = false;
    let mut failure = None;
    std::thread::scope(|scope| {
        for _ in 0..count {
            let entries = &*entries;
            let next = &next;
            let stop = &stop;
            let send = send.clone();
            scope.spawn(move || {
                let mut callbacks = CheckCallbacks { send, stop };
                let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
                    let native = NativeFormats::load()?;
                    while !callbacks.cancelled() {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        if index >= job.tracks.len() {
                            break;
                        }
                        let result = check_track(job, entries, index, &native, &mut callbacks);
                        let failed = result.is_err();
                        if callbacks
                            .send
                            .send(CheckEvent::Track(index, result))
                            .is_err()
                            || failed
                        {
                            stop.store(true, Ordering::Release);
                            break;
                        }
                    }
                    Ok(())
                }))
                .unwrap_or_else(|_| Err("Panic caught in MLP check worker".into()));
                if let Err(error) = result {
                    stop.store(true, Ordering::Release);
                    let _ = callbacks.send.send(CheckEvent::Failed(error));
                }
            });
        }
        drop(send);
        let callbacks = catch_unwind(AssertUnwindSafe(|| {
            let mut completed = 0;
            loop {
                if caller.cancelled() {
                    cancelled = true;
                    stop.store(true, Ordering::Release);
                }
                match receive.recv_timeout(Duration::from_millis(20)) {
                    Ok(CheckEvent::Log(stream, text)) => caller.emit(stream, &text),
                    Ok(CheckEvent::Track(index, result)) => {
                        match result {
                            Ok(CheckedTrack::Cached {
                                index,
                                destination,
                                track,
                                entry,
                            }) => {
                                tracks[index] = track;
                                updates.push((destination, entry));
                                hits += 1;
                            }
                            Ok(CheckedTrack::Pending(track)) => pending.push(track),
                            Ok(CheckedTrack::Missing(issue)) => issues.push((index, issue)),
                            Err(error) => {
                                failure.get_or_insert(error);
                                stop.store(true, Ordering::Release);
                                continue;
                            }
                        }
                        completed += 1;
                        caller.progress(completed as u64, job.tracks.len() as u64);
                        caller.emit(
                            1,
                            &format!(
                                "[MLP-CHECK] progress {completed}/{} {}",
                                job.tracks.len(),
                                text(&job.tracks[index], "Title").unwrap_or("Track")
                            ),
                        );
                    }
                    Ok(CheckEvent::Failed(error)) => {
                        failure.get_or_insert(error);
                        stop.store(true, Ordering::Release);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => (),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        }));
        if callbacks.is_err() {
            stop.store(true, Ordering::Release);
            // Drain before joining: workers may be waiting on a bounded send.
            while receive.recv().is_ok() {}
            failure = Some("Panic caught in MLP check callback".into());
        }
    });
    if cancelled || caller.cancelled() {
        return Err("MLP 编码已被取消。".into());
    }
    if let Some(error) = failure {
        return Err(error);
    }
    for (destination, entry) in updates {
        cache::record_mlp(entries, &destination, entry);
    }
    pending.sort_by_key(|track| track.index);
    issues.sort_by_key(|(index, _)| *index);
    diagnostics.extend(issues.into_iter().map(|(_, issue)| issue));
    caller.emit(1, &format!("[MLP-CHECK] complete {hits} {}", pending.len()));
    Ok((pending, hits))
}

fn check_track(
    job: &Job,
    entries: &Map<String, Value>,
    index: usize,
    native: &NativeFormats,
    caller: &mut dyn Callbacks,
) -> Result<CheckedTrack, String> {
    let track = &job.tracks[index];
    let source = text(track, "SourcePath").ok_or("Missing source path")?;
    if !Path::new(source).is_file() {
        return Ok(CheckedTrack::Missing(error(
            "SOURCE_MISSING",
            format!("源文件不存在: {source}"),
        )));
    }
    let source_identity = identity::compute(source).map_err(|error| error.to_string())?;
    let destination = destination(source, &job.source_root, &job.output_root);
    if cache_matches(entries, &destination, &source_identity, job)
        && let Ok(inspection) = native.inspect_file(&destination)
        && valid_inspection(&inspection)
    {
        if caller.cancelled() {
            return Err("MLP 编码已被取消。".into());
        }
        let mut entry = cache::find_mlp(entries, &destination).unwrap().clone();
        let updated = update_track(
            track,
            &destination,
            &inspection,
            Some(&entry),
            &job.media_library,
            caller,
        )?;
        entry["parameters_version"] = json!(1);
        entry["source_parameters"] = json!(updated.source_parameters);
        entry["output_parameters"] = json!(updated.output_parameters);
        return Ok(CheckedTrack::Cached {
            index,
            destination,
            track: updated.track,
            entry,
        });
    }
    Ok(CheckedTrack::Pending(Pending {
        index,
        source: source.into(),
        destination,
        source_identity,
    }))
}

fn validate(job: &Job) -> Result<(), String> {
    if !crate::native_components::media_available(&job.media_library) {
        return Err("内置媒体组件缺失，请完整解压发布包。".into());
    }
    if !cfg!(feature = "rust-mlp") && !job.encoder_library.is_file() {
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

fn valid_inspection(inspection: &MlpInspection) -> bool {
    inspection.is_valid && inspection.has_end_of_stream
}

struct UpdatedTrack {
    track: Value,
    source_parameters: audio::Parameters,
    output_parameters: audio::Parameters,
}

fn cached_parameters(entry: Option<&Value>, key: &str) -> Option<audio::Parameters> {
    let entry = entry?;
    if entry["parameters_version"] != 1 {
        return None;
    }
    let parameters: audio::Parameters = serde_json::from_value(entry.get(key)?.clone()).ok()?;
    (parameters.sample_rate > 0
        && (1..=64).contains(&parameters.bits)
        && (1..=8).contains(&parameters.channels))
    .then_some(parameters)
}

fn update_track(
    track: &Value,
    destination: &Path,
    inspection: &MlpInspection,
    entry: Option<&Value>,
    media_library: &Path,
    caller: &mut dyn Callbacks,
) -> Result<UpdatedTrack, String> {
    let source = text(track, "SourcePath").ok_or("Missing source path")?;
    // Parameter evidence is only supplied after source/output identities and
    // the full MLP stream have passed validation. Old caches probe once.
    let source_parameters = match cached_parameters(entry, "source_parameters") {
        Some(parameters) => parameters,
        None => audio::read_parameters(
            &audio::Job {
                library: media_library.to_owned(),
                input: source.into(),
                operation: audio::Operation::Parameters,
                resample_to: None,
            },
            caller,
        )
        .map_err(|error| error.message)?,
    };
    let mlp_parameters = match cached_parameters(entry, "output_parameters").filter(|parameters| {
        parameters.sample_rate == inspection.sample_rate && matches!(parameters.bits, 16 | 20 | 24)
    }) {
        Some(parameters) => parameters,
        None => audio::read_parameters(
            &audio::Job {
                library: media_library.to_owned(),
                input: destination.to_string_lossy().into_owned(),
                operation: audio::Operation::Parameters,
                resample_to: None,
            },
            caller,
        )
        .map_err(|error| error.message)?,
    };
    let mut result = track
        .as_object()
        .cloned()
        .ok_or("Track must be an object")?;
    result.insert("MlpPath".into(), json!(destination.to_string_lossy()));
    result.insert("MlpSize".into(), json!(inspection.size));
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
    Ok(UpdatedTrack {
        track: Value::Object(result),
        source_parameters,
        output_parameters: mlp_parameters,
    })
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

    #[derive(Default)]
    struct CheckEvents {
        logs: Vec<String>,
        progress: Vec<(u64, u64)>,
        cancel_after: Option<usize>,
        panic_on_progress: bool,
    }

    impl Callbacks for CheckEvents {
        fn emit(&mut self, _: i32, text: &str) {
            if text.starts_with("[MLP-CHECK] progress ") {
                assert!(
                    !self.panic_on_progress,
                    "intentional check callback failure"
                );
            }
            self.logs.push(text.into());
        }
        fn cancelled(&mut self) -> bool {
            self.cancel_after
                .is_some_and(|count| self.progress.len() >= count)
        }
        fn progress(&mut self, completed: u64, total: u64) {
            self.progress.push((completed, total));
        }
    }

    fn check_job(root: &Path, tracks: Vec<Value>) -> Job {
        Job {
            media_library: root.join("unusable-media.dll"),
            encoder_library: root.join("unusable-encoder.dll"),
            source_root: root.to_string_lossy().into_owned(),
            output_root: root.join("output"),
            cache_path: root.join("cache.json"),
            temporary_directory: root.join("temporary"),
            stage_directory: root.join("stage"),
            sample_rate: 48000,
            bits: 24,
            jobs: 0,
            metadata_context: String::new(),
            encoder_identity: "encoder-v1".into(),
            tracks,
        }
    }

    #[test]
    fn incomplete_or_unknown_parameter_evidence_requires_a_fresh_probe() {
        let valid = json!({"SampleRate":44100,"Bits":16,"Channels":2});
        let mut entry = json!({"source_parameters":valid});
        assert!(cached_parameters(Some(&entry), "source_parameters").is_none());
        entry["parameters_version"] = json!(1);
        assert_eq!(
            cached_parameters(Some(&entry), "source_parameters")
                .unwrap()
                .sample_rate,
            44100
        );
        for (key, invalid) in [("Bits", 0), ("Channels", 0), ("SampleRate", 0)] {
            entry["source_parameters"] = valid.clone();
            entry["source_parameters"][key] = json!(invalid);
            assert!(cached_parameters(Some(&entry), "source_parameters").is_none());
        }
        entry["source_parameters"] = valid;
        entry["parameters_version"] = json!(2);
        assert!(cached_parameters(Some(&entry), "source_parameters").is_none());
    }

    #[test]
    fn automatic_check_workers_follow_cores_with_the_encoding_cap() {
        let detected = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1)
            .saturating_mul(2);
        assert_eq!(check_worker_count(147), detected.min(147));
        assert_eq!(check_worker_count(2), detected.min(2));
        assert_eq!(check_worker_count(1), 1);
        assert_eq!(check_worker_count(0), 1);
    }

    #[test]
    #[ignore = "requires source-built x64 media and author runtime"]
    fn parallel_checks_reuse_parameters_preserve_order_detect_middle_damage_and_cancel() {
        let root = std::env::temp_dir().join(format!("dvda-mlp-check-{}", std::process::id()));
        fs::create_dir_all(root.join("output")).unwrap();
        let native = NativeFormats::load().unwrap();
        let mut unit = vec![0; 40];
        unit[..2].copy_from_slice(&0x9014u16.to_be_bytes());
        unit[4..7].copy_from_slice(&[0xf8, 0x72, 0x6f]);
        unit[12..14].copy_from_slice(&0xb752u16.to_be_bytes());
        unit[18..20].copy_from_slice(&3200u16.to_be_bytes());
        unit[20] = 1;
        unit[32..34].copy_from_slice(&3u16.to_be_bytes());
        unit[34..38].copy_from_slice(&[0xd2, 0x34, 0xd2, 0x34]);
        let stream = native.align(&unit).unwrap().data.repeat(8000);
        let mut entries = Map::new();
        let mut tracks = Vec::new();
        for index in 0..6 {
            let source = root.join(format!("音源-{index}.flac"));
            let destination = root.join("output").join(format!("音源-{index}.mlp"));
            // These intentionally cannot be probed. Reuse must rely on
            // identity-validated parameter evidence, while still scanning MLP.
            fs::write(&source, b"not an audio file").unwrap();
            fs::write(&destination, &stream).unwrap();
            assert!(native.inspect_file(&destination).unwrap().is_valid);
            tracks.push(json!({"SourcePath":source,"Title":format!("曲目 {index}")}));
            cache::record_mlp(
                &mut entries,
                &destination,
                json!({
                    "source":identity::compute(&source.to_string_lossy()).unwrap(),
                    "output":identity::compute(&destination.to_string_lossy()).unwrap(),
                    "encoder":"encoder-v1", "bits":24, "resample_to":48000, "max_interval":8,
                    "parameters_version":1,
                    "source_parameters":{"SampleRate":44100,"Bits":16,"Channels":2},
                    "output_parameters":{"SampleRate":48000,"Bits":24,"Channels":2}
                }),
            );
        }
        let mut job = check_job(&root, tracks);
        let mut updated = job.tracks.clone();
        let mut events = CheckEvents::default();
        let mut diagnostics = Vec::new();
        let (pending, hits) = check_tracks(
            &job,
            &mut entries,
            &mut updated,
            &mut diagnostics,
            &mut events,
        )
        .unwrap();
        assert!(pending.is_empty());
        assert_eq!(hits, 6);
        assert!(diagnostics.is_empty());
        assert_eq!(
            events.progress,
            (1..=6).map(|done| (done, 6)).collect::<Vec<_>>()
        );
        for (index, track) in updated.iter().enumerate() {
            assert_eq!(track["Title"], format!("曲目 {index}"));
            assert_eq!(track["SourceSampleRate"], 44100);
            assert_eq!(track["SourceBits"], 16);
            assert_eq!(track["ResampleTo"], 48000);
            assert_eq!(track["MlpSize"], stream.len());
            assert_eq!(track["ExternalResampled"], true);
            assert_eq!(track["ExternalRebitded"], true);
        }
        for index in [1, 4] {
            let destination = root.join("output").join(format!("音源-{index}.mlp"));
            let previous = identity::compute(&destination.to_string_lossy()).unwrap();
            let time = fs::metadata(&destination).unwrap().modified().unwrap();
            let mut damaged = stream.clone();
            damaged[4000 * 40 + 30] ^= 1;
            fs::write(&destination, &damaged).unwrap();
            fs::OpenOptions::new()
                .write(true)
                .open(&destination)
                .unwrap()
                .set_modified(time)
                .unwrap();
            assert!(same_identity(
                &previous,
                &identity::compute(&destination.to_string_lossy()).unwrap()
            ));
        }
        let (pending, hits) = check_tracks(
            &job,
            &mut entries,
            &mut updated,
            &mut diagnostics,
            &mut CheckEvents::default(),
        )
        .unwrap();
        assert_eq!(hits, 4);
        assert_eq!(
            pending.iter().map(|track| track.index).collect::<Vec<_>>(),
            [1, 4]
        );
        let before = entries.clone();
        for mut events in [
            CheckEvents {
                cancel_after: Some(0),
                ..Default::default()
            },
            CheckEvents {
                cancel_after: Some(2),
                ..Default::default()
            },
            CheckEvents {
                panic_on_progress: true,
                ..Default::default()
            },
        ] {
            assert!(
                check_tracks(
                    &job,
                    &mut entries,
                    &mut updated,
                    &mut diagnostics,
                    &mut events
                )
                .is_err()
            );
            assert_eq!(entries, before);
        }
        job.tracks[0] = json!({"Title":"Missing source property"});
        let failure = check_tracks(
            &job,
            &mut entries,
            &mut updated,
            &mut diagnostics,
            &mut CheckEvents::default(),
        )
        .err()
        .unwrap();
        assert_eq!(failure, "Missing source path");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires DVDA_MLP_PREFLIGHT_* paths to a prepared, fully cached real sample set"]
    fn real_cached_mlp_preflight_benchmark_is_read_only() {
        let path = |name: &str| {
            PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("Set {name}")))
        };
        let manifest = path("DVDA_MLP_PREFLIGHT_MANIFEST");
        let source_root = path("DVDA_MLP_PREFLIGHT_ROOT");
        let output_root = path("DVDA_MLP_PREFLIGHT_OUTPUT");
        let tracks =
            crate::manifest::read(&manifest.to_string_lossy(), &output_root.to_string_lossy())
                .unwrap();
        let mut job = check_job(
            &source_root,
            serde_json::to_value(tracks)
                .unwrap()
                .as_array()
                .unwrap()
                .clone(),
        );
        job.media_library = path("DVDA_MLP_PREFLIGHT_MEDIA");
        job.encoder_library = path("DVDA_MLP_PREFLIGHT_ENCODER");
        job.output_root = output_root.clone();
        job.encoder_identity = crate::signature::encoding_identity(
            "mlp-swr-normalized-v1|rust-mlp-encoder",
            &job.media_library,
            Some(&job.encoder_library),
            "",
        )
        .unwrap();
        let mut events = CheckEvents::default();
        let mut entries = cache::load_mlp(&output_root.join("mlp-cache.json"), &mut events);
        let mut records = Vec::new();
        let mut previous = None;
        for (label, workers) in [
            ("parallel_first", 0),
            ("parallel_cached_parameters", 0),
            ("serial_cached_parameters", 1),
            ("workers_2_cached_parameters", 2),
            ("workers_4_cached_parameters", 4),
            ("workers_8_cached_parameters", 8),
            ("workers_16_cached_parameters", 16),
        ] {
            job.jobs = workers;
            let mut updated = job.tracks.clone();
            let mut events = CheckEvents::default();
            let mut diagnostics = Vec::new();
            let start = std::time::Instant::now();
            let (pending, hits) = check_tracks(
                &job,
                &mut entries,
                &mut updated,
                &mut diagnostics,
                &mut events,
            )
            .unwrap();
            let seconds = start.elapsed().as_secs_f64();
            assert!(
                pending.is_empty(),
                "Refusing to encode: {} samples missed cache",
                pending.len()
            );
            assert_eq!(hits as usize, job.tracks.len());
            assert!(diagnostics.is_empty());
            if let Some(previous) = &previous {
                assert_eq!(&updated, previous);
            }
            previous = Some(updated);
            let record = json!({"phase":label,"requested_workers":workers,"workers":check_worker_count(job.tracks.len()),"tracks":hits,"seconds":seconds,"progress_events":events.progress.len()});
            println!("{record}");
            records.push(record);
        }
        let report = path("DVDA_MLP_PREFLIGHT_REPORT");
        fs::write(report, serde_json::to_vec_pretty(&records).unwrap()).unwrap();
        // Only the requested benchmark report is written. The real source,
        // output files and cache are read without saving or launching a codec.
    }

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
    #[ignore = "requires source-built x64 media and author runtime"]
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
