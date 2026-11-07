//! Frozen pre-migration host references; this test does not execute C#.
#[cfg(feature = "rust-mlp")]
use dvda_core::audio;
use dvda_core::{encoder, hash, media};
use dvda_native::media::{Callbacks, Operation, OutputFormat, Request};
use serde::Deserialize;
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Case {
    rate: u32,
    bits: u32,
    channels: u16,
    frames: usize,
    mask: Option<u32>,
    metadata_version: u32,
    wide: bool,
    wave_sha256: String,
    mlp_sha256: String,
    bytes: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Fixtures {
    cases: Vec<Case>,
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "dvda-rust-encoder-中文-日本語-🎵-{}-{}-{}",
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
    completed: usize,
    polls: usize,
    cancel_after: Option<usize>,
    cancel_on_complete: bool,
}
impl Callbacks for Events {
    fn emit(&mut self, stream: i32, _: &str) {
        if stream == 4 {
            self.completed += 1;
        }
    }
    fn cancelled(&mut self) -> bool {
        self.polls += 1;
        self.cancel_after.is_some_and(|n| self.polls >= n)
            || (self.cancel_on_complete && self.completed != 0)
    }
}
fn media_library() -> PathBuf {
    std::env::current_exe().unwrap()
}
fn passed(outcome: &media::Outcome) {
    assert!(outcome.failure.is_none(), "{:?}", outcome.failure);
    assert_eq!(outcome.exit_code, Some(0));
}
#[allow(
    clippy::cast_precision_loss,
    reason = "Fixture frame counts are small, but duration_seconds is intentionally an approximate floating-point value"
)]
fn duration_seconds(case: &Case) -> f64 {
    case.frames as f64 / f64::from(case.rate)
}

fn digest(bytes: &[u8]) -> String {
    hash::reader_digest(Cursor::new(bytes))
        .unwrap()
        .to_uppercase()
}

// Integer definition frozen with the independent managed reference. Values use
// the 24-bit PCM domain; valid-bit precision is enforced before storage packing.
fn wave(path: &Path, case: &Case) -> Vec<u8> {
    let width: usize = if case.wide {
        4
    } else if case.bits == 16 {
        2
    } else {
        3
    };
    let width32 = u32::try_from(width).unwrap();
    let width16 = u16::try_from(width).unwrap();
    let mut data = Vec::new();
    let mut expected = Vec::new();
    for n in 0..case.frames {
        for channel in 0..usize::from(case.channels) {
            let value = (i32::try_from((n * (97 + channel * 18) + channel * 53) % 100_001)
                .unwrap()
                - 50_000)
                & !((1 << (24 - case.bits)) - 1);
            expected.extend_from_slice(&value.to_le_bytes()[..3]);
            let stored = if case.wide {
                value << 8
            } else if width == 2 {
                value >> 8
            } else {
                value
            };
            data.extend_from_slice(&stored.to_le_bytes()[..width]);
        }
    }
    let size = u32::try_from(data.len()).unwrap();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(60 + size + size % 2).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&0xfffeu16.to_le_bytes());
    bytes.extend_from_slice(&case.channels.to_le_bytes());
    bytes.extend_from_slice(&case.rate.to_le_bytes());
    bytes.extend_from_slice(&(case.rate * u32::from(case.channels) * width32).to_le_bytes());
    bytes.extend_from_slice(&(case.channels * width16).to_le_bytes());
    bytes.extend_from_slice(&(width16 * 8).to_le_bytes());
    bytes.extend_from_slice(&22u16.to_le_bytes());
    bytes.extend_from_slice(&(u16::try_from(case.bits).unwrap()).to_le_bytes());
    let mask = case
        .mask
        .unwrap_or([0, 4, 3, 7, 0x33, 0x37, 0x3f][usize::from(case.channels)]);
    bytes.extend_from_slice(&mask.to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]);
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend(data);
    if !size.is_multiple_of(2) {
        bytes.push(0);
    }
    fs::write(path, bytes).unwrap();
    expected
}
fn context(path: &Path, case: &Case) -> String {
    // Frozen files used an empty stamp; supply it explicitly now that the
    // production default records the machine identity and encoding time.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(if case.metadata_version == 2 {
        b"MSCTX002"
    } else {
        b"MSCTX001"
    });
    bytes.extend_from_slice(
        &encoder::access_units(i64::try_from(case.frames).unwrap(), case.rate)
            .unwrap()
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&4u32.to_le_bytes());
    if case.metadata_version == 2 {
        bytes.extend_from_slice(&0u32.to_le_bytes());
    }
    bytes.extend_from_slice(&[0, 0, 0x40, 0]);
    fs::write(path, bytes).unwrap();
    path.to_str().unwrap().into()
}
fn decode(input: &Path, output: &Path) {
    passed(&media::execute(
        media::Job {
            library: media_library(),
            request: Request {
                operation: Operation::Audio,
                rate: 0,
                bits: 24,
                output_format: OutputFormat::S24,
                soxr: false,
                dither: false,
                compression: 8,
                cover: false,
                input: input.to_str().unwrap().into(),
                output: Some(output.to_str().unwrap().into()),
                tags: vec![],
            },
            replace: false,
            timeout_millis: Some(30000),
        },
        &mut Events::default(),
    ));
}

#[test]
#[cfg(feature = "rust-mlp")]
fn linked_mlp_matrix_without_encoder_dll() {
    let fixtures: Fixtures =
        serde_json::from_str(include_str!("fixtures/encoder-managed-v1.json")).unwrap();
    assert_eq!(fixtures.cases.len(), 116);
    let root = Scratch::new();
    let input = root.0.join("input.wav");
    let output = root.0.join("output.mlp");
    let stamp = root.0.join("context.bin");
    let missing_library = root.0.join("missing-encoder.dll");
    for (index, case) in fixtures.cases.iter().enumerate() {
        wave(&input, case);
        assert_eq!(digest(&fs::read(&input).unwrap()), case.wave_sha256);
        let mut events = Events::default();
        passed(&encoder::execute(
            encoder::Job {
                library: missing_library.clone(),
                wave: input.to_string_lossy().into_owned(),
                destination: output.clone(),
                metadata_context: context(&stamp, case),
                timeout_millis: None,
            },
            &mut events,
        ));
        assert_eq!(events.completed, 1);
        let actual = fs::read(&output).unwrap();
        assert_eq!(actual.len(), case.bytes, "case {index}: {case:?}");
        assert_eq!(digest(&actual), case.mlp_sha256, "case {index}: {case:?}");
        assert!(!missing_library.exists());
        fs::remove_file(&output).unwrap();
    }
}

#[test]
#[cfg(feature = "rust-mlp")]
#[ignore = "requires source-built media runtime"]
fn streamed_decode_to_mlp_matches_the_lossless_pcm_matrix_without_wave_output() {
    let fixtures: Fixtures =
        serde_json::from_str(include_str!("fixtures/encoder-managed-v1.json")).unwrap();
    let root = Scratch::new();
    let input = root.0.join("input.wav");
    let output = root.0.join("streamed.mlp");
    let raw = root.0.join("decoded.raw");
    let unwanted = root.0.join("production.wav");
    let normalized = root.0.join("normalized.wav");
    for (index, case) in fixtures.cases.iter().enumerate() {
        let expected = wave(&input, case);
        let result = audio::check_decode(
            &audio::Job {
                library: media_library(),
                input: input.to_string_lossy().into_owned(),
                operation: audio::Operation::Decode,
                resample_to: None,
                decode_output: Some(audio::DecodeOutput {
                    output: unwanted.clone(),
                    normalized: normalized.clone(),
                    rate: i32::try_from(case.rate).unwrap(),
                    bits: i32::try_from(case.bits).unwrap(),
                    dither: false,
                    mlp_output: Some(output.clone()),
                }),
            },
            &mut Events::default(),
        )
        .unwrap();
        assert_eq!(result.error_count, 0, "case {index}: {case:?}");
        assert_eq!(result.samples, Some(i64::try_from(case.frames).unwrap()));
        assert!(!unwanted.exists());
        assert!(!normalized.exists());
        decode(&output, &raw);
        let decoded = fs::read(&raw).unwrap();
        let block = 40
            * (case.rate
                / if case.rate.is_multiple_of(44100) {
                    44100
                } else {
                    48000
                });
        let padded_frames = case.frames.div_ceil(block as usize) * block as usize;
        assert_eq!(
            decoded.len(),
            padded_frames * usize::from(case.channels) * 3,
            "decoded length {index}"
        );
        assert_eq!(&decoded[..expected.len()], expected, "PCM case {index}");
        assert!(decoded[expected.len()..].iter().all(|byte| *byte == 0));
        fs::remove_file(&output).unwrap();
        fs::remove_file(&raw).unwrap();
    }
}

#[test]
#[cfg(feature = "rust-mlp")]
#[ignore = "requires source-built media runtime"]
fn streamed_resampling_matches_the_wave_encoding_path() {
    let root = Scratch::new();
    let input = root.0.join("input.wav");
    let case = Case {
        rate: 44100,
        bits: 24,
        channels: 2,
        frames: 8193,
        mask: None,
        metadata_version: 0,
        wide: false,
        wave_sha256: String::new(),
        mlp_sha256: String::new(),
        bytes: 0,
    };
    wave(&input, &case);
    let streamed = root.0.join("streamed.mlp");
    let result = audio::check_decode(
        &audio::Job {
            library: media_library(),
            input: input.to_string_lossy().into_owned(),
            operation: audio::Operation::Decode,
            resample_to: None,
            decode_output: Some(audio::DecodeOutput {
                output: root.0.join("unused.wav"),
                normalized: root.0.join("unused-normalized.wav"),
                rate: 48000,
                bits: 24,
                dither: false,
                mlp_output: Some(streamed.clone()),
            }),
        },
        &mut Events::default(),
    )
    .unwrap();
    assert_eq!(result.error_count, 0);
    assert!(
        result
            .samples
            .is_some_and(|samples| samples > i64::try_from(case.frames).unwrap())
    );
    let pcm = root.0.join("baseline.wav");
    passed(&media::execute(
        media::Job {
            library: media_library(),
            request: Request {
                operation: Operation::Audio,
                rate: 48000,
                bits: 24,
                output_format: OutputFormat::Wave,
                soxr: false,
                dither: false,
                compression: 8,
                cover: false,
                input: input.to_string_lossy().into_owned(),
                output: Some(pcm.to_string_lossy().into_owned()),
                tags: Vec::new(),
            },
            replace: false,
            timeout_millis: None,
        },
        &mut Events::default(),
    ));
    let baseline = root.0.join("baseline.mlp");
    passed(&encoder::execute(
        encoder::Job {
            library: root.0.join("unused-encoder.dll"),
            wave: pcm.to_string_lossy().into_owned(),
            destination: baseline.clone(),
            metadata_context: String::new(),
            timeout_millis: None,
        },
        &mut Events::default(),
    ));
    let streamed_raw = root.0.join("streamed.raw");
    let baseline_raw = root.0.join("baseline.raw");
    decode(&streamed, &streamed_raw);
    decode(&baseline, &baseline_raw);
    let streamed_bytes = fs::read(&streamed_raw).unwrap();
    assert_ne!(streamed_bytes, [] as [u8; 0]);
    assert_eq!(streamed_bytes, fs::read(&baseline_raw).unwrap());
}

#[test]
#[cfg(feature = "rust-mlp")]
#[ignore = "requires source-built media runtime"]
fn streamed_decode_cancellation_discards_all_outputs() {
    let root = Scratch::new();
    let input = root.0.join("input.wav");
    let case = Case {
        rate: 48000,
        bits: 24,
        channels: 2,
        frames: 200_003,
        mask: None,
        metadata_version: 0,
        wide: false,
        wave_sha256: String::new(),
        mlp_sha256: String::new(),
        bytes: 0,
    };
    wave(&input, &case);
    for polls in [1, 3, 10] {
        let output = root.0.join("cancelled.mlp");
        let error = audio::check_decode(
            &audio::Job {
                library: media_library(),
                input: input.to_string_lossy().into_owned(),
                operation: audio::Operation::Decode,
                resample_to: None,
                decode_output: Some(audio::DecodeOutput {
                    output: root.0.join("unused.wav"),
                    normalized: root.0.join("unused-normalized.wav"),
                    rate: 48000,
                    bits: 24,
                    dither: false,
                    mlp_output: Some(output.clone()),
                }),
            },
            &mut Events {
                cancel_after: Some(polls),
                ..Events::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.kind, "Cancelled");
        assert!(!output.exists());
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
    }
}

fn batch_job(root: &Path, cases: &[Case]) -> dvda_core::batch::Job {
    let media_library = media_library();
    dvda_core::batch::Job {
        media_library,
        encoder_library: root.join("missing-encoder.dll"),
        temporary_directory: root.join("temp"),
        output_directory: root.join("output"),
        sample_rate: i32::try_from(cases[0].rate).unwrap(),
        bits: i32::try_from(cases[0].bits).unwrap(),
        dither: false,
        metadata_context: String::new(),
        tracks: cases
            .iter()
            .enumerate()
            .map(|(index, case)| {
                let source = root.join(format!("input-{index}.wav"));
                wave(&source, case);
                dvda_core::batch::Track {
                    source_path: source.to_str().unwrap().into(),
                    work_name: format!("track{index}"),
                    display_name: format!("音轨 日本語 🎵 {index}"),
                    duration_seconds: duration_seconds(case),
                    source_identity: serde_json::Value::Null,
                    source_evidence: serde_json::Value::Null,
                }
            })
            .collect(),
    }
}
struct BatchEvents {
    caller: std::thread::ThreadId,
    events: Vec<serde_json::Value>,
    cancel: bool,
    cancel_on_started: bool,
    panic_on_started: bool,
}
impl BatchEvents {
    fn new() -> Self {
        Self {
            caller: std::thread::current().id(),
            events: vec![],
            cancel: false,
            cancel_on_started: false,
            panic_on_started: false,
        }
    }
}
impl Callbacks for BatchEvents {
    fn emit(&mut self, stream: i32, text: &str) {
        assert_eq!(
            std::thread::current().id(),
            self.caller,
            "UI callbacks escaped the invoking thread"
        );
        assert_eq!(stream, 5);
        let event: serde_json::Value = serde_json::from_str(text).unwrap();
        if event["Kind"] == "Started" {
            assert!(!self.panic_on_started, "Intentional batch callback panic");
            self.cancel |= self.cancel_on_started;
        }
        self.events.push(event);
    }
    fn cancelled(&mut self) -> bool {
        assert_eq!(std::thread::current().id(), self.caller);
        self.cancel
    }
}
#[test]
#[cfg(feature = "rust-mlp")]
#[ignore = "requires source-built media DLL"]
fn execute_with_runs_finalizers_concurrently_and_preserves_result_order() {
    let fixtures: Fixtures =
        serde_json::from_str(include_str!("fixtures/encoder-managed-v1.json")).unwrap();
    let root = Scratch::new();
    let active = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let peak = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (outcome, results) = dvda_core::batch::execute_with(
        batch_job(&root.0, &fixtures.cases[..3]),
        &mut Events::default(),
        {
            let active = active.clone();
            let peak = peak.clone();
            let calls = calls.clone();
            move |index, _, _| {
                active.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                peak.fetch_max(
                    active.load(std::sync::atomic::Ordering::Acquire),
                    std::sync::atomic::Ordering::AcqRel,
                );
                std::thread::sleep(std::time::Duration::from_millis(25));
                calls.lock().unwrap().push(index);
                active.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
                Ok(index)
            }
        },
    );
    passed(&outcome);
    assert_eq!(results, [0, 1, 2]);
    assert_eq!(calls.lock().unwrap().len(), 3);
    assert!(peak.load(std::sync::atomic::Ordering::Acquire) >= 2);

    let root = Scratch::new();
    let (outcome, results) = dvda_core::batch::execute_with(
        batch_job(&root.0, &fixtures.cases[..2]),
        &mut Events::default(),
        |index, _, _| {
            if index == 1 {
                Err(std::io::Error::other("intentional finalize failure").into())
            } else {
                Ok(index)
            }
        },
    );
    assert_eq!(
        outcome.failure.unwrap().message,
        "intentional finalize failure"
    );
    assert_eq!(results, [] as [usize; 0]);

    let root = Scratch::new();
    let (outcome, results) = dvda_core::batch::execute_with(
        batch_job(&root.0, &fixtures.cases[..1]),
        &mut Events {
            cancel_after: Some(1),
            ..Events::default()
        },
        |index, _, events| {
            assert!(events.cancelled());
            Ok(index)
        },
    );
    assert_eq!(outcome.failure.unwrap().kind, "Cancelled");
    assert_eq!(results, [] as [usize; 0]);
}

#[test]
#[ignore = "opt-in synthetic native batch worker benchmark"]
#[allow(
    clippy::too_many_lines,
    reason = "The opt-in benchmark keeps generation, repeated execution, and byte verification stages ordered"
)]
fn synthetic_batch_worker_benchmark() {
    assert_eq!(std::env::var("DVDA_BATCH_BENCH").as_deref(), Ok("1"));
    let root = PathBuf::from(std::env::var_os("DVDA_BATCH_BENCH_ROOT").unwrap());
    fs::create_dir(&root).expect("benchmark root must be new; never overwrite user files");
    let report = root.join("worker-benchmark.json");
    let executable = fs::read(std::env::var_os("DVDA_BATCH_BENCH_EXE").unwrap()).unwrap();
    let archive = executable
        .windows(8)
        .enumerate()
        .find_map(|(offset, bytes)| {
            if bytes != b"DVDRUN01" {
                return None;
            }
            let length =
                u32::from_le_bytes(executable.get(offset + 8..offset + 12)?.try_into().ok()?)
                    as usize;
            let metadata: serde_json::Value =
                serde_json::from_slice(executable.get(offset + 12..offset + 12 + length)?).ok()?;
            let end = metadata.as_array()?.iter().try_fold(0usize, |end, entry| {
                Some(end.max(
                    usize::try_from(entry["offset"].as_u64()?).ok()?
                        + usize::try_from(entry["length"].as_u64()?).ok()?,
                ))
            })?;
            executable.get(offset..offset + 12 + length + end)
        })
        .expect("embedded verified runtime archive");
    let _runtime = dvda_core::runtime::extract(archive, &root.join("runtime")).unwrap();
    let media_library = media_library();
    let fixtures: Fixtures =
        serde_json::from_str(include_str!("fixtures/encoder-managed-v1.json")).unwrap();
    let mut measurements = Vec::new();
    let mut verified = 0;
    for bits in [16, 20, 24] {
        let cases: Vec<_> = (0..16)
            .map(|index| {
                let mut case = fixtures
                    .cases
                    .iter()
                    .find(|case| {
                        case.rate == 48000
                            && case.bits == bits
                            && case.channels == if index % 2 == 0 { 2 } else { 6 }
                    })
                    .unwrap()
                    .clone();
                case.frames = 24013 + index * 7;
                case.wide = false;
                case.mask = Some(if case.channels == 2 { 3 } else { 0x3f });
                case.metadata_version = 0;
                case
            })
            .collect();
        let input = root.join(format!("input-{bits}"));
        fs::create_dir(&input).unwrap();
        let tracks: Vec<_> = cases
            .iter()
            .enumerate()
            .map(|(index, case)| {
                let source = input.join(format!("track{index}.wav"));
                wave(&source, case);
                dvda_core::batch::Track {
                    source_path: source.to_str().unwrap().into(),
                    work_name: format!("track{index}"),
                    display_name: format!("synthetic {bits}-bit {}ch {index}", case.channels),
                    duration_seconds: duration_seconds(case),
                    source_identity: serde_json::Value::Null,
                    source_evidence: serde_json::Value::Null,
                }
            })
            .collect();
        let mut reference = Vec::new();
        for repeat in 0..3 {
            for run_index in 0..5 {
                let run = root.join(format!("run-{bits}-{repeat}-{run_index}"));
                fs::create_dir(&run).unwrap();
                let job = dvda_core::batch::Job {
                    media_library: media_library.clone(),
                    encoder_library: media_library.clone(),
                    temporary_directory: run.join("temp"),
                    output_directory: run.join("output"),
                    sample_rate: 48000,
                    bits: i32::try_from(bits).unwrap(),
                    dither: false,
                    metadata_context: String::new(),
                    tracks: tracks
                        .iter()
                        .map(|track| dvda_core::batch::Track {
                            source_path: track.source_path.clone(),
                            work_name: track.work_name.clone(),
                            display_name: track.display_name.clone(),
                            duration_seconds: track.duration_seconds,
                            source_identity: serde_json::Value::Null,
                            source_evidence: serde_json::Value::Null,
                        })
                        .collect(),
                };
                let mut events = BatchEvents::new();
                let started = std::time::Instant::now();
                passed(&dvda_core::batch::execute(job, &mut events));
                let seconds = started.elapsed().as_secs_f64();
                assert_eq!(
                    events
                        .events
                        .iter()
                        .filter(|event| event["Kind"] == "Encoded")
                        .count(),
                    16
                );
                assert_eq!(fs::read_dir(run.join("temp")).unwrap().count(), 0);
                let mut output_bytes = 0;
                for (index, case) in cases.iter().enumerate() {
                    let mlp = run.join("output").join(format!("track{index}.mlp"));
                    let bytes = fs::read(&mlp).unwrap();
                    output_bytes += bytes.len();
                    let checksum = digest(&bytes);
                    if repeat == 0 && run_index == 0 {
                        reference.push(checksum);
                    } else {
                        assert_eq!(
                            checksum, reference[index],
                            "non-deterministic automatic-policy output"
                        );
                    }
                    let expected = wave(&run.join(format!("expected-{index}.wav")), case);
                    let raw = run.join(format!("decoded-{index}.raw"));
                    passed(&media::execute(
                        media::Job {
                            library: media_library.clone(),
                            request: Request {
                                operation: Operation::Audio,
                                rate: 0,
                                bits: 24,
                                output_format: OutputFormat::S24,
                                soxr: false,
                                dither: false,
                                compression: 8,
                                cover: false,
                                input: mlp.to_str().unwrap().into(),
                                output: Some(raw.to_str().unwrap().into()),
                                tags: vec![],
                            },
                            replace: false,
                            timeout_millis: Some(30000),
                        },
                        &mut Events::default(),
                    ));
                    let decoded = fs::read(&raw).unwrap();
                    assert_eq!(
                        decoded.len(),
                        case.frames.div_ceil(40) * 40 * usize::from(case.channels) * 3
                    );
                    assert_eq!(
                        digest(&decoded[..expected.len()]),
                        digest(&expected),
                        "lossless decode {bits}/{run_index}/{index}"
                    );
                    assert!(decoded[expected.len()..].iter().all(|byte| *byte == 0));
                    verified += 1;
                }
                measurements.push(serde_json::json!({"bits":bits,"run":run_index,"workers":dvda_core::options::worker_count(tracks.len()),"repeat":repeat,"seconds":seconds,"output_bytes":output_bytes,"tracks":16}));
                println!(
                    "BENCH bits={bits} repeat={repeat} run={run_index} workers={} seconds={seconds:.6} verified=16",
                    dvda_core::options::worker_count(tracks.len())
                );
                fs::write(&report, serde_json::to_vec_pretty(&serde_json::json!({
                    "profile":"release x86_64-pc-windows-msvc", "logical_processors":std::thread::available_parallelism().unwrap().get(),
                    "media_library":media_library,"encoder_library":media_library.clone(),"media_sha256":digest(&fs::read(&media_library).unwrap()),
                    "encoder_sha256":digest(&fs::read(&media_library).unwrap()),"frames":"24013 + track_index * 7", "sample_rate":48000,
                    "channels":[2,6],"tracks_per_batch":16,"repeats":3,"verified_lossless_outputs":verified,
                    "verification":"Every output decoded to S24 and compared against independent fixture PCM; MLP SHA256 matches first automatic-policy baseline",
                    "measurements":measurements
                })).unwrap()).unwrap();
                fs::remove_dir_all(&run).unwrap();
            }
        }
    }
    println!("REPORT {} verified={verified}", report.display());
}
