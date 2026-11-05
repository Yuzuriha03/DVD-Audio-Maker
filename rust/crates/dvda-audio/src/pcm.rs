//! Little-endian PCM representations accepted by the media input pipeline.

use std::io::{Error, ErrorKind, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Signed8,
    Unsigned8,
    Signed16,
    Signed24,
    Signed32,
    Float32,
    Float64,
}

impl Encoding {
    #[must_use]
    pub const fn bytes(self) -> usize {
        match self {
            Self::Signed8 | Self::Unsigned8 => 1,
            Self::Signed16 => 2,
            Self::Signed24 => 3,
            Self::Signed32 | Self::Float32 => 4,
            Self::Float64 => 8,
        }
    }

    #[must_use]
    pub const fn is_float(self) -> bool {
        matches!(self, Self::Float32 | Self::Float64)
    }

    /// Decode complete interleaved samples without changing their ordering.
    /// Integer PCM is exactly representable in the normalized f64 result.
    ///
    /// # Errors
    /// Returns `InvalidData` when the input ends with an incomplete sample.
    ///
    /// # Panics
    /// Panics only if a validated sample chunk no longer matches its encoding width.
    pub fn decode(self, bytes: &[u8], output: &mut Vec<f64>) -> Result<()> {
        if !bytes.len().is_multiple_of(self.bytes()) {
            return Err(Error::new(ErrorKind::InvalidData, "incomplete PCM sample"));
        }
        output.clear();
        output.reserve(bytes.len() / self.bytes());
        for sample in bytes.chunks_exact(self.bytes()) {
            let value = match self {
                Self::Signed8 => f64::from(sample[0] as i8) / 128.0,
                Self::Unsigned8 => (f64::from(sample[0]) - 128.0) / 128.0,
                Self::Signed16 => f64::from(i16::from_le_bytes([sample[0], sample[1]])) / 32768.0,
                Self::Signed24 => {
                    let aligned = i32::from_le_bytes([0, sample[0], sample[1], sample[2]]);
                    f64::from(aligned) / 2_147_483_648.0
                }
                Self::Signed32 => {
                    f64::from(i32::from_le_bytes(sample.try_into().unwrap())) / 2_147_483_648.0
                }
                Self::Float32 => f64::from(f32::from_le_bytes(sample.try_into().unwrap())),
                Self::Float64 => f64::from_le_bytes(sample.try_into().unwrap()),
            };
            output.push(value);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_extrema_and_sign_extension() {
        for (encoding, bytes, expected) in [
            (
                Encoding::Signed8,
                vec![128, 0, 127],
                vec![-1.0, 0.0, 127.0 / 128.0],
            ),
            (
                Encoding::Unsigned8,
                vec![0, 128, 255],
                vec![-1.0, 0.0, 127.0 / 128.0],
            ),
            (
                Encoding::Signed16,
                vec![0, 128, 255, 127],
                vec![-1.0, 32767.0 / 32768.0],
            ),
            (
                Encoding::Signed24,
                vec![0, 0, 128, 255, 255, 127, 255, 255, 255],
                vec![-1.0, 8_388_607.0 / 8_388_608.0, -1.0 / 8_388_608.0],
            ),
            (
                Encoding::Signed32,
                vec![0, 0, 0, 128, 255, 255, 255, 127],
                vec![-1.0, 2_147_483_647.0 / 2_147_483_648.0],
            ),
        ] {
            let mut decoded = vec![42.0];
            encoding.decode(&bytes, &mut decoded).unwrap();
            assert_eq!(decoded, expected);
        }
    }

    #[test]
    fn float_precision_and_nonfinite_values_are_preserved() {
        let mut decoded = Vec::new();
        Encoding::Float32
            .decode(&0.125f32.to_le_bytes(), &mut decoded)
            .unwrap();
        assert_eq!(decoded, [0.125]);
        for value in [f64::MIN_POSITIVE, -1.25, f64::INFINITY, f64::NAN] {
            Encoding::Float64
                .decode(&value.to_le_bytes(), &mut decoded)
                .unwrap();
            assert_eq!(decoded[0].to_bits(), value.to_bits());
        }
    }

    #[test]
    fn incomplete_sample_does_not_modify_output() {
        let mut decoded = vec![42.0];
        assert!(Encoding::Signed24.decode(&[0, 0], &mut decoded).is_err());
        assert_eq!(decoded, [42.0]);
    }
}
