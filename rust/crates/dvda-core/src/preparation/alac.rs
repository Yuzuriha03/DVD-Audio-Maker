//! ALAC inspection and repair; source bytes are never rewritten.
use super::models::Patch;
use crate::{
    identity,
    media::{self, Failure},
};
use dvda_native::{
    files,
    media::{Callbacks, Operation, OutputFormat, Request},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Cookie {
    pub max_samples_per_frame: i32,
    pub sample_size: i32,
    pub channels: i32,
    pub sample_rate: i32,
    pub max_coded_frame_size: i32,
    pub offset: i32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Packet {
    pub presentation_time: f64,
    pub size: i32,
    pub position: i64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Inspection {
    pub cookie: Cookie,
    pub patches: Vec<Patch>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Repair {
    pub output_path: String,
    pub cookie: Cookie,
    pub patches: Vec<Patch>,
}
pub fn check_cancel(events: &mut dyn Callbacks) -> Result<(), Failure> {
    if events.cancelled() {
        Err(Failure::new("Cancelled", "Source preparation cancelled"))
    } else {
        Ok(())
    }
}
#[must_use]
pub fn parse_cookie(data: &[u8], offset: i32) -> Option<Cookie> {
    if data.len() < 24 {
        return None;
    }
    let i32_at = |index| i32::from_be_bytes(data[index..index + 4].try_into().unwrap());
    let cookie = Cookie {
        max_samples_per_frame: i32_at(0),
        sample_size: i32::from(data[5]),
        channels: i32::from(data[9]),
        sample_rate: i32_at(20),
        max_coded_frame_size: i32_at(12),
        offset,
    };
    ((512..=16384).contains(&cookie.max_samples_per_frame)
        && matches!(cookie.sample_size, 16 | 20 | 24 | 32)
        && (1..=8).contains(&cookie.channels)
        && [
            8000, 11025, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000, 176_400,
            192_000, 352_800, 384_000,
        ]
        .contains(&cookie.sample_rate))
    .then_some(cookie)
}
#[must_use]
pub fn read_cookie(data: &[u8]) -> Option<Cookie> {
    for (index, _) in data.windows(4).enumerate().filter(|(_, v)| *v == b"alac") {
        for skip in [8, 12, 4] {
            let offset = index + skip;
            if let Some(cookie) = data
                .get(offset..)
                .and_then(|v| parse_cookie(v, i32::try_from(offset).ok()?))
            {
                return Some(cookie);
            }
        }
    }
    None
}
fn bits(data: &[u8], offset: usize, count: usize) -> Option<u64> {
    if count == 0 || count > 64 || offset.checked_add(count)? > data.len().checked_mul(8)? {
        return None;
    }
    let mut result = 0;
    for bit in offset..offset + count {
        result = (result << 1) | u64::from((data[bit / 8] >> (7 - bit % 8)) & 1);
    }
    Some(result)
}
trait BitInput {
    fn bits(&mut self, offset: usize, count: usize) -> Result<Option<u64>, Failure>;
}
struct SliceBits<'a>(&'a [u8]);
impl BitInput for SliceBits<'_> {
    fn bits(&mut self, offset: usize, count: usize) -> Result<Option<u64>, Failure> {
        Ok(bits(self.0, offset, count))
    }
}
struct FileBits<'a> {
    file: &'a mut std::fs::File,
    position: u64,
    size: u64,
}
impl BitInput for FileBits<'_> {
    fn bits(&mut self, offset: usize, count: usize) -> Result<Option<u64>, Failure> {
        if count == 0
            || count > 64
            || offset
                .checked_add(count)
                .is_none_or(|end| end as u64 > self.size * 8)
        {
            return Ok(None);
        }
        let byte = offset / 8;
        let bytes = ((offset % 8 + count).div_ceil(8)).max(1);
        let position = self
            .position
            .checked_add(byte as u64)
            .ok_or_else(|| Failure::new("Overflow", "ALAC file position overflow"))?;
        self.file.seek(SeekFrom::Start(position))?;
        let mut data = [0u8; 9];
        self.file.read_exact(&mut data[..bytes])?;
        Ok(bits(&data[..bytes], offset % 8, count))
    }
}
fn packet_patch(
    packet_index: usize,
    packet: &Packet,
    cookie: &Cookie,
    input: &mut impl BitInput,
) -> Result<Option<Patch>, Failure> {
    let flags = u8::try_from(input.bits(16, 8)?.unwrap_or(0)).unwrap_or(0);
    if flags & 2 == 0 {
        return Ok(None);
    }
    let mut samples = if flags & 0x10 != 0 && packet.size >= 7 {
        let Some(value) = input.bits(22, 32)? else {
            return Ok(None);
        };
        i32::try_from(value).map_err(|_| Failure::new("Overflow", "ALAC sample count overflow"))?
    } else {
        cookie.max_samples_per_frame
    };
    if samples <= 0 {
        samples = cookie.max_samples_per_frame;
    }
    let end_bit = i64::from(samples)
        .checked_mul(i64::from(cookie.channels))
        .and_then(|n| n.checked_mul(i64::from(cookie.sample_size)))
        .and_then(|n| n.checked_add(23))
        .ok_or_else(|| Failure::new("Overflow", "ALAC frame bit count overflow"))?;
    if end_bit < 0
        || end_bit
            .checked_add(3)
            .is_none_or(|n| n > i64::from(packet.size) * 8)
    {
        return Ok(None);
    }
    let end_bit = usize::try_from(end_bit)
        .map_err(|_| Failure::new("Overflow", "ALAC frame bit offset overflow"))?;
    let Some(previous_bits) = input.bits(end_bit, 3)? else {
        return Ok(None);
    };
    if previous_bits == 7 {
        return Ok(None);
    }
    Ok(Some(Patch {
        packet_index: i32::try_from(packet_index)
            .map_err(|_| Failure::new("Overflow", "ALAC packet count overflow"))?,
        presentation_time: packet.presentation_time,
        position: packet.position,
        size: packet.size,
        sample_count: samples,
        end_bit: i64::try_from(end_bit)
            .map_err(|_| Failure::new("Overflow", "ALAC frame bit offset overflow"))?,
        previous_bits: i32::try_from(previous_bits)
            .map_err(|_| Failure::new("Overflow", "ALAC frame header overflow"))?,
    }))
}
/// Locate ALAC end-marker patches in a memory-backed packet sequence.
///
/// # Errors
/// Returns an error for invalid ALAC metadata, arithmetic overflow, or bitstream read failures.
pub fn find_patches(
    data: &[u8],
    cookie: &Cookie,
    packets: &[Packet],
) -> Result<Vec<Patch>, Failure> {
    let mut patches = Vec::new();
    let mut input = SliceBits(data);
    for (packet_index, packet) in packets.iter().enumerate() {
        let (Ok(position), Ok(packet_size)) =
            (u64::try_from(packet.position), u64::try_from(packet.size))
        else {
            continue;
        };
        let data_size = data.len() as u64;
        if packet.size < 4 || position > data_size || packet_size > data_size - position {
            continue;
        }
        if let Some(patch) = packet_patch(packet_index, packet, cookie, &mut input)? {
            patches.push(patch);
        }
    }
    Ok(patches)
}
fn find_patches_file_impl(
    path: &str,
    cookie: &Cookie,
    packets: &[Packet],
    mut cancelled: impl FnMut() -> bool,
) -> Result<Vec<Patch>, Failure> {
    let mut file = identity::open_read(path)?;
    let size = file.metadata()?.len();
    let mut patches = Vec::new();
    for (packet_index, packet) in packets.iter().enumerate() {
        if cancelled() {
            return Err(Failure::new("Cancelled", "Source preparation cancelled"));
        }
        let (Ok(position), Ok(packet_size)) =
            (u64::try_from(packet.position), u64::try_from(packet.size))
        else {
            continue;
        };
        if packet.size < 4 || position > size || packet_size > size - position {
            continue;
        }
        let mut input = FileBits {
            file: &mut file,
            position,
            size: packet_size,
        };
        if let Some(patch) = packet_patch(packet_index, packet, cookie, &mut input)? {
            patches.push(patch);
        }
    }
    Ok(patches)
}
pub fn find_patches_file(
    path: &str,
    cookie: &Cookie,
    packets: &[Packet],
) -> Result<Vec<Patch>, Failure> {
    find_patches_file_impl(path, cookie, packets, || false)
}
fn find_patches_file_cancellable(
    path: &str,
    cookie: &Cookie,
    packets: &[Packet],
    events: &mut dyn Callbacks,
) -> Result<Vec<Patch>, Failure> {
    find_patches_file_impl(path, cookie, packets, || events.cancelled())
}
pub fn apply_bits(
    data: &mut [u8],
    positions: impl IntoIterator<Item = (i64, i64)>,
) -> Result<(), Failure> {
    for (position, end_bit) in positions {
        if position < 0 || end_bit < 0 {
            return Err(Failure::new(
                "InvalidData",
                "ALAC patch bit position is negative",
            ));
        }
        let base = position
            .checked_mul(8)
            .and_then(|v| v.checked_add(end_bit))
            .ok_or_else(|| Failure::new("Overflow", "ALAC patch bit position overflow"))?;
        let end = base
            .checked_add(3)
            .ok_or_else(|| Failure::new("Overflow", "ALAC patch bit position overflow"))?;
        let end_u64 = u64::try_from(end)
            .map_err(|_| Failure::new("Overflow", "ALAC patch bit position overflow"))?;
        if end_u64 > data.len() as u64 * 8 {
            return Err(Failure::new(
                "InvalidData",
                "ALAC patch exceeds the file boundary",
            ));
        }
        for bit in base..end {
            let bit = usize::try_from(bit)
                .map_err(|_| Failure::new("Overflow", "ALAC patch bit position overflow"))?;
            data[bit / 8] |= 1 << (7 - bit % 8);
        }
    }
    Ok(())
}
fn apply_file_impl(
    source: &str,
    destination: &Path,
    positions: impl IntoIterator<Item = (i64, i64)>,
    mut cancelled: impl FnMut() -> bool,
) -> Result<u64, Failure> {
    let mut input = identity::open_read(source)?;
    let size = input.metadata()?.len();
    let bits = size
        .checked_mul(8)
        .ok_or_else(|| Failure::new("Overflow", "ALAC file bit count overflow"))?;
    let mut masks: Vec<(u64, u8)> = Vec::new();
    for (position, end_bit) in positions {
        if position < 0 || end_bit < 0 {
            return Err(Failure::new(
                "InvalidData",
                "ALAC patch bit position is negative",
            ));
        }
        let base = position
            .checked_mul(8)
            .and_then(|v| v.checked_add(end_bit))
            .ok_or_else(|| Failure::new("Overflow", "ALAC patch bit position overflow"))?;
        let end = base
            .checked_add(3)
            .ok_or_else(|| Failure::new("Overflow", "ALAC patch bit position overflow"))?;
        let end_u64 = u64::try_from(end)
            .map_err(|_| Failure::new("Overflow", "ALAC patch bit position overflow"))?;
        if end_u64 > bits {
            return Err(Failure::new(
                "InvalidData",
                "ALAC patch exceeds the file boundary",
            ));
        }
        for bit in base..end {
            let byte = u64::try_from(bit / 8)
                .map_err(|_| Failure::new("Overflow", "ALAC patch byte position overflow"))?;
            masks.push((byte, 1 << (7 - bit % 8)));
        }
    }
    masks.sort_by_key(|(byte, _)| *byte);
    let mut combined: Vec<(u64, u8)> = Vec::with_capacity(masks.len());
    for (byte, mask) in masks {
        if let Some((current_byte, current_mask)) = combined.last_mut()
            && *current_byte == byte
        {
            *current_mask |= mask;
        } else {
            combined.push((byte, mask));
        }
    }
    if let Some(parent) = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = media::temporary_path(destination);
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(&temporary)?;
    let mut owned = media::Temporary(Some(temporary.clone()));
    let result = (|| -> Result<(), Failure> {
        let mut buffer = [0u8; 65536];
        let mut position = 0u64;
        let mut patch = 0usize;
        while position < size {
            if cancelled() {
                return Err(Failure::new("Cancelled", "Source preparation cancelled"));
            }
            let count = (size - position).min(buffer.len() as u64) as usize;
            input.read_exact(&mut buffer[..count])?;
            while patch < combined.len()
                && combined[patch].0 >= position
                && combined[patch].0 < position + count as u64
            {
                let (byte, mask) = combined[patch];
                buffer[(byte - position) as usize] |= mask;
                patch += 1;
            }
            output.write_all(&buffer[..count])?;
            position += count as u64;
        }
        Ok(())
    })();
    drop(output);
    drop(input);
    let result = result.and_then(|()| {
        if cancelled() {
            return Err(Failure::new("Cancelled", "Source preparation cancelled"));
        }
        files::move_file(&temporary, destination, true)?;
        owned.0 = None;
        Ok(size)
    });
    owned.clean().map_err(Failure::from).and(result)
}
pub fn apply_patches_file(
    source: &str,
    destination: &Path,
    positions: impl IntoIterator<Item = (i64, i64)>,
) -> Result<u64, Failure> {
    apply_file_impl(source, destination, positions, || false)
}
fn apply_patches_file_cancellable(
    source: &str,
    destination: &Path,
    positions: impl IntoIterator<Item = (i64, i64)>,
    events: &mut dyn Callbacks,
) -> Result<u64, Failure> {
    apply_file_impl(source, destination, positions, || events.cancelled())
}
fn supported(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|e| {
        ["m4a", "mp4", "alac"]
            .iter()
            .any(|ext| e.eq_ignore_ascii_case(ext))
    })
}
fn read_cookie_file(path: &str, events: &mut dyn Callbacks) -> Result<Option<Cookie>, Failure> {
    check_cancel(events)?;
    let mut file = identity::open_read(path)?;
    let mut buffer = [0u8; 65536];
    let mut carry = Vec::new();
    let mut total = 0u64;
    loop {
        check_cancel(events)?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            return Ok(None);
        }
        let base = total - carry.len() as u64;
        let mut combined = Vec::with_capacity(carry.len() + count);
        combined.extend_from_slice(&carry);
        combined.extend_from_slice(&buffer[..count]);
        if let Some(mut cookie) = read_cookie(&combined) {
            let offset = base + cookie.offset as u64;
            cookie.offset = i32::try_from(offset)
                .map_err(|_| Failure::new("Overflow", "ALAC magic cookie position overflow"))?;
            return Ok(Some(cookie));
        }
        total += count as u64;
        let keep = 35.min(combined.len());
        carry = combined[combined.len() - keep..].to_vec();
    }
}
struct Packets<'a> {
    events: &'a mut dyn Callbacks,
    packets: Vec<Packet>,
}
impl Callbacks for Packets<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        if stream != 1 {
            self.events.emit(stream, text);
            return;
        }
        for line in text.split('\n') {
            let fields: Vec<_> = line.trim().split(',').collect();
            if fields.len() < 4 {
                continue;
            }
            if let (Ok(size), Ok(position)) = (fields[2].trim().parse(), fields[3].trim().parse()) {
                self.packets.push(Packet {
                    presentation_time: fields[0].trim().parse().unwrap_or(0.0),
                    size,
                    position,
                });
            }
        }
    }
    fn cancelled(&mut self) -> bool {
        self.events.cancelled()
    }
}
pub fn packets(
    library: &Path,
    path: &str,
    events: &mut dyn Callbacks,
) -> Result<Vec<Packet>, Failure> {
    let mut capture = Packets {
        events,
        packets: Vec::new(),
    };
    let result = media::execute(
        media::Job {
            library: library.into(),
            replace: false,
            timeout_millis: None,
            request: Request {
                operation: Operation::Packets,
                rate: 0,
                bits: 0,
                output_format: OutputFormat::None,
                soxr: false,
                dither: false,
                compression: 8,
                cover: false,
                input: path.into(),
                output: None,
                tags: Vec::new(),
            },
        },
        &mut capture,
    );
    if let Some(failure) = result.failure {
        return Err(failure);
    }
    if result.exit_code != Some(0) {
        return Err(Failure::new(
            "InvalidData",
            &format!(
                "ffprobe 无法枚举 ALAC 包（退出码 {}）: {path}",
                result.exit_code.unwrap_or(1)
            ),
        ));
    }
    Ok(capture.packets)
}
pub fn inspect(
    library: &Path,
    path: &str,
    events: &mut dyn Callbacks,
) -> Result<Inspection, Failure> {
    if !supported(path) {
        return Err(Failure::new(
            "InvalidData",
            &format!("不支持的 ALAC 容器扩展名: {path}"),
        ));
    }
    let cookie = read_cookie_file(path, events)?.ok_or_else(|| {
        Failure::new(
            "InvalidData",
            "无法读取 ALAC magic cookie，可能不是 ALAC 文件",
        )
    })?;
    let packets = packets(library, path, events)?;
    let patches = find_patches_file_cancellable(path, &cookie, &packets, events)?;
    check_cancel(events)?;
    Ok(Inspection { cookie, patches })
}
pub fn try_repair(
    library: &Path,
    path: &str,
    directory: &Path,
    events: &mut dyn Callbacks,
) -> Result<Option<Repair>, Failure> {
    if !supported(path) {
        return Ok(None);
    }
    let Some(cookie) = read_cookie_file(path, events)? else {
        return Ok(None);
    };
    let packets = packets(library, path, events)?;
    let patches = find_patches_file_cancellable(path, &cookie, &packets, events)?;
    if patches.is_empty() {
        return Ok(None);
    }
    let output = directory.join(Path::new(path).file_name().unwrap());
    if files::ordinal_ignore_case(
        &std::path::absolute(path)?.to_string_lossy(),
        &std::path::absolute(&output)?.to_string_lossy(),
    ) {
        return Err(Failure::new(
            "Io",
            "ALAC repair destination must differ from the source",
        ));
    }
    apply_patches_file_cancellable(
        path,
        &output,
        patches.iter().map(|p| (p.position, p.end_bit)),
        events,
    )?;
    check_cancel(events)?;
    Ok(Some(Repair {
        output_path: output.to_string_lossy().into_owned(),
        cookie,
        patches,
    }))
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    pub library: PathBuf,
    pub input: String,
    pub directory: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Outcome {
    pub exit_code: Option<i32>,
    pub failure: Option<Failure>,
    pub data: serde_json::Value,
}
pub fn execute(job: Job, events: &mut dyn Callbacks) -> Outcome {
    let result = if let Some(directory) = job.directory {
        try_repair(&job.library, &job.input, &directory, events)
            .map(|value| serde_json::json!(value))
    } else {
        inspect(&job.library, &job.input, events).map(|value| serde_json::json!(value))
    };
    match result {
        Ok(data) => Outcome {
            exit_code: Some(0),
            failure: None,
            data,
        },
        Err(failure) => Outcome {
            exit_code: None,
            failure: Some(failure),
            data: serde_json::Value::Null,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    struct Events;
    impl Callbacks for Events {
        fn emit(&mut self, _: i32, _: &str) {}
        fn cancelled(&mut self) -> bool {
            false
        }
    }
    fn scratch() -> Scratch {
        let path = std::env::temp_dir().join(format!(
            "dvda-alac-stream-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Scratch(path)
    }

    #[test]
    fn cookie_scan_preserves_matches_across_buffer_boundaries() {
        let root = scratch();
        let path = root.0.join("boundary.m4a");
        let mut data = vec![0u8; 65536 + 128];
        let marker = 65530;
        data[marker..marker + 4].copy_from_slice(b"alac");
        let cookie = marker + 8;
        data[cookie..cookie + 4].copy_from_slice(&512i32.to_be_bytes());
        data[cookie + 5] = 24;
        data[cookie + 9] = 2;
        data[cookie + 12..cookie + 16].copy_from_slice(&1024i32.to_be_bytes());
        data[cookie + 20..cookie + 24].copy_from_slice(&48000i32.to_be_bytes());
        std::fs::write(&path, data).unwrap();
        let cookie = read_cookie_file(path.to_str().unwrap(), &mut Events)
            .unwrap()
            .unwrap();
        assert_eq!(cookie.offset as usize, marker + 8);
        assert_eq!(cookie.max_samples_per_frame, 512);
        assert_eq!(cookie.channels, 2);
    }

    #[test]
    fn file_patching_matches_memory_and_cancellation_preserves_output() {
        let root = scratch();
        let source = root.0.join("source.m4a");
        let destination = root.0.join("repaired.m4a");
        let original: Vec<u8> = (0..65552).map(|index| index as u8).collect();
        std::fs::write(&source, &original).unwrap();
        let positions = [(65535, 6), (10, 0), (65535, 6)];
        let mut expected = original.clone();
        apply_bits(&mut expected, positions).unwrap();
        apply_file_impl(source.to_str().unwrap(), &destination, positions, || false).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), expected);

        let mut calls = 0;
        let error = apply_file_impl(source.to_str().unwrap(), &destination, [(0, 0)], || {
            calls += 1;
            calls >= 3
        })
        .unwrap_err();
        assert_eq!(error.kind, "Cancelled");
        assert_eq!(std::fs::read(&destination).unwrap(), expected);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 2);
    }
}
