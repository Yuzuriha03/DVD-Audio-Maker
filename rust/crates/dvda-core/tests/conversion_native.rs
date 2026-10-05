//! Standalone conversion parity, including metadata, cover, failure and cancellation.
use dvda_core::{
    conversion::{self, Job},
    formats,
};
use dvda_native::media::Callbacks;
use serde_json::json;
use std::{
    fs,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);
#[derive(Default)]
struct Events {
    cancel: bool,
    panic: bool,
}
impl Callbacks for Events {
    fn emit(&mut self, _: i32, _: &str) {
        assert!(!self.panic, "callback fixture");
    }
    fn cancelled(&mut self) -> bool {
        self.cancel
    }
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "dvda-convert-中文-日本語-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn job(root: &Path) -> Job {
    Job {
        library: PathBuf::from(std::env::var_os("DVDA_MEDIA_NATIVE_DIR").expect("media runtime"))
            .join("dvda-media.dll"),
        paths: vec![root.to_owned()],
        compression: 8,
        jobs: 2,
        dry_run: false,
        delete_sources: false,
    }
}
fn atom(name: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut result = ((data.len() + 8) as u32).to_be_bytes().to_vec();
    result.extend_from_slice(name);
    result.extend_from_slice(data);
    result
}
fn tag(name: &[u8; 4], data: &[u8], kind: u32) -> Vec<u8> {
    let mut value = kind.to_be_bytes().to_vec();
    value.extend_from_slice(&[0; 4]);
    value.extend_from_slice(data);
    atom(name, &atom(b"data", &value))
}
fn tagged() -> Vec<u8> {
    // Inject iTunes metadata into a tail moov; existing audio chunk offsets stay valid.
    let input = include_bytes!("fixtures/alac-valid.m4a");
    let png = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2,
        0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 8, 215, 99, 248, 207, 192, 0, 0,
        3, 1, 1, 0, 24, 221, 141, 176, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let mut tags = tag(b"\xa9nam", "  标题\n日本語  ".as_bytes(), 1);
    tags.extend(tag(b"\xa9alb", "专辑".as_bytes(), 1));
    tags.extend(tag(b"covr", &png, 14));
    let mut metadata = vec![0; 4];
    metadata.extend(atom(
        b"hdlr",
        &[
            0, 0, 0, 0, 0, 0, 0, 0, b'm', b'd', b'i', b'r', b'a', b'p', b'p', b'l', 0, 0, 0, 0, 0,
            0, 0, 0, 0,
        ],
    ));
    metadata.extend(atom(b"ilst", &tags));
    let mut output = vec![];
    let mut offset = 0;
    while offset < input.len() {
        let size = u32::from_be_bytes(input[offset..offset + 4].try_into().unwrap()) as usize;
        if &input[offset + 4..offset + 8] == b"moov" {
            let mut body = vec![];
            let mut child = offset + 8;
            while child < offset + size {
                let n = u32::from_be_bytes(input[child..child + 4].try_into().unwrap()) as usize;
                if &input[child + 4..child + 8] != b"udta" {
                    body.extend_from_slice(&input[child..child + n]);
                }
                child += n;
            }
            body.extend(atom(b"udta", &atom(b"meta", &metadata)));
            output.extend(atom(b"moov", &body));
        } else {
            output.extend_from_slice(&input[offset..offset + size]);
        }
        offset += size;
    }
    output
}
#[test]
#[ignore = "requires source-built media and C17 formats"]
fn converter_preserves_tags_cover_pcm_and_batch_failure_semantics() {
    let root = Scratch::new();
    let source = root.0.join("cover.m4a");
    let bytes = tagged();
    fs::write(&source, &bytes).unwrap();
    let broken = root.0.join("broken.m4a");
    fs::write(&broken, include_bytes!("fixtures/alac-broken.m4a")).unwrap();
    let mut task = job(&root.0);
    task.dry_run = true;
    let dry = conversion::execute(&task, &mut Events::default()).unwrap();
    assert!(dry.iter().all(|r| r.status == "DRY"));
    assert!(!source.with_extension("flac").exists());
    assert_eq!(fs::read(&source).unwrap(), bytes);
    task.dry_run = false;
    let results = conversion::execute(&task, &mut Events::default()).unwrap();
    assert!(results.iter().all(|r| r.status == "OK"), "{results:?}");
    let covered = results
        .iter()
        .find(|r| r.source_path.ends_with("cover.m4a"))
        .unwrap();
    assert!(covered.has_cover && covered.cover_exact);
    assert!(covered.missing_tags.is_empty());
    assert_eq!(covered.picture.as_ref().unwrap()["Type"], 3);
    let comments =
        formats::dispatch("flac.comments", json!(source.with_extension("flac"))).unwrap();
    assert!(
        comments
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["Key"] == "TITLE" && t["Value"] == "  标题\n日本語  ")
    );
    assert_eq!(
        results
            .iter()
            .find(|r| r.source_path.ends_with("broken.m4a"))
            .unwrap()
            .repaired_frames,
        2
    );
    // A malformed and a non-ALAC source cannot prevent another source from finishing.
    fs::write(root.0.join("invalid.m4a"), b"not audio").unwrap();
    fs::copy(source.with_extension("flac"), root.0.join("non-alac.m4a")).unwrap();
    task.delete_sources = true;
    let mixed = conversion::execute(&task, &mut Events::default()).unwrap();
    assert_eq!(mixed.iter().filter(|r| r.status == "OK").count(), 2);
    assert_eq!(mixed.iter().filter(|r| r.status == "FAIL").count(), 2);
    assert!(source.exists() && broken.exists());
    // A locked old output is preserved when the final replacement fails.
    let destination = source.with_extension("flac");
    let before = fs::read(&destination).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&destination)
        .unwrap();
    task.paths = vec![source.clone()];
    assert_eq!(
        conversion::execute(&task, &mut Events::default()).unwrap()[0].status,
        "FAIL"
    );
    drop(lock);
    assert_eq!(fs::read(&destination).unwrap(), before);
    assert!(source.exists());
    assert!(
        conversion::execute(
            &task,
            &mut Events {
                cancel: true,
                panic: false
            }
        )
        .is_err()
    );
    assert!(
        conversion::execute(
            &task,
            &mut Events {
                cancel: false,
                panic: true
            }
        )
        .is_err()
    );
    assert!(source.exists());
    // All conversions successful: one deletion failure does not stop later deletion.
    task.paths = vec![source.clone(), broken.clone()];
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&broken)
        .unwrap();
    let deleted = conversion::execute(&task, &mut Events::default()).unwrap();
    assert!(deleted.iter().all(|r| r.status == "OK"), "{deleted:?}");
    assert_eq!(
        deleted
            .iter()
            .filter(|r| r.source_delete_error.is_some())
            .count(),
        1
    );
    assert!(!source.exists() && broken.exists());
    drop(lock);
    assert!(
        fs::read_dir(&root.0)
            .unwrap()
            .all(|e| !e.unwrap().path().is_dir())
    );
}
