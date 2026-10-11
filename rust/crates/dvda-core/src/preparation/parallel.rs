// Parallel processing utilities for dvda-core
// This module provides helper functions for estimating parallel performance

/// Estimate performance improvement from parallelization using Amdahl's Law
///
/// # Arguments
/// * `num_files` - Number of files to process
/// * `num_cores` - Number of CPU cores available
///
/// # Returns
/// Expected speedup factor (e.g., 3.2 means 3.2x faster)
pub fn estimate_speedup(num_files: usize, num_cores: usize) -> f64 {
    // Amdahl's Law: speedup = 1 / ((1 - p) + p / s)
    // where p = parallelizable portion, s = number of cores

    let parallelizable = 0.85; // 85% of metadata reading is parallelizable
    let overhead = 0.05; // 5% overhead for synchronization

    let theoretical_speedup = 1.0 / ((1.0 - parallelizable) + parallelizable / num_cores as f64);
    let practical_speedup = theoretical_speedup * (1.0 - overhead);

    // For small datasets, speedup is limited
    if num_files < num_cores * 2 {
        practical_speedup * (num_files as f64 / (num_cores * 2) as f64)
    } else {
        practical_speedup
    }
}

/// Check if parallel processing should be used based on file count
pub fn should_use_parallel(num_files: usize, min_threshold: usize) -> bool {
    num_files >= min_threshold && rayon::current_num_threads() > 1
}

/// Get optimal chunk size for parallel processing
pub fn optimal_chunk_size(total_items: usize, num_threads: usize) -> usize {
    let base_chunk = (total_items + num_threads - 1) / num_threads;
    base_chunk.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speedup_estimation() {
        // 4 cores, 100 files
        let speedup = estimate_speedup(100, 4);
        assert!(speedup >= 2.5 && speedup <= 3.5,
            "Expected speedup 2.5-3.5x, got {}", speedup);

        // Small dataset
        let speedup = estimate_speedup(4, 4);
        assert!(speedup >= 1.0 && speedup <= 2.0,
            "Small dataset speedup should be limited");

        // Single core (no speedup)
        let speedup = estimate_speedup(100, 1);
        assert!((speedup - 1.0).abs() < 0.1,
            "Single core should have ~1x speedup");
    }

    #[test]
    fn test_should_use_parallel() {
        assert!(should_use_parallel(10, 4));
        assert!(!should_use_parallel(2, 4));
    }

    #[test]
    fn test_optimal_chunk_size() {
        assert_eq!(optimal_chunk_size(100, 4), 25);
        assert_eq!(optimal_chunk_size(10, 4), 3);
        assert_eq!(optimal_chunk_size(1, 4), 1);
    }
}
