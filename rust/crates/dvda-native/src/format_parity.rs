//! Rust format behavior regression tests.
use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(
                "dvda-format-parity-中文-日本語-{}-{}",
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

fn fixture_crc(data: &[u8]) -> u16 {
    mlp_formats::checksum16(data)
}
fn fixture(major: bool, eos: bool) -> Vec<u8> {
    let major_length = if major { 28 } else { 0 };
    let length = 4 + major_length + 2 + 6;
    let mut data = vec![0; length];
    if major {
        data[4..7].copy_from_slice(&[0xf8, 0x72, 0x6f]);
        data[12..14].copy_from_slice(&0xb752u16.to_be_bytes());
        data[18..20].copy_from_slice(&3200u16.to_be_bytes());
        data[20] = 1;
        let crc = fixture_crc(&data[4..30]);
        data[30..32].copy_from_slice(&crc.to_le_bytes());
    }
    let substream = 4 + major_length;
    data[substream..substream + 2].copy_from_slice(&3u16.to_be_bytes());
    data[substream + 2..substream + 6].copy_from_slice(if eos {
        &[0xd2, 0x34, 0xd2, 0x34]
    } else {
        &[0x10, 0x20, 0x30, 0x40]
    });
    let words = length / 2;
    let mut parity = words ^ 3;
    parity ^= parity >> 8;
    parity ^= parity >> 4;
    let header = u16::try_from((((parity & 15) ^ 15) << 12) | words).unwrap();
    data[..2].copy_from_slice(&header.to_be_bytes());
    data
}

#[test]
fn native_format_checksum_parity_peak_rate_and_pts_match_historical_constants() {
    let native = NativeFormats::load().unwrap();
    assert_eq!(fixture_crc(&[0xf8, 0x72, 0x6f, 0xbb, 0, 0]), 0x8699);
    assert_eq!(mlp_formats::checksum8(&[1, 2, 3, 0]), 0xb0);
    for value in [0_i64, 1, 90000, 0x1_ffff_ffff] {
        let encoded = [
            0x21 | ((value >> 30) & 7).to_le_bytes()[0] << 1,
            (value >> 22).to_le_bytes()[0],
            ((value >> 15) & 0x7f).to_le_bytes()[0] << 1 | 1,
            (value >> 7).to_le_bytes()[0],
            (value & 0x7f).to_le_bytes()[0] << 1 | 1,
        ];
        assert_eq!(native.parse_pts(&encoded).unwrap(), value);
        for length in 0..5 {
            assert!(native.parse_pts(&encoded[..length]).is_err());
        }
    }
}
#[test]
fn native_format_mlp_stream_buffer_damage_and_alignment_match_historical_cases() {
    let native = NativeFormats::load().unwrap();
    let root = Scratch::new();
    let path = root.0.join("样本.mlp");
    let inspect = |data: &[u8]| {
        fs::write(&path, data).unwrap();
        let file = native.inspect_file(&path);
        let buffer = mlp_formats::inspect_buffer(data);
        assert_eq!(file.is_ok(), buffer.is_ok());
        if let (Ok(file), Ok(buffer)) = (&file, &buffer) {
            assert_eq!(format!("{file:?}"), format!("{buffer:?}"));
        }
        file
    };
    let valid = fixture(true, true);
    assert!(inspect(&valid).unwrap().is_valid);
    assert_eq!(native.align(&valid).unwrap().data, valid);
    let no_major = inspect(&fixture(false, true)).unwrap();
    assert_eq!(no_major.major_sync_count, 0);
    assert!(!no_major.is_valid);
    let mut truncated = vec![0; 12];
    truncated[1] = 6;
    truncated[4..7].copy_from_slice(&[0xf8, 0x72, 0x6f]);
    assert!(inspect(&truncated).unwrap().major_sync_error_count > 0);
    let mut fields = valid.clone();
    fields[18..20].copy_from_slice(&1u16.to_be_bytes());
    fields[20] = 2;
    fields[30] ^= 1;
    let bad = inspect(&fields).unwrap();
    assert!(bad.major_sync_error_count >= 3);
    assert!(!bad.is_valid);
    let fixed = native.align(&fields).unwrap();
    assert_eq!(fixed.peak_changes, 1);
    assert_eq!(fixed.extended_changes, 1);
    assert_eq!(fixed.checksum_changes, 1);
    assert_eq!(fixed.data, valid);
    let mut parity = valid.clone();
    parity[0] ^= 0x10;
    assert_eq!(inspect(&parity).unwrap().access_unit_parity_error_count, 1);
    let mut crc = valid.clone();
    crc[30] ^= 1;
    assert!(inspect(&crc).unwrap().major_sync_error_count > 0);
    let mut sub = valid.clone();
    sub[32..34].copy_from_slice(&0xfffu16.to_be_bytes());
    assert!(inspect(&sub).unwrap().substream_error_count > 0);
    let no_eos = fixture(true, false);
    assert!(!inspect(&no_eos).unwrap().has_end_of_stream);
    let aligned = native.align(&no_eos).unwrap();
    assert!(aligned.inserted_end_of_stream);
    assert_eq!(aligned.data.len(), no_eos.len() + 4);
    assert!(inspect(&aligned.data).unwrap().is_valid);
    let again = native.align(&aligned.data).unwrap();
    assert!(!again.inserted_end_of_stream);
    assert_eq!(again.data, aligned.data);
    for bytes in [
        &[0, 3, 0, 0][..],
        &[0][..],
        &[0, 1, 0, 0][..],
        &valid[..valid.len() - 1],
    ] {
        assert!(inspect(bytes).is_err());
        assert!(native.align(bytes).is_err());
    }
    let mut stream = Vec::new();
    for index in 0..16 {
        stream.extend(fixture(index % 8 == 0, index == 15));
    }
    let result = inspect(&stream).unwrap();
    assert!(result.is_valid);
    assert_eq!(result.access_unit_count, 16);
    assert_eq!(result.major_sync_count, 2);
    assert_eq!(result.major_sync_interval, 8.0);
    let simple_walk = [0xf0, 2, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0];
    assert_eq!(inspect(&simple_walk).unwrap().access_unit_count, 2);
}

#[test]
fn native_format_pcm_strict_zero_tail_and_chunk_mismatch_match_historical_cases() {
    let native = NativeFormats::load().unwrap();
    let root = Scratch::new();
    let source = root.0.join("source.raw");
    let decoded = root.0.join("decoded.raw");
    let bytes = [1, 2, 3, 4, 5, 6, 7, 8];
    fs::write(&source, bytes).unwrap();
    for (contents, frame, max, expected, reason, offset) in [
        (bytes.to_vec(), 0, 0, true, 0, 0),
        (vec![1, 2, 3, 4, 5, 6, 7, 8, 9], 0, 0, false, 1, 0),
        (vec![1, 2, 3, 4, 5, 6, 7], 0, 0, false, 1, 0),
        (vec![1, 2, 3, 4, 5, 6, 7, 9], 0, 0, false, 2, 7),
        (vec![1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0], 0, 0, false, 1, 0),
        (vec![1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0], 2, 2, true, 0, 0),
        (vec![1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 1, 0], 2, 2, false, 3, 10),
        (
            vec![1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 0, 0],
            2,
            2,
            false,
            1,
            0,
        ),
        (vec![1, 2, 3, 4, 5, 6, 7, 9, 0, 0, 0, 0], 2, 2, false, 2, 7),
        (vec![1, 2, 3, 4, 5, 6, 7, 8, 0], 2, 2, false, 1, 0),
        (vec![1, 2, 3, 4, 5, 6, 7], 2, 2, false, 1, 0),
    ] {
        fs::write(&decoded, &contents).unwrap();
        let result = native.compare_pcm(&source, &decoded, frame, max).unwrap();
        assert_eq!(result.matches, expected);
        assert_eq!(result.reason_code, reason);
        assert_eq!(result.source_bytes, 8);
        assert_eq!(result.decoded_bytes, contents.len() as u64);
        assert_eq!(result.first_mismatch_offset, offset);
        assert_eq!(
            result.trailing_zero_bytes,
            if expected && contents.len() > 8 {
                (contents.len() - 8) as u64
            } else {
                0
            }
        );
    }
    fs::write(&source, [1, 2, 3, 4, 5, 6]).unwrap();
    for (frames, expected) in [(29, true), (47, true), (48, false)] {
        let mut padded = vec![1, 2, 3, 4, 5, 6];
        padded.resize(6 + frames * 6, 0);
        fs::write(&decoded, padded).unwrap();
        let result = native.compare_pcm(&source, &decoded, 6, 47).unwrap();
        assert_eq!(result.matches, expected);
        if expected {
            assert_eq!(result.trailing_zero_bytes, (frames * 6) as u64);
        }
    }
    let mut large = vec![0; 300 * 1024 + 1];
    fs::write(&source, &large).unwrap();
    for offset in [0, 1, 131_071, 131_072, 131_073, 200_000, large.len() - 1] {
        large[offset] = 255;
        fs::write(&decoded, &large).unwrap();
        let result = native.compare_pcm(&source, &decoded, 0, 0).unwrap();
        assert!(!result.matches);
        assert_eq!(result.reason_code, 2);
        assert_eq!(result.first_mismatch_offset, offset as u64);
        large[offset] = 0;
    }
    fs::write(&decoded, &large).unwrap();
    assert!(native.compare_pcm(&source, &decoded, 0, 0).unwrap().matches);
    fs::write(&source, []).unwrap();
    fs::write(&decoded, []).unwrap();
    assert!(native.compare_pcm(&source, &decoded, 0, 0).unwrap().matches);
    assert!(native.compare_pcm(&source, &decoded, 0, 1).is_err());
    assert!(
        native
            .compare_pcm(&root.0.join("missing.raw"), &decoded, 0, 0)
            .is_err()
    );
}

#[test]
#[ignore = "manual release PCM comparison benchmark"]
fn pcm_comparison_throughput() {
    let root = Scratch::new();
    let source = root.0.join("source.raw");
    let decoded = root.0.join("decoded.raw");
    let bytes: Vec<u8> = (0_i32..=(64 * 1024 * 1024))
        .map(|n| n.to_le_bytes()[0])
        .collect();
    fs::write(&source, &bytes).unwrap();
    fs::write(&decoded, &bytes).unwrap();
    for round in 0..9 {
        let start = std::time::Instant::now();
        let result = NativeFormats.compare_pcm(&source, &decoded, 0, 0).unwrap();
        let elapsed = start.elapsed();
        assert!(result.matches);
        assert_eq!(result.reason_code, 0);
        assert_eq!(result.source_bytes, bytes.len() as u64);
        assert_eq!(result.decoded_bytes, bytes.len() as u64);
        println!(
            "COMPARE_BENCH round={round} ms={:.3}",
            elapsed.as_secs_f64() * 1000.0
        );
    }
}
