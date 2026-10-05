//! Independent preparation regression; fixtures were frozen from the old host.
//! Only the source-built C media component is loaded. No managed/external host.
use dvda_core::{
    audio, media,
    preparation::{alac, pipeline, state},
};
use dvda_native::media::{Callbacks, Operation, OutputFormat, Request};
use serde_json::{Value, json};
use std::{
    fs,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "dvda-rust-prepare-中文-日本語-🎵-{}-{}-{}",
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
    lines: Vec<String>,
    cancel: bool,
    cancel_on: Option<&'static str>,
    panic_on: Option<&'static str>,
}
impl Callbacks for Events {
    fn emit(&mut self, _: i32, text: &str) {
        assert!(
            !self.panic_on.is_some_and(|key| text.contains(key)),
            "intentional event failure"
        );
        self.cancel |= self.cancel_on.is_some_and(|key| text.contains(key));
        self.lines.push(text.into());
    }
    fn cancelled(&mut self) -> bool {
        self.cancel
    }
}
fn library() -> PathBuf {
    PathBuf::from(std::env::var_os("DVDA_MEDIA_NATIVE_DIR").expect("Set DVDA_MEDIA_NATIVE_DIR"))
        .join("dvda-media.dll")
}
fn job(root: &Path) -> pipeline::Job {
    let build = root.join("build");
    pipeline::Job {
        language: "zh-CN".into(),
        library: library(),
        source_directory: root.join("src").to_str().unwrap().into(),
        manifest_path: build.join("manifest.json"),
        report_path: build.join("decode_report.txt"),
        prepare_cache_path: build.join("prepare-cache.json"),
        prepare_snapshot_path: build.join("prepare-snapshot.json"),
        alac_fix_directory: build.join("alacfix"),
        mlp_source: "surcode-batch".into(),
        prepare_cache_enabled: true,
        loss_error_seconds: 0.05,
        loss_warning_seconds: 0.005,
        force_revalidation: false,
    }
}
struct Track<'a> {
    name: &'a str,
    rate: u32,
    bits: u16,
    channels: u16,
    album: &'a str,
    track: &'a str,
    title: &'a str,
    frames: usize,
}
fn audio(root: &Path, track: Track<'_>) {
    let Track {
        name,
        rate,
        bits,
        channels,
        album,
        track,
        title,
        frames,
    } = track;
    fs::create_dir_all(root.join("src")).unwrap();
    let wave = root.join("input.wav");
    let output = root.join("src").join(format!("{name}.flac"));
    let width = if bits == 16 { 2usize } else { 3 };
    let size = frames * usize::from(channels) * width;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(60 + size as u32 + size as u32 % 2).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&0xfffeu16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(channels) * width as u32).to_le_bytes());
    bytes.extend_from_slice(&(channels * width as u16).to_le_bytes());
    bytes.extend_from_slice(&(width as u16 * 8).to_le_bytes());
    bytes.extend_from_slice(&22u16.to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes
        .extend_from_slice(&[0u32, 4, 3, 7, 0x33, 0x37, 0x3f][usize::from(channels)].to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]);
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(size as u32).to_le_bytes());
    for n in 0..frames {
        for c in 0..usize::from(channels) {
            let value = (((n * (97 + c * 18) + c * 53) % 100001) as i32 - 50000)
                & !((1 << (24 - bits)) - 1);
            let stored = if bits == 16 { value >> 8 } else { value };
            bytes.extend_from_slice(&stored.to_le_bytes()[..width]);
        }
    }
    if !size.is_multiple_of(2) {
        bytes.push(0);
    }
    fs::write(&wave, bytes).unwrap();
    let result = media::execute(
        media::Job {
            library: library(),
            replace: false,
            timeout_millis: None,
            request: Request {
                operation: Operation::Audio,
                rate: 0,
                bits: 0,
                output_format: OutputFormat::Flac,
                soxr: false,
                compression: 8,
                cover: false,
                input: wave.to_str().unwrap().into(),
                output: Some(output.to_str().unwrap().into()),
                tags: vec![
                    ("ALBUM".into(), album.into()),
                    ("TRACK".into(), track.into()),
                    ("TITLE".into(), title.into()),
                    ("DATE".into(), "2026-01-02".into()),
                ],
            },
        },
        &mut Events::default(),
    );
    assert!(result.failure.is_none(), "{:?}", result.failure);
    assert_eq!(result.exit_code, Some(0));
}
fn sources(root: &Path) {
    for track in [
        Track {
            name: "a",
            rate: 48000,
            bits: 24,
            channels: 2,
            album: "Album A",
            track: "12",
            title: "曲目 😀",
            frames: 817,
        },
        Track {
            name: "b",
            rate: 48000,
            bits: 24,
            channels: 2,
            album: "Album A",
            track: "2",
            title: "曲目 日本語",
            frames: 817,
        },
        Track {
            name: "c",
            rate: 44100,
            bits: 16,
            channels: 2,
            album: "Album A",
            track: "1",
            title: "曲目 中文",
            frames: 817,
        },
        Track {
            name: "d",
            rate: 96000,
            bits: 24,
            channels: 6,
            album: "Album B",
            track: "3/12",
            title: "Surround",
            frames: 9617,
        },
        Track {
            name: "e",
            rate: 192000,
            bits: 24,
            channels: 2,
            album: "Album C",
            track: "1",
            title: "High rate",
            frames: 817,
        },
    ] {
        audio(root, track);
    }
}
fn canonical(value: &mut Value, root: &Path) {
    match value {
        Value::Object(object) => {
            for item in object.values_mut() {
                canonical(item, root);
            }
        }
        Value::Array(array) => {
            for item in array {
                canonical(item, root);
            }
        }
        Value::String(text) => *text = text.replace(root.to_str().unwrap(), "<ROOT>"),
        _ => {}
    }
}
fn run(job: &pipeline::Job, root: &Path, events: &mut Events) -> Value {
    let result = pipeline::run(job, events).unwrap();
    let manifest = job
        .manifest_path
        .is_file()
        .then(|| state::read_json(&job.manifest_path).unwrap());
    let mut value = json!({"Result":result,"Manifest":manifest,"Report":fs::read_to_string(&job.report_path).unwrap()});
    canonical(&mut value, root);
    value
}
fn snapshot_reusable(job: &pipeline::Job) -> bool {
    state::reuse_snapshot(
        &job.prepare_snapshot_path,
        &job.source_directory,
        &job.fingerprint().unwrap(),
        job.manifest_path.to_str().unwrap(),
    )
    .unwrap()
    .0
    .is_some()
}
fn assert_no_temporary(path: &Path) {
    if !path.is_dir() {
        return;
    }
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        assert!(
            !path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".partial"),
            "Leaked {}",
            path.display()
        );
        if path.is_dir() {
            assert_no_temporary(&path);
        }
    }
}

#[test]
#[ignore = "requires source-built Windows x64 media component"]
fn frozen_preparation_manifest_report_cache_and_alac() {
    let mut expected: Value =
        serde_json::from_str(include_str!("fixtures/preparation-managed-v1.json")).unwrap();
    // Deserialize schema-defined doubles before comparison: the managed JSON
    // writer spells a zero double as 0; serde spells the same value as 0.0.
    for scenario in expected.as_object_mut().unwrap().values_mut() {
        let result: dvda_core::preparation::models::Result =
            serde_json::from_value(scenario["Result"].clone()).unwrap();
        scenario["Result"] = json!(result);
    }
    let root = Scratch::new();
    sources(&root.0);
    let mut job = job(&root.0);
    assert_eq!(run(&job, &root.0, &mut Events::default()), expected["Cold"]);
    assert!(snapshot_reusable(&job));
    let cold_cache = state::read_json(&job.prepare_cache_path).unwrap();
    let mut warm = Events::default();
    assert_eq!(run(&job, &root.0, &mut warm), expected["Cold"]);
    assert!(
        warm.lines
            .iter()
            .any(|line| line.contains("复用已校验结果 5 首 / 重新校验 0 首"))
    );
    assert_eq!(
        state::read_json(&job.prepare_cache_path).unwrap(),
        cold_cache
    );
    job.force_revalidation = true;
    assert_eq!(run(&job, &root.0, &mut Events::default()), expected["Cold"]);
    job.force_revalidation = false;
    fs::write(&job.prepare_cache_path, b"{ broken").unwrap();
    assert_eq!(run(&job, &root.0, &mut Events::default()), expected["Cold"]);
    audio(
        &root.0,
        Track {
            name: "mixed",
            rate: 48000,
            bits: 24,
            channels: 1,
            album: "Album X",
            track: "1",
            title: "Mono",
            frames: 817,
        },
    );
    assert!(!snapshot_reusable(&job));
    assert_eq!(
        run(&job, &root.0, &mut Events::default()),
        expected["MixedChannels"]
    );
    assert!(!job.manifest_path.exists());
    assert!(!job.prepare_snapshot_path.exists());
    job.mlp_source = "lpcm".into();
    assert_eq!(run(&job, &root.0, &mut Events::default()), expected["Lpcm"]);
    fs::remove_file(root.0.join("src/mixed.flac")).unwrap();
    job.mlp_source = "surcode-batch".into();
    fs::write(
        root.0.join("src/broken.m4a"),
        include_bytes!("fixtures/alac-broken.m4a"),
    )
    .unwrap();
    assert_eq!(run(&job, &root.0, &mut Events::default()), expected["Alac"]);
    assert_eq!(
        fs::read(job.alac_fix_directory.join("broken.m4a")).unwrap(),
        include_bytes!("fixtures/alac-valid.m4a")
    );
    assert_eq!(run(&job, &root.0, &mut Events::default()), expected["Alac"]);
    fs::remove_file(job.alac_fix_directory.join("broken.m4a")).unwrap();
    pipeline::ensure_ready(&job, &mut Events::default()).unwrap();
    assert!(job.alac_fix_directory.join("broken.m4a").is_file());
    assert_eq!(run(&job, &root.0, &mut Events::default()), expected["Alac"]);
    assert_eq!(
        fs::read(root.0.join("src/broken.m4a")).unwrap(),
        include_bytes!("fixtures/alac-broken.m4a")
    );
    // Cached identities must reject content changes even when size/time are unchanged.
    let source = root.0.join("src/a.flac");
    let time = fs::metadata(&source).unwrap().modified().unwrap();
    let mut bytes = fs::read(&source).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&source, bytes).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&source)
        .unwrap()
        .set_modified(time)
        .unwrap();
    assert!(!snapshot_reusable(&job));
    assert!(
        state::Cache::load(&job.prepare_cache_path, &mut Events::default())
            .find(source.to_str().unwrap())
            .is_none()
    );
    assert_no_temporary(&root.0);
}

#[test]
#[ignore = "requires source-built Windows x64 media component"]
fn alac_repair_preserves_source_and_matches_valid_container() {
    let root = Scratch::new();
    let input = root.0.join("broken.m4a");
    let broken = include_bytes!("fixtures/alac-broken.m4a");
    let valid = include_bytes!("fixtures/alac-valid.m4a");
    fs::write(&input, broken).unwrap();
    let directory = root.0.join("repair");
    let path = input.to_str().unwrap();
    let inspection = alac::inspect(&library(), path, &mut Events::default()).unwrap();
    assert_eq!(inspection.patches.len(), 2);
    assert!(
        inspection
            .patches
            .iter()
            .all(|p| p.previous_bits == 0 && p.sample_count == 512)
    );
    assert!(alac::try_repair(&library(), path, &root.0, &mut Events::default()).is_err());
    let failure = alac::try_repair(
        &library(),
        path,
        &directory,
        &mut Events {
            cancel: true,
            ..Events::default()
        },
    )
    .unwrap_err();
    assert_eq!(failure.kind, "Cancelled");
    assert!(!directory.exists());
    let repair = alac::try_repair(&library(), path, &directory, &mut Events::default())
        .unwrap()
        .unwrap();
    assert_eq!(repair.patches, inspection.patches);
    assert_eq!(fs::read(&repair.output_path).unwrap(), valid);
    assert_eq!(fs::read(&input).unwrap(), broken);
    let decoded = audio::check_decode(
        &audio::Job {
            library: library(),
            input: repair.output_path.clone(),
            operation: audio::Operation::Decode,
            resample_to: None,
        },
        &mut Events::default(),
    )
    .unwrap();
    assert_eq!(decoded.samples, Some(1024));
    assert_eq!(decoded.error_count, 0);
    assert!(
        alac::try_repair(
            &library(),
            &repair.output_path,
            &root.0.join("unused"),
            &mut Events::default()
        )
        .unwrap()
        .is_none()
    );
    assert!(!root.0.join("unused").exists());
    // A locked destination cannot be replaced or left partially written.
    fs::write(&repair.output_path, b"preserve").unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&repair.output_path)
        .unwrap();
    assert!(alac::try_repair(&library(), path, &directory, &mut Events::default()).is_err());
    drop(lock);
    assert_eq!(fs::read(&repair.output_path).unwrap(), b"preserve");
    assert_eq!(fs::read(input).unwrap(), broken);
    assert_no_temporary(&root.0);
}

#[test]
#[ignore = "requires source-built Windows x64 media component"]
fn preparation_cancel_panic_and_output_failures() {
    for stage in ["发现", "共需重采样", "group_48000_24"] {
        let root = Scratch::new();
        sources(&root.0);
        let job = job(&root.0);
        run(&job, &root.0, &mut Events::default());
        let failure = pipeline::run(
            &job,
            &mut Events {
                cancel_on: Some(stage),
                ..Events::default()
            },
        )
        .unwrap_err();
        assert_eq!(failure.kind, "Cancelled", "{stage}");
        assert!(!job.manifest_path.exists());
        assert!(!job.prepare_snapshot_path.exists());
        assert_no_temporary(&root.0);
    }
    let root = Scratch::new();
    sources(&root.0);
    let job = job(&root.0);
    run(&job, &root.0, &mut Events::default());
    let previous = fs::read(&job.manifest_path).unwrap();
    assert!(
        pipeline::run(
            &job,
            &mut Events {
                cancel: true,
                ..Events::default()
            }
        )
        .is_err()
    );
    assert_eq!(fs::read(&job.manifest_path).unwrap(), previous);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pipeline::run(
            &job,
            &mut Events {
                panic_on: Some("发现"),
                ..Events::default()
            },
        )
    }));
    assert!(panicked.is_err());
    assert!(!job.manifest_path.exists());
    assert!(!job.prepare_snapshot_path.exists());
    assert_no_temporary(&root.0);
    fs::create_dir(&job.manifest_path).unwrap();
    assert!(pipeline::run(&job, &mut Events::default()).is_err());
    assert!(job.manifest_path.is_dir());
    assert!(!job.prepare_snapshot_path.exists());
    assert_no_temporary(&root.0);
}
