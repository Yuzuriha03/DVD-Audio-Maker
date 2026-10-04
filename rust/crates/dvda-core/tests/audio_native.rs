//! Independent Rust checks against the frozen pre-migration audio host.
use dvda_core::{audio, hash, media};
use dvda_native::media::{Callbacks, Operation, OutputFormat, Request};
use serde::Deserialize;
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Fixture {
    media_identity: String,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Case {
    rate: u32,
    bits: u32,
    channels: u16,
    frames: usize,
    format: String,
    wave_sha256: String,
    metadata: audio::Metadata,
    parameters: audio::Parameters,
    decodes: Vec<Decode>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Decode {
    target: Option<i32>,
    result: audio::DecodeResult,
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "dvda-rust-audio-中文-日本語-🎵-{}-{}-{}",
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
    polls: usize,
    cancel_at: Option<usize>,
    cancel_on_event: bool,
    emitted: bool,
    panic: bool,
}
impl Callbacks for Events {
    fn emit(&mut self, _: i32, _: &str) {
        assert!(!self.panic, "intentional callback failure");
        self.emitted = true;
    }
    fn cancelled(&mut self) -> bool {
        self.polls += 1;
        self.cancel_at.is_some_and(|n| self.polls >= n) || self.cancel_on_event && self.emitted
    }
}
fn library() -> PathBuf {
    PathBuf::from(std::env::var_os("DVDA_MEDIA_NATIVE_DIR").expect("Set DVDA_MEDIA_NATIVE_DIR"))
        .join("dvda-media.dll")
}
fn job(path: &Path) -> audio::Job {
    audio::Job {
        library: library(),
        input: path.to_str().unwrap().into(),
        operation: audio::Operation::Metadata,
        resample_to: None,
    }
}
fn digest(bytes: &[u8]) -> String {
    hash::reader_digest(Cursor::new(bytes))
        .unwrap()
        .to_uppercase()
}
fn wave(path: &Path, case: &Case) {
    let width = if case.bits == 16 { 2 } else { 3 };
    let mut data = Vec::new();
    for n in 0..case.frames {
        for channel in 0..usize::from(case.channels) {
            let value = (((n * (97 + channel * 18) + channel * 53) % 100001) as i32 - 50000)
                & !((1 << (24 - case.bits)) - 1);
            let stored = if width == 2 { value >> 8 } else { value };
            data.extend_from_slice(&stored.to_le_bytes()[..width]);
        }
    }
    let size = data.len() as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(60 + size + size % 2).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&0xfffeu16.to_le_bytes());
    bytes.extend_from_slice(&case.channels.to_le_bytes());
    bytes.extend_from_slice(&case.rate.to_le_bytes());
    bytes.extend_from_slice(&(case.rate * u32::from(case.channels) * width as u32).to_le_bytes());
    bytes.extend_from_slice(&(case.channels * width as u16).to_le_bytes());
    bytes.extend_from_slice(&(width as u16 * 8).to_le_bytes());
    bytes.extend_from_slice(&22u16.to_le_bytes());
    bytes.extend_from_slice(&(case.bits as u16).to_le_bytes());
    bytes.extend_from_slice(
        &[0u32, 4, 3, 7, 0x33, 0x37, 0x3f][usize::from(case.channels)].to_le_bytes(),
    );
    bytes.extend_from_slice(&[1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]);
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend(data);
    if !size.is_multiple_of(2) {
        bytes.push(0);
    }
    assert_eq!(digest(&bytes), case.wave_sha256);
    fs::write(path, bytes).unwrap();
}
fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/audio-managed-v1.json")).unwrap()
}
fn media_identity() -> String {
    let mut paths: Vec<_> = fs::read_dir(library().parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
        })
        .collect();
    paths.sort_by_key(|p| p.file_name().unwrap().to_string_lossy().to_lowercase());
    let text = paths
        .iter()
        .map(|p| {
            format!(
                "{}:{}",
                p.file_name().unwrap().to_string_lossy(),
                digest(&fs::read(p).unwrap())
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    format!("native-media-v1:{}", digest(text.as_bytes()))
}
#[test]
#[ignore = "requires the frozen source-built Windows x64 media component"]
fn frozen_audio_metadata_parameters_and_sample_counts() {
    let fixtures = fixture();
    assert_eq!(fixtures.media_identity, media_identity());
    assert_eq!(fixtures.cases.len(), 168);
    let root = Scratch::new();
    let input = root.0.join("input.wav");
    let flac = root.0.join("input.flac");
    let mut decodes = 0;
    for case in fixtures.cases {
        wave(&input, &case);
        let source = if case.format == "wav" {
            &input
        } else {
            let result = media::execute(
                media::Job {
                    library: library(),
                    replace: true,
                    timeout_millis: None,
                    request: Request {
                        operation: Operation::Audio,
                        rate: 0,
                        bits: 0,
                        output_format: OutputFormat::Flac,
                        soxr: false,
                        compression: 8,
                        cover: false,
                        input: input.to_str().unwrap().into(),
                        output: Some(flac.to_str().unwrap().into()),
                        tags: [
                            ("TITLE", " 曲目 日本語 🎵  "),
                            ("ALBUM", "Album=中文"),
                            ("DATE", "2026-01-02"),
                            ("TRACK", "03/12"),
                        ]
                        .map(|(k, v)| (k.into(), v.into()))
                        .into(),
                    },
                },
                &mut Events::default(),
            );
            assert!(result.failure.is_none(), "{:?}", result.failure);
            assert_eq!(result.exit_code, Some(0));
            &flac
        };
        let mut request = job(source);
        let mut metadata = audio::read_metadata(&request, &mut Events::default()).unwrap();
        metadata.path = source.file_name().unwrap().to_str().unwrap().into();
        assert_eq!(
            metadata, case.metadata,
            "{}/{}/{}/{}",
            case.rate, case.bits, case.channels, case.format
        );
        assert_eq!(
            audio::read_parameters(&request, &mut Events::default()).unwrap(),
            case.parameters
        );
        for expected in case.decodes {
            request.resample_to = expected.target;
            assert_eq!(
                audio::check_decode(&request, &mut Events::default()).unwrap(),
                expected.result
            );
            decodes += 1;
        }
    }
    assert_eq!(decodes, 1176);
}
#[test]
#[ignore = "requires the source-built Windows x64 media DLL"]
fn cancellation_failures_and_no_writes() {
    let root = Scratch::new();
    let input = root.0.join("input.wav");
    let fixture = fixture();
    wave(&input, &fixture.cases[0]);
    let original = fs::read(&input).unwrap();
    let request = job(&input);
    for cancel_at in [1, 2, 3, 5] {
        assert_eq!(
            audio::check_decode(
                &request,
                &mut Events {
                    cancel_at: Some(cancel_at),
                    ..Default::default()
                }
            )
            .unwrap_err()
            .kind,
            "Cancelled"
        );
    }
    assert_eq!(
        audio::check_decode(
            &request,
            &mut Events {
                cancel_on_event: true,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        "Cancelled"
    );
    assert_eq!(
        audio::check_decode(
            &request,
            &mut Events {
                panic: true,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        "Io"
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    for path in [&root.0.join("absent.wav"), &input, &root.0] {
        if *path == input {
            fs::write(path, b"malformed audio").unwrap();
        }
        let request = job(path);
        assert_eq!(
            audio::read_metadata(&request, &mut Events::default())
                .unwrap_err()
                .kind,
            "InvalidData"
        );
        assert_eq!(
            audio::read_parameters(&request, &mut Events::default())
                .unwrap_err()
                .kind,
            "InvalidData"
        );
        let decoded = audio::check_decode(&request, &mut Events::default()).unwrap();
        assert_eq!(decoded.exit_code, 1);
        assert!(decoded.error_count > 0);
        assert!(decoded.samples.is_none());
    }
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
}
