//! Standalone tests: no managed runner or external converter is involved.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::needless_pass_by_value,
    clippy::struct_excessive_bools,
    reason = "Media fixtures use explicit callback switches and bounded PCM test data"
)]
use dvda_core::media::{Job, Outcome, execute};
use dvda_native::media::{Callbacks, Operation, OutputFormat, Request};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "dvda-rust-media-日本語-中文-🎵-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[derive(Default)]
struct Events {
    text: Vec<(i32, String)>,
    cancel: bool,
    cancel_on_emit: bool,
    delay: bool,
    panic: bool,
}
impl Callbacks for Events {
    fn emit(&mut self, stream: i32, text: &str) {
        assert!(!self.panic, "intentional callback failure");
        self.text.push((stream, text.into()));
        self.cancel |= self.cancel_on_emit;
        if self.delay {
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    fn cancelled(&mut self) -> bool {
        self.cancel
    }
}
fn library() -> PathBuf {
    std::env::current_exe().unwrap()
}
fn job(input: &Path, output: &Path, rate: u32, bits: u32) -> Job {
    Job {
        library: library(),
        replace: false,
        timeout_millis: None,
        request: Request {
            operation: Operation::Audio,
            rate,
            bits: if bits == 16 { 24 } else { bits },
            output_format: OutputFormat::S24,
            soxr: false,
            dither: false,
            compression: 8,
            cover: false,
            input: input.to_str().unwrap().into(),
            output: Some(output.to_str().unwrap().into()),
            tags: vec![],
        },
    }
}
fn passed(result: Outcome) {
    assert!(result.failure.is_none(), "{:?}", result.failure);
    assert_eq!(result.exit_code, Some(0));
}
fn wave(path: &Path, rate: u32, bits: u32, channels: u16) -> Vec<u8> {
    let frames = 417usize;
    let width = if bits == 16 { 2usize } else { 3 };
    let mut data = Vec::new();
    let mut expected = Vec::new();
    let mut seed = 0x7132_91ab_u32;
    for index in 0..frames * channels as usize {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let value = match index % 17 {
            0 => -8_388_608,
            1 => 8_388_607,
            2 => 0,
            3 => 1,
            4 => -1,
            _ => (seed >> 8) as i32 - 8_388_608,
        };
        let value = value & !((1 << (24 - bits)) - 1);
        expected.extend_from_slice(&value.to_le_bytes()[..3]);
        let stored = if bits == 16 { value >> 8 } else { value };
        data.extend_from_slice(&stored.to_le_bytes()[..width]);
    }
    let size = data.len() as u32;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(60 + size + size % 2).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&40u32.to_le_bytes());
    wav.extend_from_slice(&0xfffeu16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * u32::from(channels) * width as u32).to_le_bytes());
    wav.extend_from_slice(&(channels * width as u16).to_le_bytes());
    wav.extend_from_slice(&(width as u16 * 8).to_le_bytes());
    wav.extend_from_slice(&22u16.to_le_bytes());
    wav.extend_from_slice(&(bits as u16).to_le_bytes());
    wav.extend_from_slice(&[0u32, 4, 3, 7, 0x33, 0x37, 0x3f][channels as usize].to_le_bytes());
    wav.extend_from_slice(&[1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]);
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&size.to_le_bytes());
    wav.extend(data);
    if !size.is_multiple_of(2) {
        wav.push(0);
    }
    fs::write(path, wav).unwrap();
    expected
}

#[test]
#[ignore = "requires the source-built Windows x64 media DLL"]
fn rust_wave_probe_packets_and_signature_detection() {
    let root = Scratch::new();
    let input = root.0.join("input.bin");
    let output = root.0.join("output.raw");
    let expected = wave(&input, 48000, 24, 2);
    let mut probe = job(&input, &output, 48000, 24);
    probe.request.operation = Operation::Probe;
    probe.request.output = None;
    probe.request.output_format = OutputFormat::None;
    let mut events = Events::default();
    passed(execute(probe, &mut events));
    let json: serde_json::Value = serde_json::from_str(
        &events
            .text
            .iter()
            .filter(|(stream, _)| *stream == 1)
            .map(|(_, text)| text.as_str())
            .collect::<String>(),
    )
    .unwrap();
    assert_eq!(json["streams"][0]["codec_name"], "pcm_s24le");
    assert_eq!(json["streams"][0]["channels"], 2);
    assert_eq!(json["streams"][0]["duration_ts"], 417);
    let mut packets = job(&input, &output, 48000, 24);
    packets.request.operation = Operation::Packets;
    packets.request.output = None;
    packets.request.output_format = OutputFormat::None;
    let mut events = Events::default();
    passed(execute(packets, &mut events));
    assert!(
        events
            .text
            .iter()
            .any(|(stream, text)| *stream == 1 && text == "0.000000000,0.008687500,2502,68"),
        "{:?}",
        events.text
    );
    passed(execute(
        job(&input, &output, 48000, 24),
        &mut Events::default(),
    ));
    assert_eq!(fs::read(&output).unwrap(), expected);
}

#[test]
#[ignore = "requires the source-built Windows x64 media DLL"]
fn rust_wave_s16_output_preserves_full_precision_input() {
    let root = Scratch::new();
    let input = root.0.join("input.wav");
    let output = root.0.join("output.raw");
    let source = wave(&input, 48000, 16, 2);
    let expected: Vec<u8> = source
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|sample| [sample[1], sample[2]])
        .collect();
    let mut request = job(&input, &output, 48000, 16);
    request.request.bits = 16;
    request.request.output_format = OutputFormat::S16;
    passed(execute(request, &mut Events::default()));
    assert_eq!(fs::read(output).unwrap(), expected);
}

#[test]
#[ignore = "requires the source-built Windows x64 media DLL"]
fn rust_wave_pcm_encodings_preserve_exact_samples() {
    let root = Scratch::new();
    for (tag, bits) in [(1u16, 8u16), (1, 16), (1, 24), (1, 32), (3, 32), (3, 64)] {
        let input = root.0.join(format!("pcm-{tag}-{bits}.wav"));
        let output = root.0.join(format!("pcm-{tag}-{bits}.raw"));
        let values = [-128i32, -127, -1, 0, 1, 126, 127];
        let mut data = Vec::new();
        let mut expected = Vec::new();
        for value in values.into_iter().cycle().take(5003) {
            match (tag, bits) {
                (1, 8) => data.push((value + 128) as u8),
                (1, 16) => data.extend(((value << 8) as i16).to_le_bytes()),
                (1, 24) => data.extend(&(value << 16).to_le_bytes()[..3]),
                (1, 32) => data.extend((value << 24).to_le_bytes()),
                (3, 32) => data.extend((value as f32 / 128.0).to_le_bytes()),
                (3, 64) => data.extend((f64::from(value) / 128.0).to_le_bytes()),
                _ => unreachable!(),
            }
            expected.extend_from_slice(&(value << 16).to_le_bytes()[..3]);
        }
        let size = data.len() as u32;
        let mut bytes = b"RIFF".to_vec();
        bytes.extend((36 + size + size % 2).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(tag.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(48000u32.to_le_bytes());
        bytes.extend((48000 * u32::from(bits / 8)).to_le_bytes());
        bytes.extend((bits / 8).to_le_bytes());
        bytes.extend(bits.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(size.to_le_bytes());
        bytes.extend(data);
        if !size.is_multiple_of(2) {
            bytes.push(0);
        }
        fs::write(&input, bytes).unwrap();
        passed(execute(
            job(&input, &output, 48000, 24),
            &mut Events::default(),
        ));
        assert_eq!(
            fs::read(&output).unwrap(),
            expected,
            "tag {tag}, bits {bits}"
        );
        let mut results = Vec::new();
        for enabled in [false, true] {
            let mut request = job(&input, &output, 48000, 16);
            request.replace = true;
            request.request.bits = 16;
            request.request.output_format = OutputFormat::S16;
            request.request.dither = enabled;
            let mut events = Events::default();
            passed(execute(request, &mut events));
            assert_eq!(
                events
                    .text
                    .iter()
                    .filter(|(_, text)| text.contains("applying Rust TPDF"))
                    .count(),
                usize::from(enabled && (tag == 3 || bits > 16)),
                "tag {tag}, bits {bits}"
            );
            results.push(fs::read(&output).unwrap());
        }
        if tag == 1 && bits <= 16 {
            assert_eq!(results[0], results[1]);
        } else {
            assert_ne!(results[0], results[1]);
        }
    }
}

#[test]
#[ignore = "requires source-built native media libraries"]
fn dither_policy_uses_original_source_precision() {
    let root = Scratch::new();
    let input = root.0.join("input.wav");
    let output = root.0.join("output.raw");
    for source_bits in [16, 20, 24] {
        wave(&input, 48000, source_bits, 2);
        for rate in [48000, 44100] {
            let mut results = Vec::new();
            for enabled in [false, true] {
                let mut request = job(&input, &output, rate, 16);
                request.replace = true;
                request.request.bits = 16;
                request.request.output_format = OutputFormat::S16;
                request.request.dither = enabled;
                let mut events = Events::default();
                passed(execute(request, &mut events));
                assert_eq!(
                    events
                        .text
                        .iter()
                        .filter(|(_, text)| text.contains("applying Rust TPDF"))
                        .count(),
                    usize::from(enabled && source_bits > 16),
                );
                results.push(fs::read(&output).unwrap());
            }
            if source_bits == 16 {
                assert_eq!(results[0], results[1], "16-bit source at {rate}");
            } else {
                assert_ne!(results[0], results[1], "{source_bits}-bit source at {rate}");
            }
        }
    }
}

#[test]
#[ignore = "requires the source-built Windows x64 media DLL"]
fn exact_pcm_matrix_and_transaction_failures() {
    let root = Scratch::new();
    let input = root.0.join("input.wav");
    let output = root.0.join("output.raw");
    let mut cases = 0;
    for rate in [44100, 48000, 88200, 96000, 176_400, 192_000] {
        for bits in [16, 20, 24] {
            for channels in 1..=if rate > 96000 { 2 } else { 6 } {
                let expected = wave(&input, rate, bits, channels);
                passed(execute(
                    job(&input, &output, rate, bits),
                    &mut Events::default(),
                ));
                let actual = fs::read(&output).unwrap();
                assert_eq!(
                    actual.len(),
                    expected.len(),
                    "{rate}/{bits}/{channels} length"
                );
                let mismatch = actual.iter().zip(&expected).position(|(a, b)| a != b);
                assert_eq!(mismatch, None, "{rate}/{bits}/{channels} first mismatch");
                fs::remove_file(&output).unwrap();
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 84);
    wave(&input, 48000, 24, 2);
    fs::write(&output, b"preserve").unwrap();
    let mut incompatible = job(&input, &output, 48000, 24);
    incompatible.replace = true;
    incompatible.request.bits = 16;
    assert!(
        execute(incompatible, &mut Events::default())
            .failure
            .is_some()
    );
    assert_eq!(fs::read(&output).unwrap(), b"preserve");
    assert!(
        execute(job(&input, &output, 48000, 24), &mut Events::default())
            .failure
            .is_some()
    );
    for (mut events, timeout, kind) in [
        (
            Events {
                cancel: true,
                ..Default::default()
            },
            None,
            "Cancelled",
        ),
        (
            Events {
                cancel_on_emit: true,
                ..Default::default()
            },
            None,
            "Cancelled",
        ),
        (
            Events {
                delay: true,
                ..Default::default()
            },
            Some(10),
            "Timeout",
        ),
        (
            Events {
                panic: true,
                ..Default::default()
            },
            None,
            "Io",
        ),
    ] {
        let mut request = job(&input, &output, 48000, 24);
        request.replace = true;
        request.timeout_millis = timeout;
        assert_eq!(execute(request, &mut events).failure.unwrap().kind, kind);
        assert_eq!(fs::read(&output).unwrap(), b"preserve");
    }
    let original = fs::read(&input).unwrap();
    let mut same = job(&input, &input, 48000, 24);
    same.replace = true;
    assert!(execute(same, &mut Events::default()).failure.is_some());
    assert_eq!(fs::read(&input).unwrap(), original);
    fs::write(&input, b"invalid audio").unwrap();
    let mut invalid = job(&input, &output, 48000, 24);
    invalid.replace = true;
    assert_eq!(execute(invalid, &mut Events::default()).exit_code, Some(1));
    assert_eq!(fs::read(&output).unwrap(), b"preserve");
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 2);
}
