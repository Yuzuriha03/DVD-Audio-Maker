// Enhanced parallel processing utilities with advanced features
// Supports audio decoding, MLP encoding, and pipeline optimization

use rayon::prelude::*;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};

/// Configuration for parallel processing
#[derive(Clone)]
pub struct ParallelConfig {
    /// Minimum number of items to enable parallel processing
    pub min_items_for_parallel: usize,

    /// Maximum number of threads (None = auto detect)
    pub max_threads: Option<usize>,

    /// Whether to enable parallel processing
    pub enabled: bool,

    /// Chunk size for batch processing
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

/// Estimate performance improvement from parallelization using Amdahl's Law
pub fn estimate_speedup(num_files: usize, num_cores: usize) -> f64 {
    let parallelizable = 0.85;
    let overhead = 0.05;
    let theoretical_speedup = 1.0 / ((1.0 - parallelizable) + parallelizable / num_cores as f64);
    let practical_speedup = theoretical_speedup * (1.0 - overhead);

    if num_files < num_cores * 2 {
        practical_speedup * (num_files as f64 / (num_cores * 2) as f64)
    } else {
        practical_speedup
    }
}

/// Check if parallel processing should be used
pub fn should_use_parallel(num_files: usize, config: &ParallelConfig) -> bool {
    config.enabled
        && num_files >= config.min_items_for_parallel
        && rayon::current_num_threads() > 1
}

/// Get optimal chunk size for parallel processing
pub fn optimal_chunk_size(total_items: usize, num_threads: usize) -> usize {
    let base_chunk = (total_items + num_threads - 1) / num_threads;
    base_chunk.max(1).min(16) // Cap at 16 for better load balancing
}

/// Thread-safe progress tracker
pub struct ProgressTracker {
    completed: AtomicU64,
    total: u64,
    cancelled: Arc<AtomicBool>,
}

impl ProgressTracker {
    pub fn new(total: u64) -> Self {
        Self {
            completed: AtomicU64::new(0),
            total,
            cancelled: Arc::new(AtomicBool::new(false)),
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
            (self.current() as f64 / self.total as f64) * 100.0
        }
    }
}

/// Parallel batch processor with configurable chunking
pub struct BatchProcessor<T> {
    items: Vec<T>,
    config: ParallelConfig,
}

impl<T: Send> BatchProcessor<T> {
    pub fn new(items: Vec<T>, config: ParallelConfig) -> Self {
        Self { items, config }
    }

    /// Process items in parallel with a given function
    pub fn process<F, R>(self, f: F) -> Vec<R>
    where
        F: Fn(&T) -> R + Send + Sync,
        R: Send,
        T: Sync,
    {
        if should_use_parallel(self.items.len(), &self.config) {
            self.items.par_iter().map(f).collect()
        } else {
            self.items.iter().map(f).collect()
        }
    }

    /// Process items with error handling
    pub fn process_with_result<F, R, E>(self, f: F) -> Result<Vec<R>, E>
    where
        F: Fn(&T) -> Result<R, E> + Send + Sync,
        R: Send,
        E: Send,
        T: Sync,
    {
        if should_use_parallel(self.items.len(), &self.config) {
            self.items.par_iter().map(f).collect()
        } else {
            self.items.iter().map(f).collect()
        }
    }
}

/// Pipeline stage for streaming processing
pub enum PipelineStage<T> {
    Input(Vec<T>),
    Processing,
    Complete(Vec<T>),
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speedup_estimation() {
        let speedup = estimate_speedup(100, 8);
        assert!(speedup >= 3.0 && speedup <= 4.0);
    }

    #[test]
    fn test_parallel_config() {
        let config = ParallelConfig::default();
        assert!(should_use_parallel(10, &config));
        assert!(!should_use_parallel(2, &config));
    }

    #[test]
    fn test_progress_tracker() {
        let tracker = ProgressTracker::new(100);
        assert_eq!(tracker.increment(), 1);
        assert_eq!(tracker.current(), 1);
        assert_eq!(tracker.progress_percent(), 1.0);
    }

    #[test]
    fn test_batch_processor() {
        let items = vec![1, 2, 3, 4, 5];
        let config = ParallelConfig::default();
        let processor = BatchProcessor::new(items, config);
        let results = processor.process(|&x| x * 2);
        assert_eq!(results, vec![2, 4, 6, 8, 10]);
    }
}
