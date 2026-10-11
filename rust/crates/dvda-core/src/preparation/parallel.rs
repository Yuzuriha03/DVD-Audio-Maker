//! Small scheduling estimates. These are planning heuristics, not benchmark results.

/// Estimate speedup with an 85% parallel fraction and 5% scheduling overhead.
///
/// Empty batches and single-worker execution use the sequential baseline. Small
/// batches reduce the estimate, which never claims a slowdown below that baseline.
pub fn estimate_speedup(num_files: usize, num_cores: usize) -> f64 {
    let workers = num_cores.clamp(1, 16).min(num_files.max(1));
    if workers == 1 {
        return 1.0;
    }
    let theoretical = 1.0 / (0.15 + 0.85 / workers as f64);
    let occupancy = (num_files as f64 / (workers * 2) as f64).min(1.0);
    (theoretical * 0.95 * occupancy).max(1.0)
}

/// Decide whether a batch can use the active Rayon pool.
pub fn should_use_parallel(num_files: usize, min_threshold: usize) -> bool {
    num_files > 1 && num_files >= min_threshold && rayon::current_num_threads() > 1
}

/// Ceiling division with a nonzero chunk size, including empty inputs.
pub fn optimal_chunk_size(total_items: usize, num_threads: usize) -> usize {
    total_items.div_ceil(num_threads.max(1)).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_are_bounded_for_small_empty_and_single_core_batches() {
        assert!((2.5..=3.5).contains(&estimate_speedup(100, 4)));
        assert!((1.0..=2.0).contains(&estimate_speedup(4, 4)));
        for (items, cores) in [(0, 0), (1, 4), (100, 1), (4, 0)] {
            assert_eq!(estimate_speedup(items, cores), 1.0);
        }
        assert_eq!(estimate_speedup(100, 128), estimate_speedup(100, 16));
    }

    #[test]
    fn threshold_uses_the_selected_pool_instead_of_host_core_count() {
        let one = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        let two = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .unwrap();
        assert!(!one.install(|| should_use_parallel(10, 4)));
        assert!(two.install(|| should_use_parallel(10, 4)));
        assert!(!two.install(|| should_use_parallel(2, 4)));
        assert!(!two.install(|| should_use_parallel(0, 0)));
    }

    #[test]
    fn chunk_size_handles_zero_and_maximum_counts_without_overflow() {
        assert_eq!(optimal_chunk_size(100, 4), 25);
        assert_eq!(optimal_chunk_size(10, 4), 3);
        assert_eq!(optimal_chunk_size(0, 0), 1);
        assert_eq!(optimal_chunk_size(1, 4), 1);
        assert_eq!(optimal_chunk_size(usize::MAX, 1), usize::MAX);
    }
}
