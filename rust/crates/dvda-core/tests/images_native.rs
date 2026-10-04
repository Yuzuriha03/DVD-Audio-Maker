//! Native image behavior independent of the temporary managed comparison runner.
use dvda_core::images::{Job, Output, execute};
use dvda_native::{images::Images, media::Callbacks};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Default)]
struct Events {
    output: String,
    error: String,
    cancelled: bool,
}
impl Callbacks for Events {
    fn emit(&mut self, stream: i32, text: &str) {
        if stream == 2 {
            self.error.push_str(text);
        } else {
            self.output.push_str(text);
        }
    }
    fn cancelled(&mut self) -> bool {
        self.cancelled
    }
}
fn library() -> PathBuf {
    PathBuf::from(std::env::var_os("DVDA_IMAGE_NATIVE_DIR").expect("Set DVDA_IMAGE_NATIVE_DIR"))
        .join("dvda-image.dll")
}
fn job(args: &[&str], output: Option<&Path>) -> Job {
    let mut arguments: Vec<String> = args.iter().map(|s| (*s).into()).collect();
    let output = output.map(|path| {
        let index = arguments.len();
        arguments.push(path.to_string_lossy().into_owned());
        Output {
            path: path.into(),
            argument_index: index,
            format_prefix: "PNG32:".into(),
        }
    });
    Job {
        library: library(),
        arguments,
        output,
        timeout_millis: None,
    }
}

#[test]
#[ignore = "requires the source-built Windows x64 image DLL"]
fn pixels_diagnostics_concurrency_and_failure_protection() {
    let root = std::env::temp_dir().join(format!(
        "dvda-rust-image-中文-日本語-🎵-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    struct Clean(PathBuf);
    impl Drop for Clean {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let _clean = Clean(root.clone());
    let path = root.join("pixels.png");
    let images = Images::load(&library()).unwrap();
    for (color, pixel) in [
        ("red", [255, 0, 0, 255]),
        ("none", [0, 0, 0, 0]),
        ("black", [0, 0, 0, 255]),
    ] {
        let result = execute(
            job(
                &[
                    "convert",
                    "-size",
                    "17x11",
                    &format!("xc:{color}"),
                    "-depth",
                    "8",
                ],
                Some(&path),
            ),
            &mut Events::default(),
        );
        assert!(result.failure.is_none(), "{:?}", result.failure);
        assert_eq!(result.exit_code, Some(0));
        let (width, height, actual) = images.read_rgba(&path).unwrap();
        assert_eq!((width, height), (17, 11));
        assert!(
            actual.as_chunks::<4>().0.iter().all(|p| *p == pixel),
            "Wrong {color} pixels"
        );
    }
    let mut events = Events::default();
    let result = execute(
        job(
            &["identify", "-format", "%w|%h", path.to_str().unwrap()],
            None,
        ),
        &mut events,
    );
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(events.output.trim(), "17|11");
    fs::write(&path, b"preserve").unwrap();
    let result = execute(
        job(
            &["convert", root.join("missing.png").to_str().unwrap()],
            Some(&path),
        ),
        &mut Events::default(),
    );
    assert!(result.exit_code.is_some_and(|c| c != 0));
    assert_eq!(fs::read(&path).unwrap(), b"preserve");
    let result = execute(
        job(&["convert", "-size", "17x11", "xc:red"], Some(&path)),
        &mut Events {
            cancelled: true,
            ..Default::default()
        },
    );
    assert_eq!(result.failure.unwrap().kind, "Cancelled");
    let mut timed = job(
        &[
            "convert",
            "-size",
            "4096x4096",
            "gradient:",
            "-blur",
            "0x100",
        ],
        Some(&path),
    );
    timed.timeout_millis = Some(50);
    assert_eq!(
        execute(timed, &mut Events::default()).failure.unwrap().kind,
        "Timeout"
    );
    assert_eq!(fs::read(&path).unwrap(), b"preserve");
    let threads: Vec<_> = (1..=4)
        .map(|i| {
            std::thread::spawn(move || {
                let mut events = Events::default();
                let result = execute(
                    job(
                        &[
                            "convert",
                            "-size",
                            &format!("{i}x{i}"),
                            "xc:black",
                            "-format",
                            "%w",
                            "info:",
                        ],
                        None,
                    ),
                    &mut events,
                );
                assert_eq!(result.exit_code, Some(0));
                assert_eq!(events.output.trim(), i.to_string());
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(
        fs::read_dir(&root).unwrap().count(),
        1,
        "Leaked temporary image"
    );
}
