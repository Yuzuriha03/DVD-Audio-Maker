use crate::extended::Extended as E;
use crate::matrix::MlpMatrixCandidate;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct MlpDownmixDesign {
    pub forward: [MlpMatrixCandidate; 2],
    pub inverse: [MlpMatrixCandidate; 2],
    pub permutation: [u32; 6],
    pub post_shift: [u32; 6],
    pub required_headroom: [u32; 6],
}

#[allow(
    clippy::needless_range_loop,
    reason = "Preserve C row and pivot indexing"
)]
pub(crate) fn mlp_matrix_design_downmix(
    coefficients: &[[f64; 6]; 2],
    precision: u32,
) -> Result<MlpDownmixDesign, ()> {
    if precision > 14
        || coefficients
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 16.0)
    {
        return Err(());
    }
    let mut result = MlpDownmixDesign::default();
    let mut forward = [[0.0f64; 6]; 2];
    let mut inverse = [[0.0f64; 2]; 2];
    let mut basis = [[0.0f64; 6]; 2];
    let mut maximum = 0;
    let grid = f64::from(1u32 << precision);
    let step = 1.0 / grid;
    result.permutation = [0, 1, 2, 3, 4, 5];
    for r in 0..2 {
        let mut vector =
            std::array::from_fn::<_, 6, _>(|j| coefficients[r][result.permutation[j] as usize]);
        let mut largest = -1.0;
        let mut eligible = [true; 6];
        for p in 0..r {
            let factor = E::from_f64(vector[p]).div(E::from_f64(forward[p][p]));
            for (j, v) in vector.iter_mut().enumerate() {
                if j != p {
                    *v = E::from_f64(*v)
                        .sub(factor.mul(E::from_f64(forward[p][j])))
                        .to_f64();
                }
            }
            if forward[p][p] < 0.0 {
                vector[p] = -vector[p];
            }
            eligible[p] = false;
        }
        let mut pivot = 0;
        for j in 0..6 {
            if eligible[j] && vector[j].abs() > largest {
                largest = vector[j].abs();
                pivot = j;
            }
        }
        let factor = if vector[pivot] != 0.0 {
            E::from_f64(-1.0).div(E::from_f64(vector[pivot]))
        } else {
            E::from_f64(1.0)
        };
        for j in 0..6 {
            forward[r][j] = if eligible[j] {
                factor.mul(E::from_f64(vector[j])).to_f64()
            } else {
                0.0
            };
        }
        forward[r][pivot] = -1.0;
        for j in 0..2 {
            inverse[r][j] = if eligible[j] { 0.0 } else { vector[j] };
        }
        inverse[r][r] = -vector[pivot];
        if pivot != r {
            result.permutation.swap(r, pivot);
            for row in forward.iter_mut().take(r + 1) {
                row.swap(r, pivot);
            }
        }
        let mut identity = true;
        for j in 0..6 {
            forward[r][j] = E::from_f64(grid)
                .mul(E::from_f64(forward[r][j]))
                .add(E::from_f64(0.5))
                .to_f64()
                .floor()
                * step;
            if j != r {
                if forward[r][j] <= -1.0 {
                    forward[r][j] = step - 1.0;
                }
                if forward[r][j] >= 1.0 {
                    forward[r][j] = 1.0 - step;
                }
            }
            if forward[r][j] != if j == r { -1.0 } else { 0.0 } {
                identity = false;
            }
        }
        if identity {
            forward[r][r] = 1.0;
            inverse[r][r] = -inverse[r][r];
        }
    }
    basis[0][0] = 1.0;
    basis[1][1] = 1.0;
    for r in 0..2 {
        let mut vector = [0.0; 6];
        let mut sum = E::from_f64(0.0);
        for j in 0..6 {
            vector[j] = E::from_f64(basis[0][j])
                .mul(E::from_f64(forward[r][0]))
                .add(E::from_f64(basis[1][j]).mul(E::from_f64(forward[r][1])))
                .to_f64();
            if j >= 2 {
                vector[j] += forward[r][j];
            }
            sum = sum.add(E::from_f64(vector[j].abs()));
        }
        basis[r] = vector;
        while E::from_f64(0.99).less(sum) {
            sum = sum.mul(E::from_f64(0.5));
            result.required_headroom[r] += 1;
        }
        maximum = maximum.max(result.required_headroom[r]);
    }
    for r in 0..2 {
        if maximum != 0 && result.required_headroom[r] < maximum - 1 {
            result.required_headroom[r] = maximum - 1;
        }
    }
    for target in (0..2).rev() {
        let mut vector = [0.0; 6];
        let mut sum = E::from_f64(0.0);
        let mut largest = E::from_f64(inverse[target][0].abs().max(inverse[target][1].abs()));
        for j in 0..6 {
            vector[j] = E::from_f64(basis[0][j])
                .mul(E::from_f64(inverse[target][0]))
                .add(E::from_f64(basis[1][j]).mul(E::from_f64(inverse[target][1])))
                .to_f64();
            sum = sum.add(E::from_f64(vector[j].abs()));
        }
        basis[target] = vector;
        while E::from_f64(0.99).less(sum) || E::from_f64(1.9).less(largest) {
            sum = sum.mul(E::from_f64(0.5));
            largest = largest.mul(E::from_f64(0.5));
            result.post_shift[target] += 1;
        }
        if result.post_shift[target] > 15 {
            return Err(());
        }
        let divisor = f64::from(1u32 << result.post_shift[target]);
        inverse[target][0] /= divisor;
        inverse[target][1] /= divisor;
        for row in inverse.iter_mut().take(target) {
            row[target] *= divisor;
        }
    }
    for r in 0..2 {
        result.forward[r].target = r as u32;
        result.inverse[1 - r].target = r as u32;
        for j in 0..6 {
            result.forward[r].coefficient[j] = (forward[r][j] * 16384.0) as i32;
        }
        for j in 0..2 {
            let value = E::from_f64(grid)
                .mul(E::from_f64(inverse[r][j]))
                .add(E::from_f64(0.5))
                .to_f64()
                .floor()
                * f64::from(1u32 << (14 - precision));
            if value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
                return Err(());
            }
            result.inverse[1 - r].coefficient[j] = value as i32;
        }
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments, reason = "Preserve C rendering boundary")]
pub(crate) fn mlp_matrix_render_stereo(
    matrix: &[crate::matrix::MlpMatrixPrimitive],
    qss: &[u32; 2],
    samples: &[i32],
    count: usize,
    stride: usize,
    extra0: &[i32],
    extra1: &[i32],
) -> Result<(Vec<f32>, bool), ()> {
    if matrix.len() > 6
        || qss.iter().any(|&v| v > 15)
        || count > 160
        || !(2..=6).contains(&stride)
        || samples.len() < count * stride
        || extra0.len() < count
        || extra1.len() < count
    {
        return Err(());
    }
    for m in matrix {
        if m.target > 1 || m.bypass != 0 || m.coefficient[4..].iter().any(|&v| v != 0) {
            return Err(());
        }
    }
    let mut work = vec![0.0f32; count * 2];
    let mut noise = [vec![0.0f32; count], vec![0.0f32; count]];
    for n in 0..count {
        for ch in 0..2 {
            let v = samples[n * stride + ch];
            if !(-8388608..=8388607).contains(&v) {
                return Err(());
            }
            work[n * 2 + ch] = v as f32;
        }
        noise[0][n] = extra0[n] as f32;
        noise[1][n] = extra1[n] as f32;
    }
    let mut overflow = false;
    for m in matrix.iter().rev() {
        let step = f64::from(1u32 << qss[m.target as usize]);
        let coefficient: [f64; 4] =
            std::array::from_fn(|ch| f64::from(m.coefficient[ch]) / (16384.0 * step));
        for n in 0..count {
            let mut sum = E::from_f64(f64::from(noise[1][n])).mul(E::from_f64(coefficient[3]));
            sum = sum.add(E::from_f64(f64::from(work[n * 2 + 1])).mul(E::from_f64(coefficient[1])));
            sum = sum.add(E::from_f64(f64::from(work[n * 2])).mul(E::from_f64(coefficient[0])));
            sum = sum.add(E::from_f64(f64::from(noise[0][n])).mul(E::from_f64(coefficient[2])));
            let value = E::from_f64(sum.to_f64().floor()).mul(E::from_f64(step));
            if value.less(E::from_f64(-8388608.0)) || !value.less(E::from_f64(8388608.0)) {
                overflow = true;
            }
            work[n * 2 + m.target as usize] = value.to_f64() as f32;
        }
    }
    Ok((work, overflow))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct MlpDownmixCheck {
    pub checksum: u32,
    pub maximum_bits: u32,
}

pub(crate) fn mlp_matrix_downmix_pcm(
    state: &mut MlpDownmixCheck,
    samples: &[f32],
    count: usize,
    channels: usize,
    output_shift: &[u32],
) -> Result<(Vec<i32>, bool), ()> {
    if state.maximum_bits > 32
        || !(1..=6).contains(&channels)
        || count > 160
        || samples.len() < count * channels
        || output_shift.len() < channels
    {
        return Err(());
    }
    let mut next = *state;
    let mut output = vec![0i32; count * channels];
    let mut minimum = 0i32;
    let mut maximum = 0i32;
    for ch in 0..channels {
        let mut checksum = 0u32;
        if output_shift[ch] > 31 {
            return Err(());
        }
        for n in 0..count {
            let sample = f64::from(samples[n * channels + ch]);
            if !sample.is_finite() || sample < f64::from(i32::MIN) || sample > f64::from(i32::MAX) {
                return Err(());
            }
            let shifted = (sample as i32 as u32) << output_shift[ch];
            if shifted == 0x80000000 {
                return Err(());
            }
            let value = shifted as i32;
            checksum ^= shifted;
            maximum = maximum.max(value);
            minimum = minimum.min(value);
            output[n * channels + ch] = value.clamp(-8388608, 8388607);
        }
        next.checksum ^= (checksum & 0xffffff) << ch;
    }
    maximum = maximum.max(-minimum);
    let mut bits = next.maximum_bits.saturating_sub(1);
    while (maximum as u32 >> bits) != 0 {
        bits += 1;
    }
    next.maximum_bits = bits + 1;
    *state = next;
    Ok((output, minimum < -8388608 || maximum >= 8388608))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct MlpDownmixPlan {
    pub forward: [crate::matrix::MlpMatrixPrimitive; 16],
    pub inverse: [crate::matrix::MlpMatrixPrimitive; 7],
    pub forward_count: u32,
    pub inverse_count: u32,
    pub bypass_bits: u32,
    pub forward_noise_shift: u32,
    pub inverse_noise_shift: u32,
    pub remaining_scale: [u32; 6],
    pub output_shift: [u32; 6],
    pub maximum_shift: i32,
}

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve C downmix constructor boundary"
)]
pub(crate) fn mlp_matrix_downmix_plan(
    forward: &[MlpMatrixCandidate],
    inverse: &[MlpMatrixCandidate],
    channels: usize,
    scale_count: &[u32; 6],
    shifts: &[u32; 6],
    qss: &[u32; 6],
    post_shift: &[u32; 6],
) -> Result<MlpDownmixPlan, ()> {
    use crate::matrix::*;
    let candidates = forward.len();
    if !(1..=6).contains(&channels)
        || candidates > 2
        || candidates > channels
        || inverse.len() != candidates
        || (0..channels)
            .any(|i| shifts[i] > 15 || qss[i] > 15 || post_shift[i] > 15 || scale_count[i] > 6)
        || forward
            .iter()
            .chain(inverse)
            .any(|p| p.target as usize >= candidates)
    {
        return Err(());
    }
    let mut work = MlpDownmixPlan::default();
    let mut dimensions = [0u32; 6];
    let mut final_shifts = [0u32; 6];
    let mut noise = [0u32; 6];
    let mut signs = [0i32; 6];
    let mut forward_buffer = [MlpMatrixPrimitive::default(); 15];
    if mlp_matrix_build_forward(
        forward,
        channels,
        scale_count,
        shifts,
        &mut forward_buffer,
        &mut work.forward_count,
        &mut work.bypass_bits,
        &mut signs,
        &mut dimensions,
    ) != 0
        || work.forward_count > 6
    {
        return Err(());
    }
    work.forward[..15].copy_from_slice(&forward_buffer);
    work.remaining_scale = *scale_count;
    for p in &work.forward[..work.forward_count as usize] {
        if p.bypass != 0 {
            work.remaining_scale[p.target as usize] =
                work.remaining_scale[p.target as usize].wrapping_sub(1);
        }
    }
    for i in 0..channels {
        noise[i] = qss[i].wrapping_add(dimensions[i]).wrapping_sub(shifts[i]);
        if noise[i] > 31 {
            return Err(());
        }
    }
    work.forward_noise_shift = noise[0].saturating_sub(8);
    work.inverse_noise_shift = qss[0].saturating_sub(8);
    work.maximum_shift = -8;
    if candidates != 0 {
        if mlp_matrix_add_dither(
            &mut work.forward,
            work.forward_count as usize,
            candidates,
            channels,
            &noise,
            &mut work.forward_noise_shift,
        ) != 0
        {
            return Err(());
        }
        let mut inverse_buffer = [MlpMatrixPrimitive::default(); 6];
        if mlp_matrix_build_reverse(
            inverse,
            candidates,
            &signs,
            &dimensions,
            &mut inverse_buffer,
            &mut final_shifts,
        ) != 0
        {
            return Err(());
        }
        work.inverse[..6].copy_from_slice(&inverse_buffer);
        work.inverse_count = candidates as u32;
        if mlp_matrix_add_dither(
            &mut work.inverse,
            candidates,
            candidates,
            candidates,
            qss,
            &mut work.inverse_noise_shift,
        ) != 0
        {
            return Err(());
        }
        for i in 0..candidates {
            work.output_shift[i] = final_shifts[i] + post_shift[i];
            work.maximum_shift = work.maximum_shift.max(work.output_shift[i] as i32);
        }
    }
    Ok(work)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct MlpDownmixState {
    pub forward_seed: u32,
    pub inverse_seed: u32,
    pub check: MlpDownmixCheck,
}

#[derive(Clone, Debug, PartialEq)]
#[repr(C)]
pub(crate) struct MlpDownmixBlock {
    pub transformed: [i32; 960],
    pub bypass: [u8; 160],
    pub stereo: [f32; 320],
    pub forward_seed: u32,
    pub inverse_seed: u32,
    pub flags: u32,
}
impl Default for MlpDownmixBlock {
    fn default() -> Self {
        Self {
            transformed: [0; 960],
            bypass: [0; 160],
            stereo: [0.0; 320],
            forward_seed: 0,
            inverse_seed: 0,
            flags: 0,
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve C downmix processing boundary"
)]
pub(crate) fn mlp_matrix_process_downmix(
    state: &mut MlpDownmixState,
    plan: &MlpDownmixPlan,
    channels: usize,
    shifts: &[u32; 6],
    qss: &[u32; 6],
    samples: &[i32],
    count: usize,
    flags: u32,
) -> Result<MlpDownmixBlock, ()> {
    use crate::matrix::*;
    if !(2..=6).contains(&channels)
        || !(1..=160).contains(&count)
        || samples.len() < count * channels
        || plan.forward_count > 6
        || plan.inverse_count != 2
        || plan.forward_noise_shift > 16
        || plan.inverse_noise_shift > 16
        || state.check.maximum_bits > 32
        || (0..channels).any(|ch| shifts[ch] > 15 || qss[ch] > 15)
    {
        return Err(());
    }
    let mut bits = 0;
    for m in &plan.forward[..plan.forward_count as usize] {
        if m.target as usize >= channels
            || m.bypass > 1
            || m.coefficient[m.target as usize] != if m.bypass != 0 { -32768 } else { -16384 }
            || (m.bypass != 0 && qss[m.target as usize] != 0)
        {
            return Err(());
        }
        for ch in 0..18 {
            if !(-32768..=32767).contains(&m.coefficient[ch])
                || (ch >= channels + 2 && m.coefficient[ch] != 0)
            {
                return Err(());
            }
        }
        bits += m.bypass;
    }
    if bits != plan.bypass_bits
        || plan.inverse[..2]
            .iter()
            .any(|m| m.coefficient.iter().any(|v| !(-32768..=32767).contains(v)))
    {
        return Err(());
    }
    let mut work = MlpDownmixBlock::default();
    for n in 0..count {
        for ch in 0..channels {
            let v = samples[n * channels + ch];
            if !(-8388608..=8388607).contains(&v) {
                return Err(());
            }
            work.transformed[n * channels + ch] = v >> shifts[ch];
        }
    }
    let mut next = *state;
    work.flags = flags;
    work.forward_seed = next.forward_seed;
    work.inverse_seed = next.inverse_seed;
    let mut noise0 = [0i32; 160];
    let mut noise1 = [0i32; 160];
    if mlp_matrix_noise(
        &mut next.forward_seed,
        count,
        plan.forward_noise_shift,
        &mut noise0,
        &mut noise1,
    ) != 0
    {
        return Err(());
    }
    for m in &plan.forward[..plan.forward_count as usize] {
        let mut one = [0u8; 160];
        let rc = mlp_matrix_apply(
            std::slice::from_ref(m),
            channels,
            qss,
            &mut work.transformed,
            count,
            Some(&noise0),
            Some(&noise1),
            Some(&mut one),
            false,
            true,
        );
        if rc < 0 {
            return Err(());
        }
        if m.bypass != 0 {
            for (n, v) in one.iter().enumerate().take(count) {
                work.bypass[n] = (*v << 7) | (work.bypass[n] >> 1);
            }
        }
        if rc != 0 {
            work.flags |= 0x200;
            break;
        }
    }
    if mlp_matrix_noise(
        &mut next.inverse_seed,
        count,
        plan.inverse_noise_shift,
        &mut noise0,
        &mut noise1,
    ) != 0
    {
        return Err(());
    }
    let (stereo, overflow) = mlp_matrix_render_stereo(
        &plan.inverse[..2],
        &[qss[0], qss[1]],
        &work.transformed,
        count,
        channels,
        &noise0,
        &noise1,
    )?;
    work.stereo[..count * 2].copy_from_slice(&stereo);
    if overflow {
        work.flags |= 0x200;
    }
    if flags & 2 != 0 {
        next.check = MlpDownmixCheck::default();
    }
    let (_, overflow) =
        mlp_matrix_downmix_pcm(&mut next.check, &stereo, count, 2, &plan.output_shift)?;
    if overflow {
        work.flags |= 0x400;
    }
    *state = next;
    Ok(work)
}
