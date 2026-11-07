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

/// Number of leading coefficients worth accumulating. Everything after the last
/// non-zero coefficient is an exact 0.0/-0.0 term whose addition cannot change a
/// finite prediction. Scanning the whole array (instead of trusting `order`)
/// keeps the shortened traversal equivalent even for a synthetic filter whose
/// tail is non-zero.
fn used_coefficients(coefficients: &[f64]) -> usize {
    coefficients
        .iter()
        .rposition(|&coefficient| coefficient != 0.0)
        .map_or(0, |index| index + 1)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "Arithmetic right shift computes an i32-bounded quotient in i64 to preserve negative-value rounding"
)]
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

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "Prediction deliberately uses the reference f32 state and truncating quantized-filter conversions"
)]
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
        || input.iter().any(|v| !(-8_388_608..=8_388_607).contains(v))
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
    let bound = 8_388_608.0 / divisor;
    let direct = used_coefficients(&a.coefficient);
    let feedback = b
        .as_ref()
        .map_or(0, |filter| used_coefficients(&filter.coefficient[..4]));
    for (&sample, destination) in input.iter().zip(&mut temporary) {
        let mut prediction = 0.0;
        if direct == a.coefficient.len() {
            for (&coefficient, &sample) in a.coefficient.iter().zip(&work.input) {
                prediction += coefficient * f64::from(sample);
            }
        } else {
            for (&coefficient, &sample) in a.coefficient[..direct].iter().zip(&work.input[..direct])
            {
                prediction += coefficient * f64::from(sample);
            }
        }
        if let Some(filter) = b.as_ref() {
            // Feedback taps accumulate from the highest index down, so
            // truncating the prefix keeps the original accumulation order.
            if feedback == 4 {
                for (&coefficient, &residual) in
                    filter.coefficient[..4].iter().zip(&work.residual).rev()
                {
                    prediction += coefficient * f64::from(residual);
                }
            } else {
                for (&coefficient, &residual) in filter.coefficient[..feedback]
                    .iter()
                    .zip(&work.residual[..feedback])
                    .rev()
                {
                    prediction += coefficient * f64::from(residual);
                }
            }
        }
        let residual = if b.is_some() {
            f64::from(sample) - prediction.floor()
        } else {
            -(prediction - f64::from(sample)).floor()
        };
        let result = (residual / divisor).ceil();
        if !residual.is_finite()
            || !(-2_147_483_647.0..=2_147_483_647.0).contains(&residual)
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
    #![allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::field_reassign_with_default,
        clippy::ref_option,
        clippy::too_many_arguments,
        clippy::unreadable_literal,
        reason = "Deterministic oracle fixtures use bounded numeric conversions, explicit bit patterns, and a full-traversal helper signature"
    )]

    use super::*;
    use std::time::{Duration, Instant};

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

    /// Verbatim copy of the pre-optimization traversal, kept as the in-process
    /// oracle for equivalence and benchmark tests.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "The reference implementation deliberately retains the original full-width coefficient traversal"
    )]
    fn reference_predict_block(
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
            || input.iter().any(|v| !(-8_388_608..=8_388_607).contains(v))
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
        let bound = 8_388_608.0 / divisor;
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
                || !(-2_147_483_647.0..=2_147_483_647.0).contains(&residual)
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

    struct PredictCase {
        a: MlpPredictFilter,
        b: Option<MlpPredictFilter>,
        state: MlpPredictState,
        qss: u32,
        input: Vec<i32>,
        output_len: usize,
        update_flag: u32,
    }

    struct PredictRun {
        result: Result<PredictionOutcome, ()>,
        output: Vec<i32>,
        a: MlpPredictFilter,
        b: Option<MlpPredictFilter>,
        state: MlpPredictState,
    }

    type PredictImplementation = fn(
        &mut MlpPredictFilter,
        Option<&mut MlpPredictFilter>,
        &mut MlpPredictState,
        u32,
        &[i32],
        &mut [i32],
        u32,
    ) -> Result<PredictionOutcome, ()>;

    fn run_predict(case: &PredictCase, implementation: PredictImplementation) -> PredictRun {
        let mut a = case.a.clone();
        let mut b = case.b.clone();
        let mut state = case.state;
        let mut output = vec![i32::MIN; case.output_len];
        let result = implementation(
            &mut a,
            b.as_mut(),
            &mut state,
            case.qss,
            &case.input,
            &mut output,
            case.update_flag,
        );
        PredictRun {
            result,
            output,
            a,
            b,
            state,
        }
    }

    fn assert_filter_bits(left: &MlpPredictFilter, right: &MlpPredictFilter) {
        assert_eq!(left.changed, right.changed);
        assert_eq!(left.order, right.order);
        assert_eq!(
            left.coefficient.map(f64::to_bits),
            right.coefficient.map(f64::to_bits)
        );
    }

    fn assert_case_matches_reference(case: &PredictCase) {
        let shortened = run_predict(case, mlp_predict_block);
        let reference = run_predict(case, reference_predict_block);
        assert_eq!(shortened.result, reference.result);
        assert_eq!(shortened.output, reference.output);
        assert_filter_bits(&shortened.a, &reference.a);
        match (&shortened.b, &reference.b) {
            (Some(left), Some(right)) => assert_filter_bits(left, right),
            (None, None) => {}
            _ => panic!("feedback filter presence mismatch"),
        }
        assert_eq!(
            shortened.state.input.map(f32::to_bits),
            reference.state.input.map(f32::to_bits)
        );
        assert_eq!(
            shortened.state.residual.map(f32::to_bits),
            reference.state.residual.map(f32::to_bits)
        );
    }

    fn patterned_filter(order: u32, negative_zero_tail: bool) -> MlpPredictFilter {
        let mut filter = MlpPredictFilter::default();
        filter.order = order;
        for (index, coefficient) in filter.coefficient.iter_mut().enumerate() {
            if index < order as usize {
                let sign = if index & 1 == 0 { 1.0 } else { -1.0 };
                *coefficient = sign * (0.25 + index as f64 * 0.125);
            } else if negative_zero_tail {
                *coefficient = -0.0;
            }
        }
        filter
    }

    #[test]
    fn shortened_traversal_matches_reference_across_orders() {
        for a_order in 0..=8u32 {
            for b_order in 0..=4u32 {
                for b_present in [false, true] {
                    for qss in [0, 1, 8, 15] {
                        for count in [2usize, 8, 160] {
                            let case = PredictCase {
                                a: patterned_filter(a_order, true),
                                b: b_present.then(|| patterned_filter(b_order, true)),
                                state: MlpPredictState {
                                    input: std::array::from_fn(|i| (i as f32 - 3.5) * 11.25),
                                    residual: std::array::from_fn(|i| (2.5 - i as f32) * 5.5),
                                },
                                qss,
                                input: (0..count).map(|i| (i as i32 * 97) % 4001 - 2000).collect(),
                                output_len: count,
                                update_flag: (a_order + b_order) & 1,
                            };
                            assert_case_matches_reference(&case);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn shortened_traversal_matches_reference_for_all_zero_filters() {
        for qss in [0, 1, 8, 15] {
            for count in [2usize, 8, 160] {
                for b_present in [false, true] {
                    let input: Vec<i32> =
                        (0..count).map(|i| (i as i32 * 131) % 2001 - 1000).collect();
                    let case = PredictCase {
                        a: MlpPredictFilter::default(),
                        b: b_present.then(MlpPredictFilter::default),
                        state: MlpPredictState {
                            input: [1.5; 8],
                            residual: [-2.5; 4],
                        },
                        qss,
                        input: input.clone(),
                        output_len: count,
                        update_flag: 0,
                    };
                    assert_case_matches_reference(&case);
                    // With every coefficient exactly zero the prediction is 0.0,
                    // so the emitted value is ceil(sample / 2^qss).
                    let divisor = f64::from(1u32 << qss);
                    let expected: Vec<i32> = input
                        .iter()
                        .map(|&sample| (f64::from(sample) / divisor).ceil() as i32)
                        .collect();
                    assert_eq!(run_predict(&case, mlp_predict_block).output, expected);
                }
            }
        }
    }

    #[test]
    fn shortened_traversal_keeps_nonzero_tail_coefficients() {
        // Synthetic non-zero tail beyond order. Production search always leaves
        // an exact +/-0.0 tail, but the scan must still include such values.
        let mut a = MlpPredictFilter {
            order: 1,
            ..Default::default()
        };
        a.coefficient[7] = 0.5;
        let mut state = MlpPredictState::default();
        state.input[7] = 100.0;
        let mut case = PredictCase {
            a,
            b: None,
            state,
            qss: 0,
            input: vec![1, 0],
            output_len: 2,
            update_flag: 0,
        };
        assert_case_matches_reference(&case);
        let mut a = case.a.clone();
        let mut state = case.state;
        let mut output = [0; 2];
        mlp_predict_block(&mut a, None, &mut state, 0, &case.input, &mut output, 0).unwrap();
        assert_eq!(output[0], -49);

        let mut b = MlpPredictFilter {
            order: 1,
            ..Default::default()
        };
        b.coefficient[3] = 2.0;
        case.a = MlpPredictFilter::default();
        case.b = Some(b);
        case.state = MlpPredictState {
            residual: [0.0, 0.0, 0.0, 10.0],
            ..Default::default()
        };
        case.input = vec![5, 5];
        assert_case_matches_reference(&case);
        let mut a = case.a.clone();
        let mut b = case.b.clone();
        let mut state = case.state;
        let mut output = [0; 2];
        mlp_predict_block(
            &mut a,
            b.as_mut(),
            &mut state,
            0,
            &case.input,
            &mut output,
            0,
        )
        .unwrap();
        assert_eq!(output[0], -15);
    }

    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0
        }

        fn below(&mut self, limit: u64) -> u64 {
            self.next() % limit
        }
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "Deterministic fixtures deliberately mix exact zeros, subnormals, and huge finite coefficients"
    )]
    fn random_filter(rng: &mut Lcg, maximum_order: u32) -> MlpPredictFilter {
        let mut filter = MlpPredictFilter::default();
        filter.order = rng.below(u64::from(maximum_order) + 1) as u32;
        for coefficient in &mut filter.coefficient {
            *coefficient = match rng.below(100) {
                0..=9 => 0.0,
                10..=14 => -0.0,
                15..=19 => {
                    if rng.below(2) == 0 {
                        5e-324
                    } else {
                        -5e-324
                    }
                }
                20..=24 => f64::MAX,
                25..=29 => 1.0e200,
                _ => (rng.next() as f64 / (u64::MAX / 8) as f64) - 4.0,
            };
        }
        filter
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "Deterministic fixtures deliberately include non-finite and large finite state values"
    )]
    fn random_f32(rng: &mut Lcg) -> f32 {
        match rng.below(50) {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            _ => (((rng.next() as f64 / (u64::MAX / 64) as f64) - 2.0) * 1000.0) as f32,
        }
    }

    fn random_input(rng: &mut Lcg, count: usize) -> Vec<i32> {
        (0..count)
            .map(|_| match rng.below(100) {
                0 => -8_388_608,
                1 => 8_388_607,
                2 => 9_000_000,
                _ => (rng.below(1_000_001) as i32) - 500_000,
            })
            .collect()
    }

    #[test]
    fn shortened_traversal_matches_reference_deterministically() {
        let mut rng = Lcg(0x0f1e_2d3c_4b5a_6978);
        for case_index in 0..600 {
            let count = 2 * (rng.below(80) as usize + 1);
            let a = random_filter(&mut rng, 8);
            let b = if rng.below(3) != 0 {
                Some(random_filter(&mut rng, 4))
            } else {
                None
            };
            let state = MlpPredictState {
                input: std::array::from_fn(|_| random_f32(&mut rng)),
                residual: std::array::from_fn(|_| random_f32(&mut rng)),
            };
            let mut case = PredictCase {
                a,
                b,
                state,
                qss: rng.below(16) as u32,
                input: random_input(&mut rng, count),
                output_len: count,
                update_flag: rng.below(2) as u32,
            };
            match case_index % 20 {
                7 => case.input.push(0),
                11 => case.output_len = count.saturating_sub(1),
                13 => case.qss = 16,
                _ => {}
            }
            assert_case_matches_reference(&case);
        }
    }

    fn bench_fixture(
        order_a: u32,
        order_b: u32,
    ) -> (
        MlpPredictFilter,
        Option<MlpPredictFilter>,
        MlpPredictState,
        [i32; 160],
    ) {
        let coefficients_a = [
            -1.5, 0.75, -0.375, 0.1875, -0.09375, 0.046875, -0.0234375, 0.01171875,
        ];
        let mut a = MlpPredictFilter {
            order: order_a,
            ..Default::default()
        };
        a.coefficient[..order_a as usize].copy_from_slice(&coefficients_a[..order_a as usize]);
        let coefficients_b = [-0.5, 0.25, -0.125, 0.0625];
        let mut b = MlpPredictFilter {
            order: order_b,
            ..Default::default()
        };
        b.coefficient[..order_b as usize].copy_from_slice(&coefficients_b[..order_b as usize]);
        let state = MlpPredictState {
            input: std::array::from_fn(|i| (i as f32 - 4.0) * 3.5),
            residual: std::array::from_fn(|i| (1.5 - i as f32) * 2.25),
        };
        let input = std::array::from_fn(|i| (i as i32 * 53) % 401 - 200);
        (a, Some(b), state, input)
    }

    fn bench_run(
        implementation: PredictImplementation,
        a: &MlpPredictFilter,
        b: &Option<MlpPredictFilter>,
        state: &MlpPredictState,
        qss: u32,
        input: &[i32],
        expected: &[i32],
        blocks: usize,
    ) -> (Duration, u64) {
        let started = Instant::now();
        let mut checksum = 0u64;
        for _ in 0..blocks {
            let mut filter_a = a.clone();
            let mut filter_b = b.clone();
            let mut work = *state;
            let mut output = [0i32; 160];
            let result = implementation(
                &mut filter_a,
                filter_b.as_mut(),
                &mut work,
                qss,
                input,
                &mut output,
                0,
            );
            assert_eq!(result, Ok(PredictionOutcome::Predicted));
            assert_eq!(&output[..input.len()], expected);
            checksum = checksum.wrapping_add(u64::from(output[input.len() - 1] as u32));
        }
        (started.elapsed(), checksum)
    }

    #[test]
    #[ignore = "deterministic old-vs-new microbenchmark; run with --ignored --nocapture"]
    fn microbench_shortened_traversal_against_reference() {
        const ROUNDS: usize = 3;
        const BLOCKS: usize = 20_000;
        for (order_a, order_b) in [(3u32, 2u32), (8u32, 4u32)] {
            let (a, b, state, input) = bench_fixture(order_a, order_b);
            let mut expected = [0i32; 160];
            let mut expected_a = a.clone();
            let mut expected_b = b.clone();
            let mut expected_state = state;
            reference_predict_block(
                &mut expected_a,
                expected_b.as_mut(),
                &mut expected_state,
                8,
                &input,
                &mut expected,
                0,
            )
            .unwrap();
            let _ = bench_run(
                reference_predict_block,
                &a,
                &b,
                &state,
                8,
                &input,
                &expected,
                100,
            );
            let _ = bench_run(mlp_predict_block, &a, &b, &state, 8, &input, &expected, 100);
            for round in 0..ROUNDS {
                let (reference_time, reference_checksum) = bench_run(
                    reference_predict_block,
                    &a,
                    &b,
                    &state,
                    8,
                    &input,
                    &expected,
                    BLOCKS,
                );
                let (shortened_time, shortened_checksum) = bench_run(
                    mlp_predict_block,
                    &a,
                    &b,
                    &state,
                    8,
                    &input,
                    &expected,
                    BLOCKS,
                );
                assert_eq!(reference_checksum, shortened_checksum);
                println!(
                    "order_a={order_a} order_b={order_b} round={round} reference={:.3}ms shortened={:.3}ms checksum={reference_checksum}",
                    reference_time.as_secs_f64() * 1000.0,
                    shortened_time.as_secs_f64() * 1000.0,
                );
            }
        }
    }
}
