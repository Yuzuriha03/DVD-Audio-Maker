use serde_json::{Value, json};
use std::{fs, path::Path};

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
    let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
    let info = parse_iso_info(path, &bytes)?;
    serde_json::to_value(info).map_err(|error| error.to_string())
}

fn iso_list(request: Value) -> Result<Value, String> {
    let (path, inner_path) = iso_request(&request)?;
    let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
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
    let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
    let info = parse_iso_info(path, &bytes)?;
    let entry = iso_lookup(&bytes, &info, inner_path).ok();
    serde_json::to_value(entry).map_err(|error| error.to_string())
}

fn iso_all_paths(request: Value) -> Result<Value, String> {
    let (path, inner_path) = iso_request(&request)?;
    let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
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
    let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
    let info = parse_iso_info(path, &bytes)?;
    let entry = iso_lookup(&bytes, &info, inner_path)?;
    Ok(json!(
        u64::from(entry.logical_block_address) + u64::from(entry.extended_attribute_blocks)
    ))
}

fn iso_read_file(request: Value) -> Result<Value, String> {
    let (path, inner_path) = iso_request(&request)?;
    let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
    let info = parse_iso_info(path, &bytes)?;
    let entry = iso_lookup(&bytes, &info, inner_path)?;
    if entry.is_directory {
        return Err(format!("{inner_path} is a directory, not a file"));
    }
    if entry.is_multi_extent {
        return Err(format!("{inner_path} is a multi-extent file"));
    }
    let offset = iso_data_offset(&info, &entry)?;
    let end = offset
        .checked_add(entry.size as usize)
        .ok_or("ISO file range overflow")?;
    if end > bytes.len() {
        return Err("ISO file exceeds the image boundary.".into());
    }
    Ok(json!({"DataHex": hex_encode(&bytes[offset..end])}))
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
    let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
    let info = parse_iso_info(path, &bytes)?;
    let Some(entry) = iso_lookup(&bytes, &info, inner_path).ok() else {
        return Ok(json!({"Extracted":false}));
    };
    if entry.is_directory {
        fs::create_dir_all(destination).map_err(|error| format!("{destination}: {error}"))?;
        iso_extract_directory(&bytes, &info, &entry, destination)?;
    } else {
        let parent = Path::new(destination).parent();
        if let Some(parent) = parent {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("{}: {error}", parent.display()))?;
            }
        }
        let data = iso_file_data(&bytes, &info, &entry)?;
        fs::write(destination, data).map_err(|error| format!("{destination}: {error}"))?;
    }
    Ok(json!({"Extracted":true}))
}

fn iso_extract_directory(
    bytes: &[u8],
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
            let data = iso_file_data(bytes, info, &child)?;
            fs::write(&output, data).map_err(|error| format!("{}: {error}", output.display()))?;
        }
    }
    Ok(())
}

fn iso_file_data<'a>(
    bytes: &'a [u8],
    info: &IsoInfo,
    entry: &IsoEntry,
) -> Result<&'a [u8], String> {
    if entry.is_multi_extent {
        return Err(format!("{} is a multi-extent file", entry.name));
    }
    let offset = iso_data_offset(info, entry)?;
    let end = offset
        .checked_add(entry.size as usize)
        .ok_or("ISO file range overflow")?;
    if end > bytes.len() {
        return Err("ISO file exceeds the image boundary.".into());
    }
    Ok(&bytes[offset..end])
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

fn parse_iso_info(path: &str, bytes: &[u8]) -> Result<IsoInfo, String> {
    const DEFAULT_SECTOR: usize = 2048;
    for index in 0..32usize {
        let start = (16 + index)
            .checked_mul(DEFAULT_SECTOR)
            .ok_or("ISO descriptor offset overflow")?;
        let end = start
            .checked_add(DEFAULT_SECTOR)
            .ok_or("ISO descriptor end overflow")?;
        if end > bytes.len() {
            break;
        }
        let sector = &bytes[start..end];
        if &sector[1..6] != b"CD001" {
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
        let mut sector_size = u16_le(&sector[128..130])? as i32;
        if sector_size == 0 {
            sector_size = DEFAULT_SECTOR as i32;
        }
        if sector[156] < 34 {
            return Err("ISO primary volume root entry is truncated.".into());
        }
        let root = &sector[156..190];
        return Ok(IsoInfo {
            sector_size,
            root_logical_block_address: u32_le(&root[2..6])?,
            root_size: u32_le(&root[10..14])?,
            volume_identifier: String::from_utf8_lossy(&sector[40..72])
                .trim_end_matches([' ', '\0'])
                .to_owned(),
        });
    }
    Err(format!(
        "No ISO9660 Primary Volume Descriptor found in {path} (sectors 16..47)."
    ))
}

fn iso_parse_directory(
    bytes: &[u8],
    info: &IsoInfo,
    entry: &IsoEntry,
) -> Result<Vec<IsoEntry>, String> {
    let offset = iso_data_offset(info, entry)?;
    let size = entry.size as usize;
    let end = offset
        .checked_add(size)
        .ok_or("ISO directory size overflow")?;
    if end > bytes.len() {
        return Err("ISO directory exceeds the image boundary.".into());
    }
    let blob = &bytes[offset..end];
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

fn iso_lookup(bytes: &[u8], info: &IsoInfo, inner_path: &str) -> Result<IsoEntry, String> {
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
    bytes: &[u8],
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
            .all(|character| character.to_digit(10).is_some())
    {
        return Ok(Value::String(value.to_owned()));
    }
    Ok(Value::String(value[..index].to_owned()))
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct WavLayout {
    container_bits: i32,
    valid_bits: i32,
    channels: i32,
    sample_rate: i32,
    data_offset: i64,
    data_size: i64,
    bytes_per_sample: i32,
    channel_mask: u32,
}

const PCM_GUID: [u8; 16] = [1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113];

fn wav_layout(request: Value) -> Result<Value, String> {
    let path = request.as_str().ok_or("Expected WAVE path")?;
    let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
    let layout = parse_wav_layout(&bytes)?;
    Ok(serde_json::to_value(layout).map_err(|error| error.to_string())?)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WavNormalizeRequest {
    source: String,
    destination: String,
    rate: i32,
    bits: i32,
}

fn wav_normalize(request: Value) -> Result<Value, String> {
    let request: WavNormalizeRequest =
        serde_json::from_value(request).map_err(|error| error.to_string())?;
    if !matches!(request.bits, 16 | 20 | 24) {
        return Err("Target PCM depth must be 16/20/24 bits.".into());
    }
    let source =
        fs::read(&request.source).map_err(|error| format!("{}: {error}", request.source))?;
    let layout = parse_wav_layout(&source)?;
    if layout.sample_rate != request.rate {
        return Err("PCM sample rate does not match the task.".into());
    }
    let mut channel_mask = layout.channel_mask;
    if channel_mask & 0x600 != 0 {
        if channel_mask & 0x30 != 0 || channel_mask & 0x100 != 0 {
            return Err("Cannot merge simultaneous side, rear or back-center channels into DVD surround channels.".into());
        }
        channel_mask = (channel_mask & !0x600) | ((channel_mask & 0x600) >> 5);
    }
    let output_width = if request.bits == 16 { 2usize } else { 3usize };
    let source_frame = layout.channels as usize * layout.bytes_per_sample as usize;
    let frames = layout.data_size as usize / source_frame;
    let output_size = frames
        .checked_mul(layout.channels as usize)
        .and_then(|value| value.checked_mul(output_width))
        .ok_or("PCM output size overflow")?;
    if output_size
        .checked_add(60)
        .and_then(|value| value.checked_add(output_size & 1))
        .is_none_or(|value| value > u32::MAX as usize)
    {
        return Err("PCM exceeds the RIFF 4 GiB limit; split the track.".into());
    }
    let mut output = Vec::with_capacity(60 + output_size + (output_size & 1));
    output.extend_from_slice(b"RIFF");
    push_u32_le(&mut output, (60 + output_size + (output_size & 1)) as u32);
    output.extend_from_slice(b"WAVEfmt ");
    push_u32_le(&mut output, 40);
    push_u16_le(&mut output, 0xfffe);
    push_u16_le(&mut output, layout.channels as u16);
    push_u32_le(&mut output, request.rate as u32);
    push_u32_le(
        &mut output,
        (request.rate as u64 * layout.channels as u64 * output_width as u64)
            .try_into()
            .map_err(|_| "PCM byte rate overflow")?,
    );
    push_u16_le(
        &mut output,
        (layout.channels as usize * output_width) as u16,
    );
    push_u16_le(&mut output, (output_width * 8) as u16);
    push_u16_le(&mut output, 22);
    push_u16_le(&mut output, request.bits as u16);
    push_u32_le(&mut output, channel_mask);
    output.extend_from_slice(&PCM_GUID);
    output.extend_from_slice(b"data");
    push_u32_le(&mut output, output_size as u32);
    let data_start = layout.data_offset as usize;
    let data_end = data_start
        .checked_add(layout.data_size as usize)
        .ok_or("PCM data range overflow")?;
    if data_end > source.len() {
        return Err("PCM data chunk is truncated.".into());
    }
    let data = &source[data_start..data_end];
    for frame in data.chunks_exact(source_frame) {
        for sample_bytes in frame.chunks_exact(layout.bytes_per_sample as usize) {
            let mut sample = match layout.bytes_per_sample {
                2 => i16::from_le_bytes([sample_bytes[0], sample_bytes[1]]) as i64 * 256,
                3 => {
                    let value = (sample_bytes[0] as i32)
                        | ((sample_bytes[1] as i32) << 8)
                        | ((sample_bytes[2] as i32) << 16);
                    ((value << 8) >> 8) as i64
                }
                4 => i32::from_le_bytes([
                    sample_bytes[0],
                    sample_bytes[1],
                    sample_bytes[2],
                    sample_bytes[3],
                ]) as i64,
                _ => return Err("Invalid PCM storage width.".into()),
            };
            if layout.bytes_per_sample == 4 {
                if sample & 255 != 0 {
                    return Err(
                        "32-bit storage contains significant bits beyond 24-bit PCM.".into(),
                    );
                }
                sample >>= 8;
            }
            let mask = if request.bits == 24 {
                0
            } else {
                (1i64 << (24 - request.bits)) - 1
            };
            if sample & mask != 0 {
                return Err(
                    "PCM precision is higher than the target depth; silent truncation is refused."
                        .into(),
                );
            }
            if request.bits == 16 {
                sample >>= 8;
            }
            output.push(sample as u8);
            output.push((sample >> 8) as u8);
            if output_width == 3 {
                output.push((sample >> 16) as u8);
            }
        }
    }
    if output_size & 1 != 0 {
        output.push(0);
    }
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&request.destination)
        .map_err(|error| format!("{}: {error}", request.destination));
    match file {
        Ok(mut file) => {
            use std::io::Write;
            if let Err(error) = file.write_all(&output) {
                drop(file);
                let _ = fs::remove_file(&request.destination);
                return Err(format!("{}: {error}", request.destination));
            }
        }
        Err(error) => return Err(error),
    }
    Ok(json!({"WrittenBytes": output.len()}))
}

fn push_u16_le(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}
fn push_u32_le(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn parse_wav_layout(bytes: &[u8]) -> Result<WavLayout, String> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("WAVE input must be an integer RIFF/WAVE PCM stream.".into());
    }
    let riff_size = u32_le(&bytes[4..8])? as usize;
    let end = riff_size.checked_add(8).ok_or("WAVE length overflow")?;
    if end > bytes.len() || end < 12 {
        return Err("WAVE length field is invalid or the file is truncated.".into());
    }
    let mut position = 12usize;
    let mut bits = 0i32;
    let mut valid = 0i32;
    let mut channels = 0i32;
    let mut rate = 0i32;
    let mut align = 0i32;
    let mut mask = 0u32;
    let mut data_offset = None;
    let mut data_size = 0i64;
    while position + 8 <= end {
        let id = &bytes[position..position + 4];
        let length = u32_le(&bytes[position + 4..position + 8])? as usize;
        let payload = position + 8;
        let payload_end = payload
            .checked_add(length)
            .ok_or("WAVE chunk length overflow")?;
        let mut next = payload_end
            .checked_add(length & 1)
            .ok_or("WAVE chunk alignment overflow")?;
        if length & 1 != 0 && id == b"data" && payload_end == end && end == bytes.len() {
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
            let fmt = &bytes[payload..payload_end];
            let tag = u16_le(&fmt[0..2])?;
            channels = u16_le(&fmt[2..4])? as i32;
            rate = u32_le(&fmt[4..8])?
                .try_into()
                .map_err(|_| "WAVE sample rate is too large")?;
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
    let mut offset = 0usize;
    let vendor_length = u32_le(&data[offset..offset + 4])? as usize;
    offset = offset
        .checked_add(4 + vendor_length)
        .ok_or("FLAC vendor length overflow")?;
    let count = u32_le(&data[offset..offset + 4])? as usize;
    offset += 4;
    let mut comments = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let length = u32_le(&data[offset..offset + 4])? as usize;
        offset += 4;
        let end = offset
            .checked_add(length)
            .ok_or("FLAC comment length overflow")?;
        if end > data.len() {
            return Err("FLAC Vorbis comment block is truncated.".into());
        }
        let value = std::str::from_utf8(&data[offset..end])
            .map_err(|error| format!("Invalid FLAC comment UTF-8: {error}"))?;
        if let Some((key, value)) = value.split_once('=') {
            if !key.is_empty() {
                comments.push(json!({"Key":key,"Value":value}));
            }
        }
        offset = end;
    }
    if offset != data.len() {
        return Err("FLAC metadata block has trailing bytes.".into());
    }
    Ok(Value::Array(comments))
}

fn flac_picture(request: Value) -> Result<Value, String> {
    let path = request.as_str().ok_or("Expected FLAC path")?;
    let document = read_flac(path)?;
    let Some((_, data)) = document.iter().find(|(kind, _)| *kind == 6) else {
        return Ok(Value::Null);
    };
    let mut offset = 0usize;
    let picture_type = u32_be(&data[offset..offset + 4])?;
    offset += 4;
    let mime = read_flac_text(data, &mut offset)?;
    let description = read_flac_text(data, &mut offset)?;
    let width = u32_be(&data[offset..offset + 4])?;
    let height = u32_be(&data[offset + 4..offset + 8])?;
    let depth = u32_be(&data[offset + 8..offset + 12])?;
    let colors = u32_be(&data[offset + 12..offset + 16])?;
    let image_length = u32_be(&data[offset + 16..offset + 20])? as usize;
    offset += 20;
    let image_end = offset
        .checked_add(image_length)
        .ok_or("FLAC picture length overflow")?;
    if image_end != data.len() {
        return Err("FLAC PICTURE block is truncated or has trailing bytes.".into());
    }
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
        "ImageHex": hex_encode(&data[offset..image_end]),
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
    fs::write(&temporary, &output).map_err(|error| format!("{temporary}: {error}"))?;
    if let Err(error) = fs::rename(&temporary, &request.path) {
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
    let offset = object.get("Offset").and_then(Value::as_i64).unwrap_or(0);
    let data = hex_decode(hex)?;
    if data.len() < 24 {
        return Ok(Value::Null);
    }
    let max_samples = i32_be(&data[0..4])?;
    let sample_size = data[5];
    let channels = data[9];
    let max_coded_frame_size = i32_be(&data[12..16])?;
    let sample_rate = i32_be(&data[20..24])?;
    let rates = [
        8_000, 11_025, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000, 64_000, 88_200, 96_000,
        176_400, 192_000, 352_800, 384_000,
    ];
    if !(512..=16_384).contains(&max_samples)
        || !matches!(sample_size, 16 | 20 | 24 | 32)
        || !(1..=8).contains(&channels)
        || !rates.contains(&sample_rate)
    {
        return Ok(Value::Null);
    }
    Ok(json!({
        "MaxSamplesPerFrame": max_samples,
        "SampleSize": sample_size,
        "Channels": channels,
        "SampleRate": sample_rate,
        "MaxCodedFrameSize": max_coded_frame_size,
        "Offset": offset,
    }))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AlacFindRequest {
    path: String,
    cookie: AlacCookie,
    packets: Vec<AlacPacket>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AlacCookie {
    max_samples_per_frame: i32,
    sample_size: i32,
    channels: i32,
    #[serde(rename = "SampleRate")]
    _sample_rate: i32,
    #[serde(rename = "MaxCodedFrameSize")]
    _max_coded_frame_size: i32,
    #[serde(rename = "Offset")]
    _offset: i32,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AlacPacket {
    presentation_time: f64,
    size: i32,
    position: i64,
}

fn alac_find_bad_frames(request: Value) -> Result<Value, String> {
    let request: AlacFindRequest =
        serde_json::from_value(request).map_err(|error| error.to_string())?;
    let data = fs::read(&request.path).map_err(|error| format!("{}: {error}", request.path))?;
    let mut patches = Vec::new();
    for (packet_index, packet) in request.packets.iter().enumerate() {
        if packet.size < 4
            || packet.position < 0
            || packet.position as u64 > data.len() as u64
            || packet.size as u64 > data.len() as u64 - packet.position as u64
        {
            continue;
        }
        let start = packet.position as usize;
        let end = start + packet.size as usize;
        let packet_data = &data[start..end];
        let header_byte = packet_data[2];
        if header_byte & 0x02 == 0 {
            continue;
        }
        let has_size = header_byte & 0x10 != 0;
        let sample_count = if has_size && packet_data.len() >= 7 {
            read_bits(packet_data, 22, 32).unwrap_or(request.cookie.max_samples_per_frame as u64)
        } else {
            request.cookie.max_samples_per_frame as u64
        };
        let sample_count = if sample_count == 0 {
            request.cookie.max_samples_per_frame as u64
        } else {
            sample_count
        };
        let end_bit = 23i64
            .checked_add(
                (sample_count as i64)
                    .checked_mul(request.cookie.channels as i64)
                    .and_then(|value| value.checked_mul(request.cookie.sample_size as i64))
                    .ok_or("ALAC frame bit count overflow")?,
            )
            .ok_or("ALAC frame bit count overflow")?;
        if end_bit < 0 || end_bit + 3 > packet.size as i64 * 8 {
            continue;
        }
        let current =
            read_bits(packet_data, end_bit as usize, 3).ok_or("ALAC frame bit range is invalid")?;
        if current != 7 {
            patches.push(json!({
                "PacketIndex": packet_index,
                "PresentationTime": packet.presentation_time,
                "Position": packet.position,
                "Size": packet.size,
                "SampleCount": sample_count,
                "EndBit": end_bit,
                "PreviousBits": current,
            }));
        }
    }
    Ok(Value::Array(patches))
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
    for patch in &request.patches {
        if patch.position < 0 || patch.end_bit < 0 {
            return Err("ALAC patch bit position is negative.".into());
        }
        for bit in 0..3i64 {
            let absolute_bit = patch
                .position
                .checked_mul(8)
                .and_then(|value| value.checked_add(patch.end_bit))
                .and_then(|value| value.checked_add(bit))
                .ok_or("ALAC patch bit position overflow")?;
            let byte_index =
                usize::try_from(absolute_bit / 8).map_err(|_| "ALAC patch byte index overflow")?;
            if byte_index >= data.len() {
                return Err("ALAC patch exceeds the file boundary.".into());
            }
            let bit_in_byte = (absolute_bit % 8) as u8;
            data[byte_index] |= 1 << (7 - bit_in_byte);
        }
    }
    let parent = Path::new(&request.destination).parent();
    if let Some(parent) = parent {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        }
    }
    fs::write(&request.destination, &data)
        .map_err(|error| format!("{}: {error}", request.destination))?;
    Ok(json!({"WrittenBytes": data.len()}))
}

fn read_bits(data: &[u8], bit_offset: usize, count: usize) -> Option<u64> {
    if count == 0 || count > 64 || bit_offset.checked_add(count)? > data.len().checked_mul(8)? {
        return None;
    }
    let mut value = 0u64;
    for index in 0..count {
        let bit = bit_offset + index;
        let byte = data[bit / 8];
        value = (value << 1) | u64::from((byte >> (7 - bit % 8)) & 1);
    }
    Some(value)
}

fn read_flac(path: &str) -> Result<Vec<(u8, Vec<u8>)>, String> {
    let bytes = fs::read(path).map_err(|error| format!("{path}: {error}"))?;
    read_flac_document(&bytes).map(|(blocks, _)| blocks)
}

fn read_flac_document(bytes: &[u8]) -> Result<(Vec<(u8, Vec<u8>)>, usize), String> {
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

fn read_flac_text(data: &[u8], offset: &mut usize) -> Result<String, String> {
    let length = u32_be(&data[*offset..*offset + 4])? as usize;
    *offset += 4;
    let end = offset
        .checked_add(length)
        .ok_or("FLAC text length overflow")?;
    if end > data.len() {
        return Err("FLAC text field is truncated.".into());
    }
    let text = std::str::from_utf8(&data[*offset..end])
        .map_err(|error| format!("Invalid FLAC UTF-8: {error}"))?
        .to_owned();
    *offset = end;
    Ok(text)
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
fn i32_be(data: &[u8]) -> Result<i32, String> {
    Ok(i32::from_be_bytes(
        data.try_into().map_err(|_| "Truncated 32-bit value")?,
    ))
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
    if value.len() % 2 != 0 {
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
