//! Frozen pre-migration host references; this test does not execute C#.
use dvda_core::{encoder, hash, media};
use dvda_native::media::{Callbacks, Operation, OutputFormat, Request};
use serde::Deserialize;
use std::{
    fs,
    io::Cursor,
    os::windows::fs::OpenOptionsExt,
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
    encoder: String,
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
fn library() -> PathBuf {
    PathBuf::from(
        std::env::var_os("DVDA_ENCODER_LIBRARY")
            .expect("Set DVDA_ENCODER_LIBRARY to the pinned x64 C encoder"),
    )
}
fn job(wave: &Path, destination: &Path, context: String) -> encoder::Job {
    encoder::Job {
        library: library(),
        wave: wave.to_str().unwrap().into(),
        destination: destination.into(),
        metadata_context: context,
        timeout_millis: None,
    }
}
fn passed(outcome: media::Outcome) {
    assert!(outcome.failure.is_none(), "{:?}", outcome.failure);
    assert_eq!(outcome.exit_code, Some(0));
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
    let mut data = Vec::new();
    let mut expected = Vec::new();
    for n in 0..case.frames {
        for channel in 0..usize::from(case.channels) {
            let value = (((n * (97 + channel * 18) + channel * 53) % 100001) as i32 - 50000)
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
    if case.metadata_version == 0 {
        return String::new();
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(if case.metadata_version == 1 {
        b"MSCTX001"
    } else {
        b"MSCTX002"
    });
    bytes.extend_from_slice(
        &encoder::access_units(case.frames as i64, case.rate)
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
    passed(media::execute(
        media::Job {
            library: PathBuf::from(
                std::env::var_os("DVDA_MEDIA_NATIVE_DIR").expect("Set DVDA_MEDIA_NATIVE_DIR"),
            )
            .join("dvda-media.dll"),
            request: Request {
                operation: Operation::Audio,
                rate: 0,
                bits: 24,
                output_format: OutputFormat::S24,
                soxr: false,
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
#[ignore = "requires the pinned C encoder and source-built media DLL"]
fn frozen_mlp_matrix_and_lossless_decode() {
    let fixtures: Fixtures =
        serde_json::from_str(include_str!("fixtures/encoder-managed-v1.json")).unwrap();
    assert_eq!(fixtures.encoder, encoder::BINARY_SHA256);
    assert_eq!(fixtures.cases.len(), 116);
    let root = Scratch::new();
    let input = root.0.join("input.wav");
    let output = root.0.join("output.mlp");
    let raw = root.0.join("decoded.raw");
    let stamp = root.0.join("context.bin");
    for (index, case) in fixtures.cases.iter().enumerate() {
        let expected = wave(&input, case);
        assert_eq!(
            digest(&fs::read(&input).unwrap()),
            case.wave_sha256,
            "PCM generator case {index}: {case:?}"
        );
        let mut events = Events::default();
        passed(encoder::execute(
            job(&input, &output, context(&stamp, case)),
            &mut events,
        ));
        assert_eq!(events.completed, 1);
        let actual = fs::read(&output).unwrap();
        assert_eq!(actual.len(), case.bytes, "case {index}: {case:?}");
        assert_eq!(
            digest(&actual),
            case.mlp_sha256,
            "Entire MLP case {index}: {case:?}"
        );
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
            "Decoded length {index}"
        );
        assert_eq!(
            &decoded[..expected.len()],
            expected,
            "Lossless PCM case {index}: {case:?}"
        );
        assert!(
            decoded[expected.len()..].iter().all(|b| *b == 0),
            "Nonzero final AU padding {index}"
        );
        fs::remove_file(&output).unwrap();
        fs::remove_file(&raw).unwrap();
    }
}

#[test]
#[ignore = "requires the pinned C encoder"]
fn encoder_transactions_and_parallel_calls() {
    let fixtures: Fixtures =
        serde_json::from_str(include_str!("fixtures/encoder-managed-v1.json")).unwrap();
    let case = &fixtures.cases[17];
    let root = Scratch::new();
    let input = root.0.join("input.wav");
    let output = root.0.join("output.mlp");
    wave(&input, case);
    for mut events in [
        Events {
            cancel_after: Some(1),
            ..Events::default()
        },
        Events {
            cancel_after: Some(3),
            ..Events::default()
        },
        Events {
            cancel_on_complete: true,
            ..Events::default()
        },
    ] {
        assert_eq!(
            encoder::execute(job(&input, &output, String::new()), &mut events)
                .failure
                .unwrap()
                .kind,
            "Cancelled"
        );
        assert!(!output.exists());
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
    }
    let mut timed = job(&input, &output, String::new());
    timed.timeout_millis = Some(0);
    assert_eq!(
        encoder::execute(timed, &mut Events::default())
            .failure
            .unwrap()
            .kind,
        "Timeout"
    );
    fs::write(&output, b"preserve").unwrap();
    assert_eq!(
        encoder::execute(job(&input, &output, String::new()), &mut Events::default())
            .failure
            .unwrap()
            .kind,
        "Io"
    );
    assert_eq!(fs::read(&output).unwrap(), b"preserve");
    fs::remove_file(&output).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&input)
        .unwrap();
    assert_eq!(
        encoder::execute(job(&input, &output, String::new()), &mut Events::default())
            .failure
            .unwrap()
            .code,
        Some(32)
    );
    drop(lock);
    let bad = root.0.join("bad.dll");
    fs::write(&bad, b"not an encoder").unwrap();
    let mut corrupted = job(&input, &output, String::new());
    corrupted.library = bad.clone();
    assert_eq!(
        encoder::execute(corrupted, &mut Events::default())
            .failure
            .unwrap()
            .kind,
        "InvalidData"
    );
    fs::remove_file(&bad).unwrap();
    let threads: Vec<_> = (0..4)
        .map(|index| {
            let input = input.clone();
            let output = root.0.join(format!("parallel-{index}.mlp"));
            std::thread::spawn(move || {
                passed(encoder::execute(
                    job(&input, &output, String::new()),
                    &mut Events::default(),
                ));
                fs::read(output).unwrap()
            })
        })
        .collect();
    for thread in threads {
        assert_eq!(digest(&thread.join().unwrap()), case.mlp_sha256);
    }
    assert_eq!(
        fs::read_dir(&root.0).unwrap().count(),
        5,
        "Leaked partial output"
    );
}

fn batch_job(root: &Path, cases: &[Case], workers: i32) -> dvda_core::batch::Job {
    dvda_core::batch::Job {
        media_library: PathBuf::from(
            std::env::var_os("DVDA_MEDIA_NATIVE_DIR").expect("Set DVDA_MEDIA_NATIVE_DIR"),
        )
        .join("dvda-media.dll"),
        encoder_library: library(),
        temporary_directory: root.join("temp"),
        output_directory: root.join("output"),
        sample_rate: cases[0].rate as i32,
        bits: cases[0].bits as i32,
        jobs: workers,
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
                    duration_seconds: case.frames as f64 / f64::from(case.rate),
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
#[ignore = "requires the pinned C encoder and source-built media DLL"]
fn batch_matrix_matches_frozen_encoder_bytes() {
    let fixtures: Fixtures =
        serde_json::from_str(include_str!("fixtures/encoder-managed-v1.json")).unwrap();
    let root = Scratch::new();
    let mut outputs = 0;
    for rate in [44100, 48000, 88200, 96000, 176400, 192000] {
        for bits in [16, 20, 24] {
            let cases: Vec<_> = fixtures.cases[..84]
                .iter()
                .filter(|c| c.rate == rate && c.bits == bits)
                .cloned()
                .collect();
            for workers in [1, 4] {
                let folder = root.0.join(format!("{rate}-{bits}-{workers}"));
                fs::create_dir(&folder).unwrap();
                let job = batch_job(&folder, &cases, workers);
                let mut events = BatchEvents::new();
                passed(dvda_core::batch::execute(job, &mut events));
                assert_eq!(fs::read_dir(folder.join("temp")).unwrap().count(), 0);
                for (index, case) in cases.iter().enumerate() {
                    assert_eq!(
                        digest(&fs::read(folder.join(format!("output/track{index}.mlp"))).unwrap()),
                        case.mlp_sha256,
                        "Batch {rate}/{bits}/{workers}/{index}"
                    );
                    let kinds: Vec<_> = events
                        .events
                        .iter()
                        .filter(|e| e["Track"] == index)
                        .filter_map(|e| e["Kind"].as_str())
                        .filter(|k| *k != "PcmProgress")
                        .collect();
                    assert_eq!(kinds, ["Started", "PcmStarted", "PcmFinished", "Encoded"]);
                    outputs += 1;
                }
            }
        }
    }
    assert_eq!(outputs, 168);
}

#[test]
#[ignore = "requires the pinned C encoder and source-built media/formats DLLs"]
fn mlp_acquisition_upgrades_legacy_parameter_cache_without_reencoding_or_track_changes() {
    let fixtures: Fixtures =
        serde_json::from_str(include_str!("fixtures/encoder-managed-v1.json")).unwrap();
    let root = Scratch::new();
    let mut tracks = Vec::new();
    for (index, rate) in [44100, 48000].iter().enumerate() {
        let case = fixtures
            .cases
            .iter()
            .find(|case| case.rate == *rate && case.bits == 16 && case.channels == 2)
            .unwrap();
        let input = root.0.join(format!("input-{index}.wav"));
        wave(&input, case);
        tracks.push(serde_json::json!({"SourcePath":input,"Title":format!("曲目 {index}"),"Duration":case.frames as f64 / f64::from(case.rate)}));
    }
    let cache = root.0.join("cache.json");
    let output = root.0.join("output");
    let job = || dvda_core::mlp_workflow::Job {
        media_library: PathBuf::from(std::env::var_os("DVDA_MEDIA_NATIVE_DIR").unwrap())
            .join("dvda-media.dll"),
        encoder_library: library(),
        source_root: root.0.to_string_lossy().into_owned(),
        output_root: output.clone(),
        cache_path: cache.clone(),
        temporary_directory: root.0.join("temporary"),
        stage_directory: root.0.join("stage"),
        sample_rate: 48000,
        bits: 24,
        jobs: 0,
        metadata_context: String::new(),
        encoder_identity: "acquisition-regression".into(),
        tracks: tracks.clone(),
    };
    let first = dvda_core::mlp_workflow::execute(job(), &mut Events::default()).unwrap();
    assert_eq!((first.cache_hits, first.cache_rebuilt), (0, 2));
    assert!(first.diagnostics.is_empty());
    let bytes: Vec<_> = first
        .tracks
        .iter()
        .map(|track| fs::read(track["MlpPath"].as_str().unwrap()).unwrap())
        .collect();
    let mut evidence: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&fs::read(&cache).unwrap()).unwrap();
    for entry in evidence.values_mut() {
        assert_eq!(entry["parameters_version"], 1);
        for key in [
            "parameters_version",
            "source_parameters",
            "output_parameters",
        ] {
            entry.as_object_mut().unwrap().remove(key);
        }
    }
    fs::write(&cache, serde_json::to_vec(&evidence).unwrap()).unwrap();
    for _ in 0..2 {
        let reused = dvda_core::mlp_workflow::execute(job(), &mut Events::default()).unwrap();
        assert_eq!((reused.cache_hits, reused.cache_rebuilt), (2, 0));
        assert_eq!(reused.tracks, first.tracks);
        for (index, track) in reused.tracks.iter().enumerate() {
            assert_eq!(
                fs::read(track["MlpPath"].as_str().unwrap()).unwrap(),
                bytes[index]
            );
        }
        let evidence: serde_json::Value =
            serde_json::from_slice(&fs::read(&cache).unwrap()).unwrap();
        for entry in evidence.as_object().unwrap().values() {
            assert_eq!(entry["parameters_version"], 1);
        }
    }
}
#[test]
#[ignore = "requires the pinned C encoder and source-built media DLL"]
fn batch_cancellation_panic_and_partial_failure() {
    let fixtures: Fixtures =
        serde_json::from_str(include_str!("fixtures/encoder-managed-v1.json")).unwrap();
    let mut case = fixtures.cases[17].clone();
    case.frames = 192017;
    let root = Scratch::new();
    for scenario in ["cancel", "panic", "malformed", "existing", "duplicate"] {
        let folder = root.0.join(scenario);
        fs::create_dir(&folder).unwrap();
        let mut job = batch_job(&folder, &[case.clone(), case.clone(), case.clone()], 1);
        let mut events = BatchEvents::new();
        events.cancel_on_started = scenario == "cancel";
        events.panic_on_started = scenario == "panic";
        match scenario {
            "malformed" => {
                fs::write(&job.tracks[1].source_path, b"invalid wave").unwrap();
            }
            "existing" => {
                fs::create_dir_all(&job.output_directory).unwrap();
                fs::write(job.output_directory.join("track0.mlp"), b"preserve").unwrap();
            }
            "duplicate" => job.tracks[1].work_name = "TRACK0".into(),
            _ => (),
        }
        let result = dvda_core::batch::execute(job, &mut events);
        assert_eq!(
            result.failure.unwrap().kind,
            match scenario {
                "cancel" => "Cancelled",
                "duplicate" => "InvalidData",
                "existing" => "Io",
                _ => "InvalidOperation",
            }
        );
        if folder.join("temp").is_dir() {
            assert_eq!(fs::read_dir(folder.join("temp")).unwrap().count(), 0);
        }
        match scenario {
            "existing" => assert_eq!(
                fs::read(folder.join("output/track0.mlp")).unwrap(),
                b"preserve"
            ),
            "malformed" => {
                assert!(!folder.join("output/track1.mlp").exists());
                for index in [0, 2] {
                    let output = folder.join(format!("output/track{index}.mlp"));
                    if output.exists() {
                        assert!(!fs::read(output).unwrap().is_empty());
                    }
                }
            }
            "cancel" | "panic" => {
                assert_eq!(fs::read_dir(folder.join("output")).unwrap().count(), 0)
            }
            _ => assert!(!folder.join("output").exists()),
        }
    }
}

#[test]
#[ignore = "opt-in synthetic native batch worker benchmark"]
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
            let end =
                metadata.as_array()?.iter().try_fold(0usize, |end, entry| {
                    Some(end.max(
                        entry["offset"].as_u64()? as usize + entry["length"].as_u64()? as usize,
                    ))
                })?;
            executable.get(offset..offset + 12 + length + end)
        })
        .expect("embedded verified runtime archive");
    let runtime = dvda_core::runtime::extract(archive, &root.join("runtime")).unwrap();
    let media_library = runtime.join("dvda-media.dll");
    assert!(media_library.is_file());
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
                    duration_seconds: case.frames as f64 / f64::from(case.rate),
                }
            })
            .collect();
        let mut reference = Vec::new();
        for repeat in 0..3 {
            let workers: Vec<_> = if repeat == 1 {
                vec![16, 8, 4, 2, 1]
            } else {
                vec![1, 2, 4, 8, 16]
            };
            for workers in workers {
                let run = root.join(format!("run-{bits}-{repeat}-{workers}"));
                fs::create_dir(&run).unwrap();
                let job = dvda_core::batch::Job {
                    media_library: media_library.clone(),
                    encoder_library: library(),
                    temporary_directory: run.join("temp"),
                    output_directory: run.join("output"),
                    sample_rate: 48000,
                    bits: bits as i32,
                    jobs: workers,
                    metadata_context: String::new(),
                    tracks: tracks
                        .iter()
                        .map(|track| dvda_core::batch::Track {
                            source_path: track.source_path.clone(),
                            work_name: track.work_name.clone(),
                            display_name: track.display_name.clone(),
                            duration_seconds: track.duration_seconds,
                        })
                        .collect(),
                };
                let mut events = BatchEvents::new();
                let started = std::time::Instant::now();
                passed(dvda_core::batch::execute(job, &mut events));
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
                    if repeat == 0 && workers == 1 {
                        reference.push(checksum);
                    } else {
                        assert_eq!(
                            checksum, reference[index],
                            "non-deterministic worker output"
                        );
                    }
                    let expected = wave(&run.join(format!("expected-{index}.wav")), case);
                    let raw = run.join(format!("decoded-{index}.raw"));
                    passed(media::execute(
                        media::Job {
                            library: media_library.clone(),
                            request: Request {
                                operation: Operation::Audio,
                                rate: 0,
                                bits: 24,
                                output_format: OutputFormat::S24,
                                soxr: false,
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
                        "lossless decode {bits}/{workers}/{index}"
                    );
                    assert!(decoded[expected.len()..].iter().all(|byte| *byte == 0));
                    verified += 1;
                }
                measurements.push(serde_json::json!({"bits":bits,"requested_workers":workers,"workers":dvda_core::options::worker_count(tracks.len()),"repeat":repeat,"seconds":seconds,"output_bytes":output_bytes,"tracks":16}));
                println!(
                    "BENCH bits={bits} repeat={repeat} requested_workers={workers} workers={} seconds={seconds:.6} verified=16",
                    dvda_core::options::worker_count(tracks.len())
                );
                fs::write(&report, serde_json::to_vec_pretty(&serde_json::json!({
                    "profile":"release x86_64-pc-windows-gnu", "logical_processors":std::thread::available_parallelism().unwrap().get(),
                    "media_library":media_library,"encoder_library":library(),"media_sha256":digest(&fs::read(&media_library).unwrap()),
                    "encoder_sha256":digest(&fs::read(library()).unwrap()),"frames":"24013 + track_index * 7", "sample_rate":48000,
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
