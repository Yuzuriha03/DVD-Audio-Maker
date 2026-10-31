use std::borrow::Cow;

use crate::bitstream::Bits;
use crate::syntax::access_unit::serialize_planned;
#[cfg(test)]
use crate::syntax::access_unit::interval_fits;
use crate::syntax::parameters::MlpParameters;
#[cfg(test)]
use crate::syntax::parameters::MlpCodingParams;
use crate::syntax::restart::MlpRestart;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IntervalFailure {
    Fallback,
    OversizedFallback,
    Serialization,
    AccessUnitTooLarge,
    FifoRateLimit,
}

impl IntervalFailure {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Fallback => "Cannot build lossless oversized-AU fallback",
            Self::OversizedFallback => {
                "Access unit too large after lossless fallback (1536-byte limit)"
            }
            Self::Serialization => "Cannot serialize lossless access unit",
            Self::AccessUnitTooLarge => "Access unit too large (1536-byte limit)",
            Self::FifoRateLimit => {
                "MLP FIFO rate limit exceeded; input cannot be encoded at the required delivery rate"
            }
        }
    }
}

pub(crate) struct IntervalOutput {
    pub au: Vec<PlannedAu>,
    pub maximum_lsbs: u32,
    pub restart: MlpRestart,
    pub failure: Option<IntervalFailure>,
}

pub(crate) struct OutputFormat {
    pub channels: usize,
    pub sample_rate: u32,
    pub rate_field: u32,
    pub major: [u32; 14],
}

pub(crate) fn flush_interval(
    queue: &mut crate::output::queue::MlpOutputQueue,
    pending: &mut IntervalOutput,
    recycled: &mut Vec<PlannedAu>,
    format: &OutputFormat,
    rate: &mut crate::output::rate::MlpRateState,
    words_total: &mut u64,
    emit: &mut impl FnMut(&[u32]) -> bool,
) -> bool {
    if pending.au.is_empty() {
        return true;
    }
    // Serialize once: the old probe pass serialized every access unit a second
    // time only to learn its size. The restart header needs the interval LSB
    // bound before the first access unit is written.
    pending.restart.maximum_lsbs = pending.maximum_lsbs;
    let serialized = match serialize_interval(pending, format.channels) {
        Serialized::Words(words) => words,
        Serialized::Error => {
            pending.failure = Some(IntervalFailure::Serialization);
            return false;
        }
        Serialized::TooLarge => {
            if unpredicted_interval(pending, format.channels).is_err() {
                pending.failure = Some(IntervalFailure::Fallback);
                return false;
            }
            pending.restart.maximum_lsbs = pending.maximum_lsbs;
            match serialize_interval(pending, format.channels) {
                Serialized::Words(words) => words,
                Serialized::Error => {
                    pending.failure = Some(IntervalFailure::Serialization);
                    return false;
                }
                Serialized::TooLarge => {
                    pending.failure = Some(IntervalFailure::OversizedFallback);
                    return false;
                }
            }
        }
    };
    for (au, au_words) in serialized.iter().enumerate() {
        let p = &pending.au[au];
        let is_restart = au == 0;
        let length = (if is_restart { 17 } else { 3 }) + au_words.len() as u32;
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
        if crate::output::rate::mlp_rate_update(
            rate,
            p.count as u32,
            length,
            format.rate_field,
            &mut flags,
            &mut arrival,
        ) != Ok(false)
            || flags & 0x2000 != 0
        {
            pending.failure = Some(if flags & 0x2000 != 0 {
                IntervalFailure::AccessUnitTooLarge
            } else {
                IntervalFailure::FifoRateLimit
            });
            return false;
        }
        let directory =
            (if is_restart { 0x2000 } else { 0x6000 }) | au_words.len() as u32 | p.stamp << 12;
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
        words.extend_from_slice(au_words);
        if words.len() != length as usize
            || !queue.mlp_output_queue_push(&words, p.count as u32, emit)
        {
            return false;
        }
        *words_total += u64::from(length);
    }
    pending.maximum_lsbs = 0;
    recycled.clear();
    recycled.append(&mut pending.au);
    true
}

enum Serialized {
    Words(Vec<Vec<u32>>),
    Error,
    TooLarge,
}

/// Serializes every planned access unit of the interval exactly once.
fn serialize_interval(pending: &IntervalOutput, channels: usize) -> Serialized {
    let mut words = Vec::with_capacity(pending.au.len());
    for au in 0..pending.au.len() {
        let mut writer = Bits::default();
        if serialize_planned(&mut writer, pending, au, channels).is_err() {
            return Serialized::Error;
        }
        if writer.word_len() + if au == 0 { 17 } else { 3 } > 0x300 {
            return Serialized::TooLarge;
        }
        words.push(writer.words);
    }
    Serialized::Words(words)
}

pub(crate) struct EncodeOptions<'a> {
    pub format: &'a OutputFormat,
    pub restart_interval: u32,
    pub cycle: u32,
    pub predict: bool,
    pub allow_bypass: bool,
}

pub(crate) fn encode(
    input: &mut impl crate::pipeline::input::PcmInput,
    options: EncodeOptions<'_>,
    mut matrix: Option<&mut crate::pipeline::prepare::MatrixInterval>,
    pending: &mut IntervalOutput,
    queue: &mut crate::output::queue::MlpOutputQueue,
    mut stamp: Option<&mut crate::pipeline::stamp::MlpStampState<'_>>,
    emit: &mut impl FnMut(&[u32]) -> bool,
) -> Result<(), ()> {
    let EncodeOptions {
        format,
        restart_interval,
        cycle,
        predict,
        allow_bypass,
    } = options;
    use crate::pipeline::prepare::{lossless, prepare_matrix};
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
    let mut history: [crate::analysis::prediction::MlpPredictState; 6] = Default::default();
    let mut prediction_reset = [0; 6];
    let mut rate = crate::output::rate::mlp_rate_init();
    let mut boundary = Default::default();
    let mut au = 0u32;
    let mut timing = 0u32;
    let mut interval_check = 0;
    let mut noise_seed = 2;
    let mut words_total = 0;
    let mut frames = 0;
    let total_frames = input.frames();
    if let Some(m) = matrix.as_deref_mut() {
        crate::analysis::decorrelation::mlp_matrix_analysis_init(&mut m.analysis, channels as u32)?;
        if m.joint_search {
            m.search_rng = 1;
            for pool in &mut m.pool {
                crate::analysis::search::mlp_search_pool_init(pool, m.search_rng);
                m.search_rng = pool.rng;
            }
        }
    }
    let mut channel_scratch = crate::pipeline::channels::ChannelScratch::default();
    let mut recycled_au: Vec<PlannedAu> = Vec::new();
    let mut noise_left: Vec<i32> = Vec::new();
    let mut noise_right: Vec<i32> = Vec::new();
    while frames < total_frames {
        let mut span = restart_interval as usize;
        let is_restart = if cycle != 0 {
            let decision = crate::analysis::interval::mlp_interval_boundary(
                &mut boundary,
                cycle,
                restart_interval,
                input.au_samples() as u32,
            )?;
            if matrix.is_some() && decision {
                let mut ahead = boundary;
                span = 1;
                while crate::analysis::interval::mlp_interval_boundary(
                    &mut ahead,
                    cycle,
                    restart_interval,
                    input.au_samples() as u32,
                ) == Ok(false)
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
            decision
        } else {
            au.is_multiple_of(restart_interval)
        };
        if is_restart && !flush_interval(queue, pending, &mut recycled_au, format, &mut rate, &mut words_total, emit)
        {
            return Err(());
        }
        let mut p = base.clone();
        let mut initial = base.clone();
        let mut restart = MlpRestart::default();
        let mut search: [Option<&crate::analysis::search::MlpSearchPlan>; 6] = Default::default();
        if let Some(m) = matrix.as_deref_mut()
            && m.slot == m.lengths.len()
        {
            if !is_restart {
                return Err(());
            }
            prepare_matrix(input, m, span, allow_bypass, &mut 0)?;
        }
        let (pcm, count, bypass, bypass_bits) = if let Some(m) = matrix.as_deref() {
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
                    *plan = Some(&m.prediction[ch]);
                }
            }
            (
                Cow::Borrowed(&m.pcm[m.offset * channels..(m.offset + count) * channels]),
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
            (Cow::Owned(pcm), count, Vec::new(), 0)
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
        let mut planned = match recycled_au.pop() {
            Some(mut recycled) => {
                recycled.initial = initial;
                recycled.main = p;
                recycled.previous = previous;
                recycled.bypass = bypass;
                recycled.count = count;
                recycled.bypass_bits = bypass_bits;
                recycled.single_restart = is_restart && first == 0;
                recycled.end_markers = frames + count == total_frames;
                recycled.stamp = 0;
                recycled.residual.clear();
                recycled.unpredicted.clear();
                recycled
            }
            None => PlannedAu {
                initial,
                main: p,
                previous,
                residual: Vec::new(),
                unpredicted: Vec::new(),
                bypass,
                count,
                bypass_bits,
                single_restart: is_restart && first == 0,
                end_markers: frames + count == total_frames,
                stamp: 0,
            },
        };
        planned.residual.resize(planned.count * channels, 0);
        planned.unpredicted.resize(planned.count * channels, 0);
        let mut context = crate::pipeline::channels::ChannelContext {
            base: &base,
            history: &mut history,
            prediction_reset: &mut prediction_reset,
            search: &search,
            restart: is_restart,
            predict,
            channels,
        };
        crate::pipeline::channels::encode_channels(
            &mut planned,
            &pcm,
            &mut context,
            pending,
            &mut channel_scratch,
        )?;

        if pending.au.len() >= 128 {
            return Err(());
        }
        if let Some(s) = stamp.as_deref_mut() {
            planned.stamp = crate::pipeline::stamp::mlp_stamp_next(s).ok_or(())?;
            // The original refreshes after emitting the packet separator. The
            // replacement therefore starts at the next AU, never mid-packet.
            if s.start_boundary()
                && let Some(metadata) = input.refresh_stamp().map_err(|_| ())?
                && !crate::pipeline::stamp::mlp_stamp_refresh(s, metadata)
            {
                return Err(());
            }
        }
        if is_restart {
            pending.restart = restart;
        }
        previous = planned.main.clone();
        pending.au.push(planned);
        noise_left.clear();
        noise_left.resize(count, 0);
        noise_right.clear();
        noise_right.resize(count, 0);
        if crate::analysis::matrix::mlp_matrix_noise(
            &mut noise_seed,
            count,
            0,
            &mut noise_left,
            &mut noise_right,
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
        .is_some_and(|s| !crate::pipeline::stamp::mlp_stamp_complete(s))
        || !flush_interval(queue, pending, &mut recycled_au, format, &mut rate, &mut words_total, emit)
        || !queue.mlp_output_queue_finish(emit)
    {
        return Err(());
    }
    if frames != total_frames {
        return Err(());
    }
    Ok(())
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
                    crate::pipeline::channels::select(&mono[..first], qss, previous.coding[ch], true, limit);
            }
            p.main.coding[ch] = crate::pipeline::channels::select(
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
    let mut history: [crate::analysis::prediction::MlpPredictState; 6] = Default::default();
    let mut reset = [1; 6];
    let base = pending.au[0].previous.clone();
    let mut planned = pending.au.remove(0);
    crate::pipeline::channels::encode_channels(
        &mut planned,
        &(0..16).collect::<Vec<_>>(),
        &mut crate::pipeline::channels::ChannelContext {
            base: &base,
            history: &mut history,
            prediction_reset: &mut reset,
            search: &Default::default(),
            restart: true,
            predict: true,
            channels: 1,
        },
        &mut pending,
        &mut crate::pipeline::channels::ChannelScratch::default(),
    )
    .unwrap();
    pending.au.push(planned);
    assert!(interval_fits(&mut pending, 1).unwrap());
    unpredicted_interval(&mut pending, 1).unwrap();
    assert!(interval_fits(&mut pending, 1).unwrap());
    let mut writer = Bits::default();
    serialize_planned(&mut writer, &pending, 0, 1).unwrap();
    assert!(!writer.words.is_empty());
    let saved_blocksize = pending.au[0].main.blocksize;
    let saved_flags = pending.au[0].main.flags;
    for single in [false, true] {
        pending.au[0].main.blocksize = if single { 16 } else { saved_blocksize };
        pending.au[0].main.flags = if single { saved_flags | 2 } else { saved_flags };
        for end in [false, true] {
            pending.au[0].single_restart = single;
            pending.au[0].end_markers = end;
            let mut actual = Bits::default();
            let mut counter = Bits::counting();
            let actual_status = serialize_planned(&mut actual, &pending, 0, 1);
            let counted_status = serialize_planned(&mut counter, &pending, 0, 1);
            assert_eq!(actual_status, counted_status);
            if !single {
                assert!(actual_status.is_ok());
            }
            assert_eq!(actual.word_len(), counter.word_len());
        }
    }
    pending.au[0].main.blocksize = saved_blocksize;
    pending.au[0].main.flags = saved_flags;
    pending.au[0].single_restart = false;
    pending.au[0].end_markers = true;
    assert_eq!(pending.restart.maximum_lsbs, pending.maximum_lsbs);
    let mut queue = crate::output::queue::MlpOutputQueue::mlp_output_queue_init();
    let mut rate = crate::output::rate::mlp_rate_init();
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
        &mut Vec::new(),
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