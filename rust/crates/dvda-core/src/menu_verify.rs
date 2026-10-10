//! Validate the menu actually stored in an ISO, including decoded page images.
use crate::formats::IsoEntryInfo;
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
    let pointer = read32(amg, 0xcc)? as usize;
    let directory = if pointer == 0 { 0x1800 } else { pointer * 2048 };
    let pages = read16(amg, directory + 0x10)? as usize;
    if pages == 0 {
        return Err("AMG declares no menu pages".into());
    }
    // Each page contributes an eight-byte directory entry and a 306-byte PGC.
    // The 314-byte stride already includes the directory entry.
    let required = directory + 24 + pages * 314;
    if amg.len() < required {
        return Err("AMG_TABLE_OUT_OF_RANGE".into());
    }
    let mut bases = vec![directory + 0x10 + read32(amg, directory + 0x1c)? as usize];
    for i in 0..pages - 1 {
        bases.push(directory + 0x10 + read32(amg, directory + 0x24 + i * 8)? as usize);
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

fn extent_lba(entry: &IsoEntryInfo, iso_sectors: u64) -> Result<u64, String> {
    if entry.is_directory || entry.is_multi_extent {
        return Err(format!("AMG_ISO_EXTENT_INVALID: {}", entry.name));
    }
    let lba = u64::from(entry.logical_block_address)
        .checked_add(u64::from(entry.extended_attribute_blocks))
        .ok_or("AMG_ISO_EXTENT_OUT_OF_RANGE")?;
    let sectors = u64::from(entry.size).div_ceil(2048);
    if lba >= iso_sectors || lba.checked_add(sectors).is_none_or(|end| end > iso_sectors) {
        return Err(format!("AMG_ISO_EXTENT_OUT_OF_RANGE: {}", entry.name));
    }
    Ok(lba)
}

fn entry<'a>(files: &'a [IsoEntryInfo], name: &str) -> Option<&'a IsoEntryInfo> {
    files
        .iter()
        .find(|file| file.name.eq_ignore_ascii_case(name))
}

fn verify_navigation_pointers(
    amg: &[u8],
    audio_files: &[IsoEntryInfo],
    video_files: &[IsoEntryInfo],
    iso_sectors: u64,
) -> Result<(), String> {
    let amg_file = entry(audio_files, "AUDIO_TS.IFO").ok_or("MENU_FILE_MISSING: AUDIO_TS.IFO")?;
    let amg_lba = extent_lba(amg_file, iso_sectors)?;

    let amgm_declared = amg_lba
        .checked_add(u64::from(read32(amg, 0xc0)?))
        .ok_or("AMG_AMGM_POINTER_OUT_OF_RANGE")?;
    if amgm_declared >= iso_sectors {
        return Err(format!(
            "AMG_AMGM_POINTER_OUT_OF_RANGE: declared LBA {amgm_declared}, ISO has {iso_sectors} sectors"
        ));
    }
    let amgm = entry(audio_files, "AUDIO_TS.VOB").ok_or("AMG_AMGM_TARGET_MISSING")?;
    let amgm_lba = extent_lba(amgm, iso_sectors)?;
    if amgm_declared != amgm_lba {
        return Err(format!(
            "AMG_AMGM_POINTER_MISMATCH: AUDIO_TS.VOB is at LBA {amgm_lba}, pointer resolves to {amgm_declared}"
        ));
    }

    if !amg.len().is_multiple_of(2048) {
        return Err("AMG_BACKUP_POINTER_INVALID: AUDIO_TS.IFO is not sector aligned".into());
    }
    let backup_end = u64::from(read32(amg, 12)?).saturating_add(1);
    let amg_sectors = u64::try_from(amg.len() / 2048).map_err(|_| "AMG_BACKUP_POINTER_INVALID")?;
    let backup_pointer = backup_end
        .checked_sub(amg_sectors)
        .ok_or("AMG_BACKUP_POINTER_INVALID: backup address precedes AUDIO_TS.IFO")?;
    let backup_declared = amg_lba
        .checked_add(backup_pointer)
        .ok_or("AMG_BACKUP_POINTER_OUT_OF_RANGE")?;
    if backup_declared >= iso_sectors {
        return Err(format!(
            "AMG_BACKUP_POINTER_OUT_OF_RANGE: declared LBA {backup_declared}, ISO has {iso_sectors} sectors"
        ));
    }
    let backup = entry(audio_files, "AUDIO_TS.BUP").ok_or("AMG_BACKUP_TARGET_MISSING")?;
    let backup_lba = extent_lba(backup, iso_sectors)?;
    if backup_declared != backup_lba {
        return Err(format!(
            "AMG_BACKUP_POINTER_MISMATCH: AUDIO_TS.BUP is at LBA {backup_lba}, pointer resolves to {backup_declared}"
        ));
    }

    let asvs_pointer = u64::from(read32(amg, 0x30)?);
    match entry(audio_files, "AUDIO_SV.IFO") {
        Some(asvs) => {
            let expected = extent_lba(asvs, iso_sectors)?;
            let declared = amg_lba
                .checked_add(asvs_pointer)
                .ok_or("AMG_ASVS_POINTER_OUT_OF_RANGE")?;
            if declared >= iso_sectors {
                return Err(format!(
                    "AMG_ASVS_POINTER_OUT_OF_RANGE: declared LBA {declared}, ISO has {iso_sectors} sectors"
                ));
            }
            if declared != expected {
                return Err(format!(
                    "AMG_ASVS_POINTER_MISMATCH: AUDIO_SV.IFO is at LBA {expected}, pointer resolves to {declared}"
                ));
            }
        }
        None if asvs_pointer != 0 => {
            let declared = amg_lba
                .checked_add(asvs_pointer)
                .ok_or("AMG_ASVS_POINTER_OUT_OF_RANGE")?;
            if declared >= iso_sectors {
                return Err(format!(
                    "AMG_ASVS_POINTER_OUT_OF_RANGE: declared LBA {declared}, ISO has {iso_sectors} sectors"
                ));
            }
            return Err(format!(
                "AMG_ASVS_TARGET_MISSING: pointer resolves to LBA {declared}, AUDIO_SV.IFO is absent"
            ));
        }
        None => {}
    }

    // The two title tables are copies in separate sectors. Validate every
    // declared record in both so neither stale copy can hide a bad pointer.
    let mut declared_count = None;
    let pointers = [read32(amg, 0xc4)? as usize, read32(amg, 0xc8)? as usize];
    let legacy = pointers == [0, 0];
    let starts = if legacy {
        [0x800, 0x1000]
    } else {
        [pointers[0] * 2048, pointers[1] * 2048]
    };
    if !legacy && (starts[0] < 2048 || starts[1] <= starts[0]) {
        return Err("AMG_TITLE_TABLE_OUT_OF_RANGE".into());
    }
    for table_start in starts {
        let count = read16(amg, table_start)? as usize;
        let last_byte = read16(amg, table_start + 2)? as usize;
        let table_bytes = 4usize
            .checked_add(
                count
                    .checked_mul(14)
                    .ok_or("AMG_TITLE_TABLE_OUT_OF_RANGE")?,
            )
            .ok_or("AMG_TITLE_TABLE_OUT_OF_RANGE")?;
        let available = if legacy { 2048 } else { starts[1] - starts[0] };
        if table_bytes > available || last_byte.checked_add(1) != Some(table_bytes) {
            return Err("AMG_TITLE_TABLE_OUT_OF_RANGE".into());
        }
        if declared_count
            .replace(count)
            .is_some_and(|previous| previous != count)
        {
            return Err("AMG_TITLE_TABLE_COUNT_MISMATCH".into());
        }
        if table_start
            .checked_add(table_bytes)
            .is_none_or(|end| end > amg.len())
        {
            return Err("AMG_TITLE_TABLE_OUT_OF_RANGE".into());
        }
        for index in 0..count {
            let record = table_start + 4 + index * 14;
            let flags = amg[record];
            let titleset = amg[record + 8];
            let pointer = u64::from(read32(amg, record + 10)?);
            let declared = amg_lba
                .checked_add(pointer)
                .ok_or("AMG_ATSI_POINTER_OUT_OF_RANGE")?;
            if declared >= iso_sectors {
                return Err(format!(
                    "AMG_ATSI_POINTER_OUT_OF_RANGE: title {} resolves to LBA {declared}, ISO has {iso_sectors} sectors",
                    index + 1
                ));
            }

            if !(1..=99).contains(&titleset) {
                return Err(format!(
                    "AMG_ATSI_TITLESET_INVALID: title {} has titleset {titleset}",
                    index + 1
                ));
            }
            let video_link = flags & 0xc0 == 0x40;
            let name = if video_link {
                format!("VTS_{titleset:02}_0.IFO")
            } else {
                format!("ATS_{titleset:02}_0.IFO")
            };
            let directory = if video_link { video_files } else { audio_files };
            let prefix = if video_link { "AMG_VTSI" } else { "AMG_ATSI" };
            let atsi = entry(directory, &name).ok_or_else(|| {
                format!(
                    "{prefix}_TARGET_MISSING: title {} expects {name}",
                    index + 1
                )
            })?;
            let expected = extent_lba(atsi, iso_sectors)?;
            if declared != expected {
                return Err(format!(
                    "{prefix}_POINTER_MISMATCH: title {} expects {name} at LBA {expected}, pointer resolves to {declared}",
                    index + 1
                ));
            }
        }
    }
    Ok(())
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

    fn progress(&mut self, completed: u64, total: u64) {
        self.caller.progress(completed, total);
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

pub fn verify_iso_navigation(iso: &Path) -> Result<(), String> {
    let audio_files = formats::iso_list_directory(iso, "AUDIO_TS")?;
    let video_files = formats::iso_list_directory(iso, "VIDEO_TS").unwrap_or_default();
    let iso_sectors = fs::metadata(iso).map_err(|error| error.to_string())?.len() / 2048;
    let amg = read_iso_file(iso, "AUDIO_TS/AUDIO_TS.IFO")?;
    verify_navigation_pointers(&amg, &audio_files, &video_files, iso_sectors)
}

pub fn verify(
    iso: &Path,
    expected: Option<&Value>,
    job: &Job,
    caller: &mut dyn Callbacks,
) -> Result<Vec<Value>, String> {
    cancel(caller)?;
    formats::verify_dvd_audio_filesystem(iso)?;
    verify_iso_navigation(iso)?;
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
    let images = crate::native_components::images(job.image_library.as_deref())
        .map_err(|e| e.to_string())?;
    let mut source = fs::File::open(vob_path).map_err(|e| e.to_string())?;
    let index_pages = expected
        .and_then(|v| v["index_pages"].as_u64())
        .unwrap_or(0) as usize;
    let albums = expected.and_then(|v| v["albums"].as_u64()).unwrap_or(0) as usize;
    let page_total = ranges.len();
    for (page, (start, end)) in ranges.into_iter().enumerate() {
        cancel(caller)?;
        caller.progress(page as u64, page_total as u64);
        crate::task_log::emit(
            caller,
            "verify_menu_page",
            (page + 1) as u64,
            page_total as u64,
            iso.file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default(),
            &iso.to_string_lossy(),
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
            "%[fx:standard_deviation*255] %k %h",
            caller,
        )?;
        if frame.len() != 3 || !matches!(frame[2] as usize, 480 | 576) {
            return Err("Invalid menu frame statistics".into());
        }
        if frame[0] <= 1.0 || frame[1] <= 50.0 {
            issues.push(issue(
                "MENU_FRAME_NEAR_SOLID",
                format!("Menu page {} is nearly solid", page + 1),
            ));
        }
        if page < index_pages {
            let height = frame[2] as usize;
            let scaled = |value: usize| (value * height / 576) & !1;
            let thumb = scaled(100).max(50);
            for cell in 0..albums.saturating_sub(page * 12).min(12) {
                let x = (cell % 4) * 180;
                let y = scaled(60) + (cell / 4) * scaled(140);
                for (code, crop, format, kind) in [
                    (
                        "MENU_INDEX_BACKGROUND_INVALID",
                        format!("3x3+{}+{}", x + 1, y + 1),
                        "%[fx:mean*255]",
                        0,
                    ),
                    (
                        "MENU_INDEX_THUMBNAIL_MISSING",
                        format!("{thumb}x{thumb}+{}+{}", x + (180 - thumb) / 2, y + 5),
                        "%[fx:mean*255]",
                        1,
                    ),
                    (
                        "MENU_INDEX_LABEL_MISSING",
                        format!("170x{}+{}+{}", scaled(28).max(20), x + 5, y + 5 + thumb + 2),
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
    caller.progress(page_total as u64, page_total as u64);
    Ok(issues)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, lba: u32, size: u32, extended_attribute_blocks: u8) -> IsoEntryInfo {
        IsoEntryInfo {
            name: name.into(),
            is_directory: false,
            logical_block_address: lba,
            size,
            extended_attribute_blocks,
            is_multi_extent: false,
        }
    }

    fn navigation_fixture() -> (Vec<u8>, Vec<IsoEntryInfo>) {
        let mut amg = vec![0u8; 3 * 2048];
        amg[12..16].copy_from_slice(&37u32.to_be_bytes());
        amg[0x30..0x34].copy_from_slice(&42u32.to_be_bytes());
        amg[0xc0..0xc4].copy_from_slice(&30u32.to_be_bytes());
        let records = [(1u8, 80u32), (1, 80), (2, 140)];
        for table_start in [0x800usize, 0x1000] {
            amg[table_start..table_start + 2]
                .copy_from_slice(&(records.len() as u16).to_be_bytes());
            let last_byte = 4 + 14 * records.len() - 1;
            amg[table_start + 2..table_start + 4]
                .copy_from_slice(&(last_byte as u16).to_be_bytes());
            for (index, (titleset, pointer)) in records.iter().copied().enumerate() {
                let record = table_start + 4 + index * 14;
                amg[record] = 0x81;
                amg[record + 8] = titleset;
                amg[record + 10..record + 14].copy_from_slice(&pointer.to_be_bytes());
            }
        }
        let files = vec![
            file("AUDIO_TS.IFO", 19, 3 * 2048, 1),
            file("AUDIO_TS.VOB", 50, 2 * 2048, 0),
            file("AUDIO_TS.BUP", 55, 3 * 2048, 0),
            file("AUDIO_SV.IFO", 61, 2 * 2048, 1),
            file("ATS_01_0.IFO", 99, 2 * 2048, 1),
            file("ATS_02_0.IFO", 159, 2 * 2048, 1),
        ];
        (amg, files)
    }

    #[test]
    fn navigation_pointers_resolve_through_iso_extents_for_both_title_tables() {
        let (amg, files) = navigation_fixture();
        verify_navigation_pointers(&amg, &files, &[], 300).unwrap();
    }

    fn authored_manager(title_counts: &[usize], pages: usize) -> Vec<u8> {
        use dvda_author::{amg, atsi, samg};
        let groups = title_counts
            .iter()
            .map(|&count| amg::Group {
                titles: (0..count)
                    .map(|track| atsi::Title {
                        tracks: vec![
                            samg::Track {
                                mlp: false,
                                channels: 2,
                                bits: 16,
                                rate: 48000,
                                channel_assignment: 1,
                                first_pts: 0,
                                pts_length: 9000,
                                first_sector: track as u32,
                                last_sector: track as u32,
                            }
                            .into(),
                        ],
                    })
                    .collect(),
                atsi_sectors: 2,
            })
            .collect::<Vec<_>>();
        amg::encode(
            &groups,
            &amg::Layout {
                top_vob_sectors: pages as u32 * 2,
                ..Default::default()
            },
            &amg::Options {
                menu: Some(amg::Menu {
                    page_sectors: vec![3; pages],
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap()
        .bytes
    }

    #[test]
    fn authored_pgci_accepts_sector_boundary_and_maximum_page_tables() {
        for pages in [1, 26, 255] {
            let amg = authored_manager(&[1], pages);
            let actual = ranges(&amg, pages as u64 * 2 * 2048).unwrap();
            assert_eq!(
                actual,
                (0..pages)
                    .map(|page| (page as u32 * 2, page as u32 * 2 + 1))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn authored_multi_sector_title_tables_resolve_every_title() {
        for counts in [vec![99, 48], vec![99; 9]] {
            let amg = authored_manager(&counts, 26);
            let amg_lba = 20;
            let mut files = vec![
                file("AUDIO_TS.IFO", amg_lba, amg.len() as u32, 0),
                file(
                    "AUDIO_TS.VOB",
                    amg_lba + read32(&amg, 0xc0).unwrap(),
                    52 * 2048,
                    0,
                ),
                file(
                    "AUDIO_TS.BUP",
                    amg_lba + read32(&amg, 12).unwrap() + 1 - amg.len() as u32 / 2048,
                    amg.len() as u32,
                    0,
                ),
            ];
            let first_table = read32(&amg, 0xc4).unwrap() as usize * 2048;
            let mut title = 0;
            for (group, count) in counts.iter().enumerate() {
                files.push(file(
                    &format!("ATS_{:02}_0.IFO", group + 1),
                    amg_lba + read32(&amg, first_table + 4 + 14 * title + 10).unwrap(),
                    2 * 2048,
                    0,
                ));
                title += count;
            }
            verify_navigation_pointers(&amg, &files, &[], 10000).unwrap();
        }
    }

    #[test]
    fn video_link_titles_resolve_vtsi_extents_relative_to_the_amg() {
        let (mut amg, audio_files) = navigation_fixture();
        let title_count = 4u16;
        let last_byte = 4 + 14 * usize::from(title_count) - 1;
        for table_start in [0x800usize, 0x1000] {
            amg[table_start..table_start + 2].copy_from_slice(&title_count.to_be_bytes());
            amg[table_start + 2..table_start + 4]
                .copy_from_slice(&(last_byte as u16).to_be_bytes());
            let record = table_start + 4 + 3 * 14;
            amg[record] = 0x41;
            amg[record + 8] = 1;
            amg[record + 10..record + 14].copy_from_slice(&200u32.to_be_bytes());
        }
        let video_files = [file("VTS_01_0.IFO", 219, 2 * 2048, 1)];
        verify_navigation_pointers(&amg, &audio_files, &video_files, 300).unwrap();
    }

    #[test]
    fn navigation_rejects_a_bad_pointer_in_either_declared_title_table() {
        let (mut amg, files) = navigation_fixture();
        let record = 0x1000 + 4 + 2 * 14;
        amg[record + 10..record + 14].copy_from_slice(&141u32.to_be_bytes());
        assert!(
            verify_navigation_pointers(&amg, &files, &[], 300)
                .unwrap_err()
                .starts_with("AMG_ATSI_POINTER_MISMATCH")
        );
    }

    #[test]
    fn navigation_rejects_wrong_menu_video_and_backup_extents() {
        let (mut amg, files) = navigation_fixture();
        amg[0xc0..0xc4].copy_from_slice(&31u32.to_be_bytes());
        assert!(
            verify_navigation_pointers(&amg, &files, &[], 300)
                .unwrap_err()
                .starts_with("AMG_AMGM_POINTER_MISMATCH")
        );

        let (mut amg, files) = navigation_fixture();
        amg[12..16].copy_from_slice(&38u32.to_be_bytes());
        assert!(
            verify_navigation_pointers(&amg, &files, &[], 300)
                .unwrap_err()
                .starts_with("AMG_BACKUP_POINTER_MISMATCH")
        );
    }

    #[test]
    fn navigation_rejects_out_of_range_pointers_and_table_lengths() {
        let (mut amg, files) = navigation_fixture();
        amg[0x30..0x34].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(
            verify_navigation_pointers(&amg, &files, &[], 300)
                .unwrap_err()
                .starts_with("AMG_ASVS_POINTER_OUT_OF_RANGE")
        );

        let (mut amg, files) = navigation_fixture();
        amg[0x800..0x802].copy_from_slice(&200u16.to_be_bytes());
        assert_eq!(
            verify_navigation_pointers(&amg, &files, &[], 300).unwrap_err(),
            "AMG_TITLE_TABLE_OUT_OF_RANGE"
        );
    }

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
    fn still_header_accepts_large_vobs_with_valid_sector_metadata() {
        let mut ifo = vec![0u8; 0x60];
        ifo[20..24].copy_from_slice(&4096u32.to_be_bytes());
        assert_eq!(stills(&ifo, 4097 * 2048), Ok(0));
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
