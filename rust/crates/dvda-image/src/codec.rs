use crate::{
    geometry::{Dimensions, MAX_DIMENSION},
    raster::Raster,
};
use image::{ImageEncoder, ImageFormat, ImageReader, Limits};
use std::io::{self, BufRead, Seek, Write};

/// Decodes the first image without applying EXIF orientation or an ICC transform.
/// Color-policy integration is deliberately separate from container decoding.
///
/// # Errors
/// Rejects unsupported, malformed or oversized images and propagates decoder and allocation failures.
pub fn decode(reader: impl BufRead + Seek) -> io::Result<Raster> {
    let mut reader = ImageReader::new(reader).with_guessed_format()?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP)
    ) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "expected JPEG, PNG, or WebP",
        ));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(io::Error::other)?;
    let size = Dimensions::new(decoded.width(), decoded.height())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    crate::raster::check_working_bytes(&[
        decoded.as_bytes().len(),
        size.byte_len(4)
            .ok_or_else(|| io::Error::other("RGBA size overflow"))?,
    ])?;
    Raster::from_rgba(size, decoded.into_rgba8().into_raw())
}

/// Reads source dimensions and bit depth without converting its pixels to RGBA.
///
/// # Errors
/// Rejects unsupported, malformed or oversized containers and propagates metadata decoder failures.
pub fn metadata(reader: impl BufRead + Seek) -> io::Result<(Dimensions, u16)> {
    use image::ImageDecoder;
    let mut reader = ImageReader::new(reader).with_guessed_format()?;
    if !matches!(
        reader.format(),
        Some(
            ImageFormat::Bmp
                | ImageFormat::Gif
                | ImageFormat::Jpeg
                | ImageFormat::Png
                | ImageFormat::Tiff
                | ImageFormat::WebP
        )
    ) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "expected BMP, GIF, JPEG, PNG, TIFF, or WebP",
        ));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(io::Error::other)?;
    let (width, height) = decoder.dimensions();
    let dimensions = Dimensions::new(width, height)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok((dimensions, decoder.original_color_type().bits_per_pixel()))
}

/// Encode the raster as RGBA PNG.
///
/// # Errors
/// Propagates PNG encoding and destination write failures.
pub fn write_png(image: &Raster, output: impl Write) -> io::Result<()> {
    let size = image.dimensions();
    image::codecs::png::PngEncoder::new(output)
        .write_image(
            image.rgba(),
            size.width(),
            size.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(io::Error::other)
}

/// Encode the raster as JPEG after flattening alpha onto `background`.
///
/// # Errors
/// Rejects quality outside `1..=100` and propagates encoder or destination write failures.
pub fn write_jpeg(
    image: &Raster,
    output: impl Write,
    quality: u8,
    background: [u8; 3],
) -> io::Result<()> {
    if !(1..=100).contains(&quality) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "JPEG quality must be 1..=100",
        ));
    }
    let size = image.dimensions();
    image::codecs::jpeg::JpegEncoder::new_with_quality(output, quality)
        .write_image(
            &image.flatten(background),
            size.width(),
            size.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn embedded_cover_metadata_supports_preserved_formats() {
        let source = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            3,
            2,
            image::Rgb([20, 40, 60]),
        ));
        for format in [ImageFormat::Bmp, ImageFormat::Gif, ImageFormat::Tiff] {
            let mut bytes = Cursor::new(Vec::new());
            source.write_to(&mut bytes, format).unwrap();
            let (size, depth) = metadata(Cursor::new(bytes.get_ref())).unwrap();
            assert_eq!(size, Dimensions::new(3, 2).unwrap(), "{format:?}");
            assert!(depth > 0, "{format:?}");
            // Embedded covers are preserved, not added to the menu input formats.
            assert!(decode(Cursor::new(bytes.get_ref())).is_err());
        }
    }

    #[test]
    fn metadata_preserves_source_depth_without_rgba_conversion() {
        let image = Raster::solid(Dimensions::new(8, 8).unwrap(), [20, 40, 60, 255]).unwrap();
        let mut png = Vec::new();
        write_png(&image, &mut png).unwrap();
        assert_eq!(
            metadata(Cursor::new(&png)).unwrap(),
            (image.dimensions(), 32)
        );
        let mut jpeg = Vec::new();
        write_jpeg(&image, &mut jpeg, 90, [0, 0, 0]).unwrap();
        assert_eq!(
            metadata(Cursor::new(&jpeg)).unwrap(),
            (image.dimensions(), 24)
        );
        assert!(metadata(Cursor::new(b"not an image")).is_err());
    }

    #[test]
    fn metadata_and_decode_reject_oversized_images() {
        let width = MAX_DIMENSION + 1;
        let pixels = vec![0; width as usize * 3];
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&pixels, width, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        assert!(metadata(Cursor::new(&png)).is_err());
        assert!(decode(Cursor::new(&png)).is_err());
    }

    #[test]
    fn metadata_preserves_sixteen_bit_png_depth() {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[0, 0, 255, 255], 2, 1, image::ExtendedColorType::L16)
            .unwrap();
        assert_eq!(
            metadata(Cursor::new(&png)).unwrap(),
            (Dimensions::new(2, 1).unwrap(), 16)
        );
        let decoded = decode(Cursor::new(&png)).unwrap();
        assert_eq!(decoded.rgba(), &[0, 0, 0, 255, 255, 255, 255, 255]);
    }

    #[test]
    fn png_round_trip_preserves_rgba_including_hidden_color() {
        let image = Raster::from_rgba(
            Dimensions::new(2, 1).unwrap(),
            vec![255, 9, 17, 0, 0, 128, 255, 128],
        )
        .unwrap();
        let mut encoded = Vec::new();
        write_png(&image, &mut encoded).unwrap();
        let decoded = decode(Cursor::new(encoded)).unwrap();
        assert_eq!(decoded.dimensions(), image.dimensions());
        assert_eq!(decoded.rgba(), image.rgba());
    }

    #[test]
    fn jpeg_flattens_transparency() {
        let image = Raster::solid(Dimensions::new(8, 8).unwrap(), [255, 0, 0, 0]).unwrap();
        let mut encoded = Vec::new();
        write_jpeg(&image, &mut encoded, 100, [0, 0, 0]).unwrap();
        let decoded = decode(Cursor::new(encoded)).unwrap();
        assert!(
            decoded
                .rgba()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [0, 0, 0, 255])
        );
    }

    #[test]
    fn rejects_corrupt_input_and_invalid_quality() {
        assert!(decode(Cursor::new(b"not an image")).is_err());
        let image = Raster::solid(Dimensions::new(1, 1).unwrap(), [0; 4]).unwrap();
        let mut output = Vec::new();
        assert!(write_jpeg(&image, &mut output, 0, [0; 3]).is_err());
        assert_eq!(output, [] as [u8; 0]);
    }
}
