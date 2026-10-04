//! Streaming PCM/metadata host for the pinned C MLP encoder. No output patching.
use crate::{
    formats::{self, WavLayout},
    media::{Failure, Outcome, Temporary, Timed, temporary_path},
};
use dvda_native::{
    encoder::{Encoder, Request, Stamp, Stream},
    files,
    media::Callbacks,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

pub const BINARY_SHA256: &str = "ece6d0a8033a26e2528042a7b74c66c249ea3c8d7378c06809fb94c8f6bd79b8";
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    pub library: PathBuf,
    pub wave: String,
    pub destination: PathBuf,
    pub metadata_context: String,
    pub timeout_millis: Option<u64>,
}
fn invalid(message: &str) -> Failure {
    Failure::new("InvalidData", message)
}
fn io_failure(error: io::Error) -> Failure {
    match error.kind() {
        io::ErrorKind::InvalidData => invalid(&error.to_string()),
        io::ErrorKind::UnexpectedEof => Failure::new("EndOfStream", &error.to_string()),
        _ => error.into(),
    }
}
pub fn access_units(frames: i64, rate: u32) -> Result<u64, Failure> {
    if frames <= 0 || !matches!(rate, 44100 | 48000 | 88200 | 96000 | 176400 | 192000) {
        return Err(invalid("无效的 PCM 帧数或采样率。"));
    }
    let block = 40
        * i64::from(
            rate / if rate.is_multiple_of(44100) {
                44100
            } else {
                48000
            },
        );
    frames
        .checked_add(block - 1)
        .map(|n| (n / block) as u64)
        .ok_or_else(|| Failure::new("Overflow", "PCM frame count overflow"))
}
fn assignment(mask: u32) -> Result<u32, Failure> {
    [
        4, 3, 0x103, 0x33, 0xb, 0x10b, 0x3b, 7, 0x107, 0x37, 0xf, 0x10f, 0x3f,
    ]
    .iter()
    .position(|m| *m == mask)
    .map(|i| i as u32)
    .ok_or_else(|| invalid(&format!("不支持的 DVD-Audio 声道掩码：0x{mask:X}。")))
}
fn read_number<const N: usize>(input: &mut File) -> Result<[u8; N], Failure> {
    let mut bytes = [0; N];
    input.read_exact(&mut bytes).map_err(io_failure)?;
    Ok(bytes)
}
pub fn metadata(path: &str, units: u64) -> Result<Vec<Stamp>, Failure> {
    if crate::config::trim(path).is_empty() {
        return Ok(vec![Stamp {
            start: 0,
            packet: vec![0, 0, 0x40, 0],
            valid_bits: 0,
        }]);
    }
    let mut input = crate::identity::open_read(path)?;
    let mut magic = Vec::new();
    (&mut input).take(8).read_to_end(&mut magic)?;
    if !matches!(magic.as_slice(), b"MSCTX001" | b"MSCTX002")
        || u64::from_le_bytes(read_number(&mut input)?) != units
    {
        return Err(invalid("MLP 元数据上下文格式或 AU 总数与输入不符。"));
    }
    let count = u32::from_le_bytes(read_number(&mut input)?);
    if !(1..=4096).contains(&count) {
        return Err(invalid("元数据记录数量无效。"));
    }
    let mut result = Vec::with_capacity(count as usize);
    let mut total = 0u64;
    for _ in 0..count {
        let start = u64::from_le_bytes(read_number(&mut input)?);
        let size = u32::from_le_bytes(read_number(&mut input)?);
        let valid_bits = if magic == b"MSCTX002" {
            u32::from_le_bytes(read_number(&mut input)?)
        } else {
            0
        };
        total += u64::from(size);
        if !(4..=65539).contains(&size) || total > 16777216 {
            return Err(invalid("元数据记录长度无效。"));
        }
        let mut packet = vec![0; size as usize];
        if let Err(error) = input.read_exact(&mut packet) {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                return Err(invalid("元数据上下文已截断。"));
            }
            return Err(error.into());
        }
        result.push(Stamp {
            start,
            packet,
            valid_bits,
        });
    }
    let mut extra = [0];
    if input.read(&mut extra)? != 0 {
        return Err(invalid("元数据上下文包含多余数据。"));
    }
    Ok(result)
}
pub fn write_metadata(path: &Path, frames: i64, rate: u32) -> Result<(), Failure> {
    let units = access_units(frames, rate)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(path)?;
    for bytes in [
        b"MSCTX001".as_slice(),
        &units.to_le_bytes(),
        &1u32.to_le_bytes(),
        &0u64.to_le_bytes(),
        &4u32.to_le_bytes(),
        &[0, 0, 0x40, 0],
    ] {
        file.write_all(bytes)?;
    }
    Ok(())
}

struct PcmStream<'a, 'b> {
    input: File,
    output: File,
    layout: &'a WavLayout,
    remaining: u64,
    raw: Vec<u8>,
    callbacks: &'a mut Timed<'b>,
}
impl Stream for PcmStream<'_, '_> {
    fn read(&mut self, samples: &mut [i32]) -> io::Result<usize> {
        if self.callbacks.cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "MLP encoding cancelled",
            ));
        }
        let channels = self.layout.channels as usize;
        let width = self.layout.bytes_per_sample as usize;
        let frames = (samples.len() / channels)
            .min(8192)
            .min(self.remaining as usize);
        let count = frames * channels;
        self.input.read_exact(&mut self.raw[..count * width])?;
        for (index, value) in samples[..count].iter_mut().enumerate() {
            let offset = index * width;
            *value = match width {
                2 => {
                    i32::from(i16::from_le_bytes(
                        self.raw[offset..offset + 2].try_into().unwrap(),
                    )) << 8
                }
                3 => {
                    ((self.raw[offset] as i32
                        | (self.raw[offset + 1] as i32) << 8
                        | (self.raw[offset + 2] as i32) << 16)
                        << 8)
                        >> 8
                }
                4 => {
                    if self.raw[offset] != 0 {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "32 位 PCM 包含无法无损表示的有效低位。",
                        ));
                    }
                    i32::from_le_bytes(self.raw[offset..offset + 4].try_into().unwrap()) >> 8
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "不支持的 PCM 存储格式。",
                    ));
                }
            };
        }
        self.remaining -= frames as u64;
        Ok(frames)
    }
    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.callbacks.cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "MLP encoding cancelled",
            ));
        }
        self.output.write_all(bytes)
    }
}

pub fn execute(job: Job, caller: &mut dyn Callbacks) -> Outcome {
    let mut callbacks = Timed::new(caller, job.timeout_millis);
    let mut temporary = Temporary(None);
    let result = (|| -> Result<i32, Failure> {
        callbacks.check()?;
        let mut input = crate::identity::open_read(&job.wave)?;
        let layout = formats::read_wav_layout_from(&mut input).map_err(|e| match e {
            formats::WavError::Io(e) => io_failure(e),
            formats::WavError::Invalid(e) => invalid(&e),
            formats::WavError::Overflow => {
                Failure::new("Overflow", "WAVE sample rate is too large")
            }
        })?;
        let frames = layout.data_size / i64::from(layout.bytes_per_sample * layout.channels);
        let assignment = assignment(layout.channel_mask)?;
        let hash = crate::hash::reader_digest(File::open(&job.library)?)?;
        if !hash.eq_ignore_ascii_case(BINARY_SHA256) {
            return Err(invalid(
                "MLP DLL 哈希不符，请清理损坏的原生核心缓存后重试。",
            ));
        }
        let encoder = Encoder::load(&job.library)?;
        let metadata = metadata(
            &job.metadata_context,
            access_units(frames, layout.sample_rate as u32)?,
        )?;
        if job.destination.is_file() {
            return Err(Failure::new("Io", "拒绝覆盖已有的 MLP 输出文件。"));
        }
        let path = temporary_path(&job.destination);
        let output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&path)?;
        temporary.0 = Some(path.clone());
        input.seek(SeekFrom::Start(layout.data_offset as u64))?;
        let request = Request {
            sample_rate: layout.sample_rate as u32,
            bits: layout.valid_bits as u32,
            channels: layout.channels as u32,
            frames: frames as u64,
            assignment,
            metadata,
        };
        let mut stream = PcmStream {
            input,
            output,
            layout: &layout,
            remaining: frames as u64,
            raw: vec![0; 8192 * layout.channels as usize * layout.bytes_per_sample as usize],
            callbacks: &mut callbacks,
        };
        let encoded = encoder.encode(&request, &mut stream);
        stream.callbacks.check()?;
        let encoded = encoded.map_err(io_failure)?;
        let length = stream.output.metadata()?.len();
        if encoded.status != 0
            || encoded.input_frames != frames as u64
            || encoded.output_bytes != length
            || length == 0
        {
            return Err(Failure::new(
                "InvalidOperation",
                &format!(
                    "MLP 编码器 DLL 编码失败（{}）：{}",
                    encoded.status, encoded.error
                ),
            ));
        }
        // Structured completion event; localized number formatting belongs to the UI.
        stream.callbacks.emit(
            4,
            &json!({"Frames":frames,"Rate":layout.sample_rate,"Bits":layout.valid_bits,
            "Channels":layout.channels,"Bytes":length})
            .to_string(),
        );
        drop(stream);
        callbacks.check()?;
        files::move_file(&path, &job.destination, false)?;
        temporary.0 = None;
        Ok(0)
    })();
    let result = temporary.clean().map_err(Failure::from).and(result);
    match result {
        Ok(code) => Outcome {
            exit_code: Some(code),
            failure: None,
        },
        Err(error) => Outcome {
            exit_code: None,
            failure: Some(error),
        },
    }
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    if operation != "encoder.write_metadata" {
        return Err("Unknown encoder operation".into());
    }
    let path = request["Path"]
        .as_str()
        .ok_or("Expected metadata output path")?;
    let frames = request["Frames"].as_i64().ok_or("Expected frame count")?;
    let rate = request["Rate"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or("Expected sample rate")?;
    Ok(match write_metadata(Path::new(path), frames, rate) {
        Ok(()) => json!({"ExitCode":0,"Failure":null}),
        Err(error) => json!({"ExitCode":null,"Failure":error}),
    })
}
