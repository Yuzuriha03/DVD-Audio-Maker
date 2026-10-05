//! Validate the menu actually stored in an ISO, including decoded page images.
use crate::{
    formats, media,
    verify::{Job, read_iso_file},
};
use dvda_native::{
    images::Images,
    media::{Callbacks, Operation, OutputFormat, Request},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
fn read16(data: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_be_bytes(
        data.get(offset..offset + 2)
            .ok_or("Menu table is truncated")?
            .try_into()
            .unwrap(),
    ))
}
fn read32(data: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_be_bytes(
        data.get(offset..offset + 4)
            .ok_or("Menu table is truncated")?
            .try_into()
            .unwrap(),
    ))
}
fn issue(code: &str, message: impl Into<String>) -> Value {
    json!({"Severity":2,"Code":code,"Message":message.into()})
}
fn cancel(caller: &mut dyn Callbacks) -> Result<(), String> {
    if caller.cancelled() {
        Err("成品验证已取消。".into())
    } else {
        Ok(())
    }
}
pub fn ranges(amg: &[u8], vob_bytes: u64) -> Result<Vec<(u32, u32)>, String> {
    let pages = read16(amg, 0x1810)? as usize;
    if pages == 0 {
        return Err("AMG declares no menu pages".into());
    }
    let required = 0x1820 + 8 * (pages - 1) + pages * 0x13a;
    if amg.len() < required {
        return Err("AMG_TABLE_OUT_OF_RANGE".into());
    }
    let mut bases = vec![0x1810 + read32(amg, 0x181c)? as usize];
    for i in 0..pages - 1 {
        bases.push(0x1810 + read32(amg, 0x1824 + i * 8)? as usize);
    }
    if bases
        .windows(2)
        .any(|p| p[1].checked_sub(p[0]) != Some(0x132))
    {
        return Err("AMG_PGC_STRIDE_MISMATCH".into());
    }
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for (i, base) in bases.into_iter().enumerate() {
        let start = if i == 0 {
            0
        } else {
            read32(amg, base + 0x11e)?
        };
        let copy = if i == 0 {
            0
        } else {
            read32(amg, base + 0x126)?
        };
        let end = read32(amg, base + 0x12a)?;
        if copy != start {
            return Err("AMG_CELL_START_COPY_MISMATCH".into());
        }
        if end < start
            || ranges
                .last()
                .is_some_and(|p| p.1.checked_add(1) != Some(start))
        {
            return Err("AMG_CELL_CHAIN_BROKEN".into());
        }
        if pages > 1 {
            let next = read16(amg, base + 0x9c)? as usize;
            let previous = read16(amg, base + 0x9e)? as usize;
            if next != if i + 1 == pages { 0 } else { i + 2 } {
                return Err("AMG_NEXT_MENU_INVALID".into());
            }
            if i > 0 && previous != i {
                return Err("AMG_PREVIOUS_MENU_INVALID".into());
            }
        }
        ranges.push((start, end));
    }
    if u64::from(ranges.last().unwrap().1) + 1 != vob_bytes.div_ceil(2048) {
        return Err("AMG_LAST_CELL_END_MISMATCH".into());
    }
    Ok(ranges)
}
fn stills(data: &[u8], vob_bytes: u64) -> Result<u32, String> {
    if data.len() < 0x60 {
        return Err("ASVS_TOO_SHORT".into());
    }
    let records = read16(data, 12)? as usize;
    let sectors = vob_bytes.div_ceil(2048);
    if sectors > 4096 {
        return Err("ASVS_TOO_LARGE".into());
    }
    if u64::from(read32(data, 20)?) + 1 != sectors {
        return Err("ASVS_SECTOR_COUNT_MISMATCH".into());
    }
    let mut pictures = 0;
    for i in 0..records {
        let offset = 0x60 + i * 8;
        let count = *data.get(offset).ok_or("ASVS_TABLE_TRUNCATED")? as u32;
        if count == 0 {
            return Err("ASVS_EMPTY_RECORD".into());
        }
        if read16(data, offset + 2)? as u32 != pictures + 1 {
            return Err("ASVS_PICTURE_SEQUENCE_BROKEN".into());
        }
        if u64::from(read32(data, offset + 4)?) * 2048 >= vob_bytes {
            return Err("ASVS_SECTOR_OUT_OF_RANGE".into());
        }
        pictures += count;
    }
    Ok(pictures)
}
struct Workspace(PathBuf);
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Capture<'a> {
    caller: &'a mut dyn Callbacks,
    text: String,
}
impl Callbacks for Capture<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        if stream == 1 {
            self.text.push_str(text);
            self.text.push(' ');
        } else {
            self.caller.emit(stream, text);
        }
    }
    fn cancelled(&mut self) -> bool {
        self.caller.cancelled()
    }
}
fn stats(
    images: &Images,
    image: &Path,
    crop: Option<&str>,
    format: &str,
    caller: &mut dyn Callbacks,
) -> Result<Vec<f64>, String> {
    let mut args = vec!["identify".into()];
    if let Some(crop) = crop {
        args.extend(["-crop".into(), crop.into()]);
    }
    args.extend([
        "-format".into(),
        format.into(),
        image.to_string_lossy().into_owned(),
    ]);
    let mut capture = Capture {
        caller,
        text: String::new(),
    };
    let code = images.run(&args, &mut capture).map_err(|e| e.to_string())?;
    if code != 0 {
        return Err("Cannot read menu image statistics".into());
    }
    let values = capture
        .text
        .split_whitespace()
        .map(|v| v.parse::<f64>().map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    if values.is_empty() || values.iter().any(|v| !v.is_finite()) {
        return Err("Invalid menu image statistics".into());
    }
    Ok(values)
}
pub fn verify(
    iso: &Path,
    expected: Option<&Value>,
    job: &Job,
    caller: &mut dyn Callbacks,
) -> Result<Vec<Value>, String> {
    cancel(caller)?;
    let files = formats::iso_list_directory(iso, "AUDIO_TS")?;
    let vob = files
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case("AUDIO_TS.VOB"))
        .ok_or("MENU_FILE_MISSING: AUDIO_TS.VOB")?;
    let amg = read_iso_file(iso, "AUDIO_TS/AUDIO_TS.IFO")?;
    let ranges = ranges(&amg, u64::from(vob.size))?;
    let mut issues = Vec::new();
    if expected
        .and_then(|v| v["pages"].as_u64())
        .is_some_and(|pages| pages != ranges.len() as u64)
    {
        issues.push(issue(
            "MENU_PAGE_COUNT_MISMATCH",
            "AMG page count differs from the disc plan",
        ));
    }
    let still_vob = files
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case("AUDIO_SV.VOB"));
    let still_ifo = files
        .iter()
        .any(|file| file.name.eq_ignore_ascii_case("AUDIO_SV.IFO"));
    let required = expected.is_none_or(|value| value["stills"].as_u64().unwrap_or(0) > 0);
    if required && (!still_ifo || still_vob.is_none()) {
        return Err(if !still_ifo {
            "MENU_FILE_MISSING: AUDIO_SV.IFO"
        } else {
            "MENU_FILE_MISSING: AUDIO_SV.VOB"
        }
        .into());
    }
    let still_count = if let Some(vob) = still_vob {
        stills(
            &read_iso_file(iso, "AUDIO_TS/AUDIO_SV.IFO")?,
            u64::from(vob.size),
        )?
    } else {
        0
    };
    if expected
        .and_then(|v| v["stills"].as_u64())
        .is_some_and(|count| count != u64::from(still_count))
    {
        issues.push(issue(
            "ASVS_PICTURE_COUNT_MISMATCH",
            "Still-picture count differs from the disc plan",
        ));
    }
    fs::create_dir_all(&job.work_directory).map_err(|e| e.to_string())?;
    let work = Workspace(media::temporary_path(&job.work_directory.join("menu")));
    fs::create_dir(&work.0).map_err(|e| e.to_string())?;
    let vob_path = work.0.join("menu.vob");
    let mut writer = fs::File::create(&vob_path).map_err(|e| e.to_string())?;
    let mut reader = formats::iso_file_chunks(iso, "AUDIO_TS/AUDIO_TS.VOB", 128 * 1024)?;
    while let Some(data) = reader.next_chunk()? {
        cancel(caller)?;
        writer.write_all(&data).map_err(|e| e.to_string())?;
    }
    drop(writer);
    let images = Images::load(
        job.image_library
            .as_deref()
            .ok_or("Menu image component is missing")?,
    )
    .map_err(|e| e.to_string())?;
    let mut source = fs::File::open(vob_path).map_err(|e| e.to_string())?;
    let index_pages = expected
        .and_then(|v| v["index_pages"].as_u64())
        .unwrap_or(0) as usize;
    let albums = expected.and_then(|v| v["albums"].as_u64()).unwrap_or(0) as usize;
    for (page, (start, end)) in ranges.into_iter().enumerate() {
        cancel(caller)?;
        caller.emit(
            1,
            &format!("[verify] menu {} page {}", iso.display(), page + 1),
        );
        let input = work.0.join("page.vob");
        let output = work.0.join("frame.png");
        let mut file = fs::File::create(&input).map_err(|e| e.to_string())?;
        source
            .seek(SeekFrom::Start(u64::from(start) * 2048))
            .map_err(|e| e.to_string())?;
        let mut remaining = (u64::from(end) - u64::from(start) + 1) * 2048;
        let mut buffer = vec![0; 128 * 1024];
        while remaining > 0 {
            cancel(caller)?;
            let count = remaining.min(buffer.len() as u64) as usize;
            source
                .read_exact(&mut buffer[..count])
                .map_err(|e| e.to_string())?;
            file.write_all(&buffer[..count])
                .map_err(|e| e.to_string())?;
            remaining -= count as u64;
        }
        drop(file);
        let result = media::execute(
            media::Job {
                library: job.media_library.clone(),
                replace: true,
                timeout_millis: None,
                request: Request {
                    operation: Operation::VideoFrame,
                    input: input.to_string_lossy().into_owned(),
                    output: Some(output.to_string_lossy().into_owned()),
                    rate: 0,
                    bits: 0,
                    output_format: OutputFormat::None,
                    soxr: false,
                    compression: 0,
                    cover: false,
                    tags: Vec::new(),
                },
            },
            caller,
        );
        if result.exit_code != Some(0) || result.failure.is_some() {
            return Err("Cannot decode menu page".into());
        }
        let frame = stats(
            &images,
            &output,
            None,
            "%[fx:standard_deviation*255] %k",
            caller,
        )?;
        if frame.len() != 2 {
            return Err("Invalid menu frame statistics".into());
        }
        if frame[0] <= 1.0 || frame[1] <= 50.0 {
            issues.push(issue(
                "MENU_FRAME_NEAR_SOLID",
                format!("Menu page {} is nearly solid", page + 1),
            ));
        }
        if page < index_pages {
            for cell in 0..albums.saturating_sub(page * 12).min(12) {
                let x = (cell % 4) * 180;
                let y = 60 + (cell / 4) * 140;
                for (code, crop, format, kind) in [
                    (
                        "MENU_INDEX_BACKGROUND_INVALID",
                        format!("3x3+{}+{}", x + 1, y + 1),
                        "%[fx:mean*255]",
                        0,
                    ),
                    (
                        "MENU_INDEX_THUMBNAIL_MISSING",
                        format!("100x100+{}+{}", x + 40, y + 5),
                        "%[fx:mean*255]",
                        1,
                    ),
                    (
                        "MENU_INDEX_LABEL_MISSING",
                        format!("170x28+{}+{}", x + 5, y + 107),
                        "%[fx:maxima*255] %[fx:mean*255]",
                        2,
                    ),
                ] {
                    let value = stats(&images, &output, Some(&crop), format, caller)?;
                    let bad = match kind {
                        0 => value[0] > 160.0,
                        1 => value[0] <= 3.0,
                        _ => value.len() != 2 || value[0] <= 200.0 || value[1] > 200.0,
                    };
                    if bad {
                        issues.push(issue(
                            code,
                            format!("Menu page {}, cell {}", page + 1, cell + 1),
                        ));
                    }
                }
            }
        }
    }
    Ok(issues)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn still_header_rejects_every_short_self_consistent_zero_record() {
        for length in 24..0x60 {
            assert_eq!(
                stills(&vec![0; length], 2048).unwrap_err(),
                "ASVS_TOO_SHORT",
                "length {length}"
            );
        }
        assert_eq!(stills(&[0; 0x60], 2048).unwrap(), 0);
    }
    #[test]
    fn navigation_rejects_wrong_links_and_truncated_tables() {
        let mut amg = vec![0u8; 8192];
        amg[0x1810..0x1812].copy_from_slice(&2u16.to_be_bytes());
        amg[0x181c..0x1820].copy_from_slice(&0x18u32.to_be_bytes());
        amg[0x1824..0x1828].copy_from_slice(&0x14au32.to_be_bytes());
        let a = 0x1828;
        let b = a + 0x132;
        amg[a + 0x9c..a + 0x9e].copy_from_slice(&2u16.to_be_bytes());
        amg[b + 0x9e..b + 0xa0].copy_from_slice(&1u16.to_be_bytes());
        amg[a + 0x12a..a + 0x12e].copy_from_slice(&9u32.to_be_bytes());
        amg[b + 0x11e..b + 0x122].copy_from_slice(&10u32.to_be_bytes());
        amg[b + 0x126..b + 0x12a].copy_from_slice(&10u32.to_be_bytes());
        amg[b + 0x12a..b + 0x12e].copy_from_slice(&19u32.to_be_bytes());
        assert_eq!(ranges(&amg, 40960).unwrap(), vec![(0, 9), (10, 19)]);
        assert!(ranges(&amg[..6500], 40960).is_err());
        amg[a + 0x9d] = 3;
        assert_eq!(ranges(&amg, 40960).unwrap_err(), "AMG_NEXT_MENU_INVALID");
    }
}
