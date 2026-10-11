// MLP encoding batch parallel implementation
// Enables parallel encoding of multiple MLP groups

use super::parallel_enhanced::{BatchProcessor, ParallelConfig};

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

/// Encode independent MLP groups in parallel while preserving input order.
///
/// The workflow still owns staging, cache publication and progress reporting;
/// this helper only parallelizes a pure per-group operation and therefore does
/// not race those shared resources.
pub fn encode_mlp_groups<T, R, E, F>(
    groups: Vec<T>,
    config: ParallelConfig,
    encode: F,
) -> Result<Vec<R>, E>
where
    T: Send + Sync,
    R: Send,
    E: Send,
    F: Fn(&T) -> Result<R, E> + Send + Sync,
{
    BatchProcessor::new(groups, config).process_with_result(encode)
}

/// Estimate the speedup of independent groups. This is a scheduling heuristic,
/// not a benchmark result.
pub fn estimate_mlp_speedup(num_groups: usize) -> f64 {
    super::parallel::estimate_speedup(
        num_groups,
        std::thread::available_parallelism().map_or(1, usize::from),
    )
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
        assert!(speedup >= 1.0);
    }

    #[test]
    fn encodes_groups_in_input_order() {
        let result = encode_mlp_groups(vec![1_u32, 2, 3], mlp_parallel_config(), |value| {
            Ok::<_, ()>(value + 10)
        })
        .unwrap();
        assert_eq!(result, vec![11, 12, 13]);
    }
}
