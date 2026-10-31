//! AMGM menu generation: the Rust equivalent of dvdauthor_amgm_gen.
//!
//! Parses the navigation XML, concatenates the PGC VOB sources, runs the
//! scan/write pass, fills the NAV packs and writes <dest>/AUDIO_TS/AUDIO_TS.VOB.

#![allow(dead_code)]

use super::build::{self, Aspect};
use super::dvdata;
use super::fixvobus;
use super::xml;
use std::collections::BTreeMap;
use std::path::Path;

fn aspect(group: &super::model::MenuGroup) -> Aspect {
    match group.video.aspect.as_deref() {
        Some(value) if value.eq_ignore_ascii_case("4:3") => Aspect::FourByThree,
        Some(value) if value.eq_ignore_ascii_case("16:9") => Aspect::SixteenByNine,
        _ => Aspect::Unknown,
    }
}

/// Generates the AMGM menu VOB from a navigation XML document.
pub(crate) fn run(xml_path: &Path, output: &Path) -> Result<(), String> {
    let document = xml::parse(xml_path)?;
    if document.menus.is_empty() {
        return Err("navigation document has no menus".into());
    }
    let mut commands: BTreeMap<String, [u8; 8]> = BTreeMap::new();
    for group in &document.menus {
        for pgc in &group.pgcs {
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
        }
    }
    let mut input = Vec::new();
    for group in &document.menus {
        for pgc in &group.pgcs {
            for vob in &pgc.vobs {
                let file = vob.file.as_ref().ok_or("<vob> without file")?;
                let bytes = std::fs::read(file).map_err(|error| format!("{}: {error}", file.display()))?;
                if !bytes.len().is_multiple_of(2048) {
                    return Err(format!("{}: not a whole number of sectors", file.display()));
                }
                input.extend_from_slice(&bytes);
            }
        }
    }
    if input.is_empty() {
        return Err("navigation document has no VOB sources".into());
    }
    let mut build = build::build(&input, aspect(&document.menus[0]))?;
    let data_sector = build
        .dvdauthor_data
        .first()
        .ok_or("input has no dvdauthor-data sector")?
        .clone();
    let data = dvdata::parse_sector(&data_sector)
        .ok_or("dvdauthor-data sector is not a private stream")??;
    fixvobus::fill(&mut build, &data, &commands)?;
    let target_dir = output.join("AUDIO_TS");
    std::fs::create_dir_all(&target_dir).map_err(|error| format!("{}: {error}", target_dir.display()))?;
    let target = target_dir.join("AUDIO_TS.VOB");
    std::fs::write(&target, &build.output).map_err(|error| format!("{}: {error}", target.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch() -> std::path::PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos();
        let dir = std::env::temp_dir().join(format!("dvda-amgm-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn generates_the_baseline_audio_ts_vob() {
        let root = std::path::Path::new(r"C:\Users\yyz57\Documents\Visual Studio Code\DVD-Audio-Maker");
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
        if let Some(offset) = generated.iter().zip(expected.iter()).position(|(a, b)| a != b) {
            panic!("generated VOB differs at 0x{offset:x}");
        }
    }
}
