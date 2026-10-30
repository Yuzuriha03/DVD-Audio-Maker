#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct MlpMatrixPrimitive {
    pub target: u32,
    pub bypass: u32,
    pub coefficient: [i32; 18],
}

pub(crate) fn mlp_matrix_noise(
    seed: &mut u32,
    count: usize,
    shift: u32,
    first: &mut [i32],
    second: &mut [i32],
) -> i32 {
    if count > 160 || shift > 31 || first.len() < count || second.len() < count {
        return -1;
    }
    let mut state = *seed;
    for (first, second) in first[..count].iter_mut().zip(&mut second[..count]) {
        let t = ((state as i32) >> 7) as u32;
        let a = i32::from((t >> 8) as u8 as i8);
        let b = i32::from(t as u8 as i8);
        state = ((((state & 127) << 11) ^ t) << 5) ^ t;
        *first = (a as u32).wrapping_shl(shift) as i32;
        *second = (b as u32).wrapping_shl(shift) as i32;
    }
    *seed = state;
    0
}

pub(crate) struct MatrixApplyArgs<'a> {
    pub matrix: &'a [MlpMatrixPrimitive],
    pub channels: usize,
    pub qss: &'a [u32],
    pub samples: &'a mut [i32],
    pub count: usize,
    pub extra0: Option<&'a [i32]>,
    pub extra1: Option<&'a [i32]>,
    pub bypass: Option<&'a mut [u8]>,
    pub inverse: bool,
    pub clip: bool,
}

pub(crate) fn mlp_matrix_apply(args: MatrixApplyArgs<'_>) -> i32 {
    let MatrixApplyArgs {
        matrix,
        channels,
        qss,
        samples,
        count,
        extra0,
        extra1,
        mut bypass,
        inverse,
        clip,
    } = args;
    if channels == 0
        || channels > 6
        || count == 0
        || count > 160
        || matrix.len() > 15
        || qss.len() < channels
        || samples.len() < count * channels
        || qss[..channels].iter().any(|&v| v > 15)
        || extra0.is_some_and(|v| v.len() < count)
        || extra1.is_some_and(|v| v.len() < count)
        || bypass.as_ref().is_some_and(|v| v.len() < count)
    {
        return -1;
    }
    let mut bypass_count = 0;
    for p in matrix {
        let target = p.target as usize;
        if target >= channels
            || p.bypass > 1
            || p.coefficient[target] != if p.bypass != 0 { -32768 } else { -16384 }
            || (p.bypass != 0 && qss[target] != 0)
        {
            return -1;
        }
        bypass_count += p.bypass;
        if bypass_count > 8
            || (p.bypass != 0 && bypass.is_none())
            || (p.coefficient[channels] != 0 && extra0.is_none())
            || (p.coefficient[channels + 1] != 0 && extra1.is_none())
        {
            return -1;
        }
    }
    if samples[..count * channels]
        .iter()
        .any(|&v| !(-8388608..=8388607).contains(&v))
    {
        return -1;
    }
    let mut work = samples[..count * channels].to_vec();
    let mut bits = vec![0u8; count];
    if let Some(input) = bypass.as_ref() {
        bits.copy_from_slice(&input[..count]);
    }
    let mut remaining = bypass_count;
    let mut overflow = 0;
    for i in 0..matrix.len() {
        let p = &matrix[if inverse { matrix.len() - 1 - i } else { i }];
        let target = p.target as usize;
        let step = f64::from(1u32 << qss[target]);
        for (n, (frame, bits)) in work.chunks_exact_mut(channels).zip(&mut bits).enumerate() {
            let mut sum = 0.0;
            for (&sample, &coefficient) in frame.iter().zip(&p.coefficient) {
                sum += f64::from(sample) * (f64::from(coefficient) / 16384.0);
            }
            if let Some(extra) = extra0 {
                sum += f64::from(extra[n]) * (f64::from(p.coefficient[channels]) / 16384.0);
            }
            if let Some(extra) = extra1 {
                sum += f64::from(extra[n]) * (f64::from(p.coefficient[channels + 1]) / 16384.0);
            }
            let mut value;
            if p.bypass != 0 {
                if inverse {
                    value = sum.floor() + f64::from((*bits >> (remaining - 1)) & 1);
                    *bits &= ((1u32 << (remaining - 1)) - 1) as u8;
                } else {
                    value = (sum + f64::from(frame[target])).floor();
                    if !value.is_finite()
                        || value < f64::from(i32::MIN)
                        || value > f64::from(i32::MAX) - 1.0
                    {
                        return -1;
                    }
                    let rounded = value as i64;
                    *bits = (((rounded as u32 & 1) * 128) + u32::from(*bits >> 1)) as u8;
                    value = ((value + 1.0) / 2.0).floor();
                }
            } else {
                value = (sum / step).floor() * step;
            }
            if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
                return -1;
            }
            if !(-8388608.0..=8388607.0).contains(&value) {
                overflow = 1;
                if clip {
                    value = if value < 0.0 { -8388608.0 } else { 8388607.0 };
                }
            }
            frame[target] = value as i32;
        }
        if inverse && p.bypass != 0 {
            remaining -= 1;
        }
    }
    if !inverse && bypass_count != 0 {
        for bit in &mut bits {
            *bit >>= 8 - bypass_count;
        }
    }
    samples[..count * channels].copy_from_slice(&work);
    if let Some(output) = bypass.as_mut() {
        output[..count].copy_from_slice(&bits);
    }
    overflow
}

pub(crate) struct MatrixIntervalArgs<'a> {
    pub(crate) matrix: &'a [MlpMatrixPrimitive],
    pub(crate) primitives: &'a mut usize,
    pub(crate) channels: usize,
    pub(crate) qss: &'a [[u32; 6]],
    pub(crate) samples: &'a mut [i32],
    pub(crate) count: usize,
    pub(crate) bypass: &'a mut [u8],
    pub(crate) bypass_bits: &'a mut u32,
    pub(crate) lengths: Option<&'a [usize]>,
    pub(crate) blocks: usize,
    pub(crate) prefix: usize,
}

fn apply_interval(args: MatrixIntervalArgs<'_>) -> i32 {
    let MatrixIntervalArgs {
        matrix,
        primitives,
        channels,
        qss,
        samples,
        count,
        bypass,
        bypass_bits,
        lengths,
        blocks,
        prefix,
    } = args;
    if channels == 0
        || channels > 6
        || *primitives > 6
        || *primitives > matrix.len()
        || prefix > *primitives
        || count == 0
        || samples.len() < count.saturating_mul(channels)
        || bypass.len() < count
        || qss.len() < if lengths.is_some() { blocks } else { 1 }
        || lengths.is_some_and(|v| v.len() < blocks)
    {
        return -1;
    }
    for row in qss.iter().take(if lengths.is_some() { blocks } else { 1 }) {
        if row[..channels].iter().any(|&v| v > 15) {
            return -1;
        }
    }
    let mut used = 0;
    for (i, p) in matrix.iter().enumerate().take(*primitives) {
        let target = p.target as usize;
        if target >= channels
            || p.bypass > 1
            || p.coefficient[target] != if p.bypass != 0 { -32768 } else { -16384 }
            || (i >= prefix && (p.coefficient[channels] != 0 || p.coefficient[channels + 1] != 0))
            || (p.bypass != 0
                && qss
                    .iter()
                    .take(if lengths.is_some() { blocks } else { 1 })
                    .any(|row| row[target] != 0))
        {
            return -1;
        }
        if i < prefix {
            used += p.bypass;
        }
    }
    if samples[..count * channels]
        .iter()
        .any(|&v| !(-8388608..=8388607).contains(&v))
        || (prefix != 0
            && bypass[..count]
                .iter()
                .any(|&v| u32::from(v) >= 1u32 << used))
    {
        return -1;
    }
    let mut work = samples[..count * channels].to_vec();
    let mut saved = vec![0; count];
    let mut bits = vec![0u8; count];
    if prefix != 0 {
        bits.copy_from_slice(&bypass[..count]);
    }
    let mut i = prefix;
    while i < *primitives {
        let target = matrix[i].target as usize;
        for (saved, frame) in saved.iter_mut().zip(work.chunks_exact(channels)) {
            *saved = frame[target];
        }
        let mut overflow = false;
        let mut first = 0;
        for b in 0..blocks {
            let mut one = [0u8; 160];
            let block = lengths.map_or((count - first).min(160), |v| v[b]);
            if block > count - first {
                return -1;
            }
            let current_qss = &qss[if lengths.is_some() { b } else { 0 }];
            let result = mlp_matrix_apply(MatrixApplyArgs {
                matrix: &matrix[i..i + 1],
                channels,
                qss: current_qss,
                samples: &mut work[first * channels..],
                count: block,
                extra0: None,
                extra1: None,
                bypass: Some(&mut one),
                inverse: false,
                clip: false,
            });
            if result < 0 {
                return -1;
            }
            if result != 0 {
                overflow = true;
                break;
            }
            if matrix[i].bypass != 0 {
                for (bits, one) in bits[first..first + block].iter_mut().zip(one) {
                    *bits |= one << used;
                }
            }
            first += block;
        }
        if overflow {
            for ((frame, &saved), bits) in
                work.chunks_exact_mut(channels).zip(&saved).zip(&mut bits)
            {
                frame[target] = saved;
                *bits &= ((1u32 << used) - 1) as u8;
            }
            break;
        }
        used += matrix[i].bypass;
        i += 1;
    }
    let rc = i32::from(i < *primitives);
    *primitives = i;
    *bypass_bits = used;
    samples[..count * channels].copy_from_slice(&work);
    bypass[..count].copy_from_slice(&bits);
    rc
}

pub(crate) fn mlp_matrix_apply_interval(mut args: MatrixIntervalArgs<'_>) -> i32 {
    args.lengths = None;
    args.blocks = args.count.div_ceil(160);
    apply_interval(args)
}

pub(crate) fn mlp_matrix_apply_blocks(args: MatrixIntervalArgs<'_>) -> i32 {
    mlp_matrix_apply_suffix_blocks(MatrixIntervalArgs { prefix: 0, ..args })
}

pub(crate) fn mlp_matrix_apply_suffix_blocks(mut args: MatrixIntervalArgs<'_>) -> i32 {
    let lengths = args.lengths.unwrap_or(&[]);
    if lengths.is_empty() || lengths.len() > 128 || lengths.iter().any(|&v| v == 0 || v > 160) {
        return -1;
    }
    args.count = lengths.iter().sum();
    args.lengths = Some(lengths);
    args.blocks = lengths.len();
    apply_interval(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_matrix_round_trip_preserves_samples() {
        let matrix = [MlpMatrixPrimitive {
            target: 0,
            bypass: 1,
            coefficient: [
                -32768, 16384, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        }];
        let original = [-17, 23, 19, -29, 0, 1, 1, 0];
        let mut samples = original;
        let mut bypass = [0; 4];
        for inverse in [false, true] {
            assert_eq!(
                mlp_matrix_apply(MatrixApplyArgs {
                    matrix: &matrix,
                    channels: 2,
                    qss: &[0, 0],
                    samples: &mut samples,
                    count: 4,
                    extra0: None,
                    extra1: None,
                    bypass: Some(&mut bypass),
                    inverse,
                    clip: false,
                }),
                0
            );
        }
        assert_eq!(samples, original);
        assert_eq!(bypass, [0; 4]);
    }

    #[test]
    fn interval_overflow_restores_entire_target_channel() {
        let matrix = [MlpMatrixPrimitive {
            target: 0,
            coefficient: [
                -16384, 32768, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            ..MlpMatrixPrimitive::default()
        }];
        let original = [1, 2, 0, 8388607];
        let mut samples = original;
        let mut primitives = 1;
        let mut bypass = [0; 2];
        let mut bypass_bits = 0;
        assert_eq!(
            mlp_matrix_apply_blocks(MatrixIntervalArgs {
                matrix: &matrix,
                primitives: &mut primitives,
                channels: 2,
                qss: &[[0; 6]; 2],
                samples: &mut samples,
                count: 2,
                bypass: &mut bypass,
                bypass_bits: &mut bypass_bits,
                lengths: Some(&[1, 1]),
                blocks: 2,
                prefix: 0,
            }),
            1
        );
        assert_eq!(samples, original);
        assert_eq!(primitives, 0);
        assert_eq!(bypass_bits, 0);
        assert_eq!(bypass, [0; 2]);
    }
}
