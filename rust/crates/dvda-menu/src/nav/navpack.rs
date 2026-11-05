//! NAV pack filling helpers (the `FixVobus` stage of the navigation builder).
//!
//! The PCI/DSI packets store playback-time BCD fields and relative VOBU sector
//! offsets. Both are small pure functions over the scanned VOBU table, so they
//! are modelled here without pointers and checked against the C baseline values.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "NAV pack BCD and relative-sector fields use fixed protocol masks"
)]
#![allow(
    clippy::decimal_bitwise_operands,
    reason = "Test masks use the historical decimal constants"
)]
use super::build::Vobu;
use crate::spu::format::VideoFormat;

fn tobcd(value: i64) -> i64 {
    (value / 10) * 16 + value % 10
}

/// BCD hhmmssff playback time with the selected frame-rate code, as written to PCI/DSI.
pub(crate) fn cell_elapsed(pts: i64, format: VideoFormat) -> u32 {
    let (denominator, frame_rate, rate_code) = match format {
        VideoFormat::Pal => (90_000, 25, 1),
        VideoFormat::Ntsc => (90_090, 30, 3),
    };
    let mut num = pts + denominator / (2 * frame_rate) + 1;
    let mut sec = num / denominator;
    let min = sec / 60;
    let hr = tobcd(min / 60);
    let min = tobcd(min % 60);
    sec = tobcd(sec % 60);
    num %= denominator;
    let fr = tobcd(num * frame_rate / denominator);
    ((hr << 24) | (min << 16) | (sec << 8) | fr) as u32 | (rate_code << 6)
}

/// Relative VOBU sector offset with the backward/multi-vobu flag bits (C getsect).
pub(crate) fn get_sector(
    vobus: &[Vobu],
    current: usize,
    jump: isize,
    skip: bool,
    not_found: u32,
    same_cell: impl Fn(usize, usize) -> bool,
) -> u32 {
    let jump_index = if jump < 0 { None } else { Some(jump as usize) };
    let in_range = jump_index.is_some_and(|index| index < vobus.len() && same_cell(current, index));
    if !in_range {
        return not_found;
    }
    let target = jump_index.unwrap();
    let skip_bits = if skip {
        let (low, high) = if current < target {
            (current + 1, target)
        } else {
            (target + 1, current)
        };
        if (low..high).any(|index| vobus[index].first_video_pts.is_some()) {
            0x4000_0000
        } else {
            0
        }
    } else {
        0
    };
    let distance = vobus[target].sector.abs_diff(vobus[current].sector) as u32;
    let video_bit = if vobus[target].first_video_pts.is_some() {
        0x8000_0000
    } else {
        0
    };
    distance | video_bit | skip_bits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vobu(sector: usize, pts: Option<u32>) -> Vobu {
        Vobu {
            sector,
            first_video_pts: pts,
            ..Vobu::default()
        }
    }

    #[test]
    fn bcd_matches_the_c_baseline_values() {
        assert_eq!(cell_elapsed(0, VideoFormat::Pal), 0x40);
        assert_eq!(cell_elapsed(54_000, VideoFormat::Pal), 0x55);
        assert_eq!(cell_elapsed(108_000, VideoFormat::Pal), 0x145);
        assert_eq!(cell_elapsed(162_000, VideoFormat::Pal), 0x160);
        assert_eq!(cell_elapsed(0, VideoFormat::Ntsc), 0xc0);
        assert_eq!(cell_elapsed(90_090, VideoFormat::Ntsc), 0x1c0);
    }

    #[test]
    fn forward_and_backward_offsets_use_the_flag_bits() {
        let vobus = [
            vobu(0, Some(1)),
            vobu(235, Some(2)),
            vobu(480, Some(3)),
            vobu(725, Some(4)),
        ];
        let same = |_: usize, _: usize| true;
        assert_eq!(
            get_sector(&vobus, 0, 1, false, 0x3fff_ffff, same),
            235 | 0x8000_0000
        );
        assert_eq!(
            get_sector(&vobus, 1, 0, false, 0x3fff_ffff, same),
            235 | 0x8000_0000
        );
        assert_eq!(
            get_sector(&vobus, 0, 4, false, 0x3fff_ffff, same),
            0x3fff_ffff
        );
        assert_eq!(
            get_sector(&vobus, 3, 4, false, 0x3fff_ffff, same),
            0x3fff_ffff
        );
    }
}
