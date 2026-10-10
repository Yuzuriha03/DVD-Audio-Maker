use crate::extended::Extended as E;

#[derive(Clone, Debug, PartialEq)]
#[repr(C)]
pub(crate) struct MlpMatrixAnalysis {
    pub channels: u32,
    pub samples: usize,
    pub last: [f64; 6],
    pub difference: [f64; 6],
    pub energy: [f64; 6],
    pub covariance: [f64; 36],
}
impl Default for MlpMatrixAnalysis {
    fn default() -> Self {
        Self {
            channels: 0,
            samples: 0,
            last: [0.0; 6],
            difference: [0.0; 6],
            energy: [0.0; 6],
            covariance: [0.0; 36],
        }
    }
}
pub(crate) fn mlp_matrix_analysis_init(
    state: &mut MlpMatrixAnalysis,
    channels: u32,
) -> Result<(), ()> {
    if !(1..=6).contains(&channels) {
        return Err(());
    }
    *state = MlpMatrixAnalysis {
        channels,
        ..Default::default()
    };
    state.energy[..channels as usize].fill(1.0);
    Ok(())
}
pub(crate) fn mlp_matrix_analysis_reset(state: &mut MlpMatrixAnalysis) -> Result<(), ()> {
    if !(1..=6).contains(&state.channels) {
        return Err(());
    }
    state.samples = 0;
    state.covariance.fill(0.0);
    state.energy[..state.channels as usize].fill(1.0);
    Ok(())
}
#[allow(
    clippy::assign_op_pattern,
    reason = "Preserve C floating-point operand order"
)]
pub(crate) fn mlp_matrix_analysis_add(
    state: &mut MlpMatrixAnalysis,
    samples: &[i32],
    count: usize,
    stride: usize,
) -> Result<(), ()> {
    if !(1..=6).contains(&state.channels)
        || !(1..=160).contains(&count)
        || stride < state.channels as usize
        || stride > 6
        || samples.len() < count * stride
        || state.samples.checked_add(count).is_none()
    {
        return Err(());
    }
    let mut work = state.clone();
    let mut difference = [0.0; 960];
    for ch in 0..work.channels as usize {
        let mut energy = 0.0;
        let mut last = work.last[ch];
        let mut delta = work.difference[ch];
        if !last.is_finite() || !delta.is_finite() || !work.energy[ch].is_finite() {
            return Err(());
        }
        for n in 0..count {
            let v = samples[n * stride + ch];
            if !(-8388608..=8388607).contains(&v) {
                return Err(());
            }
            let next = f64::from(v) - last;
            difference[n * 6 + ch] = next - delta;
            last = f64::from(v);
            delta = next;
            energy = E::from_f64(last)
                .mul(E::from_f64(last))
                .add(E::from_f64(energy))
                .to_f64();
        }
        work.last[ch] = last;
        work.difference[ch] = delta;
        work.energy[ch] = energy + work.energy[ch];
    }
    for ch in 0..work.channels as usize {
        for j in ch..work.channels as usize {
            let mut dot = E::from_f64(0.0);
            for n in 0..count {
                dot = dot.add(
                    E::from_f64(difference[n * 6 + ch]).mul(E::from_f64(difference[n * 6 + j])),
                );
            }
            let value = dot.add(E::from_f64(work.covariance[ch * 6 + j])).to_f64();
            if !value.is_finite() {
                return Err(());
            }
            work.covariance[ch * 6 + j] = value;
            work.covariance[j * 6 + ch] = value;
        }
    }
    work.samples += count;
    *state = work;
    Ok(())
}

type Decorrelation = ([u32; 6], [f64; 36], [f64; 36]);

pub(crate) fn mlp_matrix_decorrelate(
    covariance: &[f64; 36],
    ratio: &[f64; 6],
    channels: usize,
    fixed: usize,
    threshold: f64,
) -> Result<Decorrelation, ()> {
    if !(1..=6).contains(&channels) || fixed > channels || !threshold.is_finite() || threshold < 0.0
    {
        return Err(());
    }
    let mut c = *covariance;
    let mut t = [0.0; 36];
    let mut sequence = [0u32; 6];
    for i in 0..channels {
        if !ratio[i].is_finite() || ratio[i] < 0.0 || c[i * 7] < 0.0 {
            return Err(());
        }
        t[i * 7] = 1.0;
        if c[i * 6..i * 6 + channels].iter().any(|v| !v.is_finite()) {
            return Err(());
        }
    }
    for pass in 0..2 {
        let mut visited = [false; 6];
        for (i, entry) in sequence.iter_mut().enumerate().take(channels) {
            let mut pivot = i;
            if pass == 0 {
                if i >= fixed {
                    let mut maximum = -1e30;
                    for j in 0..channels {
                        if !visited[j] && maximum < c[j * 7] {
                            maximum = c[j * 7];
                            pivot = j;
                        }
                    }
                }
                *entry = pivot as u32;
            } else {
                pivot = *entry as usize;
            }
            let diagonal = c[pivot * 7];
            visited[pivot] = true;
            if diagonal <= threshold {
                continue;
            }
            for j in fixed..channels {
                if visited[j]
                    || c[pivot * 6 + j] == 0.0
                    || !E::from_f64(ratio[pivot])
                        .less(E::from_f64(ratio[j]).mul(E::from_f64(100.0)))
                    || !E::from_f64(ratio[j])
                        .less(E::from_f64(ratio[pivot]).mul(E::from_f64(1000.0)))
                {
                    continue;
                }
                let factor = E::from_f64(c[pivot * 6 + j])
                    .neg()
                    .div(E::from_f64(diagonal));
                let gain = factor
                    .mul(E::from_f64(c[j * 6 + pivot]))
                    .div(E::from_f64(c[j * 7]))
                    .neg();
                if !E::tenth().less(gain) {
                    continue;
                }
                let factor = E::from_f64(factor.to_f64());
                for k in 0..channels {
                    c[k * 6 + j] = factor
                        .mul(E::from_f64(c[k * 6 + pivot]))
                        .add(E::from_f64(c[k * 6 + j]))
                        .to_f64();
                }
                c[pivot * 6 + j] = 0.0;
                for k in 0..channels {
                    c[j * 6 + k] = factor
                        .mul(E::from_f64(c[pivot * 6 + k]))
                        .add(E::from_f64(c[j * 6 + k]))
                        .to_f64();
                }
                c[j * 6 + pivot] = 0.0;
                for k in 0..channels {
                    t[j * 6 + k] = factor
                        .mul(E::from_f64(t[pivot * 6 + k]))
                        .add(E::from_f64(t[j * 6 + k]))
                        .to_f64();
                }
            }
        }
    }
    if t.iter().chain(&c).any(|v| !v.is_finite()) {
        return Err(());
    }
    Ok((sequence, t, c))
}

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve the C function boundary"
)]
pub(crate) fn mlp_matrix_select_append(
    covariance: &[f64; 36],
    ratio: &[f64; 6],
    channels: usize,
    fixed: usize,
    samples: usize,
    qss: &[u32; 6],
    scale_count: &[u32; 6],
    allow_bypass: bool,
    matrix: &mut [crate::matrix::MlpMatrixPrimitive; 6],
    primitives: &mut usize,
    bypass_bits: &mut u32,
) -> Result<(), ()> {
    use crate::matrix::MlpMatrixPrimitive;
    if samples == 0
        || samples > i32::MAX as usize
        || !(1..=6).contains(&channels)
        || *primitives > 6
    {
        return Err(());
    }
    let mut work = [MlpMatrixPrimitive::default(); 6];
    let mut count = *primitives;
    let mut bits = 0;
    for i in 0..count {
        let p = matrix[i];
        if p.target as usize >= channels
            || p.bypass > 1
            || p.coefficient[p.target as usize] != if p.bypass != 0 { -32768 } else { -16384 }
            || (p.bypass != 0 && qss[p.target as usize] != 0)
            || p.coefficient[8..].iter().any(|&v| v != 0)
        {
            return Err(());
        }
        bits += p.bypass;
        work[i] = p;
    }
    if bits != *bypass_bits || (0..channels).any(|i| qss[i] > 15 || scale_count[i] > 8) {
        return Err(());
    }
    let (order, transform, reduced) =
        mlp_matrix_decorrelate(covariance, ratio, channels, fixed, 10.0)?;
    for i in (0..channels).rev() {
        if count == 6 {
            break;
        }
        let target = order[i] as usize;
        if target < fixed || covariance[target * 7] <= 10.0 {
            continue;
        }
        let relative = E::from_f64(reduced[target * 7])
            .div(E::from_f64(covariance[target * 7]))
            .add(E::from_f64(1e-10));
        if !E::from_f64(0.0).less(relative) {
            return Err(());
        }
        let gain = relative.log().neg().div(E::from_f64(4.0).log());
        if !gain.finite() {
            return Err(());
        }
        let bypass = allow_bypass
            && qss[target] == 0
            && scale_count[target] != 0
            && gain.less(E::from_f64(2.0));
        let mut useful = E::from_f64(if bypass { 50.0 } else { 100.0 })
            .less(gain.sub(E::from_f64(0.05)).mul(E::from_f64(samples as f64)));
        let mut p = MlpMatrixPrimitive::default();
        if useful {
            let precision = if !gain.less(E::from_f64(9.0)) {
                12
            } else {
                gain.to_f64() as u32 + 3
            };
            for j in 0..channels {
                let rounded = E::from_f64(0.5)
                    .sub(
                        E::from_f64((1u32 << precision) as f64)
                            .mul(E::from_f64(transform[target * 6 + j])),
                    )
                    .to_f64()
                    .floor();
                let coefficient = rounded * (1u32 << (14 - precision)) as f64;
                if !(-32768.0..=32767.0).contains(&coefficient) {
                    useful = false;
                } else {
                    p.coefficient[j] = coefficient as i32;
                }
            }
        }
        if !useful {
            if !bypass {
                continue;
            }
            p = MlpMatrixPrimitive::default();
        }
        p.target = target as u32;
        p.bypass = u32::from(bypass);
        p.coefficient[target] = if bypass { -32768 } else { -16384 };
        work[count] = p;
        count += 1;
        bits += p.bypass;
    }
    *matrix = work;
    *primitives = count;
    *bypass_bits = bits;
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve the C function boundary"
)]
pub(crate) fn mlp_matrix_select(
    covariance: &[f64; 36],
    ratio: &[f64; 6],
    channels: usize,
    fixed: usize,
    samples: usize,
    qss: &[u32; 6],
    scale_count: &[u32; 6],
    allow_bypass: bool,
    matrix: &mut [crate::matrix::MlpMatrixPrimitive; 6],
    primitives: &mut usize,
    bypass_bits: &mut u32,
) -> Result<(), ()> {
    let mut work = [crate::matrix::MlpMatrixPrimitive::default(); 6];
    let mut count = 0;
    let mut bits = 0;
    mlp_matrix_select_append(
        covariance,
        ratio,
        channels,
        fixed,
        samples,
        qss,
        scale_count,
        allow_bypass,
        &mut work,
        &mut count,
        &mut bits,
    )?;
    *matrix = work;
    *primitives = count;
    *bypass_bits = bits;
    Ok(())
}
