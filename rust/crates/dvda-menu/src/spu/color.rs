//! Colour values and the small subset of spumux colour parsing the menu path uses.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Color {
    pub(crate) r: u8,
    pub(crate) g: u8,
    pub(crate) b: u8,
    pub(crate) a: u8,
}

impl Color {
    #[cfg(any(feature = "direct-link", test))]
    pub(crate) const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    pub(crate) fn y(self) -> i32 {
        (257 * i32::from(self.r) + 504 * i32::from(self.g) + 98 * i32::from(self.b) + 16500) / 1000
    }

    pub(crate) fn cr(self) -> i32 {
        (439 * i32::from(self.r) - 368 * i32::from(self.g) - 71 * i32::from(self.b) + 128_500)
            / 1000
    }

    pub(crate) fn cb(self) -> i32 {
        (-148 * i32::from(self.r) - 291 * i32::from(self.g) + 439 * i32::from(self.b) + 128_500)
            / 1000
    }
}

/// Parses a spumux colour spec: optional hash followed by 3, 6, 9 or 12 hex digits.
/// The parser keeps the historical C semantics (three equal-length components, each
/// scaled to 0..=255, alpha opaque); named and `rgb()` forms are not used by the menu
/// authoring path and are rejected.
pub(crate) fn parse_hex(text: &str) -> Result<Color, String> {
    let digits = text.strip_prefix('#').unwrap_or(text);
    if digits.is_empty() || digits.len() > 12 || !digits.len().is_multiple_of(3) {
        return Err(format!("bad hex color string {text:?}"));
    }
    let component_length = digits.len() / 3;
    let scale = (1u32 << (4 * component_length)) - 1;
    let mut components = [0u8; 3];
    for (index, component) in components.iter_mut().enumerate() {
        let mut value = 0u32;
        for ch in digits[index * component_length..(index + 1) * component_length].chars() {
            let digit = ch
                .to_digit(16)
                .ok_or_else(|| format!("bad hex color string {text:?}"))?;
            value = value * 16 + digit;
        }
        *component = u8::try_from(value * 255 / scale).unwrap();
    }
    Ok(Color {
        r: components[0],
        g: components[1],
        b: components[2],
        a: 255,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_forms_match_historical_scale() {
        assert_eq!(
            parse_hex("#ff0000").unwrap(),
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            }
        );
        assert_eq!(
            parse_hex("00ff00").unwrap(),
            Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255
            }
        );
        assert_eq!(
            parse_hex("#f00").unwrap(),
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            }
        );
        assert_eq!(
            parse_hex("#123456").unwrap(),
            Color {
                r: 0x12,
                g: 0x34,
                b: 0x56,
                a: 255
            }
        );
        assert!(parse_hex("").is_err());
        assert!(parse_hex("#1234").is_err());
        assert!(parse_hex("#gg0000").is_err());
    }

    #[test]
    fn ycrcb_matches_c_arithmetic() {
        let red = Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };
        assert_eq!(red.y(), (257 * 255 + 16500) / 1000);
        assert_eq!(red.cr(), (439 * 255 + 128_500) / 1000);
        assert_eq!(red.cb(), (-148 * 255 + 128_500) / 1000);
    }
}
