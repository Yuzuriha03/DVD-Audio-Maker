// Entropy codes use explicitly bounded integers and C-compatible wrapping cost arithmetic.
use crate::bitstream::Bits;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Coding {
    pub mode: u32,
    pub width: u32,
    pub offset: i32,
}
impl Default for Coding {
    fn default() -> Self {
        Self {
            mode: 0,
            width: 24,
            offset: 0,
        }
    }
}
impl Coding {
    pub fn word(self, x: i32, qss: u32) -> Option<(u32, u32)> {
        let b = self.width.checked_sub(qss)?;
        if b > 24 {
            return None;
        }
        let unit = 1i64 << b;
        if self.mode == 0 {
            let u = i64::from(x) - i64::from(self.offset) + unit / 2;
            return (u >= 0 && u < unit).then_some((u32::try_from(u).ok()?, b));
        }
        let central = 1i64 << (b + 3 - self.mode);
        let u = i64::from(x) - i64::from(self.offset) + central / 2;
        let (code, width) = if u < 0 {
            let quotient = u.div_euclid(unit);
            if quotient < -7 {
                return None;
            }
            (
                unit | (u & (unit - 1)),
                b + u32::try_from(-quotient).ok()? + 2,
            )
        } else if u >= central {
            let quotient = (u - central) / unit;
            if quotient > 6 {
                return None;
            }
            let width = b + u32::try_from(quotient).ok()? + 3;
            (unit | (1i64 << (width - 2)) | (u & (unit - 1)), width)
        } else {
            (u + central, b + 4 - self.mode)
        };
        (width <= 31 && code < (1i64 << width)).then_some((u32::try_from(code).ok()?, width))
    }
    pub fn put(self, b: &mut Bits, x: i32, qss: u32) {
        let (value, width) = self.word(x, qss).expect("selected entropy code");
        b.put(value, width);
    }
}
pub(crate) fn select(samples: &[i32], qss: u32, old: Coding) -> Coding {
    select_with_context(samples, qss, old, false, 31)
}

#[derive(Default)]
struct MlpCostStats {
    minimum: i32,
    maximum: i32,
    shift: u32,
    packed_cost: [u32; 3],
}

fn arithmetic_shift(value: i32, shift: u32) -> i32 {
    value >> shift
}

fn mlp_cost_table(mode: u32, q: i32) -> u32 {
    const TABLES: [[u32; 18]; 2] = [
        [
            0x0090_1403,
            0x0080_1003,
            0x0070_1003,
            0x0060_0c03,
            0x0050_0c03,
            0x0040_0c03,
            0x0030_0c03,
            0x0030_0c03,
            0x0030_0c03,
            0x0030_0c03,
            0x0030_0c03,
            0x0030_0c03,
            0x0040_0c03,
            0x0050_0c03,
            0x0060_0c03,
            0x0070_1003,
            0x0080_1003,
            0x0090_1403,
        ],
        [
            0x0ff0_1804,
            0x0090_1403,
            0x0080_1403,
            0x0070_1003,
            0x0060_1003,
            0x0050_0c02,
            0x0040_0c02,
            0x0030_0802,
            0x0020_0802,
            0x0020_0802,
            0x0030_0802,
            0x0040_0c02,
            0x0050_0c02,
            0x0060_1003,
            0x0070_1003,
            0x0080_1403,
            0x0090_1403,
            0x0ff0_1804,
        ],
    ];
    const THIRD: [u32; 36] = [
        0x0ff0_1804,
        0x0ff0_1804,
        0x0ff0_1804,
        0x0090_1804,
        0x0090_1404,
        0x0080_1404,
        0x0080_1403,
        0x0070_1403,
        0x0070_1003,
        0x0060_1003,
        0x0060_1003,
        0x0050_1003,
        0x0050_0c03,
        0x0040_0c03,
        0x0040_0c01,
        0x0030_0c01,
        0x0030_0401,
        0x0010_0401,
        0x0010_0401,
        0x0030_0401,
        0x0030_0c01,
        0x0040_0c01,
        0x0040_0c03,
        0x0050_0c03,
        0x0050_1003,
        0x0060_1003,
        0x0060_1003,
        0x0070_1003,
        0x0070_1403,
        0x0080_1403,
        0x0080_1404,
        0x0090_1404,
        0x0090_1804,
        0x0ff0_1804,
        0x0ff0_1804,
        0x0ff0_1804,
    ];
    assert!((1..=3).contains(&mode) && (-18..=17).contains(&q));
    let index = usize::try_from(q + 18).expect("asserted q range is nonnegative");
    if mode < 3 {
        TABLES[usize::try_from(mode - 1).expect("mode is at least one")][index / 2]
    } else {
        THIRD[index]
    }
}

fn mlp_cost_analyze(samples: &[i32]) -> MlpCostStats {
    let first = samples.first().copied().unwrap_or_default();
    let mut stats = MlpCostStats {
        minimum: first,
        maximum: first,
        ..MlpCostStats::default()
    };
    for &sample in samples {
        assert!((-8_388_608..=8_388_607).contains(&sample));
        stats.minimum = stats.minimum.min(sample);
        stats.maximum = stats.maximum.max(sample);
    }
    while arithmetic_shift(stats.minimum, stats.shift) < -9
        || arithmetic_shift(stats.maximum, stats.shift) > 8
    {
        stats.shift += 1;
    }
    for &sample in samples {
        let q = arithmetic_shift(sample * 2, stats.shift);
        for (mode, packed_cost) in stats.packed_cost.iter_mut().enumerate() {
            *packed_cost = packed_cost.wrapping_add(mlp_cost_table(
                u32::try_from(mode).expect("three entropy modes") + 1,
                q,
            ));
        }
    }
    stats
}

fn floor_shift(value: i32, shift: u32) -> i32 {
    value >> shift
}

fn overhead(old: Coding, base: u32, mode: u32, width: u32, offset: i32) -> i64 {
    if old.mode == mode
        && i64::from(old.width) - i64::from(base) == i64::from(width)
        && old.offset == offset
    {
        0
    } else {
        10
    }
}

#[derive(Debug, PartialEq, Eq)]
struct CostCandidate {
    cost: i64,
    mode: u32,
    width: u32,
}

fn consider(cost: i64, mode: u32, width: u32, best: &mut CostCandidate) {
    if cost < best.cost {
        *best = CostCandidate { cost, mode, width };
    }
}

pub(crate) fn select_with_context(
    samples: &[i32],
    qss: u32,
    old: Coding,
    first8: bool,
    limit: u32,
) -> Coding {
    mlp_cost_select(samples, qss, old, first8, limit)
}

fn mlp_cost_select(samples: &[i32], qss: u32, old: Coding, first8: bool, limit: u32) -> Coding {
    assert!(!samples.is_empty() && samples.len() <= 160);
    assert!(!first8 || samples.len() == 8);
    assert!(
        qss <= 24
            && old.mode <= 3
            && old.width <= 48
            && (-16384..16384).contains(&old.offset)
            && limit <= 32
    );
    let stats = mlp_cost_analyze(samples);
    let lo = stats.minimum;
    let hi = stats.maximum;
    let shift = stats.shift;
    if lo == hi && (-16384..16384).contains(&lo) {
        return Coding {
            mode: 0,
            width: qss,
            offset: lo,
        };
    }
    let packed = stats.packed_cost;
    let n = i64::try_from(samples.len()).expect("at most 160 samples");
    let qlo = floor_shift(lo * 2, shift);
    let qhi = floor_shift(hi * 2, shift);
    let mut best = CostCandidate {
        cost: i64::from(i32::MAX),
        mode: 0,
        width: 0,
    };
    for mode in 1..=3u32 {
        let index = (mode - 1) as usize;
        let a = qlo - [-18, -16, -15][index];
        let b = [17, 15, 14][index] - qhi;
        let cost = n * i64::from(shift + 2)
            + i64::from(packed[index] & 0x3ff)
            + overhead(old, qss, mode, shift + 2, 0);
        consider(cost, mode, shift + 2, &mut best);
        let cost = n * i64::from(shift + 1)
            + i64::from((packed[index] >> 10) & 0x3ff)
            + overhead(old, qss, mode, shift + 1, 0);
        consider(cost, mode, shift + 1, &mut best);
        if a >= 0
            && b >= 0
            && i64::from(shift) + 9 - i64::from(a) / 2 <= i64::from(limit)
            && i64::from(shift) + 9 - i64::from(b) / 2 <= i64::from(limit)
        {
            let cost = n * i64::from(shift)
                + i64::from(packed[index] >> 20)
                + overhead(old, qss, mode, shift, 0);
            consider(cost, mode, shift, &mut best);
        }
    }
    let mut width = 0;
    let mut power = 1i64;
    while power <= i64::from(hi) - i64::from(lo) {
        power *= 2;
        width += 1;
    }
    let mut offset = old.offset;
    if i64::from(lo) - i64::from(offset) < -(power / 2)
        || i64::from(hi) - i64::from(offset) >= power / 2
    {
        let midpoint = -floor_shift(-(lo + hi), 1);
        let mut zero_width = width;
        while i64::from(lo) < -(power / 2) || i64::from(hi) >= power / 2 {
            power *= 2;
            zero_width += 1;
        }
        if (-16384..16384).contains(&midpoint) && zero_width - width >= if first8 { 4 } else { 1 } {
            offset = midpoint;
        } else {
            offset = 0;
            width = zero_width;
        }
    }
    let cost = n * i64::from(width)
        + overhead(old, qss, 0, width, offset)
        + if offset != old.offset && offset != 0 {
            if first8 { 30 } else { 15 }
        } else {
            0
        };
    if cost < best.cost {
        Coding {
            mode: 0,
            width: qss + width,
            offset: if width == 24 { 0 } else { offset },
        }
    } else {
        Coding {
            mode: best.mode,
            width: qss + best.width,
            offset: 0,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_selection_retains_first_tie_and_replaces_all_fields() {
        let mut best = CostCandidate {
            cost: 100,
            mode: 1,
            width: 8,
        };
        consider(100, 2, 7, &mut best);
        consider(101, 3, 6, &mut best);
        assert_eq!(
            best,
            CostCandidate {
                cost: 100,
                mode: 1,
                width: 8
            }
        );
        consider(99, 3, 6, &mut best);
        assert_eq!(
            best,
            CostCandidate {
                cost: 99,
                mode: 3,
                width: 6
            }
        );
    }

    #[test]
    fn mode_zero_and_rice_bounds() {
        assert_eq!(
            Coding {
                mode: 0,
                width: 0,
                offset: 42
            }
            .word(42, 0),
            Some((0, 0))
        );
        assert_eq!(
            Coding {
                mode: 0,
                width: 0,
                offset: 42
            }
            .word(43, 0),
            None
        );
        for mode in 1..=3 {
            let c = Coding {
                mode,
                width: 3,
                offset: 0,
            };
            for x in -8..8 {
                assert!(c.word(x, 0).is_some());
            }
        }
        assert_eq!(select(&[0; 40], 8, Coding::default()).width, 8);
    }

    fn reference_shift(value: i32, shift: u32) -> i32 {
        let divisor = 1i64 << shift;
        let wide = i64::from(value);
        if wide >= 0 {
            wide / divisor
        } else {
            -((-wide + divisor - 1) / divisor)
        }
        .try_into()
        .unwrap()
    }

    fn generated_sample(state: u32) -> i32 {
        i32::try_from((state >> 8) & 0x00ff_ffff).unwrap() - 8_388_608
    }

    #[test]
    fn shifts_match_signed_floor_division() {
        let values = [
            -16_777_216,
            -16_777_215,
            -8_388_609,
            -8_388_608,
            -65_537,
            -65_536,
            -257,
            -256,
            -255,
            -2,
            -1,
            0,
            1,
            2,
            255,
            256,
            257,
            65_535,
            65_536,
            65_537,
            8_388_607,
            16_777_214,
        ];
        for shift in 0..=24 {
            for value in values {
                assert_eq!(
                    arithmetic_shift(value, shift),
                    reference_shift(value, shift)
                );
                assert_eq!(floor_shift(value, shift), reference_shift(value, shift));
            }
        }
        let mut state = 0x4d59_5df4u32;
        for _ in 0..10_000 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let value = generated_sample(state);
            let shift = state % 25;
            assert_eq!(
                arithmetic_shift(value, shift),
                reference_shift(value, shift)
            );
            assert_eq!(floor_shift(value, shift), reference_shift(value, shift));
        }
    }

    #[test]
    #[ignore = "deterministic throughput benchmark"]
    fn entropy_arithmetic_throughput() {
        use std::hint::black_box;
        use std::time::Instant;

        let mut state = 0x4d59_5df4u32;
        let mut accumulated = 0u64;
        let start = Instant::now();
        for _ in 0..1_000_000 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let x = generated_sample(state);
            let coding = Coding {
                mode: 1 + state % 3,
                width: 8 + (state >> 3) % 17,
                offset: i32::try_from((state >> 7) % 32_768).unwrap() - 16_384,
            };
            if let Some((code, width)) = black_box(coding).word(black_box(x), black_box(0)) {
                accumulated = accumulated.wrapping_add(u64::from(code) + u64::from(width));
            }
        }
        println!(
            "[METRIC] entropy_word_ns={} accumulated={accumulated}",
            start.elapsed().as_nanos()
        );

        let mut samples = [0i32; 160];
        let start = Instant::now();
        for block in 0..10_000u32 {
            for (index, sample) in samples.iter_mut().enumerate() {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let base = generated_sample(state);
                *sample = base.min(8_388_448) + i32::try_from(index).unwrap();
            }
            let selected = select(
                black_box(&samples),
                black_box(8),
                black_box(Coding::default()),
            );
            accumulated = accumulated.wrapping_add(
                u64::from(selected.mode) + u64::from(selected.width) + u64::from(block),
            );
        }
        println!(
            "[METRIC] entropy_select_ns={} accumulated={accumulated}",
            start.elapsed().as_nanos()
        );
        assert_ne!(accumulated, 0);
    }
}
