//! Source preparation, cache evidence and manifest generation.
pub mod alac;
pub mod models;
pub mod pipeline;
pub mod rules;
pub mod snapshot;
pub mod state;

// Parallel processing modules
mod parallel;
mod parallel_enhanced;
mod audio_decode_parallel;
mod mlp_parallel;
mod pipeline_optimization;
