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
    normalize_impl(job, caller, false)
}

pub(crate) fn normalize_s24(job: Job, caller: &mut dyn Callbacks) -> Result<u64, Failure> {
    normalize_impl(job, caller, true)
}

fn normalize_impl(job: Job, caller: &mut dyn Callbacks, raw_s24: bool) -> Result<u64, Failure> {
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
        let width = if !raw_s24 && job.bits == 16 {
            2usize
        } else {
            3
        };
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
        if !raw_s24 {
            output.write_all(&header)?;
        }
        input.seek(SeekFrom::Start(layout.data_offset as u64))?;
        let mut raw = vec![0u8; 8192 * channels * input_width];
        let mut converted = Vec::with_capacity(8192 * channels * width);
        let precision_mask = (1i64 << (24 - job.bits)) - 1;
        let direct_copy =
            (input_width == 2 && width == 2) || (input_width == 3 && width == 3 && job.bits == 24);
        let expand_16_to_24 = input_width == 2 && width == 3;
        let validate_and_copy_24 = input_width == 3 && width == 3 && job.bits == 20;
        let pack_32_to_24 = input_width == 4 && width == 3 && job.bits == 24;
        let mut remaining = layout.data_size as u64;
        while remaining != 0 {
            callbacks.check()?;
            let count = remaining.min(raw.len() as u64) as usize;
            input
                .read_exact(&mut raw[..count])
                .map_err(|error| Failure::from(WavError::Io(error)))?;
            if direct_copy {
                output.write_all(&raw[..count])?;
            } else if expand_16_to_24 {
                converted.resize(count / 2 * 3, 0);
                for (source, destination) in raw[..count]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .zip(converted.as_chunks_mut::<3>().0)
                {
                    *destination = [0, source[0], source[1]];
                }
                output.write_all(&converted)?;
            } else if validate_and_copy_24 {
                for bytes in raw[..count].as_chunks::<3>().0 {
                    if bytes[0] & 0x0f != 0 {
                        return Err(invalid(
                            "PCM 有效精度高于目标位深；拒绝静默截断，请检查音源转换设置。",
                        ));
                    }
                }
                output.write_all(&raw[..count])?;
            } else if pack_32_to_24 {
                converted.resize(count / 4 * 3, 0);
                for (bytes, destination) in raw[..count]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(converted.as_chunks_mut::<3>().0)
                {
                    if bytes[0] != 0 {
                        return Err(invalid("32 位存储包含超过 24 位的有效 PCM。"));
                    }
                    *destination = [bytes[1], bytes[2], bytes[3]];
                }
                output.write_all(&converted)?;
            } else {
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
            }
            remaining -= count as u64;
        }
        if !raw_s24 && size & 1 != 0 {
            output.write_all(&[0])?;
        }
        callbacks.check()?;
        drop(output);
        owned.0 = None;
        Ok(if raw_s24 { size } else { riff_size + 8 })
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

#[cfg(all(test, feature = "direct-bridges"))]
mod tests {
    use super::*;

    #[test]
    fn normalized_s24_matches_wave_decoder() {
        let root = std::env::temp_dir().join(format!("dvda-s24-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let library = PathBuf::from(std::env::var_os("DVDA_MEDIA_NATIVE_DIR").unwrap_or_default())
            .join("dvda-media.dll");
        for (bits, channels) in [16, 20, 24]
            .into_iter()
            .flat_map(|bits| (1..=6).map(move |channels| (bits, channels)))
        {
            for width in [2usize, 3, 4] {
                if width == 2 && bits != 16 {
                    continue;
                }
                let source = root.join("source.wav");
                let wav = root.join("normalized.wav");
                let raw = root.join("direct.raw");
                let decoded = root.join("decoded.raw");
                let mut data = Vec::new();
                let mut expected = Vec::new();
                for n in 0..16385 * channels {
                    let value = (match n % 5 {
                        0 => -8388608i32,
                        1 => 8388607,
                        2 => -1,
                        _ => n * 97 - 500000,
                    }) & !((1 << (24 - bits)) - 1);
                    expected.extend_from_slice(&value.to_le_bytes()[..3]);
                    let stored = match width {
                        2 => value >> 8,
                        4 => value << 8,
                        _ => value,
                    };
                    data.extend_from_slice(&stored.to_le_bytes()[..width]);
                }
                let mut bytes = Vec::new();
                bytes.extend_from_slice(b"RIFF");
                bytes.extend_from_slice(
                    &(60u32 + data.len() as u32 + data.len() as u32 % 2).to_le_bytes(),
                );
                bytes.extend_from_slice(b"WAVEfmt ");
                bytes.extend_from_slice(&40u32.to_le_bytes());
                bytes.extend_from_slice(&0xfffeu16.to_le_bytes());
                bytes.extend_from_slice(&(channels as u16).to_le_bytes());
                bytes.extend_from_slice(&48000u32.to_le_bytes());
                bytes.extend_from_slice(&(48000 * channels as u32 * width as u32).to_le_bytes());
                bytes.extend_from_slice(&(channels as u16 * width as u16).to_le_bytes());
                bytes.extend_from_slice(&(width as u16 * 8).to_le_bytes());
                bytes.extend_from_slice(&22u16.to_le_bytes());
                bytes.extend_from_slice(&(bits as u16).to_le_bytes());
                bytes.extend_from_slice(
                    &[0u32, 4, 3, 7, 0x33, 0x37, 0x3f][channels as usize].to_le_bytes(),
                );
                bytes
                    .extend_from_slice(&[1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]);
                bytes.extend_from_slice(b"data");
                bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&data);
                if data.len() % 2 != 0 {
                    bytes.push(0);
                }
                std::fs::write(&source, bytes).unwrap();
                let job = |destination| Job {
                    source: source.to_string_lossy().into_owned(),
                    destination,
                    rate: 48000,
                    bits,
                };
                normalize(job(wav.clone()), &mut NoEvents).unwrap();
                normalize_s24(job(raw.clone()), &mut NoEvents).unwrap();
                assert_eq!(std::fs::read(&raw).unwrap(), expected, "{bits}/{width}");
                crate::verify_audio::convert(
                    &library,
                    &wav,
                    &decoded,
                    0,
                    24,
                    dvda_native::media::OutputFormat::S24,
                    &mut NoEvents,
                )
                .unwrap();
                assert_eq!(
                    std::fs::read(&raw).unwrap(),
                    std::fs::read(&decoded).unwrap(),
                    "{bits}/{width}"
                );
                for path in [&wav, &raw, &decoded] {
                    std::fs::remove_file(path).unwrap();
                }
                struct Cancel;
                impl Callbacks for Cancel {
                    fn emit(&mut self, _: i32, _: &str) {}
                    fn cancelled(&mut self) -> bool {
                        true
                    }
                }
                assert!(normalize_s24(job(raw.clone()), &mut Cancel).is_err());
                assert!(!raw.exists());
                if bits < 24 && width >= 3 {
                    let mut invalid = std::fs::read(&source).unwrap();
                    invalid[68 + usize::from(width == 4)] |= 1;
                    std::fs::write(&source, invalid).unwrap();
                    assert!(normalize_s24(job(raw.clone()), &mut NoEvents).is_err());
                    assert!(!raw.exists());
                }
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod storage_tests {
    use super::*;

    fn wave_16(samples: &[i16]) -> Vec<u8> {
        let size = (samples.len() * 2) as u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + size).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&48000u32.to_le_bytes());
        bytes.extend_from_slice(&96000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&size.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    #[test]
    #[ignore = "manual PCM throughput benchmark"]
    fn normalization_throughput() {
        let root = std::env::temp_dir().join(format!("dvda-pcm-throughput-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let source = root.join("source.wav");
        let destination = root.join("output.raw");
        let samples: Vec<_> = (0..16_777_216).map(|n| n as i16).collect();
        std::fs::write(&source, wave_16(&samples)).unwrap();
        for round in 0..9 {
            let start = std::time::Instant::now();
            normalize_s24(
                Job {
                    source: source.to_string_lossy().into_owned(),
                    destination: destination.clone(),
                    rate: 48000,
                    bits: 16,
                },
                &mut NoEvents,
            )
            .unwrap();
            let elapsed = start.elapsed();
            let bytes = std::fs::read(&destination).unwrap();
            for (sample, actual) in samples.iter().zip(bytes.chunks_exact(3)) {
                assert_eq!(actual, &(i32::from(*sample) << 8).to_le_bytes()[..3]);
            }
            println!(
                "PCM_BENCH round={round} ms={:.3} bytes={}",
                elapsed.as_secs_f64() * 1000.0,
                bytes.len()
            );
            std::fs::remove_file(&destination).unwrap();
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    #[ignore = "manual PCM throughput benchmark"]
    fn packing_32_throughput() {
        let root =
            std::env::temp_dir().join(format!("dvda-pack32-throughput-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let source = root.join("source.wav");
        let destination = root.join("output.raw");
        let samples: Vec<i32> = (0..16_777_217).map(|n| (n as i32) << 8).collect();
        let mut wave = wave_16(&[]);
        let size = (samples.len() * 4) as u32;
        wave[4..8].copy_from_slice(&(36 + size).to_le_bytes());
        wave[28..32].copy_from_slice(&192000u32.to_le_bytes());
        wave[32..34].copy_from_slice(&4u16.to_le_bytes());
        wave[34..36].copy_from_slice(&32u16.to_le_bytes());
        wave[40..44].copy_from_slice(&size.to_le_bytes());
        for sample in &samples {
            wave.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(&source, wave).unwrap();
        for round in 0..9 {
            let start = std::time::Instant::now();
            normalize_s24(
                Job {
                    source: source.to_string_lossy().into_owned(),
                    destination: destination.clone(),
                    rate: 48000,
                    bits: 24,
                },
                &mut NoEvents,
            )
            .unwrap();
            let elapsed = start.elapsed();
            let bytes = std::fs::read(&destination).unwrap();
            assert_eq!(bytes.len(), samples.len() * 3);
            for (sample, actual) in samples.iter().zip(bytes.chunks_exact(3)) {
                assert_eq!(actual, &sample.to_le_bytes()[1..]);
            }
            println!(
                "PCM_BENCH round={round} ms={:.3} bytes={}",
                elapsed.as_secs_f64() * 1000.0,
                bytes.len()
            );
            std::fs::remove_file(&destination).unwrap();
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn all_i16_values_expand_losslessly_to_s24() {
        let root = std::env::temp_dir().join(format!("dvda-expand16-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _workspace = Cleanup(root.clone());
        let source = root.join("source.wav");
        let destination = root.join("output.raw");
        let samples: Vec<_> = (i16::MIN..=i16::MAX).chain([i16::MIN]).collect();
        std::fs::write(&source, wave_16(&samples)).unwrap();
        for bits in [16, 20, 24] {
            normalize_s24(
                Job {
                    source: source.to_string_lossy().into_owned(),
                    destination: destination.clone(),
                    rate: 48000,
                    bits,
                },
                &mut NoEvents,
            )
            .unwrap();
            let expected: Vec<_> = samples
                .iter()
                .flat_map(|&sample| (i32::from(sample) << 8).to_le_bytes()[..3].to_vec())
                .collect();
            assert_eq!(std::fs::read(&destination).unwrap(), expected);
            std::fs::remove_file(&destination).unwrap();
        }
    }
}

impl Callbacks for NoEvents {
    fn emit(&mut self, _: i32, _: &str) {}
    fn cancelled(&mut self) -> bool {
        false
    }
}
