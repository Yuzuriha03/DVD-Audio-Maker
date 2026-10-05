//! Real command/ISO regressions for the independent migration recheck R01-R05.
use dvda_core::{formats, media};
use dvda_native::media::{Callbacks, Operation, OutputFormat, Request};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "dvda-recheck-中文-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
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
struct Quiet;
impl Callbacks for Quiet {
    fn emit(&mut self, _: i32, _: &str) {}
    fn cancelled(&mut self) -> bool {
        false
    }
}
fn run(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dvda-cli"));
    for (key, _) in std::env::vars().filter(|(key, _)| key.starts_with("DVDA_")) {
        if !key.contains("NATIVE") && !key.contains("LIBRARY") {
            command.env_remove(key);
        }
    }
    command
        .current_dir(root)
        .env("LOCALAPPDATA", root)
        .args(args)
        .output()
        .unwrap()
}
fn expect(output: Output, code: i32) -> String {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn outcome(output: Output, code: i32) -> Value {
    let text = expect(output, code);
    let start = if text.starts_with('{') {
        0
    } else {
        text.rfind("\n{").expect("JSON outcome") + 1
    };
    serde_json::from_str(&text[start..]).unwrap()
}
fn has(value: &Value, code: &str) -> bool {
    value["Diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["Code"] == code)
}
fn write_profile(root: &Path, values: Value) -> PathBuf {
    let path = root.join("settings.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({"Version":1,"Language":"en","Values":values})).unwrap(),
    )
    .unwrap();
    path
}

struct Fixture {
    scratch: Scratch,
    profile: PathBuf,
    iso: PathBuf,
    work: PathBuf,
    original: Vec<u8>,
}
impl Fixture {
    fn build() -> Self {
        let scratch = Scratch::new();
        let root = &scratch.0;
        let src = root.join("src");
        fs::create_dir(&src).unwrap();
        let library = PathBuf::from(
            std::env::var_os("DVDA_MEDIA_NATIVE_DIR").expect("native media directory"),
        )
        .join("dvda-media.dll");
        // Two one-second, frame-aligned tracks make a genuine two-title LPCM
        // disc. Its legal title reset is below the legacy 1% abnormal limit.
        for track in 1..=2u32 {
            let frames = 48_000u32;
            let size = frames * 4;
            let mut wav = Vec::new();
            wav.extend(b"RIFF");
            wav.extend((size + 36).to_le_bytes());
            wav.extend(b"WAVEfmt ");
            wav.extend(16u32.to_le_bytes());
            wav.extend(1u16.to_le_bytes());
            wav.extend(2u16.to_le_bytes());
            wav.extend(48000u32.to_le_bytes());
            wav.extend(192000u32.to_le_bytes());
            wav.extend(4u16.to_le_bytes());
            wav.extend(16u16.to_le_bytes());
            wav.extend(b"data");
            wav.extend(size.to_le_bytes());
            for frame in 0..frames {
                for channel in 0..2 {
                    let sample =
                        ((frame * (97 + channel * 18) + track * 53) % 60001) as i32 - 30000;
                    wav.extend((sample as i16).to_le_bytes());
                }
            }
            let input = root.join("input.wav");
            fs::write(&input, wav).unwrap();
            let result = media::execute(
                media::Job {
                    library: library.clone(),
                    replace: false,
                    timeout_millis: None,
                    request: Request {
                        operation: Operation::Audio,
                        input: input.to_string_lossy().into_owned(),
                        output: Some(
                            src.join(format!("{track}.flac"))
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
                            ("ALBUM".into(), "Verification recheck".into()),
                            ("TRACK".into(), track.to_string()),
                        ],
                    },
                },
                &mut Quiet,
            );
            assert_eq!(result.exit_code, Some(0), "{:?}", result.failure);
        }
        let work = root.join("work");
        let out = root.join("out");
        let profile = write_profile(
            root,
            json!({
                "DVDA_SRC":src, "DVDA_BUILD_DIR":work, "DVDA_FINAL_DIR":out,
                "DVDA_TITLE":"Verification recheck", "DVDA_ISO_PREFIX":"check",
                "DVDA_MLP_SOURCE":"lpcm", "DVDA_MENU":"off", "DVDA_TITLE_MODE":"1",
                "DVDA_AUTHOR":std::env::var("DVDA_TEST_AUTHOR").expect("source built author")
            }),
        );
        expect(
            run(root, &["prepare", "--profile", profile.to_str().unwrap()]),
            0,
        );
        expect(
            run(
                root,
                &[
                    "build",
                    "--profile",
                    profile.to_str().unwrap(),
                    "--no-resume",
                ],
            ),
            0,
        );
        let iso = out.join("check_1.iso");
        let original = fs::read(&iso).unwrap();
        Self {
            scratch,
            profile,
            iso,
            work,
            original,
        }
    }
    fn verify(&self, mode: &str, code: i32) -> Value {
        outcome(
            run(
                &self.scratch.0,
                &["verify", mode, "--profile", self.profile.to_str().unwrap()],
            ),
            code,
        )
    }
    fn inspect(&self, mode: &str, explicit_directory: bool, code: i32) -> Value {
        let directory = self.iso.parent().unwrap().to_str().unwrap();
        let mut args = vec![mode, "--profile", self.profile.to_str().unwrap()];
        if explicit_directory {
            args.extend(["--iso-dir", directory]);
        }
        outcome(run(&self.scratch.0, &args), code)
    }
    fn rewrite_pts(&self, mut change: impl FnMut(usize, i64) -> Option<i64>) -> usize {
        let entries = formats::iso_list_directory(&self.iso, "AUDIO_TS").unwrap();
        let mut bytes = self.original.clone();
        let mut count = 0;
        for entry in entries.iter().filter(|entry| entry.name.ends_with(".AOB")) {
            let offset = (entry.logical_block_address as usize
                + entry.extended_attribute_blocks as usize)
                * 2048;
            for sector in bytes[offset..offset + entry.size as usize]
                .as_chunks_mut::<2048>()
                .0
            {
                let marker = sector[4..64]
                    .windows(4)
                    .position(|value| value == [0, 0, 1, 0xbd])
                    .unwrap()
                    + 4;
                let old = dvda_core::aob::sector_pts(sector, true).unwrap();
                assert!(old >= 0);
                if let Some(value) = change(count, old) {
                    let pts = value as u64;
                    sector[marker + 9..marker + 14].copy_from_slice(&[
                        0x21 | (((pts >> 30) as u8 & 7) << 1),
                        (pts >> 22) as u8,
                        1 | (((pts >> 15) as u8 & 127) << 1),
                        (pts >> 7) as u8,
                        1 | ((pts as u8 & 127) << 1),
                    ]);
                } else {
                    sector[marker + 7] &= !0x80;
                }
                count += 1;
            }
        }
        fs::write(&self.iso, bytes).unwrap();
        count
    }
}

#[test]
#[ignore = "requires source-built x64 native media/formats/author/verifier"]
fn formal_timeline_and_all_check_pts_statistics_and_legal_title_resets() {
    let fixture = Fixture::build();
    for mode in ["timeline", "all"] {
        let result = fixture.verify(mode, 0);
        assert_eq!(result["TrackCount"], 2);
    }
    let entries = formats::iso_list_directory(&fixture.iso, "AUDIO_TS").unwrap();
    let aob = entries
        .iter()
        .find(|entry| entry.name == "ATS_01_1.AOB")
        .unwrap();
    let start = aob.logical_block_address as usize * 2048;
    let values: Vec<_> = fixture.original[start..start + aob.size as usize]
        .as_chunks::<2048>()
        .0
        .iter()
        .map(|sector| dvda_core::aob::sector_pts(sector, true).unwrap())
        .collect();
    assert!(
        values.windows(2).any(|pair| pair[1] < pair[0]),
        "fixture must exercise a legal title reset"
    );
    assert!(fixture.rewrite_pts(|_, _| Some(0)) > 100);
    for mode in ["timeline", "all"] {
        let result = fixture.verify(mode, 1);
        assert!(has(&result, "PTS_NOT_ADVANCING"), "{result}");
        assert!(has(&result, "PTS_ABNORMAL_RATIO"), "{result}");
        assert!(
            !has(&result, "ISO_AUDIO_MISMATCH"),
            "timestamps alone must preserve encoded PCM bytes: {result}"
        );
    }
    fixture.rewrite_pts(|index, old| (index < 2).then_some(old));
    for mode in ["timeline", "all"] {
        let result = fixture.verify(mode, 1);
        assert!(has(&result, "PTS_TOO_FEW"), "{result}");
        assert!(has(&result, "PTS_MISSING"), "{result}");
    }
    // Monotone, advancing timestamps with frequent zero steps independently
    // exercise the abnormal ratio check without the constant-clock check.
    fixture.rewrite_pts(|index, _| Some((index / 2 * 900) as i64));
    for mode in ["timeline", "all"] {
        let result = fixture.verify(mode, 1);
        assert!(has(&result, "PTS_ABNORMAL_RATIO"), "{result}");
        assert!(!has(&result, "PTS_NOT_ADVANCING"), "{result}");
    }
    fs::write(&fixture.iso, &fixture.original).unwrap();
    fixture.verify("all", 0);
}

#[test]
#[ignore = "requires source-built x64 native media/formats/author/verifier"]
fn index_material_failures_do_not_skip_all_independent_checks() {
    let fixture = Fixture::build();
    let index = fixture.work.join("mlp_index.json");
    let original = fs::read(&index).unwrap();
    for (content, code) in [
        (
            Some(b"{\"__discs__\":[]}".as_slice()),
            "LOSSLESS_SOURCE_MISSING",
        ),
        (
            Some(b"{\"__discs__\":[{\"iso\":\"check_1.iso\",\"groups\":[]}]}".as_slice()),
            "LOSSLESS_SOURCE_MISSING",
        ),
        (
            Some(b"{\"__discs__\":[{\"iso\":\"check_1.iso\",\"groups\":[{\"group\":1,\"tracks\":[]}]}]}".as_slice()),
            "LOSSLESS_SOURCE_MISSING",
        ),
        (None, "MLP_INDEX_MISSING"),
        (Some(b"{broken".as_slice()), "MLP_INDEX_INVALID"),
        (
            Some(b"{\"__meta__\":{\"dry_run\":true},\"__discs__\":[]}".as_slice()),
            "MLP_INDEX_DRY_RUN",
        ),
    ] {
        if let Some(bytes) = content {
            fs::write(&index, bytes).unwrap();
        } else {
            fs::remove_file(&index).unwrap();
        }
        for mode in ["lossless", "all"] {
            let result = fixture.verify(mode, 2);
            assert!(has(&result, code), "{result}");
            let issue = result["Diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .find(|issue| issue["Code"] == code)
                .unwrap();
            assert_eq!(issue["Unavailable"], true);
            assert_eq!(result["Succeeded"], false);
        }
        fixture.verify("timeline", 0);
    }
    fs::remove_file(&index).unwrap();
    fixture.rewrite_pts(|_, _| Some(0));
    fs::write(
        fixture.work.join("manifest.json"),
        b"{\"album\":{\"files\":[]}}",
    )
    .unwrap();
    let result = fixture.verify("all", 1);
    for code in [
        "MLP_INDEX_MISSING",
        "PTS_NOT_ADVANCING",
        "TRACK_COUNT_MISMATCH",
    ] {
        assert!(
            has(&result, code),
            "independent check must survive missing index: {result}"
        );
    }
    assert_eq!(result["TrackCount"], 2);
    fs::write(&fixture.iso, b"invalid ISO").unwrap();
    let result = fixture.verify("all", 1);
    assert!(
        has(&result, "MLP_INDEX_MISSING") && has(&result, "ISO_READ_FAILED"),
        "{result}"
    );
    fs::write(index, original).unwrap();
}

#[test]
fn explicit_verification_material_paths_must_exist() {
    let root = Scratch::new();
    let profile = write_profile(
        &root.0,
        json!({"DVDA_FINAL_DIR":root.0,"DVDA_ISO_PREFIX":"check"}),
    );
    for mode in ["quick-check", "audit", "verify"] {
        for option in ["--manifest", "--log", "--iso-dir"] {
            let mut args = vec![mode];
            if mode == "verify" {
                args.push("quick");
            }
            args.extend([
                "--profile",
                profile.to_str().unwrap(),
                option,
                "explicitly-missing",
            ]);
            let output = run(&root.0, &args);
            assert!(String::from_utf8_lossy(&output.stderr).contains("explicitly-missing"));
            expect(output, 2);
        }
    }
}

#[test]
#[ignore = "requires source-built x64 native media/formats/author/verifier"]
fn iso_directory_override_preserves_prefix_and_configured_evidence() {
    let fixture = Fixture::build();
    // The corrupt file deliberately belongs to a different project.
    fs::write(
        fixture.iso.parent().unwrap().join("unrelated_1.iso"),
        b"invalid ISO",
    )
    .unwrap();
    for explicit in [false, true] {
        for mode in ["quick-check", "audit"] {
            let result = fixture.inspect(mode, explicit, 0);
            assert_eq!(result["IsoCount"], 1);
            assert_eq!(result["TrackCount"], 2);
        }
    }
    let manifest = fixture.work.join("manifest.json");
    let original_manifest = fs::read(&manifest).unwrap();
    fs::write(&manifest, b"{\"album\":{\"files\":[]}}").unwrap();
    for explicit in [false, true] {
        for mode in ["quick-check", "audit"] {
            assert!(has(
                &fixture.inspect(mode, explicit, 1),
                "TRACK_COUNT_MISMATCH"
            ));
        }
    }
    fs::write(&manifest, original_manifest).unwrap();
    let log = fixture.work.join("build.log");
    let original_log = fs::read(&log).unwrap();
    let mut bad_log = original_log.clone();
    bad_log.extend(b"\nPES_padding length must be higher\n");
    fs::write(&log, bad_log).unwrap();
    for explicit in [false, true] {
        for mode in ["quick-check", "audit"] {
            assert!(has(
                &fixture.inspect(mode, explicit, 1),
                "PES_PADDING_FAILED"
            ));
        }
    }
    fs::write(&log, original_log).unwrap();
    fs::remove_file(manifest).unwrap();
    // An absent default optional manifest remains valid.
    fixture.inspect("quick-check", true, 0);
}
