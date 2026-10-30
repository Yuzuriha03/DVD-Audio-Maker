//! Pure Rust MLP encoder with adaptive entropy, prediction and decorrelation.
//! Production links this crate directly; its DLL is a development ABI adapter.

#[cfg(target_arch = "x86_64")]
mod analysis;
#[cfg(target_arch = "x86_64")]
mod output;
#[cfg(target_arch = "x86_64")]
mod pipeline;
#[cfg(target_arch = "x86_64")]
mod syntax;
mod coefficients;
pub mod direct;
mod entropy;
pub mod ffi;
pub mod format;
#[cfg(all(test, windows, target_arch = "x86_64", feature = "external-oracle"))]
mod frozen_reference;
pub mod metadata;
mod predict;
mod streaming;
mod timing;

mod bitstream;
mod config;
mod encoder;

pub use config::{Config, Error};
pub use encoder::{encode, encode_profile};

#[cfg(test)]
mod tests;
