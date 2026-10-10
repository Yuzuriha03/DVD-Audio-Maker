use dvda_author::{
    Callbacks, Silent,
    aob::{self, TrackInput},
    audio,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rust-dvda-aob-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn wave(
    path: &Path,
    bits: u16,
    container: u16,
    channels: u16,
    samples: u32,
    rf64: bool,
) -> Vec<u8> {
    let rate = 48000u32;
    let width = container / 8;
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&if container != bits { 0xfffeu16 } else { 1 }.to_le_bytes());
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&rate.to_le_bytes());
    fmt.extend_from_slice(&(rate * u32::from(channels * width)).to_le_bytes());
    fmt.extend_from_slice(&(channels * width).to_le_bytes());
    fmt.extend_from_slice(&container.to_le_bytes());
    if container != bits {
        fmt.extend_from_slice(&22u16.to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        fmt.extend_from_slice(&0u32.to_le_bytes());
        fmt.extend_from_slice(&[1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]);
    }
    let mut data = Vec::new();
    for sample in 0..samples {
        for channel in 0..channels {
            let value =
                ((sample * 257 + u32::from(channel) * 4099) << (container - bits)).to_le_bytes();
            data.extend_from_slice(&value[..usize::from(width)]);
        }
    }
    let mut chunks = Vec::new();
    if rf64 {
        chunks.extend_from_slice(b"ds64");
        chunks.extend_from_slice(&28u32.to_le_bytes());
        chunks.extend_from_slice(&0u64.to_le_bytes());
        chunks.extend_from_slice(&(data.len() as u64).to_le_bytes());
        chunks.extend_from_slice(&u64::from(samples).to_le_bytes());
        chunks.extend_from_slice(&0u32.to_le_bytes());
    }
    chunks.extend_from_slice(b"fmt ");
    chunks.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    chunks.extend_from_slice(&fmt);
    chunks.extend_from_slice(b"data");
    chunks.extend_from_slice(&if rf64 { u32::MAX } else { data.len() as u32 }.to_le_bytes());
    chunks.extend_from_slice(&data);
    if data.len() % 2 != 0 {
        chunks.push(0);
    }
    let mut file = Vec::new();
    file.extend_from_slice(if rf64 { b"RF64" } else { b"RIFF" });
    file.extend_from_slice(
        &if rf64 {
            u32::MAX
        } else {
            chunks.len() as u32 + 4
        }
        .to_le_bytes(),
    );
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&chunks);
    if rf64 {
        let size = file.len() as u64 - 8;
        file[20..28].copy_from_slice(&size.to_le_bytes());
    }
    fs::write(path, file).unwrap();
    data
}
fn payload(files: &[PathBuf]) -> Vec<u8> {
    let mut output = Vec::new();
    for file in files {
        let bytes = fs::read(file).unwrap();
        assert_eq!(bytes.len() % 2048, 0);
        for sector in bytes.as_chunks::<2048>().0 {
            assert_eq!(&sector[..4], &[0, 0, 1, 0xba]);
            let pes = if sector[14..18] == [0, 0, 1, 0xbb] {
                32
            } else {
                14
            };
            let private = pes + 9 + usize::from(sector[pes + 8]);
            let begin = private
                + 4
                + usize::from(u16::from_be_bytes(
                    sector[private + 2..private + 4].try_into().unwrap(),
                ));
            let end = pes
                + 6
                + usize::from(u16::from_be_bytes(
                    sector[pes + 4..pes + 6].try_into().unwrap(),
                ));
            assert!(begin <= end && end <= 2048);
            output.extend_from_slice(&sector[begin..end]);
        }
    }
    output
}
#[test]
fn short_first_packet_and_exact_first_capacity_preserve_pcm() {
    let work = Work::new();
    for frames in [1, 3, 495, 496, 497, 500, 501, 996, 1000, 1001] {
        let source = work.0.join(format!("{frames}.wav"));
        let mut expected = wave(&source, 16, 16, 2, frames, false);
        if frames % 2 != 0 {
            expected.extend_from_slice(&[0; 4]);
        }
        let output = aob::author_group(
            &work.0.join(format!("out-{frames}")),
            1,
            &[TrackInput::new(source)],
            &mut Silent,
        )
        .unwrap();
        for word in expected.as_chunks_mut::<2>().0 {
            word.swap(0, 1);
        }
        assert_eq!(payload(&output.aob_paths), expected, "{frames} frames");
        assert_eq!(
            output.tracks.last().unwrap().last_sector + 1,
            output.sectors
        );
    }
}
#[test]
fn odd_frames_carry_within_title_and_pad_at_title_boundaries() {
    let work = Work::new();
    let a = work.0.join("a.wav");
    let b = work.0.join("b.wav");
    let raw_a = wave(&a, 24, 24, 1, 3, false);
    let raw_b = wave(&b, 24, 24, 1, 5, false);
    for new_title in [false, true] {
        let tracks = [
            TrackInput::new(&a),
            TrackInput {
                new_title,
                ..TrackInput::new(&b)
            },
        ];
        let output = aob::author_group(
            &work.0.join(format!("out-{new_title}")),
            1,
            &tracks,
            &mut Silent,
        )
        .unwrap();
        let mut raw = raw_a.clone();
        if new_title {
            raw.extend([0; 3]);
        }
        raw.extend_from_slice(&raw_b);
        if new_title {
            raw.extend([0; 3]);
        }
        let expected: Vec<u8> = raw
            .as_chunks::<6>()
            .0
            .iter()
            .flat_map(|pair| [pair[2], pair[1], pair[5], pair[4], pair[0], pair[3]])
            .collect();
        assert_eq!(payload(&output.aob_paths), expected);
        assert_eq!(
            output.title_starts,
            if new_title { vec![0, 1] } else { vec![0] }
        );
        assert_eq!(
            output.tracks[1].first_pts,
            if new_title {
                0
            } else {
                output.tracks[0].pts_length
            }
        );
    }
}
#[test]
fn rf64_and_valid_bits_in_wide_container_match_native_pcm() {
    let work = Work::new();
    let ordinary = work.0.join("native.wav");
    wave(&ordinary, 24, 24, 2, 4001, false);
    let wide = work.0.join("wide.wav");
    wave(&wide, 24, 32, 2, 4001, true);
    let a = aob::author_group(
        &work.0.join("ordinary"),
        1,
        &[TrackInput::new(ordinary)],
        &mut Silent,
    )
    .unwrap();
    let b = aob::author_group(
        &work.0.join("wide"),
        1,
        &[TrackInput::new(&wide)],
        &mut Silent,
    )
    .unwrap();
    assert_eq!(
        fs::read(&a.aob_paths[0]).unwrap(),
        fs::read(&b.aob_paths[0]).unwrap()
    );
    assert_eq!(audio::probe(&wide).unwrap().samples, 4001);
}
#[test]
fn format_changes_create_titles_and_split_preserves_stream() {
    let work = Work::new();
    let a = work.0.join("a.wav");
    let b = work.0.join("b.wav");
    wave(&a, 16, 16, 2, 6001, false);
    wave(&b, 24, 24, 2, 6000, false);
    let inputs = [TrackInput::new(a), TrackInput::new(b)];
    let whole = aob::author_group(&work.0.join("whole"), 1, &inputs, &mut Silent).unwrap();
    let split =
        aob::author_group_with_split(&work.0.join("split"), 1, &inputs, &mut Silent, 4).unwrap();
    assert_eq!(whole.title_starts, vec![0, 1]);
    assert_eq!(whole.tracks[1].first_pts, 0);
    assert_eq!(
        split
            .aob_paths
            .iter()
            .flat_map(|p| fs::read(p).unwrap())
            .collect::<Vec<_>>(),
        fs::read(&whole.aob_paths[0]).unwrap()
    );
    assert!(split.aob_paths.len() > 1);
    assert!(
        split.aob_paths[..split.aob_paths.len() - 1]
            .iter()
            .all(|p| fs::metadata(p).unwrap().len() == 8192)
    );
    assert_eq!(
        split
            .aob_paths
            .iter()
            .map(|p| fs::metadata(p).unwrap().len() / 2048)
            .sum::<u64>(),
        u64::from(split.sectors)
    );
}
#[test]
fn cancellation_and_changed_input_are_reported() {
    struct Cancel {
        calls: u32,
    }
    impl Callbacks for Cancel {
        fn emit(&mut self, _: i32, _: &str) {}
        fn cancelled(&mut self) -> bool {
            self.calls += 1;
            self.calls > 8
        }
    }
    struct Shrink(PathBuf);
    impl Callbacks for Shrink {
        fn emit(&mut self, _: i32, _: &str) {
            FileForTest::truncate(&self.0);
        }
        fn cancelled(&mut self) -> bool {
            false
        }
    }
    struct FileForTest;
    impl FileForTest {
        fn truncate(path: &Path) {
            std::fs::OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap()
                .set_len(45)
                .unwrap();
        }
    }
    let work = Work::new();
    let source = work.0.join("source.wav");
    wave(&source, 16, 16, 2, 12000, false);
    assert!(
        aob::author_group(
            &work.0.join("cancel"),
            1,
            &[TrackInput::new(&source)],
            &mut Cancel { calls: 0 }
        )
        .unwrap_err()
        .contains("cancelled")
    );
    assert!(
        aob::author_group(
            &work.0.join("changed"),
            1,
            &[TrackInput::new(&source)],
            &mut Shrink(source.clone())
        )
        .unwrap_err()
        .contains("Truncated PCM")
    );
}
#[test]
fn invalid_pcm_is_rejected_before_aob_creation() {
    let work = Work::new();
    let source = work.0.join("source.wav");
    let data = wave(&source, 16, 16, 2, 100, false);
    let mut file = fs::read(&source).unwrap();
    file[32..34].copy_from_slice(&3u16.to_le_bytes());
    fs::write(&source, file).unwrap();
    assert_eq!(data.len(), 400);
    let output = work.0.join("output");
    assert!(aob::author_group(&output, 1, &[TrackInput::new(&source)], &mut Silent).is_err());
    assert!(!output.exists());
}
