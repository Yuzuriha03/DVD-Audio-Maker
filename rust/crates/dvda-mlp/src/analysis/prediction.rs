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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PredictionOutcome {
    Predicted,
    Fallback,
}

pub(crate) fn mlp_predict_block(
    a: &mut MlpPredictFilter,
    mut b: Option<&mut MlpPredictFilter>,
    state: &mut MlpPredictState,
    qss: u32,
    input: &[i32],
    output: &mut [i32],
    update_flag: u32,
) -> Result<PredictionOutcome, ()> {
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
        return Err(());
    }
    let initial = *state;
    let mut work = initial;
    let mut temporary = [0i32; 160];
    let mut fallback = false;
    let divisor = f64::from(1u32 << qss);
    let bound = 8388608.0 / divisor;
    for (&sample, destination) in input.iter().zip(&mut temporary) {
        let mut prediction = 0.0;
        for (&coefficient, &sample) in a.coefficient.iter().zip(&work.input) {
            prediction += coefficient * f64::from(sample);
        }
        if let Some(filter) = b.as_ref() {
            for (&coefficient, &residual) in
                filter.coefficient[..4].iter().zip(&work.residual).rev()
            {
                prediction += coefficient * f64::from(residual);
            }
        }
        let residual = if b.is_some() {
            f64::from(sample) - prediction.floor()
        } else {
            -(prediction - f64::from(sample)).floor()
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
        *destination = result as i32;
        push(&mut work.input, sample as f32);
        if b.is_some() {
            push(&mut work.residual, residual as f32);
        }
    }
    if fallback {
        work = initial;
        for (&sample, destination) in input.iter().zip(&mut temporary) {
            *destination = sar(sample, qss);
            push(&mut work.input, sample as f32);
            if b.is_some() {
                push(&mut work.residual, sample as f32);
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
    Ok(if fallback {
        PredictionOutcome::Fallback
    } else {
        PredictionOutcome::Predicted
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_prediction_preserves_filters_state_and_output() {
        let mut filter = MlpPredictFilter::default();
        let original = filter.clone();
        let mut state = MlpPredictState::default();
        let mut output = [123; 2];
        assert_eq!(
            mlp_predict_block(&mut filter, None, &mut state, 16, &[1, 2], &mut output, 0),
            Err(())
        );
        assert_eq!(filter, original);
        assert_eq!(state, MlpPredictState::default());
        assert_eq!(output, [123; 2]);
    }

    #[test]
    fn prediction_outcome_distinguishes_fallback_and_commits_history() {
        for update_flag in [0, 1] {
            let mut filter = MlpPredictFilter {
                order: 1,
                coefficient: [f64::MAX; 8],
                ..Default::default()
            };
            let mut feedback = MlpPredictFilter::default();
            let mut state = MlpPredictState {
                input: [1.0; 8],
                ..Default::default()
            };
            let mut output = [0; 2];
            assert_eq!(
                mlp_predict_block(
                    &mut filter,
                    Some(&mut feedback),
                    &mut state,
                    1,
                    &[-3, 5],
                    &mut output,
                    update_flag,
                ),
                Ok(PredictionOutcome::Fallback)
            );
            assert_eq!(output, [-2, 2]);
            assert_eq!(&state.input[..2], &[5.0, -3.0]);
            assert_eq!(&state.residual[..2], &[5.0, -3.0]);
            assert_eq!(filter.changed, u32::from(update_flag == 0));
            assert_eq!(feedback.changed, filter.changed);
            assert_eq!(filter.coefficient[0], 0.0);
            assert_eq!(feedback.order, 0);
        }
        let mut filter = MlpPredictFilter::default();
        let mut state = MlpPredictState::default();
        let mut output = [0; 2];
        assert_eq!(
            mlp_predict_block(&mut filter, None, &mut state, 0, &[-3, 5], &mut output, 0),
            Ok(PredictionOutcome::Predicted)
        );
        assert_eq!(output, [-3, 5]);
    }
}
