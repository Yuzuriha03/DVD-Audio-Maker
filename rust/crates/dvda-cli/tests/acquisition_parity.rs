use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "dvda-mlp-import-中文-{}-{}-{}",
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

fn dispatch(operation: &str, request: Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dvda-cli"))
        .arg(operation)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{operation}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn mlp_import_unicode_paths_preserve_mirrors_and_outside_root_fallbacks() {
    let root = Scratch::new();
    let mirror = root.0.join("艺人").join("album").join("track.mlp");
    fs::create_dir_all(mirror.parent().unwrap()).unwrap();
    fs::write(&mirror, b"mirror").unwrap();
    assert_eq!(
        dispatch(
            "build.mlp_resolve",
            json!({
                "SourcePath":r"C:\MÚSIC\艺人\album\track.flac",
                "SourceRoot":r"c:\músic\", "ExternalRoot":root.0,
                "BasenameIndex":{"track":"wrong-fallback.mlp"}
            })
        ),
        json!(mirror)
    );
    for source_root in [r"C:\abcd", "", r"C:\中中\song", r"C:\中"] {
        assert_eq!(
            dispatch(
                "build.mlp_resolve",
                json!({
                    "SourcePath":r"C:\中中\song.flac", "SourceRoot":source_root,
                    "ExternalRoot":root.0, "BasenameIndex":{"SONG":"expected-song.mlp"}
                })
            ),
            "expected-song.mlp"
        );
        assert_eq!(
            dispatch(
                "build.mlp_destination",
                json!({
                    "SourcePath":r"C:\中中\song.flac", "SourceRoot":source_root,
                    "OutputRoot":root.0
                })
            ),
            json!(root.0.join("song.mlp"))
        );
    }
}

#[test]
fn mlp_import_uses_unicode_ordinal_case_without_linguistic_expansion() {
    let root = Scratch::new();
    for (directory, name) in [
        ("a", "Été"),
        ("b", "été"),
        ("c", "ÉTÉ"),
        ("a", "Éclair"),
        ("b", "éCLAIR"),
        ("a", "Straße"),
        ("b", "STRASSE"),
        ("a", "Élan"),
    ] {
        let path = root.0.join(directory).join(format!("{name}.mlp"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"fixture").unwrap();
    }
    let lookup = dispatch("build.mlp_lookup", json!({"Root":root.0}));
    assert_eq!(lookup["AmbiguousNames"], json!(["Éclair", "Été"]));
    assert_eq!(lookup["Index"].as_object().unwrap().len(), 3);
    for (source_name, expected) in [
        ("élAN", Some("a/Élan.mlp")),
        ("STRAßE", Some("a/Straße.mlp")),
        ("strasse", Some("b/STRASSE.mlp")),
        ("été", None),
    ] {
        let actual = dispatch(
            "build.mlp_resolve",
            json!({
                "SourcePath":format!("C:\\source\\{source_name}.flac"),
                "SourceRoot":r"C:\source", "ExternalRoot":root.0.join("missing"),
                "BasenameIndex":lookup["Index"]
            }),
        );
        assert_eq!(
            actual,
            expected
                .map(|p| json!(root.0.join(p).components().collect::<PathBuf>()))
                .unwrap_or(Value::Null)
        );
    }
}

#[test]
fn mlp_import_supplementary_case_matches_dotnet_for_fallback_mirror_and_ambiguity() {
    let root = Scratch::new();
    let pairs = [
        ('\u{10400}', '\u{10428}'),
        ('\u{104b0}', '\u{104d8}'),
        ('\u{10c80}', '\u{10cc0}'),
        ('\u{118a0}', '\u{118c0}'),
        ('\u{16e40}', '\u{16e60}'),
        ('\u{1e900}', '\u{1e922}'),
    ];
    let mirror = root.0.join("album").join("track.mlp");
    fs::create_dir_all(mirror.parent().unwrap()).unwrap();
    fs::write(&mirror, b"mirror").unwrap();
    for (upper, lower) in pairs {
        assert_eq!(
            dispatch(
                "build.mlp_resolve",
                json!({
                    "SourcePath":format!("C:\\source\\{upper}.flac"),
                    "SourceRoot":r"C:\source", "ExternalRoot":root.0.join("missing"),
                    "BasenameIndex":{lower.to_string():"supplementary.mlp"}
                })
            ),
            "supplementary.mlp"
        );
        assert_eq!(
            dispatch(
                "build.mlp_resolve",
                json!({
                    "SourcePath":format!("C:\\{upper}\\album\\track.flac"),
                    "SourceRoot":format!("C:\\{lower}"), "ExternalRoot":root.0,
                    "BasenameIndex":{}
                })
            ),
            json!(mirror)
        );
        for (directory, letter) in [("a", upper), ("b", lower)] {
            let path = root.0.join(directory).join(format!("{letter}.mlp"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"duplicate").unwrap();
        }
    }
    let lookup = dispatch("build.mlp_lookup", json!({"Root":root.0}));
    assert_eq!(
        lookup["AmbiguousNames"],
        json!(pairs.map(|(upper, _)| upper.to_string()))
    );
    assert_eq!(lookup["Index"].as_object().unwrap().len(), 1);
    assert!(lookup["Index"].get("track").is_some());

    let bmp = Scratch::new();
    for (upper, lower) in [('Β', 'ϐ'), ('Μ', 'µ'), ('Σ', 'ς'), ('ᾈ', 'ᾀ')] {
        assert_eq!(
            dispatch(
                "build.mlp_resolve",
                json!({
                    "SourcePath":format!("C:\\source\\{upper}.flac"),
                    "SourceRoot":r"C:\source", "ExternalRoot":bmp.0.join("missing"),
                    "BasenameIndex":{lower.to_string():"bmp-variant.mlp"}
                })
            ),
            "bmp-variant.mlp"
        );
        for (directory, letter) in [("a", upper), ("b", lower)] {
            let path = bmp.0.join(directory).join(format!("{letter}.mlp"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"duplicate").unwrap();
        }
    }
    let lookup = dispatch("build.mlp_lookup", json!({"Root":bmp.0}));
    assert_eq!(lookup["AmbiguousNames"], json!(["Β", "Μ", "Σ", "ᾈ"]));
    assert!(lookup["Index"].as_object().unwrap().is_empty());
    for (left, right) in [("İ", "i"), ("ı", "I"), ("ſ", "S"), ("ẞ", "ß"), ("K", "k")] {
        assert_eq!(
            dispatch(
                "build.mlp_resolve",
                json!({
                    "SourcePath":format!("C:\\source\\{left}.flac"),
                    "SourceRoot":r"C:\source", "ExternalRoot":bmp.0.join("missing"),
                    "BasenameIndex":{right:"must-not-match.mlp"}
                })
            ),
            Value::Null
        );
    }
}
