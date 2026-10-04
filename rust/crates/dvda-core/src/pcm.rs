//! Lossless PCM storage normalization with bounded buffers and owned output.
use crate::{
    formats::{self, WavError},
    media::{Failure, Outcome, Temporary, Timed},
};
use dvda_native::media::Callbacks;
use serde::Deserialize;
use std::{
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom, Write},
    os::windows::fs::OpenOptionsExt,
    path::PathBuf,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    pub source: String,
    pub destination: PathBuf,
    pub rate: i32,
    pub bits: i32,
}
impl From<WavError> for Failure {
    fn from(error: WavError) -> Self {
        match error {
            WavError::Io(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                Self::new("EndOfStream", &error.to_string())
            }
            WavError::Io(error) => error.into(),
            WavError::Invalid(message) => Self::new("InvalidData", &message),
            WavError::Overflow => Self::new("Overflow", "WAVE sample rate is too large"),
        }
    }
}
fn invalid(text: &str) -> Failure {
    Failure::new("InvalidData", text)
}
pub fn normalize(job: Job, caller: &mut dyn Callbacks) -> Result<u64, Failure> {
    let mut callbacks = Timed::new(caller, None);
    let mut owned = Temporary(None);
    let result = (|| -> Result<u64, Failure> {
        callbacks.check()?;
        if !matches!(job.bits, 16 | 20 | 24) {
            return Err(invalid("目标位深必须是 16/20/24。"));
        }
        let mut input = crate::identity::open_read(&job.source)?;
        let layout = formats::read_wav_layout_from(&mut input)?;
        if layout.sample_rate != job.rate {
            return Err(invalid("PCM 输出采样率与任务不符。"));
        }
        let mut mask = layout.channel_mask;
        if mask & 0x600 != 0 {
            if mask & (0x30 | 0x100) != 0 {
                return Err(invalid(
                    "不能把同时存在的侧置、后置或后中置声道合并为 DVD 环绕声道。",
                ));
            }
            mask = (mask & !0x600) | ((mask & 0x600) >> 5);
        }
        let channels = layout.channels as usize;
        let input_width = layout.bytes_per_sample as usize;
        let width = if job.bits == 16 { 2usize } else { 3 };
        let frames = layout.data_size as u64 / (channels * input_width) as u64;
        let size = frames * channels as u64 * width as u64;
        let riff_size = 60 + size + (size & 1);
        if riff_size > u64::from(u32::MAX) {
            return Err(invalid("PCM 超出 RIFF 4 GiB 限制；请拆分过长音轨。"));
        }
        let mut header = Vec::with_capacity(68);
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&(riff_size as u32).to_le_bytes());
        header.extend_from_slice(b"WAVEfmt ");
        header.extend_from_slice(&40u32.to_le_bytes());
        header.extend_from_slice(&0xfffeu16.to_le_bytes());
        header.extend_from_slice(&(channels as u16).to_le_bytes());
        header.extend_from_slice(&(job.rate as u32).to_le_bytes());
        header.extend_from_slice(&(job.rate as u32 * channels as u32 * width as u32).to_le_bytes());
        header.extend_from_slice(&((channels * width) as u16).to_le_bytes());
        header.extend_from_slice(&((width * 8) as u16).to_le_bytes());
        header.extend_from_slice(&22u16.to_le_bytes());
        header.extend_from_slice(&(job.bits as u16).to_le_bytes());
        header.extend_from_slice(&mask.to_le_bytes());
        header.extend_from_slice(&[1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]);
        header.extend_from_slice(b"data");
        header.extend_from_slice(&(size as u32).to_le_bytes());
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&job.destination)?;
        owned.0 = Some(job.destination.clone());
        output.write_all(&header)?;
        input.seek(SeekFrom::Start(layout.data_offset as u64))?;
        let mut raw = vec![0u8; 8192 * channels * input_width];
        let mut converted = Vec::with_capacity(8192 * channels * width);
        let precision_mask = (1i64 << (24 - job.bits)) - 1;
        let mut remaining = layout.data_size as u64;
        while remaining != 0 {
            callbacks.check()?;
            let count = remaining.min(raw.len() as u64) as usize;
            input
                .read_exact(&mut raw[..count])
                .map_err(|error| Failure::from(WavError::Io(error)))?;
            converted.clear();
            for bytes in raw[..count].chunks_exact(input_width) {
                let sample = match input_width {
                    2 => i64::from(i16::from_le_bytes(bytes.try_into().unwrap())) << 8,
                    3 => i64::from(
                        ((i32::from(bytes[0])
                            | i32::from(bytes[1]) << 8
                            | i32::from(bytes[2]) << 16)
                            << 8)
                            >> 8,
                    ),
                    4 => {
                        let value = i32::from_le_bytes(bytes.try_into().unwrap());
                        if value & 255 != 0 {
                            return Err(invalid("32 位存储包含超过 24 位的有效 PCM。"));
                        }
                        i64::from(value >> 8)
                    }
                    _ => unreachable!("Layout validated storage width"),
                };
                if sample & precision_mask != 0 {
                    return Err(invalid(
                        "PCM 有效精度高于目标位深；拒绝静默截断，请检查音源转换设置。",
                    ));
                }
                let stored = if width == 2 { sample >> 8 } else { sample };
                converted.extend_from_slice(&stored.to_le_bytes()[..width]);
            }
            output.write_all(&converted)?;
            remaining -= count as u64;
        }
        if size & 1 != 0 {
            output.write_all(&[0])?;
        }
        callbacks.check()?;
        drop(output);
        owned.0 = None;
        Ok(riff_size + 8)
    })();
    owned.clean().map_err(Failure::from).and(result)
}
pub fn execute(job: Job, callbacks: &mut dyn Callbacks) -> Outcome {
    match normalize(job, callbacks) {
        Ok(_) => Outcome {
            exit_code: Some(0),
            failure: None,
        },
        Err(error) => Outcome {
            exit_code: None,
            failure: Some(error),
        },
    }
}
pub(crate) struct NoEvents;
impl Callbacks for NoEvents {
    fn emit(&mut self, _: i32, _: &str) {}
    fn cancelled(&mut self) -> bool {
        false
    }
}
