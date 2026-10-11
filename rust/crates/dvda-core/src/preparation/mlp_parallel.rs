// MLP encoding batch parallel implementation
// Enables parallel encoding of multiple MLP groups

use super::parallel_enhanced::{ParallelConfig, BatchProcessor};
use rayon::prelude::*;

/// Enable parallel MLP encoding
pub const ENABLE_PARALLEL_MLP_ENCODING: bool = true;

/// Minimum files per group to enable parallel encoding
pub const MIN_FILES_FOR_PARALLEL_MLP: usize = 2;

/// Configuration for parallel MLP encoding operations
pub fn mlp_parallel_config() -> ParallelConfig {
    ParallelConfig {
        min_items_for_parallel: MIN_FILES_FOR_PARALLEL_MLP,
        max_threads: None, // Auto-detect
        enabled: ENABLE_PARALLEL_MLP_ENCODING,
        chunk_size: Some(2), // Process 2 groups at a time
    }
}

/// Batch encode MLP groups in parallel
///
/// Example integration in mlp_workflow.rs:
///
/// ```rust
/// use rayon::prelude::*;
///
/// // Instead of sequential:
/// // for group in groups { encode_group(group)?; }
///
/// // Use parallel:
/// let results: Result<Vec<_>, _> = groups
///     .par_iter()
///     .map(|group| {
///         // Check cancel
///         if cancelled() { return Err(...); }
///
///         // Encode group
///         encode_mlp_group(group)
///     })
///     .collect();
/// ```

/// Estimate MLP encoding speedup
pub fn estimate_mlp_speedup(num_groups: usize) -> f64 {
    let num_cores = rayon::current_num_threads();

    // MLP encoding is highly parallelizable (90%)
    let parallelizable = 0.90;
    let overhead = 0.03; // Lower overhead for CPU-bound work

    let theoretical = 1.0 / ((1.0 - parallelizable) + parallelizable / num_cores as f64);
    let practical = theoretical * (1.0 - overhead);

    if num_groups < num_cores {
        practical * (num_groups as f64 / num_cores as f64)
    } else {
        practical
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mlp_config() {
        let config = mlp_parallel_config();
        assert!(config.enabled);
        assert_eq!(config.min_items_for_parallel, 2);
    }

    #[test]
    fn test_mlp_speedup() {
        let speedup = estimate_mlp_speedup(8);
        assert!(speedup >= 4.0);
    }
}
