use crate::bitstream::{Bits, ParameterBlock};
use crate::{Config, Error, entropy, format, metadata, predict, timing};

fn restart(b: &mut Bits, channels: u8, timing: u32, check: u8) {
    b.put(3, 2);
    b.put(0x31ea, 14);
    b.put(timing & 65535, 16);
    b.put(0, 4);
    b.put(u32::from(channels - 1), 4);
    b.put(u32::from(channels - 1), 4);
    b.put(0, 4);
    b.put(0, 7);
    b.put(2, 16);
    b.put(0, 4);
    b.put(24, 5);
    b.put(33 * 25, 10);
    b.put(0, 1);
    b.put(u32::from(check), 8);
    b.put(0, 16);
    for ch in 0..channels {
        b.put(u32::from(ch), 6);
    }
    let mut crc = 0x31eau32;
    for &word in &b.words[1..] {
        let t = crc >> 9;
        crc = ((((crc & 0x1ff) << 15) ^ t) << 1) ^ word ^ t;
    }
    for _ in 0..17 {
        if crc & 0x1000000 != 0 {
            crc ^= 0x11d0000;
        }
        crc <<= 1;
    }
    if b.width != 0 {
        crc ^= b.pending << (17 - b.width);
    }
    for _ in 0..b.width {
        if crc & 0x1000000 != 0 {
            crc ^= 0x11d0000;
        }
        crc <<= 1;
    }
    b.put((crc >> 17) & 255, 8);
}
struct CodingState {
    coding: Vec<entropy::Coding>,
    filters: Vec<Vec<i32>>,
    iirs: Vec<Vec<i32>>,
}

struct ParameterState<'a> {
    coding: &'a [entropy::Coding],
    filters: &'a [Vec<i32>],
    iirs: &'a [Vec<i32>],
}

fn parameters(
    b: &mut Bits,
    qss: &[u32],
    state: ParameterState<'_>,
    previous_state: ParameterState<'_>,
    matrix: u32,
    block: ParameterBlock,
    samples: u32,
) {
    let initial = matches!(block, ParameterBlock::RestartInitial);
    let restart = !matches!(block, ParameterBlock::Continuation);
    let ParameterState {
        coding,
        filters,
        iirs,
    } = state;
    let ParameterState {
        coding: old,
        filters: old_filters,
        iirs: old_iirs,
    } = previous_state;
    b.put(0, 1);
    b.put(u32::from(!initial), 1);
    if !initial {
        b.put(samples - if restart { 8 } else { 0 }, 9);
    }
    b.put(u32::from(initial && matrix != 0), 1);
    if initial && matrix != 0 {
        b.put(matrix.count_ones(), 4);
        for target in 1..qss.len() {
            if matrix & (1 << target) == 0 {
                continue;
            }
            b.put(target as u32, 4);
            b.put(0, 4);
            b.put(0, 1);
            for ch in 0..qss.len() + 2 {
                let coefficient = u32::from(ch == 0 || ch == target);
                b.put(coefficient, 1);
                if coefficient != 0 {
                    b.put(coefficient, 2);
                }
            }
        }
    }
    b.put(0, 1);
    let changed = initial && qss.iter().any(|&q| q != 0);
    b.put(u32::from(changed), 1);
    if changed {
        for &q in qss {
            b.put(q, 4);
        }
    }
    for (ch, (&c, &previous)) in coding.iter().zip(old).enumerate() {
        let new_filter = !initial && filters[ch] != old_filters[ch];
        let new_iir = !initial && iirs[ch] != old_iirs[ch];
        b.put(u32::from(c != previous || new_filter || new_iir), 1);
        if c != previous || new_filter || new_iir {
            b.put(u32::from(new_filter), 1);
            if new_filter {
                b.put(filters[ch].len() as u32, 4);
                if !filters[ch].is_empty() {
                    b.put(8, 4);
                    b.put(12, 5);
                    b.put(0, 3);
                    for &coefficient in &filters[ch] {
                        b.put(coefficient as u32 & 4095, 12);
                    }
                    b.put(0, 1);
                }
            }
            b.put(u32::from(new_iir), 1);
            if new_iir {
                b.put(iirs[ch].len() as u32, 4);
                if !iirs[ch].is_empty() {
                    b.put(8, 4);
                    b.put(12, 5);
                    b.put(0, 3);
                    for &coefficient in &iirs[ch] {
                        b.put(coefficient as u32 & 4095, 12);
                    }
                    b.put(0, 1);
                }
            }
            b.put(u32::from(c.offset != previous.offset), 1);
            if c.offset != previous.offset {
                b.put(c.offset as u32 & 32767, 15);
            }
            b.put(c.mode, 2);
            b.put(c.width, 5);
        }
    }
}
fn transform(r: u32, last: u32) -> u32 {
    let t = r >> 8;
    let v = (((r & 255) << 3) ^ (t & 0x7ff)) << 2;
    let v = (v ^ (t & 0x1fff)) << 1;
    ((v ^ (t & 0x3fff)) << 2) ^ ((t ^ last) & 65535)
}
pub(crate) fn major(
    c: &Config,
    profile: &format::Profile,
    rate_code: u32,
    rate_field: u32,
    samples: u32,
) -> [u32; 14] {
    let mut w = [
        0xf872, 0x6fbb, 0, 0, 0xb752, 0x4000, 0, 0, 0x1105, 0, 0, 0x8080, 0, 0,
    ];
    let depth = u32::from((c.bits - 16) / 4);
    let group2 = format::FIRST[profile.assignment] < c.channels as usize;
    w[2] = depth << 12
        | (if group2 {
            u32::from((profile.group2_bits - 16) / 4)
        } else {
            15
        }) << 8
        | rate_code << 4
        | if group2 {
            rate_code - u32::from(profile.group2_rate != c.sample_rate)
        } else {
            15
        };
    w[3] = profile.assignment as u32;
    w[7] = rate_field;
    w[8] = if samples == 160 {
        0x1107
    } else if c.channels > 2 {
        0x1104
    } else {
        0x1105
    };
    w[9] = ((if rate_code & 8 != 0 { 9 } else { 10 }) + 4 * (rate_code & 3)) << 11
        | u32::from(c.bits) << 6
        | ((1 << c.channels) - 1);
    w[12] = format::MEANING[profile.assignment];
    let mut r = w[0];
    for &word in &w[1..13] {
        r = transform(transform(r, 0), word);
    }
    w[13] = r & 65535;
    w
}
pub(crate) fn stamp(packet: &[u8]) -> Vec<u32> {
    let mut out = vec![1; 9];
    let mut ones = 0;
    for &byte in packet {
        for i in (0..8).rev() {
            if ones == 8 {
                out.push(0);
                ones = 0;
            }
            let bit = u32::from((byte >> i) & 1);
            out.push(bit);
            ones = if bit == 1 { ones + 1 } else { 0 };
        }
    }
    if ones == 8 {
        out.push(0);
    }
    out.push(0);
    out
}

/// Encode signed, left-aligned 24-bit interleaved PCM, padding the last AU with zeros.
/// Cancellation is checked before each AU; no partial output is returned on failure.
pub fn encode(c: &Config, pcm: &[i32], cancelled: impl FnMut() -> bool) -> Result<Vec<u8>, Error> {
    encode_profile(
        c,
        &format::Profile::standard(c),
        &[metadata::Metadata {
            start: 0,
            packet: c.metadata.clone(),
            valid_bits: 0,
        }],
        pcm,
        cancelled,
    )
}
pub fn encode_profile(
    c: &Config,
    profile: &format::Profile,
    records: &[metadata::Metadata],
    pcm: &[i32],
    mut cancelled: impl FnMut() -> bool,
) -> Result<Vec<u8>, Error> {
    let validated = crate::config::BatchConfig::new(c, profile, pcm.len())?;
    let c = validated.config();
    let order = profile.order();
    for frame in pcm.chunks_exact(c.channels as usize) {
        for (ch, &input) in order.iter().enumerate() {
            let x = frame[input];
            let qss = 24 - profile.depth(c, ch);
            if !(-8388608..=8388607).contains(&x) || x & ((1 << qss) - 1) != 0 {
                return Err(Error::InvalidPcm);
            }
        }
    }
    let matrix = pair_matrix(c, profile, pcm);
    let samples = validated.samples_per_access_unit();
    let channels = usize::from(c.channels);
    let au_len = samples as usize * channels;
    let count = pcm.len().div_ceil(au_len);
    let stamps = metadata::timeline(records, count)?;
    let mut encoder = AuEncoder::new(c, profile, matrix);
    encoder.quantization = quantization(c, profile, pcm, matrix);
    let mut output = Vec::new();
    for (au, &bit) in stamps.iter().enumerate() {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let start = au * au_len;
        output.extend(encoder.push(
            &pcm[start..(start + au_len).min(pcm.len())],
            bit,
            au + 1 == count,
        )?);
    }
    timing::rewrite(&mut output, samples)?;
    Ok(output)
}

pub(crate) fn pair_matrix(c: &Config, profile: &format::Profile, pcm: &[i32]) -> u32 {
    let order = profile.order();
    let mut mask = 0;
    for target in 1..c.channels as usize {
        if profile.depth(c, 0) != profile.depth(c, target) {
            continue;
        }
        let mut plain = 0u64;
        let mut difference = 0u64;
        let mut valid = true;
        for frame in pcm.chunks_exact(c.channels as usize) {
            let delta = i64::from(frame[order[target]]) - i64::from(frame[order[0]]);
            if !(-8388608..=8388607).contains(&delta) {
                valid = false;
                break;
            }
            plain += u64::from(frame[order[target]].unsigned_abs());
            difference += delta.unsigned_abs();
        }
        if valid && difference.saturating_mul(2) < plain {
            mask |= 1 << target;
        }
    }
    mask
}
pub(crate) fn quantization(
    c: &Config,
    profile: &format::Profile,
    pcm: &[i32],
    matrix: u32,
) -> Vec<u32> {
    let order = profile.order();
    let mut values: Vec<_> = (0..c.channels as usize)
        .map(|ch| {
            let minimum = 24 - u32::from(profile.depth(c, ch));
            pcm.chunks_exact(c.channels as usize)
                .map(|frame| {
                    let x = if matrix & (1 << ch) != 0 {
                        frame[order[ch]] - frame[order[0]]
                    } else {
                        frame[order[ch]]
                    };
                    x.trailing_zeros().min(15)
                })
                .min()
                .unwrap_or(minimum)
                .max(minimum)
        })
        .collect();
    if matrix != 0 {
        let common = values
            .iter()
            .enumerate()
            .filter(|&(ch, _)| ch == 0 || matrix & (1 << ch) != 0)
            .map(|(_, &q)| q)
            .min()
            .unwrap();
        for (ch, q) in values.iter_mut().enumerate() {
            if ch == 0 || matrix & (1 << ch) != 0 {
                *q = common;
            }
        }
    }
    values
}
/// Stateful AU encoding; retains predictor history and one restart checksum interval.
pub(crate) struct AuEncoder {
    c: Config,
    profile: format::Profile,
    au: usize,
    previous: CodingState,
    residual_history: Vec<Vec<i32>>,
    history: Vec<Vec<i32>>,
    preceding: Vec<i32>,
    pub(crate) matrix: u32,
    pub(crate) quantization: Vec<u32>,
}
impl AuEncoder {
    pub(crate) fn new(c: &Config, profile: &format::Profile, matrix: u32) -> Self {
        let channels = c.channels as usize;
        Self {
            c: c.clone(),
            profile: profile.clone(),
            au: 0,
            previous: CodingState {
                coding: vec![entropy::Coding::default(); channels],
                filters: vec![Vec::new(); channels],
                iirs: vec![Vec::new(); channels],
            },
            residual_history: vec![Vec::new(); channels],
            history: vec![Vec::new(); channels],
            preceding: Vec::new(),
            matrix,
            quantization: (0..channels)
                .map(|ch| 24 - u32::from(profile.depth(c, ch)))
                .collect(),
        }
    }
    pub(crate) fn push(
        &mut self,
        pcm: &[i32],
        stamp_bit: u32,
        last: bool,
    ) -> Result<Vec<u8>, Error> {
        let c = &self.c;
        let profile = &self.profile;
        let order = profile.order();
        let qss = &self.quantization;
        for frame in pcm.chunks_exact(c.channels as usize) {
            for (ch, &input) in order.iter().enumerate() {
                let x = frame[input];
                let precision = 24 - u32::from(profile.depth(c, ch));
                if !(-8388608..=8388607).contains(&x) || x & ((1 << precision) - 1) != 0 {
                    return Err(Error::InvalidPcm);
                }
            }
        }
        let shift = if c.sample_rate > 96000 {
            2
        } else if c.sample_rate > 48000 {
            1
        } else {
            0
        };
        let base = if c.sample_rate.is_multiple_of(44100) {
            44100
        } else {
            48000
        };
        let samples = 40u32 << shift;
        let channels = c.channels as usize;
        let rate_code = if base == 44100 { 8 } else { 0 } | shift;
        let rate_field = 0x8000 | ((if base == 44100 { 3482 } else { 3200 }) >> shift);
        let major = major(c, profile, rate_code, rate_field, samples);
        let defaults = vec![entropy::Coding::default(); channels];
        let empty_filters = vec![Vec::new(); channels];
        let CodingState {
            coding: previous,
            filters: previous_filters,
            iirs: previous_iirs,
        } = &self.previous;
        let history = &self.history;
        let matrix = self.matrix;
        let au = self.au;
        let mut output = Vec::new();
        let is_restart = au.is_multiple_of(c.restart_interval);
        let mut channel_samples = vec![Vec::with_capacity(samples as usize); channels];
        for frame in 0..samples as usize {
            for (ch, &input_ch) in order.iter().enumerate() {
                let x = pcm.get(frame * channels + input_ch).copied().unwrap_or(0);
                let x = if matrix & (1 << ch) != 0 {
                    x - pcm.get(frame * channels + order[0]).copied().unwrap_or(0)
                } else {
                    x
                };
                channel_samples[ch].push(x >> qss[ch]);
            }
        }
        let initial: Vec<_> = channel_samples
            .iter()
            .enumerate()
            .map(|(ch, s)| entropy::select_with_context(&s[..8], qss[ch], defaults[ch], true, 31))
            .collect();
        let mut plans: Vec<_> = channel_samples
            .iter()
            .enumerate()
            .map(|(ch, s)| {
                let initial_residuals: Vec<_> = s[..8].iter().map(|&x| x << qss[ch]).collect();
                predict::select(
                    if is_restart { &s[8..] } else { s },
                    if is_restart { &s[..8] } else { &history[ch] },
                    if is_restart {
                        &initial_residuals
                    } else {
                        &self.residual_history[ch]
                    },
                    qss[ch],
                    if is_restart {
                        initial[ch]
                    } else {
                        previous[ch]
                    },
                    if is_restart {
                        &[]
                    } else {
                        &previous_filters[ch]
                    },
                    if is_restart { &[] } else { &previous_iirs[ch] },
                )
            })
            .collect();
        let main: Vec<_> = plans.iter().map(|p| p.coding).collect();
        let filters: Vec<_> = plans
            .iter_mut()
            .map(|p| std::mem::take(&mut p.filter))
            .collect();
        let iirs: Vec<_> = plans
            .iter_mut()
            .map(|p| std::mem::take(&mut p.iir))
            .collect();
        let mut b = Bits::default();
        if is_restart {
            // Restart checksum covers the preceding restart interval, not this AU.
            let mut preceding = 0;
            for frame in self.preceding.chunks_exact(channels) {
                for (ch, &input_ch) in order.iter().enumerate() {
                    let v = (frame[input_ch] as u32 & 0xffffff) << ch;
                    preceding ^= (v ^ (v >> 8) ^ (v >> 16) ^ (v >> 24)) as u8;
                }
            }
            restart(
                &mut b,
                c.channels,
                (au as u32).wrapping_mul(samples),
                preceding,
            );

            parameters(
                &mut b,
                qss,
                ParameterState {
                    coding: &initial,
                    filters: &empty_filters,
                    iirs: &empty_filters,
                },
                ParameterState {
                    coding: &defaults,
                    filters: &empty_filters,
                    iirs: &empty_filters,
                },
                matrix,
                ParameterBlock::RestartInitial,
                samples,
            );
        } else {
            b.put(2, 2);
            parameters(
                &mut b,
                qss,
                ParameterState {
                    coding: &main,
                    filters: &filters,
                    iirs: &iirs,
                },
                ParameterState {
                    coding: previous,
                    filters: previous_filters,
                    iirs: previous_iirs,
                },
                matrix,
                ParameterBlock::Continuation,
                samples,
            );
        }
        for (frame, _) in channel_samples[0].iter().enumerate() {
            if is_restart && frame == 8 {
                b.put(2, 3);
                parameters(
                    &mut b,
                    qss,
                    ParameterState {
                        coding: &main,
                        filters: &filters,
                        iirs: &iirs,
                    },
                    ParameterState {
                        coding: &initial,
                        filters: &empty_filters,
                        iirs: &empty_filters,
                    },
                    matrix,
                    ParameterBlock::RestartMain,
                    samples,
                );
            }
            for ch in 0..channels {
                let coding = if is_restart && frame < 8 {
                    initial[ch]
                } else {
                    main[ch]
                };
                let value = if is_restart && frame < 8 {
                    channel_samples[ch][frame]
                } else {
                    plans[ch].residual[frame - if is_restart { 8 } else { 0 }]
                };
                coding.put(&mut b, value, qss[ch]);
            }
        }
        self.previous = CodingState {
            coding: main,
            filters,
            iirs,
        };
        self.residual_history = plans
            .iter()
            .map(|p| p.state[p.state.len().saturating_sub(4)..].to_vec())
            .collect();
        for (ch, channel) in channel_samples.iter().enumerate() {
            self.history[ch] = channel[samples as usize - 8..].to_vec();
        }
        b.put(1, 1);
        let payload = b.seal(last);
        let words = payload.len() + if is_restart { 17 } else { 3 };
        if words > 768 {
            return Err(Error::AccessUnitTooLarge);
        }
        let directory =
            (if is_restart { 0x2000 } else { 0x6000 }) | payload.len() as u32 | stamp_bit << 12;
        let arrival = 0x8000u32.wrapping_add((au as u32).wrapping_mul(samples)) & 65535;
        let mut v = words as u32 ^ arrival ^ directory;
        let mut parity = 15;
        while v != 0 {
            parity ^= v & 15;
            v >>= 4;
        }
        let mut header = vec![parity << 12 | words as u32, arrival];
        if is_restart {
            header.extend(major);
        }
        header.push(directory);
        header.extend(payload);
        for word in header {
            output.extend((word as u16).to_be_bytes());
        }
        if is_restart {
            self.preceding.clear();
        }
        self.preceding.extend_from_slice(pcm);
        self.au += 1;
        Ok(output)
    }
}
