use crate::Bits;
use crate::c_restart::MlpRestart;
use crate::c_substream::{MlpSubstream, mlp_substream_put};
use crate::parameters::{MlpCodingParams, MlpParameters};

pub(crate) struct PlannedAu {
    pub initial: MlpParameters,
    pub main: MlpParameters,
    pub previous: MlpParameters,
    pub residual: Vec<i32>,
    pub unpredicted: Vec<i32>,
    pub bypass: Vec<u8>,
    pub count: usize,
    pub bypass_bits: u32,
    pub single_restart: bool,
    pub end_markers: bool,
    pub stamp: u32,
}

pub(crate) struct IntervalOutput {
    pub au: Vec<PlannedAu>,
    pub maximum_lsbs: u32,
    pub restart: MlpRestart,
    pub failure: Option<&'static str>,
}

pub(crate) struct OutputFormat {
    pub channels: usize,
    pub sample_rate: u32,
    pub rate_field: u32,
    pub major: [u32; 14],
}

pub(crate) fn flush_interval(
    queue: &mut crate::c_output_queue::MlpOutputQueue,
    pending: &mut IntervalOutput,
    format: &OutputFormat,
    rate: &mut crate::c_rate::MlpRateState,
    words_total: &mut u64,
    emit: &mut impl FnMut(&[u32]) -> bool,
) -> bool {
    if pending.au.is_empty() {
        return true;
    }
    let mut fits = interval_fits(pending, format.channels);
    if fits == Ok(false) {
        if unpredicted_interval(pending, format.channels).is_err() {
            pending.failure = Some("Cannot build lossless oversized-AU fallback");
            return false;
        }
        fits = interval_fits(pending, format.channels);
    }
    if fits != Ok(true) {
        pending.failure = Some(if fits == Ok(false) {
            "Access unit too large after lossless fallback (1536-byte limit)"
        } else {
            "Cannot serialize lossless access unit"
        });
        return false;
    }
    for au in 0..pending.au.len() {
        let p = &pending.au[au];
        let mut writer = Bits::default();
        if serialize_planned(&mut writer, pending, au, format.channels).is_err() {
            return false;
        }
        let is_restart = au == 0;
        let length = (if is_restart { 17 } else { 3 }) + writer.words.len() as u32;
        let mut next = i32::from(rate.arrival);
        let earliest = i32::from(rate.decode) - (format.sample_rate * 75 / 1000) as i32;
        if next > i32::from(rate.decode) + 0x4000 {
            next -= 0x10000;
        }
        if next < earliest {
            rate.arrival = earliest as u16;
        }
        let mut flags = 0;
        let mut arrival = 0;
        if crate::c_rate::mlp_rate_update(
            rate,
            p.count as u32,
            length,
            format.rate_field,
            &mut flags,
            &mut arrival,
        ) != 0
            || flags & 0x2000 != 0
        {
            pending.failure = Some(if flags & 0x2000 != 0 {
                "Access unit too large (1536-byte limit)"
            } else {
                "MLP FIFO rate limit exceeded; input cannot be encoded at the required delivery rate"
            });
            return false;
        }
        let directory =
            (if is_restart { 0x2000 } else { 0x6000 }) | writer.words.len() as u32 | p.stamp << 12;
        let mut value = length ^ u32::from(arrival) ^ directory;
        let mut parity = 15;
        while value != 0 {
            parity ^= value & 15;
            value >>= 4;
        }
        let mut words = Vec::with_capacity(length as usize);
        words.push(parity << 12 | length);
        words.push(u32::from(arrival));
        if is_restart {
            words.extend_from_slice(&format.major);
        }
        words.push(directory);
        words.extend_from_slice(&writer.words);
        if words.len() != length as usize
            || !queue.mlp_output_queue_push(&words, p.count as u32, emit)
        {
            return false;
        }
        *words_total += u64::from(length);
    }
    pending.maximum_lsbs = 0;
    pending.au.clear();
    true
}

#[allow(
    clippy::too_many_arguments,
    reason = "Preserve the original C encode call boundary"
)]
pub(crate) fn encode(
    input: &mut impl crate::c_prepare::PcmInput,
    format: &OutputFormat,
    restart_interval: u32,
    cycle: u32,
    predict: bool,
    mut matrix: Option<&mut crate::c_prepare::MatrixInterval>,
    allow_bypass: bool,
    pending: &mut IntervalOutput,
    queue: &mut crate::c_output_queue::MlpOutputQueue,
    mut stamp: Option<&mut crate::c_stamp::MlpStampState<'_>>,
    emit: &mut impl FnMut(&[u32]) -> bool,
) -> Result<(), ()> {
    use crate::c_prepare::{lossless, prepare_matrix};
    let channels = input.channels();
    if channels != format.channels
        || !(1..=6).contains(&channels)
        || !(1..=128).contains(&restart_interval)
        || !(8..=160).contains(&input.au_samples())
        || ![16, 20, 24].contains(&input.bits())
        || input.position() != 0
    {
        return Err(());
    }
    let mut base = MlpParameters {
        maximum_channel: channels as u32 - 1,
        ..Default::default()
    };
    for ch in 0..channels {
        base.coding[ch].total_width = 24;
    }
    let mut previous = base.clone();
    let mut history: [crate::c_predict::MlpPredictState; 6] = Default::default();
    let mut prediction_reset = [0; 6];
    let mut rate = crate::c_rate::mlp_rate_init();
    let mut boundary = Default::default();
    let mut au = 0u32;
    let mut timing = 0u32;
    let mut interval_check = 0;
    let mut noise_seed = 2;
    let mut words_total = 0;
    let mut frames = 0;
    let total_frames = input.frames();
    if let Some(m) = matrix.as_deref_mut() {
        crate::c_analysis::mlp_matrix_analysis_init(&mut m.analysis, channels as u32)?;
        if m.joint_search {
            m.search_rng = 1;
            for pool in &mut m.pool {
                crate::search::mlp_search_pool_init(pool, m.search_rng);
                m.search_rng = pool.rng;
            }
        }
    }
    while frames < total_frames {
        let mut span = restart_interval as usize;
        let is_restart = if cycle != 0 {
            let decision = crate::interval::mlp_interval_boundary(
                &mut boundary,
                cycle,
                restart_interval,
                input.au_samples() as u32,
            );
            if decision < 0 {
                return Err(());
            }
            if matrix.is_some() && decision != 0 {
                let mut ahead = boundary;
                span = 1;
                while crate::interval::mlp_interval_boundary(
                    &mut ahead,
                    cycle,
                    restart_interval,
                    input.au_samples() as u32,
                ) == 0
                {
                    span += 1;
                    if span > 128 {
                        return Err(());
                    }
                }
                if matrix.as_ref().is_some_and(|m| m.original_scale) && span > 32 {
                    return Err(());
                }
            }
            decision != 0
        } else {
            au.is_multiple_of(restart_interval)
        };
        if is_restart && !flush_interval(queue, pending, format, &mut rate, &mut words_total, emit)
        {
            return Err(());
        }
        let mut p = base.clone();
        let mut initial = base.clone();
        let mut restart = MlpRestart::default();
        let mut search: [Option<crate::search::MlpSearchPlan>; 6] = Default::default();
        let (pcm, count, bypass, bypass_bits) = if let Some(m) = matrix.as_deref_mut() {
            if m.slot == m.lengths.len() {
                if !is_restart {
                    return Err(());
                }
                prepare_matrix(input, m, span, allow_bypass, &mut 0)?;
            }
            let count = m.lengths[m.slot];
            p.matrix_count = m.count as u32;
            initial.matrix_count = m.count as u32;
            p.matrix[..m.count].copy_from_slice(&m.matrix[..m.count]);
            initial.matrix[..m.count].copy_from_slice(&m.matrix[..m.count]);
            p.matrix_changed = u32::from(is_restart && m.selected_matrix);
            initial.matrix_changed = p.matrix_changed;
            for (ch, plan) in search.iter_mut().enumerate().take(channels) {
                if m.original_scale {
                    p.output_shift[ch] = m.scale.shift[ch] as i32;
                    initial.output_shift[ch] = p.output_shift[ch];
                }
                if m.joint_search && m.search_ready[ch] {
                    *plan = Some(m.prediction[ch].clone());
                }
            }
            (
                m.pcm[m.offset * channels..(m.offset + count) * channels].to_vec(),
                count,
                m.bypass[m.offset..m.offset + count].to_vec(),
                m.bits,
            )
        } else {
            let remaining = total_frames - input.position();
            let mut capacity = input.au_samples();
            if remaining > capacity && remaining < capacity + 8 {
                capacity = remaining - 8;
            }
            let count = capacity.min(remaining);
            let pcm = input.read(count)?;
            if pcm.len() != count * channels {
                return Err(());
            }
            (pcm, count, Vec::new(), 0)
        };
        let first = if is_restart && count >= 16 { 8 } else { 0 };
        p.flags = u32::from(is_restart || count != previous.blocksize as usize) * 2;
        p.blocksize = count as u32;
        initial.flags = 0;
        initial.blocksize = 8;
        if is_restart {
            previous = base.clone();
            history = Default::default();
            prediction_reset = [1; 6];
            restart.maximum_channel = channels as u32 - 1;
            restart.seed = noise_seed;
            restart.maximum_lsbs = 24;
            restart.maximum_bits = 25;
            if let Some(m) = matrix.as_ref().filter(|m| m.original_scale) {
                restart.maximum_shift = m.scale.maximum_shift;
                restart.maximum_bits = m.scale.maximum_bits;
                restart.dither_shift = m.scale.qss[0][0].saturating_sub(8);
            }
            restart.timing = timing;
            restart.lossless_check = interval_check;
            interval_check = 0;
        }
        interval_check = if let Some(m) = matrix.as_ref() {
            interval_check ^ m.checks[m.slot]
        } else {
            lossless(interval_check, &pcm, count, channels)
        };
        for ch in 0..channels {
            p.qss[ch] = if let Some(m) = matrix.as_ref().filter(|m| m.original_scale) {
                m.scale.qss[m.slot][ch]
            } else {
                24 - input.bits()
            };
            initial.qss[ch] = p.qss[ch];
        }
        let mut planned = PlannedAu {
            initial,
            main: p,
            previous: previous.clone(),
            residual: Vec::new(),
            unpredicted: Vec::new(),
            bypass,
            count,
            bypass_bits,
            single_restart: is_restart && first == 0,
            end_markers: frames + count == total_frames,
            stamp: 0,
        };
        planned.residual.resize(planned.count * channels, 0);
        planned.unpredicted.resize(planned.count * channels, 0);
        for ch in 0..channels {
            let qss = planned.main.qss[ch];
            if qss > 15 {
                return Err(());
            }
            let limit = 32
                - if ch == 0 || ch == channels - 1 {
                    planned.bypass_bits
                } else {
                    0
                };
            let input: Vec<i32> = (0..planned.count).map(|i| pcm[i * channels + ch]).collect();
            let mut output = vec![0i32; planned.count];
            for (i, &value) in input.iter().enumerate() {
                planned.unpredicted[i * channels + ch] = value / (1i32 << qss);
            }
            planned.initial.qss[ch] = qss;
            for i in 0..first {
                output[i] = input[i] / (1i32 << qss);
                history[ch].input[i] = input[7 - i] as f32;
            }
            if first != 0 {
                planned.initial.coding[ch] =
                    crate::c_channels::select(&output[..first], qss, base.coding[ch], true, limit);
            }
            let mut a = crate::c_predict::MlpPredictFilter::default();
            let mut b = crate::c_predict::MlpPredictFilter::default();
            let mut rc = 0;
            let searched = search[ch].is_some()
                && (!is_restart || first != 0)
                && (planned.count - first) & 1 == 0;
            if predict && (!is_restart || first != 0) && (planned.count - first) & 1 == 0 {
                a.order = 1;
                a.coefficient[0] = 1.0;
            }
            if searched {
                let plan = search[ch].as_ref().ok_or(())?;
                planned.main.a[ch] = plan.a.clone();
                planned.main.b[ch] = plan.b.clone();
                a.order = plan.a.order;
                b.order = plan.b.order;
                a.coefficient = plan.a.coefficient;
                b.coefficient = plan.b.coefficient;
                if is_restart && b.order != 0 {
                    planned.main.state[ch] = plan.state.clone();
                    for i in 0..b.order as usize {
                        history[ch].residual[i] = plan.state.value[i] as f32;
                    }
                }
            }
            if (planned.count - first) & 1 != 0 {
                for i in first..planned.count {
                    output[i] = input[i] / (1i32 << qss);
                    for k in (1..8).rev() {
                        history[ch].input[k] = history[ch].input[k - 1];
                    }
                    history[ch].input[0] = input[i] as f32;
                }
            } else if planned.count > first {
                let feedback = b.order != 0;
                rc = crate::c_predict::mlp_predict_block(
                    &mut a,
                    if feedback { Some(&mut b) } else { None },
                    &mut history[ch],
                    qss,
                    &input[first..],
                    &mut output[first..],
                    prediction_reset[ch],
                );
            }
            if rc < 0 {
                return Err(());
            }
            if a.order == 0 && !is_restart && planned.previous.a[ch].order != 0 {
                a.order = 1;
                a.coefficient = [0.0; 8];
            }
            planned.main.a[ch].order = a.order;
            if !searched {
                planned.main.a[ch].precision = 8;
            }
            planned.main.a[ch].coefficient = a.coefficient;
            planned.main.a[ch].changed = u32::from(
                (is_restart && a.order != 0)
                    || a.order != planned.previous.a[ch].order
                    || a.coefficient[..a.order as usize]
                        .iter()
                        .zip(&planned.previous.a[ch].coefficient)
                        .any(|(a, b)| a.to_bits() != b.to_bits()),
            );
            planned.main.b[ch].order = b.order;
            planned.main.b[ch].coefficient = b.coefficient;
            planned.main.b[ch].changed = u32::from(
                b.order != planned.previous.b[ch].order
                    || b.coefficient
                        .iter()
                        .zip(&planned.previous.b[ch].coefficient)
                        .any(|(a, b)| a.to_bits() != b.to_bits()),
            );
            if rc == 1 && prediction_reset[ch] != 0 {
                planned.main.a[ch] = planned.previous.a[ch].clone();
                planned.main.b[ch] = planned.previous.b[ch].clone();
                planned.main.a[ch].changed = 0;
                planned.main.b[ch].changed = 0;
            }
            prediction_reset[ch] = u32::from(rc == 1);
            if b.order == 0 {
                planned.main.state[ch].changed = 0;
            }
            planned.main.coding[ch] = crate::c_channels::select(
                &output[first..],
                qss,
                if first != 0 {
                    planned.initial.coding[ch]
                } else {
                    planned.previous.coding[ch]
                },
                false,
                limit,
            );
            pending.maximum_lsbs = pending
                .maximum_lsbs
                .max(planned.main.coding[ch].total_width as u32);
            if first != 0 {
                pending.maximum_lsbs = pending
                    .maximum_lsbs
                    .max(planned.initial.coding[ch].total_width as u32);
            }
            for (i, &value) in output.iter().enumerate() {
                planned.residual[i * channels + ch] = value;
            }
        }

        if pending.au.len() >= 128 {
            return Err(());
        }
        if let Some(s) = stamp.as_deref_mut()
            && !crate::c_stamp::mlp_stamp_next(s, &mut planned.stamp)
        {
            return Err(());
        }
        if is_restart {
            pending.restart = restart;
        }
        previous = planned.main.clone();
        pending.au.push(planned);
        if crate::matrix::mlp_matrix_noise(
            &mut noise_seed,
            count,
            0,
            &mut vec![0; count],
            &mut vec![0; count],
        ) != 0
        {
            return Err(());
        }
        au += 1;
        timing = (timing + count as u32) & 65535;
        frames += count;
        if let Some(m) = matrix.as_deref_mut() {
            m.slot += 1;
            m.offset += count;
        }
    }
    if stamp
        .as_ref()
        .is_some_and(|s| !crate::c_stamp::mlp_stamp_complete(s))
        || !flush_interval(queue, pending, format, &mut rate, &mut words_total, emit)
        || !queue.mlp_output_queue_finish(emit)
    {
        return Err(());
    }
    if frames != total_frames {
        return Err(());
    }
    Ok(())
}

pub(crate) fn serialize_planned(
    writer: &mut Bits,
    pending: &IntervalOutput,
    au: usize,
    channels: usize,
) -> Result<(), ()> {
    let p = pending.au.get(au).ok_or(())?;
    mlp_substream_put(
        writer,
        &MlpSubstream {
            restart: (au == 0).then_some(&pending.restart),
            initial: Some(&p.initial),
            main: &p.main,
            previous: &p.previous,
            residual: &p.residual,
            bypass: &p.bypass,
            count: p.count,
            stride: channels,
            bypass_bits: p.bypass_bits,
            primary: true,
            single_restart: p.single_restart,
            end_markers: p.end_markers,
        },
    )
}

pub(crate) fn interval_fits(pending: &mut IntervalOutput, channels: usize) -> Result<bool, ()> {
    pending.restart.maximum_lsbs = pending.maximum_lsbs;
    for au in 0..pending.au.len() {
        let mut writer = Bits::default();
        serialize_planned(&mut writer, pending, au, channels)?;
        if writer.words.len() + if au == 0 { 17 } else { 3 } > 0x300 {
            return Ok(false);
        }
    }
    Ok(true)
}

fn select(
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

pub(crate) fn unpredicted_interval(
    pending: &mut IntervalOutput,
    channels: usize,
) -> Result<(), ()> {
    if !(1..=6).contains(&channels) || pending.au.is_empty() {
        return Err(());
    }
    let mut previous = pending.au[0].previous.clone();
    pending.maximum_lsbs = 0;
    for (au, p) in pending.au.iter_mut().enumerate() {
        let first = if au == 0 && !p.single_restart { 8 } else { 0 };
        if p.count <= first || p.count > 160 || p.unpredicted.len() < p.count * channels {
            return Err(());
        }
        p.previous = previous.clone();
        p.main.a = Default::default();
        p.main.b = Default::default();
        p.main.state = Default::default();
        p.initial.a = Default::default();
        p.initial.b = Default::default();
        p.initial.state = Default::default();
        p.residual = p.unpredicted.clone();
        for ch in 0..channels {
            let qss = p.main.qss[ch];
            let limit = 32
                - if ch == 0 || ch == channels - 1 {
                    p.bypass_bits
                } else {
                    0
                };
            let mono: Vec<i32> = (0..p.count)
                .map(|i| p.unpredicted[i * channels + ch])
                .collect();
            p.main.a[ch].precision = 8;
            p.initial.a[ch].precision = 8;
            if first != 0 {
                p.initial.coding[ch] =
                    select(&mono[..first], qss, previous.coding[ch], true, limit);
            }
            p.main.coding[ch] = select(
                &mono[first..],
                qss,
                if first != 0 {
                    p.initial.coding[ch]
                } else {
                    previous.coding[ch]
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
        }
        previous = p.main.clone();
    }
    Ok(())
}

#[test]
fn planned_interval_fallback_serializes() {
    let parameters = MlpParameters {
        blocksize: 16,
        flags: 2,
        coding: [MlpCodingParams {
            mode: 0,
            total_width: 24,
            offset: 0,
        }; 16],
        ..Default::default()
    };
    let mut pending = IntervalOutput {
        au: vec![PlannedAu {
            initial: parameters.clone(),
            main: parameters.clone(),
            previous: parameters,
            residual: vec![0; 16],
            unpredicted: (0..16).collect(),
            bypass: vec![],
            count: 16,
            bypass_bits: 0,
            single_restart: false,
            end_markers: true,
            stamp: 0,
        }],
        failure: None,
        maximum_lsbs: 24,
        restart: MlpRestart {
            maximum_bits: 25,
            ..Default::default()
        },
    };
    let mut history: [crate::c_predict::MlpPredictState; 6] = Default::default();
    let mut reset = [1; 6];
    let base = pending.au[0].previous.clone();
    let mut planned = pending.au.remove(0);
    crate::c_channels::encode_channels(
        &mut planned,
        &(0..16).collect::<Vec<_>>(),
        &mut crate::c_channels::ChannelContext {
            base: &base,
            history: &mut history,
            prediction_reset: &mut reset,
            search: &Default::default(),
            restart: true,
            predict: true,
            channels: 1,
        },
        &mut pending,
    )
    .unwrap();
    pending.au.push(planned);
    assert!(interval_fits(&mut pending, 1).unwrap());
    unpredicted_interval(&mut pending, 1).unwrap();
    assert!(interval_fits(&mut pending, 1).unwrap());
    let mut writer = Bits::default();
    serialize_planned(&mut writer, &pending, 0, 1).unwrap();
    assert!(!writer.words.is_empty());
    assert_eq!(pending.restart.maximum_lsbs, pending.maximum_lsbs);
    let mut queue = crate::c_output_queue::MlpOutputQueue::mlp_output_queue_init();
    let mut rate = crate::c_rate::mlp_rate_init();
    let mut total = 0;
    let mut emitted = Vec::new();
    let mut emit = |words: &[u32]| {
        emitted.extend_from_slice(words);
        true
    };
    let mut major = [0; 14];
    major[0] = 0xf872;
    major[1] = 0x6fbb;
    major[7] = 0x8040;
    major[8] = 0x1000;
    assert!(flush_interval(
        &mut queue,
        &mut pending,
        &OutputFormat {
            channels: 1,
            sample_rate: 48000,
            rate_field: 0x8040,
            major
        },
        &mut rate,
        &mut total,
        &mut emit
    ));
    assert!(pending.au.is_empty());
    assert!(pending.failure.is_none());
    assert!(queue.mlp_output_queue_finish(&mut emit));
    assert_eq!(total as usize, emitted.len());
}
