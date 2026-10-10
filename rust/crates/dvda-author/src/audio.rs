//! Streaming integer WAVE and MLP input parsing; no media decoder is required.
use crate::{Callbacks, Silent};
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
};

pub const CHANNELS: [u8; 21] = [
    1, 2, 3, 4, 3, 4, 5, 3, 4, 5, 4, 5, 6, 4, 5, 4, 5, 6, 5, 5, 6,
];
const MASKS: [u32; 21] = [
    4, 3, 0x103, 0x33, 0xb, 0x10b, 0x3b, 7, 0x107, 0x37, 0xf, 0x10f, 0x3f, 0x107, 0x37, 0xf, 0x10f,
    0x3f, 0x3b, 0x37, 0x3b,
];
const DEFAULT_CGA: [u8; 6] = [0, 1, 7, 3, 9, 12];

#[derive(Clone, Copy, Debug)]
pub struct MlpSector {
    pub offset: u64,
    pub samples: u64,
}
#[derive(Clone, Debug)]
pub struct AudioInfo {
    pub mlp: bool,
    pub bits: u8,
    pub container_bits: u8,
    pub rate: u32,
    pub channels: u8,
    pub channel_assignment: u8,
    pub samples: u64,
    pub data_offset: u64,
    pub data_bytes: u64,
    pub mlp_layout: Vec<MlpSector>,
}
impl AudioInfo {
    pub fn frame_samples(&self) -> u32 {
        40 * (self.rate
            / if self.rate.is_multiple_of(44100) {
                44100
            } else {
                48000
            })
    }
    pub fn frame_bytes(&self) -> u64 {
        u64::from(self.frame_samples()) * u64::from(self.channels) * u64::from(self.bits) / 8
    }
    pub fn pts_length(&self) -> Result<u32, String> {
        let ticks =
            (u128::from(self.samples) * 90000 + u128::from(self.rate) / 2) / u128::from(self.rate);
        ticks
            .try_into()
            .map_err(|_| "Audio duration exceeds the 32-bit DVD PTS domain".into())
    }
    pub fn same_format(&self, other: &Self) -> bool {
        (
            self.mlp,
            self.bits,
            self.rate,
            self.channels,
            self.channel_assignment,
        ) == (
            other.mlp,
            other.bits,
            other.rate,
            other.channels,
            other.channel_assignment,
        )
    }
}
fn le16(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes(b[p..p + 2].try_into().unwrap())
}
fn le32(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}
fn validate(info: &AudioInfo) -> Result<(), String> {
    if !(1..=6).contains(&info.channels)
        || ![16, 20, 24].contains(&info.bits)
        || ![44100, 48000, 88200, 96000, 176400, 192000].contains(&info.rate)
    {
        return Err(
            "DVD audio requires 1..6 channels, 16/20/24 bits and a supported sample rate".into(),
        );
    }
    if !info.mlp
        && u64::from(info.rate) * u64::from(info.bits) * u64::from(info.channels) > 9_600_000
    {
        return Err(
            "LPCM exceeds the DVD-Audio 9.6 Mbit/s bandwidth; encode it as MLP first".into(),
        );
    }
    info.pts_length()?;
    Ok(())
}
pub fn probe(path: &Path) -> Result<AudioInfo, String> {
    probe_with_callbacks(path, &mut Silent)
}
pub fn probe_with_callbacks(
    path: &Path,
    callbacks: &mut dyn Callbacks,
) -> Result<AudioInfo, String> {
    let mut input = BufReader::with_capacity(
        65536,
        File::open(path).map_err(|e| format!("{}: {e}", path.display()))?,
    );
    let size = input.get_ref().metadata().map_err(|e| e.to_string())?.len();
    let mut head = [0; 12];
    input
        .read_exact(&mut head)
        .map_err(|e| format!("Audio header: {e}"))?;
    input.rewind().map_err(|e| e.to_string())?;
    let info = if &head[..4] == b"RIFF" || &head[..4] == b"RF64" {
        wave(&mut input, size)?
    } else if head[4..8] == [0xf8, 0x72, 0x6f, 0xbb] {
        mlp(&mut input, size, callbacks)?
    } else {
        return Err("Input must be integer WAVE or DVD MLP".into());
    };
    validate(&info)?;
    Ok(info)
}
fn wave(input: &mut (impl Read + Seek), size: u64) -> Result<AudioInfo, String> {
    let mut head = [0; 12];
    input.read_exact(&mut head).map_err(|e| e.to_string())?;
    if &head[8..12] != b"WAVE" {
        return Err("Invalid WAVE signature".into());
    }
    let rf64 = &head[..4] == b"RF64";
    let mut end = if rf64 {
        size
    } else {
        u64::from(le32(&head, 4)) + 8
    };
    if end != size {
        return Err("RIFF size does not match the input file".into());
    }
    let mut format = None;
    let mut data = None;
    let mut rf_data = None;
    let mut position = 12;
    while position + 8 <= end {
        input
            .seek(SeekFrom::Start(position))
            .map_err(|e| e.to_string())?;
        let mut chunk = [0; 8];
        input.read_exact(&mut chunk).map_err(|e| e.to_string())?;
        let mut length = u64::from(le32(&chunk, 4));
        let payload = position + 8;
        if &chunk[..4] == b"data" && rf64 && length == u64::from(u32::MAX) {
            length = rf_data.ok_or("RF64 data requires a preceding ds64 chunk")?;
        }
        let next = payload
            .checked_add(length)
            .and_then(|n| n.checked_add(length & 1))
            .ok_or("WAVE chunk size overflow")?;
        if next > end {
            return Err("Truncated WAVE chunk".into());
        }
        if &chunk[..4] == b"ds64" && rf64 {
            if length < 28 {
                return Err("Short RF64 ds64 chunk".into());
            }
            let mut value = [0; 28];
            input.read_exact(&mut value).map_err(|e| e.to_string())?;
            end = u64::from_le_bytes(value[..8].try_into().unwrap())
                .checked_add(8)
                .ok_or("RF64 size overflow")?;
            if end != size {
                return Err("RF64 size does not match input file".into());
            }
            rf_data = Some(u64::from_le_bytes(value[8..16].try_into().unwrap()));
        } else if &chunk[..4] == b"fmt " {
            if format.is_some() || length < 16 {
                return Err("Invalid/duplicate WAVE format chunk".into());
            }
            let mut fmt = vec![0; length.min(40) as usize];
            input.read_exact(&mut fmt).map_err(|e| e.to_string())?;
            let tag = le16(&fmt, 0);
            let channels = le16(&fmt, 2);
            let rate = le32(&fmt, 4);
            let align = le16(&fmt, 12);
            let container = le16(&fmt, 14);
            let mut bits = container;
            let mut mask = 0;
            if tag == 0xfffe {
                if length < 40
                    || le16(&fmt, 16) < 22
                    || fmt[24..40] != [1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]
                {
                    return Err("Unsupported WAVE extensible subtype".into());
                }
                bits = le16(&fmt, 18);
                mask = le32(&fmt, 20);
            } else if tag != 1 {
                return Err("WAVE input must use integer PCM".into());
            }
            if !(1..=6).contains(&channels)
                || ![16, 24, 32].contains(&container)
                || ![16, 20, 24].contains(&bits)
                || bits > container
                || align != channels * (container / 8)
            {
                return Err("Invalid PCM sample format/block alignment".into());
            }
            let cga = MASKS
                .iter()
                .position(|&m| m == mask)
                .map_or(DEFAULT_CGA[channels as usize - 1], |i| i as u8);
            if CHANNELS[cga as usize] != channels as u8 {
                return Err("WAVE speaker mask does not match its channel count".into());
            }
            format = Some((
                bits as u8,
                container as u8,
                rate,
                channels as u8,
                cga,
                align,
            ));
        } else if &chunk[..4] == b"data" {
            if data.is_some() {
                return Err("Multiple WAVE data chunks are unsupported".into());
            }
            data = Some((payload, length));
        }
        position = next;
    }
    if position != end {
        return Err("Trailing partial WAVE chunk".into());
    }
    let (bits, container_bits, rate, channels, channel_assignment, align) =
        format.ok_or("Missing WAVE fmt chunk")?;
    let (data_offset, data_bytes) = data.ok_or("Missing WAVE data chunk")?;
    if data_bytes == 0 || !data_bytes.is_multiple_of(u64::from(align)) {
        return Err("WAVE data must contain complete PCM sample frames".into());
    }
    Ok(AudioInfo {
        mlp: false,
        bits,
        container_bits,
        rate,
        channels,
        channel_assignment,
        samples: data_bytes / u64::from(align),
        data_offset,
        data_bytes,
        mlp_layout: vec![],
    })
}
fn mlp(
    input: &mut (impl Read + Seek),
    size: u64,
    callbacks: &mut dyn Callbacks,
) -> Result<AudioInfo, String> {
    let mut packet = vec![0; 8190];
    let mut position = 0;
    let mut count = 0u64;
    let mut samples = 0u64;
    let mut format = None;
    let mut layout = vec![];
    let mut header_offset = 64u64;
    let mut last_rank = 0u64;
    let mut last_position = 0;
    let mut previous_timing = None;
    let mut substreams = 0usize;
    while position < size {
        if count.is_multiple_of(1024) && callbacks.cancelled() {
            return Err("Authoring cancelled".into());
        }
        input
            .read_exact(&mut packet[..4])
            .map_err(|e| format!("Truncated MLP AU header: {e}"))?;
        let length = usize::from(u16::from_be_bytes(packet[..2].try_into().unwrap()) & 0xfff) * 2;
        if length < 6 || position + length as u64 > size {
            return Err("Invalid/truncated MLP access unit".into());
        }
        input
            .read_exact(&mut packet[4..length])
            .map_err(|e| e.to_string())?;
        let major = length >= 32 && packet[4..8] == [0xf8, 0x72, 0x6f, 0xbb];
        if major {
            let mut crc = 0u16;
            for byte in &packet[4..28] {
                crc ^= u16::from(*byte) << 8;
                for _ in 0..8 {
                    crc = if crc & 0x8000 != 0 {
                        crc.wrapping_shl(1) ^ 0x2d
                    } else {
                        crc.wrapping_shl(1)
                    };
                }
            }
            crc ^= u16::from_be_bytes(packet[28..30].try_into().unwrap());
            if crc != u16::from_be_bytes(packet[30..32].try_into().unwrap()) {
                return Err("MLP major-sync checksum mismatch".into());
            }
            substreams = usize::from(packet[20] >> 4);
            if !(1..=4).contains(&substreams) {
                return Err("Invalid MLP substream count".into());
            }
            let bits = match packet[8] >> 4 {
                0 => 16,
                1 => 20,
                2 => 24,
                _ => return Err("Invalid MLP quantization".into()),
            };
            let rate = match packet[9] >> 4 {
                0 => 48000,
                1 => 96000,
                2 => 192000,
                8 => 44100,
                9 => 88200,
                10 => 176400,
                _ => return Err("Invalid MLP sample rate".into()),
            };
            let cga = packet[11] & 31;
            let channels = *CHANNELS
                .get(cga as usize)
                .ok_or("Invalid MLP channel assignment")?;
            let current = (bits, rate, channels, cga);
            if format.is_some_and(|old| old != current) {
                return Err("MLP format changes within one file".into());
            }
            format = Some(current);
        }
        let (_, rate, _, _) = format.ok_or("MLP file must begin with a major sync access unit")?;
        let header = if major { 32 } else { 4 };
        let end = header + substreams * 2;
        if end > length {
            return Err("Truncated MLP substream header".into());
        }
        let mut parity = packet[..4].iter().fold(0u8, |a, &b| a ^ b);
        for bytes in packet[header..end].as_chunks::<2>().0 {
            if bytes[0] & 0x80 != 0 {
                return Err("MLP extra-word substream headers are invalid".into());
            }
            parity ^= bytes[0] ^ bytes[1];
            let length_end = usize::from(u16::from_be_bytes(*bytes) & 0xfff) * 2;
            if end + length_end > length {
                return Err("MLP substream extends beyond its access unit".into());
            }
        }
        if ((parity >> 4) ^ parity) & 15 != 15 {
            return Err("MLP access-unit header parity mismatch".into());
        }
        let frame = 40 * (rate / if rate % 44100 == 0 { 44100 } else { 48000 });
        let timing = u16::from_be_bytes(packet[2..4].try_into().unwrap());
        if previous_timing
            .is_some_and(|previous: u16| timing.wrapping_sub(previous) != frame as u16)
        {
            return Err("MLP input timing is discontinuous".into());
        }
        previous_timing = Some(timing);
        let rank = (position + header_offset - 1) / 2048;
        if count == 0 || rank != last_rank {
            if rank != last_rank {
                header_offset += 43;
            }
            layout.push(MlpSector {
                offset: position,
                samples,
            });
        }
        last_rank = rank;
        last_position = position;
        samples += u64::from(frame);
        position += length as u64;
        count += 1;
    }
    layout.push(MlpSector {
        offset: last_position,
        samples,
    });
    let (bits, rate, channels, channel_assignment) = format.ok_or("Empty MLP input")?;
    Ok(AudioInfo {
        mlp: true,
        bits,
        container_bits: bits,
        rate,
        channels,
        channel_assignment,
        samples,
        data_offset: 0,
        data_bytes: size,
        mlp_layout: layout,
    })
}

const PERM16: [&[usize]; 6] = [
    &[1, 0, 3, 2],
    &[1, 0, 3, 2, 5, 4, 7, 6],
    &[5, 4, 11, 10, 1, 0, 3, 2, 7, 6, 9, 8],
    &[5, 4, 7, 6, 13, 12, 15, 14, 1, 0, 3, 2, 9, 8, 11, 10],
    &[
        7, 6, 9, 8, 17, 16, 19, 18, 1, 0, 3, 2, 5, 4, 11, 10, 13, 12, 15, 14,
    ],
    &[
        7, 6, 9, 8, 11, 10, 19, 18, 21, 20, 23, 22, 1, 0, 3, 2, 5, 4, 13, 12, 15, 14, 17, 16,
    ],
];
const PERM24: [&[usize]; 6] = [
    &[2, 1, 5, 4, 0, 3],
    &[2, 1, 5, 4, 8, 7, 11, 10, 0, 3, 6, 9],
    &[8, 7, 17, 16, 6, 15, 2, 1, 5, 4, 11, 10, 14, 13, 0, 3, 9, 12],
    &[
        8, 7, 11, 10, 20, 19, 23, 22, 6, 9, 18, 21, 2, 1, 5, 4, 14, 13, 17, 16, 0, 3, 12, 15,
    ],
    &[
        11, 10, 14, 13, 26, 25, 29, 28, 9, 12, 24, 27, 2, 1, 5, 4, 8, 7, 17, 16, 20, 19, 23, 22, 0,
        3, 6, 15, 18, 21,
    ],
    &[
        8, 7, 11, 10, 26, 25, 29, 28, 6, 9, 24, 27, 2, 1, 5, 4, 14, 13, 17, 16, 20, 19, 23, 22, 32,
        31, 35, 34, 0, 3, 12, 15, 18, 21, 30, 33,
    ],
];
/// Convert two interleaved little-endian sample frames into DVD-Audio byte order.
/// Twenty valid bits occupy the most significant bits of each input 24-bit word.
pub fn pack_pcm_pair(input: &[u8], bits: u8, channels: u8) -> Result<Vec<u8>, String> {
    if !(1..=6).contains(&channels) || ![16, 20, 24].contains(&bits) {
        return Err("Invalid LPCM packing format".into());
    }
    let width = if bits == 16 { 2 } else { 3 };
    let expected = 2 * usize::from(channels) * width;
    if input.len() != expected {
        return Err("LPCM packing requires two complete sample frames".into());
    }
    let mut output = Vec::with_capacity(expected);
    append_pcm_pair(input, bits, channels, &mut output);
    Ok(output)
}
pub(crate) fn append_pcm_pair(input: &[u8], bits: u8, channels: u8, output: &mut Vec<u8>) {
    let permutation = if bits == 16 {
        PERM16[channels as usize - 1]
    } else {
        PERM24[channels as usize - 1]
    };
    if bits == 20 {
        let group2 = [0, 0, 1, 2, 2, 2][channels as usize - 1];
        let mut position = 0;
        for group in [group2, usize::from(channels) - group2] {
            let high = group * 4;
            let indices = &permutation[position..position + group * 6];
            output.extend(indices[..high].iter().map(|&index| input[index]));
            output.extend(
                indices[high..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| (input[pair[0]] & 0xf0) | (input[pair[1]] >> 4)),
            );
            position += group * 6;
        }
    } else {
        output.extend(permutation.iter().map(|&index| input[index]));
    }
}
