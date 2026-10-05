//! Frozen compatibility cases from the removed C# test runner, using C17 exports.
use super::*;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
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

type Inspect = unsafe extern "C" fn(*const u8, usize, *mut RawInspection) -> i32;
type Checksum16 = unsafe extern "C" fn(*const u8, usize, *mut u16) -> i32;
type Checksum8 = unsafe extern "C" fn(*const u8, usize, *mut u8) -> i32;
type Parity = unsafe extern "C" fn(*const u8, usize) -> u8;
type Peak = unsafe extern "C" fn(i32) -> i32;
type Rate = unsafe extern "C" fn(*const u8, usize, *mut i32) -> i32;

fn checksum16(native: &NativeFormats, data: &[u8]) -> u16 {
    let mut value = 0;
    // SAFETY: Exact header signature, live DLL, slice and output cover call.
    unsafe {
        let function: Checksum16 = export(native.module, c"dvda_formats_checksum16").unwrap();
        assert_eq!(function(data.as_ptr(), data.len(), &mut value), 0);
    }
    value
}

fn fixture(native: &NativeFormats, major: bool, eos: bool) -> Vec<u8> {
    let major_length = if major { 28 } else { 0 };
    let length = 4 + major_length + 2 + 6;
    let mut data = vec![0; length];
    if major {
        data[4..7].copy_from_slice(&[0xf8, 0x72, 0x6f]);
        data[12..14].copy_from_slice(&0xb752u16.to_be_bytes());
        data[18..20].copy_from_slice(&3200u16.to_be_bytes());
        data[20] = 1;
        let crc = checksum16(native, &data[4..30]);
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
    let header = ((((parity & 15) ^ 15) << 12) | words) as u16;
    data[..2].copy_from_slice(&header.to_be_bytes());
    data
}

#[test]
#[ignore = "requires source-built x64 dvda-formats.dll"]
fn native_format_checksum_parity_peak_rate_and_pts_match_historical_constants() {
    let native = NativeFormats::load().unwrap();
    assert_eq!(checksum16(&native, &[0xf8, 0x72, 0x6f, 0xbb, 0, 0]), 0x8699);
    // SAFETY: These exact C17 signatures are declared in dvda-formats.h;
    // native owns the module until all synchronous calls finish.
    unsafe {
        let checksum: Checksum8 = export(native.module, c"dvda_formats_checksum8").unwrap();
        let mut crc = 0;
        assert_eq!(checksum([1, 2, 3, 0].as_ptr(), 4, &mut crc), 0);
        assert_eq!(crc, 0xb0);
        assert_ne!(checksum([0].as_ptr(), 0, &mut crc), 0);
        let parity: Parity = export(native.module, c"dvda_formats_calculate_parity").unwrap();
        assert_eq!(parity([0x10, 0x20, 0x30, 0x40].as_ptr(), 4), 0x40);
        let peak: Peak = export(native.module, c"dvda_formats_peak_bitrate_raw").unwrap();
        assert_eq!(peak(48000), 3200);
        assert_eq!(peak(44100), 3483);
        let rate: Rate = export(native.module, c"dvda_formats_sample_rate").unwrap();
        for (bits, expected) in [
            (0, 48000),
            (1, 96000),
            (2, 192000),
            (8, 44100),
            (9, 88200),
            (10, 176400),
        ] {
            let mut header = [0; 28];
            header[5] = bits << 4;
            let mut actual = 0;
            assert_eq!(rate(header.as_ptr(), header.len(), &mut actual), 0);
            assert_eq!(actual, expected);
        }
        let mut actual = 0;
        assert_ne!(rate([0, 0, 0, 0, 0, 0xf0].as_ptr(), 6, &mut actual), 0);
        assert_ne!(rate([0; 5].as_ptr(), 5, &mut actual), 0);
    }
    for value in [0, 1, 90000, 0x1_ffff_ffff] {
        let encoded = [
            0x21 | (((value >> 30) & 7) as u8) << 1,
            (value >> 22) as u8,
            (((value >> 15) & 0x7f) as u8) << 1 | 1,
            (value >> 7) as u8,
            ((value & 0x7f) as u8) << 1 | 1,
        ];
        assert_eq!(native.parse_pts(&encoded).unwrap(), value);
        for length in 0..5 {
            assert!(native.parse_pts(&encoded[..length]).is_err());
        }
    }
}

#[test]
#[ignore = "requires source-built x64 dvda-formats.dll"]
fn native_format_mlp_stream_buffer_damage_and_alignment_match_historical_cases() {
    let native = NativeFormats::load().unwrap();
    let root = Scratch::new();
    let path = root.0.join("样本.mlp");
    let inspect = |data: &[u8]| {
        fs::write(&path, data).unwrap();
        let file = native.inspect_file(&path);
        let mut raw = RawInspection::default();
        // SAFETY: Exact exported signature and valid input/output storage.
        let status = unsafe {
            let function: Inspect =
                export(native.module, c"dvda_formats_mlp_inspect_buffer").unwrap();
            function(data.as_ptr(), data.len(), &mut raw)
        };
        assert_eq!(file.is_ok(), status == 0);
        if let Ok(value) = &file {
            let raw_values = (
                raw.size,
                raw.access_unit_count,
                raw.major_sync_count,
                raw.major_sync_interval,
                raw.major_sync_error_count,
                raw.access_unit_parity_error_count,
                raw.substream_error_count,
                raw.has_end_of_stream != 0,
                raw.peak_bitrate_raw,
                raw.extended_substream_info,
                raw.sample_rate,
                raw.is_valid != 0,
            );
            assert_eq!(
                (
                    value.size,
                    value.access_unit_count,
                    value.major_sync_count,
                    value.major_sync_interval,
                    value.major_sync_error_count,
                    value.access_unit_parity_error_count,
                    value.substream_error_count,
                    value.has_end_of_stream,
                    value.peak_bitrate_raw,
                    value.extended_substream_info,
                    value.sample_rate,
                    value.is_valid
                ),
                raw_values
            );
        }
        file
    };
    let valid = fixture(&native, true, true);
    assert!(inspect(&valid).unwrap().is_valid);
    assert_eq!(native.align(&valid).unwrap().data, valid);
    let no_major = inspect(&fixture(&native, false, true)).unwrap();
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
    let no_eos = fixture(&native, true, false);
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
        stream.extend(fixture(&native, index % 8 == 0, index == 15));
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
#[ignore = "requires source-built x64 dvda-formats.dll"]
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
    let mut large = vec![0; 300 * 1024];
    fs::write(&source, &large).unwrap();
    large[200000] = 255;
    fs::write(&decoded, large).unwrap();
    let result = native.compare_pcm(&source, &decoded, 0, 0).unwrap();
    assert!(!result.matches);
    assert_eq!(result.first_mismatch_offset, 200000);
    assert!(native.compare_pcm(&source, &decoded, 0, 1).is_err());
    assert!(
        native
            .compare_pcm(&root.0.join("missing.raw"), &decoded, 0, 0)
            .is_err()
    );
}
