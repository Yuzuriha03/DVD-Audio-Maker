// Audio decoding parallel implementation
// Provides thread-safe parallel audio validation and decoding

use super::parallel_enhanced::{BatchProcessor, ParallelConfig};
use crate::{audio, media::Failure};
use dvda_native::media::Callbacks;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

/// Enable parallel audio decoding for a group of tracks
///
/// This is a configuration flag that can be integrated into pipeline.rs
/// to enable parallel processing of audio validation
pub const ENABLE_PARALLEL_AUDIO_DECODE: bool = true;

/// Minimum tracks to enable parallel audio processing
pub const MIN_TRACKS_FOR_PARALLEL: usize = 4;

/// Configuration for parallel audio operations
pub fn audio_parallel_config() -> ParallelConfig {
    ParallelConfig {
        min_items_for_parallel: MIN_TRACKS_FOR_PARALLEL,
        max_threads: None, // Auto-detect
        enabled: ENABLE_PARALLEL_AUDIO_DECODE,
        chunk_size: Some(4), // Process in chunks of 4
    }
}

/// Run independent audio jobs in parallel while preserving input order.
///
/// The callback must own all state it needs for one item.  Progress and GUI
/// callbacks stay outside this function so they are updated on the caller's
/// thread after the batch completes; this keeps the media FFI boundary free of
/// shared mutable callback state.
pub fn process_audio_jobs<T, R, E, F>(
    items: Vec<T>,
    config: ParallelConfig,
    process: F,
) -> Result<Vec<R>, E>
where
    T: Send + Sync,
    R: Send,
    E: Send,
    F: Fn(&T) -> Result<R, E> + Send + Sync,
{
    BatchProcessor::new(items, config).process_with_result(process)
}

/// Inspect files with private per-worker callbacks. The caller's callbacks are
/// polled and invoked only on the caller thread, including during long decodes.
pub(crate) fn inspect_jobs<R: Send>(
    jobs: &[audio::Job],
    callbacks: &mut dyn Callbacks,
    phase: &str,
    progress_start: u64,
    progress_total: u64,
    durations: Option<&[f64]>,
    inspect: fn(&audio::Job, &mut dyn Callbacks) -> Result<R, Failure>,
) -> Result<Vec<R>, Failure> {
    enum Event<R> {
        Started(usize),
        Message(usize, i32, String),
        Finished(usize, Result<R, Failure>),
    }
    struct WorkerCallbacks<'a, R> {
        stop: &'a AtomicBool,
        index: usize,
        send: &'a mpsc::SyncSender<Event<R>>,
    }
    impl<R> Callbacks for WorkerCallbacks<'_, R> {
        fn emit(&mut self, level: i32, text: &str) {
            let _ = self
                .send
                .send(Event::Message(self.index, level, text.into()));
        }
        fn cancelled(&mut self) -> bool {
            self.stop.load(Ordering::Acquire)
        }
    }
    if jobs.is_empty() {
        return Ok(Vec::new());
    }
    let stop = AtomicBool::new(false);
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(super::parallel_enhanced::MAX_BATCH_THREADS)
        .min(jobs.len());
    std::thread::scope(|scope| {
        let (send, receive) = mpsc::sync_channel(workers * 2);
        for _ in 0..workers {
            let send = send.clone();
            let stop = &stop;
            let next = &next;
            scope.spawn(move || {
                let mut callbacks = WorkerCallbacks {
                    stop,
                    index: 0,
                    send: &send,
                };
                while !stop.load(Ordering::Acquire) {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(index) else {
                        break;
                    };
                    if send.send(Event::Started(index)).is_err() {
                        break;
                    }
                    callbacks.index = index;
                    let result = inspect(job, &mut callbacks);
                    if send.send(Event::Finished(index, result)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(send);
        let mut results: Vec<_> = (0..jobs.len()).map(|_| None).collect();
        let mut completed = 0;
        let mut portions = vec![0u64; jobs.len()];
        let mut completed_units = 0u64;
        loop {
            if callbacks.cancelled() {
                stop.store(true, Ordering::Release);
            }
            match receive.recv_timeout(Duration::from_millis(50)) {
                Ok(Event::Started(index)) => {
                    let path = &jobs[index].input;
                    crate::task_log::emit(
                        callbacks,
                        phase,
                        completed,
                        jobs.len() as u64,
                        path.rsplit(['/', '\\']).next().unwrap_or(path),
                        path,
                    );
                }
                Ok(Event::Finished(index, result)) => {
                    results[index] = Some(result);
                    completed += 1;
                    completed_units += 1000 - portions[index];
                    portions[index] = 1000;
                    callbacks.progress(
                        progress_start * 1000 + completed_units,
                        progress_total * 1000,
                    );
                }
                Ok(Event::Message(index, level, text)) => {
                    if let Some(duration) = durations.and_then(|values| values.get(index)).copied()
                        && duration.is_finite()
                        && duration > 0.0
                        && let Some(micros) = text
                            .trim()
                            .strip_prefix("out_time_us=")
                            .and_then(|value| value.parse::<u64>().ok())
                    {
                        let portion = (micros as f64 / (duration * 1_000_000.0) * 1000.0)
                            .clamp(0.0, 999.0) as u64;
                        if portion > portions[index] {
                            completed_units += portion - portions[index];
                            portions[index] = portion;
                            callbacks.progress(
                                progress_start * 1000 + completed_units,
                                progress_total * 1000,
                            );
                        }
                    }
                    callbacks.emit(level, &text);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        if stop.load(Ordering::Acquire) {
            return Err(Failure::new("Cancelled", "Audio inspection cancelled"));
        }
        results
            .into_iter()
            .map(|result| {
                result.unwrap_or_else(|| {
                    Err(Failure::new(
                        "InvalidOperation",
                        "Audio worker returned no result",
                    ))
                })
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_config() {
        let config = audio_parallel_config();
        assert!(config.enabled);
        assert_eq!(config.min_items_for_parallel, 4);
    }

    #[test]
    fn processes_audio_jobs_in_input_order() {
        let result = process_audio_jobs(vec![1_u32, 2, 3, 4], audio_parallel_config(), |value| {
            Ok::<_, ()>(value * 2)
        })
        .unwrap();
        assert_eq!(result, vec![2, 4, 6, 8]);
    }

    fn fixture_jobs() -> Vec<audio::Job> {
        (0..8)
            .map(|index| audio::Job {
                library: Default::default(),
                input: index.to_string(),
                operation: audio::Operation::Metadata,
                resample_to: None,
            })
            .collect()
    }

    #[test]
    fn inspection_preserves_order_and_runs_callbacks_on_caller() {
        struct Events(std::thread::ThreadId, u64);
        impl Callbacks for Events {
            fn emit(&mut self, _: i32, _: &str) {
                assert_eq!(std::thread::current().id(), self.0);
            }
            fn cancelled(&mut self) -> bool {
                assert_eq!(std::thread::current().id(), self.0);
                false
            }
            fn progress(&mut self, value: u64, _: u64) {
                assert_eq!(std::thread::current().id(), self.0);
                assert!(value >= self.1);
                self.1 = value;
            }
        }
        let mut events = Events(std::thread::current().id(), 0);
        let result = inspect_jobs(
            &fixture_jobs(),
            &mut events,
            "source_metadata",
            0,
            8,
            None,
            |job, _| Ok(job.input.parse::<u32>().unwrap()),
        )
        .unwrap();
        assert_eq!(result, (0..8).collect::<Vec<_>>());
        assert_eq!(events.1, 8000);
    }

    #[test]
    fn decode_progress_advances_inside_a_track_and_waits_for_completion() {
        #[derive(Default)]
        struct Events(Vec<(u64, u64)>);
        impl Callbacks for Events {
            fn emit(&mut self, _: i32, _: &str) {}
            fn cancelled(&mut self) -> bool {
                false
            }
            fn progress(&mut self, done: u64, total: u64) {
                self.0.push((done, total));
            }
        }
        let mut events = Events::default();
        inspect_jobs(
            &fixture_jobs()[..1],
            &mut events,
            "source_decode",
            2,
            3,
            Some(&[10.0]),
            |_, callbacks| {
                callbacks.emit(3, "out_time_us=5000000");
                callbacks.emit(3, "out_time_us=1000000");
                callbacks.emit(3, "out_time_us=10000000");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(events.0, [(2500, 3000), (2999, 3000), (3000, 3000)]);
    }

    #[test]
    fn inspection_cancellation_is_forwarded_to_workers() {
        struct Events;
        impl Callbacks for Events {
            fn emit(&mut self, _: i32, _: &str) {}
            fn cancelled(&mut self) -> bool {
                true
            }
        }
        let result = inspect_jobs(
            &fixture_jobs(),
            &mut Events,
            "source_decode",
            0,
            8,
            None,
            |_, callbacks| {
                while !callbacks.cancelled() {
                    std::thread::yield_now();
                }
                Err::<u32, _>(Failure::new("Cancelled", "cancelled"))
            },
        );
        assert_eq!(result.unwrap_err().kind, "Cancelled");
    }
}
