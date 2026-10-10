#[derive(Clone, Debug, Default, PartialEq)]
#[repr(C)]
pub(crate) struct MlpWireFilter {
    pub changed: u32,
    pub precision: u32,
    pub order: u32,
    pub coefficient: [f64; 8],
}

pub(crate) fn mlp_search_iir_random(
    state: &mut u32,
    requested: u32,
    maximum: u32,
    attempts: u32,
    result: &mut MlpWireFilter,
) -> i32 {
    if requested > 4 || maximum == 0 || maximum > 4 || attempts == 0 {
        return -1;
    }
    let mut rng = *state;
    for _ in 0..attempts {
        let mut f = MlpWireFilter::default();
        let mut a = [0.0; 8];
        let mut k = [0.0; 8];
        let mut order = requested;
        if order == 0 {
            let choice = f64::from(mlp_search_rand(&mut rng)) * (1.0 / 327.68);
            order = if choice < 5.0 {
                1
            } else if choice < 20.0 {
                2
            } else if choice < 50.0 {
                3
            } else {
                4
            };
            order = order.min(maximum);
        }
        f.changed = 1;
        f.order = order;
        f.precision = order + 4;
        let scale = f64::from(1u32 << f.precision);
        for (i, value) in k.iter_mut().enumerate().take(order as usize) {
            let u = f64::from(mlp_search_rand(&mut rng)) * f64::from_bits(0x3f00002000400080);
            *value = if i < order as usize - 1 {
                (u - 0.5) * 1.9
            } else {
                (u + 0.05) * 0.9
            };
            if i == order as usize - 1 && *value < 0.5 {
                *value -= 1.0;
            }
        }
        for m in 0..8 {
            let old = a;
            for i in 0..m {
                a[i] = old[i] + k[m] * old[m - 1 - i];
            }
            a[m] = k[m];
        }
        for (i, value) in a.iter().enumerate().take(order as usize) {
            f.coefficient[i] = (value * scale + 0.5).floor() / scale;
        }
        let coefficients = f.coefficient[..4].try_into().unwrap();
        if mlp_search_iir_stable(&coefficients, order as usize) == 1 {
            *state = rng;
            *result = f;
            return 0;
        }
    }
    -1
}

#[derive(Clone, Debug, Default, PartialEq)]
#[repr(C)]
pub(crate) struct MlpSearchEntry {
    pub filter: MlpWireFilter,
    pub timestamp: u32,
    pub fixed: u32,
    pub no_perturb: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
#[repr(C)]
pub(crate) struct MlpSearchPool {
    pub entry: [MlpSearchEntry; 18],
    pub count: u32,
    pub clock: u32,
    pub rng: u32,
}

pub(crate) fn mlp_search_pool_init(pool: &mut MlpSearchPool, seed: u32) -> i32 {
    let precision = [3, 4, 5, 8, 8, 5];
    let coefficient = [
        [-0.875, 0.0],
        [-0.9375, 0.0],
        [-0.96875, 0.0],
        [-1.921875, 0.92578125],
        [-1.78125, 0.8125],
        [-1.34375, 0.5625],
    ];
    let mut work = MlpSearchPool {
        count: 18,
        rng: seed,
        ..Default::default()
    };
    for i in 0..work.count as usize {
        let e = &mut work.entry[i];
        if i < 6 {
            e.filter.changed = 1;
            e.filter.order = if i < 3 { 1 } else { 2 };
            e.filter.precision = precision[i];
            e.filter.coefficient[..2].copy_from_slice(&coefficient[i]);
            e.fixed = 1;
            e.no_perturb = 1;
        } else {
            let relative = i - 6;
            let order = if relative > 6 {
                4
            } else if relative > 2 {
                3
            } else {
                u32::from(relative > 0) + 1
            };
            if mlp_search_iir_random(&mut work.rng, order, 4, 10000, &mut e.filter) != 0 {
                return -1;
            }
        }
    }
    *pool = work;
    0
}

pub(crate) fn mlp_search_pool_replacement(pool: &MlpSearchPool, order: u32) -> usize {
    let mut retained = 0;
    let mut replacement = 0;
    let mut saved_age = 0;
    if pool.count == 0 || pool.count > 18 {
        return 0;
    }
    for i in 0..pool.count as usize {
        let age = pool.clock.wrapping_sub(pool.entry[retained].timestamp);
        if pool.entry[i].fixed == 0 && pool.entry[i].filter.order == order && age >= saved_age {
            retained = i;
            replacement = i;
            saved_age = age;
        }
    }
    replacement
}

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve the C mlp_search_pool_select function boundary"
)]
pub(crate) fn mlp_search_pool_select(
    pool: &mut MlpSearchPool,
    correlation: &[f64],
    lag: usize,
    maximum_b: u32,
    maximum_a: usize,
    perturbations: u32,
    b: &mut MlpWireFilter,
    reflection: &mut [f64; 8],
    relative_error: &mut f64,
) -> i32 {
    let mut best_k = [0.0; 8];
    let mut trial_k = [0.0; 8];
    let mut best = 0.0;
    let mut weight = 1.0;
    let mut value = 0.0;
    let mut selected = 0;
    let mut found = false;
    if pool.count == 0
        || pool.count > 18
        || maximum_b == 0
        || maximum_b > 4
        || maximum_a > 8
        || lag > 49
        || correlation.len() <= lag
        || correlation[0] <= 0.0
        || perturbations > 100000
    {
        return -1;
    }
    let mut work = pool.clone();
    for i in 0..work.count as usize {
        let f = &work.entry[i].filter;
        if f.order > 4 || f.precision > 15 {
            return -1;
        }
        if f.order > maximum_b {
            continue;
        }
        if mlp_search_iir_evaluate(
            correlation,
            lag,
            &f.coefficient,
            f.order as usize,
            maximum_a,
            &mut trial_k,
            &mut value,
        ) != 0
        {
            return -1;
        }
        let w = if f.order != 0 {
            1.02 + 0.005 * f64::from(f.order)
        } else {
            1.0
        };
        if !found || w * value < weight * best {
            found = true;
            selected = i;
            best = value;
            weight = w;
            best_k = trial_k;
        }
    }
    let mut fresh = MlpWireFilter::default();
    if !found
        || mlp_search_iir_random(&mut work.rng, 0, maximum_b, 10000, &mut fresh) != 0
        || mlp_search_iir_evaluate(
            correlation,
            lag,
            &fresh.coefficient,
            fresh.order as usize,
            maximum_a,
            &mut trial_k,
            &mut value,
        ) != 0
    {
        return -1;
    }
    if (1.02 + 0.005 * f64::from(fresh.order)) * value < weight * best {
        selected = mlp_search_pool_replacement(&work, fresh.order);
        weight = 1.02 + 0.005 * f64::from(fresh.order);
        work.entry[selected].filter = fresh;
        work.entry[selected].timestamp = work.clock;
        best = value;
        best_k = trial_k;
    }
    if work.entry[selected].no_perturb == 0 {
        for _ in 0..perturbations {
            let mut coefficients = [0.0; 4];
            let mut changed = false;
            let f = &mut work.entry[selected].filter;
            for (j, c) in coefficients.iter_mut().enumerate().take(f.order as usize) {
                let delta = (mlp_search_rand(&mut work.rng) / 10922) as i32 - 1;
                *c = f.coefficient[j] + f64::from(delta) * 2.0f64.powi(-(f.precision as i32));
                changed |= delta != 0;
            }
            if !changed || mlp_search_iir_stable(&coefficients, f.order as usize) != 1 {
                continue;
            }
            if mlp_search_iir_evaluate(
                correlation,
                lag,
                &coefficients,
                f.order as usize,
                maximum_a,
                &mut trial_k,
                &mut value,
            ) != 0
            {
                return -1;
            }
            if value < best {
                f.coefficient[..f.order as usize]
                    .copy_from_slice(&coefficients[..f.order as usize]);
                best = value;
                best_k = trial_k;
            }
        }
    }
    if mlp_search_iir_evaluate(
        correlation,
        lag,
        &[],
        0,
        maximum_a,
        &mut trial_k,
        &mut value,
    ) != 0
    {
        return -1;
    }
    let mut chosen = work.entry[selected].filter.clone();
    if value < weight * best {
        chosen = MlpWireFilter::default();
        best = value;
        best_k = trial_k;
    }
    value = best / correlation[0];
    if !value.is_finite() {
        return -1;
    }
    work.clock = work.clock.wrapping_add(1);
    *pool = work;
    *b = chosen;
    *reflection = best_k;
    *relative_error = value;
    0
}

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve the C mlp_search_joint function boundary"
)]
pub(crate) fn mlp_search_joint(
    pool: &mut MlpSearchPool,
    correlation: &[f64],
    lag: usize,
    maximum_b: u32,
    maximum_a: usize,
    perturbations: u32,
    probability: f64,
    a_result: &mut MlpWireFilter,
    b_result: &mut MlpWireFilter,
) -> i32 {
    if lag > 49
        || correlation.len() <= lag
        || !probability.is_finite()
        || !(0.0..=1.0).contains(&probability)
    {
        return -1;
    }
    let mut work = pool.clone();
    let mut a = MlpWireFilter::default();
    let mut b = MlpWireFilter::default();
    let mut r = [0.0; 50];
    let mut k = [0.0; 8];
    let mut direct = [0.0; 8];
    let mut relative = 0.0;
    let mut factor = 0.25;
    let mut feedback = 0.0;
    let mut magnitude = 65536u32;
    let mut order = 1usize;
    let mut q = 3u32;
    let mut threshold = 15i32;
    r[..=lag].copy_from_slice(&correlation[..=lag]);
    r[0] += 10.0;
    if mlp_search_pool_select(
        &mut work,
        &r,
        lag,
        maximum_b,
        maximum_a,
        perturbations,
        &mut b,
        &mut k,
        &mut relative,
    ) != 0
    {
        return -1;
    }
    let limit = 8 - b.order as usize;
    let mut ratio = 1.0 - k[0] * k[0];
    while order < limit && ratio > 2.0f64.powi(-2 * threshold) {
        order += 1;
        threshold -= 1;
        ratio *= 1.0 - k[order - 1] * k[order - 1];
    }
    while order != 0 && k[order - 1] * k[order - 1] < 0.03 {
        order -= 1;
    }
    k[order..].fill(0.0);
    let rho = k[0];
    for m in 0..8 {
        let old = direct;
        for i in 0..m {
            direct[i] = old[i] + k[m] * old[m - 1 - i];
        }
        direct[m] = k[m];
    }
    for coefficient in direct[..order]
        .iter()
        .chain(&b.coefficient[..b.order as usize])
    {
        let v = (65536.0 * coefficient.abs()).trunc();
        if !v.is_finite() || v > f64::from(i32::MAX) {
            return -1;
        }
        if v > f64::from(magnitude) {
            magnitude = v as u32;
        }
    }
    if magnitude & 0xffffe000 < 0x07ffc000 {
        while factor > ratio {
            q += 1;
            factor *= 0.25;
            if q > 15 {
                return -1;
            }
            if magnitude >> (16 - q) >= 0x3ffe {
                break;
            }
        }
    }
    let scale = f64::from(1u32 << q);
    for i in (1..=8).rev() {
        let v = feedback * rho + scale * direct[i - 1];
        let z = (v + 0.5).floor();
        feedback = z - v;
        a.coefficient[i - 1] = -z / scale;
    }
    while order != 0 && a.coefficient[order - 1] == 0.0 {
        order -= 1;
    }
    a.order = order as u32;
    a.precision = q;
    a.changed = u32::from(order > 0);
    if order + (b.order as usize) < 7
        && b.order > 0
        && b.order < 4
        && probability > 0.0
        && f64::from(mlp_search_rand(&mut work.rng)) < probability * 32767.0
        && magnitude >> (16 - q) < 5000
    {
        for i in (1..=a.order as usize).rev() {
            a.coefficient[i] += 2.0 * a.coefficient[i - 1];
        }
        a.coefficient[0] -= 2.0;
        a.order += 1;
        for i in (1..=b.order as usize).rev() {
            b.coefficient[i] += 0.5 * b.coefficient[i - 1];
        }
        b.coefficient[0] += 0.5;
        b.order += 1;
        b.precision += 1;
    }
    *pool = work;
    *a_result = a;
    *b_result = b;
    0
}

pub(crate) fn mlp_search_fir(
    correlation: &[f64; 9],
    mut order: usize,
    result: &mut MlpWireFilter,
) -> i32 {
    if !valid_correlation(correlation, order) {
        return -1;
    }
    let mut r = *correlation;
    let original = r[0];
    r[0] += 10.0;
    let mut k = [0.0; 8];
    let mut a = [0.0; 8];
    let mut factor = 0.25;
    let mut feedback = 0.0;
    let mut maximum = 65536u32;
    let mut q = 3u32;
    let mut f = MlpWireFilter::default();
    if reflect(&mut r, order, &mut k) != 0 {
        return -1;
    }
    let rho = k[0];
    while order != 0 && k[order - 1] * k[order - 1] < 0.03 {
        order -= 1;
    }
    k[order..].fill(0.0);
    for m in 0..8 {
        let old = a;
        for i in 0..m {
            a[i] = old[i] + k[m] * old[m - 1 - i];
        }
        a[m] = k[m];
    }
    for coefficient in &a[..order] {
        let magnitude = (65536.0 * coefficient.abs()).trunc();
        if !magnitude.is_finite() || magnitude > f64::from(i32::MAX) {
            return -1;
        }
        if magnitude > f64::from(maximum) {
            maximum = magnitude as u32;
        }
    }
    if maximum & 0xffffe000 < 0x07ffc000 {
        while factor * original > r[0] {
            q += 1;
            factor *= 0.25;
            if q > 15 {
                return -1;
            }
            if maximum >> (16 - q) >= 0x3ffe {
                break;
            }
        }
    }
    let scale = f64::from(1u32 << q);
    for i in (1..=8).rev() {
        let v = feedback * rho + scale * a[i - 1];
        let z = (v + 0.5).floor();
        feedback = z - v;
        f.coefficient[i - 1] = -z / scale;
    }
    while order != 0 && f.coefficient[order - 1] == 0.0 {
        order -= 1;
    }
    f.order = order as u32;
    f.precision = q;
    f.changed = u32::from(order > 0);
    *result = f;
    0
}

#[allow(
    clippy::needless_range_loop,
    reason = "Preserve C matrix elimination indexing and accumulation order"
)]
pub(crate) fn mlp_search_feedback_state(
    pcm: &[i32; 40],
    a: &MlpWireFilter,
    b: &MlpWireFilter,
    attempts: u32,
    state: &mut [i32; 4],
    quantization: &mut u32,
) -> i32 {
    let mut e = [0.0f32; 40];
    let mut h = [0.0f32; 40];
    let mut scratch = [0.0; 5];
    let mut lambda = 1.0;
    let mut s = [0i32; 4];
    let mut output = [0i32; 4];
    if a.order > 8
        || b.order == 0
        || b.order > 4
        || attempts == 0
        || pcm.iter().any(|v| !(-8388608..=8388607).contains(v))
        || a.coefficient[..a.order as usize]
            .iter()
            .any(|v| !v.is_finite())
        || b.coefficient[..b.order as usize]
            .iter()
            .any(|v| !v.is_finite())
    {
        return -1;
    }
    let p = b.order as usize;
    h[7] = 1.0;
    for n in 8..40 {
        let mut v = f64::from(pcm[n] as f32);
        let mut impulse = 0.0;
        for j in 0..a.order as usize {
            v -= a.coefficient[j] * f64::from(pcm[n - 1 - j] as f32);
        }
        for j in 0..p {
            v -= b.coefficient[j] * f64::from(e[n - 1 - j]);
            impulse -= b.coefficient[j] * f64::from(h[n - 1 - j]);
        }
        e[n] = v as f32;
        h[n] = impulse as f32;
        if !e[n].is_finite() || !h[n].is_finite() {
            return -1;
        }
    }
    for _ in 0..attempts {
        let mut matrix = [[0.0; 37]; 5];
        let mut maximum = 0.0;
        let mut invalid = false;
        for k in 0..p {
            matrix[k][0] = b.coefficient[p - 1 - k];
        }
        matrix[p][0] = f64::from(e[8]);
        for t in 1..32 {
            matrix[0][t] = f64::from(h[7 + t]) * b.coefficient[p - 1];
            for k in 1..p {
                matrix[k][t] =
                    f64::from(h[7 + t]) * b.coefficient[p - 1 - k] + matrix[k - 1][t - 1];
            }
            matrix[p][t] = f64::from(e[8 + t]);
        }
        for k in 0..=p {
            matrix[k][32 + k] = lambda;
        }
        for k in 0..=p {
            for j in k..=p {
                scratch[j] = 0.0;
                for t in 0..32 + p {
                    scratch[j] += matrix[j][t] * matrix[k][t];
                }
            }
            if (!scratch[k].is_finite() || scratch[k] <= 0.0) && (k != p || scratch[k] != 0.0) {
                return -1;
            }
            for j in k + 1..=p {
                let ratio = scratch[j] / scratch[k];
                scratch[j] = ratio;
                for t in 0..32 + p {
                    matrix[j][t] -= ratio * matrix[k][t];
                }
            }
            matrix[k][k..=p].copy_from_slice(&scratch[k..=p]);
        }
        for (k, row) in matrix.iter().enumerate().take(p) {
            if row[k] > maximum {
                maximum = row[k];
            }
        }
        let bits = (matrix[p][p] / (512.0 * maximum)).ln_1p() / 4.0f64.ln();
        if !bits.is_finite() || bits < 0.0 {
            return -1;
        }
        let q = if bits >= 15.0 { 15 } else { bits as u32 };
        for k in (1..=p).rev() {
            let mut v = matrix[k - 1][p];
            for (j, &value) in s.iter().enumerate().take(p).skip(k) {
                v -= f64::from(value) * matrix[k - 1][j];
            }
            let z = (v * 2.0f64.powi(-(q as i32)) + 0.5).floor() * 2.0f64.powi(q as i32);
            if !z.is_finite() {
                return -1;
            }
            if !(-8388607.0..=8388607.0).contains(&z) {
                invalid = true;
                break;
            }
            s[k - 1] = z as i32;
        }
        if !invalid {
            for k in 0..p {
                output[k] = s[p - 1 - k];
            }
            *state = output;
            *quantization = q;
            return 0;
        }
        lambda *= 2.0;
        if !lambda.is_finite() {
            return -1;
        }
    }
    -1
}

#[derive(Clone, Debug, Default, PartialEq)]
#[repr(C)]
pub(crate) struct MlpWireState {
    pub changed: u32,
    pub count: u32,
    pub value: [i32; 8],
}

#[derive(Clone, Debug, Default, PartialEq)]
#[repr(C)]
pub(crate) struct MlpSearchPlan {
    pub a: MlpWireFilter,
    pub b: MlpWireFilter,
    pub state: MlpWireState,
}

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve the C mlp_search_interval function boundary"
)]
pub(crate) fn mlp_search_interval(
    mut pool: Option<&mut MlpSearchPool>,
    pcm: &[i32],
    lengths: &[usize],
    maximum_a: usize,
    maximum_b: u32,
    lag: usize,
    mode_flags: u32,
    plan: &mut MlpSearchPlan,
) -> i32 {
    let mut work = MlpSearchPlan::default();
    let mut correlation = [0.0; 50];
    let mut total = 0usize;
    let joint = maximum_b != 0 && mode_flags & 2 == 0;
    if lengths.is_empty()
        || lengths.len() > 128
        || maximum_a > 8
        || maximum_b > 4
        || lag > 49
        || (joint && (pool.is_none() || lag < 8))
    {
        return -1;
    }
    for &length in lengths {
        if !(8..=160).contains(&length) || length & 1 != 0 {
            return -1;
        }
        total += length;
    }
    let mut candidates = pool.as_deref().cloned().unwrap_or_default();
    if joint {
        if total < 40 || pcm.len() < 40 {
            return -1;
        }
        if mlp_search_correlation_extended(pcm, lengths, lag, &mut correlation) != 0
            || mlp_search_joint(
                &mut candidates,
                &correlation,
                lag,
                maximum_b,
                maximum_a,
                16,
                0.001,
                &mut work.a,
                &mut work.b,
            ) != 0
        {
            return -1;
        }
        if work.b.order != 0 {
            let mut state = [0; 4];
            let mut quantization = 0;
            if mlp_search_feedback_state(
                pcm[..40].try_into().unwrap(),
                &work.a,
                &work.b,
                64,
                &mut state,
                &mut quantization,
            ) != 0
            {
                return -1;
            }
            work.state.value[..4].copy_from_slice(&state);
            work.state.count = work.b.order;
            work.state.changed = 1;
        }
    } else {
        let order = (if mode_flags & 2 != 0 { 4 } else { 8 }).min(maximum_a);
        if mlp_search_correlation_extended(pcm, lengths, order, &mut correlation) != 0
            || mlp_search_fir(correlation[..9].try_into().unwrap(), order, &mut work.a) != 0
        {
            return -1;
        }
    }
    work.a.changed = 1;
    work.b.changed = 1;
    if joint {
        **pool.as_mut().unwrap() = candidates;
    }
    *plan = work;
    0
}

pub(crate) fn mlp_search_rand(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(214013).wrapping_add(2531011);
    (*state >> 16) & 32767
}

pub(crate) fn mlp_search_iir_stable(coefficient: &[f64; 4], order: usize) -> i32 {
    let mut a = [0.0; 4];
    if order == 0 || order > 4 {
        return -1;
    }
    for i in 0..order {
        if !coefficient[i].is_finite() {
            return -1;
        }
        a[i] = coefficient[i];
    }
    if a[order - 1].abs() < 0.5 {
        return 0;
    }
    for m in (1..=order).rev() {
        let remaining = m - 1;
        let k = a[remaining];
        if !k.is_finite() || k.abs() > 0.98 {
            return 0;
        }
        let inverse = 1.0 / (1.0 - k * k);
        for i in 0..remaining / 2 {
            let left = a[i];
            let right = a[remaining - 1 - i];
            a[i] = (left - right * k) * inverse;
            a[remaining - 1 - i] = (right - left * k) * inverse;
        }
        if remaining & 1 != 0 {
            a[remaining / 2] /= 1.0 + k;
        }
    }
    1
}

pub(crate) fn mlp_search_correlation_extended(
    pcm: &[i32],
    lengths: &[usize],
    order: usize,
    result: &mut [f64; 50],
) -> i32 {
    let mut r = [0.0; 50];
    let mut history = [0.0; 49];
    let mut y = [0.0; 209];
    let mut window = [0.0; 160];
    let mut offset = 0usize;
    if lengths.is_empty() || order > 49 {
        return -1;
    }
    for &length in lengths {
        if !(8..=160).contains(&length) || length & 1 != 0 {
            return -1;
        }
        let Some(end) = offset.checked_add(length) else {
            return -1;
        };
        if end > pcm.len()
            || pcm[offset..end]
                .iter()
                .any(|v| !(-8388608..=8388607).contains(v))
        {
            return -1;
        }
        offset = end;
    }
    for (n, w) in window.iter_mut().enumerate() {
        let t = (n + 1) as f64 * f64::from_bits(0x3f7970e4f80cb872);
        *w = (3.0 - (t + t)) * t * t;
    }
    offset = 0;
    for (b, &length) in lengths.iter().enumerate() {
        let step = 160 / length;
        y[..49].copy_from_slice(&history);
        for n in 0..length {
            let w = if b == 0 {
                window[n * step]
            } else if b == lengths.len() - 1 {
                window[159 - n * step]
            } else {
                1.0
            };
            y[49 + n] = f64::from(pcm[offset + n] as f32) * w;
        }
        for k in 0..=order {
            let mut sum = 0.0;
            for n in 0..length {
                sum += y[49 + n] * y[49 + n - k];
            }
            r[k] += sum;
        }
        history.copy_from_slice(&y[length..length + 49]);
        offset += length;
    }
    *result = r;
    0
}

fn reflect(r: &mut [f64; 9], order: usize, out: &mut [f64; 8]) -> i32 {
    let mut s = [0.0; 17];
    for j in 0..9 {
        s[8 + j] = r[j];
        s[8 - j] = r[j];
    }
    for m in 1..=order {
        if r[0] <= 0.0 || !r[0].is_finite() {
            return -1;
        }
        let mut k = -s[8 + m] / r[0];
        if !k.is_finite() {
            return -1;
        }
        k = k.clamp(-1.0, 1.0);
        out[m - 1] = k;
        let kk = k * k;
        for j in (1..=order - m).rev() {
            let a = s[8 + m + j];
            let b = s[8 + m - j];
            let c = r[j];
            let twice = c * k + c * k;
            s[8 + m + j] = b * kk + a + twice;
            s[8 + m - j] = a * kk + b + twice;
            r[j] = (1.0 + kk) * c + (a + b) * k;
        }
        let v = s[8 + m];
        let e = r[0];
        s[8 + m] = kk * v + k * e + k * e + v;
        r[0] = k * v + k * v + (1.0 + kk) * e;
        if !r[0].is_finite() || r[0] < 0.0 {
            return -1;
        }
    }
    0
}

fn valid_correlation(r: &[f64; 9], order: usize) -> bool {
    order <= 8 && r[0] >= 0.0 && r.iter().all(|v| v.is_finite())
}

pub(crate) fn mlp_search_iir_evaluate(
    correlation: &[f64],
    lag: usize,
    coefficient: &[f64],
    order: usize,
    max_fir_order: usize,
    reflection: &mut [f64; 8],
    score: &mut f64,
) -> i32 {
    let mut impulse = [0.0; 50];
    impulse[0] = 1.0;
    let mut h = [0.0; 50];
    let mut r = [0.0; 9];
    let mut k = [0.0; 8];
    if order > 4
        || max_fir_order > 8
        || lag > 49
        || correlation.len() <= lag
        || coefficient.len() < order
    {
        return -1;
    }
    let aorder = max_fir_order.min(8 - order);
    if lag < aorder
        || correlation[0] < 0.0
        || correlation[..=lag].iter().any(|v| !v.is_finite())
        || coefficient[..order].iter().any(|v| !v.is_finite())
    {
        return -1;
    }
    let tail = lag - aorder;
    for n in 1..=tail {
        for j in 1..=n.min(order) {
            impulse[n] -= impulse[n - j] * coefficient[j - 1];
        }
        if !impulse[n].is_finite() {
            return -1;
        }
    }
    for j in 0..=tail {
        for n in 0..=tail - j {
            h[j] += impulse[n + j] * impulse[n];
        }
    }
    for n in 0..=aorder {
        r[n] = h[0] * correlation[n];
        for j in 1..=tail {
            let index = n.abs_diff(j);
            r[n] += (correlation[index] + correlation[n + j]) * h[j];
        }
        if !r[n].is_finite() {
            return -1;
        }
    }
    if reflect(&mut r, aorder, &mut k) != 0 {
        return -1;
    }
    let value = h[0] + h[0] + r[0];
    if !value.is_finite() {
        return -1;
    }
    *reflection = k;
    *score = value;
    0
}
