//! Streaming PCM WAVE output with explicit finalization.

use crate::pcm::Encoding;
use crate::wav::Format;
use std::io::{Error, ErrorKind, Result, Seek, SeekFrom, Write};

pub struct Writer<W> {
    destination: W,
    format: Format,
    bytes: u64,
    rf64: bool,
}

impl<W: Write + Seek> Writer<W> {
    /// Write an extensible PCM header. RF64 is selected at finalization only when needed.
    pub fn new(destination: W, format: Format) -> Result<Self> {
        Self::create(destination, format, true)
    }

    /// Write compact RIFF without RF64 reservation; reject output exceeding the RIFF limit.
    pub fn new_riff(destination: W, format: Format) -> Result<Self> {
        Self::create(destination, format, false)
    }

    fn create(mut destination: W, format: Format, rf64: bool) -> Result<Self> {
        let bits = match format.encoding {
            Encoding::Signed16 => 16u16,
            Encoding::Signed24 => 24,
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "WAVE output must be 16/24-bit PCM",
                ));
            }
        };
        let align = u16::try_from(format.block_align())
            .map_err(|_| Error::new(ErrorKind::InvalidInput, "WAVE frame too large"))?;
        let byte_rate = format
            .sample_rate
            .checked_mul(u32::from(align))
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "WAVE byte rate overflow"))?;
        if format.channels == 0
            || format.sample_rate == 0
            || format.valid_bits == 0
            || format.valid_bits > bits
            || (format.channel_mask != 0
                && format.channel_mask.count_ones() != u32::from(format.channels))
        {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid WAVE output geometry",
            ));
        }
        destination.seek(SeekFrom::Start(0))?;
        destination.write_all(b"RIFF\0\0\0\0WAVE")?;
        if rf64 {
            destination.write_all(b"JUNK")?;
            destination.write_all(&28u32.to_le_bytes())?;
            destination.write_all(&[0; 28])?;
        }
        destination.write_all(b"fmt ")?;
        destination.write_all(&40u32.to_le_bytes())?;
        destination.write_all(&0xfffeu16.to_le_bytes())?;
        destination.write_all(&format.channels.to_le_bytes())?;
        destination.write_all(&format.sample_rate.to_le_bytes())?;
        destination.write_all(&byte_rate.to_le_bytes())?;
        destination.write_all(&align.to_le_bytes())?;
        destination.write_all(&bits.to_le_bytes())?;
        destination.write_all(&22u16.to_le_bytes())?;
        destination.write_all(&format.valid_bits.to_le_bytes())?;
        destination.write_all(&format.channel_mask.to_le_bytes())?;
        destination.write_all(&[1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113])?;
        destination.write_all(b"data\0\0\0\0")?;
        Ok(Self {
            destination,
            format,
            bytes: 0,
            rf64,
        })
    }

    pub fn write_frames(&mut self, bytes: &[u8]) -> Result<()> {
        if !bytes.len().is_multiple_of(self.format.block_align()) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "incomplete output PCM frame",
            ));
        }
        let length = self
            .bytes
            .checked_add(bytes.len() as u64)
            .filter(|length| length.checked_add(97).is_some())
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "WAVE output size overflow"))?;
        if !self.rf64 && 60 + length + (length & 1) > u64::from(u32::MAX) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "WAVE exceeds RIFF size limit",
            ));
        }
        self.destination.write_all(bytes)?;
        self.bytes = length;
        Ok(())
    }

    pub fn finish(self) -> Result<W> {
        self.finish_with_tags(&[])
    }

    pub fn finish_with_tags(mut self, tags: &[(String, String)]) -> Result<W> {
        let padding = self.bytes & 1;
        if padding != 0 {
            self.destination.write_all(&[0])?;
        }
        let mut info = Vec::new();
        for (key, value) in tags {
            let id = match key.as_str() {
                "artist" => b"IART",
                "comment" => b"ICMT",
                "copyright" => b"ICOP",
                "date" => b"ICRD",
                "genre" => b"IGNR",
                "title" => b"INAM",
                "album" => b"IPRD",
                "track" => b"ITRK",
                "encoder" => b"ISFT",
                _ if key.len() == 4 && key.starts_with('I') => key.as_bytes(),
                _ => continue,
            };
            let size = u32::try_from(value.len().saturating_add(1))
                .map_err(|_| Error::new(ErrorKind::InvalidInput, "WAVE tag too large"))?;
            info.extend_from_slice(id);
            info.extend_from_slice(&size.to_le_bytes());
            info.extend_from_slice(value.as_bytes());
            info.push(0);
            if size & 1 != 0 {
                info.push(0);
            }
        }
        if !info.is_empty() {
            let size = u32::try_from(info.len().saturating_add(4))
                .map_err(|_| Error::new(ErrorKind::InvalidInput, "WAVE metadata too large"))?;
            self.destination.write_all(b"LIST")?;
            self.destination.write_all(&size.to_le_bytes())?;
            self.destination.write_all(b"INFO")?;
            self.destination.write_all(&info)?;
        }
        let riff_size = self.destination.stream_position()? - 8;
        if !self.rf64 && riff_size > u64::from(u32::MAX) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "WAVE exceeds RIFF size limit",
            ));
        }
        self.destination.seek(SeekFrom::Start(0))?;
        if riff_size > u64::from(u32::MAX) {
            self.destination.write_all(b"RF64")?;
            self.destination.write_all(&u32::MAX.to_le_bytes())?;
            self.destination.seek(SeekFrom::Start(12))?;
            self.destination.write_all(b"ds64")?;
            self.destination.seek(SeekFrom::Start(20))?;
            self.destination.write_all(&riff_size.to_le_bytes())?;
            self.destination.write_all(&self.bytes.to_le_bytes())?;
            self.destination
                .write_all(&(self.bytes / self.format.block_align() as u64).to_le_bytes())?;
            self.destination.write_all(&0u32.to_le_bytes())?;
            self.destination.seek(SeekFrom::Start(100))?;
            self.destination.write_all(&u32::MAX.to_le_bytes())?;
        } else {
            self.destination.seek(SeekFrom::Start(4))?;
            self.destination
                .write_all(&(riff_size as u32).to_le_bytes())?;
            self.destination
                .seek(SeekFrom::Start(if self.rf64 { 100 } else { 64 }))?;
            self.destination
                .write_all(&(self.bytes as u32).to_le_bytes())?;
        }
        self.destination.seek(SeekFrom::Start(riff_size + 8))?;
        self.destination.flush()?;
        Ok(self.destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::Reader;
    use std::io::Cursor;

    #[test]
    fn compact_riff_layout_padding_and_limit() {
        let format = Format {
            encoding: Encoding::Signed24,
            channels: 1,
            sample_rate: 48000,
            valid_bits: 20,
            channel_mask: 4,
        };
        let mut writer = Writer::new_riff(Cursor::new(Vec::new()), format).unwrap();
        writer.write_frames(&[16, 32, 48]).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert_eq!(bytes.len(), 72);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[4..8], &64u32.to_le_bytes());
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[60..68], b"data\x03\0\0\0");
        assert_eq!(&bytes[68..], &[16, 32, 48, 0]);
        let reader = Reader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(reader.format, format);
        assert_eq!(reader.data_offset, 68);
        let mut writer = Writer::new_riff(Cursor::new(Vec::new()), format).unwrap();
        writer.bytes = u64::from(u32::MAX) - 60;
        assert!(writer.write_frames(&[0; 3]).is_err());
        assert_eq!(writer.destination.get_ref().len(), 68);
    }

    #[test]
    fn trailing_info_keeps_pcm_size_and_padding() {
        let format = Format {
            encoding: Encoding::Signed24,
            channels: 1,
            sample_rate: 48000,
            valid_bits: 24,
            channel_mask: 4,
        };
        let mut writer = Writer::new(Cursor::new(Vec::new()), format).unwrap();
        writer.write_frames(&[1, 2, 3]).unwrap();
        let output = writer
            .finish_with_tags(&[("title".into(), "测试".into())])
            .unwrap();
        let bytes = output.get_ref();
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
            bytes.len() - 8
        );
        assert_eq!(&bytes[100..104], &3u32.to_le_bytes());
        assert_eq!(&bytes[108..112], b"LIST");
        assert_eq!(&bytes[116..124], b"INFOINAM");
        assert_eq!(&bytes[128..134], "测试".as_bytes());
        assert_eq!(&bytes[134..136], &[0, 0]);
        let mut reader = Reader::new(output).unwrap();
        assert_eq!(reader.frames(), 1);
        assert_eq!(reader.tags, [("title".into(), "测试".into())]);
        let mut pcm = [0; 3];
        assert_eq!(reader.read_frames(&mut pcm).unwrap(), 3);
        assert_eq!(pcm, [1, 2, 3]);
    }

    #[test]
    fn round_trip_pcm_and_effective_depth() {
        for (encoding, bits) in [
            (Encoding::Signed16, 16),
            (Encoding::Signed24, 20),
            (Encoding::Signed24, 24),
        ] {
            for channels in [1, 2, 6] {
                let format = Format {
                    encoding,
                    channels,
                    sample_rate: 48000,
                    valid_bits: bits,
                    channel_mask: 0,
                };
                let mut writer = Writer::new(Cursor::new(Vec::new()), format).unwrap();
                let data = vec![0; format.block_align() * 3];
                writer.write_frames(&data).unwrap();
                let mut reader = Reader::new(writer.finish().unwrap()).unwrap();
                assert_eq!(reader.format, format);
                assert_eq!(reader.frames(), 3);
                let mut result = vec![1; data.len()];
                assert_eq!(reader.read_frames(&mut result).unwrap(), data.len());
                assert_eq!(result, data);
            }
        }
    }
}
