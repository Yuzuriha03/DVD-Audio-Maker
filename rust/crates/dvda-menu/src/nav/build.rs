//! VOB scan/write pass (the FindVobus stage of the navigation builder).
//!
//! Reads a menu MPEG program stream, splits it into VOBUs at the NAV-pack
//! placeholder sectors written by the SPU multiplexer, drops the private
//! dvdauthor-data sector, rewrites the sequence-header aspect field the same
//! way the C scanner does and writes the VOB bytes with placeholder PCI/DSI
//! packets that the NAV-pack pass fills in later.

#![allow(dead_code)]

use super::mpeg;
use std::path::Path;

const SECTOR: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Aspect {
    Unknown,
    FourByThree,
    SixteenByNine,
}

#[derive(Clone, Debug)]
pub(crate) struct Vobu {
    /// Sector index in the output VOB.
    pub(crate) sector: usize,
    /// Sector index in the input program stream.
    pub(crate) input_sector: usize,
    /// Last output sector of the VOBU (inclusive).
    pub(crate) last_sector: usize,
    /// First video PTS seen in the VOBU, if any.
    pub(crate) first_video_pts: Option<u32>,
    pub(crate) start_pts: u32,
    pub(crate) end_pts: u32,
    pub(crate) has_sequence_end: bool,
    /// PACK and system headers saved for the NAV-pack pass.
    pub(crate) sectdata: [u8; 0x26],
}

impl Default for Vobu {
    fn default() -> Self {
        Self {
            sector: 0,
            input_sector: 0,
            last_sector: 0,
            first_video_pts: None,
            start_pts: 0,
            end_pts: 0,
            has_sequence_end: false,
            sectdata: [0; 0x26],
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Build {
    pub(crate) output: Vec<u8>,
    pub(crate) vobus: Vec<Vobu>,
    pub(crate) dvdauthor_data: Vec<Vec<u8>>,
}

fn read_u16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn is_placeholder(sector: &[u8]) -> bool {
    sector.len() >= 1028
        && sector[14..18] == [0, 0, 1, 0xbb]
        && sector[38..42] == [0, 0, 1, 0xbf]
        && sector[1024..1028] == [0, 0, 1, 0xbf]
}

fn is_dvdauthor_data(sector: &[u8]) -> bool {
    sector.len() >= 40 && sector[14..18] == [0, 0, 1, 0xbe] && &sector[20..34] == b"dvdauthor-data"
}

fn nav_placeholder(source: &[u8]) -> [u8; SECTOR] {
    let mut sector = [0u8; SECTOR];
    sector.copy_from_slice(&source[..SECTOR]);
    sector[41] = 0xbf;
    sector[42] = 0x03;
    sector[43] = 0xd4;
    sector[44] = 0x81;
    sector[45..].fill(0);
    sector[1026] = 1;
    sector[1027] = 0xbf;
    sector[1028] = 0x03;
    sector[1029] = 0xfa;
    sector[1030] = 0x81;
    sector
}

fn fix_sequence_headers(sector: &mut [u8], aspect: Aspect) {
    let value = match aspect {
        Aspect::Unknown | Aspect::SixteenByNine => 3u8,
        Aspect::FourByThree => 2,
    };
    let mut index = 0;
    while index + 8 <= sector.len() {
        if sector[index..index + 3] == [0, 0, 1] && sector[index + 3] == 0xb3 {
            sector[index + 7] = (sector[index + 7] & 0x0f) | (value << 4);
            index += 8;
        } else {
            index += 1;
        }
    }
}

/// Scans the input program stream and writes the VOB body with placeholder NAV sectors.
pub(crate) fn build(input: &[u8], aspect: Aspect) -> Result<Build, String> {
    if !input.len().is_multiple_of(SECTOR) {
        return Err("input is not a whole number of 2048-byte sectors".into());
    }
    let (sectors, _) = input.as_chunks::<SECTOR>();
    let mut build = Build::default();
    build.output.reserve(input.len() + SECTOR * 4);
    let mut vobu: Option<Vobu> = None;
    let mut back_offs = 0i64;
    let mut first_pack = true;
    for (index, source) in sectors.iter().enumerate() {
        if first_pack && source[0] == 0 && source[1] == 0 && source[2] == 1 && source[3] == 0xba {
            back_offs = i64::from(mpeg::read_scr(&source[4..9]));
            first_pack = false;
        }
        if is_placeholder(source) {
            if let Some(mut previous) = vobu.take() {
                previous.last_sector = build.output.len() / SECTOR - 1;
                build.vobus.push(previous);
            }
            let mut info = Vobu {
                sector: build.output.len() / SECTOR,
                input_sector: index,
                ..Vobu::default()
            };
            info.sectdata.copy_from_slice(&source[..0x26]);
            let nav = nav_placeholder(source);
            build.output.extend_from_slice(&nav);
            vobu = Some(info);
            continue;
        }
        if is_dvdauthor_data(source) {
            build.dvdauthor_data.push(source.to_vec());
            continue;
        }
        if vobu.is_none() {
            continue;
        }
        let mut sector = *source;
        mpeg::transpose_ts(&mut sector, -back_offs);
        fix_sequence_headers(&mut sector, aspect);
        #[allow(clippy::collapsible_if)]
        for offset in 0..SECTOR - 9 {
            if sector[offset..offset + 3] != [0, 0, 1] {
                continue;
            }
            if sector[offset + 3] == 0xb7 {
                if let Some(vobu) = vobu.as_mut() {
                    vobu.has_sequence_end = true;
                }
            }
            if sector[offset + 3] == 0xe0 && sector[offset + 7] & 0x80 != 0 {
                let pts = mpeg::read_pts(&sector[offset + 9..]);
                if let Some(vobu) = vobu.as_mut() {
                    if vobu.first_video_pts.is_none() {
                        vobu.first_video_pts = Some(pts);
                        vobu.start_pts = pts;
                    }
                    vobu.end_pts = pts;
                }
            }
        }
        build.output.extend_from_slice(&sector);
    }
    if let Some(mut previous) = vobu.take() {
        previous.last_sector = build.output.len() / SECTOR - 1;
        build.vobus.push(previous);
    }
    Ok(build)
}

/// Convenience wrapper for file-based callers.
pub(crate) fn build_file(input: &Path, aspect: Aspect) -> Result<Build, String> {
    let bytes = std::fs::read(input).map_err(|error| format!("{}: {error}", input.display()))?;
    build(&bytes, aspect)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> (std::path::PathBuf, std::path::PathBuf) {
        let root =
            std::path::Path::new(r"C:\Users\yyz57\Documents\Visual Studio Code\DVD-Audio-Maker");
        (
            root.join("build/menu-accept-old2/menu.mpg"),
            root.join("build/menu-accept-old2/导航 日本語/AUDIO_TS/AUDIO_TS.VOB"),
        )
    }

    #[test]
    fn scan_matches_the_c_vob_body() {
        let (input_path, baseline_path) = paths();
        let Ok(input) = std::fs::read(input_path) else {
            return;
        };
        let baseline = std::fs::read(baseline_path).expect("baseline VOB");
        let build = build(&input, Aspect::Unknown).unwrap();
        assert_eq!(build.output.len(), baseline.len());
        assert_eq!(build.vobus.len(), 4);
        let sectors = build.output.as_chunks::<SECTOR>().0;
        let baseline_sectors = baseline.as_chunks::<SECTOR>().0;
        let nav: Vec<usize> = build.vobus.iter().map(|vobu| vobu.sector).collect();
        assert_eq!(nav, [0, 235, 480, 725]);
        for (index, (sector, expected)) in sectors.iter().zip(baseline_sectors).enumerate() {
            if nav.contains(&index) {
                assert_eq!(&sector[..0x26], &expected[..0x26], "NAV header at {index}");
            } else {
                assert_eq!(sector, expected, "data sector {index} differs");
            }
        }
        assert_eq!(build.dvdauthor_data.len(), 1);
    }
}
