//! Pure Rust MLP encoder with adaptive entropy, prediction and decorrelation.
//!
//! Every target builds the same `analysis`/`output`/`pipeline`/`syntax`
//! implementation; the FFI streaming entry points use that pipeline, including
//! live metadata refresh, on Windows x64, Windows ARM64 and Android ARM64.
//!
//! The batch `encode`/`encode_profile` compatibility API drives the legacy
//! access-unit builder. That builder is isolated behind `cfg(test)` as a
//! fixture/oracle implementation and is never compiled into a production build;
//! production output comes only from the pipeline.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod analysis;
mod coefficients;
#[allow(unsafe_code)]
pub mod direct;
mod entropy;
#[allow(unsafe_code)]
pub mod ffi;
pub mod format;
mod header;
pub mod metadata;
mod output;
mod pipeline;
#[cfg(test)]
mod predict;
mod streaming;
mod syntax;
mod timing;

mod bitstream;
mod config;
#[cfg(test)]
mod encoder;

pub use config::{Config, Error};
#[cfg(test)]
pub use encoder::{encode, encode_profile};

#[cfg(test)]
mod tests;
