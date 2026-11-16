//! MLP access-unit header fields shared by the streaming pipeline.
//!
//! These helpers are the single production implementation of the major-sync
//! layout and metadata stamp expansion; the batch fixture encoder isolated
//! behind cfg(test) is the only other consumer and never ships in non-test builds.
use crate::{Config, format};
fn transform(r: u32, last: u32) -> u32 {
    let t = r >> 8;
    let v = (((r & 255) << 3) ^ (t & 0x7ff)) << 2;
    let v = (v ^ (t & 0x1fff)) << 1;
    ((v ^ (t & 0x3fff)) << 2) ^ ((t ^ last) & 65535)
}
#[allow(
    clippy::cast_possible_truncation,
    reason = "Major-sync sizes and rates are validated before serialization into fixed-width header fields"
)]
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
