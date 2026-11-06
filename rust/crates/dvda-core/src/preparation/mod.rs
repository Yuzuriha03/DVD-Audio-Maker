//! Source preparation, cache evidence and manifest generation.
#[warn(clippy::missing_errors_doc)]
pub mod alac;
pub mod models;
pub mod pipeline;
pub mod rules;
#[warn(clippy::missing_errors_doc)]
pub mod snapshot;
#[warn(clippy::missing_errors_doc)]
pub mod state;

pub mod audio_decode_parallel;
