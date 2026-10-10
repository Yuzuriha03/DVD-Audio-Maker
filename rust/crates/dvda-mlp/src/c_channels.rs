#[cfg(test)]
use crate::c_encode::{IntervalOutput, PlannedAu};
#[cfg(test)]
use crate::c_predict::{MlpPredictFilter, MlpPredictState, mlp_predict_block};
use crate::parameters::MlpCodingParams;
#[cfg(test)]
use crate::parameters::MlpParameters;
#[cfg(test)]
use crate::search::MlpSearchPlan;

pub(crate) fn select(
    samples: &[i32],
    qss: u32,
    old: MlpCodingParams,
    first: bool,
    limit: u32,
) -> MlpCodingParams {
    let c = crate::entropy::select_with_context(
        samples,
        qss,
        crate::entropy::Coding {
            mode: old.mode as u32,
            width: old.total_width as u32,
            offset: old.offset,
        },
        first,
        limit,
    );
    MlpCodingParams {
        mode: c.mode as i32,
        total_width: c.width as i32,
        offset: c.offset,
    }
}

#[cfg(test)]
pub(crate) struct ChannelContext<'a> {
    pub base: &'a MlpParameters,
    pub history: &'a mut [MlpPredictState; 6],
    pub prediction_reset: &'a mut [u32; 6],
    pub search: &'a [Option<MlpSearchPlan>; 6],
    pub restart: bool,
    pub predict: bool,
    pub channels: usize,
}

#[cfg(test)]
pub(crate) fn encode_channels(
    p: &mut PlannedAu,
    pcm: &[i32],
    context: &mut ChannelContext<'_>,
    pending: &mut IntervalOutput,
) -> Result<(), ()> {
    let channels = context.channels;
    if !(1..=6).contains(&channels)
        || !(8..=160).contains(&p.count)
        || pcm.len() < p.count * channels
        || p.bypass_bits > 8
    {
        return Err(());
    }
    let first = if context.restart && p.count >= 16 {
        8
    } else {
        0
    };
    p.residual.resize(p.count * channels, 0);
    p.unpredicted.resize(p.count * channels, 0);
    for ch in 0..channels {
        let qss = p.main.qss[ch];
        if qss > 15 {
            return Err(());
        }
        let limit = 32
            - if ch == 0 || ch == channels - 1 {
                p.bypass_bits
            } else {
                0
            };
        let input: Vec<i32> = (0..p.count).map(|i| pcm[i * channels + ch]).collect();
        let mut output = vec![0i32; p.count];
        for (i, &value) in input.iter().enumerate() {
            p.unpredicted[i * channels + ch] = value / (1i32 << qss);
        }
        p.initial.qss[ch] = qss;
        for i in 0..first {
            output[i] = input[i] / (1i32 << qss);
            context.history[ch].input[i] = input[7 - i] as f32;
        }
        if first != 0 {
            p.initial.coding[ch] =
                select(&output[..first], qss, context.base.coding[ch], true, limit);
        }
        let mut a = MlpPredictFilter::default();
        let mut b = MlpPredictFilter::default();
        let mut rc = 0;
        let searched = context.search[ch].is_some()
            && (!context.restart || first != 0)
            && (p.count - first) & 1 == 0;
        if context.predict && (!context.restart || first != 0) && (p.count - first) & 1 == 0 {
            a.order = 1;
            a.coefficient[0] = 1.0;
        }
        if searched {
            let plan = context.search[ch].as_ref().ok_or(())?;
            p.main.a[ch] = plan.a.clone();
            p.main.b[ch] = plan.b.clone();
            a.order = plan.a.order;
            b.order = plan.b.order;
            a.coefficient = plan.a.coefficient;
            b.coefficient = plan.b.coefficient;
            if context.restart && b.order != 0 {
                p.main.state[ch] = plan.state.clone();
                for i in 0..b.order as usize {
                    context.history[ch].residual[i] = plan.state.value[i] as f32;
                }
            }
        }
        if (p.count - first) & 1 != 0 {
            for i in first..p.count {
                output[i] = input[i] / (1i32 << qss);
                for k in (1..8).rev() {
                    context.history[ch].input[k] = context.history[ch].input[k - 1];
                }
                context.history[ch].input[0] = input[i] as f32;
            }
        } else if p.count > first {
            let feedback = b.order != 0;
            rc = mlp_predict_block(
                &mut a,
                if feedback { Some(&mut b) } else { None },
                &mut context.history[ch],
                qss,
                &input[first..],
                &mut output[first..],
                context.prediction_reset[ch],
            );
        }
        if rc < 0 {
            return Err(());
        }
        if a.order == 0 && !context.restart && p.previous.a[ch].order != 0 {
            a.order = 1;
            a.coefficient = [0.0; 8];
        }
        p.main.a[ch].order = a.order;
        if !searched {
            p.main.a[ch].precision = 8;
        }
        p.main.a[ch].coefficient = a.coefficient;
        p.main.a[ch].changed = u32::from(
            (context.restart && a.order != 0)
                || a.order != p.previous.a[ch].order
                || a.coefficient[..a.order as usize]
                    .iter()
                    .zip(&p.previous.a[ch].coefficient)
                    .any(|(a, b)| a.to_bits() != b.to_bits()),
        );
        p.main.b[ch].order = b.order;
        p.main.b[ch].coefficient = b.coefficient;
        p.main.b[ch].changed = u32::from(
            b.order != p.previous.b[ch].order
                || b.coefficient
                    .iter()
                    .zip(&p.previous.b[ch].coefficient)
                    .any(|(a, b)| a.to_bits() != b.to_bits()),
        );
        if rc == 1 && context.prediction_reset[ch] != 0 {
            p.main.a[ch] = p.previous.a[ch].clone();
            p.main.b[ch] = p.previous.b[ch].clone();
            p.main.a[ch].changed = 0;
            p.main.b[ch].changed = 0;
        }
        context.prediction_reset[ch] = u32::from(rc == 1);
        if b.order == 0 {
            p.main.state[ch].changed = 0;
        }
        p.main.coding[ch] = select(
            &output[first..],
            qss,
            if first != 0 {
                p.initial.coding[ch]
            } else {
                p.previous.coding[ch]
            },
            false,
            limit,
        );
        pending.maximum_lsbs = pending
            .maximum_lsbs
            .max(p.main.coding[ch].total_width as u32);
        if first != 0 {
            pending.maximum_lsbs = pending
                .maximum_lsbs
                .max(p.initial.coding[ch].total_width as u32);
        }
        for (i, &value) in output.iter().enumerate() {
            p.residual[i * channels + ch] = value;
        }
    }
    Ok(())
}
