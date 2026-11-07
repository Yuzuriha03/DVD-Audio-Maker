//! Streaming PCM/metadata host for the linked Rust encoder. No output patching.
use crate::{
    formats::{self, WavLayout},
    media::{Failure, Outcome, Temporary, Timed, temporary_path},
};
use dvda_native::{
    encoder::{Request, Stamp, Stream},
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
/// Compute the number of padded MLP access units for a PCM stream.
///
/// # Errors
/// Rejects non-positive frame counts and unsupported sample rates, and reports arithmetic overflow.
///
/// # Panics
/// The validated positive access-unit count always fits `u64`; the internal conversion
/// expectation is unreachable for every accepted frame count and sample rate.
pub fn access_units(frames: i64, rate: u32) -> Result<u64, Failure> {
    if frames <= 0 || !matches!(rate, 44100 | 48000 | 88200 | 96000 | 176_400 | 192_000) {
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
        .map(|n| u64::try_from(n / block).expect("positive access-unit count fits u64"))
        .ok_or_else(|| Failure::new("Overflow", "PCM frame count overflow"))
}
pub(crate) fn assignment(mask: u32) -> Result<u32, Failure> {
    [
        4, 3, 0x103, 0x33, 0xb, 0x10b, 0x3b, 7, 0x107, 0x37, 0xf, 0x10f, 0x3f,
    ]
    .iter()
    .position(|m| *m == mask)
    .map(|i| u32::try_from(i).expect("channel assignment index is at most 12"))
    .ok_or_else(|| invalid(&format!("不支持的 DVD-Audio 声道掩码：0x{mask:X}。")))
}
fn read_number<const N: usize>(input: &mut File) -> Result<[u8; N], Failure> {
    let mut bytes = [0; N];
    input.read_exact(&mut bytes).map_err(io_failure)?;
    Ok(bytes)
}
/// # Errors
/// Propagates default stamp generation or context-file I/O failures. Rejects an
/// unknown context version, a mismatched access-unit count, invalid record
/// counts/sizes, truncated packets, and trailing data.
pub fn metadata(path: &str, units: u64) -> Result<Vec<Stamp>, Failure> {
    if crate::config::trim(path).is_empty() {
        return Ok(vec![crate::encoder_stamp::current()?]);
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
        if !(4..=65539).contains(&size) || total > 16_777_216 {
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
/// Write a new metadata context for a PCM stream without replacing an existing file.
///
/// # Errors
/// Rejects invalid PCM geometry and existing destinations, and propagates metadata generation or I/O failures.
///
/// # Panics
/// The embedded encoder stamp packet has a fixed `u32`-representable length, so
/// its internal length-conversion expectation cannot fail.
pub fn write_metadata(path: &Path, frames: i64, rate: u32) -> Result<(), Failure> {
    let units = access_units(frames, rate)?;
    let stamp = crate::encoder_stamp::current()?;
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
        &u32::try_from(stamp.packet.len())
            .expect("embedded encoder stamp packet length fits u32")
            .to_le_bytes(),
        &stamp.packet,
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
        let channels = usize::try_from(self.layout.channels)
            .expect("validated WAVE channel count is positive");
        let width = usize::try_from(self.layout.bytes_per_sample)
            .expect("validated WAVE sample width is positive");
        let frames = (samples.len() / channels)
            .min(8192)
            .min(usize::try_from(self.remaining.min(8192)).expect("frame chunk is at most 8192"));
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
                    ((i32::from(self.raw[offset])
                        | i32::from(self.raw[offset + 1]) << 8
                        | i32::from(self.raw[offset + 2]) << 16)
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

#[cfg(not(feature = "rust-mlp"))]
fn encode_rust(
    _: &Request,
    _: &mut dyn Stream,
    _: bool,
) -> io::Result<dvda_native::encoder::ResultInfo> {
    Err(io::Error::other("Rust MLP backend feature is disabled"))
}

#[cfg(feature = "rust-mlp")]
#[allow(
    clippy::too_many_lines,
    reason = "The Rust encoder adapter keeps stream callbacks, stamp refresh, and result finalization ordered"
)]
fn encode_rust(
    request: &Request,
    stream: &mut dyn Stream,
    live_metadata: bool,
) -> io::Result<dvda_native::encoder::ResultInfo> {
    struct Adapter<'a> {
        stream: &'a mut dyn Stream,
        failure: Option<io::Error>,
        stamp_refresh: Option<crate::encoder_stamp::LiveStamp>,
    }
    impl dvda_mlp::direct::StreamIo for Adapter<'_> {
        fn read(&mut self, _: usize, pcm: &mut [i32]) -> Result<usize, i32> {
            self.stream.read(pcm).map_err(|error| {
                self.failure = Some(error);
                -3
            })
        }
        fn write(&mut self, bytes: &[u8]) -> Result<(), i32> {
            self.stream.write(bytes).map_err(|error| {
                self.failure = Some(error);
                -4
            })
        }
        fn refresh_stamp(&mut self) -> Result<Option<dvda_mlp::metadata::Metadata>, i32> {
            let Some(stamp) = self.stamp_refresh.as_mut() else {
                return Ok(None);
            };
            match stamp.refresh() {
                Ok(next) => Ok(next.map(|stamp| dvda_mlp::metadata::Metadata {
                    start: stamp.start,
                    packet: stamp.packet,
                    valid_bits: stamp.valid_bits,
                })),
                Err(error) => {
                    self.failure = Some(io::Error::other(error.message));
                    Err(-5)
                }
            }
        }
    }
    let config = dvda_mlp::Config {
        sample_rate: request.sample_rate,
        bits: u8::try_from(request.bits).unwrap_or(0),
        channels: u8::try_from(request.channels).unwrap_or(0),
        restart_interval: 8,
        metadata: Vec::new(),
    };
    let profile = dvda_mlp::format::Profile {
        assignment: request.assignment as usize,
        group2_bits: config.bits,
        group2_rate: config.sample_rate,
    };
    let metadata = request
        .metadata
        .iter()
        .map(|stamp| dvda_mlp::metadata::Metadata {
            start: stamp.start,
            packet: stamp.packet.clone(),
            valid_bits: stamp.valid_bits,
        })
        .collect::<Vec<_>>();
    let mut adapter = Adapter {
        stream,
        failure: None,
        stamp_refresh: if live_metadata {
            let stamp = request
                .metadata
                .first()
                .ok_or_else(|| io::Error::other("Missing live stamp"))?;
            Some(
                crate::encoder_stamp::LiveStamp::new(stamp)
                    .map_err(|error| io::Error::other(error.message))?,
            )
        } else {
            None
        },
    };
    let result = if adapter.stamp_refresh.is_some() {
        dvda_mlp::direct::encode_stream_with_stamp_refresh(
            &config,
            &profile,
            request.frames,
            &metadata,
            &mut adapter,
        )
    } else {
        dvda_mlp::direct::encode_stream(
            &config,
            &profile,
            request.frames,
            None,
            &metadata,
            &mut adapter,
        )
    };
    if let Some(error) = adapter.failure {
        return Err(error);
    }
    let error = result
        .error
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c.cast_unsigned())
        .collect::<Vec<_>>();
    Ok(dvda_native::encoder::ResultInfo {
        status: result.status,
        access_units: result.access_units,
        input_frames: result.input_frames,
        encoded_frames: result.encoded_frames,
        output_bytes: result.output_bytes,
        error: String::from_utf8_lossy(&error).into_owned(),
    })
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "The public typed encoder adapter retains its owned-job API"
)]
pub fn execute(job: Job, caller: &mut dyn Callbacks) -> Outcome {
    execute_with(&job, caller)
}

#[allow(
    clippy::too_many_lines,
    reason = "Streaming encoding keeps backend selection, cancellation, metadata, and atomic output cleanup together"
)]
fn execute_with(job: &Job, caller: &mut dyn Callbacks) -> Outcome {
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
        let metadata = metadata(
            &job.metadata_context,
            access_units(frames, layout.sample_rate.cast_unsigned())?,
        )?;
        let frame_count =
            u64::try_from(frames).expect("access_units accepted a positive frame count");
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
        input.seek(SeekFrom::Start(
            u64::try_from(layout.data_offset).expect("WAVE data offset is nonnegative"),
        ))?;
        let request = Request {
            sample_rate: layout.sample_rate.cast_unsigned(),
            bits: layout.valid_bits.cast_unsigned(),
            channels: layout.channels.cast_unsigned(),
            frames: frame_count,
            assignment,
            metadata,
        };
        let mut stream = PcmStream {
            input,
            output,
            layout: &layout,
            remaining: frame_count,
            raw: vec![
                0;
                8192 * usize::try_from(layout.channels)
                    .expect("validated WAVE channel count is positive")
                    * usize::try_from(layout.bytes_per_sample)
                        .expect("validated WAVE sample width is positive")
            ],
            callbacks: &mut callbacks,
        };
        let encoded = encode_rust(
            &request,
            &mut stream,
            crate::config::trim(&job.metadata_context).is_empty(),
        );
        stream.callbacks.check()?;
        let encoded = encoded.map_err(io_failure)?;
        let length = stream.output.metadata()?.len();
        if encoded.status != 0
            || encoded.input_frames != frame_count
            || encoded.output_bytes != length
            || length == 0
        {
            return Err(Failure::new(
                "InvalidOperation",
                &format!(
                    "MLP 编码器编码失败（{}）：{}",
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

#[cfg(all(test, feature = "rust-mlp"))]
mod rust_tests {
    use super::*;
    #[test]
    fn generated_metadata_context_roundtrips_and_never_overwrites() {
        let path = std::env::temp_dir().join(format!(
            "dvda-stamp-{}-{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_metadata(&path, 48000, 48000).unwrap();
        let saved = std::fs::read(&path).unwrap();
        let units = access_units(48000, 48000).unwrap();
        let records = metadata(path.to_str().unwrap(), units).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].start, 0);
        assert_eq!(&records[0].packet[2..4], &[0x40, 0]);
        assert!(records[0].packet.windows(4).any(|v| v == [0, 20, 0x14, 0]));
        assert_eq!(
            metadata(path.to_str().unwrap(), units).unwrap()[0].packet,
            records[0].packet
        );
        assert!(metadata(path.to_str().unwrap(), units + 1).is_err());
        assert!(write_metadata(&path, 48000, 48000).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        std::fs::remove_file(path).unwrap();
    }

    struct Host {
        output: Vec<u8>,
        failure: Option<io::ErrorKind>,
        output_failure: bool,
        panic: bool,
    }
    impl Stream for Host {
        fn read(&mut self, samples: &mut [i32]) -> io::Result<usize> {
            assert!(!self.panic, "callback panic");
            if let Some(kind) = self.failure {
                return Err(io::Error::new(kind, "original input failure"));
            }
            samples.fill(0);
            Ok(samples.len() / 2)
        }
        fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
            if self.output_failure {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "original output failure",
                ));
            }
            self.output.extend_from_slice(bytes);
            Ok(())
        }
    }
    #[test]
    fn production_entrypoint_encodes_without_encoder_dll() {
        struct Caller;
        impl Callbacks for Caller {
            fn emit(&mut self, _: i32, _: &str) {}
            fn cancelled(&mut self) -> bool {
                false
            }
        }
        let root = std::env::temp_dir().join(format!(
            "dvda-linked-encoder-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let wave = root.join("input.wav");
        let destination = root.join("output.mlp");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&200u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&48000u32.to_le_bytes());
        bytes.extend_from_slice(&192_000_u32.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&164u32.to_le_bytes());
        bytes.resize(208, 0);
        std::fs::write(&wave, bytes).unwrap();
        let job = || Job {
            library: root.join("missing-encoder.dll"),
            wave: wave.to_string_lossy().into_owned(),
            destination: destination.clone(),
            metadata_context: String::new(),
            timeout_millis: None,
        };
        let outcome = execute(job(), &mut Caller);
        assert_eq!(outcome.exit_code, Some(0), "{:?}", outcome.failure);
        let output = std::fs::read(&destination).unwrap();
        assert_ne!(output, [] as [u8; 0]);
        assert!(execute(job(), &mut Caller).failure.is_some());
        assert_eq!(std::fs::read(&destination).unwrap(), output);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_identity_reaches_auxiliary_bits_without_changing_coded_payload() {
        let identity = crate::encoder_stamp::current().unwrap();
        let expected = dvda_mlp::metadata::timeline(
            &[dvda_mlp::metadata::Metadata {
                start: 0,
                packet: identity.packet.clone(),
                valid_bits: 0,
            }],
            9 + 8 * identity.packet.len(),
        )
        .unwrap();
        let mut request = Request {
            sample_rate: 48000,
            bits: 24,
            channels: 2,
            frames: 160_000,
            assignment: 1,
            metadata: vec![identity],
        };
        let run = |request: &Request, live| {
            let mut host = Host {
                output: Vec::new(),
                failure: None,
                output_failure: false,
                panic: false,
            };
            assert_eq!(encode_rust(request, &mut host, live).unwrap().status, 0);
            host.output
        };
        let generated = run(&request, true);
        request.metadata = vec![Stamp {
            start: 0,
            packet: vec![0, 0, 0x40, 0],
            valid_bits: 0,
        }];
        let reference = run(&request, false);
        let normalized = |mut bytes: Vec<u8>| {
            let mut at = 0;
            let mut bits = Vec::new();
            while at < bytes.len() {
                let length =
                    usize::from(u16::from_be_bytes([bytes[at], bytes[at + 1]]) & 0xfff) * 2;
                assert!(length >= 8 && at + length <= bytes.len());
                let directory = at
                    + if bytes[at + 4..at + 8] == [0xf8, 0x72, 0x6f, 0xbb] {
                        32
                    } else {
                        4
                    };
                let bit = (bytes[directory] >> 4) & 1;
                bits.push(u32::from(bit));
                bytes[directory] &= 0xef;
                bytes[at] ^= bit << 4;
                at += length;
            }
            (crate::hash::hex_digest(&bytes), bits)
        };
        let (generated_hash, bits) = normalized(generated);
        let (reference_hash, _) = normalized(reference);
        assert_eq!(bits.len(), 4000);
        assert_eq!(&bits[..expected.len()], &expected);
        assert_eq!(generated_hash, reference_hash);
    }

    #[test]
    fn rust_adapter_preserves_results_and_input_errors() {
        let mut request = Request {
            sample_rate: 48000,
            bits: 16,
            channels: 2,
            frames: 41,
            assignment: 1,
            metadata: vec![Stamp {
                start: 0,
                packet: vec![0, 0, 0x40, 0],
                valid_bits: 0,
            }],
        };
        let mut host = Host {
            output: Vec::new(),
            failure: None,
            output_failure: false,
            panic: false,
        };
        let result = encode_rust(&request, &mut host, false).unwrap();
        assert_eq!(result.status, 0);
        assert_eq!(
            (
                result.input_frames,
                result.encoded_frames,
                result.access_units
            ),
            (41, 80, 2)
        );
        assert_eq!(result.output_bytes, host.output.len() as u64);
        for kind in [io::ErrorKind::UnexpectedEof, io::ErrorKind::Interrupted] {
            host.failure = Some(kind);
            let error = encode_rust(&request, &mut host, false).unwrap_err();
            assert_eq!(error.kind(), kind);
            assert_eq!(error.to_string(), "original input failure");
        }
        host.failure = None;
        host.output_failure = true;
        let error = encode_rust(&request, &mut host, false).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WriteZero);
        assert_eq!(error.to_string(), "original output failure");
        host.output_failure = false;
        host.panic = true;
        assert_eq!(encode_rust(&request, &mut host, false).unwrap().status, -5);
        host.panic = false;
        request.assignment = 21;
        assert_eq!(encode_rust(&request, &mut host, false).unwrap().status, -1);
    }
}

/// # Errors
/// Rejects unknown operations or missing/invalid output path, frame count, or
/// sample rate. Metadata-writing failures are returned in the JSON `Failure`
/// field rather than as an error from this function.
#[allow(
    clippy::needless_pass_by_value,
    reason = "The public JSON dispatcher retains the owned-request ABI contract"
)]
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
