//! Ordered independent batch jobs with bounded, explicitly configured workers.

use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub use super::parallel::estimate_speedup;

pub const MAX_BATCH_THREADS: usize = 16;

/// Configuration for independent jobs. Workflow callbacks remain on the caller.
#[derive(Clone, Debug)]
pub struct ParallelConfig {
    pub min_items_for_parallel: usize,
    /// Further limit the host's available workers. Zero requests one worker.
    pub max_threads: Option<usize>,
    pub enabled: bool,
    /// Jobs per scheduling chunk. Zero requests one job per chunk.
    pub chunk_size: Option<usize>,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            min_items_for_parallel: 4,
            max_threads: None,
            enabled: true,
            chunk_size: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchPlan {
    pub threads: usize,
    pub chunk_size: usize,
}

impl ParallelConfig {
    /// Build a deterministic plan from the supplied available-worker count.
    /// The process never requests more than 16 workers or useful input chunks.
    pub fn plan(&self, items: usize, available_threads: usize) -> BatchPlan {
        let limit = self.max_threads.unwrap_or(MAX_BATCH_THREADS).max(1);
        let workers = available_threads
            .max(1)
            .min(limit)
            .min(MAX_BATCH_THREADS)
            .min(items.max(1));
        let chunk_size = self
            .chunk_size
            .unwrap_or_else(|| optimal_chunk_size(items, workers))
            .clamp(1, items.max(1));
        let threads = if self.enabled && items >= self.min_items_for_parallel {
            workers.min(items.div_ceil(chunk_size).max(1))
        } else {
            1
        };
        BatchPlan {
            threads,
            chunk_size,
        }
    }
}

fn available_threads() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

pub fn should_use_parallel(items: usize, config: &ParallelConfig) -> bool {
    config.plan(items, available_threads()).threads > 1
}

pub fn optimal_chunk_size(items: usize, threads: usize) -> usize {
    super::parallel::optimal_chunk_size(items, threads).min(16)
}

/// Shared job progress; cancellation does not invoke workflow callbacks.
pub struct ProgressTracker {
    completed: AtomicU64,
    total: u64,
    cancelled: AtomicBool,
}

impl ProgressTracker {
    pub fn new(total: u64) -> Self {
        Self {
            completed: AtomicU64::new(0),
            total,
            cancelled: AtomicBool::new(false),
        }
    }
    pub fn increment(&self) -> u64 {
        self.completed.fetch_add(1, Ordering::Relaxed) + 1
    }
    pub fn current(&self) -> u64 {
        self.completed.load(Ordering::Relaxed)
    }
    pub fn total(&self) -> u64 {
        self.total
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
    pub fn progress_percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.current().min(self.total) as f64 / self.total as f64) * 100.0
        }
    }
}

pub struct BatchProcessor<T> {
    items: Vec<T>,
    config: ParallelConfig,
}

impl<T: Send + Sync> BatchProcessor<T> {
    pub fn new(items: Vec<T>, config: ParallelConfig) -> Self {
        Self { items, config }
    }

    fn map_with_plan<R: Send, F: Fn(&T) -> R + Send + Sync>(
        &self,
        plan: BatchPlan,
        f: F,
    ) -> Vec<R> {
        if plan.threads > 1
            && let Ok(pool) = rayon::ThreadPoolBuilder::new()
                .num_threads(plan.threads)
                .build()
        {
            return pool.install(|| {
                self.items
                    .par_chunks(plan.chunk_size)
                    .flat_map_iter(|chunk| chunk.iter().map(&f))
                    .collect()
            });
        }
        // A worker creation failure still permits correct sequential execution.
        self.items.iter().map(f).collect()
    }

    /// Run jobs in an independent pool and preserve their input order.
    pub fn process<R: Send, F: Fn(&T) -> R + Send + Sync>(self, f: F) -> Vec<R> {
        self.map_with_plan(self.config.plan(self.items.len(), available_threads()), f)
    }

    /// Return the earliest input error, regardless of parallel completion order.
    /// Other already scheduled independent jobs can finish after an error occurs.
    pub fn process_with_result<R, E, F>(self, f: F) -> Result<Vec<R>, E>
    where
        R: Send,
        E: Send,
        F: Fn(&T) -> Result<R, E> + Send + Sync,
    {
        self.map_with_plan(self.config.plan(self.items.len(), available_threads()), f)
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_honor_thread_chunk_and_threshold_settings() {
        let mut config = ParallelConfig {
            min_items_for_parallel: 1,
            chunk_size: Some(1),
            ..Default::default()
        };
        assert_eq!(config.plan(100, 128).threads, 16);
        assert_eq!(config.plan(3, 128).threads, 3);
        assert_eq!(config.plan(100, 1).threads, 1);
        config.max_threads = Some(2);
        assert_eq!(config.plan(100, 8).threads, 2);
        config.chunk_size = Some(4);
        assert_eq!(
            config.plan(5, 8),
            BatchPlan {
                threads: 2,
                chunk_size: 4
            }
        );
        config.chunk_size = Some(100);
        assert_eq!(
            config.plan(5, 8),
            BatchPlan {
                threads: 1,
                chunk_size: 5
            }
        );
        config.enabled = false;
        assert_eq!(config.plan(100, 8).threads, 1);
        config.enabled = true;
        config.min_items_for_parallel = 101;
        assert_eq!(config.plan(100, 8).threads, 1);
        config.max_threads = Some(0);
        config.chunk_size = Some(0);
        assert_eq!(
            config.plan(0, 0),
            BatchPlan {
                threads: 1,
                chunk_size: 1
            }
        );
    }

    #[test]
    fn independent_pool_preserves_order_and_uses_its_worker_limit() {
        let processor = BatchProcessor::new((0..64).collect::<Vec<_>>(), ParallelConfig::default());
        let result = processor.map_with_plan(
            BatchPlan {
                threads: 2,
                chunk_size: 3,
            },
            |value| {
                assert_eq!(rayon::current_num_threads(), 2);
                (value * 2, rayon::current_thread_index().unwrap())
            },
        );
        assert_eq!(
            result.iter().map(|value| value.0).collect::<Vec<_>>(),
            (0..64).map(|value| value * 2).collect::<Vec<_>>()
        );
        assert!(result.iter().all(|value| value.1 < 2));
    }

    #[test]
    fn independent_pool_reports_first_input_error_and_handles_empty_input() {
        let processor = BatchProcessor::new((0..32).collect::<Vec<_>>(), ParallelConfig::default());
        let result: Result<Vec<_>, _> = processor
            .map_with_plan(
                BatchPlan {
                    threads: 2,
                    chunk_size: 1,
                },
                |value| {
                    if *value == 3 || *value == 17 {
                        Err(*value)
                    } else {
                        Ok(*value)
                    }
                },
            )
            .into_iter()
            .collect();
        assert_eq!(result, Err(3));
        let empty = BatchProcessor::new(Vec::<u32>::new(), ParallelConfig::default());
        assert_eq!(empty.process(|value| value * 2), Vec::<u32>::new());
        let serial = BatchProcessor::new(
            vec![1, 2, 3],
            ParallelConfig {
                enabled: false,
                ..Default::default()
            },
        );
        assert_eq!(
            serial.process_with_result(|value| if *value == 2 { Err(2) } else { Ok(*value) }),
            Err(2)
        );
    }

    #[test]
    fn progress_and_cancellation_are_bounded() {
        let tracker = ProgressTracker::new(1);
        assert_eq!(tracker.increment(), 1);
        assert_eq!(tracker.current(), 1);
        tracker.increment();
        assert_eq!(tracker.progress_percent(), 100.0);
        assert_eq!(tracker.total(), 1);
        tracker.cancel();
        assert!(tracker.is_cancelled());
        assert_eq!(ProgressTracker::new(0).progress_percent(), 0.0);
    }
}
