//! Pure Rust MLP encoder with adaptive entropy, prediction and decorrelation.
//! Production links this crate directly; its DLL is a development ABI adapter.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_arch = "x86_64")]
mod analysis;
mod coefficients;
#[allow(unsafe_code)]
pub mod direct;
mod entropy;
#[allow(unsafe_code)]
pub mod ffi;
pub mod format;
#[cfg(all(test, windows, target_arch = "x86_64", feature = "external-oracle"))]
#[allow(unsafe_code)]
mod frozen_reference;
pub mod metadata;
#[cfg(target_arch = "x86_64")]
mod output;
#[cfg(target_arch = "x86_64")]
mod pipeline;
mod predict;
mod streaming;
#[cfg(target_arch = "x86_64")]
mod syntax;
mod timing;

mod bitstream;
mod config;
mod encoder;

pub use config::{Config, Error};
pub use encoder::{encode, encode_profile};

#[cfg(test)]
mod tests;
