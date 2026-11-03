//! Bounded RIFF/WAVE and RF64 PCM input.

use crate::pcm::Encoding;
use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom};

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
fn u16le(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes[..2].try_into().unwrap())
}
fn u32le(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().unwrap())
}
fn u64le(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes[..8].try_into().unwrap())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub encoding: Encoding,
    pub channels: u16,
    pub sample_rate: u32,
    pub valid_bits: u16,
    pub channel_mask: u32,
}

impl Format {
    pub fn block_align(self) -> usize {
        usize::from(self.channels) * self.encoding.bytes()
    }

    fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 16 {
            return Err(invalid("short WAVE format"));
        }
        let mut tag = u16le(bytes);
        let channels = u16le(&bytes[2..]);
        let sample_rate = u32le(&bytes[4..]);
        let align = u16le(&bytes[12..]);
        let bits = u16le(&bytes[14..]);
        let mut valid_bits = bits;
        let mut channel_mask = 0;
        if tag == 0xfffe {
            if bytes.len() < 40 || u16le(&bytes[16..]) < 22 {
                return Err(invalid("short extensible WAVE format"));
            }
            valid_bits = u16le(&bytes[18..]);
            if valid_bits == 0 {
                valid_bits = bits;
            }
            channel_mask = u32le(&bytes[20..]);
            if bytes[26..40] != [0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113] {
                return Err(invalid("unsupported WAVE subformat GUID"));
            }
            tag = u16le(&bytes[24..]);
        }
        let encoding = match (tag, bits) {
            (1, 8) => Encoding::Unsigned8,
            (1, 16) => Encoding::Signed16,
            (1, 24) => Encoding::Signed24,
            (1, 32) => Encoding::Signed32,
            (3, 32) => Encoding::Float32,
            (3, 64) => Encoding::Float64,
            _ => return Err(invalid("unsupported WAVE PCM format")),
        };
        let format = Self {
            encoding,
            channels,
            sample_rate,
            valid_bits,
            channel_mask,
        };
        if channels == 0
            || sample_rate == 0
            || valid_bits == 0
            || valid_bits > bits
            || usize::from(align) != format.block_align()
            || (encoding.is_float() && valid_bits != bits)
            || (channel_mask != 0 && channel_mask.count_ones() != u32::from(channels))
        {
            return Err(invalid("invalid WAVE PCM geometry"));
        }
        Ok(format)
    }
}

pub struct Reader<R> {
    source: R,
    pub format: Format,
    pub data_offset: u64,
    pub data_bytes: u64,
    pub tags: Vec<(String, String)>,
    remaining: u64,
}

impl<R: Read + Seek> Reader<R> {
    pub fn new(source: R) -> Result<Self> {
        Self::open(source, false)
    }

    /// Read integer RIFF PCM for authoring, without interpreting ancillary metadata.
    pub fn new_integer_riff(source: R) -> Result<Self> {
        Self::open(source, true)
    }

    fn open(mut source: R, integer_riff: bool) -> Result<Self> {
        let length = source.seek(SeekFrom::End(0))?;
        source.seek(SeekFrom::Start(0))?;
        let mut header = [0; 12];
        source.read_exact(&mut header)?;
        let rf64 = &header[..4] == b"RF64";
        if (integer_riff && rf64) || (!rf64 && &header[..4] != b"RIFF") || &header[8..] != b"WAVE" {
            return Err(invalid("not a RIFF/RF64 WAVE file"));
        }
        let mut end = if rf64 {
            length
        } else {
            u64::from(u32le(&header[4..])) + 8
        };
        if end < 12 || end > length {
            return Err(invalid("truncated WAVE container"));
        }
        let mut format = None;
        let mut data = None;
        let mut rf64_data = None;
        let mut rf64_sizes = Vec::new();
        let mut tags = Vec::new();
        let mut position = 12;
        while position < end {
            if end - position < 8 {
                return Err(invalid("truncated WAVE chunk header"));
            }
            source.seek(SeekFrom::Start(position))?;
            let mut chunk = [0; 8];
            source.read_exact(&mut chunk)?;
            let mut size = u64::from(u32le(&chunk[4..]));
            if size == u64::from(u32::MAX) && rf64 {
                size = if &chunk[..4] == b"data" && data.is_none() {
                    rf64_data.ok_or_else(|| invalid("RF64 data precedes ds64"))?
                } else {
                    let index = rf64_sizes
                        .iter()
                        .position(|(id, _)| id == &chunk[..4])
                        .ok_or_else(|| invalid("RF64 chunk missing size table entry"))?;
                    rf64_sizes.remove(index).1
                };
            }
            let start = position + 8;
            let payload_end = start
                .checked_add(size)
                .ok_or_else(|| invalid("WAVE chunk size overflow"))?;
            let next =
                if integer_riff && &chunk[..4] == b"data" && payload_end == end && end == length {
                    // Accept an unpadded terminal data chunk, but never a truncated payload.
                    payload_end
                } else {
                    payload_end
                        .checked_add(size & 1)
                        .ok_or_else(|| invalid("WAVE chunk size overflow"))?
                };
            if next > end {
                return Err(invalid("truncated WAVE chunk"));
            }
            match &chunk[..4] {
                b"ds64" if rf64 => {
                    if size < 28 || rf64_data.is_some() {
                        return Err(invalid("invalid RF64 ds64"));
                    }
                    let mut ds64 = [0; 28];
                    source.read_exact(&mut ds64)?;
                    end = u64le(&ds64)
                        .checked_add(8)
                        .ok_or_else(|| invalid("RF64 size overflow"))?;
                    if end > length || end < next || u64::from(u32le(&ds64[24..])) * 12 + 28 > size
                    {
                        return Err(invalid("invalid RF64 size table"));
                    }
                    rf64_data = Some(u64le(&ds64[8..]));
                    let entries = u32le(&ds64[24..]);
                    if entries > 65536 {
                        return Err(invalid("RF64 size table exceeds metadata limit"));
                    }
                    for _ in 0..entries {
                        let mut entry = [0; 12];
                        source.read_exact(&mut entry)?;
                        let id: [u8; 4] = entry[..4].try_into().unwrap();
                        rf64_sizes.push((id, u64le(&entry[4..])));
                    }
                }
                b"fmt " if integer_riff || format.is_none() => {
                    if format.is_some() || (integer_riff && size > 4096) {
                        return Err(invalid("Invalid or duplicate WAVE fmt chunk"));
                    }
                    let mut bytes = [0; 40];
                    let count = size.min(40) as usize;
                    source.read_exact(&mut bytes[..count])?;
                    let parsed = Format::parse(&bytes[..count])?;
                    if integer_riff
                        && !matches!(
                            parsed.encoding,
                            Encoding::Signed16 | Encoding::Signed24 | Encoding::Signed32
                        )
                    {
                        return Err(invalid("WAVE input must be integer PCM"));
                    }
                    format = Some(parsed);
                }
                b"data" if integer_riff || data.is_none() => {
                    if data.replace((start, size)).is_some() {
                        return Err(invalid("Duplicate WAVE data chunk"));
                    }
                }
                b"LIST" if size >= 4 && !integer_riff => {
                    let mut kind = [0; 4];
                    source.read_exact(&mut kind)?;
                    if &kind == b"INFO" {
                        let mut remaining = size - 4;
                        while remaining >= 8 {
                            let mut entry = [0; 8];
                            source.read_exact(&mut entry)?;
                            let count = u64::from(u32le(&entry[4..]));
                            let padded = count + (count & 1);
                            if padded > remaining - 8 {
                                return Err(invalid("truncated WAVE INFO entry"));
                            }
                            // Bound individual metadata allocation independently of the audio size.
                            if count <= 1024 * 1024 {
                                let mut value = vec![0; count as usize];
                                source.read_exact(&mut value)?;
                                let key = match &entry[..4] {
                                    b"IART" => "artist",
                                    b"ICMT" => "comment",
                                    b"ICOP" => "copyright",
                                    b"ICRD" => "date",
                                    b"IGNR" => "genre",
                                    b"INAM" => "title",
                                    b"IPRD" => "album",
                                    b"ITRK" => "track",
                                    b"ISFT" => "encoder",
                                    _ => std::str::from_utf8(&entry[..4]).unwrap_or(""),
                                };
                                if !key.is_empty() {
                                    let end =
                                        value.iter().position(|&b| b == 0).unwrap_or(value.len());
                                    tags.push((
                                        key.into(),
                                        String::from_utf8_lossy(&value[..end]).into_owned(),
                                    ));
                                }
                                source.seek(SeekFrom::Current((count & 1) as i64))?;
                            } else {
                                return Err(invalid("WAVE INFO entry exceeds metadata limit"));
                            }
                            remaining -= 8 + padded;
                        }
                        if remaining != 0 {
                            return Err(invalid("truncated WAVE INFO header"));
                        }
                    }
                }
                _ => {}
            }
            position = next;
        }
        if rf64 && rf64_data.is_none() {
            return Err(invalid("RF64 has no ds64"));
        }
        let format = format.ok_or_else(|| invalid("WAVE has no format"))?;
        let (data_offset, data_bytes) = data.ok_or_else(|| invalid("WAVE has no data"))?;
        if !data_bytes.is_multiple_of(format.block_align() as u64) {
            return Err(invalid("incomplete WAVE PCM frame"));
        }
        source.seek(SeekFrom::Start(data_offset))?;
        Ok(Self {
            source,
            format,
            data_offset,
            data_bytes,
            tags,
            remaining: data_bytes,
        })
    }

    pub fn frames(&self) -> u64 {
        self.data_bytes / self.format.block_align() as u64
    }

    /// Read whole frames into a reusable buffer. A zero result denotes end of data.
    pub fn read_frames(&mut self, buffer: &mut [u8]) -> Result<usize> {
        let align = self.format.block_align();
        let capacity = buffer.len() / align * align;
        if capacity == 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "buffer cannot hold a PCM frame",
            ));
        }
        let count = self.remaining.min(capacity as u64) as usize;
        self.source.read_exact(&mut buffer[..count])?;
        self.remaining -= count as u64;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn wave(data: &[u8]) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend((36 + data.len() as u32).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(48000u32.to_le_bytes());
        bytes.extend(192000u32.to_le_bytes());
        bytes.extend(4u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend(data);
        bytes
    }

    #[test]
    fn integer_riff_rejects_non_integer_formats_and_oversized_fmt() {
        for (tag, bits) in [(1u16, 8u16), (3, 32), (3, 64)] {
            let mut bytes = wave(&[]);
            bytes[20..22].copy_from_slice(&tag.to_le_bytes());
            bytes[32..34].copy_from_slice(&(2 * bits / 8).to_le_bytes());
            bytes[34..36].copy_from_slice(&bits.to_le_bytes());
            assert!(Reader::new(Cursor::new(&bytes)).is_ok());
            assert!(Reader::new_integer_riff(Cursor::new(bytes)).is_err());
        }
        let mut bytes = wave(&[0; 4]);
        bytes.splice(36..36, vec![0; 4082]);
        bytes[16..20].copy_from_slice(&4098u32.to_le_bytes());
        let size = bytes.len() as u32 - 8;
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        assert!(Reader::new(Cursor::new(&bytes)).is_ok());
        assert!(Reader::new_integer_riff(Cursor::new(bytes)).is_err());
    }

    #[test]
    fn integer_riff_preserves_authoring_chunk_policy() {
        let original = wave(&[0; 4]);
        for chunk in [&original[12..36], &original[36..]] {
            let mut bytes = original.clone();
            bytes.extend(chunk);
            let size = bytes.len() as u32 - 8;
            bytes[4..8].copy_from_slice(&size.to_le_bytes());
            assert!(Reader::new_integer_riff(Cursor::new(&bytes)).is_err());
            assert!(Reader::new(Cursor::new(bytes)).is_ok());
        }
        let mut bytes = original;
        bytes.extend(b"LIST\x0c\0\0\0INFOINAM\xff\xff\xff\xff");
        let size = bytes.len() as u32 - 8;
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        assert!(Reader::new(Cursor::new(&bytes)).is_err());
        assert!(
            Reader::new_integer_riff(Cursor::new(bytes))
                .unwrap()
                .tags
                .is_empty()
        );
    }

    #[test]
    fn integer_riff_accepts_only_missing_terminal_data_padding() {
        let format = Format {
            encoding: Encoding::Signed24,
            channels: 1,
            sample_rate: 48000,
            valid_bits: 24,
            channel_mask: 4,
        };
        let mut writer = crate::writer::Writer::new_riff(Cursor::new(Vec::new()), format).unwrap();
        writer.write_frames(&[1, 2, 3]).unwrap();
        let mut bytes = writer.finish().unwrap().into_inner();
        bytes.pop();
        let size = bytes.len() as u32 - 8;
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        assert_eq!(
            Reader::new_integer_riff(Cursor::new(&bytes))
                .unwrap()
                .data_bytes,
            3
        );
        assert!(Reader::new(Cursor::new(&bytes)).is_err());
        bytes.extend(b"JUNK\0\0\0\0");
        let size = bytes.len() as u32 - 8;
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        assert!(Reader::new_integer_riff(Cursor::new(bytes)).is_err());
    }

    #[test]
    fn info_entries_are_bounded_and_unknown_keys_survive() {
        let mut bytes = wave(&[0; 4]);
        let mut info = b"INFOIZZZ".to_vec();
        info.extend(3u32.to_le_bytes());
        info.extend(b"ok\0\0");
        bytes.extend(b"LIST");
        bytes.extend((info.len() as u32).to_le_bytes());
        bytes.extend(info);
        let length = bytes.len() as u32 - 8;
        bytes[4..8].copy_from_slice(&length.to_le_bytes());
        let reader = Reader::new(Cursor::new(&bytes)).unwrap();
        assert_eq!(reader.tags, [("IZZZ".into(), "ok".into())]);
        let count = bytes.len() - 8;
        bytes[count..count + 4].copy_from_slice(&100u32.to_le_bytes());
        assert!(Reader::new(Cursor::new(bytes)).is_err());
    }

    #[test]
    fn streaming_reads_only_whole_frames() {
        let data: Vec<_> = (0..20).collect();
        let mut reader = Reader::new(Cursor::new(wave(&data))).unwrap();
        assert_eq!(reader.frames(), 5);
        let mut output = Vec::new();
        let mut block = [0; 7];
        loop {
            let count = reader.read_frames(&mut block).unwrap();
            if count == 0 {
                break;
            }
            assert_eq!(count, 4);
            output.extend_from_slice(&block[..count]);
        }
        assert_eq!(output, data);
        assert!(reader.read_frames(&mut [0; 3]).is_err());
    }

    #[test]
    fn every_truncation_is_rejected() {
        let bytes = wave(&[0; 20]);
        for length in 0..bytes.len() {
            assert!(
                Reader::new(Cursor::new(&bytes[..length])).is_err(),
                "length {length}"
            );
        }
        assert!(Reader::new(Cursor::new(wave(&[0; 2]))).is_err());
    }

    #[test]
    fn rf64_repeated_ancillary_chunks_consume_size_entries_in_order() {
        let riff = wave(&[1, 2, 3, 4]);
        let mut bytes = b"RF64".to_vec();
        bytes.extend(u32::MAX.to_le_bytes());
        bytes.extend(b"WAVEds64");
        bytes.extend(52u32.to_le_bytes());
        bytes.extend(0u64.to_le_bytes());
        bytes.extend(4u64.to_le_bytes());
        bytes.extend(1u64.to_le_bytes());
        bytes.extend(2u32.to_le_bytes());
        for size in [3u64, 6] {
            bytes.extend(b"JUNK");
            bytes.extend(size.to_le_bytes());
        }
        for size in [3usize, 6] {
            bytes.extend(b"JUNK");
            bytes.extend(u32::MAX.to_le_bytes());
            bytes.resize(bytes.len() + size + (size & 1), 0);
        }
        bytes.extend(&riff[12..]);
        let length = bytes.len() as u64 - 8;
        bytes[20..28].copy_from_slice(&length.to_le_bytes());
        let mut reader = Reader::new(Cursor::new(&bytes)).unwrap();
        let mut data = [0; 4];
        assert_eq!(reader.read_frames(&mut data).unwrap(), 4);
        assert_eq!(data, [1, 2, 3, 4]);
        bytes[60..64].copy_from_slice(b"MISS");
        assert!(Reader::new(Cursor::new(bytes)).is_err());
    }

    #[test]
    fn rf64_sizes_and_missing_table() {
        let format = Format {
            encoding: Encoding::Signed16,
            channels: 2,
            sample_rate: 48000,
            valid_bits: 16,
            channel_mask: 3,
        };
        let mut writer = crate::writer::Writer::new(Cursor::new(Vec::new()), format).unwrap();
        writer.write_frames(&[0; 8]).unwrap();
        let mut bytes = writer.finish().unwrap().into_inner();
        let size = bytes.len() as u64 - 8;
        bytes[..4].copy_from_slice(b"RF64");
        bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        bytes[12..16].copy_from_slice(b"ds64");
        bytes[20..28].copy_from_slice(&size.to_le_bytes());
        bytes[28..36].copy_from_slice(&8u64.to_le_bytes());
        bytes[36..44].copy_from_slice(&2u64.to_le_bytes());
        bytes[100..104].copy_from_slice(&u32::MAX.to_le_bytes());
        let reader = Reader::new(Cursor::new(&bytes)).unwrap();
        assert_eq!(reader.frames(), 2);
        assert_eq!(reader.format, format);
        assert!(Reader::new_integer_riff(Cursor::new(&bytes)).is_err());
        bytes[12..16].copy_from_slice(b"JUNK");
        assert!(Reader::new(Cursor::new(bytes)).is_err());
    }

    #[test]
    fn extensible_geometry_and_guid_are_validated() {
        let format = Format {
            encoding: Encoding::Signed24,
            channels: 2,
            sample_rate: 96000,
            valid_bits: 20,
            channel_mask: 3,
        };
        let bytes = crate::writer::Writer::new(Cursor::new(Vec::new()), format)
            .unwrap()
            .finish()
            .unwrap()
            .into_inner();
        assert_eq!(Reader::new(Cursor::new(&bytes)).unwrap().format, format);
        for (offset, value) in [(74, 25), (76, 1), (84, 1)] {
            let mut invalid = bytes.clone();
            invalid[offset] = value;
            assert!(
                Reader::new(Cursor::new(invalid)).is_err(),
                "offset {offset}"
            );
        }
    }

    #[test]
    fn skips_padded_unknown_chunks() {
        let mut bytes = wave(&[1, 2, 3, 4]);
        bytes.splice(12..12, *b"JUNK\x01\0\0\0x\0");
        let length = bytes.len() as u32 - 8;
        bytes[4..8].copy_from_slice(&length.to_le_bytes());
        let mut reader = Reader::new(Cursor::new(bytes)).unwrap();
        let mut data = [0; 4];
        assert_eq!(reader.read_frames(&mut data).unwrap(), 4);
        assert_eq!(data, [1, 2, 3, 4]);
    }
}
