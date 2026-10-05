use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "dvda-cli-中文-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
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
fn run(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dvda-cli"));
    for (key, _) in std::env::vars().filter(|(k, _)| k.starts_with("DVDA_")) {
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
fn ok(output: Output) -> String {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn disc_source(root: &Path, destination: &Path) {
    use dvda_core::media;
    use dvda_native::media::{Callbacks, Operation, OutputFormat, Request};
    struct Quiet;
    impl Callbacks for Quiet {
        fn emit(&mut self, _: i32, _: &str) {}
        fn cancelled(&mut self) -> bool {
            false
        }
    }
    // The ALAC repair fixture above is only two MLP packs long. A generated
    // one-second source also satisfies the real timeline's minimum PTS count.
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
            wav.extend(
                (((frame * (97 + channel * 18)) % 60001) as i32 - 30000).to_le_bytes()[..2].iter(),
            );
        }
    }
    let input = root.join("disc.wav");
    fs::write(&input, wav).unwrap();
    let result = media::execute(
        media::Job {
            library: PathBuf::from(std::env::var_os("DVDA_MEDIA_NATIVE_DIR").unwrap())
                .join("dvda-media.dll"),
            replace: false,
            timeout_millis: None,
            request: Request {
                operation: Operation::Audio,
                input: input.to_string_lossy().into_owned(),
                output: Some(destination.to_string_lossy().into_owned()),
                rate: 0,
                bits: 0,
                output_format: OutputFormat::Flac,
                soxr: false,
                compression: 8,
                cover: false,
                tags: vec![],
            },
        },
        &mut Quiet,
    );
    assert_eq!(result.exit_code, Some(0), "{:?}", result.failure);
}
#[test]
fn json_profiles_help_shell_and_argument_validation() {
    let root = Scratch::new();
    let profile = root.0.join("方案.json");
    fs::write(&profile,serde_json::to_vec(&json!({"Version":1,"Language":"ja","Values":{"DVDA_SRC":"C:/source","DVDA_FINAL_DIR":"C:/output","DVDA_TITLE":"a'b"}})).unwrap()).unwrap();
    let path = profile.to_str().unwrap();
    assert!(ok(run(&root.0, &["help"])).contains("convert"));
    let value: Value =
        serde_json::from_str(&ok(run(&root.0, &["config", "--profile", path]))).unwrap();
    assert_eq!(value["Language"], "ja");
    assert_eq!(
        run(&root.0, &["config", "--profile", path, "--check"])
            .status
            .code(),
        Some(0)
    );
    let shell = ok(run(&root.0, &["config", "--profile", path, "--shell"]));
    assert!(shell.contains("DVDA_TITLE='a'\\''b'"));
    assert!(shell.contains("DVDA_ISO_PREFIX='a_b'"));
    assert!(shell.contains("DVDA_MANIFEST='") && shell.contains("manifest.json"));
    assert!(shell.contains("DVDA_LOSS_ERROR_S='0.05'"));
    assert!(!shell.contains("DVDA_FFMPEG="));
    assert!(!shell.contains("DVDA_MENU_DIR="));
    assert!(
        ok(run(&root.0, &["config", "--profile", path, "--shell-all"])).contains("DVDA_MENU_DIR=")
    );
    assert_eq!(value["Sources"]["DVDA_SRC"], "方案.json");
    let aliases: Value = serde_json::from_str(&ok(run(
        &root.0,
        &["--language", "日本語", "config", "--profile", path],
    )))
    .unwrap();
    assert_eq!(aliases["Language"], "ja");
    for args in [
        vec!["--profile", path, "--profile", path],
        vec!["config", "--check", "--shell"],
        vec!["convert", "--jobs", "0", "x.m4a"],
        vec!["mlp", "--check", "--align", "x.mlp"],
        vec!["build", "--unknown"],
        vec!["--language", "xx", "config"],
        vec!["config", "--profile", "missing.json"],
    ] {
        assert_eq!(run(&root.0, &args).status.code(), Some(2), "{args:?}");
    }
    let env = root.0.join("config.env");
    fs::write(&env, b"DVDA_SRC=x").unwrap();
    assert_eq!(
        run(&root.0, &["config", "--profile", env.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
    assert!(ok(run(&root.0, &["abi.version"])).contains("rust"));
}
#[test]
#[ignore = "requires existing source-built x64 media/formats/encoder/author/verifier"]
fn cli_conversion_audio_tools_and_disc_verification_modes() {
    let root = Scratch::new();
    let src = root.0.join("src");
    fs::create_dir(&src).unwrap();
    let original = include_bytes!("../../dvda-core/tests/fixtures/alac-valid.m4a");
    let input = src.join("音源.m4a");
    fs::write(&input, original).unwrap();
    let input_text = input.to_str().unwrap();
    assert!(ok(run(&root.0, &["convert", input_text, "--dry-run"])).contains("DRY"));
    assert!(!input.with_extension("flac").exists());
    assert!(ok(run(&root.0, &["m4a2flac", input_text, "--jobs", "2"])).contains("PcmMd5"));
    ok(run(&root.0, &["alac", "check", input_text]));
    let bad = root.0.join("bad.m4a");
    fs::write(
        &bad,
        include_bytes!("../../dvda-core/tests/fixtures/alac-broken.m4a"),
    )
    .unwrap();
    assert_eq!(
        run(&root.0, &["alac", "check", bad.to_str().unwrap()])
            .status
            .code(),
        Some(1)
    );
    ok(run(&root.0, &["alac", "repair", bad.to_str().unwrap()]));
    assert_eq!(
        fs::read(root.0.join("bad.m4a.fixed.m4a")).unwrap(),
        original
    );
    // Keep one prepared source, and execute the actual developer entrypoints.
    fs::remove_file(input.with_extension("flac")).unwrap();
    fs::remove_file(&input).unwrap();
    disc_source(&root.0, &src.join("disc.flac"));
    let profile = root.0.join("settings.json");
    let out = root.0.join("out");
    let build = root.0.join("work");
    let author = std::env::var("DVDA_TEST_AUTHOR").expect("test author");
    fs::write(&profile,serde_json::to_vec(&json!({"Version":1,"Language":"en","Values":{"DVDA_SRC":src,"DVDA_FINAL_DIR":out,"DVDA_BUILD_DIR":build,"DVDA_TITLE":"Developer fixture","DVDA_ISO_PREFIX":"fixture","DVDA_DISC_BYTES":"4700000000","DVDA_AUTHOR":author,"DVDA_MENU":"off"}})).unwrap()).unwrap();
    let profile = profile.to_str().unwrap();
    ok(run(&root.0, &["prepare", "--profile", profile, "--force"]));
    let plan = run(&root.0, &["plan", "--profile", profile]);
    assert!([Some(0), Some(1)].contains(&plan.status.code()));
    assert!(String::from_utf8_lossy(&plan.stdout).contains("Discs"));
    ok(run(
        &root.0,
        &["build", "--profile", profile, "--dry-run", "--no-resume"],
    ));
    assert!(!out.join("fixture_1.iso").exists());
    ok(run(
        &root.0,
        &["build", "--profile", profile, "--no-resume"],
    ));
    for mode in ["quick", "capacity", "audit", "timeline", "lossless", "all"] {
        ok(run(&root.0, &["verify", mode, "--profile", profile]));
    }
    let iso = fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|x| x == "iso"))
        .unwrap();
    let iso_text = iso.to_str().unwrap();
    let list = ok(run(&root.0, &["iso", "list", iso_text, "AUDIO_TS"]));
    assert!(list.contains("ATS_01_1.AOB"));
    let aob = root.0.join("audio.aob");
    ok(run(
        &root.0,
        &[
            "iso",
            "extract",
            iso_text,
            "AUDIO_TS/ATS_01_1.AOB",
            aob.to_str().unwrap(),
        ],
    ));
    let pts = run(&root.0, &["aob-pts", aob.to_str().unwrap()]);
    assert!([Some(0), Some(1)].contains(&pts.status.code()));
    assert!(String::from_utf8_lossy(&pts.stdout).contains("Statistics"));
    ok(run(
        &root.0,
        &[
            "quick-check",
            "--profile",
            profile,
            "--iso-dir",
            out.to_str().unwrap(),
        ],
    ));
    assert_eq!(
        run(
            &root.0,
            &[
                "audit",
                "--profile",
                profile,
                "--iso-dir",
                out.to_str().unwrap(),
                "--log",
                root.0.join("absent.log").to_str().unwrap()
            ]
        )
        .status
        .code(),
        Some(2)
    );
    let index: Value =
        serde_json::from_slice(&fs::read(build.join("mlp_index.json")).unwrap()).unwrap();
    let mlp = index
        .as_object()
        .unwrap()
        .keys()
        .find(|k| k.ends_with(".mlp"))
        .unwrap();
    ok(run(&root.0, &["mlp", "--check", mlp]));
    let before = fs::read(mlp).unwrap();
    ok(run(&root.0, &["mlp", "--align", mlp, "-q"]));
    assert_eq!(fs::read(mlp).unwrap(), before);
}
