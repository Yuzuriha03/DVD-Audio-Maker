#[derive(Clone, Debug, Default, PartialEq)]
#[repr(C)]
pub(crate) struct MlpPredictFilter {
    pub changed: u32,
    pub order: u32,
    pub coefficient: [f64; 8],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub(crate) struct MlpPredictState {
    pub input: [f32; 8],
    pub residual: [f32; 4],
}

fn push(history: &mut [f32], value: f32) {
    for i in (1..history.len()).rev() {
        history[i] = history[i - 1];
    }
    history[0] = value;
}

fn valid_filter(filter: &MlpPredictFilter, limit: u32) -> bool {
    filter.order <= limit && filter.coefficient.iter().all(|v| v.is_finite())
}

fn sar(value: i32, shift: u32) -> i32 {
    let divisor = 1i64 << shift;
    let v = i64::from(value);
    (if v >= 0 {
        v / divisor
    } else {
        -((-v + divisor - 1) / divisor)
    }) as i32
}

pub(crate) fn mlp_predict_block(
    a: &mut MlpPredictFilter,
    mut b: Option<&mut MlpPredictFilter>,
    state: &mut MlpPredictState,
    qss: u32,
    input: &[i32],
    output: &mut [i32],
    update_flag: u32,
) -> i32 {
    let count = input.len();
    if qss > 15
        || !(2..=160).contains(&count)
        || count & 1 != 0
        || output.len() < count
        || !valid_filter(a, 8)
        || b.as_ref().is_some_and(|filter| !valid_filter(filter, 4))
        || input.iter().any(|v| !(-8388608..=8388607).contains(v))
        || state
            .input
            .iter()
            .chain(state.residual.iter())
            .any(|v| !v.is_finite())
    {
        return -1;
    }
    let initial = *state;
    let mut work = initial;
    let mut temporary = [0i32; 160];
    let mut fallback = false;
    let divisor = f64::from(1u32 << qss);
    let bound = 8388608.0 / divisor;
    for n in 0..count {
        let mut prediction = 0.0;
        for k in 0..8 {
            prediction += a.coefficient[k] * f64::from(work.input[k]);
        }
        if let Some(filter) = b.as_ref() {
            for k in (1..=4).rev() {
                prediction += filter.coefficient[k - 1] * f64::from(work.residual[k - 1]);
            }
        }
        let residual = if b.is_some() {
            f64::from(input[n]) - prediction.floor()
        } else {
            -(prediction - f64::from(input[n])).floor()
        };
        let result = (residual / divisor).ceil();
        if !residual.is_finite()
            || !(-2147483647.0..=2147483647.0).contains(&residual)
            || result < -bound
            || result >= bound
        {
            fallback = true;
            break;
        }
        temporary[n] = result as i32;
        push(&mut work.input, input[n] as f32);
        if b.is_some() {
            push(&mut work.residual, residual as f32);
        }
    }
    if fallback {
        work = initial;
        for n in 0..count {
            temporary[n] = sar(input[n], qss);
            push(&mut work.input, input[n] as f32);
            if b.is_some() {
                push(&mut work.residual, input[n] as f32);
            }
        }
        a.changed = u32::from(update_flag == 0);
        a.order = 1;
        a.coefficient[0] = 0.0;
        if let Some(filter) = b.as_mut() {
            filter.changed = u32::from(update_flag == 0);
            filter.order = 0;
        }
    }
    output[..count].copy_from_slice(&temporary[..count]);
    *state = work;
    i32::from(fallback)
}
