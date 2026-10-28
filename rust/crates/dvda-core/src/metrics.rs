//! Diagnostic-only workflow metrics; never part of encoding or acceptance evidence.
use dvda_native::media::Callbacks;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub const PREFIX: &str = "[METRIC] ";
const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const SCAN_LIMIT: usize = 16_384;

#[derive(Default)]
struct Counters {
    active: AtomicU64,
    peak: AtomicU64,
    completed: AtomicU64,
    input_bytes: AtomicU64,
    missing_sizes: AtomicU64,
    stream_bytes: AtomicU64,
}

#[derive(Clone, Default)]
pub(crate) struct Work(Arc<Counters>);

pub(crate) struct Active(Arc<Counters>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::Relaxed);
    }
}
impl Work {
    pub fn enter(&self) -> Active {
        let active = self.0.active.fetch_add(1, Ordering::Relaxed) + 1;
        self.0.peak.fetch_max(active, Ordering::Relaxed);
        Active(Arc::clone(&self.0))
    }
    pub fn read_stream(&self, bytes: usize) {
        self.0
            .stream_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }
    pub fn complete(&self, bytes: Option<u64>) {
        self.0.completed.fetch_add(1, Ordering::Relaxed);
        if let Some(bytes) = bytes {
            self.0.input_bytes.fetch_add(bytes, Ordering::Relaxed);
        } else {
            self.0.missing_sizes.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub(crate) fn file_bytes(path: impl AsRef<Path>) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len())
}

#[derive(Default, Serialize)]
struct Space {
    sampled_peak_bytes: u64,
    samples: u64,
    incomplete_samples: u64,
}

fn roots(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut paths: Vec<_> = paths
        .into_iter()
        .filter(|p| !p.as_os_str().is_empty())
        .filter_map(|p| std::path::absolute(p).ok())
        .collect();
    paths.sort_by_key(|p| p.components().count());
    let mut result: Vec<PathBuf> = Vec::new();
    for path in paths {
        let components: Vec<_> = path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect();
        if !result.iter().any(|root| {
            let root: Vec<_> = root
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
                .collect();
            components.starts_with(&root)
        }) {
            result.push(path);
        }
    }
    result
}

fn sample(paths: &[PathBuf], space: &mut Space) {
    use std::os::windows::fs::MetadataExt;
    let mut pending = paths.to_vec();
    let mut bytes = 0u64;
    let mut visited = 0;
    let mut incomplete = false;
    while let Some(path) = pending.pop() {
        visited += 1;
        if visited > SCAN_LIMIT {
            incomplete = true;
            break;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                incomplete = true;
                continue;
            }
        };
        // Do not follow symlinks or Windows junctions outside the task directories.
        if metadata.file_attributes() & 0x400 != 0 {
            incomplete = true;
            continue;
        }
        if metadata.is_file() {
            bytes = bytes.saturating_add(metadata.len());
        } else if metadata.is_dir() {
            match fs::read_dir(path) {
                Ok(entries) => {
                    for entry in entries {
                        if visited + pending.len() >= SCAN_LIMIT {
                            incomplete = true;
                            break;
                        }
                        match entry {
                            Ok(entry) => pending.push(entry.path()),
                            Err(_) => incomplete = true,
                        }
                    }
                }
                Err(_) => incomplete = true,
            }
        }
    }
    space.sampled_peak_bytes = space.sampled_peak_bytes.max(bytes);
    space.samples += 1;
    space.incomplete_samples += u64::from(incomplete);
}

struct Sampler {
    stop: Arc<(Mutex<bool>, Condvar)>,
    thread: Option<std::thread::JoinHandle<Space>>,
}
impl Sampler {
    fn start(paths: Vec<PathBuf>) -> Self {
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let signal = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("workflow-metrics".into())
            .spawn(move || {
                let mut space = Space::default();
                loop {
                    sample(&paths, &mut space);
                    let (lock, wake) = &*signal;
                    let stopped = lock.lock().unwrap_or_else(|e| e.into_inner());
                    if *stopped {
                        break;
                    }
                    let (stopped, _) = wake
                        .wait_timeout_while(stopped, SAMPLE_INTERVAL, |stop| !*stop)
                        .unwrap_or_else(|e| e.into_inner());
                    if *stopped {
                        break;
                    }
                }
                sample(&paths, &mut space);
                space
            })
            .ok();
        Self { stop, thread }
    }
    fn finish(&mut self) -> Option<Space> {
        let (lock, wake) = &*self.stop;
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
        wake.notify_all();
        self.thread.take().and_then(|thread| thread.join().ok())
    }
}
impl Drop for Sampler {
    fn drop(&mut self) {
        self.finish();
    }
}

pub(crate) fn measure<T>(
    stage: &str,
    temporary_roots: Vec<PathBuf>,
    worker_limit: usize,
    caller: &mut dyn Callbacks,
    operation: impl FnOnce(&Work, &mut dyn Callbacks) -> T,
    succeeded: impl FnOnce(&T) -> bool,
) -> T {
    let roots = roots(temporary_roots);
    caller.emit(
        1,
        &format!(
            "{PREFIX}{}",
            serde_json::json!({
                "version": 1, "event": "start", "stage": stage, "worker_limit": worker_limit,
                "temporary_roots": roots,
            })
        ),
    );
    let mut sampler = (!roots.is_empty()).then(|| Sampler::start(roots));
    let work = Work::default();
    let start = Instant::now();
    let result = operation(&work, caller);
    let elapsed = start.elapsed().as_millis();
    let space = sampler.as_mut().and_then(Sampler::finish);
    caller.emit(
        1,
        &format!(
            "{PREFIX}{}",
            serde_json::json!({
                "version": 1, "event": "finish", "stage": stage, "succeeded": succeeded(&result),
                "elapsed_ms": elapsed, "worker_limit": worker_limit,
                "active_workers_peak": work.0.peak.load(Ordering::Relaxed),
                "completed_items": work.0.completed.load(Ordering::Relaxed),
                "completed_input_file_bytes": work.0.input_bytes.load(Ordering::Relaxed),
                "input_size_unavailable": work.0.missing_sizes.load(Ordering::Relaxed),
                "stream_bytes_read": work.0.stream_bytes.load(Ordering::Relaxed),
                "temporary_space": space,
            })
        ),
    );
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activity_and_logical_bytes_are_counted_only_on_completion() {
        let work = Work::default();
        let first = work.enter();
        {
            let _second = work.enter();
            work.complete(Some(123));
        }
        drop(first);
        work.complete(None);
        assert_eq!(work.0.active.load(Ordering::Relaxed), 0);
        assert_eq!(work.0.peak.load(Ordering::Relaxed), 2);
        assert_eq!(work.0.completed.load(Ordering::Relaxed), 2);
        assert_eq!(work.0.input_bytes.load(Ordering::Relaxed), 123);
        assert_eq!(work.0.missing_sizes.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn nested_roots_are_not_double_counted() {
        let base = std::env::temp_dir().join("dvda-metric-root");
        assert_eq!(
            roots(vec![base.join("child"), base.clone(), base.clone()]),
            vec![base]
        );
    }
    #[test]
    fn finish_event_preserves_failures_and_progress() {
        #[derive(Default)]
        struct Capture {
            lines: Vec<String>,
            progress: u64,
        }
        impl Callbacks for Capture {
            fn emit(&mut self, _: i32, text: &str) {
                self.lines.push(text.into());
            }
            fn cancelled(&mut self) -> bool {
                false
            }
            fn progress(&mut self, completed: u64, _: u64) {
                self.progress = completed;
            }
        }
        let mut capture = Capture::default();
        let result: Result<(), &str> = measure(
            "test",
            vec![],
            2,
            &mut capture,
            |work, caller| {
                let _active = work.enter();
                work.complete(Some(42));
                work.read_stream(17);
                work.read_stream(25);
                caller.progress(7, 10);
                Err("original failure")
            },
            Result::is_ok,
        );
        assert_eq!(result, Err("original failure"));
        assert_eq!(capture.progress, 7);
        let event: serde_json::Value =
            serde_json::from_str(capture.lines[1].strip_prefix(PREFIX).unwrap()).unwrap();
        assert_eq!(event["succeeded"], false);
        assert_eq!(event["completed_input_file_bytes"], 42);
        assert_eq!(event["stream_bytes_read"], 42);
        assert_eq!(event["active_workers_peak"], 1);
        assert!(event["temporary_space"].is_null());
    }
    #[test]
    fn space_samples_include_existing_files_and_handle_deleted_roots() {
        let root = crate::media::temporary_path(&std::env::temp_dir().join("dvda-metric"));
        fs::create_dir_all(root.join("child")).unwrap();
        fs::write(root.join("a"), [0; 7]).unwrap();
        fs::write(root.join("child").join("b"), [0; 11]).unwrap();
        let mut space = Space::default();
        sample(std::slice::from_ref(&root), &mut space);
        assert_eq!(space.sampled_peak_bytes, 18);
        assert_eq!(space.incomplete_samples, 0);
        fs::remove_dir_all(&root).unwrap();
        sample(&[root], &mut space);
        assert_eq!(space.sampled_peak_bytes, 18);
        assert_eq!(space.samples, 2);
    }
}
