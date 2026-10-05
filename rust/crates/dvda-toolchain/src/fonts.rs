//! Build-time OpenType font preparation. Only the verified collection is shipped.
//!
//! This is a Rust port of the former FontTool, kept in the developer tool rather
//! than the application C17 DLL because no runtime font parsing API is needed.
use dvda_core::hash::sha256;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const COLLECTION_FILE: &str = "DvdaNotoCJK-Regular.ttc";
pub const TARGETS: [(&str, &str); 3] = [
    ("Noto Sans CJK SC", "NotoSansCJKsc-Regular.otf"),
    ("Noto Sans CJK JP", "NotoSansCJKjp-Regular.otf"),
    ("Noto Sans CJK KR", "NotoSansCJKkr-Regular.otf"),
];
const TTC: u32 = 0x7474_6366;
const CHECKSUM_MAGIC: u32 = 0xb1b0_afba;
const REPRESENTATIVES: [u32; 4] = [0x6c49, 0x3042, 0xac00, 0x41];

#[derive(Debug)]
pub struct FontFaceInspection {
    pub family_name: String,
    pub post_script_name: String,
    pub has_han: bool,
    pub has_kana: bool,
    pub has_hangul: bool,
    pub has_latin: bool,
    pub checksum_valid: bool,
}

#[derive(Debug)]
pub struct ExtractedFontFace {
    pub source_face_index: usize,
    pub family_name: String,
    pub post_script_name: String,
    pub output_path: PathBuf,
}

#[derive(Debug, Clone)]
struct Table {
    tag: [u8; 4],
    checksum: u32,
    offset: usize,
    length: usize,
}

#[derive(Debug)]
struct Face {
    offset: usize,
    scaler: u32,
    tables: Vec<Table>,
    family: String,
    post_script: String,
}

fn range<'a>(
    data: &'a [u8],
    offset: usize,
    length: usize,
    label: &str,
) -> Result<&'a [u8], String> {
    offset
        .checked_add(length)
        .and_then(|end| data.get(offset..end))
        .ok_or_else(|| format!("{label} is out of bounds: offset={offset}, length={length}"))
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_be_bytes(
        range(data, offset, 2, "uint16")?.try_into().unwrap(),
    ))
}
fn u32_at(data: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_be_bytes(
        range(data, offset, 4, "uint32")?.try_into().unwrap(),
    ))
}
fn put16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}
fn put32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}
fn align4(length: usize) -> Result<usize, String> {
    length
        .checked_add(3)
        .map(|value| value & !3)
        .ok_or("Font size overflow".into())
}
fn grow(length: usize, extra: usize) -> Result<usize, String> {
    length
        .checked_add(extra)
        .filter(|value| *value <= i32::MAX as usize)
        .ok_or("Font size exceeds supported range".into())
}
fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, bytes| {
        let mut word = [0; 4];
        word[..bytes.len()].copy_from_slice(bytes);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}
fn required<'a>(face: &'a Face, tag: &[u8; 4]) -> Result<&'a Table, String> {
    face.tables
        .iter()
        .find(|table| &table.tag == tag)
        .ok_or_else(|| format!("Font is missing the {} table", String::from_utf8_lossy(tag)))
}
fn table_bytes<'a>(data: &'a [u8], table: &Table) -> Result<&'a [u8], String> {
    range(
        data,
        table.offset,
        table.length,
        &String::from_utf8_lossy(&table.tag),
    )
}

fn read_names(data: &[u8]) -> Result<(String, String), String> {
    range(data, 0, 6, "name header")?;
    let count = u16_at(data, 2)? as usize;
    let strings = u16_at(data, 4)? as usize;
    range(data, 6, count * 12, "name records")?;
    if strings < 6 + count * 12 || strings > data.len() {
        return Err("Invalid name string storage offset".into());
    }
    let mut names: [Option<(u16, String)>; 2] = [None, None];
    for index in 0..count {
        let record = 6 + index * 12;
        let platform = u16_at(data, record)?;
        let language = u16_at(data, record + 4)?;
        let id = u16_at(data, record + 6)?;
        let length = u16_at(data, record + 8)? as usize;
        let offset = strings + u16_at(data, record + 10)? as usize;
        let bytes = range(data, offset, length, "name string")?;
        let value = if platform == 0 || platform == 3 {
            if !length.is_multiple_of(2) {
                continue;
            }
            String::from_utf16_lossy(
                &bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                    .collect::<Vec<_>>(),
            )
        } else {
            bytes.iter().map(|byte| char::from(*byte)).collect()
        };
        let value = value.trim_matches(['\0', ' ']);
        let slot = match id {
            1 => 0,
            6 => 1,
            _ => continue,
        };
        let score = match platform {
            3 if language == 0x409 => 400,
            0 => 300,
            3 => 200,
            _ => 100,
        };
        if !value.is_empty() && names[slot].as_ref().is_none_or(|(old, _)| score > *old) {
            names[slot] = Some((score, value.into()));
        }
    }
    Ok((
        names[0].take().ok_or("name table is missing name ID 1")?.1,
        names[1].take().ok_or("name table is missing name ID 6")?.1,
    ))
}

fn parse_face(data: &[u8], offset: usize) -> Result<Face, String> {
    range(data, offset, 12, "SFNT header")?;
    let scaler = u32_at(data, offset)?;
    if !matches!(
        scaler,
        0x0001_0000 | 0x4f54_544f | 0x7472_7565 | 0x7479_7031
    ) {
        return Err(format!("Unsupported SFNT scaler: {scaler:08X}"));
    }
    let count = u16_at(data, offset + 4)? as usize;
    if !(1..=256).contains(&count) {
        return Err(format!("Invalid SFNT table count: {count}"));
    }
    range(data, offset + 12, count * 16, "SFNT directory")?;
    let mut tables = Vec::with_capacity(count);
    let mut tags = HashSet::new();
    for index in 0..count {
        let record = offset + 12 + index * 16;
        let tag: [u8; 4] = data[record..record + 4].try_into().unwrap();
        if !tags.insert(tag) {
            return Err("Duplicate SFNT table tag".into());
        }
        let table = Table {
            tag,
            checksum: u32_at(data, record + 4)?,
            offset: u32_at(data, record + 8)? as usize,
            length: u32_at(data, record + 12)? as usize,
        };
        table_bytes(data, &table)?;
        tables.push(table);
    }
    let mut face = Face {
        offset,
        scaler,
        tables,
        family: String::new(),
        post_script: String::new(),
    };
    let (family, post_script) = read_names(table_bytes(data, required(&face, b"name")?)?)?;
    face.family = family;
    face.post_script = post_script;
    // Validate every supported mapping, even if an earlier subtable has all four
    // representative characters. Damaged secondary tables cannot be hidden.
    coverage(data, &face)?;
    Ok(face)
}

fn read_faces(data: &[u8]) -> Result<Vec<Face>, String> {
    if u32_at(data, 0)? != TTC {
        return Ok(vec![parse_face(data, 0)?]);
    }
    range(data, 0, 12, "TTC header")?;
    let version = u32_at(data, 4)?;
    if !matches!(version, 0x0001_0000 | 0x0002_0000) {
        return Err("Unsupported TTC version".into());
    }
    let count = u32_at(data, 8)? as usize;
    if !(1..=256).contains(&count) {
        return Err(format!("Invalid TTC face count: {count}"));
    }
    let header = 12 + count * 4 + if version == 0x0002_0000 { 12 } else { 0 };
    range(data, 0, header, "TTC face offsets")?;
    (0..count)
        .map(|index| {
            let offset = u32_at(data, 12 + index * 4)? as usize;
            if offset < header {
                return Err("TTC face overlaps its header".into());
            }
            parse_face(data, offset)
        })
        .collect()
}

fn format12(data: &[u8], coverage: &mut [bool; 4]) -> Result<(), String> {
    range(data, 0, 16, "cmap format 12 header")?;
    let count = u32_at(data, 12)? as usize;
    range(
        data,
        16,
        count.checked_mul(12).ok_or("cmap group count overflow")?,
        "cmap format 12 groups",
    )?;
    let mut previous_end = None;
    for index in 0..count {
        let group = 16 + index * 12;
        let start = u32_at(data, group)?;
        let end = u32_at(data, group + 4)?;
        let glyph = u32_at(data, group + 8)?;
        if start > end
            || end > 0x10ffff
            || previous_end.is_some_and(|previous| start <= previous)
            || glyph.checked_add(end - start).is_none()
        {
            return Err("Invalid cmap format 12 group mapping".into());
        }
        for (point, found) in REPRESENTATIVES.iter().zip(coverage.iter_mut()) {
            if *point >= start && *point <= end && glyph + *point - start != 0 {
                *found = true;
            }
        }
        previous_end = Some(end);
    }
    Ok(())
}

fn format4(data: &[u8], coverage: &mut [bool; 4]) -> Result<(), String> {
    range(data, 0, 16, "cmap format 4 header")?;
    let count_x2 = u16_at(data, 6)? as usize;
    if count_x2 == 0 || !count_x2.is_multiple_of(2) {
        return Err("Invalid cmap format 4 segment count".into());
    }
    let count = count_x2 / 2;
    let starts = 16 + count * 2;
    let deltas = starts + count * 2;
    let offsets = deltas + count * 2;
    range(data, offsets, count * 2, "cmap format 4 segments")?;
    let mut previous_end = None;
    for index in 0..count {
        let start = u16_at(data, starts + index * 2)?;
        let end = u16_at(data, 14 + index * 2)?;
        let delta = u16_at(data, deltas + index * 2)?;
        let position = offsets + index * 2;
        let relative = u16_at(data, position)? as usize;
        if start > end
            || previous_end.is_some_and(|previous| start <= previous)
            || !relative.is_multiple_of(2)
        {
            return Err("Invalid cmap format 4 segment mapping".into());
        }
        if relative != 0 {
            let glyph_start = position + relative;
            if glyph_start < offsets + count * 2 {
                return Err("Invalid cmap glyph array offset".into());
            }
            range(
                data,
                glyph_start,
                (end as usize - start as usize + 1) * 2,
                "cmap format 4 glyph array",
            )?;
        }
        for (point, found) in REPRESENTATIVES.iter().zip(coverage.iter_mut()) {
            if *point < start as u32 || *point > end as u32 {
                continue;
            }
            let glyph = if relative == 0 {
                (*point as u16).wrapping_add(delta)
            } else {
                let raw = u16_at(
                    data,
                    position + relative + (*point as usize - start as usize) * 2,
                )?;
                if raw == 0 { 0 } else { raw.wrapping_add(delta) }
            };
            if glyph != 0 {
                *found = true;
            }
        }
        previous_end = Some(end);
    }
    Ok(())
}

fn coverage(data: &[u8], face: &Face) -> Result<[bool; 4], String> {
    let cmap = table_bytes(data, required(face, b"cmap")?)?;
    range(cmap, 0, 4, "cmap header")?;
    let count = u16_at(cmap, 2)? as usize;
    range(cmap, 4, count * 8, "cmap encoding records")?;
    let mut covered = [false; 4];
    let mut visited = HashSet::new();
    for index in 0..count {
        let offset = u32_at(cmap, 4 + index * 8 + 4)? as usize;
        if offset < 4 + count * 8 {
            return Err("cmap subtable overlaps encoding records".into());
        }
        if !visited.insert(offset) {
            continue;
        }
        let format = u16_at(cmap, offset)?;
        match format {
            4 => {
                let length = u16_at(cmap, offset + 2)? as usize;
                format4(range(cmap, offset, length, "cmap format 4")?, &mut covered)?;
            }
            12 => {
                let length = u32_at(cmap, offset + 4)? as usize;
                format12(range(cmap, offset, length, "cmap format 12")?, &mut covered)?;
            }
            // Other mappings (e.g. CJK variation selector format 14) are retained
            // byte-for-byte, but do not count towards the four coverage probes.
            _ => {}
        }
    }
    Ok(covered)
}

fn validate_tables(data: &[u8], face: &Face) -> Result<(), String> {
    for table in &face.tables {
        let bytes = table_bytes(data, table)?;
        let actual = if &table.tag == b"head" {
            range(bytes, 0, 12, "head table")?;
            checksum(bytes).wrapping_sub(u32_at(bytes, 8)?)
        } else {
            checksum(bytes)
        };
        if actual != table.checksum {
            return Err(format!(
                "Invalid {} table checksum",
                String::from_utf8_lossy(&table.tag)
            ));
        }
    }
    Ok(())
}

fn standalone(source: &[u8], face: &Face) -> Result<Vec<u8>, String> {
    let count = face.tables.len();
    let mut length = 12 + count * 16;
    for table in &face.tables {
        length = grow(align4(length)?, align4(table.length)?)?;
    }
    let mut output = vec![0; length];
    put32(&mut output, 0, face.scaler);
    put16(&mut output, 4, count as u16);
    let power = 1usize << (usize::BITS - 1 - count.leading_zeros());
    put16(&mut output, 6, (power * 16) as u16);
    put16(&mut output, 8, power.ilog2() as u16);
    put16(&mut output, 10, (count * 16 - power * 16) as u16);
    let mut offset = 12 + count * 16;
    let mut head = None;
    for (index, table) in face.tables.iter().enumerate() {
        offset = align4(offset)?;
        output[offset..offset + table.length].copy_from_slice(table_bytes(source, table)?);
        if &table.tag == b"head" {
            if table.length < 12 {
                return Err("head table is too short".into());
            }
            put32(&mut output, offset + 8, 0);
            head = Some(offset);
        }
        let record = 12 + index * 16;
        output[record..record + 4].copy_from_slice(&table.tag);
        let sum = checksum(&output[offset..offset + table.length]);
        put32(&mut output, record + 4, sum);
        put32(&mut output, record + 8, offset as u32);
        put32(&mut output, record + 12, table.length as u32);
        offset += align4(table.length)?;
    }
    let head = head.ok_or("Font is missing the head table")?;
    let adjustment = CHECKSUM_MAGIC.wrapping_sub(checksum(&output));
    put32(&mut output, head + 8, adjustment);
    if checksum(&output) != CHECKSUM_MAGIC {
        return Err("Could not rebuild a valid checkSumAdjustment".into());
    }
    Ok(output)
}

fn inspect_bytes(data: &[u8]) -> Result<FontFaceInspection, String> {
    if u32_at(data, 0)? == TTC {
        return Err("Expected a single font face, got a TTC collection".into());
    }
    let face = parse_face(data, 0)?;
    let [has_han, has_kana, has_hangul, has_latin] = coverage(data, &face)?;
    Ok(FontFaceInspection {
        family_name: face.family,
        post_script_name: face.post_script,
        has_han,
        has_kana,
        has_hangul,
        has_latin,
        checksum_valid: checksum(data) == CHECKSUM_MAGIC,
    })
}

fn verify_bytes(data: &[u8], expected_family: &str) -> Result<FontFaceInspection, String> {
    let inspection = inspect_bytes(data)?;
    if inspection.family_name != expected_family {
        return Err(format!(
            "Expected family '{expected_family}', got '{}'",
            inspection.family_name
        ));
    }
    let missing: Vec<_> = [
        inspection.has_han,
        inspection.has_kana,
        inspection.has_hangul,
        inspection.has_latin,
    ]
    .into_iter()
    .zip(["Han", "Kana", "Hangul", "Latin"])
    .filter_map(|(present, name)| (!present).then_some(name))
    .collect();
    if !missing.is_empty() {
        return Err(format!(
            "Font is missing representative characters: {}",
            missing.join(", ")
        ));
    }
    if !inspection.checksum_valid {
        return Err("Invalid OpenType whole-font checksum".into());
    }
    validate_tables(data, &parse_face(data, 0)?)?;
    Ok(inspection)
}

pub fn inspect_face(path: &Path) -> Result<FontFaceInspection, String> {
    inspect_bytes(&fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?)
}
pub fn verify_face(path: &Path, expected_family: &str) -> Result<FontFaceInspection, String> {
    verify_bytes(
        &fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?,
        expected_family,
    )
}

fn verify_collection_bytes(data: &[u8]) -> Result<(), String> {
    if u32_at(data, 0)? != TTC {
        return Err("Expected a TTC collection".into());
    }
    let faces = read_faces(data)?;
    if faces.len() != TARGETS.len() {
        return Err("Menu collection must contain exactly SC, JP and KR faces".into());
    }
    for (index, (face, (family, _))) in faces.iter().zip(TARGETS).enumerate() {
        if face.family != family || !coverage(data, face)?.into_iter().all(|present| present) {
            return Err(format!(
                "Collection face[{index}] has an incorrect family or representative character coverage"
            ));
        }
        validate_tables(data, face)?;
        verify_bytes(&standalone(data, face)?, family)?;
    }
    Ok(())
}

pub fn verify_noto_cjk_collection(path: &Path) -> Result<(), String> {
    verify_collection_bytes(
        &fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?,
    )
}

fn atomic_write(destination: &Path, data: &[u8]) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let absolute = std::path::absolute(destination).map_err(|e| e.to_string())?;
    fs::create_dir_all(absolute.parent().ok_or("Destination has no parent")?)
        .map_err(|e| e.to_string())?;
    let mut name = absolute.as_os_str().to_owned();
    name.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = PathBuf::from(name);
    let mut created = false;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        created = true;
        file.write_all(data).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temporary, &absolute).map_err(|e| e.to_string())
    })();
    if created && temporary.is_file() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn extract_noto_cjk_faces(
    source: &Path,
    destination: &Path,
) -> Result<Vec<ExtractedFontFace>, String> {
    let data = fs::read(source).map_err(|e| e.to_string())?;
    let faces = read_faces(&data)?;
    if faces.len() < 2 {
        return Err("Input is not a multi-face TTC collection".into());
    }
    // Validate every target before writing any output; malformed later faces do
    // not leave a misleading partial set of extracted fonts.
    let mut prepared = Vec::new();
    for (family, filename) in TARGETS {
        let matches: Vec<_> = faces
            .iter()
            .enumerate()
            .filter(|(_, face)| face.family == family)
            .collect();
        if matches.len() != 1 {
            return Err(format!(
                "Expected exactly one '{family}' family, found {}",
                matches.len()
            ));
        }
        let (index, face) = matches[0];
        validate_tables(&data, face)?;
        let output = standalone(&data, face)?;
        let inspection = verify_bytes(&output, family)?;
        prepared.push((
            ExtractedFontFace {
                source_face_index: index,
                family_name: inspection.family_name,
                post_script_name: inspection.post_script_name,
                output_path: destination.join(filename),
            },
            output,
        ));
    }
    let mut results = Vec::new();
    for (result, output) in prepared {
        atomic_write(&result.output_path, &output)?;
        results.push(result);
    }
    Ok(results)
}

fn pack_bytes(sources: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    if sources.len() != TARGETS.len() {
        return Err("Expected three regional source fonts".into());
    }
    let mut faces = Vec::new();
    for (source, (family, _)) in sources.iter().zip(TARGETS) {
        verify_bytes(source, family)?;
        faces.push(parse_face(source, 0)?);
    }
    let mut directories = Vec::new();
    let mut length = 12 + sources.len() * 4;
    for face in &faces {
        directories.push(length);
        length = grow(length, 12 + face.tables.len() * 16)?;
    }
    type SharedTables = HashMap<([u8; 4], [u8; 32]), (usize, Vec<u8>)>;
    let mut shared: SharedTables = HashMap::new();
    let mut all_offsets = Vec::new();
    for (source, face) in sources.iter().zip(&faces) {
        let mut offsets = Vec::new();
        for table in &face.tables {
            let bytes = table_bytes(source, table)?;
            let key = (table.tag, sha256(bytes));
            let offset = if let Some((offset, previous)) = shared.get(&key) {
                if previous != bytes {
                    return Err("Font table hash collision; refusing to merge".into());
                }
                *offset
            } else {
                let offset = length;
                length = grow(length, align4(bytes.len())?)?;
                shared.insert(key, (offset, bytes.to_vec()));
                offset
            };
            offsets.push(offset);
        }
        all_offsets.push(offsets);
    }
    let mut output = vec![0; length];
    put32(&mut output, 0, TTC);
    put32(&mut output, 4, 0x0001_0000);
    put32(&mut output, 8, sources.len() as u32);
    for (index, ((source, face), directory)) in
        sources.iter().zip(&faces).zip(directories).enumerate()
    {
        put32(&mut output, 12 + index * 4, directory as u32);
        output[directory..directory + 12].copy_from_slice(&source[face.offset..face.offset + 12]);
        for (table_index, table_offset) in all_offsets[index].iter().enumerate() {
            let record = 12 + table_index * 16;
            output[directory + record..directory + record + 16]
                .copy_from_slice(&source[record..record + 16]);
            put32(&mut output, directory + record + 8, *table_offset as u32);
        }
    }
    for (_, (offset, bytes)) in shared {
        output[offset..offset + bytes.len()].copy_from_slice(&bytes);
    }
    let roundtrip = read_faces(&output)?;
    for ((source, face), rebuilt) in sources.iter().zip(&faces).zip(&roundtrip) {
        if standalone(source, face)? != standalone(&output, rebuilt)? {
            return Err("Regional face changed when merging font tables".into());
        }
    }
    verify_collection_bytes(&output)?;
    Ok(output)
}

pub fn pack_noto_cjk_faces(source: &Path, destination: &Path) -> Result<(), String> {
    let sources = TARGETS
        .iter()
        .map(|(_, name)| {
            fs::read(source.join(name))
                .map_err(|error| format!("{}: {error}", source.join(name).display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    atomic_write(destination, &pack_bytes(&sources)?)
}

/// Stage exactly one verified font collection. Existing invalid candidates are
/// errors rather than a reason to silently use unrelated cached release fonts.
pub fn stage_menu_fonts(sources: &[PathBuf], destination: &Path) -> Result<(), String> {
    for source in sources {
        if TARGETS.iter().all(|(_, name)| source.join(name).is_file()) {
            return pack_noto_cjk_faces(source, &destination.join(COLLECTION_FILE));
        }
        let collection = source.join(COLLECTION_FILE);
        if collection.is_file() {
            let data = fs::read(&collection).map_err(|e| e.to_string())?;
            verify_collection_bytes(&data)?;
            return atomic_write(&destination.join(COLLECTION_FILE), &data);
        }
    }
    Err(format!(
        "Required menu fonts are missing. Supply {COLLECTION_FILE} or all SC/JP/KR source OTF files in: {}",
        sources
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn print_inspection(value: &FontFaceInspection) {
    println!(
        "family='{}' ps='{}' Han={} Kana={} Hangul={} Latin={} checksum={}",
        value.family_name,
        value.post_script_name,
        value.has_han,
        value.has_kana,
        value.has_hangul,
        value.has_latin,
        value.checksum_valid
    );
}

pub fn run(args: &[String]) -> Result<(), String> {
    match args {
        [command, source, output] if command == "extract" => {
            for face in extract_noto_cjk_faces(Path::new(source), Path::new(output))? {
                println!("face[{}] '{}' ps='{}' -> {} ({} bytes)", face.source_face_index, face.family_name, face.post_script_name, face.output_path.display(), fs::metadata(&face.output_path).map_err(|e| e.to_string())?.len());
            }
        }
        [command, path, family] if command == "verify" => print_inspection(&verify_face(Path::new(path), family)?),
        [command, path] if command == "inspect" => print_inspection(&inspect_face(Path::new(path))?),
        [command, source, output] if command == "pack" => {
            pack_noto_cjk_faces(Path::new(source), Path::new(output))?;
            println!("[OK] Verified SC/JP/KR collection: {output}");
        }
        [command, path] if command == "verify-collection" => {
            verify_noto_cjk_collection(Path::new(path))?;
            println!("[OK] Verified SC/JP/KR collection: {path}");
        }
        _ => return Err("Usage: dvda-toolchain font extract <TTC> <output-directory> | verify <OTF> <family> | inspect <OTF> | pack <OTF-directory> <TTC> | verify-collection <TTC>".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "dvda-font-tests-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn names(family: &str) -> Vec<u8> {
        let family: Vec<u8> = family.encode_utf16().flat_map(u16::to_be_bytes).collect();
        let ps: Vec<u8> = "Regional-Regular"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect();
        let mut data = vec![0; 30];
        put16(&mut data, 2, 2);
        put16(&mut data, 4, 30);
        for (index, (id, bytes, offset)) in [(1, &family, 0), (6, &ps, family.len())]
            .into_iter()
            .enumerate()
        {
            let record = 6 + index * 12;
            put16(&mut data, record, 3);
            put16(&mut data, record + 2, 1);
            put16(&mut data, record + 4, 0x409);
            put16(&mut data, record + 6, id);
            put16(&mut data, record + 8, bytes.len() as u16);
            put16(&mut data, record + 10, offset as u16);
        }
        data.extend_from_slice(&family);
        data.extend_from_slice(&ps);
        data
    }

    fn cmap12(missing: Option<u32>) -> Vec<u8> {
        let mut points = REPRESENTATIVES.to_vec();
        points.sort_unstable();
        points.retain(|point| Some(*point) != missing);
        let mut data = vec![0; 28 + points.len() * 12];
        put16(&mut data, 2, 1);
        put16(&mut data, 4, 3);
        put16(&mut data, 6, 10);
        put32(&mut data, 8, 12);
        put16(&mut data, 12, 12);
        let length = data.len() - 12;
        put32(&mut data, 16, length as u32);
        put32(&mut data, 24, points.len() as u32);
        for (index, point) in points.into_iter().enumerate() {
            let record = 28 + index * 12;
            put32(&mut data, record, point);
            put32(&mut data, record + 4, point);
            put32(&mut data, record + 8, index as u32 + 1);
        }
        data
    }

    fn cmap4() -> Vec<u8> {
        let mut points = REPRESENTATIVES.to_vec();
        points.sort_unstable();
        points.push(0xffff);
        let count = points.len();
        let mut data = vec![0; 12 + 16 + count * 8];
        put16(&mut data, 2, 1);
        put16(&mut data, 4, 3);
        put16(&mut data, 6, 1);
        put32(&mut data, 8, 12);
        put16(&mut data, 12, 4);
        put16(&mut data, 14, (16 + count * 8) as u16);
        put16(&mut data, 18, (count * 2) as u16);
        for (index, point) in points.into_iter().enumerate() {
            put16(&mut data, 12 + 14 + index * 2, point as u16);
            put16(&mut data, 12 + 16 + count * 2 + index * 2, point as u16);
            // Real representative code points map to nonzero IDs; the final
            // required 0xffff sentinel maps to .notdef.
            put16(
                &mut data,
                12 + 16 + count * 4 + index * 2,
                if point == 0xffff { 1 } else { 0 },
            );
        }
        data
    }

    fn synthetic(family: &str, cmap: Vec<u8>, marker: u8) -> Vec<u8> {
        let source_tables = [
            (b"head", vec![0u8; 54]),
            (b"name", names(family)),
            (b"cmap", cmap),
            (b"CFF ", vec![marker; 9]),
            (b"hhea", vec![7; 36]),
        ];
        let mut data = vec![0; 12 + source_tables.len() * 16];
        put32(&mut data, 0, 0x4f54_544f);
        put16(&mut data, 4, source_tables.len() as u16);
        for (index, (tag, bytes)) in source_tables.into_iter().enumerate() {
            let offset = data.len();
            let record = 12 + index * 16;
            data[record..record + 4].copy_from_slice(tag);
            put32(&mut data, record + 4, checksum(&bytes));
            put32(&mut data, record + 8, offset as u32);
            put32(&mut data, record + 12, bytes.len() as u32);
            data.extend_from_slice(&bytes);
            data.resize(align4(data.len()).unwrap(), 0);
        }
        standalone(&data, &parse_face(&data, 0).unwrap()).unwrap()
    }

    fn sources() -> Vec<Vec<u8>> {
        TARGETS
            .iter()
            .enumerate()
            .map(|(index, (family, _))| synthetic(family, cmap12(None), index as u8))
            .collect()
    }

    #[test]
    fn fonts_parse_rejects_truncated_headers_counts_offsets_names_and_mappings() {
        let good = sources().remove(0);
        for length in 0..12 {
            assert!(read_faces(&good[..length]).is_err());
        }
        let mut bad = good.clone();
        put16(&mut bad, 4, 257);
        assert!(read_faces(&bad).is_err());
        let mut bad = good.clone();
        put32(&mut bad, 20, u32::MAX);
        assert!(read_faces(&bad).is_err());
        let face = parse_face(&good, 0).unwrap();
        let name = required(&face, b"name").unwrap();
        let mut bad = good.clone();
        put16(&mut bad, name.offset + 4, name.length as u16 - 1);
        assert!(read_faces(&bad).is_err());
        let cmap = required(&face, b"cmap").unwrap();
        let mut bad = good.clone();
        put32(&mut bad, cmap.offset + 8, cmap.length as u32 + 1);
        assert!(read_faces(&bad).is_err());
        let mut bad = good.clone();
        put32(&mut bad, cmap.offset + 24, u32::MAX);
        assert!(read_faces(&bad).is_err());
        let mut bad = good.clone();
        put32(&mut bad, cmap.offset + 28, 0x110000);
        assert!(read_faces(&bad).is_err());
        let mut collection = pack_bytes(&sources()).unwrap();
        for count in [0, 257, u32::MAX] {
            put32(&mut collection, 8, count);
            assert!(read_faces(&collection).is_err());
        }
    }

    #[test]
    fn fonts_verify_family_each_representative_and_whole_font_checksum() {
        for (family, _) in TARGETS {
            let good = synthetic(family, cmap12(None), 1);
            let inspected = verify_bytes(&good, family).unwrap();
            assert!(inspected.checksum_valid);
            assert!(verify_bytes(&good, "Wrong family").is_err());
            for missing in REPRESENTATIVES {
                assert!(
                    verify_bytes(&synthetic(family, cmap12(Some(missing)), 1), family)
                        .unwrap_err()
                        .contains("representative")
                );
            }
            let mut wrong = good;
            let last = wrong.len() - 1;
            wrong[last] ^= 1;
            assert!(!inspect_bytes(&wrong).unwrap().checksum_valid);
            assert!(
                verify_bytes(&wrong, family)
                    .unwrap_err()
                    .contains("whole-font")
            );
        }
        assert!(
            inspect_bytes(&pack_bytes(&sources()).unwrap())
                .unwrap_err()
                .contains("TTC")
        );
    }

    #[test]
    fn fonts_cmap4_handles_direct_and_array_glyphs_and_rejects_bad_ranges() {
        let family = TARGETS[0].0;
        let good = synthetic(family, cmap4(), 1);
        verify_bytes(&good, family).unwrap();
        let mut mapping = cmap4();
        // First segment (Latin A) uses a glyph array located after five offsets.
        let offsets = 12 + 16 + 5 * 6;
        put16(&mut mapping, offsets, 10);
        mapping.extend_from_slice(&1u16.to_be_bytes());
        let length = mapping.len() - 12;
        put16(&mut mapping, 14, length as u16);
        verify_bytes(&synthetic(family, mapping.clone(), 1), family).unwrap();
        let last = mapping.len() - 1;
        mapping[last] = 0;
        assert!(verify_bytes(&synthetic(family, mapping.clone(), 1), family).is_err());
        put16(&mut mapping, offsets, 0xfffe);
        let mut found = [false; 4];
        assert!(format4(&mapping[12..], &mut found).is_err());
        let mut mapping = cmap4();
        put16(&mut mapping, 18, 9);
        assert!(format4(&mapping[12..], &mut found).is_err());
        let mut mapping = cmap4();
        put16(&mut mapping, 12 + 16 + 5 * 2, 0xffff);
        assert!(format4(&mapping[12..], &mut found).is_err());
    }

    #[test]
    fn fonts_pack_deduplicates_shared_tables_preserves_regions_and_rebuilds_exactly() {
        let sources = sources();
        let packed = pack_bytes(&sources).unwrap();
        let faces = read_faces(&packed).unwrap();
        assert!(packed.len() < sources.iter().map(Vec::len).sum::<usize>());
        for (index, face) in faces.iter().enumerate() {
            assert_eq!(standalone(&packed, face).unwrap(), sources[index]);
            assert_eq!(
                required(face, b"hhea").unwrap().offset,
                required(&faces[0], b"hhea").unwrap().offset
            );
            assert_eq!(
                table_bytes(&packed, required(face, b"CFF ").unwrap()).unwrap(),
                &[index as u8; 9]
            );
            for table in &face.tables {
                assert!(table.offset.is_multiple_of(4));
            }
        }
        assert_eq!(pack_bytes(&sources).unwrap(), packed);
    }

    #[test]
    fn fonts_collection_rejects_wrong_order_count_family_coverage_and_table_checksum() {
        let good = pack_bytes(&sources()).unwrap();
        let mut wrong = good.clone();
        let first = u32_at(&wrong, 12).unwrap();
        let second = u32_at(&wrong, 16).unwrap();
        put32(&mut wrong, 12, second);
        put32(&mut wrong, 16, first);
        assert!(verify_collection_bytes(&wrong).is_err());
        let mut wrong = good.clone();
        put32(&mut wrong, 8, 2);
        assert!(
            verify_collection_bytes(&wrong)
                .unwrap_err()
                .contains("exactly")
        );
        let mut wrong = good.clone();
        // Insert another directory offset while adjusting existing directory and
        // table addresses to form a structurally valid four-face collection.
        wrong.splice(24..24, [0, 0, 0, 0]);
        put32(&mut wrong, 8, 4);
        let faces = read_faces(&good).unwrap();
        for (index, face) in faces.iter().enumerate() {
            put32(&mut wrong, 12 + index * 4, (face.offset + 4) as u32);
            for (table_index, table) in face.tables.iter().enumerate() {
                put32(
                    &mut wrong,
                    face.offset + 4 + 12 + table_index * 16 + 8,
                    (table.offset + 4) as u32,
                );
            }
        }
        put32(&mut wrong, 24, (faces[0].offset + 4) as u32);
        assert!(
            verify_collection_bytes(&wrong)
                .unwrap_err()
                .contains("exactly")
        );
        let mut wrong = good.clone();
        let name = required(&faces[0], b"name").unwrap();
        wrong[name.offset + 30 + TARGETS[0].0.encode_utf16().count() * 2 - 1] = b'X';
        assert!(
            verify_collection_bytes(&wrong)
                .unwrap_err()
                .contains("incorrect family")
        );
        let mut wrong = good.clone();
        wrong[name.offset + name.length - 1] ^= 1;
        assert!(verify_collection_bytes(&wrong).is_err());
        let mut wrong = good.clone();
        let cmap = required(&faces[0], b"cmap").unwrap();
        put32(&mut wrong, cmap.offset + 28 + 8, 0);
        assert!(verify_collection_bytes(&wrong).is_err());
        let mut wrong = good;
        let glyph = required(&faces[0], b"CFF ").unwrap();
        wrong[glyph.offset] ^= 1;
        assert!(
            verify_collection_bytes(&wrong)
                .unwrap_err()
                .contains("checksum")
        );
    }

    #[test]
    fn fonts_extract_rejects_missing_duplicate_families_before_writing() {
        let scratch = Scratch::new();
        let source = scratch.0.join("source.ttc");
        let output = scratch.0.join("output");
        let good = pack_bytes(&sources()).unwrap();
        fs::write(&source, &good).unwrap();
        let results = extract_noto_cjk_faces(&source, &output).unwrap();
        assert_eq!(
            results
                .iter()
                .map(|result| result.source_face_index)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        let hashes: Vec<_> = results
            .iter()
            .map(|result| sha256(&fs::read(&result.output_path).unwrap()))
            .collect();
        let mut reordered = good.clone();
        let first = u32_at(&reordered, 12).unwrap();
        let second = u32_at(&reordered, 16).unwrap();
        put32(&mut reordered, 12, second);
        put32(&mut reordered, 16, first);
        fs::write(&source, &reordered).unwrap();
        let extracted = extract_noto_cjk_faces(&source, &output).unwrap();
        assert_eq!(
            extracted
                .iter()
                .map(|result| result.source_face_index)
                .collect::<Vec<_>>(),
            [1, 0, 2]
        );
        let mut wrong = good.clone();
        let first = u32_at(&wrong, 12).unwrap();
        put32(&mut wrong, 16, first);
        fs::write(&source, &wrong).unwrap();
        assert!(
            extract_noto_cjk_faces(&source, &output)
                .unwrap_err()
                .contains("found 2")
        );
        let mut wrong = good;
        put32(&mut wrong, 8, 2);
        fs::write(&source, &wrong).unwrap();
        assert!(
            extract_noto_cjk_faces(&source, &output)
                .unwrap_err()
                .contains("found 0")
        );
        assert_eq!(
            results
                .iter()
                .map(|result| sha256(&fs::read(&result.output_path).unwrap()))
                .collect::<Vec<_>>(),
            hashes
        );
        assert_eq!(fs::read_dir(&output).unwrap().count(), 3);
    }

    #[test]
    fn fonts_staging_accepts_sources_or_collection_and_only_copies_one_file() {
        let scratch = Scratch::new();
        let source = scratch.0.join("原始 OTF");
        let stage = scratch.0.join("staged");
        fs::create_dir_all(&source).unwrap();
        for ((_, name), data) in TARGETS.iter().zip(sources()) {
            fs::write(source.join(name), data).unwrap();
        }
        stage_menu_fonts(std::slice::from_ref(&source), &stage).unwrap();
        verify_noto_cjk_collection(&stage.join(COLLECTION_FILE)).unwrap();
        assert_eq!(fs::read_dir(&stage).unwrap().count(), 1);
        assert_eq!(fs::read_dir(&source).unwrap().count(), 3);
        // The original packer regenerates from complete source faces even if a
        // stale prepacked TTC is also present in the same source directory.
        fs::write(source.join(COLLECTION_FILE), b"stale collection").unwrap();
        let regenerated = scratch.0.join("regenerated");
        stage_menu_fonts(std::slice::from_ref(&source), &regenerated).unwrap();
        assert_eq!(
            fs::read(stage.join(COLLECTION_FILE)).unwrap(),
            fs::read(regenerated.join(COLLECTION_FILE)).unwrap()
        );
        let second = scratch.0.join("from-ttc");
        stage_menu_fonts(std::slice::from_ref(&stage), &second).unwrap();
        assert_eq!(
            fs::read(stage.join(COLLECTION_FILE)).unwrap(),
            fs::read(second.join(COLLECTION_FILE)).unwrap()
        );
        fs::write(stage.join(COLLECTION_FILE), b"broken").unwrap();
        assert!(stage_menu_fonts(&[stage, source], &scratch.0.join("invalid")).is_err());
    }

    #[test]
    fn fonts_atomic_replacement_preserves_old_output_on_failure() {
        let scratch = Scratch::new();
        let output = scratch.0.join("font.ttc");
        atomic_write(&output, b"old complete file").unwrap();
        atomic_write(&output, b"new complete file").unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"new complete file");
        let blocked = scratch.0.join("existing directory");
        fs::create_dir_all(&blocked).unwrap();
        assert!(atomic_write(&blocked, b"cannot replace directory").is_err());
        assert!(blocked.is_dir());
        assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 2);
        #[cfg(windows)]
        {
            let file = OpenOptions::new().read(true).open(&output).unwrap();
            // An explicit no-delete-share handle represents a destination in use.
            use std::os::windows::fs::OpenOptionsExt;
            let lock = OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&output)
                .unwrap();
            assert!(atomic_write(&output, b"incomplete replacement").is_err());
            assert_eq!(fs::read(&output).unwrap(), b"new complete file");
            assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 2);
            drop(lock);
            drop(file);
        }
    }
}
