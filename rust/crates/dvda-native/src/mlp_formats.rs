use super::{MlpAlignment, MlpInspection};
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

const EOS: [u8; 4] = [0xd2, 0x34, 0xd2, 0x34];
const INVALID: &str = "MLP inspection failed: -1";
// Tables ported from tools/formats-native/dvda-formats.c (AV-style CRC-16/CRC-8).
const CRC16_TABLE: [u16; 256] = [
    0x0000, 0x2d00, 0x5a00, 0x7700, 0xb400, 0x9900, 0xee00, 0xc300, 0x6801, 0x4501, 0x3201, 0x1f01,
    0xdc01, 0xf101, 0x8601, 0xab01, 0xd002, 0xfd02, 0x8a02, 0xa702, 0x6402, 0x4902, 0x3e02, 0x1302,
    0xb803, 0x9503, 0xe203, 0xcf03, 0x0c03, 0x2103, 0x5603, 0x7b03, 0xa005, 0x8d05, 0xfa05, 0xd705,
    0x1405, 0x3905, 0x4e05, 0x6305, 0xc804, 0xe504, 0x9204, 0xbf04, 0x7c04, 0x5104, 0x2604, 0x0b04,
    0x7007, 0x5d07, 0x2a07, 0x0707, 0xc407, 0xe907, 0x9e07, 0xb307, 0x1806, 0x3506, 0x4206, 0x6f06,
    0xac06, 0x8106, 0xf606, 0xdb06, 0x400b, 0x6d0b, 0x1a0b, 0x370b, 0xf40b, 0xd90b, 0xae0b, 0x830b,
    0x280a, 0x050a, 0x720a, 0x5f0a, 0x9c0a, 0xb10a, 0xc60a, 0xeb0a, 0x9009, 0xbd09, 0xca09, 0xe709,
    0x2409, 0x0909, 0x7e09, 0x5309, 0xf808, 0xd508, 0xa208, 0x8f08, 0x4c08, 0x6108, 0x1608, 0x3b08,
    0xe00e, 0xcd0e, 0xba0e, 0x970e, 0x540e, 0x790e, 0x0e0e, 0x230e, 0x880f, 0xa50f, 0xd20f, 0xff0f,
    0x3c0f, 0x110f, 0x660f, 0x4b0f, 0x300c, 0x1d0c, 0x6a0c, 0x470c, 0x840c, 0xa90c, 0xde0c, 0xf30c,
    0x580d, 0x750d, 0x020d, 0x2f0d, 0xec0d, 0xc10d, 0xb60d, 0x9b0d, 0x8016, 0xad16, 0xda16, 0xf716,
    0x3416, 0x1916, 0x6e16, 0x4316, 0xe817, 0xc517, 0xb217, 0x9f17, 0x5c17, 0x7117, 0x0617, 0x2b17,
    0x5014, 0x7d14, 0x0a14, 0x2714, 0xe414, 0xc914, 0xbe14, 0x9314, 0x3815, 0x1515, 0x6215, 0x4f15,
    0x8c15, 0xa115, 0xd615, 0xfb15, 0x2013, 0x0d13, 0x7a13, 0x5713, 0x9413, 0xb913, 0xce13, 0xe313,
    0x4812, 0x6512, 0x1212, 0x3f12, 0xfc12, 0xd112, 0xa612, 0x8b12, 0xf011, 0xdd11, 0xaa11, 0x8711,
    0x4411, 0x6911, 0x1e11, 0x3311, 0x9810, 0xb510, 0xc210, 0xef10, 0x2c10, 0x0110, 0x7610, 0x5b10,
    0xc01d, 0xed1d, 0x9a1d, 0xb71d, 0x741d, 0x591d, 0x2e1d, 0x031d, 0xa81c, 0x851c, 0xf21c, 0xdf1c,
    0x1c1c, 0x311c, 0x461c, 0x6b1c, 0x101f, 0x3d1f, 0x4a1f, 0x671f, 0xa41f, 0x891f, 0xfe1f, 0xd31f,
    0x781e, 0x551e, 0x221e, 0x0f1e, 0xcc1e, 0xe11e, 0x961e, 0xbb1e, 0x6018, 0x4d18, 0x3a18, 0x1718,
    0xd418, 0xf918, 0x8e18, 0xa318, 0x0819, 0x2519, 0x5219, 0x7f19, 0xbc19, 0x9119, 0xe619, 0xcb19,
    0xb01a, 0x9d1a, 0xea1a, 0xc71a, 0x041a, 0x291a, 0x5e1a, 0x731a, 0xd81b, 0xf51b, 0x821b, 0xaf1b,
    0x6c1b, 0x411b, 0x361b, 0x1b1b,
];

pub(super) fn be16(data: &[u8]) -> u16 {
    u16::from_be_bytes([data[0], data[1]])
}

fn sample_rate(major: &[u8]) -> Result<i32, ()> {
    let bits = (major[5] >> 4) & 15;
    if bits == 15 {
        return Err(());
    }
    Ok((if bits & 8 != 0 { 44100 } else { 48000 }) << (bits & 7))
}

fn peak_rate(rate: i32) -> i32 {
    if rate <= 0 {
        0
    } else {
        ((9600000i64 * 16 - 8 + i64::from(rate) - 1) / i64::from(rate)) as i32
    }
}

// Reflected CRC tables of the original AV checksum: the wire CRC is little endian.
pub(super) fn checksum16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &byte in &data[..data.len() - 2] {
        crc = CRC16_TABLE[usize::from((crc as u8) ^ byte)] ^ (crc >> 8);
    }
    crc ^ u16::from_le_bytes([data[data.len() - 2], data[data.len() - 1]])
}

const CRC8_TABLE: [u8; 256] = [
    0x00, 0x63, 0xc6, 0xa5, 0xef, 0x8c, 0x29, 0x4a, 0xbd, 0xde, 0x7b, 0x18, 0x52, 0x31, 0x94, 0xf7,
    0x19, 0x7a, 0xdf, 0xbc, 0xf6, 0x95, 0x30, 0x53, 0xa4, 0xc7, 0x62, 0x01, 0x4b, 0x28, 0x8d, 0xee,
    0x32, 0x51, 0xf4, 0x97, 0xdd, 0xbe, 0x1b, 0x78, 0x8f, 0xec, 0x49, 0x2a, 0x60, 0x03, 0xa6, 0xc5,
    0x2b, 0x48, 0xed, 0x8e, 0xc4, 0xa7, 0x02, 0x61, 0x96, 0xf5, 0x50, 0x33, 0x79, 0x1a, 0xbf, 0xdc,
    0x64, 0x07, 0xa2, 0xc1, 0x8b, 0xe8, 0x4d, 0x2e, 0xd9, 0xba, 0x1f, 0x7c, 0x36, 0x55, 0xf0, 0x93,
    0x7d, 0x1e, 0xbb, 0xd8, 0x92, 0xf1, 0x54, 0x37, 0xc0, 0xa3, 0x06, 0x65, 0x2f, 0x4c, 0xe9, 0x8a,
    0x56, 0x35, 0x90, 0xf3, 0xb9, 0xda, 0x7f, 0x1c, 0xeb, 0x88, 0x2d, 0x4e, 0x04, 0x67, 0xc2, 0xa1,
    0x4f, 0x2c, 0x89, 0xea, 0xa0, 0xc3, 0x66, 0x05, 0xf2, 0x91, 0x34, 0x57, 0x1d, 0x7e, 0xdb, 0xb8,
    0xc8, 0xab, 0x0e, 0x6d, 0x27, 0x44, 0xe1, 0x82, 0x75, 0x16, 0xb3, 0xd0, 0x9a, 0xf9, 0x5c, 0x3f,
    0xd1, 0xb2, 0x17, 0x74, 0x3e, 0x5d, 0xf8, 0x9b, 0x6c, 0x0f, 0xaa, 0xc9, 0x83, 0xe0, 0x45, 0x26,
    0xfa, 0x99, 0x3c, 0x5f, 0x15, 0x76, 0xd3, 0xb0, 0x47, 0x24, 0x81, 0xe2, 0xa8, 0xcb, 0x6e, 0x0d,
    0xe3, 0x80, 0x25, 0x46, 0x0c, 0x6f, 0xca, 0xa9, 0x5e, 0x3d, 0x98, 0xfb, 0xb1, 0xd2, 0x77, 0x14,
    0xac, 0xcf, 0x6a, 0x09, 0x43, 0x20, 0x85, 0xe6, 0x11, 0x72, 0xd7, 0xb4, 0xfe, 0x9d, 0x38, 0x5b,
    0xb5, 0xd6, 0x73, 0x10, 0x5a, 0x39, 0x9c, 0xff, 0x08, 0x6b, 0xce, 0xad, 0xe7, 0x84, 0x21, 0x42,
    0x9e, 0xfd, 0x58, 0x3b, 0x71, 0x12, 0xb7, 0xd4, 0x23, 0x40, 0xe5, 0x86, 0xcc, 0xaf, 0x0a, 0x69,
    0x87, 0xe4, 0x41, 0x22, 0x68, 0x0b, 0xae, 0xcd, 0x3a, 0x59, 0xfc, 0x9f, 0xd5, 0xb6, 0x13, 0x70,
];

pub(super) fn checksum8(data: &[u8]) -> u8 {
    let mut crc = 0x3cu8;
    for &byte in &data[..data.len() - 1] {
        crc = CRC8_TABLE[usize::from(crc ^ byte)];
    }
    crc ^ data[data.len() - 1]
}

fn parity(data: &[u8]) -> u8 {
    data.iter().fold(0, |acc, byte| acc ^ byte)
}

fn parity_nibble(timing: u16, words: usize, header: u16) -> u16 {
    let mut value =
        u32::from(timing) ^ words as u32 ^ u32::from(header >> 8) ^ u32::from(header & 255);
    value ^= value >> 8;
    value ^= value >> 4;
    (value & 15) as u16
}

#[derive(Default)]
struct State {
    size: u64,
    units: u32,
    majors: u32,
    major_errors: i32,
    parity_errors: i32,
    substream_errors: i32,
    peak: Option<i32>,
    extended: Option<i32>,
    rate: Option<i32>,
    last_eos: bool,
}

fn has_major(unit: &[u8]) -> bool {
    unit.len() >= 7 && unit[4..7] == [0xf8, 0x72, 0x6f]
}

impl State {
    fn unit(&mut self, unit: &[u8]) {
        let major = has_major(unit);
        self.last_eos = false;
        if major {
            self.majors += 1;
            if unit.len() < 32 {
                self.major_errors += 1;
            } else {
                let header = &unit[4..32];
                if be16(&header[8..]) != 0xb752 {
                    self.major_errors += 1;
                }
                if checksum16(&header[..26]) != u16::from_le_bytes([header[26], header[27]]) {
                    self.major_errors += 1;
                }
                let peak = i32::from(be16(&header[14..]) & 0x7fff);
                let extended = i32::from(header[16] & 3);
                if let Ok(rate) = sample_rate(header) {
                    if i64::from(peak) < (9600000i64 * 16) / i64::from(rate)
                        || peak > peak_rate(rate)
                    {
                        self.major_errors += 1;
                    }
                    if self.rate.is_some_and(|old| old != rate) {
                        self.major_errors += 1;
                    }
                    self.rate = Some(rate);
                } else {
                    self.major_errors += 1;
                }
                if extended != 1 {
                    self.major_errors += 1;
                }
                if self.peak.is_some_and(|old| old != peak) {
                    self.major_errors += 1;
                }
                if self.extended.is_some_and(|old| old != extended) {
                    self.major_errors += 1;
                }
                self.peak = Some(peak);
                self.extended = Some(extended);
            }
        }
        let offset = 4 + if major { 28 } else { 0 };
        if offset + 2 > unit.len() {
            self.substream_errors += 1;
            return;
        }
        let header = be16(&unit[offset..]);
        let end = usize::from(header & 0x0fff) * 2;
        let data_offset = offset + 2;
        if end < 2 || end > unit.len() - data_offset {
            self.substream_errors += 1;
            return;
        }
        let sub = &unit[data_offset..data_offset + end];
        let expected = u16::from(unit[0] >> 4);
        if (parity_nibble(be16(&unit[2..]), unit.len() / 2, header) ^ 15) != expected {
            self.parity_errors += 1;
        }
        if header & 0x2000 != 0 {
            let body = &sub[..end - 2];
            if body.is_empty()
                || parity(body) != sub[end - 2] ^ 0xa9
                || checksum8(body) != sub[end - 1]
            {
                self.substream_errors += 1;
            }
            self.last_eos = body.ends_with(&EOS);
        } else {
            self.last_eos = end >= 6 && sub[..end - 2].ends_with(&EOS);
        }
    }

    fn finish(self) -> MlpInspection {
        let rate = self.rate.unwrap_or(-1);
        let peak = self.peak.unwrap_or(-1);
        let extended = self.extended.unwrap_or(-1);
        let valid = self.units > 0
            && self.majors > 0
            && self.rate.is_some()
            && self.peak.is_some()
            && self.extended.is_some()
            && self.last_eos
            && self.major_errors == 0
            && self.parity_errors == 0
            && self.substream_errors == 0
            && i64::from(peak) >= (9600000i64 * 16) / i64::from(rate.max(1))
            && peak <= peak_rate(rate)
            && extended == 1;
        MlpInspection {
            size: self.size,
            access_unit_count: self.units,
            major_sync_count: self.majors,
            major_sync_interval: if self.majors == 0 {
                0.0
            } else {
                f64::from(self.units) / f64::from(self.majors)
            },
            major_sync_error_count: self.major_errors,
            access_unit_parity_error_count: self.parity_errors,
            substream_error_count: self.substream_errors,
            has_end_of_stream: self.last_eos,
            peak_bitrate_raw: peak,
            extended_substream_info: extended,
            sample_rate: rate,
            is_valid: valid,
            error_code: 0,
        }
    }
}

#[cfg(test)]
pub(super) fn inspect_buffer(data: &[u8]) -> Result<MlpInspection, String> {
    let mut state = State::default();
    let mut position = 0;
    while position < data.len() {
        if data.len() - position < 4 {
            return Err(INVALID.into());
        }
        let length = usize::from(be16(&data[position..]) & 0x0fff) * 2;
        if length < 4 || length > data.len() - position {
            return Err(INVALID.into());
        }
        state.unit(&data[position..position + length]);
        state.units += 1;
        state.size += length as u64;
        position += length;
    }
    Ok(state.finish())
}

pub(super) fn inspect_file(path: &Path) -> Result<MlpInspection, String> {
    let file = File::open(path).map_err(|_| "MLP inspection failed: -2".to_owned())?;
    let mut reader = io::BufReader::with_capacity(1024 * 1024, file);
    let mut header = [0u8; 4];
    let mut unit = [0u8; 0x0fff * 2];
    let mut state = State::default();
    loop {
        let count = reader
            .read(&mut header)
            .map_err(|_| "MLP inspection failed: -2".to_owned())?;
        if count == 0 {
            break;
        }
        if count < 4 {
            reader.read_exact(&mut header[count..]).map_err(|err| {
                if err.kind() == io::ErrorKind::UnexpectedEof {
                    INVALID.to_owned()
                } else {
                    "MLP inspection failed: -2".to_owned()
                }
            })?;
        }
        let length = usize::from(be16(&header) & 0x0fff) * 2;
        if length < 4 {
            return Err(INVALID.into());
        }
        unit[..4].copy_from_slice(&header);
        reader.read_exact(&mut unit[4..length]).map_err(|err| {
            if err.kind() == io::ErrorKind::UnexpectedEof {
                INVALID.to_owned()
            } else {
                "MLP inspection failed: -2".to_owned()
            }
        })?;
        state.unit(&unit[..length]);
        state.units += 1;
        state.size += length as u64;
    }
    Ok(state.finish())
}

pub(super) fn align(data: &[u8]) -> Result<MlpAlignment, String> {
    let invalid = || "MLP alignment failed: -1".to_owned();
    let mut units = Vec::new();
    let mut position = 0;
    while position < data.len() {
        if data.len() - position < 4 {
            return Err(invalid());
        }
        let length = usize::from(be16(&data[position..]) & 0x0fff) * 2;
        if length < 4 || length > data.len() - position {
            return Err(invalid());
        }
        units.push((
            position,
            length,
            has_major(&data[position..position + length]),
        ));
        position += length;
    }
    if units.is_empty() {
        return Err(invalid());
    }
    let mut output = data.to_vec();
    let (mut peaks, mut extended, mut checksums) = (0, 0, 0);
    for &(offset, _, major) in &units {
        if !major {
            continue;
        }
        if offset + 32 > output.len() {
            return Err(invalid());
        }
        let sync = &mut output[offset + 4..offset + 32];
        let rate = sample_rate(sync).map_err(|()| invalid())?;
        let wanted = peak_rate(rate) as u16;
        let old = be16(&sync[14..]);
        if old & 0x7fff != wanted {
            sync[14..16].copy_from_slice(&((old & 0x8000) | (wanted & 0x7fff)).to_be_bytes());
            peaks += 1;
        }
        if sync[16] & 3 != 1 {
            sync[16] = (sync[16] & !3) | 1;
            extended += 1;
        }
        let crc = checksum16(&sync[..26]);
        if u16::from_le_bytes([sync[26], sync[27]]) != crc {
            sync[26..28].copy_from_slice(&crc.to_le_bytes());
            checksums += 1;
        }
    }
    let (last, length, major) = *units.last().unwrap();
    let offset = last + 4 + if major { 28 } else { 0 };
    if offset + 2 > output.len() {
        return Err(invalid());
    }
    let mut header = be16(&output[offset..]);
    let end = usize::from(header & 0x0fff) * 2;
    let begin = offset + 2;
    if end < 2 || end > output.len() - begin {
        return Err(invalid());
    }
    let body_end = begin + end - 2;
    let (mut inserted, mut old_header, mut new_header) = (false, -1, -1);
    if !output[begin..body_end].ends_with(&EOS) {
        output.splice(
            body_end..body_end + 2,
            [EOS[0], EOS[1], EOS[2], EOS[3], 0, 0],
        );
        let body_end = body_end + 4;
        output[body_end] = parity(&output[begin..body_end]) ^ 0xa9;
        output[body_end + 1] = checksum8(&output[begin..body_end + 1]);
        header = (header & 0xf000) | ((header & 0x0fff).wrapping_add(2) & 0x0fff);
        output[offset..offset + 2].copy_from_slice(&header.to_be_bytes());
        let nibble = parity_nibble(be16(&output[last + 2..]), length / 2 + 2, header);
        old_header = i32::from(be16(&output[last..]));
        let updated = ((nibble ^ 15) << 12) | (((length / 2 + 2) as u16) & 0x0fff);
        output[last..last + 2].copy_from_slice(&updated.to_be_bytes());
        new_header = i32::from(updated);
        inserted = true;
    }
    Ok(MlpAlignment {
        data: output,
        peak_changes: peaks,
        extended_changes: extended,
        checksum_changes: checksums,
        inserted_end_of_stream: inserted,
        old_header,
        new_header,
    })
}
