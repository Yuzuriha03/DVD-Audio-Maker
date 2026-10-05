//! ALAC inspection and repair; source bytes are never rewritten.
use super::{models::Patch, state};
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
    io::Read,
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
            8000, 11025, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000, 176400,
            192000, 352800, 384000,
        ]
        .contains(&cookie.sample_rate))
    .then_some(cookie)
}
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
pub fn find_patches(
    data: &[u8],
    cookie: &Cookie,
    packets: &[Packet],
) -> Result<Vec<Patch>, Failure> {
    let mut patches = Vec::new();
    for (packet_index, packet) in packets.iter().enumerate() {
        if packet.size < 4
            || packet.position < 0
            || packet.position as u64 > data.len() as u64
            || packet.size as u64 > data.len() as u64 - packet.position as u64
        {
            continue;
        }
        let frame =
            &data[packet.position as usize..packet.position as usize + packet.size as usize];
        if frame[2] & 2 == 0 {
            continue;
        }
        let mut samples = if frame[2] & 0x10 != 0 && frame.len() >= 7 {
            i32::try_from(bits(frame, 22, 32).unwrap())
                .map_err(|_| Failure::new("Overflow", "ALAC sample count overflow"))?
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
            continue;
        }
        let previous_bits = bits(frame, end_bit as usize, 3).unwrap() as i32;
        if previous_bits != 7 {
            patches.push(Patch {
                packet_index: i32::try_from(packet_index)
                    .map_err(|_| Failure::new("Overflow", "ALAC packet count overflow"))?,
                presentation_time: packet.presentation_time,
                position: packet.position,
                size: packet.size,
                sample_count: samples,
                end_bit,
                previous_bits,
            });
        }
    }
    Ok(patches)
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
        if end as u64 > data.len() as u64 * 8 {
            return Err(Failure::new(
                "InvalidData",
                "ALAC patch exceeds the file boundary",
            ));
        }
        for bit in base..end {
            data[bit as usize / 8] |= 1 << (7 - bit % 8);
        }
    }
    Ok(())
}
fn supported(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|e| {
        ["m4a", "mp4", "alac"]
            .iter()
            .any(|ext| e.eq_ignore_ascii_case(ext))
    })
}
fn read(path: &str, events: &mut dyn Callbacks) -> Result<Vec<u8>, Failure> {
    check_cancel(events)?;
    let mut file = identity::open_read(path)?;
    let mut data = Vec::new();
    let mut buffer = [0u8; 65536];
    loop {
        check_cancel(events)?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..n]);
    }
    Ok(data)
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
    let data = read(path, events)?;
    let cookie = read_cookie(&data).ok_or_else(|| {
        Failure::new(
            "InvalidData",
            "无法读取 ALAC magic cookie，可能不是 ALAC 文件",
        )
    })?;
    let packets = packets(library, path, events)?;
    let patches = find_patches(&data, &cookie, &packets)?;
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
    let mut data = read(path, events)?;
    let Some(cookie) = read_cookie(&data) else {
        return Ok(None);
    };
    let packets = packets(library, path, events)?;
    let patches = find_patches(&data, &cookie, &packets)?;
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
    apply_bits(&mut data, patches.iter().map(|p| (p.position, p.end_bit)))?;
    check_cancel(events)?;
    state::write_bytes(&output, &data)?;
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
