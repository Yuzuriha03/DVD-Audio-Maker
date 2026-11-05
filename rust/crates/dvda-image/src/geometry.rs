//! Checked geometry for image operations. Pixel resampling is a separate operation.
use std::{error::Error, fmt};

pub const MAX_DIMENSION: u32 = 16_384;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Dimensions {
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidDimensions;

impl fmt::Display for InvalidDimensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("image dimensions must be between 1 and 16384")
    }
}
impl Error for InvalidDimensions {}

impl Dimensions {
    /// Construct dimensions within the image working-memory limits.
    ///
    /// # Errors
    /// Returns `InvalidDimensions` when either axis is zero or exceeds `MAX_DIMENSION`.
    pub fn new(width: u32, height: u32) -> Result<Self, InvalidDimensions> {
        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(InvalidDimensions);
        }
        Ok(Self { width, height })
    }

    #[must_use]
    pub fn width(self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn byte_len(self, channels: usize) -> Option<usize> {
        (self.width as usize)
            .checked_mul(self.height as usize)?
            .checked_mul(channels)
    }

    /// Aspect-preserving size, rounded to nearest with half values rounded up.
    /// Fill can exceed the target; the intermediate size is still resource-limited.
    ///
    /// # Errors
    /// Returns `InvalidDimensions` if aspect-ratio scaling overflows or produces an empty or oversized image.
    pub fn resized(self, target: Self, mode: ResizeMode) -> Result<Self, InvalidDimensions> {
        if mode == ResizeMode::Exact {
            return Ok(target);
        }
        let width_ratio_smaller = u64::from(target.width) * u64::from(self.height)
            <= u64::from(target.height) * u64::from(self.width);
        let use_width = match mode {
            ResizeMode::Fit => width_ratio_smaller,
            ResizeMode::Fill => !width_ratio_smaller,
            ResizeMode::Exact => unreachable!(),
        };
        let (width, height) = if use_width {
            (target.width, scaled(self.height, target.width, self.width)?)
        } else {
            (
                scaled(self.width, target.height, self.height)?,
                target.height,
            )
        };
        Self::new(width, height)
    }

    /// Source/destination overlap for a centered extent, without unsigned underflow.
    /// An odd spare pixel is placed at the right or bottom.
    #[must_use]
    pub fn centered_extent(self, target: Self) -> Extent {
        let width = self.width.min(target.width);
        let height = self.height.min(target.height);
        Extent {
            source_x: (self.width - width) / 2,
            source_y: (self.height - height) / 2,
            destination_x: (target.width - width) / 2,
            destination_y: (target.height - height) / 2,
            width,
            height,
        }
    }
}

fn scaled(value: u32, numerator: u32, denominator: u32) -> Result<u32, InvalidDimensions> {
    let result = (u64::from(value) * u64::from(numerator) + u64::from(denominator) / 2)
        / u64::from(denominator);
    u32::try_from(result.max(1)).map_err(|_| InvalidDimensions)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResizeMode {
    Fit,
    Fill,
    Exact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Extent {
    pub source_x: u32,
    pub source_y: u32,
    pub destination_x: u32,
    pub destination_y: u32,
    pub width: u32,
    pub height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(w: u32, h: u32) -> Dimensions {
        Dimensions::new(w, h).unwrap()
    }

    #[test]
    fn fit_fill_and_exact_sizes() {
        for (input, target, fit, fill) in [
            ((1000, 1000), (720, 576), (576, 576), (720, 720)),
            ((1920, 1080), (720, 480), (720, 405), (853, 480)),
            ((300, 900), (170, 170), (57, 170), (170, 510)),
            ((2, 3), (1, 1), (1, 1), (1, 2)),
        ] {
            let source = size(input.0, input.1);
            let target = size(target.0, target.1);
            assert_eq!(
                source.resized(target, ResizeMode::Fit),
                Ok(size(fit.0, fit.1))
            );
            assert_eq!(
                source.resized(target, ResizeMode::Fill),
                Ok(size(fill.0, fill.1))
            );
            assert_eq!(source.resized(target, ResizeMode::Exact), Ok(target));
        }
    }

    #[test]
    fn limits_intermediate_allocations() {
        assert!(Dimensions::new(0, 1).is_err());
        assert!(Dimensions::new(1, 16385).is_err());
        assert!(
            size(16384, 1)
                .resized(size(720, 576), ResizeMode::Fill)
                .is_err()
        );
        assert_eq!(size(720, 576).byte_len(4), Some(1_658_880));
        assert_eq!(size(720, 576).byte_len(usize::MAX), None);
    }

    #[test]
    fn centered_crop_and_padding_keep_odd_pixel_on_far_edge() {
        assert_eq!(
            size(5, 2).centered_extent(size(2, 5)),
            Extent {
                source_x: 1,
                source_y: 0,
                destination_x: 0,
                destination_y: 1,
                width: 2,
                height: 2,
            }
        );
        assert_eq!(
            size(720, 576).centered_extent(size(720, 576)),
            Extent {
                source_x: 0,
                source_y: 0,
                destination_x: 0,
                destination_y: 0,
                width: 720,
                height: 576,
            }
        );
    }
}
