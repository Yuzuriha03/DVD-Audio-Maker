//! Pure Rust MLP encoder with adaptive entropy, prediction and decorrelation.
//! Production links this crate directly; its DLL is a development ABI adapter.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_arch = "x86_64")]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::items_after_statements,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    reason = "Analysis routines preserve the C encoder's arithmetic, naming, and candidate ordering"
)]
mod analysis;
mod coefficients;
#[allow(unsafe_code)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    reason = "Direct stream adapters preserve callback contracts and bounded PCM geometry"
)]
pub mod direct;
mod entropy;
#[allow(unsafe_code)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::unreadable_literal,
    reason = "FFI adapters preserve C ABI widths, signed layouts, and protocol constants"
)]
pub mod ffi;
pub mod format;
#[cfg(all(test, windows, target_arch = "x86_64", feature = "external-oracle"))]
#[allow(unsafe_code)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    reason = "Frozen oracle fixtures preserve complete C reference vectors"
)]
mod frozen_reference;
#[allow(
    clippy::cast_possible_truncation,
    reason = "Metadata records preserve their protocol-sized fields"
)]
pub mod metadata;
#[cfg(target_arch = "x86_64")]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::verbose_bit_mask,
    reason = "Output rate and queue code preserve MLP header and timing bit fields"
)]
mod output;
#[cfg(target_arch = "x86_64")]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::default_trait_access,
    clippy::items_after_statements,
    clippy::needless_pass_by_value,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    reason = "Pipeline routines preserve C-compatible encoding stages and state structures"
)]
mod pipeline;
#[allow(
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    reason = "Prediction retains the original coefficient quantization and filter loop"
)]
mod predict;
mod streaming;
#[cfg(target_arch = "x86_64")]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    reason = "Syntax writers preserve MLP protocol fields and C variable naming"
)]
mod syntax;
#[allow(
    clippy::decimal_bitwise_operands,
    reason = "Timing header masks retain the historical bit constants"
)]
mod timing;

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::inline_always,
    reason = "Bitstream primitives preserve fixed-width writes and hot-path inlining"
)]
mod bitstream;
mod config;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    reason = "The encoder retains complete AU orchestration and PCM ownership boundaries"
)]
mod encoder;

pub use config::{Config, Error};
pub use encoder::{encode, encode_profile};

#[cfg(test)]
#[allow(
    clippy::similar_names,
    clippy::unreadable_literal,
    reason = "Unit fixtures retain descriptive protocol names and fixed test constants"
)]
mod tests;
