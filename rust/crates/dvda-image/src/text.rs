#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "Font coordinates intentionally use f32-compatible scaling and rounded pixel positions"
)]
use std::io;
use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source, image::Content},
};
use unicode_segmentation::UnicodeSegmentation;

pub struct TextMask {
    pub glyphs: Vec<GlyphMask>,
    pub advance: f32,
}

pub struct GlyphMask {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub coverage: Vec<u8>,
}

/// Shapes literal Unicode text using an explicit face in a TTF/OTF/TTC font.
/// Coordinates are relative to the baseline, with positive y pointing down.
///
/// # Errors
/// Rejects invalid sizes, fonts, and face indices, and propagates shaping or allocation failures.
pub fn shape_masks(
    data: &[u8],
    face_index: u32,
    text: &str,
    size: f32,
    antialias: bool,
) -> io::Result<TextMask> {
    if !size.is_finite() || !(1.0..=1024.0).contains(&size) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid font size",
        ));
    }
    if text.len() > 65_536 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "text exceeds 65536 UTF-8 bytes",
        ));
    }
    let face = rustybuzz::Face::from_slice(data, face_index)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid font face"))?;
    let font = FontRef::from_index(data, face_index as usize)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid raster font face"))?;
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let shaped = rustybuzz::shape(&face, &[], buffer);
    let scale = size / face.units_per_em() as f32;
    let mut context = ScaleContext::new();
    let mut scaler = context.builder(font).size(size).hint(true).build();
    let mut glyphs = Vec::new();
    let mut coverage_bytes = 0usize;
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;
    for (info, position) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
        if info.glyph_id == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "font has no glyph for text",
            ));
        }
        let id = u16::try_from(info.glyph_id)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if let Some(mut image) = Render::new(&[Source::Outline]).render(&mut scaler, id) {
            if image.content != Content::Mask {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "expected grayscale glyph mask",
                ));
            }
            if !antialias {
                for value in &mut image.data {
                    *value = if *value >= 128 { 255 } else { 0 };
                }
            }
            crate::raster::check_working_bytes(&[coverage_bytes, image.data.len()])?;
            coverage_bytes += image.data.len();
            let gx = (x + position.x_offset as f32 * scale).round() as i64
                + i64::from(image.placement.left);
            let gy = -((y + position.y_offset as f32 * scale).round() as i64)
                - i64::from(image.placement.top);
            glyphs.push(GlyphMask {
                x: i32::try_from(gx).map_err(io::Error::other)?,
                y: i32::try_from(gy).map_err(io::Error::other)?,
                width: image.placement.width,
                height: image.placement.height,
                coverage: image.data,
            });
        }
        x += position.x_advance as f32 * scale;
        y += position.y_advance as f32 * scale;
    }
    Ok(TextMask { glyphs, advance: x })
}

/// Wraps at word boundaries where possible, with Unicode grapheme breaks for
/// words wider than the caption. Explicit newlines are preserved.
///
/// # Errors
/// Rejects invalid fonts, face indices, text sizes, widths, and oversized text.
pub fn wrap_lines(
    data: &[u8],
    face_index: u32,
    text: &str,
    size: f32,
    width: f32,
) -> io::Result<Vec<String>> {
    if !size.is_finite()
        || !(1.0..=1024.0).contains(&size)
        || !width.is_finite()
        || width <= 0.0
        || text.len() > 65_536
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid caption dimensions or text length",
        ));
    }
    let face = rustybuzz::Face::from_slice(data, face_index)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid font face"))?;
    let fits = |line: &str| {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(line);
        buffer.guess_segment_properties();
        let shaped = rustybuzz::shape(&face, &[], buffer);
        let advance: i64 = shaped
            .glyph_positions()
            .iter()
            .map(|p| i64::from(p.x_advance))
            .sum();
        (advance as f32 * size / face.units_per_em() as f32).abs() <= width
    };
    Ok(wrap_measured(text, fits))
}

fn wrap_measured(text: &str, fits: impl Fn(&str) -> bool) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut rest = paragraph.trim_end_matches('\r');
        if rest.is_empty() {
            lines.push(String::new());
        }
        while !rest.is_empty() {
            // Keep combining marks and joined emoji together when a word
            // exceeds the caption width. Reshape emitted lines independently.
            let ends: Vec<usize> = rest
                .grapheme_indices(true)
                .map(|(i, grapheme)| i + grapheme.len())
                .collect();
            let mut low = 1;
            let mut high = ends.len();
            while low < high {
                let middle = low + (high - low).div_ceil(2);
                if fits(&rest[..ends[middle - 1]]) {
                    low = middle;
                } else {
                    high = middle - 1;
                }
            }
            let mut end = ends[low - 1];
            if end < rest.len()
                && let Some((space, _)) = rest[..end]
                    .grapheme_indices(true)
                    .rev()
                    .find(|(i, grapheme)| *i > 0 && grapheme.chars().all(char::is_whitespace))
            {
                end = space;
            }
            lines.push(rest[..end].trim_end().to_owned());
            rest = rest[end..].trim_start();
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_preserves_graphemes_newlines_and_word_boundaries() {
        assert_eq!(
            wrap_measured("a\u{301}👩‍👩‍👧‍👦中", |_| false),
            ["a\u{301}", "👩‍👩‍👧‍👦", "中"]
        );
        assert_eq!(
            wrap_measured("one two three", |s| s.len() <= 7),
            ["one", "two", "three"]
        );
        assert_eq!(wrap_measured("a\r\n\n中\n", |_| true), ["a", "", "中", ""]);
    }

    #[test]
    fn rejects_invalid_fonts_and_sizes() {
        assert!(shape_masks(&[], 0, "hello", 16.0, true).is_err());
        for size in [0.0, f32::NAN, f32::INFINITY, 1025.0] {
            assert!(shape_masks(&[], 0, "hello", size, true).is_err());
        }
    }
}
