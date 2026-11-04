//! Rust subpicture generator and multiplexer for the menu authoring path.

pub(crate) mod color;
#[cfg(any(feature = "direct-link", test))]
pub(crate) mod encode;
pub(crate) mod format;
#[cfg(any(feature = "direct-link", test))]
pub(crate) mod image;
#[cfg(any(feature = "direct-link", test))]
pub(crate) mod mux;
#[cfg(any(feature = "direct-link", test))]
pub(crate) mod xml;

#[cfg(any(feature = "direct-link", test))]
pub(crate) use image::RgbaReader;

#[cfg(any(feature = "direct-link", test))]
use std::path::Path;

/// Generates the menu subpicture stream for `xml` from `input` into `output`.
///
/// `rgba` supplies RGBA pixels for the requested image paths (the production
/// author resolves them through its image bridge).
#[cfg(any(feature = "direct-link", test))]
pub(crate) fn run(
    xml: &Path,
    input: &Path,
    output: &Path,
    rgba: &mut RgbaReader<'_>,
) -> Result<(), String> {
    let document = xml::parse(xml)?;
    mux::run(document, input, output, rgba)
}
