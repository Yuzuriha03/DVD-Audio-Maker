//! Single-frame DVD menu Y4M serialization, preserving the bridge's integer rounding.
use std::io::{self, Write};

/// Write one progressive 4:2:0 frame. Validation completes before any output is written.
/// The caller owns flushing, syncing and removal of failed filesystem outputs.
///
/// # Errors
/// Rejects invalid dimensions, rates, aspect ratios, and buffer sizes, and propagates output failures.
pub fn write_rgb(
    output: &mut impl Write,
    rgb: &[u8],
    width: usize,
    height: usize,
    rate: &str,
    aspect: &str,
) -> io::Result<()> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "invalid Y4M frame parameters");
    if width == 0
        || height == 0
        || width > 16384
        || height > 16384
        || !width.is_multiple_of(2)
        || !height.is_multiple_of(2)
        || !matches!(rate, "25" | "30")
        || !matches!(aspect, "1:1" | "4:3" | "16:9" | "2.21:1")
    {
        return Err(invalid());
    }
    let n = width.checked_mul(height).ok_or_else(invalid)?;
    if n.checked_mul(3).ok_or_else(invalid)? != rgb.len() {
        return Err(invalid());
    }
    let mut yy = vec![0; n];
    let mut uu = vec![0; n / 4];
    let mut vv = vec![0; n / 4];
    for row in 0..height {
        for col in 0..width {
            let p = &rgb[(row * width + col) * 3..];
            yy[row * width + col] = y(p[0], p[1], p[2]);
        }
    }
    for row in (0..height).step_by(2) {
        for col in (0..width).step_by(2) {
            let mut sum = [0u32; 3];
            for dy in 0..2 {
                for dx in 0..2 {
                    let p = &rgb[((row + dy) * width + col + dx) * 3..];
                    for k in 0..3 {
                        sum[k] += u32::from(p[k]);
                    }
                }
            }
            // Each sum contains four bytes, so the rounded average is at most 255.
            let [r, g, b] = sum.map(|channel| u8::try_from((channel + 2) / 4).unwrap_or(u8::MAX));
            let at = (row / 2) * (width / 2) + col / 2;
            uu[at] = u(r, g, b);
            vv[at] = v(r, g, b);
        }
    }
    let header = format!(
        "YUV4MPEG2 W{width} H{height} F{} Ip A{aspect} C420jpeg\nFRAME\n",
        if rate == "30" { "30000:1001" } else { "25:1" }
    );
    output.write_all(header.as_bytes())?;
    output.write_all(&yy)?;
    output.write_all(&uu)?;
    output.write_all(&vv)
}

fn byte(value: i32) -> u8 {
    u8::try_from(value.clamp(0, 255)).expect("clamped to byte range")
}
fn y(r: u8, g: u8, b: u8) -> u8 {
    byte(((66 * i32::from(r) + 129 * i32::from(g) + 25 * i32::from(b) + 128) >> 8) + 16)
}
fn u(r: u8, g: u8, b: u8) -> u8 {
    byte(((-38 * i32::from(r) - 74 * i32::from(g) + 112 * i32::from(b) + 128) >> 8) + 128)
}
fn v(r: u8, g: u8, b: u8) -> u8 {
    byte(((112 * i32::from(r) - 94 * i32::from(g) - 18 * i32::from(b) + 128) >> 8) + 128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_frame_bytes() {
        for (rate, fraction) in [("25", "25:1"), ("30", "30000:1001")] {
            for aspect in ["1:1", "4:3", "16:9", "2.21:1"] {
                let mut output = Vec::new();
                write_rgb(&mut output, &[255, 0, 0].repeat(4), 2, 2, rate, aspect).unwrap();
                let mut expected =
                    format!("YUV4MPEG2 W2 H2 F{fraction} Ip A{aspect} C420jpeg\nFRAME\n")
                        .into_bytes();
                expected.extend_from_slice(&[82, 82, 82, 82, 90, 240]);
                assert_eq!(output, expected);
            }
        }
    }

    #[test]
    fn chroma_averages_rgb_before_conversion() {
        let rgb = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        let mut output = Vec::new();
        write_rgb(&mut output, &rgb, 2, 2, "25", "4:3").unwrap();
        assert!(output.ends_with(&[82, 144, 41, 235, 128, 128]));
    }

    #[test]
    fn rejects_invalid_input_before_writing() {
        for (width, height, rate, aspect, len) in [
            (0, 2, "25", "4:3", 0),
            (3, 2, "25", "4:3", 18),
            (2, 3, "25", "4:3", 18),
            (16386, 2, "25", "4:3", 0),
            (2, 2, "24", "4:3", 12),
            (2, 2, "25", "3:2", 12),
            (2, 2, "25", "4:3", 11),
            (2, 2, "25", "4:3", 13),
            (usize::MAX, 2, "25", "4:3", 0),
        ] {
            let mut output = Vec::new();
            let err =
                write_rgb(&mut output, &vec![0; len], width, height, rate, aspect).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
            assert_eq!(output, [] as [u8; 0]);
        }
    }

    #[test]
    fn propagates_output_failure() {
        let mut buffer = [0u8; 4];
        let err = write_rgb(&mut buffer.as_mut_slice(), &[0; 12], 2, 2, "25", "4:3").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::WriteZero);
    }
}
