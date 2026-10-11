// Parallel audio decoding implementation
// Extends the parallel module with audio validation capabilities

use super::{alac, models, rules, state};
use crate::{audio, media::Failure};
use dvda_native::media::Callbacks;
use rayon::prelude::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}},
};

/// Thread-safe callback wrapper for parallel processing
struct ParallelCallbacks<'a> {
    inner: Arc<Mutex<&'a mut dyn Callbacks>>,
    progress_counter: Arc<AtomicU64>,
    progress_total: u64,
}

impl<'a> ParallelCallbacks<'a> {
    fn new(callbacks: &'a mut dyn Callbacks, total: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(callbacks)),
            progress_counter: Arc::new(AtomicU64::new(0)),
            progress_total: total,
        }
    }

    fn check_cancel(&self) -> Result<(), Failure> {
        let cancelled = self
            .inner
            .lock()
            .map(|cb| cb.cancelled())
            .unwrap_or(false);

        if cancelled {
            Err(Failure::new("Cancelled", "操作已取消"))
        } else {
            Ok(())
        }
    }

    fn emit(&self, level: i32, message: &str) {
        if let Ok(mut cb) = self.inner.lock() {
            cb.emit(level, message);
        }
    }

    fn increment_progress(&self) {
        let current = self.progress_counter.fetch_add(1, Ordering::Relaxed) + 1;
        if let Ok(mut cb) = self.inner.lock() {
            cb.progress(current, self.progress_total);
        }
    }

    fn task_log(&self, stage: &str, current: u64, total: u64, title: &str, path: &str) {
        if let Ok(mut cb) = self.inner.lock() {
            crate::task_log::emit(&mut **cb, stage, current, total, title, path);
        }
    }
}

/// Result of parallel audio validation
#[derive(Debug)]
pub struct ValidationResult {
    pub track_index: usize,
    pub source_path: String,
    pub original_path: Option<String>,
    pub check: audio::CheckResult,
    pub patches: Vec<String>,
}

/// Parallel audio validation with ALAC repair support
pub fn validate_audio_parallel(
    group: &[&models::Track],
    job: &super::pipeline::Job,
    cached: &HashMap<String, state::CacheEntry>,
    events: &mut dyn Callbacks,
    progress_base: u64,
    progress_total: u64,
) -> Result<Vec<ValidationResult>, Failure> {
    let callbacks = ParallelCallbacks::new(events, progress_total);

    // Parallel validation
    let results: Result<Vec<_>, Failure> = group
        .par_iter()
        .enumerate()
        .map(|(index, track)| {
            // Check cancellation
            callbacks.check_cancel()?;

            // Log progress
            callbacks.task_log(
                "source_decode",
                progress_base + index as u64,
                progress_total,
                &track.title,
                &track.path,
            );

            let expected = rules::expected_samples(track);

            // Check cache first
            if let Some(entry) = cached
                .get(&track.path)
                .filter(|entry| state::reusable(entry, track, expected))
            {
                // Use cached result
                if let Some(file) = &entry.validation.repaired_file {
                    let source = file["Path"].as_str().unwrap().to_string();
                    let patches = entry.validation.patches.clone().unwrap_or_default();

                    callbacks.emit(
                        1,
                        &format!(
                            "  ++ [缓存复用] {}: 已校验的 ALAC 修复结果（{} 处）",
                            track.title,
                            patches.len()
                        ),
                    );

                    callbacks.increment_progress();

                    return Ok(ValidationResult {
                        track_index: index,
                        source_path: source,
                        original_path: Some(track.path.clone()),
                        check: audio::CheckResult {
                            error_count: 0,
                            samples: expected,
                            errors: Vec::new(),
                        },
                        patches,
                    });
                }
            }

            // Perform actual validation
            let audio_job = audio::Job {
                library: job.library.clone(),
                input: track.path.clone(),
                operation: audio::Operation::CheckDecode,
                resample_to: track.resample_to,
            };

            // Create thread-local quiet callback
            let mut quiet = QuietCallbacks {
                inner: &callbacks,
            };

            let mut check = audio::check_decode(&audio_job, &mut quiet)?;
            let mut source = track.path.clone();
            let mut original = None;
            let mut patches = Vec::new();

            // ALAC repair if needed
            if (check.error_count > 0
                || (check.samples.is_some() && expected.is_some() && check.samples != expected))
            {
                if let Some(repair) = alac::try_repair(
                    &job.library,
                    &source,
                    &job.alac_fix_directory,
                    &mut quiet,
                )? {
                    let repaired_check = audio::check_decode(
                        &audio::Job {
                            library: job.library.clone(),
                            input: repair.output_path.clone(),
                            operation: audio::Operation::CheckDecode,
                            resample_to: track.resample_to,
                        },
                        &mut quiet,
                    )?;

                    if repaired_check.error_count == 0
                        && repaired_check.samples.is_some()
                        && (expected.is_none() || repaired_check.samples == expected)
                    {
                        callbacks.emit(
                            1,
                            &format!(
                                "  ++ [已修复] {}: ALAC END 标记 {} 处，解码采样数已达标",
                                track.title,
                                repair.patches.len()
                            ),
                        );

                        original = Some(source);
                        source = repair.output_path;
                        check = repaired_check;
                        patches = repair.patches;
                    }
                }
            }

            callbacks.increment_progress();

            Ok(ValidationResult {
                track_index: index,
                source_path: source,
                original_path: original,
                check,
                patches,
            })
        })
        .collect();

    results
}

/// Quiet callback wrapper for thread-local operations
struct QuietCallbacks<'a> {
    inner: &'a ParallelCallbacks<'a>,
}

impl Callbacks for QuietCallbacks<'_> {
    fn emit(&mut self, _level: i32, _message: &str) {
        // Suppress individual messages during parallel processing
    }

    fn cancelled(&mut self) -> bool {
        self.inner.check_cancel().is_err()
    }

    fn progress(&mut self, _completed: u64, _total: u64) {
        // Progress handled by parent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_validation() {
        // Test will be implemented with actual audio files
        // For now, verify compilation
    }
}
