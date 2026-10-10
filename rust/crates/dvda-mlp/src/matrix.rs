#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct MlpMatrixPrimitive {
    pub target: u32,
    pub bypass: u32,
    pub coefficient: [i32; 18],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
#[cfg(test)]
pub(crate) struct MlpMatrixCandidate {
    pub target: u32,
    pub coefficient: [i32; 8],
}

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve the C constructor function boundary"
)]
#[cfg(test)]
pub(crate) fn mlp_matrix_build_forward(
    candidate: &[MlpMatrixCandidate],
    channels: usize,
    scale_count: &[u32; 6],
    shifts: &[u32; 6],
    output: &mut [MlpMatrixPrimitive; 15],
    primitives: &mut u32,
    bypass_bits: &mut u32,
    signs: &mut [i32; 6],
    final_shifts: &mut [u32; 6],
) -> i32 {
    if channels == 0
        || channels > 6
        || candidate.len() > channels
        || (0..channels).any(|i| scale_count[i] > 8 || shifts[i] > 31)
        || candidate.iter().any(|p| p.target as usize >= channels)
    {
        return -1;
    }
    let mut work = [MlpMatrixPrimitive::default(); 15];
    let mut counts = *scale_count;
    let mut dims = *shifts;
    let mut count = 0;
    let mut bits = 0;
    let mut polarity = [1i32; 6];
    for i in 0..candidate.len() {
        while counts[i] > 1 {
            if count == 15 || bits == 8 || dims[i] == 31 {
                return -1;
            }
            work[count].target = i as u32;
            work[count].bypass = 1;
            work[count].coefficient[i] = -32768;
            count += 1;
            polarity[i] = -polarity[i];
            dims[i] += 1;
            counts[i] -= 1;
            bits += 1;
        }
    }
    for p in candidate {
        let target = p.target as usize;
        let bypass = counts[target] > 0;
        if p.coefficient[target] == 16384 && !bypass {
            continue;
        }
        if count == 15 || (bypass && (bits == 8 || dims[target] == 31)) {
            return -1;
        }
        work[count].target = target as u32;
        work[count].bypass = u32::from(bypass);
        let bias = (((1u32 << dims[target]) as i32) >> 1) as u32;
        for j in 0..channels {
            let v = (p.coefficient[j] as u32)
                .wrapping_mul(polarity[j] as u32)
                .wrapping_mul(polarity[target] as u32)
                .wrapping_shl(dims[j])
                .wrapping_add(bias);
            work[count].coefficient[j] = (v as i32) >> dims[target];
        }
        if work[count].coefficient[target] == 16384 {
            polarity[target] = -polarity[target];
        }
        work[count].coefficient[target] = if bypass { -32768 } else { -16384 };
        if bypass {
            dims[target] += 1;
            counts[target] -= 1;
            bits += 1;
        }
        count += 1;
    }
    *output = work;
    *signs = polarity;
    *final_shifts = dims;
    *primitives = count as u32;
    *bypass_bits = bits;
    0
}

#[cfg(test)]
pub(crate) fn mlp_matrix_build_reverse(
    candidate: &[MlpMatrixCandidate],
    channels: usize,
    signs: &[i32; 6],
    shifts: &[u32; 6],
    output: &mut [MlpMatrixPrimitive; 6],
    final_shifts: &mut [u32; 6],
) -> i32 {
    if channels == 0 || channels > 6 || candidate.len() < channels {
        return -1;
    }
    let mut work = [MlpMatrixPrimitive::default(); 6];
    let mut dims = *shifts;
    let mut polarity = *signs;
    let common = shifts[0].max(shifts[1]);
    if common > 31
        || (0..channels).any(|i| {
            dims[i] > 31
                || ![-1, 1].contains(&polarity[i])
                || candidate[i].target as usize >= channels
        })
    {
        return -1;
    }
    for i in 0..channels {
        let target = candidate[i].target as usize;
        let p = &mut work[channels - 1 - i];
        p.target = target as u32;
        for j in 0..channels {
            let mut v = (candidate[i].coefficient[j] as u32).wrapping_mul(polarity[j] as u32);
            if common > dims[j] {
                v = (((v as i32) >> (common - dims[j] - 1)) as u32).wrapping_add(1);
                v = ((v as i32) >> 1) as u32;
            }
            p.coefficient[j] = v as i32;
        }
        polarity[target] = 1;
        dims[target] = common;
    }
    *output = work;
    *final_shifts = dims;
    0
}

#[cfg(test)]
pub(crate) fn mlp_matrix_add_dither(
    matrix: &mut [MlpMatrixPrimitive],
    primitives: usize,
    targets: usize,
    extra_source: usize,
    dimensions: &[u32; 6],
    noise_shift: &mut u32,
) -> i32 {
    if primitives > 15
        || matrix.len() <= primitives
        || targets == 0
        || targets > 6
        || extra_source < targets
        || extra_source > 6
        || dimensions[..targets].iter().any(|&v| v > 31)
    {
        return -1;
    }
    let mut common = dimensions[0];
    if targets > 1 && dimensions[1] > common {
        common = dimensions[1];
    }
    let shift = common.saturating_sub(8);
    let mut work = matrix[..=primitives].to_vec();
    for (i, &dimension) in dimensions.iter().enumerate().take(targets) {
        let amplitude = 1u32 << (dimension.wrapping_sub(shift).wrapping_add(6) & 31);
        let k = work[..primitives]
            .iter()
            .position(|p| p.target as usize == i)
            .unwrap_or(primitives);
        work[k].coefficient[extra_source] = (if i != 0 {
            amplitude.wrapping_neg()
        } else {
            amplitude
        }) as i32;
        work[k].coefficient[extra_source + 1] = amplitude as i32;
    }
    matrix[..=primitives].copy_from_slice(&work);
    *noise_shift = shift;
    0
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
    for n in 0..count {
        let t = ((state as i32) >> 7) as u32;
        let a = ((t >> 8) as u8 as i8) as i32;
        let b = (t as u8 as i8) as i32;
        state = ((((state & 127) << 11) ^ t) << 5) ^ t;
        first[n] = (a as u32).wrapping_shl(shift) as i32;
        second[n] = (b as u32).wrapping_shl(shift) as i32;
    }
    *seed = state;
    0
}

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve the C matrix function boundary"
)]
pub(crate) fn mlp_matrix_apply(
    matrix: &[MlpMatrixPrimitive],
    channels: usize,
    qss: &[u32],
    samples: &mut [i32],
    count: usize,
    extra0: Option<&[i32]>,
    extra1: Option<&[i32]>,
    mut bypass: Option<&mut [u8]>,
    inverse: bool,
    clip: bool,
) -> i32 {
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
        let step = (1u32 << qss[target]) as f64;
        for n in 0..count {
            let mut sum = 0.0;
            for j in 0..channels {
                sum += work[n * channels + j] as f64 * (p.coefficient[j] as f64 / 16384.0);
            }
            if let Some(extra) = extra0 {
                sum += extra[n] as f64 * (p.coefficient[channels] as f64 / 16384.0);
            }
            if let Some(extra) = extra1 {
                sum += extra[n] as f64 * (p.coefficient[channels + 1] as f64 / 16384.0);
            }
            let mut value;
            if p.bypass != 0 {
                if inverse {
                    value = sum.floor() + ((bits[n] >> (remaining - 1)) & 1) as f64;
                    bits[n] &= ((1u32 << (remaining - 1)) - 1) as u8;
                } else {
                    value = (sum + work[n * channels + target] as f64).floor();
                    if !value.is_finite()
                        || value < i32::MIN as f64
                        || value > i32::MAX as f64 - 1.0
                    {
                        return -1;
                    }
                    let rounded = value as i64;
                    bits[n] = (((rounded as u32 & 1) * 128) + (bits[n] >> 1) as u32) as u8;
                    value = ((value + 1.0) / 2.0).floor();
                }
            } else {
                value = (sum / step).floor() * step;
            }
            if !value.is_finite() || value < i32::MIN as f64 || value > i32::MAX as f64 {
                return -1;
            }
            if !(-8388608.0..=8388607.0).contains(&value) {
                overflow = 1;
                if clip {
                    value = if value < 0.0 { -8388608.0 } else { 8388607.0 };
                }
            }
            work[n * channels + target] = value as i32;
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

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve the C interval matrix function boundary"
)]
fn apply_interval(
    matrix: &[MlpMatrixPrimitive],
    primitives: &mut usize,
    channels: usize,
    qss: &[[u32; 6]],
    samples: &mut [i32],
    count: usize,
    bypass: &mut [u8],
    bypass_bits: &mut u32,
    lengths: Option<&[usize]>,
    blocks: usize,
    prefix: usize,
) -> i32 {
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
        || (prefix != 0 && bypass[..count].iter().any(|&v| v as u32 >= 1u32 << used))
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
        for n in 0..count {
            saved[n] = work[n * channels + target];
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
            let result = mlp_matrix_apply(
                &matrix[i..i + 1],
                channels,
                current_qss,
                &mut work[first * channels..],
                block,
                None,
                None,
                Some(&mut one),
                false,
                false,
            );
            if result < 0 {
                return -1;
            }
            if result != 0 {
                overflow = true;
                break;
            }
            if matrix[i].bypass != 0 {
                for n in 0..block {
                    bits[first + n] |= one[n] << used;
                }
            }
            first += block;
        }
        if overflow {
            for n in 0..count {
                work[n * channels + target] = saved[n];
                bits[n] &= ((1u32 << used) - 1) as u8;
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

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve the C matrix function boundary"
)]
pub(crate) fn mlp_matrix_apply_interval(
    matrix: &[MlpMatrixPrimitive],
    primitives: &mut usize,
    channels: usize,
    qss: &[u32; 6],
    samples: &mut [i32],
    count: usize,
    bypass: &mut [u8],
    bypass_bits: &mut u32,
) -> i32 {
    apply_interval(
        matrix,
        primitives,
        channels,
        std::slice::from_ref(qss),
        samples,
        count,
        bypass,
        bypass_bits,
        None,
        count.div_ceil(160),
        0,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve the C matrix function boundary"
)]
pub(crate) fn mlp_matrix_apply_blocks(
    matrix: &[MlpMatrixPrimitive],
    primitives: &mut usize,
    channels: usize,
    qss: &[[u32; 6]],
    lengths: &[usize],
    samples: &mut [i32],
    bypass: &mut [u8],
    bypass_bits: &mut u32,
) -> i32 {
    mlp_matrix_apply_suffix_blocks(
        matrix,
        0,
        primitives,
        channels,
        qss,
        lengths,
        samples,
        bypass,
        bypass_bits,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve the C matrix function boundary"
)]
pub(crate) fn mlp_matrix_apply_suffix_blocks(
    matrix: &[MlpMatrixPrimitive],
    prefix: usize,
    primitives: &mut usize,
    channels: usize,
    qss: &[[u32; 6]],
    lengths: &[usize],
    samples: &mut [i32],
    bypass: &mut [u8],
    bypass_bits: &mut u32,
) -> i32 {
    if lengths.is_empty() || lengths.len() > 128 || lengths.iter().any(|&v| v == 0 || v > 160) {
        return -1;
    }
    let count = lengths.iter().sum();
    apply_interval(
        matrix,
        primitives,
        channels,
        qss,
        samples,
        count,
        bypass,
        bypass_bits,
        Some(lengths),
        lengths.len(),
        prefix,
    )
}
