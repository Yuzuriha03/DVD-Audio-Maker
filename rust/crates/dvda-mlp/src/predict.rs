use crate::entropy::{self, Coding};

#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub filter: Vec<i32>,
    pub iir: Vec<i32>,
    pub residual: Vec<i32>,
    pub state: Vec<i32>,
    pub coding: Coding,
}

fn adaptive(samples: &[i32], history: &[i32], order: usize) -> Option<Vec<i32>> {
    let mut equations = [[0f64; 9]; 8];
    for (n, &x) in samples.iter().enumerate() {
        if history.len() + n < order {
            continue;
        }
        let prior: Vec<f64> = (0..order)
            .map(|k| {
                let index = history.len() + n - k - 1;
                f64::from(if index < history.len() {
                    history[index]
                } else {
                    samples[index - history.len()]
                })
            })
            .collect();
        for row in 0..order {
            for col in 0..order {
                equations[row][col] += prior[row] * prior[col];
            }
            equations[row][order] += prior[row] * f64::from(x);
        }
    }
    for col in 0..order {
        let pivot = (col..order)
            .max_by(|&a, &b| equations[a][col].abs().total_cmp(&equations[b][col].abs()))?;
        equations.swap(col, pivot);
        let scale = equations[col][col];
        if scale.abs() < 1e-12 {
            return None;
        }
        for value in &mut equations[col][col..=order] {
            *value /= scale;
        }
        let pivot_row = equations[col];
        for (row, equation) in equations.iter_mut().enumerate().take(order) {
            if row == col {
                continue;
            }
            let factor = equation[col];
            for (value, pivot) in equation[col..=order]
                .iter_mut()
                .zip(&pivot_row[col..=order])
            {
                *value -= factor * pivot;
            }
        }
    }
    let coefficients: Vec<i32> = (0..order)
        .map(|k| (equations[k][order] * 256.0).round() as i32)
        .collect();
    coefficients
        .iter()
        .all(|&x| (-2048..=2047).contains(&x))
        .then_some(coefficients)
}
/// Evaluate exact fixed-point FIR candidates with the decoder's retained PCM history.
pub(crate) fn select(
    samples: &[i32],
    history: &[i32],
    residual_history: &[i32],
    qss: u32,
    old: Coding,
    old_filter: &[i32],
    old_iir: &[i32],
) -> Plan {
    let mut best = Plan {
        filter: Vec::new(),
        iir: Vec::new(),
        residual: samples.to_vec(),
        state: samples.iter().map(|&x| x << qss).collect(),
        coding: entropy::select(samples, qss, old),
    };
    let cost = |plan: &Plan| -> u64 {
        let payload: u64 = plan
            .residual
            .iter()
            .map(|&x| u64::from(plan.coding.word(x, qss).unwrap().1))
            .sum();
        payload
            + if plan.coding == old { 0 } else { 25 }
            + if plan.filter == old_filter {
                0
            } else {
                18 + 12 * plan.filter.len() as u64
            }
            + if plan.iir == old_iir {
                0
            } else {
                18 + 12 * plan.iir.len() as u64
            }
    };
    let mut best_cost = cost(&best);
    let mut candidates = vec![
        vec![256],
        vec![512, -256],
        vec![-256],
        vec![128],
        vec![384, -128],
    ];
    for order in 1..=8 {
        if let Some(filter) = adaptive(samples, history, order) {
            candidates.push(filter);
        }
    }
    for filter in candidates {
        let mut residual = Vec::with_capacity(samples.len());
        let mut state = Vec::with_capacity(samples.len());
        for (n, &x) in samples.iter().enumerate() {
            let mut prediction = 0i64;
            for (k, &coefficient) in filter.iter().enumerate() {
                let index = history.len() + n;
                let sample = if index > k {
                    let index = index - k - 1;
                    if index < history.len() {
                        history[index]
                    } else {
                        samples[index - history.len()]
                    }
                } else {
                    0
                };
                prediction += (i64::from(sample) << qss) * i64::from(coefficient);
            }
            let difference = (i64::from(x) << qss) - (prediction >> 8);
            let value = (difference + (1i64 << qss) - 1) >> qss;
            let bound = 1i64 << (23 - qss);
            if value < -bound || value >= bound {
                break;
            }
            residual.push(value as i32);
            state.push(difference as i32);
        }
        if residual.len() != samples.len() {
            continue;
        }
        let coding = entropy::select(&residual, qss, old);
        let plan = Plan {
            filter,
            iir: Vec::new(),
            residual,
            state,
            coding,
        };
        let candidate = cost(&plan);
        if candidate < best_cost {
            best_cost = candidate;
            best = plan;
        }
    }
    for fir in [vec![0], best.filter.clone()] {
        // Sum of absolute Q8 coefficients below256 is a sufficient stability bound.
        for iir in [
            vec![64],
            vec![128],
            vec![-128],
            vec![192],
            vec![-64],
            vec![128, 64],
            vec![128, 32, 16],
            vec![128, 32, 16, 8],
        ] {
            if fir.len() + iir.len() > 8 {
                continue;
            }
            let mut residual: Vec<i32> = Vec::with_capacity(samples.len());
            let mut state: Vec<i32> = Vec::with_capacity(samples.len());
            for (n, &x) in samples.iter().enumerate() {
                let mut prediction = 0i64;
                for (prefix, values, coefficients, shift) in [
                    (history, samples, fir.as_slice(), qss),
                    (residual_history, state.as_slice(), iir.as_slice(), 0),
                ] {
                    for (k, &coefficient) in coefficients.iter().enumerate() {
                        let index = prefix.len() + n;
                        let sample = if index > k {
                            let index = index - k - 1;
                            if index < prefix.len() {
                                prefix[index]
                            } else {
                                values[index - prefix.len()]
                            }
                        } else {
                            0
                        };
                        prediction += (i64::from(sample) << shift) * i64::from(coefficient);
                    }
                }
                let difference = (i64::from(x) << qss) - (prediction >> 8);
                let value = (difference + (1i64 << qss) - 1) >> qss;
                let bound = 1i64 << (23 - qss);
                if value < -bound || value >= bound {
                    break;
                }
                residual.push(value as i32);
                state.push(difference as i32);
            }
            if residual.len() != samples.len() {
                continue;
            }
            let coding = entropy::select(&residual, qss, old);
            let plan = Plan {
                filter: fir.clone(),
                iir,
                residual,
                state,
                coding,
            };
            let candidate = cost(&plan);
            if candidate < best_cost {
                best_cost = candidate;
                best = plan;
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ramp_selects_prediction_and_reconstructs_exactly() {
        let samples: Vec<i32> = (8..40).map(|n| n * 100).collect();
        let history = [600, 700];
        let plan = select(&samples, &history, &history, 0, Coding::default(), &[], &[]);
        assert!(!plan.filter.is_empty());
        let mut decoded = history.to_vec();
        for &residual in &plan.residual {
            let prediction: i64 = plan
                .filter
                .iter()
                .enumerate()
                .map(|(k, &c)| i64::from(decoded[decoded.len() - k - 1]) * i64::from(c))
                .sum();
            decoded.push(residual + (prediction >> 8) as i32);
        }
        assert_eq!(&decoded[2..], samples);
    }
    #[test]
    fn quantized_filter_transitions_reconstruct_decoder_state() {
        for qss in [0, 4, 8] {
            let mut history = vec![0i32; 8];
            let mut states = vec![0i32; 4];
            let mut previous = Plan {
                filter: vec![],
                iir: vec![],
                residual: vec![],
                state: vec![],
                coding: Coding::default(),
            };
            let mut chosen_iir = false;
            for block in 0..128 {
                let samples: Vec<_> = (0..40)
                    .map(|n| {
                        if block % 3 == 0 {
                            ((block * 40 + n) % 200 - 100) * 20
                        } else if block % 3 == 1 {
                            if n % 2 == 0 { 10000 } else { -10000 }
                        } else {
                            ((n * 193 + block * 971) % 16001) - 8000
                        }
                    })
                    .collect();
                let plan = select(
                    &samples,
                    &history,
                    &states,
                    qss,
                    previous.coding,
                    &previous.filter,
                    &previous.iir,
                );
                chosen_iir |= !plan.iir.is_empty();
                assert!(plan.filter.len() + plan.iir.len() <= 8);
                let mut decoded: Vec<_> = history.iter().map(|&x| x << qss).collect();
                let mut decoded_states = states.clone();
                for (n, &residual) in plan.residual.iter().enumerate() {
                    let sum = |values: &[i32], coefficients: &[i32]| -> i64 {
                        coefficients
                            .iter()
                            .enumerate()
                            .map(|(k, &c)| i64::from(values[values.len() - k - 1]) * i64::from(c))
                            .sum()
                    };
                    let prediction =
                        (sum(&decoded, &plan.filter) + sum(&decoded_states, &plan.iir)) >> 8;
                    let result = (prediction + (i64::from(residual) << qss)) & !((1i64 << qss) - 1);
                    assert_eq!(result, i64::from(samples[n]) << qss);
                    assert_eq!(result - prediction, i64::from(plan.state[n]));
                    decoded.push(result as i32);
                    decoded_states.push((result - prediction) as i32);
                }
                history = samples[32..].to_vec();
                states = decoded_states[decoded_states.len() - 4..].to_vec();
                previous = plan;
            }
            assert!(chosen_iir, "test must exercise IIR at precision {qss}");
        }
    }
}
