//! NAV pack assembly (the `FixVobus` stage of the navigation builder).
//!
//! For every scanned VOBU this pass writes the full PCI/DSI packets into the
//! placeholder NAV sectors produced by the scan/write pass, using the VOBU
//! timing, reference-frame sectors, dvdauthor-data buttons and the VOBU
//! time-step tables.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::unnecessary_wraps,
    reason = "NAV sector references and VOBU indices intentionally use their protocol field widths"
)]
use super::build::Build;
use super::dvdata::DvdData;
use super::mpeg;
use super::navpack::{cell_elapsed, get_sector};
use crate::spu::format::VideoFormat;
use std::collections::BTreeMap;

const SECTOR: usize = 2048;
const HALF_SECOND: i64 = 45_000;
const TIMELINE: [i64; 19] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 20, 60, 120, 240,
];

#[derive(Clone, Debug, Default)]
pub(crate) struct VobuTiming {
    pub(crate) start: u32,
    pub(crate) end: u32,
    /// Reference frame end sectors relative to the VOBU start.
    pub(crate) references: Vec<u32>,
    /// Audio channel entries: channel -> (pts start, pts end, absolute sector).
    pub(crate) audio: BTreeMap<u8, Vec<(u32, u32, usize)>>,
    /// Subpicture channel entries: channel -> (pts start, pts end, absolute sector).
    pub(crate) subpictures: BTreeMap<u8, Vec<(u32, u32, usize)>>,
}

fn write_u32(sector: &mut [u8], at: usize, value: u32) {
    sector[at..at + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_u16(sector: &mut [u8], at: usize, value: u16) {
    sector[at..at + 2].copy_from_slice(&value.to_be_bytes());
}

/// Scans one VOBU for reference frames and subpicture packets.
fn scan_vobu(build: &Build, vobu_index: usize, format: VideoFormat) -> VobuTiming {
    let vobu = &build.vobus[vobu_index];
    let mut timing = VobuTiming {
        start: vobu.start_pts,
        end: vobu.end_pts + format.frame_duration(),
        ..VobuTiming::default()
    };
    let first = vobu.sector;
    let last = vobu.last_sector;
    let mut pending_reference = false;
    for sector_index in first..=last {
        let base = sector_index * SECTOR;
        if sector_index == first {
            continue;
        }
        let bytes = &build.output[base..base + SECTOR];
        for offset in 0..SECTOR - 4 {
            if bytes[offset] != 0 || bytes[offset + 1] != 0 || bytes[offset + 2] != 1 {
                continue;
            }
            match bytes[offset + 3] {
                0x00 => {
                    if pending_reference {
                        let previous_sector = if offset == 0 {
                            sector_index - 1
                        } else {
                            sector_index
                        };
                        if timing.references.len() < 3 {
                            timing
                                .references
                                .push((previous_sector.saturating_sub(first) as u32).min(0x3fff));
                        }
                    }
                    let ptype = (bytes[offset + 5] >> 3) & 7;
                    pending_reference = ptype == 1 || ptype == 2;
                }
                0xbd => {
                    let header_len = usize::from(bytes[offset + 8]);
                    let data = offset + 9 + header_len;
                    let Some(sid) = bytes.get(data).copied() else {
                        continue;
                    };
                    let Some(&frames) = bytes.get(data + 1) else {
                        continue;
                    };
                    let mut pts0 = 0u32;
                    if bytes[offset + 7] & 0x80 != 0 {
                        pts0 = mpeg::read_pts(&bytes[offset + 9..]);
                    }
                    match sid & 0xf8 {
                        0x80 => {
                            let pts1 = pts0.saturating_add(2880 * u32::from(frames));
                            timing.audio.entry(sid & 7).or_default().push((
                                pts0,
                                pts1,
                                sector_index,
                            ));
                        }
                        0x20 | 0x28 | 0x30 | 0x38 => {
                            let pts1 = pts0;
                            timing.subpictures.entry(sid).or_default().push((
                                pts0,
                                pts1,
                                sector_index,
                            ));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    timing
}

/// Builds the button colour lookup table from the dvdauthor-data groups.
fn find_vobu_timing(timings: &[VobuTiming], pts: i64) -> isize {
    let high = timings.len() as isize - 1;
    if high < 0 {
        return -1;
    }
    if pts < i64::from(timings[0].start) {
        return -1;
    }
    if pts >= i64::from(timings[timings.len() - 1].end) {
        return high + 1;
    }
    let (mut l, mut h) = (0isize, high);
    while l < h {
        let m = (l + h + 1) / 2;
        if pts < i64::from(timings[m.max(0) as usize].start) {
            h = m - 1;
        } else {
            l = m;
        }
    }
    l
}

fn find_vobu_by_sector(build: &Build, sector: usize) -> Option<usize> {
    build
        .vobus
        .iter()
        .take_while(|vobu| vobu.sector <= sector)
        .count()
        .checked_sub(1)
}

fn find_subpicture_index(entries: &[(u32, u32, usize)], pts: u32) -> usize {
    let mut l = 0usize;
    let mut h = entries.len().saturating_sub(1);
    while l < h {
        let m = l + (h - l).div_ceil(2);
        if pts < entries[m].0 {
            h = m - 1;
        } else {
            l = m;
        }
    }
    l
}

fn button_colours(data: &DvdData) -> [u8; 24] {
    let mut table = [0u8; 24];
    for (group_index, group) in data.groups.iter().take(3).enumerate() {
        for (index, word) in group.entries.iter().enumerate() {
            write_u16(&mut table, group_index * 8 + index * 2, *word);
        }
    }
    table
}

fn neighbour(name: Option<&str>, buttons: &[super::dvdata::Button], fallback: usize) -> u8 {
    name.and_then(|value| buttons.iter().position(|button| button.name == value))
        .map_or(fallback as u8, |found| (found + 1) as u8)
}

fn button_table(data: &DvdData, commands: &BTreeMap<String, [u8; 8]>, sector: &mut [u8]) {
    let count = data.buttons.len();
    for (index, button) in data.buttons.iter().enumerate() {
        let at = 0xbb + index * 18;
        let up = neighbour(
            button.up.as_deref(),
            &data.buttons,
            if index == 0 { count } else { index },
        );
        let down = neighbour(
            button.down.as_deref(),
            &data.buttons,
            if index + 1 == count { 1 } else { index + 2 },
        );
        let left = neighbour(
            button.left.as_deref(),
            &data.buttons,
            if index == 0 { count } else { index },
        );
        let right = neighbour(
            button.right.as_deref(),
            &data.buttons,
            if index + 1 == count { 1 } else { index + 2 },
        );
        sector[at] = (button.group << 6) | ((button.x0 >> 4) as u8 & 0x3f);
        sector[at + 1] = ((button.x0 as u8 & 0x0f) << 4) | ((button.x1 >> 8) as u8 & 0x0f);
        sector[at + 2] = button.x1 as u8;
        sector[at + 3] = (u8::from(button.auto_action) << 6) | ((button.y0 >> 4) as u8 & 0x3f);
        sector[at + 4] = ((button.y0 as u8 & 0x0f) << 4) | ((button.y1 >> 8) as u8 & 0x0f);
        sector[at + 5] = button.y1 as u8;
        sector[at + 6] = up;
        sector[at + 7] = down;
        sector[at + 8] = left;
        sector[at + 9] = right;
        let command = commands.get(&button.name).copied();
        if let Some(command) = command {
            sector[at + 10..at + 18].copy_from_slice(&command);
        }
    }
}

/// Source-local searches stop at VOB boundaries; only recorded disc addresses
/// include the sector base of this source in `AUDIO_TS.VOB`.
pub(crate) fn fill_vob(
    build: &mut Build,
    data: &DvdData,
    commands: &BTreeMap<String, [u8; 8]>,
    format: VideoFormat,
    sector_base: u32,
    vob_id: u16,
) -> Result<(), String> {
    let mut timings: Vec<VobuTiming> = (0..build.vobus.len())
        .map(|index| scan_vobu(build, index, format))
        .collect();
    let numvobus = build.vobus.len();
    let vob_start = timings.first().map_or(0, |timing| timing.start);
    let mut audio: BTreeMap<u8, Vec<(u32, u32, usize)>> = BTreeMap::new();
    let mut subpictures: BTreeMap<u8, Vec<(u32, u32, usize)>> = BTreeMap::new();
    for timing in &timings {
        for (channel, entries) in &timing.audio {
            audio
                .entry(*channel)
                .or_default()
                .extend(entries.iter().copied());
        }
        for (channel, entries) in &timing.subpictures {
            subpictures
                .entry(*channel)
                .or_default()
                .extend(entries.iter().copied());
        }
    }
    for entries in audio.values_mut().chain(subpictures.values_mut()) {
        entries.sort_by_key(|entry| entry.0);
    }
    let mut final_audio = 0u32;
    for entries in audio.values() {
        if let Some(last) = entries.last() {
            final_audio = final_audio.max(last.1);
        }
    }
    let vob_end = timings
        .last()
        .map_or(0, |timing| timing.end)
        .max(final_audio);
    if let Some(last) = timings.last_mut() {
        last.end = vob_end;
    }
    for (index, timing) in timings.iter().enumerate() {
        let vobu = build.vobus[index].clone();
        let base = vobu.sector * SECTOR;
        let mut sector = build.output[base..base + SECTOR].to_vec();
        sector[0x26..0x2a].copy_from_slice(&[0, 0, 1, 0xbf]);
        write_u16(&mut sector, 0x2a, 0x3d4);
        sector[0x2c] = 0;
        sector[0x2d..0x400].fill(0);
        sector[0x400..0x404].copy_from_slice(&[0, 0, 1, 0xbf]);
        write_u16(&mut sector, 0x404, 0x3fa);
        sector[0x406] = 1;
        sector[0x407..].fill(0);
        write_u32(&mut sector, 0x2d, sector_base + vobu.sector as u32);
        write_u32(&mut sector, 0x39, timing.start);
        write_u32(&mut sector, 0x3d, timing.end);
        if vobu.has_sequence_end {
            write_u32(&mut sector, 0x41, timing.end);
        }
        let elapsed = cell_elapsed(i64::from(timing.start - vob_start), format);
        write_u32(&mut sector, 0x45, elapsed);
        write_u16(&mut sector, 0x8d, 1);
        write_u32(&mut sector, 0x8f, vob_start);
        write_u32(&mut sector, 0x93, u32::MAX);
        write_u32(&mut sector, 0x97, u32::MAX);
        write_u16(&mut sector, 0x9b, 0x1000);
        sector[0x9e] = data.buttons.len() as u8;
        sector[0x9f] = data.buttons.len() as u8;
        sector[0xa3..0xbb].copy_from_slice(&button_colours(data));
        button_table(data, commands, &mut sector);
        let scr = mpeg::read_scr(&sector[4..9]);
        write_u32(&mut sector, 0x407, scr);
        write_u32(&mut sector, 0x40b, sector_base + vobu.sector as u32);
        for (slot, reference) in timing.references.iter().enumerate().take(3) {
            write_u32(&mut sector, 0x413 + slot * 4, *reference);
        }
        write_u16(&mut sector, 0x41f, vob_id);
        sector[0x422] = 1;
        write_u32(&mut sector, 0x423, elapsed);
        write_u32(&mut sector, 0x433, vob_start);
        write_u32(&mut sector, 0x437, vob_end);
        write_u32(&mut sector, 0x40f, (vobu.last_sector - vobu.sector) as u32);
        let same_cell = |_: usize, _: usize| true;
        let next_video =
            (index + 1..numvobus).find(|&other| build.vobus[other].first_video_pts.is_some());
        let prev_video = (0..index)
            .rev()
            .find(|&other| build.vobus[other].first_video_pts.is_some());
        for (track, entries) in audio.values().enumerate().take(8) {
            let mut found: Option<usize> = None;
            for entry in entries {
                if entry.0 <= timing.start {
                    found = Some(entry.2);
                }
            }
            let value = match found {
                Some(asect) => {
                    let delta = asect as i64 - vobu.sector as i64;
                    if delta.unsigned_abs() > 0x1fff {
                        0u16
                    } else if delta < 0 {
                        (delta.unsigned_abs() as u16) | 0x8000
                    } else {
                        delta as u16
                    }
                }
                None => 0x3fff,
            };
            write_u16(&mut sector, 0x599 + track * 2, value);
        }
        for (channel_index, entries) in subpictures.values().enumerate() {
            if entries.is_empty() {
                continue;
            }
            let id = find_subpicture_index(entries, timing.start);
            let (start, end, entry_sector) = entries[id];
            let target = if start < timing.end && end >= timing.start {
                find_vobu_by_sector(build, entry_sector)
            } else if end < timing.start
                && (id + 1 == entries.len() || entries[id + 1].0 >= timing.end)
            {
                Some(index)
            } else if id + 1 < entries.len() {
                find_vobu_by_sector(build, entries[id + 1].2)
            } else {
                None
            };
            let Some(target) = target else { continue };
            let prior = target < index;
            let encoded = get_sector(
                &build.vobus,
                index,
                target as isize,
                false,
                0x7fff_ffff,
                same_cell,
            ) & 0x7fff_ffff;
            let encoded = if encoded == 0 { 0x7fff_ffff } else { encoded };
            let encoded = if encoded != 0x7fff_ffff && prior {
                encoded | 0x8000_0000
            } else {
                encoded
            };
            write_u32(&mut sector, 0x5a9 + channel_index * 4, encoded);
        }
        write_u32(
            &mut sector,
            0x4f1,
            get_sector(
                &build.vobus,
                index,
                next_video.map_or(-1, |value| value as isize),
                false,
                0xbfff_ffff,
                same_cell,
            ),
        );
        write_u32(
            &mut sector,
            0x595,
            get_sector(
                &build.vobus,
                index,
                prev_video.map_or(-1, |value| value as isize),
                false,
                0xbfff_ffff,
                same_cell,
            ),
        );
        write_u32(
            &mut sector,
            0x541,
            get_sector(
                &build.vobus,
                index,
                index as isize + 1,
                false,
                0x3fff_ffff,
                same_cell,
            ),
        );
        write_u32(
            &mut sector,
            0x545,
            get_sector(
                &build.vobus,
                index,
                index as isize - 1,
                false,
                0x3fff_ffff,
                same_cell,
            ),
        );
        let mut forward = index;
        let mut backward = index;
        for (step, offset) in TIMELINE.iter().enumerate() {
            let time = i64::from(timing.start) + offset * HALF_SECOND;
            let next = find_vobu_timing(&timings, time);
            let previous =
                find_vobu_timing(&timings, i64::from(timing.start) - offset * HALF_SECOND);
            let skip = step >= 15;
            let next = if next < 0 { -1 } else { next };
            let previous = if previous < 0 { -1 } else { previous };
            write_u32(
                &mut sector,
                0x53d - step * 4,
                get_sector(
                    &build.vobus,
                    index,
                    next,
                    skip && next > forward as isize + 1,
                    0x3fff_ffff,
                    same_cell,
                ),
            );
            write_u32(
                &mut sector,
                0x549 + step * 4,
                get_sector(
                    &build.vobus,
                    index,
                    previous,
                    skip && previous < backward as isize - 1,
                    0x3fff_ffff,
                    same_cell,
                ),
            );
            if next >= 0 {
                forward = next as usize;
            }
            if previous >= 0 {
                backward = previous as usize;
            }
        }
        build.output[base..base + SECTOR].copy_from_slice(&sector);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::dvdata;

    fn fixture() -> Option<(Vec<u8>, Vec<u8>, DvdData)> {
        let root =
            std::path::Path::new(r"C:\Users\yyz57\Documents\Visual Studio Code\DVD-Audio-Maker");
        let input = std::fs::read(root.join("build/menu-accept-old2/menu.mpg")).ok()?;
        let baseline =
            std::fs::read(root.join("build/menu-accept-old2/导航 日本語/AUDIO_TS/AUDIO_TS.VOB"))
                .ok()?;
        let data = dvdata::parse_sector(&input[119 * 2048..120 * 2048])
            .unwrap()
            .unwrap();
        Some((input, baseline, data))
    }

    fn commands() -> BTreeMap<String, [u8; 8]> {
        let mut map = BTreeMap::new();
        map.insert(
            "button01".to_string(),
            [0x20, 0x04, 0x00, 0x00, 0x00, 0xc0, 0x00, 0x01],
        );
        map
    }

    #[test]
    fn neighbour_uses_first_named_button_or_fallback() {
        let buttons: Vec<_> = ["first", "second", "first"]
            .into_iter()
            .map(|name| dvdata::Button {
                name: name.to_owned(),
                auto_action: false,
                group: 0,
                x0: 0,
                y0: 0,
                x1: 0,
                y1: 0,
                up: None,
                down: None,
                left: None,
                right: None,
            })
            .collect();
        assert_eq!(neighbour(Some("first"), &buttons, 3), 1);
        assert_eq!(neighbour(Some("second"), &buttons, 3), 2);
        assert_eq!(neighbour(Some("missing"), &buttons, 3), 3);
        assert_eq!(neighbour(None, &buttons, 3), 3);
        assert_eq!(neighbour(Some("first"), &[], 0), 0);
    }

    #[test]
    fn filled_nav_packs_match_the_c_baseline() {
        let Some((input, baseline, data)) = fixture() else {
            return;
        };
        let mut build =
            super::super::build::build(&input, super::super::build::Aspect::Unknown).unwrap();
        for (i, v) in build.vobus.iter().enumerate() {
            eprintln!(
                "vobu {i}: sector {} last {} start {} end_raw {}",
                v.sector, v.last_sector, v.start_pts, v.end_pts
            );
        }
        fill_vob(&mut build, &data, &commands(), VideoFormat::Pal, 0, 1).unwrap();
        assert_eq!(build.output.len(), baseline.len());
        for index in 0..build.vobus.len() {
            let vobu = &build.vobus[index];
            let start = vobu.sector * SECTOR;
            let ours = &build.output[start..start + SECTOR];
            let expected = &baseline[start..start + SECTOR];
            if let Some(offset) = ours.iter().zip(expected).position(|(a, b)| a != b) {
                panic!(
                    "NAV pack {index} at sector {} differs at 0x{offset:x}: ours {:02x?} expected {:02x?}",
                    vobu.sector, ours[offset], expected[offset]
                );
            }
        }
    }
}
