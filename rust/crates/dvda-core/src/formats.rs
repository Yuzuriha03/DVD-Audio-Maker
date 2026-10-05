use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::{Read, Seek, SeekFrom, Write},
};
use std::{fs, path::Path};

struct IsoSource {
    file: RefCell<fs::File>,
    length: u64,
}
impl IsoSource {
    fn open(path: &str) -> Result<Self, String> {
        let file = crate::identity::open_read(path).map_err(|e| format!("{path}: {e}"))?;
        let length = file.metadata().map_err(|e| e.to_string())?.len();
        Ok(Self {
            file: RefCell::new(file),
            length,
        })
    }

    fn read_at(&self, offset: u64, count: usize) -> Result<Vec<u8>, String> {
        if count > i32::MAX as usize {
            return Err("Requested ISO range exceeds the managed size limit".into());
        }
        let available = self.length.saturating_sub(offset).min(count as u64) as usize;
        let mut buffer = vec![0; available];
        let mut file = self.file.borrow_mut();
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        file.read_exact(&mut buffer).map_err(|e| e.to_string())?;
        Ok(buffer)
    }

    fn extract(&self, offset: u64, size: u32, destination: &Path) -> Result<(), String> {
        if size > i32::MAX as u32 {
            return Err("Requested ISO file exceeds the managed size limit".into());
        }
        if let Some(parent) = destination.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut file = self.file.borrow_mut();
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let available = self.length.saturating_sub(offset).min(u64::from(size));
        let mut limited = (&mut *file).take(available);
        let mut output = fs::File::create(destination).map_err(|e| e.to_string())?;
        let copied = std::io::copy(&mut limited, &mut output).map_err(|e| e.to_string())?;
        if copied != available {
            return Err("ISO changed during extraction".into());
        }
        Ok(())
    }
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "iso.info" => iso_info(request),
        "iso.list" => iso_list(request),
        "iso.entry" => iso_entry(request),
        "iso.all_paths" => iso_all_paths(request),
        "iso.data_lba" => iso_data_lba(request),
        "iso.read_file" => iso_read_file(request),
        "iso.extract" => iso_extract(request),
        "iso.strip_version" => strip_version(request),
        "wav.layout" => wav_layout(request),
        "wav.normalize" => wav_normalize(request),
        "flac.comments" => flac_comments(request),
        "flac.picture" => flac_picture(request),
        "flac.replace_picture" => flac_replace_picture(request),
        "alac.cookie" => alac_cookie(request),
        "alac.find_bad_frames" => alac_find_bad_frames(request),
        "alac.apply_patches" => alac_apply_patches(request),
        _ => Err(format!("Unsupported Rust format operation: {operation}")),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct IsoEntry {
    name: String,
    is_directory: bool,
    logical_block_address: u32,
    size: u32,
    extended_attribute_blocks: u8,
    is_multi_extent: bool,
}

/// Directory entry exposed to the Rust verification and desktop layers.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct IsoEntryInfo {
    pub name: String,
    pub is_directory: bool,
    pub logical_block_address: u32,
    pub size: u32,
    pub extended_attribute_blocks: u8,
    pub is_multi_extent: bool,
}

/// A bounded, sector-aligned reader for one ISO9660 file.
pub struct IsoFileChunks {
    source: IsoSource,
    offset: u64,
    remaining: u64,
    chunk_size: usize,
}

impl IsoFileChunks {
    pub fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, String> {
        if self.remaining == 0 {
            return Ok(None);
        }
        let count = self
            .remaining
            .min(self.chunk_size as u64)
            .try_into()
            .map_err(|_| "ISO chunk size overflow")?;
        let bytes = self.source.read_at(self.offset, count)?;
        if bytes.len() != count {
            return Err("ISO file ended before its directory-declared size".into());
        }
        self.offset = self
            .offset
            .checked_add(count as u64)
            .ok_or("ISO file offset overflow")?;
        self.remaining -= count as u64;
        Ok(Some(bytes))
    }
}

pub fn iso_list_directory(path: &Path, inner_path: &str) -> Result<Vec<IsoEntryInfo>, String> {
    let source = IsoSource::open(&path.to_string_lossy())?;
    let info = parse_iso_info(&path.to_string_lossy(), &source)?;
    let entry = iso_lookup(&source, &info, inner_path)?;
    if !entry.is_directory {
        return Err(format!("ISO entry is not a directory: {inner_path}"));
    }
    Ok(iso_parse_directory(&source, &info, &entry)?
        .into_iter()
        .map(IsoEntryInfo::from)
        .collect())
}

pub fn iso_file_chunks(
    path: &Path,
    inner_path: &str,
    chunk_size: usize,
) -> Result<IsoFileChunks, String> {
    if chunk_size == 0 || !chunk_size.is_multiple_of(2048) {
        return Err("ISO chunk size must be a non-zero multiple of 2048".into());
    }
    let source = IsoSource::open(&path.to_string_lossy())?;
    let info = parse_iso_info(&path.to_string_lossy(), &source)?;
    let entry = iso_lookup(&source, &info, inner_path)?;
    if entry.is_directory {
        return Err(format!("ISO entry is a directory: {inner_path}"));
    }
    if entry.is_multi_extent {
        return Err(format!("ISO entry is multi-extent: {inner_path}"));
    }
    let offset = iso_data_offset(&info, &entry)? as u64;
    Ok(IsoFileChunks {
        source,
        offset,
        remaining: entry.size as u64,
        chunk_size,
    })
}

impl From<IsoEntry> for IsoEntryInfo {
    fn from(value: IsoEntry) -> Self {
        Self {
            name: value.name,
            is_directory: value.is_directory,
            logical_block_address: value.logical_block_address,
            size: value.size,
            extended_attribute_blocks: value.extended_attribute_blocks,
            is_multi_extent: value.is_multi_extent,
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct IsoInfo {
    sector_size: i32,
    root_logical_block_address: u32,
    root_size: u32,
    volume_identifier: String,
}

fn iso_info(request: Value) -> Result<Value, String> {
    let path = request.as_str().ok_or("Expected ISO path")?;
    let bytes = IsoSource::open(path)?;
    let info = parse_iso_info(path, &bytes)?;
    serde_json::to_value(info).map_err(|error| error.to_string())
}

fn iso_list(request: Value) -> Result<Value, String> {
    let (path, inner_path) = iso_request(&request)?;
    let bytes = IsoSource::open(path)?;
    let info = parse_iso_info(path, &bytes)?;
    let Ok(entry) = iso_lookup(&bytes, &info, inner_path) else {
        return Ok(json!([]));
    };
    if !entry.is_directory {
        return Ok(json!([]));
    }
    Ok(json!(iso_parse_directory(&bytes, &info, &entry)?))
}

fn iso_entry(request: Value) -> Result<Value, String> {
    let (path, inner_path) = iso_request(&request)?;
    let bytes = IsoSource::open(path)?;
    let info = parse_iso_info(path, &bytes)?;
    let entry = iso_lookup(&bytes, &info, inner_path).ok();
    serde_json::to_value(entry).map_err(|error| error.to_string())
}

fn iso_all_paths(request: Value) -> Result<Value, String> {
    let (path, inner_path) = iso_request(&request)?;
    let bytes = IsoSource::open(path)?;
    let info = parse_iso_info(path, &bytes)?;
    let Ok(root) = iso_lookup(&bytes, &info, inner_path) else {
        return Ok(json!([]));
    };
    let parts = iso_split_path(inner_path);
    let prefix = if parts.is_empty() {
        String::new()
    } else {
        format!("/{}", parts.join("/"))
    };
    let mut output = Vec::new();
    if !root.is_directory {
        output.push(prefix);
    } else {
        iso_recurse_paths(&bytes, &info, &root, &prefix, &mut output)?;
    }
    Ok(json!(output))
}

fn iso_data_lba(request: Value) -> Result<Value, String> {
    let (path, inner_path) = iso_request(&request)?;
    let bytes = IsoSource::open(path)?;
    let info = parse_iso_info(path, &bytes)?;
    let entry = iso_lookup(&bytes, &info, inner_path)?;
    Ok(json!(
        u64::from(entry.logical_block_address) + u64::from(entry.extended_attribute_blocks)
    ))
}

fn iso_read_file(request: Value) -> Result<Value, String> {
    let (path, inner_path) = iso_request(&request)?;
    let bytes = IsoSource::open(path)?;
    let info = parse_iso_info(path, &bytes)?;
    let entry = iso_lookup(&bytes, &info, inner_path)?;
    if entry.is_directory {
        return Err(format!("{inner_path} is a directory, not a file"));
    }
    if entry.is_multi_extent {
        return Err(format!("{inner_path} is a multi-extent file"));
    }
    let offset = iso_data_offset(&info, &entry)?;
    Ok(json!({"DataHex": hex_encode(&bytes.read_at(offset as u64, entry.size as usize)?)}))
}

fn iso_extract(request: Value) -> Result<Value, String> {
    let object = request.as_object().ok_or("Expected ISO extract request")?;
    let path = object
        .get("Path")
        .and_then(Value::as_str)
        .ok_or("Missing ISO path")?;
    let inner_path = object
        .get("InnerPath")
        .and_then(Value::as_str)
        .unwrap_or("");
    let destination = object
        .get("Destination")
        .and_then(Value::as_str)
        .ok_or("Missing extraction destination")?;
    let bytes = IsoSource::open(path)?;
    let info = parse_iso_info(path, &bytes)?;
    let Some(entry) = iso_lookup(&bytes, &info, inner_path).ok() else {
        return Ok(json!({"Extracted":false}));
    };
    if entry.is_directory {
        fs::create_dir_all(destination).map_err(|error| format!("{destination}: {error}"))?;
        iso_extract_directory(&bytes, &info, &entry, destination)?;
    } else {
        let parent = Path::new(destination).parent();
        if let Some(parent) = parent
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        iso_extract_file(&bytes, &info, &entry, Path::new(destination))?;
    }
    Ok(json!({"Extracted":true}))
}

fn iso_extract_directory(
    bytes: &IsoSource,
    info: &IsoInfo,
    directory: &IsoEntry,
    destination: &str,
) -> Result<(), String> {
    for child in iso_parse_directory(bytes, info, directory)? {
        let output = Path::new(destination).join(&child.name);
        if child.is_directory {
            fs::create_dir_all(&output)
                .map_err(|error| format!("{}: {error}", output.display()))?;
            iso_extract_directory(bytes, info, &child, &output.to_string_lossy())?;
        } else {
            iso_extract_file(bytes, info, &child, &output)?;
        }
    }
    Ok(())
}

fn iso_extract_file(
    bytes: &IsoSource,
    info: &IsoInfo,
    entry: &IsoEntry,
    destination: &Path,
) -> Result<(), String> {
    if entry.is_multi_extent {
        return Err(format!("{} is a multi-extent file", entry.name));
    }
    let offset = iso_data_offset(info, entry)?;
    bytes.extract(offset as u64, entry.size, destination)
}

fn iso_request(request: &Value) -> Result<(&str, &str), String> {
    let object = request.as_object().ok_or("Expected ISO request object")?;
    let path = object
        .get("Path")
        .and_then(Value::as_str)
        .ok_or("Missing ISO path")?;
    let inner_path = object
        .get("InnerPath")
        .and_then(Value::as_str)
        .unwrap_or("");
    Ok((path, inner_path))
}

fn parse_iso_info(path: &str, bytes: &IsoSource) -> Result<IsoInfo, String> {
    const DEFAULT_SECTOR: usize = 2048;
    for index in 0..32usize {
        let start = (16 + index)
            .checked_mul(DEFAULT_SECTOR)
            .ok_or("ISO descriptor offset overflow")?;
        let sector = bytes.read_at(start as u64, DEFAULT_SECTOR)?;
        if sector.len() < 7 || &sector[1..6] != b"CD001" {
            if index == 0 {
                continue;
            }
            break;
        }
        let descriptor_type = sector[0];
        if descriptor_type == 0xff {
            break;
        }
        if descriptor_type != 1 {
            continue;
        }
        if sector.len() < 190 {
            return Err("ISO primary volume root entry is truncated.".into());
        }
        let mut sector_size = u16_le(&sector[128..130])? as i32;
        if sector_size == 0 {
            sector_size = DEFAULT_SECTOR as i32;
        }
        let root = &sector[156..190];
        return Ok(IsoInfo {
            sector_size,
            root_logical_block_address: u32_le(&root[2..6])?,
            root_size: u32_le(&root[10..14])?,
            volume_identifier: sector[40..72]
                .iter()
                .map(|&c| if c.is_ascii() { c as char } else { '?' })
                .collect::<String>()
                .trim_end_matches([' ', '\0'])
                .to_owned(),
        });
    }
    Err(format!(
        "No ISO9660 Primary Volume Descriptor found in {path} (sectors 16..47)."
    ))
}

fn iso_parse_directory(
    bytes: &IsoSource,
    info: &IsoInfo,
    entry: &IsoEntry,
) -> Result<Vec<IsoEntry>, String> {
    let offset = iso_data_offset(info, entry)?;
    let size = entry.size as usize;
    let blob = bytes.read_at(offset as u64, size)?;
    let mut result = Vec::new();
    let mut cursor = 0usize;
    while cursor < blob.len() {
        let length = blob[cursor] as usize;
        if length == 0 {
            let next = ((cursor / info.sector_size as usize) + 1)
                .checked_mul(info.sector_size as usize)
                .ok_or("ISO directory sector overflow")?;
            if next <= cursor {
                break;
            }
            cursor = next;
            continue;
        }
        let record_end = cursor
            .checked_add(length)
            .ok_or("ISO directory record overflow")?;
        if record_end > blob.len() {
            break;
        }
        let record = &blob[cursor..record_end];
        cursor = record_end;
        if record.len() < 33 {
            continue;
        }
        let name_length = record[32] as usize;
        let name_end = 33usize
            .checked_add(name_length)
            .ok_or("ISO name length overflow")?;
        if name_end > record.len() || (name_length == 1 && matches!(record[33], 0 | 1)) {
            continue;
        }
        let raw_name = &record[33..name_end];
        if !raw_name.is_ascii() {
            continue;
        }
        let name = strip_iso_version(std::str::from_utf8(raw_name).unwrap_or_default());
        if name.is_empty() {
            continue;
        }
        let flags = record[25];
        result.push(IsoEntry {
            name,
            is_directory: flags & 0x02 != 0,
            logical_block_address: u32_le(&record[2..6])?,
            size: u32_le(&record[10..14])?,
            extended_attribute_blocks: record[1],
            is_multi_extent: flags & 0x80 != 0,
        });
    }
    Ok(result)
}

fn iso_lookup(bytes: &IsoSource, info: &IsoInfo, inner_path: &str) -> Result<IsoEntry, String> {
    let parts = iso_split_path(inner_path);
    let mut current = IsoEntry {
        name: String::new(),
        is_directory: true,
        logical_block_address: info.root_logical_block_address,
        size: info.root_size,
        extended_attribute_blocks: 0,
        is_multi_extent: false,
    };
    for (index, part) in parts.iter().enumerate() {
        let found = iso_parse_directory(bytes, info, &current)?
            .into_iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(part));
        let Some(found) = found else {
            return Err(format!("ISO entry not found: {inner_path}"));
        };
        if index + 1 < parts.len() && !found.is_directory {
            return Err(format!("ISO path component is not a directory: {part}"));
        }
        current = found;
    }
    Ok(current)
}

fn iso_recurse_paths(
    bytes: &IsoSource,
    info: &IsoInfo,
    parent: &IsoEntry,
    base: &str,
    output: &mut Vec<String>,
) -> Result<(), String> {
    for child in iso_parse_directory(bytes, info, parent)? {
        let path = format!("{base}/{}", child.name);
        output.push(path.clone());
        if child.is_directory {
            iso_recurse_paths(bytes, info, &child, &path, output)?;
        }
    }
    Ok(())
}

fn iso_split_path(value: &str) -> Vec<&str> {
    value
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect()
}

fn iso_data_offset(info: &IsoInfo, entry: &IsoEntry) -> Result<usize, String> {
    let lba = u64::from(entry.logical_block_address)
        .checked_add(u64::from(entry.extended_attribute_blocks))
        .ok_or("ISO logical block address overflow")?;
    lba.checked_mul(info.sector_size as u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or("ISO data offset overflow".into())
}

fn strip_iso_version(value: &str) -> String {
    let Some(index) = value.rfind(';') else {
        return value.to_owned();
    };
    if index + 1 >= value.len() || !value[index + 1..].chars().all(|c| c.is_ascii_digit()) {
        value.to_owned()
    } else {
        value[..index].to_owned()
    }
}

fn strip_version(request: Value) -> Result<Value, String> {
    let value = request.as_str().ok_or("Expected ISO file name")?;
    let Some(index) = value.rfind(';') else {
        return Ok(Value::String(value.to_owned()));
    };
    if index + 1 >= value.len()
        || !value[index + 1..]
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Ok(Value::String(value.to_owned()));
    }
    Ok(Value::String(value[..index].to_owned()))
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct WavLayout {
    pub container_bits: i32,
    pub valid_bits: i32,
    pub channels: i32,
    pub sample_rate: i32,
    pub data_offset: i64,
    pub data_size: i64,
    pub bytes_per_sample: i32,
    pub channel_mask: u32,
}

const PCM_GUID: [u8; 16] = [1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113];

fn wav_layout(request: Value) -> Result<Value, String> {
    let path = request.as_str().ok_or("Expected WAVE path")?;
    let layout = read_wav_layout(path)?;
    serde_json::to_value(layout).map_err(|error| error.to_string())
}

pub fn read_wav_layout(path: &str) -> Result<WavLayout, String> {
    let mut input = crate::identity::open_read(path).map_err(|e| format!("{path}: {e}"))?;
    read_wav_layout_from(&mut input).map_err(|e| e.to_string())
}

fn wav_normalize(request: Value) -> Result<Value, String> {
    let job = serde_json::from_value(request).map_err(|e| e.to_string())?;
    let written = crate::pcm::normalize(job, &mut crate::pcm::NoEvents).map_err(|e| e.message)?;
    Ok(json!({"WrittenBytes":written}))
}

#[derive(Debug)]
pub enum WavError {
    Io(std::io::Error),
    Invalid(String),
    Overflow,
}
impl From<std::io::Error> for WavError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<String> for WavError {
    fn from(e: String) -> Self {
        Self::Invalid(e)
    }
}
impl From<&str> for WavError {
    fn from(e: &str) -> Self {
        Self::Invalid(e.into())
    }
}
impl std::fmt::Display for WavError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Invalid(e) => e.fmt(f),
            Self::Overflow => f.write_str("WAVE sample rate is too large"),
        }
    }
}
impl std::error::Error for WavError {}

pub fn read_wav_layout_from(input: &mut (impl Read + Seek)) -> Result<WavLayout, WavError> {
    let length = input.seek(SeekFrom::End(0))?;
    input.seek(SeekFrom::Start(0))?;
    let mut head = [0u8; 12];
    input.read_exact(&mut head)?;
    if &head[..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return Err("WAVE input must be an integer RIFF/WAVE PCM stream.".into());
    }
    let riff_size = u32_le(&head[4..8])? as u64;
    let end = riff_size.checked_add(8).ok_or("WAVE length overflow")?;
    if end > length || end < 12 {
        return Err("WAVE length field is invalid or the file is truncated.".into());
    }
    let file_length = length;
    let mut position = 12u64;
    let mut bits = 0i32;
    let mut valid = 0i32;
    let mut channels = 0i32;
    let mut rate = 0i32;
    let mut align = 0i32;
    let mut mask = 0u32;
    let mut data_offset = None;
    let mut data_size = 0i64;
    while position + 8 <= end {
        input.seek(SeekFrom::Start(position))?;
        let mut chunk = [0u8; 8];
        input.read_exact(&mut chunk)?;
        let id = &chunk[..4];
        let length = u32_le(&chunk[4..])? as u64;
        let payload = position + 8;
        let payload_end = payload
            .checked_add(length)
            .ok_or("WAVE chunk length overflow")?;
        let mut next = payload_end
            .checked_add(length & 1)
            .ok_or("WAVE chunk alignment overflow")?;
        if length & 1 != 0 && id == b"data" && payload_end == end && end == file_length {
            // eac3to omits the RIFF alignment byte after an odd terminal data chunk.
            next = end;
        }
        if payload_end > end || next > end {
            return Err("WAVE chunk exceeds the file boundary.".into());
        }
        if id == b"fmt " {
            if bits != 0 || !(16..=4096).contains(&length) {
                return Err("WAVE fmt chunk is invalid.".into());
            }
            let mut fmt = vec![0u8; length as usize];
            input.read_exact(&mut fmt)?;
            let tag = u16_le(&fmt[0..2])?;
            channels = u16_le(&fmt[2..4])? as i32;
            rate = u32_le(&fmt[4..8])?
                .try_into()
                .map_err(|_| WavError::Overflow)?;
            align = u16_le(&fmt[12..14])? as i32;
            bits = u16_le(&fmt[14..16])? as i32;
            valid = bits;
            if tag == 0xfffe {
                if length < 40 || u16_le(&fmt[16..18])? < 22 || fmt[24..40] != PCM_GUID {
                    return Err("WAVE extensible format is not integer PCM.".into());
                }
                valid = u16_le(&fmt[18..20])? as i32;
                mask = u32_le(&fmt[20..24])?;
                if valid == 0 {
                    valid = bits;
                }
            } else if tag != 1 {
                return Err("Compressed or floating-point WAVE input is not supported.".into());
            }
        } else if id == b"data" {
            if data_offset.is_some() {
                return Err("WAVE contains duplicate data chunks.".into());
            }
            data_offset = Some(payload as i64);
            data_size = length as i64;
        }
        position = next;
    }
    if position != end
        || !matches!(bits, 16 | 24 | 32)
        || valid < 1
        || valid > bits
        || !(1..=6).contains(&channels)
        || align != channels * (bits / 8)
        || data_offset.is_none()
        || data_size <= 0
        || data_size % i64::from(align) != 0
        || !matches!(rate, 44_100 | 48_000 | 88_200 | 96_000 | 176_400 | 192_000)
        || (rate > 96_000 && channels > 2)
    {
        return Err("WAVE PCM parameters or frame boundary are not DVD-Audio compatible.".into());
    }
    if mask == 0 {
        mask = match channels {
            1 => 4,
            2 => 3,
            3 => 7,
            4 => 0x33,
            5 => 0x37,
            6 => 0x3f,
            _ => 0,
        };
    }
    if mask.count_ones() != channels as u32 {
        return Err("WAVE channel mask does not match the channel count.".into());
    }
    Ok(WavLayout {
        container_bits: bits,
        valid_bits: valid,
        channels,
        sample_rate: rate,
        data_offset: data_offset.unwrap_or_default(),
        data_size,
        bytes_per_sample: bits / 8,
        channel_mask: mask,
    })
}

fn flac_comments(request: Value) -> Result<Value, String> {
    let path = request.as_str().ok_or("Expected FLAC path")?;
    let document = read_flac(path)?;
    let block = document
        .iter()
        .find(|(kind, _)| *kind == 4)
        .map(|(_, data)| data.as_slice());
    let Some(data) = block else {
        return Ok(json!([]));
    };
    let mut reader = FlacReader::new(data);
    let vendor_length = reader.u32_le()?;
    reader.bytes(vendor_length)?;
    let count = reader.u32_le()? as usize;
    if count > i32::MAX as usize {
        return Err("FLAC Vorbis comment count is too large.".into());
    }
    let mut comments = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let length = reader.u32_le()?;
        let value = reader.text(length)?;
        if let Some((key, value)) = value.split_once('=')
            && !key.is_empty()
        {
            comments.push(json!({"Key":key,"Value":value}));
        }
    }
    reader.require_end()?;
    Ok(Value::Array(comments))
}

fn flac_picture(request: Value) -> Result<Value, String> {
    let path = request.as_str().ok_or("Expected FLAC path")?;
    let document = read_flac(path)?;
    let Some((_, data)) = document.iter().find(|(kind, _)| *kind == 6) else {
        return Ok(Value::Null);
    };
    let mut reader = FlacReader::new(data);
    let picture_type = reader.u32_be()?;
    let mime_length = reader.u32_be()?;
    let mime = reader.text(mime_length)?;
    let description_length = reader.u32_be()?;
    let description = reader.text(description_length)?;
    let width = reader.u32_be()?;
    let height = reader.u32_be()?;
    let depth = reader.u32_be()?;
    let colors = reader.u32_be()?;
    let image_length = reader.u32_be()?;
    let image = reader.bytes(image_length)?;
    reader.require_end()?;
    if picture_type > i32::MAX as u32
        || width > i32::MAX as u32
        || height > i32::MAX as u32
        || depth > i32::MAX as u32
        || colors > i32::MAX as u32
    {
        return Err("FLAC PICTURE values exceed the supported range.".into());
    }
    Ok(json!({
        "Descriptor": {
            "Type": picture_type,
            "MimeType": mime,
            "Width": width,
            "Height": height,
            "Depth": depth,
        },
        "Description": description,
        "Colors": colors,
        "ImageHex": hex_encode(image),
    }))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct FlacReplaceRequest {
    path: String,
    image_path: String,
    template: FlacPictureTemplate,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct FlacPictureTemplate {
    descriptor: FlacPictureDescriptor,
    description: String,
    colors: i32,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct FlacPictureDescriptor {
    #[serde(rename = "Type")]
    picture_type: i32,
    mime_type: String,
    width: i32,
    height: i32,
    depth: i32,
}

fn flac_replace_picture(request: Value) -> Result<Value, String> {
    let request: FlacReplaceRequest =
        serde_json::from_value(request).map_err(|error| error.to_string())?;
    let source = fs::read(&request.path).map_err(|error| format!("{}: {error}", request.path))?;
    let (blocks, audio_offset) = read_flac_document(&source)?;
    if !blocks.iter().any(|(kind, _)| *kind == 6) {
        return Err("FLAC file does not contain a PICTURE block.".into());
    }
    let image = fs::read(&request.image_path)
        .map_err(|error| format!("{}: {error}", request.image_path))?;
    let replacement = build_picture_payload(&request.template, &image)?;
    let first_picture = blocks
        .iter()
        .position(|(kind, _)| *kind == 6)
        .ok_or("FLAC PICTURE block is missing")?;
    let mut output_blocks = Vec::with_capacity(blocks.len());
    for (index, (kind, data)) in blocks.into_iter().enumerate() {
        if kind != 6 {
            output_blocks.push((kind, data));
        }
        if index == first_picture {
            output_blocks.push((6, replacement.clone()));
        }
    }
    let mut output = Vec::with_capacity(source.len() + replacement.len());
    output.extend_from_slice(b"fLaC");
    for (index, (kind, data)) in output_blocks.iter().enumerate() {
        if *kind > 127 || data.len() > 0x00ff_ffff {
            return Err("FLAC metadata block is too large or has an invalid type.".into());
        }
        output.push(
            *kind
                | if index + 1 == output_blocks.len() {
                    0x80
                } else {
                    0
                },
        );
        output.push((data.len() >> 16) as u8);
        output.push((data.len() >> 8) as u8);
        output.push(data.len() as u8);
        output.extend_from_slice(data);
    }
    if audio_offset > source.len() {
        return Err("FLAC audio offset is outside the file.".into());
    }
    output.extend_from_slice(&source[audio_offset..]);
    let temporary = format!("{}.{}.tmp", request.path, unique_suffix());
    // Own the temporary before arranging cleanup, and never truncate a stale
    // file from another attempt. A write/flush/rename failure preserves input.
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("{temporary}: {error}"))?;
    let result = (|| {
        file.write_all(&output)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &request.path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("{}: {error}", request.path));
    }
    Ok(json!({"WrittenBytes": output.len()}))
}

fn build_picture_payload(template: &FlacPictureTemplate, image: &[u8]) -> Result<Vec<u8>, String> {
    let descriptor = &template.descriptor;
    if descriptor.picture_type < 0
        || descriptor.width < 0
        || descriptor.height < 0
        || descriptor.depth < 0
        || template.colors < 0
    {
        return Err("FLAC PICTURE values are invalid.".into());
    }
    let mime = descriptor.mime_type.as_bytes();
    let description = template.description.as_bytes();
    let total = 4usize
        .checked_add(4 + mime.len())
        .and_then(|value| value.checked_add(4 + description.len()))
        .and_then(|value| value.checked_add(4 * 5))
        .and_then(|value| value.checked_add(image.len()))
        .ok_or("FLAC PICTURE payload is too large")?;
    let mut output = Vec::with_capacity(total);
    push_u32_be(&mut output, descriptor.picture_type as u32);
    push_u32_be(
        &mut output,
        mime.len().try_into().map_err(|_| "MIME is too large")?,
    );
    output.extend_from_slice(mime);
    push_u32_be(
        &mut output,
        description
            .len()
            .try_into()
            .map_err(|_| "Description is too large")?,
    );
    output.extend_from_slice(description);
    push_u32_be(&mut output, descriptor.width as u32);
    push_u32_be(&mut output, descriptor.height as u32);
    push_u32_be(&mut output, descriptor.depth as u32);
    push_u32_be(&mut output, template.colors as u32);
    push_u32_be(
        &mut output,
        image.len().try_into().map_err(|_| "Image is too large")?,
    );
    output.extend_from_slice(image);
    Ok(output)
}

fn push_u32_be(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn unique_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{value:x}")
}

fn alac_cookie(request: Value) -> Result<Value, String> {
    let object = request.as_object().ok_or("Expected ALAC cookie request")?;
    let hex = object
        .get("DataHex")
        .and_then(Value::as_str)
        .ok_or("Missing ALAC cookie bytes")?;
    let offset = i32::try_from(object.get("Offset").and_then(Value::as_i64).unwrap_or(0))
        .map_err(|_| "ALAC cookie offset overflow")?;
    Ok(json!(crate::preparation::alac::parse_cookie(
        &hex_decode(hex)?,
        offset
    )))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AlacFindRequest {
    path: String,
    cookie: crate::preparation::alac::Cookie,
    packets: Vec<crate::preparation::alac::Packet>,
}

fn alac_find_bad_frames(request: Value) -> Result<Value, String> {
    let request: AlacFindRequest =
        serde_json::from_value(request).map_err(|error| error.to_string())?;
    let data = fs::read(&request.path).map_err(|error| format!("{}: {error}", request.path))?;
    crate::preparation::alac::find_patches(&data, &request.cookie, &request.packets)
        .map(|patches| json!(patches))
        .map_err(|error| error.message)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AlacApplyRequest {
    source: String,
    destination: String,
    patches: Vec<AlacPatch>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AlacPatch {
    position: i64,
    end_bit: i64,
}

fn alac_apply_patches(request: Value) -> Result<Value, String> {
    let request: AlacApplyRequest =
        serde_json::from_value(request).map_err(|error| error.to_string())?;
    let mut data =
        fs::read(&request.source).map_err(|error| format!("{}: {error}", request.source))?;
    crate::preparation::alac::apply_bits(
        &mut data,
        request.patches.iter().map(|p| (p.position, p.end_bit)),
    )
    .map_err(|error| error.message)?;
    let parent = Path::new(&request.destination).parent();
    if let Some(parent) = parent
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::write(&request.destination, &data)
        .map_err(|error| format!("{}: {error}", request.destination))?;
    Ok(json!({"WrittenBytes": data.len()}))
}

fn read_flac(path: &str) -> Result<Vec<(u8, Vec<u8>)>, String> {
    let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
    read_flac_document(&bytes).map(|(blocks, _)| blocks)
}

type FlacBlocks = Vec<(u8, Vec<u8>)>;

fn read_flac_document(bytes: &[u8]) -> Result<(FlacBlocks, usize), String> {
    if bytes.len() < 4 || &bytes[..4] != b"fLaC" {
        return Err("Not a FLAC stream.".into());
    }
    let mut offset = 4usize;
    let mut blocks = Vec::new();
    let mut last = false;
    while !last {
        if offset + 4 > bytes.len() {
            return Err("FLAC metadata header is truncated.".into());
        }
        let header = bytes[offset];
        last = header & 0x80 != 0;
        let kind = header & 0x7f;
        let length = ((bytes[offset + 1] as usize) << 16)
            | ((bytes[offset + 2] as usize) << 8)
            | bytes[offset + 3] as usize;
        offset += 4;
        let end = offset
            .checked_add(length)
            .ok_or("FLAC metadata length overflow")?;
        if end > bytes.len() {
            return Err("FLAC metadata block is truncated.".into());
        }
        blocks.push((kind, bytes[offset..end].to_vec()));
        offset = end;
    }
    if blocks
        .first()
        .is_none_or(|(kind, data)| *kind != 0 || data.len() != 34)
    {
        return Err("FLAC STREAMINFO block is missing or invalid.".into());
    }
    Ok((blocks, offset))
}

struct FlacReader<'a> {
    data: &'a [u8],
    offset: usize,
}
impl<'a> FlacReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }
    fn bytes(&mut self, length: u32) -> Result<&'a [u8], String> {
        if length > i32::MAX as u32 {
            return Err("FLAC metadata length is too large.".into());
        }
        let end = self
            .offset
            .checked_add(length as usize)
            .ok_or("FLAC metadata length overflow")?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or("FLAC metadata block is truncated.")?;
        self.offset = end;
        Ok(bytes)
    }
    fn u32_le(&mut self) -> Result<u32, String> {
        u32_le(self.bytes(4)?)
    }
    fn u32_be(&mut self) -> Result<u32, String> {
        u32_be(self.bytes(4)?)
    }
    fn text(&mut self, length: u32) -> Result<&'a str, String> {
        std::str::from_utf8(self.bytes(length)?)
            .map_err(|error| format!("Invalid FLAC UTF-8: {error}"))
    }
    fn require_end(&self) -> Result<(), String> {
        if self.offset == self.data.len() {
            Ok(())
        } else {
            Err("FLAC metadata block has trailing bytes.".into())
        }
    }
}

fn u16_le(data: &[u8]) -> Result<u16, String> {
    let bytes: [u8; 2] = data.try_into().map_err(|_| "Truncated 16-bit value")?;
    Ok(u16::from_le_bytes(bytes))
}
fn u32_le(data: &[u8]) -> Result<u32, String> {
    let bytes: [u8; 4] = data.try_into().map_err(|_| "Truncated 32-bit value")?;
    Ok(u32::from_le_bytes(bytes))
}
fn u32_be(data: &[u8]) -> Result<u32, String> {
    let bytes: [u8; 4] = data.try_into().map_err(|_| "Truncated 32-bit value")?;
    Ok(u32::from_be_bytes(bytes))
}
fn hex_encode(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(data.len() * 2);
    for byte in data {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 15) as usize] as char);
    }
    result
}
fn hex_decode(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("Hex input has an odd length".into());
    }
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len() / 2);
    for index in (0..bytes.len()).step_by(2) {
        let high = hex_digit(bytes[index]).ok_or("Invalid hex input")?;
        let low = hex_digit(bytes[index + 1]).ok_or("Invalid hex input")?;
        output.push((high << 4) | low);
    }
    Ok(output)
}
fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[allow(dead_code)]
fn _path_is_file(path: &str) -> bool {
    Path::new(path).is_file()
}
