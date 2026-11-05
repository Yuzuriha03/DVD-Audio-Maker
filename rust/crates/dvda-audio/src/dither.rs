//! Stateful TPDF noise in units of one output least-significant bit.

/// Independent random draws are consumed for every interleaved channel sample.
/// Keeping this state for a conversion makes the result independent of block sizes.
pub struct Tpdf {
    state: u64,
}

impl Tpdf {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "The random value is shifted to exactly 53 bits before conversion"
    )]
    fn uniform(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
        (value >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }

    pub fn noise(&mut self) -> f64 {
        self.uniform() - self.uniform()
    }

    /// Quantize normalized audio to S16, adding TPDF once before rounding.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Rounding and clamping define the intentional S16 quantization"
    )]
    pub fn quantize(&mut self, sample: f64) -> i16 {
        let value = sample * 32768.0 + self.noise();
        value.round_ties_even().clamp(-32768.0, 32767.0) as i16
    }
}

#[must_use]
pub const fn enabled(switch: bool, source_bits: u16, source_float: bool, target_bits: u16) -> bool {
    switch && target_bits == 16 && (source_float || source_bits > 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precision_policy() {
        for switch in [false, true] {
            for bits in [8, 16, 20, 24, 32, 64] {
                for target in [16, 20, 24] {
                    for float in [false, true] {
                        assert_eq!(
                            enabled(switch, bits, float, target),
                            switch && target == 16 && (float || bits > 16)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn noise_has_triangular_moments_and_independent_channels() {
        let mut generator = Tpdf::new(0x1234_5678_9abc_def0);
        let count = 200_000;
        let mut sum = 0.0;
        let mut square = 0.0;
        let mut cross = 0.0;
        for _ in 0..count {
            let left = generator.noise();
            let right = generator.noise();
            assert!(left > -1.0 && left < 1.0);
            sum += left;
            square += left * left;
            cross += left * right;
        }
        assert!((sum / f64::from(count)).abs() < 0.003);
        assert!((square / f64::from(count) - 1.0 / 6.0).abs() < 0.003);
        assert!((cross / f64::from(count)).abs() < 0.003);
    }

    #[test]
    fn sample_sequence_does_not_depend_on_blocks() {
        let samples: Vec<_> = (0..10_003)
            .map(|n| f64::from(n - 5000) / 327_680.0)
            .collect();
        let mut reference = Tpdf::new(42);
        let expected: Vec<_> = samples
            .iter()
            .map(|sample| reference.quantize(*sample))
            .collect();
        for size in [1, 2, 127, 4096] {
            let mut generator = Tpdf::new(42);
            let mut actual = Vec::new();
            for block in samples.chunks(size) {
                actual.extend(block.iter().map(|sample| generator.quantize(*sample)));
            }
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn clipping_does_not_wrap() {
        let mut generator = Tpdf::new(0);
        for _ in 0..1000 {
            assert_eq!(generator.quantize(2.0), i16::MAX);
            assert_eq!(generator.quantize(-2.0), i16::MIN);
        }
    }
}
