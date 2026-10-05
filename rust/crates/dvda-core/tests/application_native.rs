//! Exercise the same entry points as the desktop, with generated audio only.
use dvda_core::{app, media, verify_audio};
use dvda_native::media::{Callbacks, Operation, OutputFormat, Request};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "dvda-app-中文-日本語-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
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
    cancel_on_staged: bool,
    cancelled: bool,
}
impl Callbacks for Events {
    fn emit(&mut self, _: i32, text: &str) {
        if self.cancel_on_staged && text.starts_with("[build] staged disc") {
            self.cancelled = true;
        }
        self.lines.push(text.into());
    }
    fn cancelled(&mut self) -> bool {
        self.cancelled
    }
}
fn library() -> PathBuf {
    PathBuf::from(std::env::var_os("DVDA_MEDIA_NATIVE_DIR").expect("Set DVDA_MEDIA_NATIVE_DIR"))
        .join("dvda-media.dll")
}
fn sources(root: &Path) {
    sources_aligned(root, false);
}
fn sources_aligned(root: &Path, align: bool) {
    fs::create_dir_all(root.join("src")).unwrap();
    for n in 1..=2u32 {
        let frames = 24003 + n * 23;
        let frames = if align {
            frames.div_ceil(40) * 40
        } else {
            frames
        };
        let size = frames * 4;
        let mut wave = Vec::new();
        wave.extend_from_slice(b"RIFF");
        wave.extend_from_slice(&(size + 36).to_le_bytes());
        wave.extend_from_slice(b"WAVEfmt ");
        wave.extend_from_slice(&16u32.to_le_bytes());
        wave.extend_from_slice(&1u16.to_le_bytes());
        wave.extend_from_slice(&2u16.to_le_bytes());
        wave.extend_from_slice(&48000u32.to_le_bytes());
        wave.extend_from_slice(&192000u32.to_le_bytes());
        wave.extend_from_slice(&4u16.to_le_bytes());
        wave.extend_from_slice(&16u16.to_le_bytes());
        wave.extend_from_slice(b"data");
        wave.extend_from_slice(&size.to_le_bytes());
        for frame in 0..frames {
            for c in 0..2 {
                let sample = ((frame * (97 + c * 18) + n * 53) % 60001) as i32 - 30000;
                wave.extend_from_slice(&(sample as i16).to_le_bytes());
            }
        }
        let input = root.join("input.wav");
        fs::write(&input, wave).unwrap();
        let result = media::execute(
            media::Job {
                library: library(),
                replace: false,
                timeout_millis: None,
                request: Request {
                    operation: Operation::Audio,
                    input: input.to_string_lossy().into_owned(),
                    output: Some(
                        root.join("src")
                            .join(format!("{n}.flac"))
                            .to_string_lossy()
                            .into_owned(),
                    ),
                    rate: 0,
                    bits: 0,
                    output_format: OutputFormat::Flac,
                    soxr: false,
                    compression: 8,
                    cover: false,
                    tags: vec![
                        ("ALBUM".into(), "Test album".into()),
                        ("TRACK".into(), n.to_string()),
                        ("TITLE".into(), format!("Track {n}")),
                        ("DATE".into(), "2026".into()),
                    ],
                },
            },
            &mut Events::default(),
        );
        assert_eq!(result.exit_code, Some(0), "{:?}", result.failure);
    }
}
fn options(root: &Path, mode: &str) -> app::AppOptions {
    let author = std::env::var_os("DVDA_TEST_AUTHOR").expect("Set DVDA_TEST_AUTHOR");
    let profile = root.join("settings.json");
    app::save_profile(&profile, &serde_json::Map::new(), "zh").unwrap();
    app::AppOptions::load(Some(&profile))
        .unwrap()
        .with_profile_values(
            json!({
                "DVDA_SRC":root.join("src"), "DVDA_BUILD_DIR":root.join("build"),
                "DVDA_FINAL_DIR":root.join("final"), "DVDA_MLP_EXTERNAL_DIR":"",
                "DVDA_MLP_SOURCE":mode, "DVDA_AUTHOR":PathBuf::from(author),
                "DVDA_MENU":false, "DVDA_RESUME":true,
                "DVDA_MLP_SURCODE_SAMPLE_RATE":48000, "DVDA_MLP_SURCODE_BITS":24,
                "DVDA_TITLE":"Application regression", "DVDA_ISO_PREFIX":"regression",
                "DVDA_KEEP_TMP":false, "DVDA_KEEP_INTERMEDIATE":false
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .unwrap()
}

#[test]
#[ignore = "requires source-built media/formats/author"]
fn disabled_resume_and_intermediate_mode_ignore_locked_old_record() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = Scratch::new();
    sources(&root.0);
    let options = options(&root.0, "lpcm");
    let staging = options.path("BuildDirectory").join("publish-staging");
    fs::create_dir_all(&staging).unwrap();
    let record = staging.join("resume.json");
    fs::write(&record, b"{ deliberately unreadable old record").unwrap();
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&record)
        .unwrap();
    for values in [
        json!({"DVDA_RESUME":false}),
        json!({"DVDA_RESUME":true,"DVDA_KEEP_INTERMEDIATE":true}),
    ] {
        let options = options
            .with_profile_values(values.as_object().unwrap().clone())
            .unwrap();
        let mut events = Events::default();
        let result = app::run_build(&options, false, &mut events).unwrap();
        assert!(result.succeeded, "{:?}", result.diagnostics);
        assert!(
            !events
                .lines
                .iter()
                .any(|line| line.contains("续跑记录无法读取"))
        );
        assert_eq!(result.published.len(), 1);
        assert!(
            !options
                .path("MlpIndexPath")
                .with_file_name("mlp_index.json.pending")
                .exists()
        );
        let verified = app::run_verify(&options, &mut Events::default()).unwrap();
        assert!(verified.succeeded, "{:?}", verified.diagnostics);
    }
    drop(locked);
    assert_eq!(
        fs::read(&record).unwrap(),
        b"{ deliberately unreadable old record"
    );
}

struct BuildLocks {
    record: Option<PathBuf>,
    cleanup: Vec<PathBuf>,
    handles: Vec<fs::File>,
}
impl Callbacks for BuildLocks {
    fn emit(&mut self, _: i32, text: &str) {
        use std::os::windows::fs::OpenOptionsExt;
        if text.starts_with("[author] disc")
            && let Some(path) = self.record.take()
        {
            // The record was loaded before authoring; deny replacement only
            // once the build is committed to generating this disc.
            self.handles.push(
                fs::OpenOptions::new()
                    .read(true)
                    .share_mode(1)
                    .open(path)
                    .unwrap(),
            );
        }
        if text.starts_with("[build] staged disc") {
            for path in self.cleanup.drain(..) {
                fs::write(&path, b"locked test artifact").unwrap();
                self.handles.push(
                    fs::OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .open(path)
                        .unwrap(),
                );
            }
        }
    }
    fn cancelled(&mut self) -> bool {
        false
    }
}

#[test]
#[ignore = "requires source-built media/formats/author"]
fn resume_write_failure_warns_and_publishes_verified_iso() {
    let root = Scratch::new();
    sources(&root.0);
    let options = options(&root.0, "lpcm");
    let staging = options.path("BuildDirectory").join("publish-staging");
    fs::create_dir_all(&staging).unwrap();
    let record = staging.join("resume.json");
    fs::write(&record, b"{}").unwrap();
    let mut events = BuildLocks {
        record: Some(record.clone()),
        cleanup: vec![],
        handles: vec![],
    };
    let result = app::run_build(&options, false, &mut events).unwrap();
    assert_eq!(events.handles.len(), 1);
    assert!(result.succeeded, "{:?}", result.diagnostics);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d["Code"] == "RESUME_WRITE_FAILED" && d["Severity"] == 1)
    );
    assert_eq!(result.published.len(), 1);
    assert!(
        !options
            .path("MlpIndexPath")
            .with_file_name("mlp_index.json.pending")
            .exists()
    );
    assert_eq!(fs::read(&record).unwrap(), b"{}");
    assert!(
        fs::read_to_string(options.path("BuildLogPath"))
            .unwrap()
            .contains("[警告] 续跑记录写入失败:")
    );
    drop(events);
    let verified = app::run_verify(&options, &mut Events::default()).unwrap();
    assert!(verified.succeeded, "{:?}", verified.diagnostics);
}

#[test]
#[ignore = "requires source-built media/formats/author"]
fn cleanup_failure_warns_and_next_build_rejects_dirty_workspace() {
    let root = Scratch::new();
    sources(&root.0);
    let options = options(&root.0, "lpcm")
        .with_profile_values(json!({"DVDA_RESUME":false}).as_object().unwrap().clone())
        .unwrap();
    let temporary = options.path("TemporaryRoot").join("disc1/locked.txt");
    let output = options.path("OutputRoot").join("disc1/locked.txt");
    let mut events = BuildLocks {
        record: None,
        cleanup: vec![temporary.clone(), output.clone()],
        handles: vec![],
    };
    let result = app::run_build(&options, false, &mut events).unwrap();
    assert_eq!(events.handles.len(), 2);
    assert!(result.succeeded, "{:?}", result.diagnostics);
    for code in ["TEMP_CLEANUP_FAILED", "OUTPUT_CLEANUP_FAILED"] {
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d["Code"] == code && d["Severity"] == 1)
        );
    }
    assert!(temporary.exists() && output.exists());
    let published = fs::read(&result.published[0]).unwrap();
    let index = fs::read(options.path("MlpIndexPath")).unwrap();
    let mut retried = Events::default();
    let failed = app::run_build(&options, false, &mut retried).unwrap();
    assert!(!failed.succeeded);
    assert!(
        failed
            .diagnostics
            .iter()
            .any(|d| d["Code"] == "BUILD_FAILED")
    );
    assert!(
        !retried
            .lines
            .iter()
            .any(|line| line.starts_with("[author] disc"))
    );
    assert_eq!(fs::read(&result.published[0]).unwrap(), published);
    assert_eq!(fs::read(options.path("MlpIndexPath")).unwrap(), index);
    assert!(
        !options
            .path("MlpIndexPath")
            .with_file_name("mlp_index.json.pending")
            .exists()
    );
    drop(events);
    let recovered = app::run_build(&options, false, &mut Events::default()).unwrap();
    assert!(recovered.succeeded, "{:?}", recovered.diagnostics);
    assert!(!temporary.exists() && !output.exists());
}

#[test]
#[ignore = "requires source-built x64 media/formats/encoder/author/disc verifier"]
fn application_preparation_cache_resume_and_lossless_verification() {
    for mode in ["surcode-batch", "lpcm"] {
        let root = Scratch::new();
        sources(&root.0);
        let options = options(&root.0, mode);
        assert!(!options.boolean("MenuEnabled"));
        assert!(options.boolean("ResumeEnabled"));
        let first = app::run_build(&options, true, &mut Events::default()).unwrap();
        assert!(first.succeeded, "{:?}", first.diagnostics);
        assert_eq!(first.cache_rebuilt, 2);
        assert!(options.path("PrepareSnapshotPath").is_file());
        assert!(!options.path("MlpIndexPath").exists());
        let mut events = Events::default();
        let second = app::run_build(&options, true, &mut events).unwrap();
        assert!(second.succeeded, "{:?}", second.diagnostics);
        assert_eq!(second.cache_hits, 2);
        assert_eq!(second.cache_rebuilt, 0);
        assert!(
            events
                .lines
                .iter()
                .any(|s| s.contains("复用") || s.contains("重用")),
            "{:?}",
            events.lines
        );

        let mut cancelled = Events {
            cancel_on_staged: true,
            ..Default::default()
        };
        let interrupted = app::run_build(&options, false, &mut cancelled).unwrap();
        assert!(cancelled.cancelled, "{:?}", interrupted.diagnostics);
        assert!(!interrupted.succeeded);
        assert!(!options.path("MlpIndexPath").exists());
        let mut resumed = Events::default();
        let built = app::run_build(&options, false, &mut resumed).unwrap();
        assert!(built.succeeded, "{:?}", built.diagnostics);
        assert_eq!(built.published.len(), 1);
        assert!(
            resumed.lines.iter().any(|s| s.starts_with("[续跑]")),
            "{:?}",
            resumed.lines
        );
        assert!(!resumed.lines.iter().any(|s| s.starts_with("[author] disc")));
        let verified = app::run_verify(&options, &mut Events::default()).unwrap();
        assert!(verified.succeeded, "{mode}: {:?}", verified.diagnostics);
        assert_eq!(verified.track_count, 2);
        assert_eq!(verified.iso_count, 1);

        let index: Value =
            serde_json::from_slice(&fs::read(options.path("MlpIndexPath")).unwrap()).unwrap();
        let nested = &index["__discs__"][0]["groups"][0]["tracks"][0];
        assert!(
            nested["mlp_source"].is_null(),
            "Regression requires sparse nested index"
        );
        let mut details = index[nested["mlp"].as_str().unwrap()].clone();
        details["mlp"] = nested["mlp"].clone();
        // A different, valid audio track must fail full PCM verification.
        let original = details["src"].as_str().unwrap();
        let other = if original.ends_with("1.flac") {
            "2.flac"
        } else {
            "1.flac"
        };
        details["src"] = json!(root.0.join("src").join(other));
        assert!(
            verify_audio::track(
                &library(),
                &root.0.join("check"),
                &details,
                &mut Events::default()
            )
            .is_err()
        );
        if mode == "surcode-batch" {
            let imported = root.0.join("import");
            fs::create_dir(&imported).unwrap();
            for group in index["__discs__"][0]["groups"].as_array().unwrap() {
                for track in group["tracks"].as_array().unwrap() {
                    let name = Path::new(track["src"].as_str().unwrap())
                        .file_stem()
                        .unwrap();
                    fs::copy(
                        track["mlp"].as_str().unwrap(),
                        imported.join(name).with_extension("mlp"),
                    )
                    .unwrap();
                }
            }
            let imported_options = options
                .with_profile_values(
                    json!({"DVDA_MLP_SOURCE":"external",
                "DVDA_MLP_EXTERNAL_DIR":imported,"DVDA_BUILD_DIR":root.0.join("import-work"),
                "DVDA_FINAL_DIR":root.0.join("import-final")})
                    .as_object()
                    .unwrap()
                    .clone(),
                )
                .unwrap();
            let imported_result =
                app::run_build(&imported_options, false, &mut Events::default()).unwrap();
            assert!(
                imported_result.succeeded,
                "{:?}",
                imported_result.diagnostics
            );
            let imported_verify =
                app::run_verify(&imported_options, &mut Events::default()).unwrap();
            assert!(!imported_verify.succeeded);
            assert!(
                imported_verify
                    .diagnostics
                    .iter()
                    .all(|d| d["Code"] == "TRACK_PCM_MISMATCH")
            );
        }
        fs::write(root.0.join("src/1.flac"), b"broken source").unwrap();
        let before = fs::read(options.path("MlpIndexPath")).unwrap();
        assert!(app::run_build(&options, false, &mut Events::default()).is_err());
        assert_eq!(fs::read(options.path("MlpIndexPath")).unwrap(), before);
    }
}

#[test]
#[ignore = "requires source-built author and image/media DLLs plus menu data/fonts"]
fn application_menu_and_title_boundaries() {
    let root = Scratch::new();
    sources(&root.0);
    let image_library =
        PathBuf::from(std::env::var_os("DVDA_IMAGE_NATIVE_DIR").unwrap()).join("dvda-image.dll");
    let images = dvda_native::images::Images::load(&image_library).unwrap();
    assert_eq!(
        images
            .run(
                &[
                    "convert".into(),
                    "-size".into(),
                    "320x320".into(),
                    "gradient:#102040-#c0e0ff".into(),
                    root.0.join("src/cover.png").to_string_lossy().into_owned(),
                ],
                &mut Events::default()
            )
            .unwrap(),
        0
    );
    let data =
        PathBuf::from(std::env::var_os("DVDA_TEST_MENU_DATA").expect("Set DVDA_TEST_MENU_DATA"));
    let options = options(&root.0, "lpcm")
        .with_profile_values(
            json!({
            "DVDA_MENU":true, "DVDA_MENU_STILLPICS":true,"DVDA_MENU_INDEX_MIN_ALBUMS":1,
                "DVDA_AUTHOR_SRC":data,"DVDA_TITLE_MODE":"1","DVDA_MENU_TRACKS_PER_PAGE":1,
                "DVDA_KEEP_TMP":true,"DVDA_KEEP_INTERMEDIATE":true,
                "DVDA_MENU_FONT":"nonexistent preferred family", "DVDA_MENU_FONT_JP":"nonexistent regional family"
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .unwrap();
    let mut events = Events::default();
    assert!(options.boolean("MenuEnabled"));
    assert!(options.boolean("KeepTemporary"));
    let built = app::run_build(&options, false, &mut events).unwrap();
    assert!(
        built.succeeded,
        "{:?}\n{}",
        built.diagnostics,
        events.lines.join("\n")
    );
    assert!(
        built
            .diagnostics
            .iter()
            .any(|d| d["Code"] == "MENU_FONT_UNAVAILABLE" && d["Severity"] == 1),
        "font={} diagnostics={:?}",
        options.text("MenuFont"),
        built.diagnostics
    );
    assert!(
        built
            .diagnostics
            .iter()
            .any(|d| d["Code"] == "MENU_REGIONAL_FONT_UNAVAILABLE" && d["Severity"] == 1)
    );
    assert!(
        !fs::read_dir(options.path("MenuDirectory").join("disc1"))
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("font-probe"))
    );
    // Changing artwork must invalidate a complete staged disc even with the same audio.
    let mut interrupted = Events {
        cancel_on_staged: true,
        ..Default::default()
    };
    let no_resume = options
        .with_profile_values(json!({"DVDA_RESUME":false}).as_object().unwrap().clone())
        .unwrap();
    assert!(
        !app::run_build(&no_resume, false, &mut interrupted)
            .unwrap()
            .succeeded
    );
    assert_eq!(
        images
            .run(
                &[
                    "convert".into(),
                    "-size".into(),
                    "320x320".into(),
                    "gradient:#602040-#80e0ff".into(),
                    root.0.join("src/cover.png").to_string_lossy().into_owned()
                ],
                &mut Events::default()
            )
            .unwrap(),
        0
    );
    let mut restarted = Events::default();
    assert!(
        app::run_build(&options, false, &mut restarted)
            .unwrap()
            .succeeded
    );
    assert!(
        restarted
            .lines
            .iter()
            .any(|s| s.starts_with("[author] disc"))
    );
    assert!(!restarted.lines.iter().any(|s| s.starts_with("[续跑]")));
    let verified = app::run_verify(&options, &mut Events::default()).unwrap();
    assert!(verified.succeeded, "{:?}", verified.diagnostics);
    let iso = options.path("FinalDirectory").join("regression_1.iso");
    let entries = dvda_core::formats::iso_list_directory(&iso, "AUDIO_TS").unwrap();
    let original_iso = fs::read(&iso).unwrap();
    let still_ifo_record = iso_record_offset(&iso, &original_iso, "AUDIO_SV.IFO");
    let still_vob_record = iso_record_offset(&iso, &original_iso, "AUDIO_SV.VOB");
    let still_ifo = entries
        .iter()
        .find(|entry| entry.name == "AUDIO_SV.IFO")
        .unwrap()
        .logical_block_address as usize
        * 2048;
    let still_bytes = entries
        .iter()
        .find(|entry| entry.name == "AUDIO_SV.VOB")
        .unwrap()
        .size;
    for length in [24u32, 95] {
        let mut bad = original_iso.clone();
        bad[still_ifo..still_ifo + 96].fill(0);
        bad[still_ifo + 20..still_ifo + 24]
            .copy_from_slice(&(still_bytes.div_ceil(2048) - 1).to_be_bytes());
        set_iso_u32(&mut bad, still_ifo_record + 10, length);
        fs::write(&iso, &bad).unwrap();
        assert_diagnostic(run_verify(&options), "ASVS_TOO_SHORT");
    }
    fs::write(&iso, &original_iso).unwrap();
    let index_path = options.path("MlpIndexPath");
    let original_index = fs::read(&index_path).unwrap();
    let original_value: Value = serde_json::from_slice(&original_index).unwrap();
    let menu_only = || {
        dvda_core::verify::execute_mode(
            options.verify_job().unwrap(),
            dvda_core::verify::Mode::Menu,
            Some(iso.clone()),
            &mut Events::default(),
        )
    };
    for missing in [[true, false], [false, true], [true, true]] {
        let mut bad = original_iso.clone();
        if missing[0] {
            bad[still_ifo_record + 33] = b'X';
        }
        if missing[1] {
            bad[still_vob_record + 33] = b'X';
        }
        fs::write(&iso, &bad).unwrap();
        // Both explicit positive expectations and no expectations require files.
        assert_diagnostic(menu_only(), "MENU_FILE_MISSING");
        let mut no_expectation = original_value.clone();
        no_expectation["__discs__"][0]
            .as_object_mut()
            .unwrap()
            .remove("menu");
        fs::write(&index_path, serde_json::to_vec(&no_expectation).unwrap()).unwrap();
        assert_diagnostic(menu_only(), "MENU_FILE_MISSING");
        fs::write(&index_path, &original_index).unwrap();
    }
    let mut zero = original_value.clone();
    zero["__discs__"][0]["menu"]["stills"] = json!(0);
    fs::write(&index_path, serde_json::to_vec(&zero).unwrap()).unwrap();
    let zero_result = menu_only();
    assert!(zero_result.succeeded, "{:?}", zero_result.diagnostics);
    fs::write(&index_path, &original_index).unwrap();
    fs::write(&iso, &original_iso).unwrap();
    // The pre-publication verifier keeps missing playback covers at warning
    // severity, while exercising the same menu XML and image checks as build.
    let output = options.path("OutputRoot").join("disc1");
    let temporary = options.path("TemporaryRoot").join("disc1");
    let authored_still = output.join("AUDIO_TS/AUDIO_SV.VOB");
    let still_contents = fs::read(&authored_still).unwrap();
    let job = options.build_job(false).unwrap();
    let tracks: Vec<dvda_core::disc::Track> =
        serde_json::from_value(built.plan.as_ref().unwrap()["Tracks"].clone()).unwrap();
    let disc = dvda_core::disc::plan(tracks, job.disc_bytes, job.max_discs, job.group_track_limit)
        .unwrap()
        .discs
        .remove(0);
    let check_still = || {
        dvda_core::menu_check::verify(
            &job,
            &disc,
            &output,
            &temporary,
            &["--stillpics".into(), "cover.jpg".into()],
            &mut Events::default(),
        )
        .unwrap()
    };
    assert!(
        !check_still()
            .iter()
            .any(|v| v["Code"] == "MENU_STILL_VOB_MISSING")
    );
    for missing in [false, true] {
        if missing {
            fs::remove_file(&authored_still).unwrap();
        } else {
            fs::write(&authored_still, []).unwrap();
        }
        let warnings = check_still();
        assert!(
            warnings
                .iter()
                .any(|v| v["Code"] == "MENU_STILL_VOB_MISSING" && v["Severity"] == 1),
            "{warnings:?}"
        );
        assert!(!warnings.iter().any(|v| v["Severity"] == 2), "{warnings:?}");
    }
    fs::write(&authored_still, still_contents).unwrap();
    let mut missing_images = options.build_job(false).unwrap();
    missing_images.image_library = Some(root.0.join("absent-image.dll"));
    let skipped = dvda_core::menu_check::verify(
        &missing_images,
        &disc,
        &output,
        &temporary,
        &[],
        &mut Events::default(),
    )
    .unwrap();
    assert!(
        skipped
            .iter()
            .any(|d| d["Code"] == "MENU_OVERLAY_CHECK_SKIPPED" && d["Severity"] == 1)
    );
    let mut missing_diagnostics = Vec::new();
    missing_images.author_source = root.0.join("missing-menu-data");
    missing_images.menu_binary_directory = root.0.join("missing-menu-runtime");
    let missing_assets = dvda_core::menu::build_assets(
        &missing_images,
        &disc,
        &mut Events::default(),
        &mut missing_diagnostics,
    )
    .unwrap();
    assert!(missing_assets.is_empty());
    for code in [
        "IMAGEMAGICK_MISSING",
        "IMAGEMAGICK_IDENTIFY_MISSING",
        "MENU_DATA_MISSING",
        "MENU_LIBRARY_MISSING",
    ] {
        assert!(
            missing_diagnostics
                .iter()
                .any(|d| d["Code"] == code && d["Severity"] == 2)
        );
    }
    // A playback JPEG failure is advisory: retaining the valid page background
    // still permits a disc with zero playback pictures. Trigger it by creating
    // a directory at that fixture's output filename after background rendering.
    struct BlockStill {
        root: PathBuf,
        blocked: bool,
    }
    impl Callbacks for BlockStill {
        fn emit(&mut self, _: i32, text: &str) {
            if !self.blocked && text.contains("720 576") && self.root.join("bg0.jpg").is_file() {
                fs::create_dir(self.root.join("still0.jpg")).unwrap();
                self.blocked = true;
            }
        }
        fn cancelled(&mut self) -> bool {
            false
        }
    }
    let mut blocker = BlockStill {
        root: options.path("MenuDirectory").join("disc1"),
        blocked: false,
    };
    let mut still_diagnostics = Vec::new();
    let arguments =
        dvda_core::menu::build_assets(&job, &disc, &mut blocker, &mut still_diagnostics).unwrap();
    assert!(blocker.blocked);
    assert!(
        still_diagnostics
            .iter()
            .any(|d| d["Code"] == "MENU_STILL_FAILED" && d["Severity"] == 1),
        "{still_diagnostics:?}"
    );
    assert!(!still_diagnostics.iter().any(|d| d["Severity"] == 2));
    assert!(!arguments.iter().any(|arg| arg == "--stillpics"));
    // Damage owned fixture bytes only: both structural and payload faults must fail.
    for (name, byte, code) in [
        ("ATS_01_0.IFO", 0, "IFO_INVALID"),
        ("ATS_01_1.AOB", 256, "ISO_AUDIO_MISMATCH"),
    ] {
        let entry = entries.iter().find(|e| e.name == name).unwrap();
        let offset = u64::from(entry.logical_block_address) * 2048 + byte;
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&iso)
            .unwrap();
        file.seek(SeekFrom::Start(offset)).unwrap();
        let mut original = [0];
        file.read_exact(&mut original).unwrap();
        file.seek(SeekFrom::Start(offset)).unwrap();
        file.write_all(&[original[0] ^ 0x55]).unwrap();
        drop(file);
        let rejected = app::run_verify(&options, &mut Events::default()).unwrap();
        assert!(!rejected.succeeded, "corrupt {name} accepted");
        assert!(
            rejected.diagnostics.iter().any(|v| v["Code"] == code),
            "{name}: {:?}",
            rejected.diagnostics
        );
        let mut file = fs::OpenOptions::new().write(true).open(&iso).unwrap();
        file.seek(SeekFrom::Start(offset)).unwrap();
        file.write_all(&original).unwrap();
    }
    let cancelled = app::run_verify(
        &options,
        &mut Events {
            cancelled: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!cancelled.succeeded);
    let temporary = options.path("TemporaryRoot").join("disc1");
    let plan = built.plan.as_ref().unwrap();
    let pages = dvda_core::menu::plan(json!({"Disc":plan["Discs"][0],"Title":"Test",
        "MenuTracksPerPage":1,"MenuIndexMinimumAlbums":1}))
    .unwrap();
    let total = pages["TotalPages"].as_u64().unwrap() as usize;
    assert!(dvda_core::menu_check::buttons(&temporary, total, 1).is_empty());
    fs::write(
        temporary.join("spu_xmltemp_0.xml"),
        "<button name=\"button999\"/>",
    )
    .unwrap();
    assert!(
        dvda_core::menu_check::buttons(&temporary, total, 1)
            .iter()
            .any(|item| item["Code"] == "MENU_BUTTON_MISMATCH")
    );
}

#[test]
#[ignore = "requires source-built media/formats/encoder/author/disc verifier"]
fn application_import_preserves_bytes_and_verifies_aligned_pcm() {
    let root = Scratch::new();
    sources_aligned(&root.0, true);
    let encode = options(&root.0, "surcode-batch");
    assert!(
        app::run_build(&encode, false, &mut Events::default())
            .unwrap()
            .succeeded
    );
    let index: Value =
        serde_json::from_slice(&fs::read(encode.path("MlpIndexPath")).unwrap()).unwrap();
    let imported = root.0.join("import");
    fs::create_dir(&imported).unwrap();
    let mut hashes = Vec::new();
    for group in index["__discs__"][0]["groups"].as_array().unwrap() {
        for track in group["tracks"].as_array().unwrap() {
            let name = Path::new(track["src"].as_str().unwrap())
                .file_stem()
                .unwrap();
            let path = imported.join(name).with_extension("mlp");
            fs::copy(track["mlp"].as_str().unwrap(), &path).unwrap();
            hashes.push((
                path.clone(),
                dvda_core::hash::hex_digest(&fs::read(path).unwrap()),
            ));
        }
    }
    let import=encode.with_profile_values(json!({"DVDA_MLP_SOURCE":"external","DVDA_MLP_EXTERNAL_DIR":imported,
        "DVDA_BUILD_DIR":root.0.join("external-work"),"DVDA_FINAL_DIR":root.0.join("external-final")}).as_object().unwrap().clone()).unwrap();
    let built = app::run_build(&import, false, &mut Events::default()).unwrap();
    assert!(built.succeeded, "{:?}", built.diagnostics);
    let verified = app::run_verify(&import, &mut Events::default()).unwrap();
    assert!(verified.succeeded, "{:?}", verified.diagnostics);
    for (path, hash) in hashes {
        assert_eq!(dvda_core::hash::hex_digest(&fs::read(path).unwrap()), hash);
    }
}

fn assert_diagnostic(outcome: dvda_core::verify::Outcome, code: &str) {
    assert!(!outcome.succeeded, "{code}: unexpectedly succeeded");
    assert!(
        outcome.diagnostics.iter().any(|d| d["Code"] == code),
        "{code}: {:?}",
        outcome.diagnostics
    );
}
fn render_command(command: &dvda_core::verification::DiscCommand) -> String {
    let groups = (0..command.group_count)
        .map(|_| "-g audio.mlp ")
        .collect::<String>();
    let mut result = format!(
        "+ author {groups}-o \"C:\\中文 with spaces\\{}\"\n",
        command.disc_tag
    );
    for row in &command.rows {
        result.push_str(&format!(
            "{} {}/1 {} {} {} {} {} 0\n",
            row.group, row.title, row.track, row.first, row.last, row.pts, row.length
        ));
    }
    result
}
fn run_verify(options: &app::AppOptions) -> dvda_core::verify::Outcome {
    app::run_verify(options, &mut Events::default()).unwrap()
}
#[test]
#[ignore = "requires source-built x64 media/formats/encoder/author/disc verifier"]
fn application_manifest_log_and_sector_audit_regressions() {
    let root = Scratch::new();
    sources(&root.0);
    let options = options(&root.0, "lpcm")
        .with_profile_values(json!({"DVDA_TITLE_MODE":"1"}).as_object().unwrap().clone())
        .unwrap();
    let built = app::run_build(&options, false, &mut Events::default()).unwrap();
    assert!(built.succeeded, "{:?}", built.diagnostics);
    let baseline = run_verify(&options);
    assert!(baseline.succeeded, "{:?}", baseline.diagnostics);
    let volume_iso = options.path("FinalDirectory").join("regression_1.iso");
    let volume_original = fs::read(&volume_iso).unwrap();
    let mut bad_volume = volume_original.clone();
    for sector in 16..48 {
        let offset = sector * 2048;
        if bad_volume[offset + 1..offset + 6] == *b"CD001" && matches!(bad_volume[offset], 1 | 2) {
            bad_volume[offset + 40..offset + 72].fill(b'X');
        }
    }
    fs::write(&volume_iso, bad_volume).unwrap();
    assert_diagnostic(run_verify(&options), "VOLUME_ID_MISMATCH");
    fs::write(&volume_iso, &volume_original).unwrap();
    assert!(
        dvda_core::verify::execute_mode(
            options.verify_job().unwrap(),
            dvda_core::verify::Mode::Menu,
            None,
            &mut Events::default()
        )
        .succeeded
    );
    let log_path = options.path("BuildLogPath");
    let original_log = fs::read_to_string(&log_path).unwrap();
    let parsed = dvda_core::verification::parse_audit_log(&original_log);
    assert_eq!(parsed.commands.len(), 1);
    assert_eq!(parsed.rows.len(), 2);
    assert_ne!(
        parsed.rows[0].title, parsed.rows[1].title,
        "fixture must cover adjacent titles"
    );
    let manifest_path = options.path("ManifestPath");
    let original_manifest = fs::read(&manifest_path).unwrap();
    // IFO + index agree on two tracks. The independent prepared-release plan
    // still catches both an omitted and an extra track in that consistent pair.
    for count in [1, 3] {
        fs::write(
            &manifest_path,
            serde_json::to_vec(&json!({"album":{"files":vec!["source.flac";count]}})).unwrap(),
        )
        .unwrap();
        assert_diagnostic(run_verify(&options), "TRACK_COUNT_MISMATCH");
    }
    fs::write(&manifest_path, b"{ invalid").unwrap();
    assert_diagnostic(run_verify(&options), "MANIFEST_INVALID");
    fs::remove_file(&manifest_path).unwrap();
    assert!(
        run_verify(&options).succeeded,
        "missing optional manifest must not mean zero tracks"
    );
    fs::write(&manifest_path, &original_manifest).unwrap();
    // All legacy log candidates and an explicitly disabled fallback.
    fs::remove_file(&log_path).unwrap();
    let timeline = dvda_core::verify::execute_mode(
        options.verify_job().unwrap(),
        dvda_core::verify::Mode::Timeline,
        None,
        &mut Events::default(),
    );
    assert!(timeline.succeeded, "{:?}", timeline.diagnostics);
    let absent = run_verify(&options);
    assert!(
        absent
            .diagnostics
            .iter()
            .any(|d| d["Code"] == "BUILD_LOG_MISSING" && d["Unavailable"] == true)
    );
    assert!(!absent.succeeded);
    let alternate = log_path.with_file_name("rebuild-final.log");
    fs::write(&alternate, &original_log).unwrap();
    assert!(run_verify(&options).succeeded);
    let mut strict = options.verify_job().unwrap();
    strict.allow_log_fallback = false;
    assert_diagnostic(
        dvda_core::verify::execute(strict, &mut Events::default()),
        "BUILD_LOG_MISSING",
    );
    fs::write(&log_path, "no track table\n").unwrap();
    let oldest = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let newest = oldest + std::time::Duration::from_secs(60);
    fs::File::options()
        .write(true)
        .open(&log_path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(oldest))
        .unwrap();
    fs::File::options()
        .write(true)
        .open(&alternate)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(newest))
        .unwrap();
    assert_eq!(
        dvda_core::verification::select_latest_build_log(&log_path, true).unwrap(),
        alternate
    );
    assert!(run_verify(&options).succeeded);
    let final_rebuild = log_path.with_file_name("finalrebuild.log");
    fs::rename(&alternate, &final_rebuild).unwrap();
    assert!(run_verify(&options).succeeded);
    fs::remove_file(&final_rebuild).unwrap();
    assert_diagnostic(run_verify(&options), "TRACK_TABLE_MISSING");
    for body in ["+ author -g x -o disc1\n", "1 1/1 1 0 9 0 9000 0\n"] {
        fs::write(&log_path, body).unwrap();
        assert_diagnostic(run_verify(&options), "TRACK_TABLE_MISSING");
    }
    for changed in [1, 3] {
        let mut command = parsed.commands[0].clone();
        if changed == 1 {
            command.rows.pop();
        } else {
            command.rows.push(command.rows[1].clone());
        }
        fs::write(&log_path, render_command(&command)).unwrap();
        let outcome = run_verify(&options);
        assert!(
            outcome
                .diagnostics
                .iter()
                .any(|d| d["Code"] == "DISC_TRACK_MAPPING_MISMATCH" && d["Unavailable"] == true)
        );
        assert!(!outcome.succeeded);
    }
    for padding in [
        "pes_padding length must be higher",
        "PeS_PaDdInG LeNgTh MuSt Be HiGhEr",
        "\u{1b}[31mpes_padding length must be higher\u{1b}[0m",
    ] {
        fs::write(&log_path, format!("{original_log}\n{padding}\n")).unwrap();
        assert_diagnostic(run_verify(&options), "PES_PADDING_FAILED");
    }
    let mixed = format!(
        "[C# build] old\n+ author -g ignored -o old\n1 1/1 1 0 9 0 10 0\n[Rust build] [DRY-RUN] earlier\n{original_log}\n[Rust build] [DRY-RUN] later\n"
    );
    fs::write(&log_path, mixed).unwrap();
    assert!(run_verify(&options).succeeded);
    fs::write(&log_path, &original_log).unwrap();
    let iso = options.path("FinalDirectory").join("regression_1.iso");
    let original_iso = fs::read(&iso).unwrap();
    let ifo_entry = dvda_core::formats::iso_list_directory(&iso, "AUDIO_TS")
        .unwrap()
        .into_iter()
        .find(|entry| entry.name == "ATS_01_0.IFO")
        .unwrap();
    let ifo = ifo_entry.logical_block_address as usize * 2048;
    let be32 = |data: &[u8], offset: usize| {
        u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize
    };
    let be16 = |data: &[u8], offset: usize| {
        u16::from_be_bytes(data[offset..offset + 2].try_into().unwrap()) as usize
    };
    let pgc = ifo + be32(&original_iso, ifo + 204) * 2048;
    let title2 = pgc + be32(&original_iso, pgc + 20);
    let table2 = title2 + be16(&original_iso, title2 + 12);
    for delta in [-1i32, 1] {
        let mut corrupt = original_iso.clone();
        let first = be32(&corrupt, table2 + 4) as i32;
        corrupt[table2 + 4..table2 + 8].copy_from_slice(&(first + delta).to_be_bytes());
        fs::write(&iso, corrupt).unwrap();
        assert_diagnostic(run_verify(&options), "TRACK_GAP");
    }
    for delta in [-1i32, 1] {
        let mut corrupt = original_iso.clone();
        let last = be32(&corrupt, table2 + 8) as i32;
        corrupt[table2 + 8..table2 + 12].copy_from_slice(&(last + delta).to_be_bytes());
        fs::write(&iso, corrupt).unwrap();
        assert_diagnostic(run_verify(&options), "AOB_SECTOR_MISMATCH");
    }
    fs::write(&iso, &original_iso).unwrap();
    // Corrupt only the log relation: a self-consistent IFO/index cannot hide it.
    for delta in [-1, 1] {
        let mut command = parsed.commands[0].clone();
        command.rows[1].first += delta;
        fs::write(&log_path, render_command(&command)).unwrap();
        assert_diagnostic(run_verify(&options), "TRACK_GAP");
        let mut command = parsed.commands[0].clone();
        command.rows[1].last += delta;
        fs::write(&log_path, render_command(&command)).unwrap();
        assert_diagnostic(run_verify(&options), "AOB_SECTOR_MISMATCH");
    }
    fs::write(&log_path, &original_log).unwrap();
    // A real two-disc release with independent manifest totals; commands appear
    // in reverse order to ensure numeric tags win over the positional fallback.
    let index_path = options.path("MlpIndexPath");
    let original_index = fs::read(&index_path).unwrap();
    let mut index: Value = serde_json::from_slice(&original_index).unwrap();
    let mut second = index["__discs__"][0].clone();
    second["iso"] = json!("regression_2.iso");
    index["__discs__"].as_array_mut().unwrap().push(second);
    let second_iso = iso.with_file_name("regression_2.iso");
    let mut second_bytes = original_iso.clone();
    let title = format!("{} 2", options.text("Title"));
    second_bytes[16 * 2048 + 40..16 * 2048 + 72].fill(b' ');
    second_bytes[16 * 2048 + 40..16 * 2048 + 40 + title.len()].copy_from_slice(title.as_bytes());
    fs::write(&second_iso, second_bytes).unwrap();
    fs::write(&index_path, serde_json::to_vec(&index).unwrap()).unwrap();
    fs::write(
        &manifest_path,
        serde_json::to_vec(&json!({"one":{"files":["a","b"]},"two":{"files":["c","d"]}})).unwrap(),
    )
    .unwrap();
    let mut second_command = parsed.commands[0].clone();
    second_command.disc_tag = "disc2".into();
    fs::write(
        &log_path,
        render_command(&second_command) + &render_command(&parsed.commands[0]),
    )
    .unwrap();
    let two = run_verify(&options);
    assert!(two.succeeded, "{:?}", two.diagnostics);
    assert_eq!(two.track_count, 4);
    assert_eq!(two.iso_count, 2);
    fs::write(&log_path, render_command(&parsed.commands[0])).unwrap();
    assert_diagnostic(run_verify(&options), "DISC_LOG_MAPPING_MISSING");
    fs::remove_file(&second_iso).unwrap();
    fs::write(&index_path, &original_index).unwrap();
    fs::write(&manifest_path, &original_manifest).unwrap();
    fs::write(&log_path, &original_log).unwrap();
    assert!(run_verify(&options).succeeded);
    // Split the original byte-identical payload into two directory AOB entries.
    // Both streaming validators and the sector-total audit must sum all segments.
    let segmented = split_aob(&iso, &original_iso);
    fs::write(&iso, segmented).unwrap();
    let split_result = run_verify(&options);
    assert!(split_result.succeeded, "{:?}", split_result.diagnostics);
    fs::write(&iso, &original_iso).unwrap();
    // Standalone audit and quick-check use the same real disc, no index or media.
    let inspect = |audit| dvda_core::verify::Inspection {
        final_directory: options.path("FinalDirectory"),
        iso_prefix: "regression".into(),
        manifest_path: Some(manifest_path.clone()),
        build_log_path: Some(log_path.clone()),
        allow_log_fallback: false,
        audit,
    };
    fs::remove_file(index_path).unwrap();
    for audit in [false, true] {
        let result = dvda_core::verify::inspect(inspect(audit), &mut Events::default());
        assert!(result.succeeded, "{:?}", result.diagnostics);
    }
}

fn set_iso_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    bytes[offset + 4..offset + 8].copy_from_slice(&value.to_be_bytes());
}
fn iso_record_offset(iso: &Path, bytes: &[u8], name: &str) -> usize {
    let directory =
        dvda_core::formats::dispatch("iso.entry", json!({"Path":iso,"InnerPath":"AUDIO_TS"}))
            .unwrap();
    let base = directory["LogicalBlockAddress"].as_u64().unwrap() as usize * 2048;
    let end = base + directory["Size"].as_u64().unwrap() as usize;
    let mut offset = base;
    while offset < end {
        let length = bytes[offset] as usize;
        if length == 0 {
            offset = (offset / 2048 + 1) * 2048;
            continue;
        }
        let count = bytes[offset + 32] as usize;
        if String::from_utf8_lossy(&bytes[offset + 33..offset + 33 + count])
            .split(';')
            .next()
            == Some(name)
        {
            return offset;
        }
        offset += length;
    }
    panic!("ISO directory record not found: {name}");
}
fn split_aob(iso: &Path, original: &[u8]) -> Vec<u8> {
    let first = iso_record_offset(iso, original, "ATS_01_1.AOB");
    let length = original[first] as usize;
    let mut end = first + length;
    while original[end] != 0 {
        end += original[end] as usize;
    }
    assert_eq!(
        end / 2048,
        (end + length - 1) / 2048,
        "fixture directory needs room for another AOB"
    );
    let mut result = original.to_vec();
    let lba = u32::from_le_bytes(original[first + 2..first + 6].try_into().unwrap());
    let size = u32::from_le_bytes(original[first + 10..first + 14].try_into().unwrap());
    let first_bytes = (size / 2048 / 2) * 2048;
    assert!(first_bytes > 0);
    result[end..end + length].copy_from_slice(&original[first..first + length]);
    result[end + 33 + 7] = b'2';
    set_iso_u32(&mut result, first + 10, first_bytes);
    set_iso_u32(&mut result, end + 2, lba + first_bytes / 2048);
    set_iso_u32(&mut result, end + 10, size - first_bytes);
    result
}

#[test]
#[ignore = "requires source-built x64 media/formats/encoder/author/disc verifier"]
fn application_failed_author_keeps_workspace_and_previous_release() {
    let root = Scratch::new();
    sources(&root.0);
    let options = options(&root.0, "lpcm")
        .with_profile_values(
            json!({"DVDA_KEEP_TMP":true,"DVDA_KEEP_INTERMEDIATE":true,"DVDA_RESUME":false})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
    let success = app::run_build(&options, false, &mut Events::default()).unwrap();
    assert!(success.succeeded, "{:?}", success.diagnostics);
    let iso = options.path("FinalDirectory").join("regression_1.iso");
    let previous = fs::read(&iso).unwrap();
    let old_index = fs::read(options.path("MlpIndexPath")).unwrap();
    assert!(options.path("OutputRoot").join("disc1/AUDIO_TS").is_dir());
    assert!(options.path("TemporaryRoot").join("disc1").is_dir());
    assert!(options.path("IsoDirectory").join("disc1.iso").is_file());
    let mut bad = options.build_job(false).unwrap();
    // An executable with a deterministic non-zero response to author arguments
    // is sufficient: the workflow must retain evidence for any author failure.
    let failed_author_directory = root.0.join("failing-author");
    fs::create_dir(&failed_author_directory).unwrap();
    let failed_author = failed_author_directory.join("author.exe");
    fs::copy(
        PathBuf::from(std::env::var_os("WINDIR").unwrap()).join("System32/where.exe"),
        &failed_author,
    )
    .unwrap();
    bad.dvda_author = failed_author.to_string_lossy().into_owned();
    let failed = dvda_core::build::execute(bad, &mut Events::default());
    assert_diagnostic(
        dvda_core::verify::Outcome {
            succeeded: failed.succeeded,
            diagnostics: failed.diagnostics,
            iso_count: 0,
            track_count: 0,
        },
        "DVDA_AUTHOR_FAILED",
    );
    assert!(options.path("OutputRoot").join("disc1").is_dir());
    assert!(options.path("TemporaryRoot").join("disc1").is_dir());
    assert_eq!(fs::read(&iso).unwrap(), previous);
    assert_eq!(fs::read(options.path("MlpIndexPath")).unwrap(), old_index);
    assert_eq!(
        fs::read_dir(options.path("FinalDirectory"))
            .unwrap()
            .filter(|entry| entry
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|extension| extension == "iso"))
            .count(),
        1
    );
    // Locking the previous release simulates Windows access/replacement failure.
    use std::os::windows::fs::OpenOptionsExt;
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&iso)
        .unwrap();
    let failed = app::run_build(&options, false, &mut Events::default()).unwrap();
    assert!(!failed.succeeded);
    assert!(
        failed
            .diagnostics
            .iter()
            .any(|d| d["Code"] == "FINAL_PUBLICATION_FAILED"),
        "{:?}",
        failed.diagnostics
    );
    assert_eq!(fs::read(&iso).unwrap(), previous);
    assert_eq!(fs::read(options.path("MlpIndexPath")).unwrap(), old_index);
    drop(locked);
    let recovered = app::run_build(&options, false, &mut Events::default()).unwrap();
    assert!(recovered.succeeded, "{:?}", recovered.diagnostics);
    let resume = options
        // KeepIntermediate deliberately disables resume, as in the old flow.
        .with_profile_values(
            json!({"DVDA_RESUME":true,"DVDA_KEEP_INTERMEDIATE":false})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
    fs::write(
        options
            .path("BuildDirectory")
            .join("publish-staging/resume.json"),
        b"{ corrupt",
    )
    .unwrap();
    let mut events = Events::default();
    let rebuilt = app::run_build(&resume, false, &mut events).unwrap();
    assert!(rebuilt.succeeded, "{:?}", rebuilt.diagnostics);
    assert!(
        events
            .lines
            .iter()
            .any(|line| line.starts_with("[警告] 续跑记录无法读取，将重新出盘:"))
    );
    assert!(
        events
            .lines
            .iter()
            .any(|line| line.starts_with("[author] disc"))
    );
    // Missing artwork warns and uses black backgrounds; index artwork may not be
    // silently compressed because index cells refer to cover positions.
    let data = PathBuf::from(std::env::var_os("DVDA_TEST_MENU_DATA").unwrap());
    let menu=options.with_profile_values(json!({"DVDA_MENU":true,"DVDA_MENU_STILLPICS":true,"DVDA_MENU_INDEX_MIN_ALBUMS":99,"DVDA_AUTHOR_SRC":data}).as_object().unwrap().clone()).unwrap();
    let no_cover = app::run_build(&menu, false, &mut Events::default()).unwrap();
    assert!(no_cover.succeeded, "{:?}", no_cover.diagnostics);
    assert!(
        no_cover
            .diagnostics
            .iter()
            .any(|d| d["Code"] == "MENU_COVER_MISSING" && d["Severity"] == 1)
    );
    let index: Value =
        serde_json::from_slice(&fs::read(menu.path("MlpIndexPath")).unwrap()).unwrap();
    assert_eq!(index["__discs__"][0]["menu"]["stills"], 0);
    let menu_index = menu
        .with_profile_values(
            json!({"DVDA_MENU_INDEX_MIN_ALBUMS":1})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
    let failure = app::run_build(&menu_index, false, &mut Events::default()).unwrap();
    assert!(!failure.succeeded);
    for code in [
        "MENU_INDEX_COVER_MISSING",
        "MENU_INDEX_COVER_COUNT_MISMATCH",
    ] {
        assert!(
            failure
                .diagnostics
                .iter()
                .any(|d| d["Code"] == code && d["Severity"] == 2),
            "{:?}",
            failure.diagnostics
        );
    }
}
