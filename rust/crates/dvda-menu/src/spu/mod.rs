#![allow(dead_code)] // the pipeline is only reachable through the direct-link feature; unit tests cover the primitives
//! Rust subpicture generator and multiplexer for the menu authoring path.

pub(crate) mod color;
pub(crate) mod encode;
pub(crate) mod image;
pub(crate) mod mux;
pub(crate) mod xml;

pub(crate) use image::RgbaReader;

use std::path::Path;

/// Generates the menu subpicture stream for `xml` from `input` into `output`.
///
/// `rgba` supplies RGBA pixels for the requested image paths (the production
/// author resolves them through its image bridge).
pub(crate) fn run(
    xml: &Path,
    input: &Path,
    output: &Path,
    rgba: &mut RgbaReader<'_>,
) -> Result<(), String> {
    let document = xml::parse(xml)?;
    mux::run(document, input, output, rgba)
}
