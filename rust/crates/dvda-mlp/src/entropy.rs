use crate::Bits;
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
            return (u >= 0 && u < unit).then_some((u as u32, b));
        }
        let central = 1i64 << (b + 3 - self.mode);
        let u = i64::from(x) - i64::from(self.offset) + central / 2;
        let (code, width) = if u < 0 {
            let quotient = u.div_euclid(unit);
            if quotient < -7 {
                return None;
            }
            (unit | (u & (unit - 1)), b + (-quotient) as u32 + 2)
        } else if u >= central {
            let quotient = (u - central) / unit;
            if quotient > 6 {
                return None;
            }
            let width = b + quotient as u32 + 3;
            (unit | (1i64 << (width - 2)) | (u & (unit - 1)), width)
        } else {
            (u + central, b + 4 - self.mode)
        };
        (width <= 31 && code < (1i64 << width)).then_some((code as u32, width))
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
    let divisor = 1i64 << shift;
    let wide = i64::from(value);
    (if wide >= 0 {
        wide / divisor
    } else {
        -((-wide + divisor - 1) / divisor)
    }) as i32
}

fn mlp_cost_table(mode: u32, q: i32) -> u32 {
    const TABLES: [[u32; 18]; 2] = [
        [
            0x00901403, 0x00801003, 0x00701003, 0x00600c03, 0x00500c03, 0x00400c03, 0x00300c03,
            0x00300c03, 0x00300c03, 0x00300c03, 0x00300c03, 0x00300c03, 0x00400c03, 0x00500c03,
            0x00600c03, 0x00701003, 0x00801003, 0x00901403,
        ],
        [
            0x0ff01804, 0x00901403, 0x00801403, 0x00701003, 0x00601003, 0x00500c02, 0x00400c02,
            0x00300802, 0x00200802, 0x00200802, 0x00300802, 0x00400c02, 0x00500c02, 0x00601003,
            0x00701003, 0x00801403, 0x00901403, 0x0ff01804,
        ],
    ];
    const THIRD: [u32; 36] = [
        0x0ff01804, 0x0ff01804, 0x0ff01804, 0x00901804, 0x00901404, 0x00801404, 0x00801403,
        0x00701403, 0x00701003, 0x00601003, 0x00601003, 0x00501003, 0x00500c03, 0x00400c03,
        0x00400c01, 0x00300c01, 0x00300401, 0x00100401, 0x00100401, 0x00300401, 0x00300c01,
        0x00400c01, 0x00400c03, 0x00500c03, 0x00501003, 0x00601003, 0x00601003, 0x00701003,
        0x00701403, 0x00801403, 0x00801404, 0x00901404, 0x00901804, 0x0ff01804, 0x0ff01804,
        0x0ff01804,
    ];
    assert!((1..=3).contains(&mode) && (-18..=17).contains(&q));
    let index = (q + 18) as usize;
    if mode < 3 {
        TABLES[(mode - 1) as usize][index / 2]
    } else {
        THIRD[index]
    }
}

fn mlp_cost_analyze(samples: &[i32]) -> MlpCostStats {
    let mut stats = MlpCostStats::default();
    if let Some(&sample) = samples.first() {
        stats.minimum = sample;
        stats.maximum = sample;
    }
    for &sample in samples {
        assert!((-8388608..=8388607).contains(&sample));
        if sample < stats.minimum {
            stats.minimum = sample;
        }
        if sample > stats.maximum {
            stats.maximum = sample;
        }
    }
    while arithmetic_shift(stats.minimum, stats.shift) < -9
        || arithmetic_shift(stats.maximum, stats.shift) > 8
    {
        stats.shift += 1;
    }
    for &sample in samples {
        let q = arithmetic_shift(sample * 2, stats.shift);
        for mode in 0..3 {
            stats.packed_cost[mode] =
                stats.packed_cost[mode].wrapping_add(mlp_cost_table(mode as u32 + 1, q));
        }
    }
    stats
}

fn floor_shift(value: i32, shift: u32) -> i32 {
    let divisor = 1 << shift;
    if value >= 0 {
        value / divisor
    } else {
        -((-value + divisor - 1) / divisor)
    }
}

fn overhead(old: Coding, base: u32, mode: u32, width: u32, offset: i32) -> i64 {
    if old.mode == mode && old.width as i32 - base as i32 == width as i32 && old.offset == offset {
        0
    } else {
        10
    }
}

fn consider(
    cost: i64,
    mode: u32,
    width: u32,
    best: &mut i64,
    best_mode: &mut u32,
    best_width: &mut u32,
) {
    if cost < *best {
        *best = cost;
        *best_mode = mode;
        *best_width = width;
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
    let n = samples.len() as i64;
    let qlo = floor_shift(lo * 2, shift);
    let qhi = floor_shift(hi * 2, shift);
    let mut best_cost = i64::from(i32::MAX);
    let mut best_mode = 0;
    let mut best_width = 0;
    for mode in 1..=3u32 {
        let index = (mode - 1) as usize;
        let a = qlo - [-18, -16, -15][index];
        let b = [17, 15, 14][index] - qhi;
        let cost = n * i64::from(shift + 2)
            + i64::from(packed[index] & 0x3ff)
            + overhead(old, qss, mode, shift + 2, 0);
        consider(
            cost,
            mode,
            shift + 2,
            &mut best_cost,
            &mut best_mode,
            &mut best_width,
        );
        let cost = n * i64::from(shift + 1)
            + i64::from((packed[index] >> 10) & 0x3ff)
            + overhead(old, qss, mode, shift + 1, 0);
        consider(
            cost,
            mode,
            shift + 1,
            &mut best_cost,
            &mut best_mode,
            &mut best_width,
        );
        if a >= 0
            && b >= 0
            && shift as i32 + 9 - a / 2 <= limit as i32
            && shift as i32 + 9 - b / 2 <= limit as i32
        {
            let cost = n * i64::from(shift)
                + i64::from(packed[index] >> 20)
                + overhead(old, qss, mode, shift, 0);
            consider(
                cost,
                mode,
                shift,
                &mut best_cost,
                &mut best_mode,
                &mut best_width,
            );
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
    if cost < best_cost {
        Coding {
            mode: 0,
            width: qss + width,
            offset: if width != 24 { offset } else { 0 },
        }
    } else {
        Coding {
            mode: best_mode,
            width: qss + best_width,
            offset: 0,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
}
