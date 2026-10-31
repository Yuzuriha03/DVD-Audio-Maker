//! NAV pack filling helpers (the FixVobus stage of the navigation builder).
//!
//! The PCI/DSI packets store playback-time BCD fields and relative VOBU sector
//! offsets. Both are small pure functions over the scanned VOBU table, so they
//! are modelled here without pointers and checked against the C baseline values.

#![allow(dead_code)]

use super::build::Vobu;

/// Frame rate used for cell/pgc BCD playback times (menu authoring is PAL).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameRate {
    Pal25,
    Ntsc30,
}

impl FrameRate {
    fn denominator(self) -> i64 {
        match self {
            FrameRate::Pal25 => 90_000,
            FrameRate::Ntsc30 => 90_090,
        }
    }

    fn nominal(self) -> i64 {
        match self {
            FrameRate::Pal25 => 25,
            FrameRate::Ntsc30 => 30,
        }
    }

    fn rate_code(self) -> u32 {
        match self {
            FrameRate::Pal25 => 1,
            FrameRate::Ntsc30 => 3,
        }
    }
}

fn tobcd(value: i64) -> i64 {
    (value / 10) * 16 + value % 10
}

/// BCD hhmmssff playback time with the frame-rate code, as written to PCI/DSI.
pub(crate) fn cell_elapsed(pts: i64, rate: FrameRate) -> u32 {
    let denom = rate.denominator();
    let frate = rate.nominal();
    let mut num = pts + denom / (2 * frate) + 1;
    let mut sec = num / denom;
    let min = sec / 60;
    let hr = tobcd(min / 60);
    let min = tobcd(min % 60);
    sec = tobcd(sec % 60);
    num %= denom;
    let fr = tobcd(num * frate / denom);
    ((hr << 24) | (min << 16) | (sec << 8) | fr) as u32 | (rate.rate_code() << 6)
}

/// Returns the next/previous VOBU index that carries video.
pub(crate) fn find_next_video(vobus: &[Vobu], cur: isize, forward: bool) -> Option<usize> {
    let has_video = |index: usize| vobus[index].first_video_pts.is_some();
    if forward {
        (cur as usize + 1..vobus.len()).find(|&index| has_video(index))
    } else {
        (0..cur as usize).rev().find(|&index| has_video(index))
    }
}

/// Finds the VOBU containing a presentation time within [low, high] (C findvobu).
pub(crate) fn find_vobu(vobus: &[Vobu], pts: i64, low: isize, high: isize) -> isize {
    if high < low {
        return low - 1;
    }
    let low_index = low.max(0) as usize;
    let high_index = high.max(0) as usize;
    if pts < i64::from(vobus[low_index].start_pts) {
        return low - 1;
    }
    if pts >= i64::from(vobus[high_index].end_pts) {
        return high + 1;
    }
    let (mut l, mut h) = (low, high);
    while l < h {
        let m = (l + h + 1) / 2;
        let index = m.max(0) as usize;
        if pts < i64::from(vobus[index].start_pts) {
            h = m - 1;
        } else {
            l = m;
        }
    }
    l
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
    let video_bit = if vobus[target].first_video_pts.is_some() { 0x8000_0000 } else { 0 };
    distance | video_bit | skip_bits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vobu(sector: usize, pts: Option<u32>) -> Vobu {
        Vobu { sector, first_video_pts: pts, ..Vobu::default() }
    }

    #[test]
    fn bcd_matches_the_c_baseline_values() {
        assert_eq!(cell_elapsed(0, FrameRate::Pal25), 0x40);
        assert_eq!(cell_elapsed(54_000, FrameRate::Pal25), 0x55);
        assert_eq!(cell_elapsed(108_000, FrameRate::Pal25), 0x145);
        assert_eq!(cell_elapsed(162_000, FrameRate::Pal25), 0x160);
    }

    #[test]
    fn forward_and_backward_offsets_use_the_flag_bits() {
        let vobus = [vobu(0, Some(1)), vobu(235, Some(2)), vobu(480, Some(3)), vobu(725, Some(4))];
        let same = |_: usize, _: usize| true;
        assert_eq!(get_sector(&vobus, 0, 1, false, 0x3fff_ffff, same), 235 | 0x8000_0000);
        assert_eq!(get_sector(&vobus, 1, 0, false, 0x3fff_ffff, same), 235 | 0x8000_0000);
        assert_eq!(get_sector(&vobus, 0, 4, false, 0x3fff_ffff, same), 0x3fff_ffff);
        assert_eq!(get_sector(&vobus, 3, 4, false, 0x3fff_ffff, same), 0x3fff_ffff);
    }
}
