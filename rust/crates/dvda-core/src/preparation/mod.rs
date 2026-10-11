//! Source preparation, cache evidence and manifest generation.
pub mod alac;
pub mod models;
pub mod pipeline;
pub mod rules;
pub mod snapshot;
pub mod state;

// Parallel processing modules
pub mod audio_decode_parallel;
pub mod mlp_parallel;
pub mod parallel;
pub mod parallel_enhanced;
pub mod pipeline_optimization;
