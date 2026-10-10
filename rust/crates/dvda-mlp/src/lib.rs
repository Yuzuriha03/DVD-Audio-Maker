//! Pure Rust MLP encoder with adaptive entropy, prediction and decorrelation.
//! Production links this crate directly; its DLL is a development ABI adapter.

#[cfg(target_arch = "x86_64")]
mod c_analysis;
#[cfg(target_arch = "x86_64")]
mod c_channels;
#[cfg(target_arch = "x86_64")]
mod c_encode;
#[cfg(target_arch = "x86_64")]
mod c_entropy;
#[cfg(target_arch = "x86_64")]
mod c_host;
#[cfg(target_arch = "x86_64")]
mod c_output_queue;
#[cfg(target_arch = "x86_64")]
mod c_predict;
#[cfg(target_arch = "x86_64")]
mod c_prepare;
#[cfg(target_arch = "x86_64")]
mod c_rate;
#[cfg(target_arch = "x86_64")]
mod c_restart;
#[cfg(target_arch = "x86_64")]
mod c_stamp;
#[cfg(target_arch = "x86_64")]
mod c_substream;
mod coefficients;
pub mod direct;
mod entropy;
#[cfg(target_arch = "x86_64")]
mod extended;
pub mod ffi;
pub mod format;
#[cfg(all(test, windows, target_arch = "x86_64", feature = "external-oracle"))]
mod frozen_reference;
#[cfg(target_arch = "x86_64")]
mod interval;
#[cfg(target_arch = "x86_64")]
mod matrix;
pub mod metadata;
#[cfg(target_arch = "x86_64")]
mod parameters;
mod predict;
#[cfg(target_arch = "x86_64")]
mod scale;
#[cfg(target_arch = "x86_64")]
mod search;
mod streaming;
mod timing;

#[derive(Clone, Debug)]
pub struct Config {
    pub sample_rate: u32,
    pub bits: u8,
    pub channels: u8,
    pub restart_interval: usize,
    /// Explicit framed metadata packet (length BE16, 0x40, 0, payload).
    pub metadata: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidConfig,
    InvalidMetadata,
    InvalidPcm,
    AccessUnitTooLarge,
    FifoOverflow,
    Cancelled,
}

#[derive(Default)]
struct Bits {
    words: Vec<u32>,
    pending: u32,
    width: u32,
}
impl Bits {
    fn put(&mut self, value: u32, width: u32) {
        assert!(width <= 32 && (width == 32 || value >> width == 0));
        for i in (0..width).rev() {
            self.pending = (self.pending << 1) | ((value >> i) & 1);
            self.width += 1;
            if self.width == 16 {
                self.words.push(self.pending);
                self.pending = 0;
                self.width = 0;
            }
        }
    }
    fn seal(mut self, last: bool) -> Vec<u32> {
        if self.width != 0 {
            self.words.push(self.pending << (16 - self.width));
        }
        if last {
            self.words.extend([0xd234, 0xd234]);
        }
        self.words.push(checksum(&self.words));
        self.words
    }
}
fn checksum(words: &[u32]) -> u32 {
    let (mut x, mut c) = (0xa9u32, 0xa2u32);
    for &w in words {
        let t = ((c as i32) >> 7) as u32;
        x ^= w;
        c = ((((c & 0x7f) << 12) ^ t) << 4) ^ t ^ w;
    }
    for _ in 0..15 {
        if c & 0x400000 != 0 {
            c ^= 0x58c000;
        }
        c <<= 1;
    }
    ((x & 255) << 8) ^ (x & 0xff00) | ((c as i32 >> 15) as u32)
}
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
fn parameters(
    b: &mut Bits,
    qss: &[u32],
    state: (&[entropy::Coding], &[Vec<i32>], &[Vec<i32>]),
    previous_state: (&[entropy::Coding], &[Vec<i32>], &[Vec<i32>]),
    matrix: u32,
    flags: (bool, bool),
    samples: u32,
) {
    let (initial, restart) = flags;
    let (coding, filters, iirs) = state;
    let (old, old_filters, old_iirs) = previous_state;
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
fn major(
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
fn stamp(packet: &[u8]) -> Vec<u32> {
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
    let mut normalized;
    let c = if c.restart_interval == 0 {
        normalized = c.clone();
        normalized.restart_interval = 8;
        &normalized
    } else {
        c
    };
    if !profile.valid(c) {
        return Err(Error::InvalidConfig);
    }
    if !(1..=6).contains(&c.channels)
        || ![16, 20, 24].contains(&c.bits)
        || !(1..=32).contains(&c.restart_interval)
        || pcm.is_empty()
        || !pcm.len().is_multiple_of(usize::from(c.channels))
    {
        return Err(Error::InvalidConfig);
    }
    let (_base, shift) = match c.sample_rate {
        44100 => (44100, 0),
        48000 => (48000, 0),
        88200 => (44100, 1),
        96000 => (48000, 1),
        176400 => (44100, 2),
        192000 => (48000, 2),
        _ => return Err(Error::InvalidConfig),
    };
    if shift == 2 && c.channels > 2 {
        return Err(Error::InvalidConfig);
    }
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
    let samples = 40u32 << shift;
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

fn pair_matrix(c: &Config, profile: &format::Profile, pcm: &[i32]) -> u32 {
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
fn quantization(c: &Config, profile: &format::Profile, pcm: &[i32], matrix: u32) -> Vec<u32> {
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
struct AuEncoder {
    c: Config,
    profile: format::Profile,
    au: usize,
    previous: Vec<entropy::Coding>,
    previous_filters: Vec<Vec<i32>>,
    previous_iirs: Vec<Vec<i32>>,
    residual_history: Vec<Vec<i32>>,
    history: Vec<Vec<i32>>,
    preceding: Vec<i32>,
    matrix: u32,
    quantization: Vec<u32>,
}
impl AuEncoder {
    fn new(c: &Config, profile: &format::Profile, matrix: u32) -> Self {
        let channels = c.channels as usize;
        Self {
            c: c.clone(),
            profile: profile.clone(),
            au: 0,
            previous: vec![entropy::Coding::default(); channels],
            previous_filters: vec![Vec::new(); channels],
            previous_iirs: vec![Vec::new(); channels],
            residual_history: vec![Vec::new(); channels],
            history: vec![Vec::new(); channels],
            preceding: Vec::new(),
            matrix,
            quantization: (0..channels)
                .map(|ch| 24 - u32::from(profile.depth(c, ch)))
                .collect(),
        }
    }
    fn push(&mut self, pcm: &[i32], stamp_bit: u32, last: bool) -> Result<Vec<u8>, Error> {
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
        let previous = &self.previous;
        let previous_filters = &self.previous_filters;
        let history = &self.history;
        let previous_iirs = &self.previous_iirs;
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
        let plans: Vec<_> = channel_samples
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
        let filters: Vec<_> = plans.iter().map(|p| p.filter.clone()).collect();
        let iirs: Vec<_> = plans.iter().map(|p| p.iir.clone()).collect();
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
                (&initial, &empty_filters, &empty_filters),
                (&defaults, &empty_filters, &empty_filters),
                matrix,
                (true, true),
                samples,
            );
        } else {
            b.put(2, 2);
            parameters(
                &mut b,
                qss,
                (&main, &filters, &iirs),
                (previous, previous_filters, previous_iirs),
                matrix,
                (false, false),
                samples,
            );
        }
        for (frame, _) in channel_samples[0].iter().enumerate() {
            if is_restart && frame == 8 {
                b.put(2, 3);
                parameters(
                    &mut b,
                    qss,
                    (&main, &filters, &iirs),
                    (&initial, &empty_filters, &empty_filters),
                    matrix,
                    (false, true),
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
        self.previous = main;
        self.previous_filters = filters;
        self.previous_iirs = iirs;
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
#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config {
            sample_rate: 48000,
            bits: 16,
            channels: 2,
            restart_interval: 8,
            metadata: vec![0, 0, 0x40, 0],
        }
    }
    #[test]
    fn silence_has_major_sync_and_padding() {
        let bytes = encode(&config(), &[0; 82], || false).unwrap();
        assert_eq!(&bytes[4..8], &[0xf8, 0x72, 0x6f, 0xbb]);
        assert!(bytes.len() > 32);
    }
    #[test]
    fn restart_interval_matches_native_admission() {
        let mut c = config();
        let expected = encode(&c, &[0; 82], || false).unwrap();
        c.restart_interval = 0;
        assert_eq!(encode(&c, &[0; 82], || false).unwrap(), expected);
        c.restart_interval = 33;
        assert_eq!(encode(&c, &[0; 82], || false), Err(Error::InvalidConfig));
    }
    #[test]
    fn cancellation_discards_output() {
        let mut calls = 0;
        assert_eq!(
            encode(&config(), &[0; 160], || {
                calls += 1;
                calls == 2
            }),
            Err(Error::Cancelled)
        );
    }
    #[test]
    fn rejects_invalid_precision() {
        assert_eq!(encode(&config(), &[1, 0], || false), Err(Error::InvalidPcm));
    }
    #[test]
    fn bit_writer_and_checksum() {
        let mut b = Bits::default();
        b.put(0x1234, 16);
        b.put(0x56, 8);
        assert_eq!(&b.seal(false)[..2], &[0x1234, 0x5600]);
        assert_eq!(checksum(&[]), 0xa9a2);
    }
    #[test]
    fn scaling_matrix_and_prediction_compress_correlated_pcm() {
        let c = Config {
            sample_rate: 48000,
            bits: 24,
            channels: 2,
            restart_interval: 8,
            metadata: vec![0, 0, 64, 0],
        };
        let profile = format::Profile::standard(&c);
        let pcm: Vec<_> = (0..4000)
            .flat_map(|n| {
                let x = (n % 1000 - 500) * 256;
                [x, x + 256]
            })
            .collect();
        assert_eq!(pair_matrix(&c, &profile, &pcm), 2);
        let qss = quantization(&c, &profile, &pcm, 2);
        assert_eq!(qss, [8, 8]);
        let bytes = encode(&c, &pcm, || false).unwrap();
        assert!(
            bytes.len() < pcm.len() * 3 / 4,
            "prediction/scaling/matrix must beat raw24bit"
        );
    }
}
