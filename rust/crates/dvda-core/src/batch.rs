//! Batch MLP encoding through typed native APIs, with bounded worker/event queues.
use crate::{
    encoder,
    media::{self, Failure, Outcome, temporary_path},
    pcm,
};
use dvda_native::media::{Callbacks, Operation, OutputFormat, Request};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashSet,
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
    },
    time::Duration,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Track {
    pub source_path: String,
    pub work_name: String,
    pub display_name: String,
    pub duration_seconds: f64,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    pub media_library: PathBuf,
    pub encoder_library: PathBuf,
    pub temporary_directory: PathBuf,
    pub output_directory: PathBuf,
    pub sample_rate: i32,
    pub bits: i32,
    pub jobs: i32,
    pub metadata_context: String,
    pub tracks: Vec<Track>,
}
fn missing(message: &str) -> Failure {
    Failure {
        kind: "Io",
        code: Some(2),
        message: message.into(),
    }
}
fn validate(job: &Job) -> Result<(), Failure> {
    if !job.media_library.is_file() {
        return Err(missing("内置媒体组件缺失，请完整解压发布包。"));
    }
    if !job.metadata_context.is_empty() && !Path::new(&job.metadata_context).is_file() {
        return Err(missing("找不到显式 MLP 元数据上下文。"));
    }
    if !matches!(
        job.sample_rate,
        44100 | 48000 | 88200 | 96000 | 176400 | 192000
    ) {
        return Err(Failure::new(
            "InvalidData",
            &format!("不支持的目标采样率: {}", job.sample_rate),
        ));
    }
    if !matches!(job.bits, 16 | 20 | 24) {
        return Err(Failure::new(
            "InvalidData",
            &format!("不支持的目标位深: {}", job.bits),
        ));
    }
    if job.tracks.is_empty() {
        return Err(Failure::new("InvalidData", "编码任务不包含任何音轨。"));
    }
    let mut names = HashSet::new();
    for track in &job.tracks {
        if !Path::new(&track.source_path).is_file() {
            return Err(missing("找不到待编码音源。"));
        }
        if track.work_name.is_empty()
            || !track
                .work_name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            || !names.insert(track.work_name.to_ascii_lowercase())
        {
            return Err(Failure::new(
                "InvalidData",
                &format!("工作文件名必须是唯一的安全 ASCII 名称: {}", track.work_name),
            ));
        }
        if !track.duration_seconds.is_finite() || track.duration_seconds < 0.0 {
            return Err(Failure::new("InvalidData", "无效的音轨时长。"));
        }
        if job
            .output_directory
            .join(format!("{}.mlp", track.work_name))
            .is_file()
        {
            return Err(Failure::new("Io", "拒绝覆盖已有的暂存 MLP 文件。"));
        }
    }
    Ok(())
}
struct Folder(PathBuf);
impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Events<'a> {
    index: usize,
    track: &'a Track,
    send: SyncSender<String>,
    stop: &'a AtomicBool,
    media_phase: bool,
    previous: i32,
    errors: String,
}
impl Events<'_> {
    fn event(&self, kind: &str, value: serde_json::Value) {
        let event =
            json!({"Kind":kind,"Track":self.index,"Name":self.track.display_name,"Value":value})
                .to_string();
        if self.send.send(event).is_err() {
            self.stop.store(true, Ordering::Release);
        }
    }
    fn check(&mut self) -> Result<(), Failure> {
        if self.cancelled() {
            Err(Failure::new("Cancelled", "Batch encoding cancelled"))
        } else {
            Ok(())
        }
    }
}
impl Callbacks for Events<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        if self.media_phase {
            let text = text.trim_end_matches(['\r', '\n']);
            if stream == 2 {
                if text.starts_with("Number of samples:") {
                    return;
                }
                self.errors.push_str(text);
                self.errors.push_str("\r\n");
                self.event("PcmError", json!(text));
            } else if self.track.duration_seconds > 0.0
                && let Some(value) = text.strip_prefix("out_time_us=")
                && let Ok(microseconds) = value.trim().parse::<i64>()
            {
                let percent = (microseconds as f64 / (self.track.duration_seconds * 10000.0))
                    .clamp(0.0, 99.0) as i32;
                if percent != self.previous {
                    self.previous = percent;
                    self.event("PcmProgress", json!(percent));
                }
            }
        } else if stream == 4 {
            match serde_json::from_str(text) {
                Ok(value) => self.event("Encoded", value),
                Err(_) => self.event("Detail", json!(text)),
            }
        } else {
            self.event("Detail", json!(text));
        }
    }
    fn cancelled(&mut self) -> bool {
        self.stop.load(Ordering::Acquire)
    }
}
fn timeout(seconds: f64, factor: f64) -> Result<u64, Failure> {
    let millis = (seconds * factor + 120.0).max(300.0) * 1000.0;
    // Preserve the timer range accepted by the previous CancellationTokenSource.
    if !millis.is_finite() || millis > f64::from(u32::MAX - 1) {
        return Err(Failure::new(
            "ArgumentOutOfRange",
            "Timeout exceeds the supported timer range",
        ));
    }
    Ok(millis.trunc() as u64)
}
fn track(
    job: &Job,
    index: usize,
    send: SyncSender<String>,
    stop: &AtomicBool,
) -> Result<(), Failure> {
    let track = &job.tracks[index];
    let mut events = Events {
        index,
        track,
        send,
        stop,
        media_phase: true,
        previous: -1,
        errors: String::new(),
    };
    events.check()?;
    let path = temporary_path(&job.temporary_directory.join("track"));
    fs::create_dir(&path)?;
    let folder = Folder(path);
    let result = (|| -> Result<(), Failure> {
        events.event("Started", json!(null));
        events.event("PcmStarted", json!(null));
        let decoded = folder.0.join("decoded.wav");
        let prepared = folder.0.join("input.wav");
        let result = media::execute(
            media::Job {
                library: job.media_library.clone(),
                replace: false,
                timeout_millis: Some(timeout(track.duration_seconds, 2.0)?),
                request: Request {
                    operation: Operation::Audio,
                    rate: job.sample_rate as u32,
                    bits: job.bits as u32,
                    output_format: OutputFormat::Wave,
                    soxr: false,
                    compression: 8,
                    cover: false,
                    input: track.source_path.clone(),
                    output: Some(decoded.to_string_lossy().into_owned()),
                    tags: vec![],
                },
            },
            &mut events,
        );
        if let Some(error) = result.failure {
            return Err(error);
        }
        if result.exit_code != Some(0) {
            return Err(Failure::new(
                "InvalidOperation",
                &format!(
                    "音源转换失败：{}（FFmpeg 退出码 {}）。{}",
                    track.display_name,
                    result.exit_code.unwrap_or(-1),
                    events.errors.trim()
                ),
            ));
        }
        events.event("PcmFinished", json!(null));
        pcm::normalize(
            pcm::Job {
                source: decoded.to_string_lossy().into_owned(),
                destination: prepared.clone(),
                rate: job.sample_rate,
                bits: job.bits,
            },
            &mut events,
        )?;
        fs::remove_file(&decoded)?;
        events.media_phase = false;
        let result = encoder::execute(
            encoder::Job {
                library: job.encoder_library.clone(),
                wave: prepared.to_string_lossy().into_owned(),
                destination: job
                    .output_directory
                    .join(format!("{}.mlp", track.work_name)),
                metadata_context: job.metadata_context.clone(),
                timeout_millis: Some(timeout(track.duration_seconds, 3.0)?),
            },
            &mut events,
        );
        if let Some(error) = result.failure {
            return Err(error);
        }
        Ok(())
    })();
    fs::remove_dir_all(&folder.0)
        .map_err(Failure::from)
        .and(result)
}

fn worker_count(requested: i32, tracks: usize) -> usize {
    let detected = crate::options::default_mlp_jobs() as usize;
    let requested = if requested <= 0 {
        detected
    } else {
        requested as usize
    };
    requested.clamp(1, 16).min(tracks)
}

pub fn execute(job: Job, caller: &mut dyn Callbacks) -> Outcome {
    let result = (|| -> Result<(), Failure> {
        validate(&job)?;
        fs::create_dir_all(&job.temporary_directory)?;
        fs::create_dir_all(&job.output_directory)?;
        if caller.cancelled() {
            return Err(Failure::new("Cancelled", "Batch encoding cancelled"));
        }
        let count = worker_count(job.jobs, job.tracks.len());
        let next = AtomicUsize::new(0);
        let stop = AtomicBool::new(false);
        let external = AtomicBool::new(false);
        let first_error = Mutex::new(None);
        let (send, receive) = mpsc::sync_channel::<String>(64);
        std::thread::scope(|scope| {
            for _ in 0..count {
                let send = send.clone();
                let job = &job;
                let next = &next;
                let stop = &stop;
                let errors = &first_error;
                scope.spawn(move || {
                    let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), Failure> {
                        while !stop.load(Ordering::Acquire) {
                            let index = next.fetch_add(1, Ordering::Relaxed);
                            if index >= job.tracks.len() {
                                break;
                            }
                            track(job, index, send.clone(), stop)?;
                        }
                        Ok(())
                    }))
                    .unwrap_or_else(|_| {
                        Err(Failure::new(
                            "InvalidOperation",
                            "Panic caught in encoding worker",
                        ))
                    });
                    if let Err(error) = result {
                        let mut first = errors.lock().unwrap();
                        if first.is_none() {
                            *first = Some(error);
                        }
                        stop.store(true, Ordering::Release);
                    }
                });
            }
            drop(send);
            // Keep draining after cancellation so workers never block during cleanup.
            let callbacks = catch_unwind(AssertUnwindSafe(|| {
                loop {
                    if caller.cancelled() {
                        external.store(true, Ordering::Release);
                        stop.store(true, Ordering::Release);
                    }
                    match receive.recv_timeout(Duration::from_millis(20)) {
                        Ok(event) => caller.emit(5, &event),
                        Err(mpsc::RecvTimeoutError::Timeout) => (),
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            }));
            if callbacks.is_err() {
                stop.store(true, Ordering::Release);
                while receive.recv().is_ok() {}
                let mut first = first_error.lock().unwrap();
                *first = Some(Failure::new(
                    "InvalidOperation",
                    "Panic caught in batch callback",
                ));
            }
        });
        if external.load(Ordering::Acquire) {
            return Err(Failure::new("Cancelled", "Batch encoding cancelled"));
        }
        if let Some(error) = first_error.into_inner().unwrap() {
            return Err(error);
        }
        Ok(())
    })();
    match result {
        Ok(()) => Outcome {
            exit_code: Some(0),
            failure: None,
        },
        Err(error) => Outcome {
            exit_code: None,
            failure: Some(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::worker_count;

    #[test]
    fn automatic_workers_follow_detected_parallelism_and_caps() {
        let detected = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1)
            .clamp(1, 16);
        assert_eq!(worker_count(0, usize::MAX), detected);
        assert_eq!(worker_count(-1, 2), detected.min(2));
        assert_eq!(worker_count(1, 2), 1);
        assert_eq!(worker_count(99, 99), 16);
    }
}
