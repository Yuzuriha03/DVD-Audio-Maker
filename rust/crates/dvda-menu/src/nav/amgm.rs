//! AMGM menu generation: the Rust equivalent of `dvdauthor_amgm_gen`.
//!
//! Parses the navigation XML, scans each PGC VOB source independently, fills
//! its NAV packs and concatenates the results into <dest>/`AUDIO_TS/AUDIO_TS.VOB`.

use super::build::{self, Aspect};
use super::dvdata;
use super::fixvobus;
use super::xml;
use crate::spu::format::VideoFormat;
use std::collections::BTreeMap;
use std::path::Path;

fn aspect(group: &super::model::MenuGroup) -> Aspect {
    match group.video.aspect.as_deref() {
        Some(value) if value.eq_ignore_ascii_case("4:3") => Aspect::FourByThree,
        Some(value) if value.eq_ignore_ascii_case("16:9") => Aspect::SixteenByNine,
        _ => Aspect::Unknown,
    }
}

fn video_format(
    document: &super::model::Document,
    group: &super::model::MenuGroup,
) -> Result<VideoFormat, String> {
    match group.video.format.as_deref() {
        None => Ok(document.format),
        Some(value) if value.eq_ignore_ascii_case("ntsc") => Ok(VideoFormat::Ntsc),
        Some(value) if value.eq_ignore_ascii_case("pal") || value.eq_ignore_ascii_case("secam") => {
            Ok(VideoFormat::Pal)
        }
        Some(value) => Err(format!("unrecognized video format {value:?}")),
    }
}

/// Generates the AMGM menu VOB from a navigation XML document.
pub(crate) fn run(xml_path: &Path, output: &Path) -> Result<(), String> {
    let document = xml::parse(xml_path)?;
    if document.menus.is_empty() {
        return Err("navigation document has no menus".into());
    }
    let mut output_bytes = Vec::new();
    let mut vob_id = 0u16;
    for group in &document.menus {
        let format = video_format(&document, group)?;
        for pgc in &group.pgcs {
            let mut commands: BTreeMap<String, [u8; 8]> = BTreeMap::new();
            for (index, button) in pgc.buttons.iter().enumerate() {
                if button.compiled.len() != 8 {
                    return Err(format!(
                        "button command must compile to one instruction, got {} bytes",
                        button.compiled.len()
                    ));
                }
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&button.compiled);
                let name = button
                    .name
                    .clone()
                    .unwrap_or_else(|| (index + 1).to_string());
                commands.insert(name, bytes);
            }
            for vob in &pgc.vobs {
                let file = vob.file.as_ref().ok_or("<vob> without file")?;
                let bytes =
                    std::fs::read(file).map_err(|error| format!("{}: {error}", file.display()))?;
                if !bytes.len().is_multiple_of(2048) {
                    return Err(format!("{}: not a whole number of sectors", file.display()));
                }
                let mut build = build::build(&bytes, aspect(group))?;
                let data_sector = build
                    .dvdauthor_data
                    .first()
                    .ok_or("input has no dvdauthor-data sector")?;
                let mut data = dvdata::parse_sector(data_sector)
                    .ok_or("dvdauthor-data sector is not a private stream")??;
                if let Some(path) = &pgc.palette {
                    let text = std::fs::read_to_string(path)
                        .map_err(|error| format!("{}: {error}", path.display()))?;
                    let colors = text
                        .split_whitespace()
                        .map(|value| {
                            u32::from_str_radix(value, 16)
                                .map_err(|error| format!("invalid palette: {error}"))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let palette: [u32; 16] = colors
                        .try_into()
                        .map_err(|_| "palette must contain 16 colors")?;
                    super::remap::remap(&mut build.output, &mut data, palette)?;
                }
                vob_id = vob_id.checked_add(1).ok_or("too many menu VOB sources")?;
                let sector_base = u32::try_from(output_bytes.len() / 2048)
                    .map_err(|_| "menu VOB sector address overflow")?;
                fixvobus::fill_vob(&mut build, &data, &commands, format, sector_base, vob_id)?;
                output_bytes.extend_from_slice(&build.output);
            }
        }
    }
    if output_bytes.is_empty() {
        return Err("navigation document has no VOB sources".into());
    }
    let target_dir = output.join("AUDIO_TS");
    std::fs::create_dir_all(&target_dir)
        .map_err(|error| format!("{}: {error}", target_dir.display()))?;
    let target = target_dir.join("AUDIO_TS.VOB");
    std::fs::write(&target, &output_bytes)
        .map_err(|error| format!("{}: {error}", target.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!("dvda-amgm-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn menu_group_video_format_controls_dvd_scan_timing() {
        let mut document = crate::nav::model::Document::default();
        let mut group = crate::nav::model::MenuGroup::default();
        document.format = VideoFormat::Ntsc;
        assert_eq!(video_format(&document, &group).unwrap(), VideoFormat::Ntsc);
        for (name, expected) in [
            ("pal", VideoFormat::Pal),
            ("secam", VideoFormat::Pal),
            ("ntsc", VideoFormat::Ntsc),
        ] {
            group.video.format = Some(name.into());
            assert_eq!(video_format(&document, &group).unwrap(), expected);
        }
        group.video.format = Some("invalid".into());
        assert!(video_format(&document, &group).is_err());
    }

    fn synthetic_menu(x: u16, colour: u16) -> Vec<u8> {
        let mut sector = vec![0u8; 2048];
        sector[14..18].copy_from_slice(&[0, 0, 1, 0xbb]);
        sector[38..42].copy_from_slice(&[0, 0, 1, 0xbf]);
        sector[1024..1028].copy_from_slice(&[0, 0, 1, 0xbf]);
        let mut payload = b"dvdauthor-data\0\x02\x01\x00".to_vec();
        payload.extend_from_slice(&0u32.to_be_bytes());
        payload.extend_from_slice(&u32::MAX.to_be_bytes());
        payload.extend_from_slice(&[1, 0, 2, 3]);
        for group in 0..3u16 {
            for entry in 0..4u16 {
                payload.extend_from_slice(&(colour + group * 4 + entry).to_be_bytes());
            }
        }
        payload.extend_from_slice(b"\x03\x01button01\0");
        payload.extend_from_slice(&[0, 0, 0, 3]);
        for coordinate in [x, 50, x + 100, 90] {
            payload.extend_from_slice(&coordinate.to_be_bytes());
        }
        payload.extend_from_slice(&[0; 4]);
        let mut metadata = vec![0u8; 2048];
        metadata[14..18].copy_from_slice(&[0, 0, 1, 0xbe]);
        metadata[18..20].copy_from_slice(&(payload.len() as u16).to_be_bytes());
        metadata[20..20 + payload.len()].copy_from_slice(&payload);
        sector.extend_from_slice(&metadata);
        sector
    }

    #[test]
    fn generated_nav_packs_share_pal_secam_timing_and_distinguish_ntsc() {
        let dir = scratch();
        let menu = dir.join("menu.mpg");
        std::fs::write(&menu, synthetic_menu(40, 0x1110)).unwrap();
        let xml = dir.join("menus.xml");

        for (norm, elapsed, frame_duration) in [
            ("pal", 0x40_u32, 3_600),
            ("secam", 0x40, 3_600),
            ("ntsc", 0xc0, 3_003),
        ] {
            std::fs::write(
                &xml,
                format!(
                    "<dvdauthor jumppad=\"1\"><amgm><menus><video format=\"{norm}\"/><pgc><button name=\"button01\">jump menu 1;</button><vob file=\"{}\"/></pgc></menus></amgm></dvdauthor>",
                    menu.display()
                ),
            )
            .unwrap();
            run(&xml, &dir).unwrap();
            let bytes = std::fs::read(dir.join("AUDIO_TS/AUDIO_TS.VOB")).unwrap();
            assert_eq!(bytes.len(), 2048);
            assert_eq!(&bytes[0x45..0x49], &elapsed.to_be_bytes(), "{norm} PCI");
            assert_eq!(&bytes[0x423..0x427], &elapsed.to_be_bytes(), "{norm} DSI");
            let start = u32::from_be_bytes(bytes[0x39..0x3d].try_into().unwrap());
            let end = u32::from_be_bytes(bytes[0x3d..0x41].try_into().unwrap());
            assert_eq!(end - start, frame_duration, "{norm} VOBU duration");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn repeated_button_names_keep_each_pages_geometry_commands_and_colours() {
        let dir = scratch();
        let first = dir.join("first.mpg");
        let second = dir.join("second.mpg");
        std::fs::write(&first, synthetic_menu(40, 0x1110)).unwrap();
        std::fs::write(&second, synthetic_menu(520, 0x2220)).unwrap();
        let xml = dir.join("menus.xml");
        std::fs::write(&xml, format!(
            "<dvdauthor jumppad=\"1\"><amgm><menus><pgc><button name=\"button01\">jump menu 2;</button><vob file=\"{}\"/></pgc><pgc><button name=\"button01\">jump menu 1;</button><vob file=\"{}\"/></pgc></menus></amgm></dvdauthor>",
            first.display(), second.display()
        )).unwrap();
        run(&xml, &dir).unwrap();
        let bytes = std::fs::read(dir.join("AUDIO_TS").join("AUDIO_TS.VOB")).unwrap();
        assert_eq!(bytes.len(), 4096);
        for (index, (x, colour, target)) in [(40u16, 0x1110u16, 2u8), (520, 0x2220, 1)]
            .into_iter()
            .enumerate()
        {
            let nav = &bytes[index * 2048..(index + 1) * 2048];
            assert_eq!(&nav[0x2d..0x31], &(index as u32).to_be_bytes());
            assert_eq!(&nav[0x40b..0x40f], &(index as u32).to_be_bytes());
            assert_eq!(&nav[0x41f..0x421], &((index + 1) as u16).to_be_bytes());
            assert_eq!(nav[0x422], 1);
            assert_eq!((u16::from_be_bytes([nav[0xbb], nav[0xbc]]) >> 4) & 0x3ff, x);
            assert_eq!(nav[0xbb + 17], target);
            for entry in 0..12usize {
                assert_eq!(
                    &nav[0xa3 + entry * 2..0xa5 + entry * 2],
                    &(colour + entry as u16).to_be_bytes()
                );
            }
            assert_eq!(&nav[0x541..0x545], &0x3fff_ffffu32.to_be_bytes());
            assert_eq!(&nav[0x545..0x549], &0x3fff_ffffu32.to_be_bytes());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn generates_the_baseline_audio_ts_vob() {
        let root =
            std::path::Path::new(r"C:\Users\yyz57\Documents\Visual Studio Code\DVD-Audio-Maker");
        let menu = root.join("build/menu-accept-old2/menu.mpg");
        let baseline = root.join("build/menu-accept-old2/导航 日本語/AUDIO_TS/AUDIO_TS.VOB");
        let Ok(_) = std::fs::metadata(&menu) else {
            return;
        };
        let expected = std::fs::read(&baseline).unwrap();
        let xml = root.join("nav-test.xml");
        let source = format!(
            "<dvdauthor jumppad=\"1\"><amgm><menus lang=\"en\"><video format=\"pal\"/><audio format=\"ac3\" lang=\"en\"/><pgc><button name=\"button01\">jump menu 1;</button><vob file=\"{}\"/></pgc></menus></amgm></dvdauthor>",
            menu.display().to_string().replace('\\', "/")
        );
        std::fs::write(&xml, source).unwrap();
        let out = scratch();
        let result = run(&xml, &out);
        let _ = std::fs::remove_file(&xml);
        result.unwrap();
        let generated = std::fs::read(out.join("AUDIO_TS/AUDIO_TS.VOB")).unwrap();
        let _ = std::fs::remove_dir_all(&out);
        assert_eq!(generated.len(), expected.len());
        if let Some(offset) = generated
            .iter()
            .zip(expected.iter())
            .position(|(a, b)| a != b)
        {
            panic!("generated VOB differs at 0x{offset:x}");
        }
    }
}
