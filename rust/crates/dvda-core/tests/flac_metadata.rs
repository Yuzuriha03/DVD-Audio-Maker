//! Former FlacMetadataEditor SpanReader failure semantics at the public boundary.
use dvda_core::formats;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "dvda-flac-metadata-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn stream(blocks: &[(u8, &[u8])], audio: &[u8]) -> Vec<u8> {
    let mut output = b"fLaC".to_vec();
    let mut all = vec![(0, &[0; 34][..])];
    all.extend_from_slice(blocks);
    for (index, (kind, data)) in all.iter().enumerate() {
        output.push(*kind | if index + 1 == all.len() { 0x80 } else { 0 });
        output.extend_from_slice(&(data.len() as u32).to_be_bytes()[1..]);
        output.extend_from_slice(data);
    }
    output.extend_from_slice(audio);
    output
}
fn comments(values: &[&str]) -> Vec<u8> {
    let mut data = 3u32.to_le_bytes().to_vec();
    data.extend_from_slice(b"old");
    data.extend_from_slice(&(values.len() as u32).to_le_bytes());
    for value in values {
        data.extend_from_slice(&(value.len() as u32).to_le_bytes());
        data.extend_from_slice(value.as_bytes());
    }
    data
}
fn picture() -> Vec<u8> {
    let mut data = 3u32.to_be_bytes().to_vec();
    for text in ["image/png", "中文 日本語"] {
        data.extend_from_slice(&(text.len() as u32).to_be_bytes());
        data.extend_from_slice(text.as_bytes());
    }
    for number in [3u32, 2, 24, 0, 4] {
        data.extend_from_slice(&number.to_be_bytes());
    }
    data.extend_from_slice(&[0, 1, 254, 255]);
    data
}
fn query(scratch: &Scratch, kind: u8, payload: &[u8], operation: &str) -> Result<Value, String> {
    let path = scratch.0.join("音源.flac");
    fs::write(&path, stream(&[(kind, payload)], b"audio untouched")).unwrap();
    formats::dispatch(operation, json!(path))
}

#[test]
fn flac_comments_rejects_every_truncation_oversize_count_bad_utf8_and_trailing_bytes() {
    let scratch = Scratch::new();
    let good = comments(&[
        "TITLE=中文 日本語",
        "ARTIST=One",
        "ARTIST=Two",
        "ignored",
        "=ignored",
        "EMPTY=",
    ]);
    let result = query(&scratch, 4, &good, "flac.comments").unwrap();
    assert_eq!(
        result,
        json!([{"Key":"TITLE","Value":"中文 日本語"},{"Key":"ARTIST","Value":"One"},{"Key":"ARTIST","Value":"Two"},{"Key":"EMPTY","Value":""}])
    );
    for end in 0..good.len() {
        assert!(
            query(&scratch, 4, &good[..end], "flac.comments").is_err(),
            "accepted truncated comment payload {end}"
        );
    }
    for offset in [0, 7, 11] {
        let mut wrong = good.clone();
        wrong[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(query(&scratch, 4, &wrong, "flac.comments").is_err());
    }
    let mut wrong = good.clone();
    wrong[15] = 0xff;
    assert!(
        query(&scratch, 4, &wrong, "flac.comments")
            .unwrap_err()
            .contains("UTF-8")
    );
    let mut wrong = good;
    wrong.push(0);
    assert!(
        query(&scratch, 4, &wrong, "flac.comments")
            .unwrap_err()
            .contains("trailing")
    );
}

#[test]
fn flac_picture_rejects_every_truncation_oversize_field_bad_utf8_and_trailing_bytes() {
    let scratch = Scratch::new();
    let good = picture();
    assert_eq!(
        query(&scratch, 6, &good, "flac.picture").unwrap(),
        json!({"Descriptor":{"Type":3,"MimeType":"image/png","Width":3,"Height":2,"Depth":24},"Description":"中文 日本語","Colors":0,"ImageHex":"0001feff"})
    );
    for end in 0..good.len() {
        assert!(
            query(&scratch, 6, &good[..end], "flac.picture").is_err(),
            "accepted truncated picture payload {end}"
        );
    }
    let dimensions = 12 + "image/png".len() + "中文 日本語".len();
    for offset in [
        0,
        4,
        8 + "image/png".len(),
        dimensions,
        dimensions + 4,
        dimensions + 8,
        dimensions + 12,
        dimensions + 16,
    ] {
        let mut wrong = good.clone();
        wrong[offset..offset + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(query(&scratch, 6, &wrong, "flac.picture").is_err());
    }
    for offset in [8, 12 + "image/png".len()] {
        let mut wrong = good.clone();
        wrong[offset] = 0xff;
        assert!(
            query(&scratch, 6, &wrong, "flac.picture")
                .unwrap_err()
                .contains("UTF-8")
        );
    }
    let mut wrong = good;
    wrong.push(0);
    assert!(
        query(&scratch, 6, &wrong, "flac.picture")
            .unwrap_err()
            .contains("trailing")
    );
}

#[test]
fn flac_document_rejects_truncated_block_headers_and_preserves_absent_optional_blocks() {
    let scratch = Scratch::new();
    let path = scratch.0.join("test.flac");
    let good = stream(&[], &[]);
    fs::write(&path, &good).unwrap();
    assert_eq!(
        formats::dispatch("flac.comments", json!(path)).unwrap(),
        json!([])
    );
    assert_eq!(
        formats::dispatch("flac.picture", json!(path)).unwrap(),
        Value::Null
    );
    for end in 0..good.len() {
        fs::write(&path, &good[..end]).unwrap();
        assert!(formats::dispatch("flac.comments", json!(path)).is_err());
        assert!(formats::dispatch("flac.picture", json!(path)).is_err());
    }
}

#[test]
fn flac_picture_replacement_preserves_audio_and_other_blocks_and_cleans_failed_commit() {
    let scratch = Scratch::new();
    let path = scratch.0.join("source.flac");
    let image = scratch.0.join("new.png");
    let old_picture = picture();
    let comment = comments(&["TITLE=unchanged"]);
    let audio: Vec<_> = (0..65539).map(|n| (n * 37 % 256) as u8).collect();
    let original = stream(
        &[
            (4, &comment),
            (6, &old_picture),
            (1, &[9, 8, 7]),
            (6, &old_picture),
        ],
        &audio,
    );
    fs::write(&path, &original).unwrap();
    fs::write(&image, [9, 8, 7, 6, 5]).unwrap();
    let mut template = formats::dispatch("flac.picture", json!(path)).unwrap();
    template.as_object_mut().unwrap().remove("ImageHex");
    let request = json!({"Path":path,"ImagePath":image,"Template":template});
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let guard = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        assert!(formats::dispatch("flac.replace_picture", request.clone()).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(
            fs::read_dir(&scratch.0).unwrap().count(),
            2,
            "failed publication left a temporary file"
        );
        drop(guard);
    }
    formats::dispatch("flac.replace_picture", request).unwrap();
    let after = fs::read(&path).unwrap();
    assert!(after.ends_with(&audio));
    assert_eq!(
        formats::dispatch("flac.comments", json!(path)).unwrap(),
        json!([{"Key":"TITLE","Value":"unchanged"}])
    );
    assert_eq!(
        formats::dispatch("flac.picture", json!(path)).unwrap()["ImageHex"],
        "0908070605"
    );
    let mut offset = 4;
    let mut kinds = Vec::new();
    loop {
        let header = after[offset];
        let length = (usize::from(after[offset + 1]) << 16)
            | (usize::from(after[offset + 2]) << 8)
            | usize::from(after[offset + 3]);
        kinds.push(header & 127);
        if header & 127 == 1 {
            assert_eq!(&after[offset + 4..offset + 4 + length], &[9, 8, 7]);
        }
        offset += 4 + length;
        if header & 128 != 0 {
            break;
        }
    }
    assert_eq!(kinds, [0, 4, 6, 1]);
    assert_eq!(&after[offset..], &audio);
    assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 2);
}
