use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::{Read, Seek, SeekFrom, Write},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct IsoSource {
    file: RefCell<fs::File>,
    length: u64,
}

const ISO_SECTOR_SIZE: u64 = 2048;
const ISO_SECTOR_USIZE: usize = 2048;
const ISO_SECTOR_U16: u16 = 2048;

fn read_sector(input: &mut (impl Read + Seek), lba: u64) -> Result<[u8; 2048], String> {
    let offset = lba
        .checked_mul(ISO_SECTOR_SIZE)
        .ok_or("ISO_SECTOR_OUT_OF_RANGE")?;
    input
        .seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    let mut sector = [0; ISO_SECTOR_USIZE];
    input
        .read_exact(&mut sector)
        .map_err(|error| format!("ISO_SECTOR_OUT_OF_RANGE: {error}"))?;
    Ok(sector)
}

fn little_u32(data: &[u8]) -> u32 {
    u32::from_le_bytes(data[..4].try_into().unwrap())
}

fn udf_crc16(data: &[u8]) -> u16 {
    data.iter().fold(0u16, |mut crc, byte| {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
        crc
    })
}

fn udf_tag_valid(descriptor: &[u8], expected_id: u16, expected_location: u32) -> bool {
    if descriptor.len() < 16
        || u16::from_le_bytes(descriptor[..2].try_into().unwrap()) != expected_id
        || little_u32(&descriptor[12..16]) != expected_location
    {
        return false;
    }
    let checksum = descriptor[..4]
        .iter()
        .chain(&descriptor[5..16])
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    let crc_length = u16::from_le_bytes(descriptor[10..12].try_into().unwrap()) as usize;
    crc_length <= descriptor.len() - 16
        && checksum == descriptor[4]
        && udf_crc16(&descriptor[16..16 + crc_length])
            == u16::from_le_bytes(descriptor[8..10].try_into().unwrap())
}

fn require_udf_tag(
    input: &mut (impl Read + Seek),
    lba: u64,
    expected_id: u16,
    expected_location: u32,
) -> Result<[u8; 2048], String> {
    let descriptor = read_sector(input, lba)?;
    if !udf_tag_valid(&descriptor, expected_id, expected_location) {
        return Err(format!("UDF_DESCRIPTOR_INVALID: LBA {lba}"));
    }
    Ok(descriptor)
}

fn udf_location(lba: u64) -> Result<u32, String> {
    u32::try_from(lba).map_err(|_| format!("UDF_DESCRIPTOR_OUT_OF_RANGE: LBA {lba}"))
}

#[allow(
    clippy::too_many_lines,
    reason = "ISO/UDF validation keeps descriptor ordering and extent checks in one fail-fast sequence"
)]
fn verify_dvd_audio_filesystem_reader(
    input: &mut (impl Read + Seek),
    byte_length: u64,
) -> Result<(), String> {
    if !byte_length.is_multiple_of(ISO_SECTOR_SIZE) || byte_length / ISO_SECTOR_SIZE <= 256 {
        return Err("ISO_SECTOR_ALIGNMENT: image is not a complete 2048-byte-sector volume".into());
    }
    let sectors = byte_length / ISO_SECTOR_SIZE;
    let pvd = read_sector(input, 16)?;
    if pvd[0] != 1 || &pvd[1..6] != b"CD001" || pvd[6] != 1 {
        return Err("ISO_PVD_INVALID: primary volume descriptor is missing or malformed".into());
    }
    if u64::from(u32::from_le_bytes(pvd[80..84].try_into().unwrap())) != sectors
        || u64::from(u32::from_be_bytes(pvd[84..88].try_into().unwrap())) != sectors
    {
        return Err("ISO_VOLUME_SPACE_MISMATCH: PVD size does not match the image".into());
    }
    if u16::from_le_bytes(pvd[128..130].try_into().unwrap()) != ISO_SECTOR_U16
        || u16::from_be_bytes(pvd[130..132].try_into().unwrap()) != ISO_SECTOR_U16
    {
        return Err("ISO_BLOCK_SIZE_INVALID: PVD logical block size is not 2048".into());
    }
    let root = &pvd[156..190];
    if root[0] != 34
        || root[25] != 2
        || u16::from_le_bytes(root[28..30].try_into().unwrap()) != 1
        || u16::from_be_bytes(root[30..32].try_into().unwrap()) != 1
        || pvd[881] != 1
    {
        return Err("ISO_ROOT_RECORD_INVALID: PVD root directory fields are malformed".into());
    }
    let terminator = read_sector(input, 17)?;
    if terminator[0] != 255 || &terminator[1..6] != b"CD001" || terminator[6] != 1 {
        return Err("ISO_DESCRIPTOR_TERMINATOR_MISSING".into());
    }
    for (lba, identifier) in [(18, b"BEA01".as_slice()), (19, b"NSR02"), (20, b"TEA01")] {
        let descriptor = read_sector(input, lba)?;
        if descriptor[0] != 0 || &descriptor[1..6] != identifier {
            return Err(format!("UDF_RECOGNITION_INVALID: sector {lba}"));
        }
    }
    let anchor = require_udf_tag(input, 256, 2, 256)
        .map_err(|_| "UDF_ANCHOR_MISSING: invalid anchor descriptor at sector 256".to_owned())?;
    let mut descriptor_sequences = Vec::new();
    for (length_at, location_at) in [(16, 20), (24, 28)] {
        let bytes = u64::from(little_u32(&anchor[length_at..length_at + 4]));
        let location = u64::from(little_u32(&anchor[location_at..location_at + 4]));
        let extent_sectors = bytes.div_ceil(ISO_SECTOR_SIZE);
        if bytes < 6 * ISO_SECTOR_SIZE
            || !bytes.is_multiple_of(ISO_SECTOR_SIZE)
            || location < 16
            || location
                .checked_add(extent_sectors)
                .is_none_or(|end| end > sectors)
        {
            return Err(
                "UDF_VDS_EXTENT_INVALID: anchor descriptor sequence is outside the image".into(),
            );
        }
        descriptor_sequences.push((location, extent_sectors));
    }
    let main_vds = descriptor_sequences[0].0;
    let main_vds_end = main_vds + descriptor_sequences[0].1;
    let reserve_vds = descriptor_sequences[1].0;
    let reserve_vds_end = reserve_vds + descriptor_sequences[1].1;
    if (main_vds < reserve_vds_end && reserve_vds < main_vds_end)
        || main_vds_end > sectors
        || reserve_vds_end > sectors
    {
        return Err("UDF_VDS_EXTENT_INVALID: primary and reserve sequences overlap".into());
    }
    let expected_ids = [1u16, 4, 5, 6, 7, 8];
    let mut main_descriptors = Vec::with_capacity(expected_ids.len());
    for (index, expected_id) in expected_ids.into_iter().enumerate() {
        let lba = main_vds + index as u64;
        main_descriptors.push(require_udf_tag(
            input,
            lba,
            expected_id,
            udf_location(lba)?,
        )?);
    }
    for (index, expected_id) in [1u16, 4, 5, 6, 7, 8].into_iter().enumerate() {
        let lba = reserve_vds + index as u64;
        require_udf_tag(input, lba, expected_id, udf_location(lba)?)?;
    }
    let partition = &main_descriptors[2];
    let partition_start = u64::from(little_u32(&partition[188..192]));
    let partition_length = u64::from(little_u32(&partition[192..196]));
    if partition_start < 257
        || partition_length == 0
        || partition_start
            .checked_add(partition_length)
            .is_none_or(|end| end >= sectors)
    {
        return Err("UDF_PARTITION_INVALID: partition lies outside the image".into());
    }
    let integrity_bytes = u64::from(little_u32(&main_descriptors[3][432..436]));
    let integrity_lba = u64::from(little_u32(&main_descriptors[3][436..440]));
    let Some(integrity_end) = integrity_lba.checked_add(integrity_bytes.div_ceil(ISO_SECTOR_SIZE))
    else {
        return Err("UDF_INTEGRITY_SEQUENCE_INVALID: extent overflows".into());
    };
    if integrity_bytes < 2 * ISO_SECTOR_SIZE
        || !integrity_bytes.is_multiple_of(ISO_SECTOR_SIZE)
        || integrity_lba < 16
        || integrity_end > partition_start
        || (integrity_lba < main_vds_end && main_vds < integrity_end)
        || (integrity_lba < reserve_vds_end && reserve_vds < integrity_end)
    {
        return Err("UDF_INTEGRITY_SEQUENCE_INVALID: location is outside the image".into());
    }
    require_udf_tag(input, integrity_lba, 9, udf_location(integrity_lba)?)?;
    require_udf_tag(
        input,
        integrity_lba + 1,
        8,
        udf_location(integrity_lba + 1)?,
    )?;
    require_udf_tag(input, partition_start, 256, 0)?;
    require_udf_tag(input, partition_start + 1, 8, 1)?;
    let end_anchor_lba = sectors - 1;
    let end_anchor = require_udf_tag(input, end_anchor_lba, 2, udf_location(end_anchor_lba)?)?;
    if u64::from(little_u32(&end_anchor[20..24])) != main_vds
        || u64::from(little_u32(&end_anchor[28..32])) != reserve_vds
    {
        return Err("UDF_END_ANCHOR_INVALID: backup extents do not match".into());
    }
    Ok(())
}

/// Validate the ISO9660 and UDF structures required by a DVD-Audio image.
///
/// # Errors
/// Returns a diagnostic for file I/O failures, malformed descriptors, invalid extents, or inconsistent volume sizes.
pub fn verify_dvd_audio_filesystem(path: &Path) -> Result<(), String> {
    let path = path.to_string_lossy();
    let mut input = crate::identity::open_read(&path).map_err(|error| error.to_string())?;
    let length = input.metadata().map_err(|error| error.to_string())?.len();
    verify_dvd_audio_filesystem_reader(&mut input, length)
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
        let available = usize::try_from(self.length.saturating_sub(offset).min(count as u64))
            .expect("available byte count is bounded by the requested usize");
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

/// # Errors
/// Rejects unknown operations, malformed requests, invalid hex data, and
/// unrepresentable request values. Propagates the selected ISO lookup/read/
/// extraction, WAVE validation/normalization, FLAC metadata parsing/replacement,
/// or ALAC patch scanning/application error, including filesystem failures.
/// An absent FLAC picture or unrecognized ALAC cookie returns JSON null instead.
pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "iso.info" => iso_info(&request),
        "iso.list" => iso_list(&request),
        "iso.entry" => iso_entry(&request),
        "iso.all_paths" => iso_all_paths(&request),
        "iso.data_lba" => iso_data_lba(&request),
        "iso.read_file" => iso_read_file(&request),
        "iso.extract" => iso_extract(&request),
        "iso.strip_version" => strip_version(&request),
        "wav.layout" => wav_layout(&request),
        "wav.normalize" => wav_normalize(request),
        "flac.comments" => flac_comments(&request),
        "flac.picture" => flac_picture(&request),
        "flac.replace_picture" => flac_replace_picture(request),
        "alac.cookie" => alac_cookie(&request),
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

type DirectoryEntries = std::sync::Mutex<
    std::collections::HashMap<(PathBuf, String), Result<Vec<IsoEntryInfo>, String>>,
>;

#[derive(Default)]
pub struct IsoMetadataCache {
    entries: DirectoryEntries,
}

impl IsoMetadataCache {
    /// # Errors
    /// Returns the errors from [`iso_list_directory`]. Both successful listings
    /// and errors are cached for the normalized path and directory key.
    pub fn list_directory(
        &self,
        path: &Path,
        inner_path: &str,
    ) -> Result<Vec<IsoEntryInfo>, String> {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let key = (path, inner_path.to_ascii_lowercase());
        if let Some(result) = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .cloned()
        {
            return result;
        }
        let result = iso_list_directory(&key.0, inner_path);
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, result.clone());
        result
    }
}

/// A bounded, sector-aligned reader for one ISO9660 file.
pub struct IsoFileChunks {
    source: IsoSource,
    offset: u64,
    remaining: u64,
    chunk_size: usize,
}

impl IsoFileChunks {
    /// # Errors
    /// Returns an error on ISO read failure, a short read relative to the
    /// directory-declared size, or chunk-size/offset overflow.
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

/// # Errors
/// Returns an error if the ISO cannot be opened or parsed, the requested entry
/// cannot be resolved, is not a directory, or its directory records cannot be
/// read or parsed.
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

/// # Errors
/// Rejects a zero or non-sector-aligned chunk size. Propagates ISO open, parse,
/// lookup, and data-offset errors, and rejects directories and multi-extent
/// entries. File-data read failures are reported by [`IsoFileChunks::next_chunk`].
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
        remaining: u64::from(entry.size),
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

fn iso_info(request: &Value) -> Result<Value, String> {
    let path = request.as_str().ok_or("Expected ISO path")?;
    let bytes = IsoSource::open(path)?;
    let info = parse_iso_info(path, &bytes)?;
    serde_json::to_value(info).map_err(|error| error.to_string())
}

fn iso_list(request: &Value) -> Result<Value, String> {
    let (path, inner_path) = iso_request(request)?;
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

fn iso_entry(request: &Value) -> Result<Value, String> {
    let (path, inner_path) = iso_request(request)?;
    let bytes = IsoSource::open(path)?;
    let info = parse_iso_info(path, &bytes)?;
    let entry = iso_lookup(&bytes, &info, inner_path).ok();
    serde_json::to_value(entry).map_err(|error| error.to_string())
}

fn iso_all_paths(request: &Value) -> Result<Value, String> {
    let (path, inner_path) = iso_request(request)?;
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
    if root.is_directory {
        iso_recurse_paths(&bytes, &info, &root, &prefix, &mut output)?;
    } else {
        output.push(prefix);
    }
    Ok(json!(output))
}

fn iso_data_lba(request: &Value) -> Result<Value, String> {
    let (path, inner_path) = iso_request(request)?;
    let bytes = IsoSource::open(path)?;
    let info = parse_iso_info(path, &bytes)?;
    let entry = iso_lookup(&bytes, &info, inner_path)?;
    Ok(json!(
        u64::from(entry.logical_block_address) + u64::from(entry.extended_attribute_blocks)
    ))
}

fn iso_read_file(request: &Value) -> Result<Value, String> {
    let (path, inner_path) = iso_request(request)?;
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

fn iso_extract(request: &Value) -> Result<Value, String> {
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
        let mut sector_size = i32::from(u16_le(&sector[128..130])?);
        if sector_size == 0 {
            sector_size = i32::try_from(DEFAULT_SECTOR).expect("default ISO sector size fits i32");
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
            let sector_size =
                usize::try_from(info.sector_size).expect("parsed ISO sector size is positive");
            let next = ((cursor / sector_size) + 1)
                .checked_mul(sector_size)
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
    lba.checked_mul(u64::try_from(info.sector_size).expect("parsed ISO sector size is positive"))
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

fn strip_version(request: &Value) -> Result<Value, String> {
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

fn wav_layout(request: &Value) -> Result<Value, String> {
    let path = request.as_str().ok_or("Expected WAVE path")?;
    let layout = read_wav_layout(path)?;
    serde_json::to_value(layout).map_err(|error| error.to_string())
}

/// # Errors
/// Returns an error if the file cannot be opened or [`read_wav_layout_from`]
/// rejects its contents or encounters an I/O failure.
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

/// # Errors
/// Returns [`WavError::Io`] on reader I/O failure and [`WavError::Overflow`] if
/// the sample rate cannot fit in `i32`. Returns [`WavError::Invalid`] for invalid
/// integer RIFF/WAVE input, unrepresentable data sizes/offsets, empty audio,
/// unsupported DVD-Audio sample-rate/channel combinations, or a channel mask
/// whose set-bit count differs from the channel count.
///
/// # Panics
/// Supported WAVE storage encodings contain at most four bytes per sample, so
/// the internal bits-per-sample conversion expectation cannot fail.
pub fn read_wav_layout_from(input: &mut (impl Read + Seek)) -> Result<WavLayout, WavError> {
    let reader = dvda_audio::wav::Reader::new_integer_riff(input).map_err(|error| {
        if error.kind() == std::io::ErrorKind::InvalidData {
            WavError::Invalid(error.to_string())
        } else {
            WavError::Io(error)
        }
    })?;
    let format = reader.format;
    let bits = i32::try_from(format.encoding.bytes() * 8)
        .expect("supported WAVE storage width has at most 32 bits");
    let valid = i32::from(format.valid_bits);
    let channels = i32::from(format.channels);
    let rate = i32::try_from(format.sample_rate).map_err(|_| WavError::Overflow)?;
    let mut mask = format.channel_mask;
    let data_size = i64::try_from(reader.data_bytes)
        .map_err(|_| WavError::Invalid("WAVE data size exceeds the supported range".into()))?;
    let data_offset = i64::try_from(reader.data_offset)
        .map_err(|_| WavError::Invalid("WAVE data offset exceeds the supported range".into()))?;
    if !(1..=6).contains(&channels)
        || data_size <= 0
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
    if mask.count_ones() != channels.cast_unsigned() {
        return Err("WAVE channel mask does not match the channel count.".into());
    }
    Ok(WavLayout {
        container_bits: bits,
        valid_bits: valid,
        channels,
        sample_rate: rate,
        data_offset,
        data_size,
        bytes_per_sample: bits / 8,
        channel_mask: mask,
    })
}

fn flac_comments(request: &Value) -> Result<Value, String> {
    let path = request.as_str().ok_or("Expected FLAC path")?;
    let (mut file, length) = open_flac(path)?;
    flac_comments_reader(&mut file, length)
}

/// Streams the Vorbis comment block from a seekable FLAC source.
///
/// # Errors
/// Returns a diagnostic for I/O failures, malformed block chains, or invalid
/// comment data, with the same precedence as the `flac.comments` operation.
#[doc(hidden)]
pub fn flac_comments_reader<R: Read + Seek>(reader: &mut R, length: u64) -> Result<Value, String> {
    let Some(data) = read_flac_metadata(reader, length, Some(4))? else {
        return Ok(json!([]));
    };
    parse_flac_comments(&data)
}

fn parse_flac_comments(data: &[u8]) -> Result<Value, String> {
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

#[doc(hidden)]
pub struct FlacPictureData {
    pub metadata: Value,
    block: Vec<u8>,
    image_offset: usize,
}

impl FlacPictureData {
    #[must_use]
    pub fn image(&self) -> &[u8] {
        &self.block[self.image_offset..]
    }
}

#[doc(hidden)]
pub fn flac_picture_data(path: &Path) -> Result<FlacPictureData, String> {
    let path = path.to_str().ok_or("Expected FLAC path")?;
    let (mut file, length) = open_flac(path)?;
    flac_picture_data_reader(&mut file, length)
}

/// Streams the PICTURE block from a seekable FLAC source.
///
/// # Errors
/// Returns a diagnostic for I/O failures, malformed block chains, or invalid
/// picture data, with the same precedence as the `flac.picture` operation.
#[doc(hidden)]
pub fn flac_picture_data_reader<R: Read + Seek>(
    reader: &mut R,
    length: u64,
) -> Result<FlacPictureData, String> {
    let Some(block) = read_flac_metadata(reader, length, Some(6))? else {
        return Ok(FlacPictureData {
            metadata: Value::Null,
            block: Vec::new(),
            image_offset: 0,
        });
    };
    parse_flac_picture(block)
}

fn parse_flac_picture(block: Vec<u8>) -> Result<FlacPictureData, String> {
    let mut reader = FlacReader::new(&block);
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
    reader.bytes(image_length)?;
    reader.require_end()?;
    if picture_type > i32::MAX as u32
        || width > i32::MAX as u32
        || height > i32::MAX as u32
        || depth > i32::MAX as u32
        || colors > i32::MAX as u32
    {
        return Err("FLAC PICTURE values exceed the supported range.".into());
    }
    let metadata = json!({
        "Descriptor": {
            "Type": picture_type,
            "MimeType": mime,
            "Width": width,
            "Height": height,
            "Depth": depth,
        },
        "Description": description,
        "Colors": colors,
    });
    Ok(FlacPictureData {
        metadata,
        image_offset: block.len() - image_length as usize,
        block,
    })
}

fn flac_picture(request: &Value) -> Result<Value, String> {
    let path = request.as_str().ok_or("Expected FLAC path")?;
    Ok(flac_picture_json(flac_picture_data(Path::new(path))?))
}

fn flac_picture_json(data: FlacPictureData) -> Value {
    if data.metadata.is_null() {
        return Value::Null;
    }
    let image = hex_encode(data.image());
    let mut metadata = data.metadata;
    metadata
        .as_object_mut()
        .expect("picture metadata is an object")
        .insert("ImageHex".into(), Value::String(image));
    metadata
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
        let length = u32::try_from(data.len()).expect("24-bit FLAC metadata length was validated");
        output.extend_from_slice(&length.to_be_bytes()[1..]);
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
    push_u32_be(&mut output, descriptor.picture_type.cast_unsigned());
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
    push_u32_be(&mut output, descriptor.width.cast_unsigned());
    push_u32_be(&mut output, descriptor.height.cast_unsigned());
    push_u32_be(&mut output, descriptor.depth.cast_unsigned());
    push_u32_be(&mut output, template.colors.cast_unsigned());
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
        .map_or_default(|duration| duration.as_nanos());
    format!("{value:x}")
}

fn alac_cookie(request: &Value) -> Result<Value, String> {
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
    crate::preparation::alac::find_patches_file(&request.path, &request.cookie, &request.packets)
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
    let written = crate::preparation::alac::apply_patches_file(
        &request.source,
        Path::new(&request.destination),
        request.patches.iter().map(|p| (p.position, p.end_bit)),
    )
    .map_err(|error| error.message)?;
    Ok(json!({"WrittenBytes": written}))
}

fn open_flac(path: &str) -> Result<(fs::File, u64), String> {
    let file = fs::File::open(path).map_err(|error| format!("{path}: {error}"))?;
    let length = file
        .metadata()
        .map_err(|error| format!("{path}: {error}"))?
        .len();
    Ok((file, length))
}

/// Streams the FLAC metadata block chain, capturing the first block of `wanted`.
///
/// Bodies of blocks that are not `wanted` are skipped with a seek, and audio
/// frames after the final metadata block are never read.
fn read_flac_metadata<R: Read + Seek>(
    reader: &mut R,
    length: u64,
    wanted: Option<u8>,
) -> Result<Option<Vec<u8>>, String> {
    if length < 4 {
        return Err("Not a FLAC stream.".into());
    }
    let mut magic = [0; 4];
    reader
        .read_exact(&mut magic)
        .map_err(|error| error.to_string())?;
    if &magic != b"fLaC" {
        return Err("Not a FLAC stream.".into());
    }
    let mut offset = 4u64;
    let mut first: Option<(u8, u32)> = None;
    let mut captured: Option<Vec<u8>> = None;
    let mut last = false;
    while !last {
        if length.saturating_sub(offset) < 4 {
            return Err("FLAC metadata header is truncated.".into());
        }
        let mut header = [0; 4];
        reader
            .read_exact(&mut header)
            .map_err(|error| error.to_string())?;
        last = header[0] & 0x80 != 0;
        let kind = header[0] & 0x7f;
        let block_length =
            (u32::from(header[1]) << 16) | (u32::from(header[2]) << 8) | u32::from(header[3]);
        offset += 4;
        let end = offset
            .checked_add(u64::from(block_length))
            .ok_or("FLAC metadata length overflow")?;
        if end > length {
            return Err("FLAC metadata block is truncated.".into());
        }
        if first.is_none() {
            first = Some((kind, block_length));
        }
        if Some(kind) == wanted && captured.is_none() {
            let mut block = vec![
                0;
                usize::try_from(block_length)
                    .expect("24-bit FLAC metadata length fits usize")
            ];
            reader
                .read_exact(&mut block)
                .map_err(|error| error.to_string())?;
            captured = Some(block);
        } else {
            reader
                .seek(SeekFrom::Start(end))
                .map_err(|error| error.to_string())?;
        }
        offset = end;
    }
    if first.is_none_or(|(kind, block_length)| kind != 0 || block_length != 34) {
        return Err("FLAC STREAMINFO block is missing or invalid.".into());
    }
    Ok(captured)
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

#[cfg(test)]
fn _path_is_file(path: &str) -> bool {
    Path::new(path).is_file()
}

#[cfg(test)]
mod dvd_audio_filesystem_tests {
    use super::*;
    use std::io::Cursor;

    const SECTORS: usize = 300;

    fn set_udf_tag(image: &mut [u8], lba: usize, id: u16, location: u32) {
        let start = lba * ISO_SECTOR_USIZE;
        let descriptor = &mut image[start..start + ISO_SECTOR_USIZE];
        descriptor[..2].copy_from_slice(&id.to_le_bytes());
        descriptor[2..4].copy_from_slice(&2u16.to_le_bytes());
        descriptor[6..8].copy_from_slice(&1u16.to_le_bytes());
        descriptor[10..12].copy_from_slice(&496u16.to_le_bytes());
        descriptor[12..16].copy_from_slice(&location.to_le_bytes());
        let crc = udf_crc16(&descriptor[16..512]);
        descriptor[8..10].copy_from_slice(&crc.to_le_bytes());
        descriptor[4] = descriptor[..4]
            .iter()
            .chain(&descriptor[5..16])
            .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    }

    fn fixture() -> Vec<u8> {
        let mut image = vec![0; SECTORS * ISO_SECTOR_USIZE];
        let pvd = &mut image[16 * ISO_SECTOR_USIZE..17 * ISO_SECTOR_USIZE];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[6] = 1;
        let sector_count = u32::try_from(SECTORS).expect("fixture sector count fits u32");
        pvd[80..84].copy_from_slice(&sector_count.to_le_bytes());
        pvd[84..88].copy_from_slice(&sector_count.to_be_bytes());
        pvd[128..130].copy_from_slice(&(ISO_SECTOR_U16).to_le_bytes());
        pvd[130..132].copy_from_slice(&(ISO_SECTOR_U16).to_be_bytes());
        pvd[156] = 34;
        pvd[181] = 2;
        pvd[184..186].copy_from_slice(&1u16.to_le_bytes());
        pvd[186..188].copy_from_slice(&1u16.to_be_bytes());
        pvd[881] = 1;

        let terminator = &mut image[17 * ISO_SECTOR_USIZE..18 * ISO_SECTOR_USIZE];
        terminator[0] = 255;
        terminator[1..6].copy_from_slice(b"CD001");
        terminator[6] = 1;
        for (lba, id) in [(18, b"BEA01".as_slice()), (19, b"NSR02"), (20, b"TEA01")] {
            let start = lba * ISO_SECTOR_USIZE;
            image[start] = 0;
            image[start + 1..start + 6].copy_from_slice(id);
        }
        let anchor = &mut image[256 * ISO_SECTOR_USIZE..257 * ISO_SECTOR_USIZE];
        anchor[..2].copy_from_slice(&2u16.to_le_bytes());
        for (length_at, location_at, location) in [(16, 20, 32u32), (24, 28, 48u32)] {
            anchor[length_at..length_at + 4].copy_from_slice(&(6 * 2048u32).to_le_bytes());
            anchor[location_at..location_at + 4].copy_from_slice(&location.to_le_bytes());
        }
        set_udf_tag(&mut image, 256, 2, 256);
        let end_anchor = 299 * ISO_SECTOR_USIZE;
        for (length_at, location_at, location) in [(16, 20, 32u32), (24, 28, 48u32)] {
            image[end_anchor + length_at..end_anchor + length_at + 4]
                .copy_from_slice(&(6 * 2048u32).to_le_bytes());
            image[end_anchor + location_at..end_anchor + location_at + 4]
                .copy_from_slice(&location.to_le_bytes());
        }
        set_udf_tag(&mut image, 299, 2, 299);
        let partition = 34 * ISO_SECTOR_USIZE;
        image[partition + 188..partition + 192].copy_from_slice(&257u32.to_le_bytes());
        image[partition + 192..partition + 196].copy_from_slice(&42u32.to_le_bytes());
        let logical = 35 * ISO_SECTOR_USIZE;
        image[logical + 432..logical + 436].copy_from_slice(&(2 * 2048u32).to_le_bytes());
        image[logical + 436..logical + 440].copy_from_slice(&64u32.to_le_bytes());
        for (vds, ids) in [(32, [1u16, 4, 5, 6, 7, 8]), (48, [1, 4, 5, 6, 7, 8])] {
            for (index, id) in ids.into_iter().enumerate() {
                set_udf_tag(
                    &mut image,
                    vds + index,
                    id,
                    u32::try_from(vds + index).expect("fixture LBA fits u32"),
                );
            }
        }
        set_udf_tag(&mut image, 64, 9, 64);
        set_udf_tag(&mut image, 65, 8, 65);
        set_udf_tag(&mut image, 257, 256, 0);
        set_udf_tag(&mut image, 258, 8, 1);
        image
    }

    fn verify(image: &[u8]) -> Result<(), String> {
        verify_dvd_audio_filesystem_reader(&mut Cursor::new(image), image.len() as u64)
    }

    #[test]
    fn accepts_iso9660_and_udf_hybrid_volume_descriptors() {
        verify(&fixture()).unwrap();
    }

    #[test]
    fn rejects_incorrect_pvd_root_record_and_volume_size() {
        let mut image = fixture();
        image[16 * ISO_SECTOR_USIZE + 181] = 1;
        assert!(
            verify(&image)
                .unwrap_err()
                .starts_with("ISO_ROOT_RECORD_INVALID")
        );

        let mut image = fixture();
        image[16 * ISO_SECTOR_USIZE + 80..16 * ISO_SECTOR_USIZE + 84]
            .copy_from_slice(&256u32.to_le_bytes());
        assert!(
            verify(&image)
                .unwrap_err()
                .starts_with("ISO_VOLUME_SPACE_MISMATCH")
        );
    }

    #[test]
    fn rejects_missing_udf_markers_anchor_and_out_of_range_descriptor_extents() {
        let mut image = fixture();
        image[19 * ISO_SECTOR_USIZE + 1..19 * ISO_SECTOR_USIZE + 6].copy_from_slice(b"NSR01");
        assert!(
            verify(&image)
                .unwrap_err()
                .starts_with("UDF_RECOGNITION_INVALID")
        );

        let mut image = fixture();
        image[256 * ISO_SECTOR_USIZE..256 * ISO_SECTOR_USIZE + 2]
            .copy_from_slice(&0u16.to_le_bytes());
        assert!(
            verify(&image)
                .unwrap_err()
                .starts_with("UDF_ANCHOR_MISSING")
        );

        let mut image = fixture();
        let location = 256 * ISO_SECTOR_USIZE + 20;
        image[location..location + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        set_udf_tag(&mut image, 256, 2, 256);
        assert!(
            verify(&image)
                .unwrap_err()
                .starts_with("UDF_VDS_EXTENT_INVALID")
        );
    }
}

/// Release-only A/B benchmark for the FLAC metadata streaming change.
///
/// The document path is the pre-change `fs::read` + `read_flac_document` query;
/// the streaming path is `open_flac` + `read_flac_metadata`. Both use the same
/// on-disk FLAC with a real audio region, large non-target metadata, a Vorbis
/// comment block, and a PICTURE block. Run with:
/// `cargo test --release -p dvda-core --lib -- --ignored --exact formats::flac_streaming_bench::flac_streaming_ab_benchmark --nocapture --test-threads=1`
#[cfg(test)]
mod flac_streaming_bench {
    use super::{
        FlacPictureData, flac_comments_reader, flac_picture_data_reader, flac_picture_json,
        open_flac, parse_flac_comments, parse_flac_picture, read_flac_document, read_flac_metadata,
    };
    use serde_json::{Value, json};
    use std::{
        fmt::Debug,
        fs::{self, File},
        hint::black_box,
        io::{BufWriter, Write},
        path::{Path, PathBuf},
        time::Instant,
    };

    const AUDIO_BYTES: usize = 64 * 1024 * 1024;
    const LEADING_PADDING: u32 = 8 * 1024 * 1024;
    const TRAILING_PADDING: u32 = 2 * 1024 * 1024;
    const IMAGE_BYTES: usize = 6 * 1024 * 1024;
    const ROUNDS: usize = 3;

    struct BenchScratch(PathBuf);

    impl BenchScratch {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("dvda-flac-stream-bench-{}", std::process::id()));
            fs::create_dir_all(&path).expect("benchmark scratch directory is creatable");
            Self(path)
        }
    }

    impl Drop for BenchScratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.0));
        }
    }

    fn encode_length(value: usize) -> u32 {
        u32::try_from(value).expect("benchmark payload fits a 24-bit FLAC block length")
    }

    fn comment_payload() -> Vec<u8> {
        let values = ["TITLE=streaming-benchmark", "ARTIST=DVDA"];
        let vendor = b"bench";
        let mut data = encode_length(vendor.len()).to_le_bytes().to_vec();
        data.extend_from_slice(vendor);
        data.extend_from_slice(&encode_length(values.len()).to_le_bytes());
        for value in values {
            data.extend_from_slice(&encode_length(value.len()).to_le_bytes());
            data.extend_from_slice(value.as_bytes());
        }
        data
    }

    fn picture_payload() -> Vec<u8> {
        let mut data = 3u32.to_be_bytes().to_vec();
        for text in ["image/png", "streaming cover"] {
            data.extend_from_slice(&encode_length(text.len()).to_be_bytes());
            data.extend_from_slice(text.as_bytes());
        }
        for number in [3u32, 2, 24, 0, encode_length(IMAGE_BYTES)] {
            data.extend_from_slice(&number.to_be_bytes());
        }
        for index in 0..IMAGE_BYTES {
            data.push(u8::try_from((index * 37 + 11) % 256).expect("modulo fits u8"));
        }
        data
    }

    fn write_block_header<W: Write>(
        output: &mut W,
        kind: u8,
        length: u32,
        last: bool,
    ) -> std::io::Result<()> {
        let bytes = length.to_be_bytes();
        output.write_all(&[
            kind | if last { 0x80 } else { 0 },
            bytes[1],
            bytes[2],
            bytes[3],
        ])
    }

    fn write_zeros<W: Write>(output: &mut W, mut remaining: u32) -> std::io::Result<()> {
        const CHUNK: u32 = 1024 * 1024;
        let zeros = vec![0u8; usize::try_from(CHUNK).expect("chunk fits usize")];
        while remaining > 0 {
            let count = remaining.min(CHUNK);
            output.write_all(&zeros[..usize::try_from(count).expect("count fits usize")])?;
            remaining -= count;
        }
        Ok(())
    }

    fn write_audio<W: Write>(output: &mut W, mut remaining: usize) -> std::io::Result<()> {
        const CHUNK: usize = 64 * 1024;
        let mut buffer = vec![0u8; CHUNK];
        let mut state = 0x1234_5678_u32;
        while remaining > 0 {
            let count = remaining.min(CHUNK);
            for byte in &mut buffer[..count] {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                *byte = u8::try_from(state & 0xff).expect("masked generator byte fits u8");
            }
            output.write_all(&buffer[..count])?;
            remaining -= count;
        }
        Ok(())
    }

    fn build_file(path: &Path) -> std::io::Result<()> {
        let mut output = BufWriter::with_capacity(1024 * 1024, File::create(path)?);
        output.write_all(b"fLaC")?;
        write_block_header(&mut output, 0, 34, false)?;
        output.write_all(&[0; 34])?;
        write_block_header(&mut output, 1, LEADING_PADDING, false)?;
        write_zeros(&mut output, LEADING_PADDING)?;
        let comments = comment_payload();
        write_block_header(&mut output, 4, encode_length(comments.len()), false)?;
        output.write_all(&comments)?;
        let picture = picture_payload();
        write_block_header(&mut output, 6, encode_length(picture.len()), false)?;
        output.write_all(&picture)?;
        write_block_header(&mut output, 1, TRAILING_PADDING, true)?;
        write_zeros(&mut output, TRAILING_PADDING)?;
        write_audio(&mut output, AUDIO_BYTES)?;
        output.flush()
    }

    fn document_comments(path: &str) -> Result<Value, String> {
        let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
        let (blocks, _) = read_flac_document(&bytes)?;
        let Some((_, data)) = blocks.iter().find(|(kind, _)| *kind == 4) else {
            return Ok(json!([]));
        };
        parse_flac_comments(data)
    }

    fn document_picture_data(path: &str) -> Result<FlacPictureData, String> {
        let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
        let (blocks, _) = read_flac_document(&bytes)?;
        let Some((_, block)) = blocks.into_iter().find(|(kind, _)| *kind == 6) else {
            return Ok(FlacPictureData {
                metadata: Value::Null,
                block: Vec::new(),
                image_offset: 0,
            });
        };
        parse_flac_picture(block)
    }

    fn stream_comments(path: &str) -> Result<Value, String> {
        let (mut file, length) = open_flac(path)?;
        flac_comments_reader(&mut file, length)
    }

    fn stream_picture_data(path: &str) -> Result<FlacPictureData, String> {
        let (mut file, length) = open_flac(path)?;
        flac_picture_data_reader(&mut file, length)
    }

    fn document_picture_json(path: &str) -> Result<Value, String> {
        document_picture_data(path).map(flac_picture_json)
    }

    fn stream_picture_json(path: &str) -> Result<Value, String> {
        stream_picture_data(path).map(flac_picture_json)
    }

    fn timed<T>(operation: impl FnOnce() -> T) -> (T, u128) {
        let start = Instant::now();
        let value = operation();
        (black_box(value), start.elapsed().as_nanos())
    }

    fn compare_pair<T: Debug + PartialEq>(
        reverse: bool,
        document: impl Fn() -> T,
        stream: impl Fn() -> T,
    ) -> (u128, u128) {
        if reverse {
            let (stream_value, stream_ns) = timed(&stream);
            let (document_value, document_ns) = timed(&document);
            assert_eq!(document_value, stream_value, "old and new results differ");
            (document_ns, stream_ns)
        } else {
            let (document_value, document_ns) = timed(&document);
            let (stream_value, stream_ns) = timed(&stream);
            assert_eq!(document_value, stream_value, "old and new results differ");
            (document_ns, stream_ns)
        }
    }

    #[derive(Default)]
    struct Timings {
        document: [u128; ROUNDS],
        stream: [u128; ROUNDS],
    }

    fn median(mut values: [u128; ROUNDS]) -> u128 {
        values.sort_unstable();
        values[ROUNDS / 2]
    }

    fn report(label: &str, timings: &Timings) {
        println!("[BENCH] {label}_document_ns={:?}", timings.document);
        println!("[BENCH] {label}_stream_ns={:?}", timings.stream);
        println!(
            "[BENCH] {label}_median_document_ns={} median_stream_ns={}",
            median(timings.document),
            median(timings.stream)
        );
    }

    fn verify_equivalence(path: &str, length: u64) {
        let bytes = fs::read(path).expect("benchmark input is readable");
        let (blocks, _) = read_flac_document(&bytes).expect("benchmark input has a block chain");
        let document_comment_block = blocks
            .iter()
            .find(|(kind, _)| *kind == 4)
            .expect("benchmark comment block exists")
            .1
            .clone();
        let document_picture_block = blocks
            .iter()
            .find(|(kind, _)| *kind == 6)
            .expect("benchmark picture block exists")
            .1
            .clone();

        let (mut comment_reader, comment_length) =
            open_flac(path).expect("benchmark input opens for comments");
        assert_eq!(comment_length, length);
        let stream_comment_block = read_flac_metadata(&mut comment_reader, length, Some(4))
            .expect("comment block chain parses")
            .expect("benchmark comment block exists");
        let (mut picture_reader, picture_length) =
            open_flac(path).expect("benchmark input opens for pictures");
        assert_eq!(picture_length, length);
        let stream_picture_block = read_flac_metadata(&mut picture_reader, length, Some(6))
            .expect("picture block chain parses")
            .expect("benchmark picture block exists");
        assert_eq!(
            document_comment_block, stream_comment_block,
            "comment block bytes differ"
        );
        assert_eq!(
            document_picture_block, stream_picture_block,
            "picture block bytes differ"
        );

        let document_comments_value = document_comments(path).expect("old comments query succeeds");
        let stream_comments_value = stream_comments(path).expect("new comments query succeeds");
        assert_eq!(
            document_comments_value, stream_comments_value,
            "comments JSON differs"
        );

        let document_picture_value =
            document_picture_data(path).expect("old picture query succeeds");
        let stream_picture_value = stream_picture_data(path).expect("new picture query succeeds");
        assert_eq!(
            document_picture_value.metadata, stream_picture_value.metadata,
            "picture metadata differs"
        );
        assert_eq!(
            document_picture_value.image(),
            stream_picture_value.image(),
            "picture bytes differ"
        );
        let document_picture_json_value = flac_picture_json(document_picture_value);
        let stream_picture_json_value = flac_picture_json(stream_picture_value);
        assert_eq!(
            document_picture_json_value, stream_picture_json_value,
            "picture JSON differs"
        );
        assert!(
            document_picture_json_value["ImageHex"]
                .as_str()
                .is_some_and(|hex| hex.len() == IMAGE_BYTES * 2),
            "picture JSON does not contain the expected image bytes"
        );
    }

    #[test]
    #[ignore = "release-only A/B benchmark for FLAC metadata streaming"]
    #[allow(
        clippy::too_many_lines,
        reason = "the benchmark keeps its input, alternating rounds, and comparison in one place"
    )]
    fn flac_streaming_ab_benchmark() {
        let scratch = BenchScratch::new();
        let path = scratch.0.join("stream-bench.flac");
        build_file(&path).expect("benchmark FLAC is writable");
        let path_text = path.to_str().expect("benchmark path is UTF-8");
        let length = fs::metadata(&path).expect("benchmark FLAC exists").len();
        verify_equivalence(path_text, length);

        let mut comments = Timings::default();
        let mut pictures = Timings::default();
        let mut picture_json = Timings::default();
        for round in 0..ROUNDS {
            let reverse = round % 2 == 1;
            let (document_ns, stream_ns) = compare_pair(
                reverse,
                || document_comments(path_text),
                || stream_comments(path_text),
            );
            comments.document[round] = document_ns;
            comments.stream[round] = stream_ns;

            if reverse {
                let (_, stream_ns) = timed(|| stream_picture_data(path_text));
                pictures.stream[round] = stream_ns;
                let (_, document_ns) = timed(|| document_picture_data(path_text));
                pictures.document[round] = document_ns;
            } else {
                let (_, document_ns) = timed(|| document_picture_data(path_text));
                pictures.document[round] = document_ns;
                let (_, stream_ns) = timed(|| stream_picture_data(path_text));
                pictures.stream[round] = stream_ns;
            }

            let (picture_document_ns, picture_stream_ns) = compare_pair(
                reverse,
                || document_picture_json(path_text),
                || stream_picture_json(path_text),
            );
            picture_json.document[round] = picture_document_ns;
            picture_json.stream[round] = picture_stream_ns;
        }

        println!(
            "[BENCH] file_bytes={length} audio_bytes={AUDIO_BYTES} image_bytes={IMAGE_BYTES} leading_padding={LEADING_PADDING} trailing_padding={TRAILING_PADDING}"
        );
        report("comments", &comments);
        report("picture_raw", &pictures);
        report("picture_json", &picture_json);
    }
}
