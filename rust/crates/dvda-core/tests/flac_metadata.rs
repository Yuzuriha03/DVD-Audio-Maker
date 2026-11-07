//! Former `FlacMetadataEditor` `SpanReader` failure semantics at the public boundary.
use dvda_core::formats;
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Seek, SeekFrom},
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
        output.extend_from_slice(&(u32::try_from(data.len()).unwrap()).to_be_bytes()[1..]);
        output.extend_from_slice(data);
    }
    output.extend_from_slice(audio);
    output
}
fn comments(values: &[&str]) -> Vec<u8> {
    let mut data = 3u32.to_le_bytes().to_vec();
    data.extend_from_slice(b"old");
    data.extend_from_slice(&(u32::try_from(values.len()).unwrap()).to_le_bytes());
    for value in values {
        data.extend_from_slice(&(u32::try_from(value.len()).unwrap()).to_le_bytes());
        data.extend_from_slice(value.as_bytes());
    }
    data
}
fn picture() -> Vec<u8> {
    let mut data = 3u32.to_be_bytes().to_vec();
    for text in ["image/png", "中文 日本語"] {
        data.extend_from_slice(&(u32::try_from(text.len()).unwrap()).to_be_bytes());
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

fn block_header(kind: u8, length: u32, last: bool) -> [u8; 4] {
    let bytes = length.to_be_bytes();
    [
        kind | if last { 0x80 } else { 0 },
        bytes[1],
        bytes[2],
        bytes[3],
    ]
}

fn len_u64(bytes: &[u8]) -> u64 {
    u64::try_from(bytes.len()).expect("test payload length fits u64")
}

/// Counts bytes actually read while forwarding seeks untouched, so a test can
/// prove the parser never pulls the audio region into memory.
struct CountingReader<R> {
    inner: R,
    bytes_read: u64,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.bytes_read += u64::try_from(read).expect("read length fits u64");
        Ok(read)
    }
}

impl<R: Seek> Seek for CountingReader<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
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
fn flac_picture_raw_data_borrows_image_without_changing_json_api() {
    let scratch = Scratch::new();
    let path = scratch.0.join("raw.flac");
    fs::write(&path, stream(&[(6, &picture())], b"audio")).unwrap();
    let data = formats::flac_picture_data(&path).unwrap();
    assert_eq!(data.image(), [0, 1, 254, 255]);
    assert_eq!(
        data.metadata,
        json!({"Descriptor":{"Type":3,"MimeType":"image/png","Width":3,"Height":2,"Depth":24},"Description":"中文 日本語","Colors":0})
    );
    assert_eq!(
        formats::dispatch("flac.picture", json!(path)).unwrap()["ImageHex"],
        "0001feff"
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
fn flac_metadata_queries_read_only_metadata_before_a_large_audio_region() {
    const AUDIO_BYTES: u64 = 8 * 1024 * 1024;
    let scratch = Scratch::new();
    let path = scratch.0.join("large-audio.flac");
    let comment = comments(&["TITLE=streamed"]);
    let picture = picture();
    let metadata = stream(&[(4, &comment), (6, &picture)], &[]);
    fs::write(&path, &metadata).unwrap();
    // Extend the file with a large audio region without materializing it.
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(len_u64(&metadata) + AUDIO_BYTES).unwrap();
    drop(file);
    let total = fs::metadata(&path).unwrap().len();
    assert_eq!(total, len_u64(&metadata) + AUDIO_BYTES);

    let mut file = fs::File::open(&path).unwrap();
    let mut counting = CountingReader {
        inner: &mut file,
        bytes_read: 0,
    };
    let value = formats::flac_comments_reader(&mut counting, total).unwrap();
    assert_eq!(value, json!([{"Key":"TITLE","Value":"streamed"}]));
    assert_eq!(counting.bytes_read, 16 + len_u64(&comment));
    assert!(counting.bytes_read < len_u64(&metadata));

    let mut file = fs::File::open(&path).unwrap();
    let mut counting = CountingReader {
        inner: &mut file,
        bytes_read: 0,
    };
    let data = formats::flac_picture_data_reader(&mut counting, total).unwrap();
    assert_eq!(data.image(), [0, 1, 254, 255]);
    assert_eq!(counting.bytes_read, 16 + len_u64(&picture));
    assert!(counting.bytes_read < len_u64(&metadata));

    // The public operations still return the unchanged JSON shape on this file.
    assert_eq!(
        formats::dispatch("flac.comments", json!(path)).unwrap(),
        json!([{"Key":"TITLE","Value":"streamed"}])
    );
    assert_eq!(
        formats::dispatch("flac.picture", json!(path)).unwrap()["ImageHex"],
        "0001feff"
    );
}

#[test]
fn flac_picture_reader_skips_large_unneeded_metadata_blocks() {
    let scratch = Scratch::new();
    let path = scratch.0.join("padded.flac");
    let comment = comments(&["TITLE=skip"]);
    let picture = picture();
    let padding = vec![0u8; 4 * 1024 * 1024];
    let metadata = stream(&[(4, &comment), (1, &padding), (6, &picture)], &[]);
    fs::write(&path, &metadata).unwrap();
    let total = fs::metadata(&path).unwrap().len();

    let mut file = fs::File::open(&path).unwrap();
    let mut counting = CountingReader {
        inner: &mut file,
        bytes_read: 0,
    };
    let data = formats::flac_picture_data_reader(&mut counting, total).unwrap();
    assert_eq!(data.image(), [0, 1, 254, 255]);
    // 4 magic bytes + four headers + only the wanted PICTURE body.
    assert_eq!(counting.bytes_read, 20 + len_u64(&picture));
    assert!(counting.bytes_read < len_u64(&padding) / 1000);
}

#[test]
fn flac_metadata_reports_block_truncation_before_streaminfo_validation() {
    let scratch = Scratch::new();
    let path = scratch.0.join("priority.flac");
    let mut bytes = b"fLaC".to_vec();
    // A non-STREAMINFO first block followed by a header whose body lies past EOF.
    bytes.extend_from_slice(&block_header(1, 0, false));
    bytes.extend_from_slice(&block_header(1, 0x00ff_ffff, false));
    fs::write(&path, &bytes).unwrap();
    for operation in ["flac.comments", "flac.picture"] {
        let error = formats::dispatch(operation, json!(path)).unwrap_err();
        assert!(error.contains("truncated"), "{operation}: {error}");
        assert!(!error.contains("STREAMINFO"), "{operation}: {error}");
    }
}

#[test]
fn flac_metadata_operations_close_the_file_handle_before_returning() {
    let scratch = Scratch::new();
    let path = scratch.0.join("handles.flac");
    fs::write(
        &path,
        stream(
            &[(4, &comments(&["TITLE=closed"])), (6, &picture())],
            b"audio",
        ),
    )
    .unwrap();
    formats::dispatch("flac.comments", json!(path)).unwrap();
    formats::dispatch("flac.picture", json!(path)).unwrap();
    let _ = formats::flac_picture_data(&path).unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // An exclusive open only succeeds when no reader handle outlived the call.
        let exclusive = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        drop(exclusive);
    }
    fs::remove_file(&path).unwrap();
    assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 0);
}

#[test]
fn flac_picture_replacement_preserves_audio_and_other_blocks_and_cleans_failed_commit() {
    let scratch = Scratch::new();
    let path = scratch.0.join("source.flac");
    let image = scratch.0.join("new.png");
    let old_picture = picture();
    let comment = comments(&["TITLE=unchanged"]);
    let audio: Vec<_> = (0..65_539)
        .map(|n| u8::try_from(n * 37 % 256).unwrap())
        .collect();
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

#[test]
#[ignore = "deterministic throughput benchmark"]
fn flac_picture_raw_data_throughput() {
    use std::fmt::Write as _;
    use std::hint::black_box;
    use std::time::Instant;

    let scratch = Scratch::new();
    let path = scratch.0.join("large-cover.flac");
    let image: Vec<u8> = (0..1024 * 1024)
        .map(|index| u8::try_from(index * 37 % 256).unwrap())
        .collect();
    let template = picture();
    let mut payload = template[..template.len() - 4].to_vec();
    let length_offset = payload.len() - 4;
    payload[length_offset..].copy_from_slice(&(u32::try_from(image.len()).unwrap()).to_be_bytes());
    payload.extend_from_slice(&image);
    fs::write(&path, stream(&[(6, &payload)], b"audio")).unwrap();
    let expected_hex = image.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    });
    let mut hex_ns = 0;
    let mut raw_ns = 0;
    let mut match_count = 0;
    for _ in 0..10 {
        let start = Instant::now();
        let picture = formats::dispatch("flac.picture", json!(black_box(&path))).unwrap();
        let matched = black_box(
            picture["ImageHex"]
                .as_str()
                .is_some_and(|hex| hex.eq_ignore_ascii_case(black_box(&expected_hex))),
        );
        hex_ns += start.elapsed().as_nanos();
        match_count += usize::from(matched);

        let start = Instant::now();
        let data = formats::flac_picture_data(black_box(&path)).unwrap();
        let matched = black_box(data.image() == black_box(image.as_slice()));
        raw_ns += start.elapsed().as_nanos();
        match_count += usize::from(matched);
    }
    println!("[METRIC] flac_picture_hex_ns={hex_ns} matches={match_count}");
    println!("[METRIC] flac_picture_raw_ns={raw_ns} matches={match_count}");
    assert_eq!(match_count, 20);
}
