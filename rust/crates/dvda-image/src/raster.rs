use crate::geometry::{Dimensions, ResizeMode};
use image::{GenericImageView, ImageBuffer, Rgba, RgbaImage, imageops};
use std::io;

pub const MAX_WORKING_BYTES: usize = 256 * 1024 * 1024;

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Callers supply rounded f32 channel values bounded to 0..=255; float-to-integer conversion has no standard checked equivalent"
)]
fn quantize_f32(value: f32) -> u8 {
    value as u8
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Radial shading is a convex combination of u8 channels, so the rounded f64 result remains within 0..=255"
)]
fn quantize_f64(value: f64) -> u8 {
    value as u8
}

pub(crate) fn check_working_bytes(buffers: &[usize]) -> io::Result<()> {
    let total = buffers
        .iter()
        .try_fold(0usize, |sum, bytes| sum.checked_add(*bytes));
    if total.is_none_or(|bytes| bytes > MAX_WORKING_BYTES) {
        return Err(io::Error::new(
            io::ErrorKind::OutOfMemory,
            "image working buffers exceed 256 MiB",
        ));
    }
    Ok(())
}

/// Owned, straight-alpha sRGB pixels. All operations preserve row-major RGBA8 layout.
#[derive(Clone, Debug)]
pub struct Raster {
    pixels: RgbaImage,
}

// Compute premultiplied samples lazily instead of allocating a full RGBA32F source.
struct Premultiplied<'a>(&'a RgbaImage);

impl GenericImageView for Premultiplied<'_> {
    type Pixel = Rgba<f32>;

    fn dimensions(&self) -> (u32, u32) {
        self.0.dimensions()
    }

    fn get_pixel(&self, x: u32, y: u32) -> Self::Pixel {
        let pixel = self.0.get_pixel(x, y);
        let alpha = f32::from(pixel[3]) / 255.0;
        Rgba([
            f32::from(pixel[0]) * alpha / 255.0,
            f32::from(pixel[1]) * alpha / 255.0,
            f32::from(pixel[2]) * alpha / 255.0,
            alpha,
        ])
    }
}

impl Raster {
    /// Allocate a solid RGBA raster within the working-memory budget.
    ///
    /// # Errors
    /// Returns an error when dimensions overflow, exceed the memory budget, or cannot be allocated.
    pub fn solid(size: Dimensions, color: [u8; 4]) -> io::Result<Self> {
        let length = size
            .byte_len(4)
            .ok_or_else(|| io::Error::other("RGBA size overflow"))?;
        check_working_bytes(&[length])?;
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(length).map_err(io::Error::other)?;
        pixels.resize(length, 0);
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&color);
        }
        Self::from_rgba(size, pixels)
    }

    /// Construct a raster from tightly packed RGBA pixels.
    ///
    /// # Errors
    /// Returns `InvalidInput` when the pixel length or image dimensions do not match.
    pub fn from_rgba(size: Dimensions, pixels: Vec<u8>) -> io::Result<Self> {
        if size.byte_len(4) != Some(pixels.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid RGBA buffer length",
            ));
        }
        Ok(Self {
            pixels: ImageBuffer::from_raw(size.width(), size.height(), pixels).ok_or_else(
                || io::Error::new(io::ErrorKind::InvalidInput, "invalid RGBA dimensions"),
            )?,
        })
    }

    /// Return the dimensions established at construction.
    ///
    /// # Panics
    /// Panics only if internal storage violates the construction invariant.
    #[must_use]
    pub fn dimensions(&self) -> Dimensions {
        Dimensions::new(self.pixels.width(), self.pixels.height())
            .expect("raster dimensions validated at construction")
    }

    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        self.pixels.as_raw()
    }

    /// Zero-contrast brightness reduction in encoded sRGB; alpha is unchanged.
    pub fn dim(&mut self, percent: u8) {
        let offset = f32::from(percent.min(100)) * 255.0 / 100.0;
        for pixel in self.pixels.pixels_mut() {
            for channel in &mut pixel.0[..3] {
                *channel = quantize_f32((f32::from(*channel) - offset).round().clamp(0.0, 255.0));
            }
        }
    }

    /// Resize the raster using the requested aspect-ratio policy.
    ///
    /// # Errors
    /// Returns an error for invalid target geometry, excessive working memory, or allocation failure.
    pub fn resized(&self, target: Dimensions, mode: ResizeMode) -> io::Result<Self> {
        let size = self
            .dimensions()
            .resized(target, mode)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        if size == self.dimensions() {
            return Ok(self.clone());
        }
        let source = self.dimensions();
        if size.height() == source.height() && source.height() > 128 {
            // Playback stills first correct aspect ratio at full source height.
            // Overlap preserves the vertical Lanczos3 neighborhood at band edges.
            let band_height = 64;
            check_working_bytes(&[
                self.rgba().len(),
                size.byte_len(4)
                    .ok_or_else(|| io::Error::other("output size overflow"))?,
                source.width() as usize * (band_height + 6) as usize * 24,
                size.width() as usize * (band_height + 6) as usize * 20,
            ])?;
            let mut output = RgbaImage::new(size.width(), size.height());
            for top in (0..source.height()).step_by(band_height as usize) {
                let bottom = (top + band_height).min(source.height());
                let start = top.saturating_sub(3);
                let end = (bottom + 3).min(source.height());
                let band = Raster {
                    pixels: imageops::crop_imm(&self.pixels, 0, start, source.width(), end - start)
                        .to_image(),
                };
                let resized = band.resized(
                    Dimensions::new(size.width(), end - start).map_err(io::Error::other)?,
                    ResizeMode::Exact,
                )?;
                for y in top..bottom {
                    for x in 0..size.width() {
                        output.put_pixel(x, y, *resized.pixels.get_pixel(x, y - start));
                    }
                }
            }
            return Ok(Self { pixels: output });
        }
        let intermediate =
            Dimensions::new(source.width(), size.height()).map_err(io::Error::other)?;
        check_working_bytes(&[
            self.rgba().len(),
            intermediate
                .byte_len(16)
                .ok_or_else(|| io::Error::other("intermediate size overflow"))?,
            size.byte_len(20)
                .ok_or_else(|| io::Error::other("output size overflow"))?,
        ])?;
        let premultiplied = Premultiplied(&self.pixels);
        let resized = imageops::resize(
            &premultiplied,
            size.width(),
            size.height(),
            imageops::FilterType::Lanczos3,
        );
        let pixels = ImageBuffer::from_fn(size.width(), size.height(), |x, y| {
            let pixel = resized.get_pixel(x, y);
            let alpha = pixel[3].clamp(0.0, 1.0);
            if alpha < 0.5 / 255.0 {
                Rgba([0; 4])
            } else {
                Rgba([
                    quantize_f32((pixel[0] * 255.0 / alpha).round().clamp(0.0, 255.0)),
                    quantize_f32((pixel[1] * 255.0 / alpha).round().clamp(0.0, 255.0)),
                    quantize_f32((pixel[2] * 255.0 / alpha).round().clamp(0.0, 255.0)),
                    quantize_f32((alpha * 255.0).round()),
                ])
            }
        });
        Ok(Self { pixels })
    }

    /// Crop or pad the raster into a centered target extent.
    ///
    /// # Errors
    /// Returns an error when the target exceeds the working-memory budget or allocation fails.
    pub fn centered_extent(&self, target: Dimensions, background: [u8; 4]) -> io::Result<Self> {
        check_working_bytes(&[
            self.rgba().len(),
            target
                .byte_len(4)
                .ok_or_else(|| io::Error::other("extent size overflow"))?,
        ])?;
        let extent = self.dimensions().centered_extent(target);
        let mut result = Self::solid(target, background)?;
        for y in 0..extent.height {
            for x in 0..extent.width {
                let pixel = *self
                    .pixels
                    .get_pixel(extent.source_x + x, extent.source_y + y);
                result
                    .pixels
                    .put_pixel(extent.destination_x + x, extent.destination_y + y, pixel);
            }
        }
        Ok(result)
    }

    /// Source-over composition in sRGB, with clipping at the canvas boundary.
    pub fn blend_pixel(&mut self, x: i32, y: i32, color: [u8; 4]) {
        let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
            return;
        };
        if x >= self.pixels.width() || y >= self.pixels.height() {
            return;
        }
        let destination = self.pixels.get_pixel_mut(x, y);
        let source_alpha = u32::from(color[3]);
        let destination_alpha = u32::from(destination[3]);
        let alpha = source_alpha * 255 + destination_alpha * (255 - source_alpha);
        if alpha == 0 {
            return;
        }
        for channel in 0..3 {
            destination[channel] = ((u32::from(color[channel]) * source_alpha * 255
                + u32::from(destination[channel]) * destination_alpha * (255 - source_alpha)
                + alpha / 2)
                / alpha)
                .to_le_bytes()[0];
        }
        destination[3] = ((alpha + 127) / 255).to_le_bytes()[0];
    }

    /// Composite validated text glyph coverage onto the raster.
    ///
    /// # Errors
    /// Returns `InvalidInput` when a glyph's coverage length does not match its dimensions.
    pub fn draw_text_mask(
        &mut self,
        mask: &crate::text::TextMask,
        x: i32,
        baseline: i32,
        color: [u8; 4],
    ) -> io::Result<()> {
        for glyph in &mask.glyphs {
            let length = (glyph.width as usize).checked_mul(glyph.height as usize);
            if length != Some(glyph.coverage.len()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid glyph coverage length",
                ));
            }
        }
        for glyph in &mask.glyphs {
            if glyph.width == 0 {
                continue;
            }
            for (index, coverage) in glyph.coverage.iter().enumerate() {
                let mut pixel = color;
                pixel[3] =
                    ((u16::from(color[3]) * u16::from(*coverage) + 127) / 255).to_le_bytes()[0];
                let index = i64::try_from(index).map_err(io::Error::other)?;
                let gx = i64::from(x) + i64::from(glyph.x) + index % i64::from(glyph.width);
                let gy = i64::from(baseline) + i64::from(glyph.y) + index / i64::from(glyph.width);
                if let (Ok(gx), Ok(gy)) = (i32::try_from(gx), i32::try_from(gy)) {
                    self.blend_pixel(gx, gy, pixel);
                }
            }
        }
        Ok(())
    }

    /// Inclusive pixel bounds, matching menu rectangle coordinates.
    pub fn fill_rectangle(&mut self, bounds: [i32; 4], color: [u8; 4]) {
        let [left, top, right, bottom] = bounds;
        let right = right.min(self.pixels.width().cast_signed() - 1);
        let bottom = bottom.min(self.pixels.height().cast_signed() - 1);
        for y in top.max(0)..=bottom {
            for x in left.max(0)..=right {
                self.blend_pixel(x, y, color);
            }
        }
    }

    /// Opaque sRGB gradient projected along the top-left to bottom-right diagonal.
    ///
    /// # Errors
    /// Returns an error when the raster exceeds the working-memory budget or allocation fails.
    pub fn diagonal_gradient(size: Dimensions, start: [u8; 3], end: [u8; 3]) -> io::Result<Self> {
        let mut result = Self::solid(size, [0, 0, 0, 255])?;
        let dx = u64::from(size.width() - 1);
        let dy = u64::from(size.height() - 1);
        let denominator = (dx * dx + dy * dy).max(1);
        for (x, y, pixel) in result.pixels.enumerate_pixels_mut() {
            let position = u64::from(x) * dx + u64::from(y) * dy;
            for channel in 0..3 {
                pixel[channel] = ((u64::from(start[channel]) * (denominator - position)
                    + u64::from(end[channel]) * position
                    + denominator / 2)
                    / denominator)
                    .to_le_bytes()[0];
            }
        }
        Ok(result)
    }

    /// Multiply RGB by a center-to-corner radial shade without altering alpha.
    pub fn multiply_radial(&mut self, center: [u8; 3], edge: [u8; 3]) {
        let cx = f64::from(self.pixels.width() - 1) / 2.0;
        let cy = f64::from(self.pixels.height() - 1) / 2.0;
        let radius = cx.hypot(cy).max(1.0);
        for (x, y, pixel) in self.pixels.enumerate_pixels_mut() {
            let position = ((f64::from(x) - cx).hypot(f64::from(y) - cy) / radius).min(1.0);
            for channel in 0..3 {
                let shade = f64::from(center[channel]) * (1.0 - position)
                    + f64::from(edge[channel]) * position;
                pixel[channel] = quantize_f64((f64::from(pixel[channel]) * shade / 255.0).round());
            }
        }
    }

    pub fn composite(&mut self, source: &Self, x: i32, y: i32) {
        let left = i64::from(x).max(0);
        let top = i64::from(y).max(0);
        let right =
            (i64::from(x) + i64::from(source.pixels.width())).min(i64::from(self.pixels.width()));
        let bottom =
            (i64::from(y) + i64::from(source.pixels.height())).min(i64::from(self.pixels.height()));
        for dy in top..bottom {
            for dx in left..right {
                if let (Ok(source_x), Ok(source_y), Ok(destination_x), Ok(destination_y)) = (
                    u32::try_from(dx - i64::from(x)),
                    u32::try_from(dy - i64::from(y)),
                    i32::try_from(dx),
                    i32::try_from(dy),
                ) {
                    let pixel = source.pixels.get_pixel(source_x, source_y);
                    self.blend_pixel(destination_x, destination_y, pixel.0);
                }
            }
        }
    }

    /// Binary coverage for DVD subpicture triangles; no intermediate palette colors.
    ///
    /// # Panics
    /// Panics only if the fixed three-point array violates the triangle routine's internal contract.
    pub fn fill_triangle(&mut self, points: [[i32; 2]; 3], color: [u8; 4]) {
        let edge = |a: [i32; 2], b: [i32; 2], p: [i32; 2]| {
            (i128::from(b[0]) - i128::from(a[0])) * (i128::from(p[1]) - i128::from(a[1]))
                - (i128::from(b[1]) - i128::from(a[1])) * (i128::from(p[0]) - i128::from(a[0]))
        };
        if edge(points[0], points[1], points[2]) == 0 {
            return;
        }
        let left = points.iter().map(|p| p[0]).min().unwrap().max(0);
        let right = points
            .iter()
            .map(|p| p[0])
            .max()
            .unwrap()
            .min(self.pixels.width().cast_signed() - 1);
        let top = points.iter().map(|p| p[1]).min().unwrap().max(0);
        let bottom = points
            .iter()
            .map(|p| p[1])
            .max()
            .unwrap()
            .min(self.pixels.height().cast_signed() - 1);
        for y in top..=bottom {
            for x in left..=right {
                let edges = [
                    edge(points[0], points[1], [x, y]),
                    edge(points[1], points[2], [x, y]),
                    edge(points[2], points[0], [x, y]),
                ];
                if edges.iter().all(|e| *e >= 0) || edges.iter().all(|e| *e <= 0) {
                    self.blend_pixel(x, y, color);
                }
            }
        }
    }

    #[must_use]
    pub fn flatten(&self, background: [u8; 3]) -> Vec<u8> {
        self.pixels
            .pixels()
            .flat_map(|pixel| {
                let alpha = u32::from(pixel[3]);
                std::array::from_fn::<_, 3, _>(|channel| {
                    ((u32::from(pixel[channel]) * alpha
                        + u32::from(background[channel]) * (255 - alpha)
                        + 127)
                        / 255)
                        .to_le_bytes()[0]
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_matches_zero_contrast_oracle_and_preserves_alpha() {
        for (percent, expected) in [
            (0, [200, 100, 50, 17]),
            (20, [149, 49, 0, 17]),
            (50, [73, 0, 0, 17]),
            (100, [0, 0, 0, 17]),
        ] {
            let mut image =
                Raster::solid(Dimensions::new(1, 1).unwrap(), [200, 100, 50, 17]).unwrap();
            image.dim(percent);
            assert_eq!(image.rgba(), expected);
        }
    }

    #[test]
    fn gradients_have_defined_endpoints_and_preserve_alpha() {
        let mut image =
            Raster::diagonal_gradient(Dimensions::new(3, 3).unwrap(), [0; 3], [200; 3]).unwrap();
        assert_eq!(&image.rgba()[..4], &[0, 0, 0, 255]);
        assert_eq!(&image.rgba()[16..20], &[100, 100, 100, 255]);
        assert_eq!(&image.rgba()[32..], &[200, 200, 200, 255]);
        image.multiply_radial([255; 3], [128; 3]);
        assert_eq!(&image.rgba()[16..20], &[100, 100, 100, 255]);
        assert_eq!(&image.rgba()[32..], &[100, 100, 100, 255]);
        let single =
            Raster::diagonal_gradient(Dimensions::new(1, 1).unwrap(), [7; 3], [99; 3]).unwrap();
        assert_eq!(single.rgba(), &[7, 7, 7, 255]);
    }

    #[test]
    fn working_budget_rejects_overflow_and_large_resize_before_allocation() {
        assert!(check_working_bytes(&[usize::MAX, 1]).is_err());
        assert!(check_working_bytes(&[MAX_WORKING_BYTES]).is_ok());
        assert!(check_working_bytes(&[MAX_WORKING_BYTES, 1]).is_err());
        let source = Raster::solid(Dimensions::new(1, 1).unwrap(), [255; 4]).unwrap();
        assert!(
            source
                .resized(Dimensions::new(16384, 16384).unwrap(), ResizeMode::Exact)
                .is_err()
        );
    }

    #[test]
    fn large_cover_downscales_within_working_budget() {
        let source =
            Raster::solid(Dimensions::new(4000, 4000).unwrap(), [80, 120, 200, 255]).unwrap();
        let result = source
            .resized(Dimensions::new(720, 576).unwrap(), ResizeMode::Exact)
            .unwrap();
        assert_eq!(result.dimensions(), Dimensions::new(720, 576).unwrap());
        assert!(
            result
                .rgba()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [80, 120, 200, 255])
        );
    }

    #[test]
    #[ignore = "requires an explicitly supplied directory of cover images"]
    fn external_covers_downscale_to_dvd_sizes() {
        let root = std::env::var_os("DVDA_TEST_COVER_ROOT").expect("DVDA_TEST_COVER_ROOT");
        let mut count = 0;
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path().join("cover.jpg");
            if !path.is_file() {
                continue;
            }
            let source =
                crate::codec::decode(std::io::BufReader::new(std::fs::File::open(&path).unwrap()))
                    .unwrap();
            let size = source.dimensions();
            let source = source
                .resized(
                    Dimensions::new((size.width() * 15 + 8) / 16, size.height()).unwrap(),
                    ResizeMode::Exact,
                )
                .unwrap();
            for height in [480, 576] {
                source
                    .resized(Dimensions::new(720, height).unwrap(), ResizeMode::Exact)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            }
            println!("{}: {:?} OK", path.display(), source.dimensions());
            count += 1;
        }
        assert!(count > 0, "no cover.jpg files found");
        println!("{count} covers passed PAL and NTSC resizing");
    }

    #[test]
    fn banded_aspect_correction_matches_full_image() {
        let pixels = ImageBuffer::from_fn(73, 259, |x, y| {
            Rgba([
                (x * 17 + y).to_le_bytes()[0],
                (y * 11).to_le_bytes()[0],
                (x * y).to_le_bytes()[0],
                ((x + y) * 5).to_le_bytes()[0],
            ])
        });
        let full = imageops::resize(
            &Premultiplied(&pixels),
            68,
            259,
            imageops::FilterType::Lanczos3,
        );
        let expected = ImageBuffer::from_fn(68, 259, |x, y| {
            let pixel = full.get_pixel(x, y);
            let alpha = pixel[3].clamp(0.0, 1.0);
            if alpha < 0.5 / 255.0 {
                Rgba([0; 4])
            } else {
                Rgba([
                    quantize_f32((pixel[0] * 255.0 / alpha).round().clamp(0.0, 255.0)),
                    quantize_f32((pixel[1] * 255.0 / alpha).round().clamp(0.0, 255.0)),
                    quantize_f32((pixel[2] * 255.0 / alpha).round().clamp(0.0, 255.0)),
                    quantize_f32((alpha * 255.0).round()),
                ])
            }
        });
        let actual = Raster { pixels }
            .resized(Dimensions::new(68, 259).unwrap(), ResizeMode::Exact)
            .unwrap();
        assert_eq!(actual.pixels, expected);
    }

    #[test]
    fn lazy_premultiplication_matches_materialized_resize() {
        let source = ImageBuffer::from_fn(37, 29, |x, y| {
            Rgba([
                (x * 7).to_le_bytes()[0],
                (y * 11).to_le_bytes()[0],
                (x * y).to_le_bytes()[0],
                ((x + y) * 5).to_le_bytes()[0],
            ])
        });
        let lazy = Premultiplied(&source);
        let materialized = ImageBuffer::from_fn(37, 29, |x, y| lazy.get_pixel(x, y));
        for (width, height) in [(13, 7), (61, 43), (37, 29)] {
            assert_eq!(
                imageops::resize(&lazy, width, height, imageops::FilterType::Lanczos3),
                imageops::resize(&materialized, width, height, imageops::FilterType::Lanczos3),
            );
        }
    }

    #[test]
    fn drawing_clips_and_triangle_winding_preserves_binary_palette() {
        let size = Dimensions::new(5, 5).unwrap();
        let mut image = Raster::solid(size, [0; 4]).unwrap();
        let mut reversed = image.clone();
        let points = [[0, 0], [4, 2], [0, 4]];
        image.fill_triangle(points, [255, 0, 0, 255]);
        reversed.fill_triangle([points[2], points[1], points[0]], [255, 0, 0, 255]);
        assert_eq!(image.rgba(), reversed.rgba());
        assert_eq!(&image.rgba()[56..60], &[255, 0, 0, 255]);
        assert!(
            image
                .rgba()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [0; 4] || *p == [255, 0, 0, 255])
        );
        image.fill_rectangle([i32::MIN, i32::MIN, 0, 0], [255; 4]);
        assert_eq!(&image.rgba()[..4], &[255; 4]);
        let overlay = Raster::solid(Dimensions::new(2, 2).unwrap(), [0, 255, 0, 255]).unwrap();
        image.composite(&overlay, -1, -1);
        assert_eq!(&image.rgba()[..4], &[0, 255, 0, 255]);
        let before = image.rgba().to_vec();
        image.composite(&overlay, i32::MAX, i32::MIN);
        assert_eq!(image.rgba(), before);
    }

    #[test]
    fn malformed_text_mask_does_not_partially_draw() {
        let mut image = Raster::solid(Dimensions::new(1, 1).unwrap(), [0; 4]).unwrap();
        let mask = crate::text::TextMask {
            advance: 1.0,
            glyphs: vec![
                crate::text::GlyphMask {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                    coverage: vec![255],
                },
                crate::text::GlyphMask {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                    coverage: vec![255, 255],
                },
            ],
        };
        assert!(image.draw_text_mask(&mask, 0, 0, [255; 4]).is_err());
        assert_eq!(image.rgba(), &[0; 4]);
    }

    #[test]
    fn resize_does_not_bleed_hidden_colors() {
        let source = Raster::from_rgba(
            Dimensions::new(2, 1).unwrap(),
            vec![255, 0, 0, 0, 0, 0, 255, 255],
        )
        .unwrap();
        let resized = source
            .resized(Dimensions::new(1, 1).unwrap(), ResizeMode::Exact)
            .unwrap();
        assert_eq!(resized.rgba(), &[0, 0, 255, 128]);
    }

    #[test]
    fn composition_clips_and_keeps_straight_alpha() {
        let mut image = Raster::solid(Dimensions::new(1, 1).unwrap(), [0; 4]).unwrap();
        image.blend_pixel(-1, 0, [255; 4]);
        image.blend_pixel(1, 0, [255; 4]);
        assert_eq!(image.rgba(), &[0; 4]);
        image.blend_pixel(0, 0, [255, 0, 0, 128]);
        assert_eq!(image.rgba(), &[255, 0, 0, 128]);
        image.blend_pixel(0, 0, [0, 0, 255, 255]);
        assert_eq!(image.rgba(), &[0, 0, 255, 255]);
    }

    #[test]
    fn rejects_trailing_or_missing_pixels() {
        let size = Dimensions::new(1, 1).unwrap();
        assert!(Raster::from_rgba(size, vec![0; 3]).is_err());
        assert!(Raster::from_rgba(size, vec![0; 5]).is_err());
    }

    #[test]
    fn flatten_uses_straight_alpha_and_rounds() {
        let image = Raster::from_rgba(
            Dimensions::new(3, 1).unwrap(),
            vec![255, 0, 0, 0, 255, 0, 0, 128, 255, 0, 0, 255],
        )
        .unwrap();
        assert_eq!(
            image.flatten([0, 0, 255]),
            [0, 0, 255, 128, 0, 127, 255, 0, 0]
        );
    }

    #[test]
    fn extent_copies_alpha_without_compositing() {
        let source = Raster::solid(Dimensions::new(1, 1).unwrap(), [255, 0, 0, 128]).unwrap();
        let result = source
            .centered_extent(Dimensions::new(3, 3).unwrap(), [0; 4])
            .unwrap();
        assert_eq!(&result.rgba()[16..20], &[255, 0, 0, 128]);
        assert_eq!(&result.rgba()[0..4], &[0; 4]);
    }
}
