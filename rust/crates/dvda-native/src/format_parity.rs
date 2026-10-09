//! Rust format behavior and differential checks against the frozen C17 DLL.
use super::*;
use std::{
    fs,
    os::windows::ffi::OsStrExt,
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
    let header = ((((parity & 15) ^ 15) << 12) | words) as u16;
    data[..2].copy_from_slice(&header.to_be_bytes());
    data
}

#[test]
fn native_format_checksum_parity_peak_rate_and_pts_match_historical_constants() {
    let native = NativeFormats::load().unwrap();
    assert_eq!(fixture_crc(&[0xf8, 0x72, 0x6f, 0xbb, 0, 0]), 0x8699);
    assert_eq!(mlp_formats::checksum8(&[1, 2, 3, 0]), 0xb0);
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

#[repr(C, packed)]
#[derive(Default)]
struct OracleInspection {
    size: u64,
    units: u32,
    majors: u32,
    interval: f64,
    major_errors: i32,
    parity_errors: i32,
    sub_errors: i32,
    eos: i32,
    peak: i32,
    extended: i32,
    rate: i32,
    valid: i32,
    error: i32,
}
#[repr(C, packed)]
#[derive(Default)]
struct OracleAlignment {
    peaks: i32,
    extended: i32,
    checksums: i32,
    inserted: i32,
    old: i32,
    new: i32,
}
#[repr(C, packed)]
#[derive(Default)]
struct OracleComparison {
    matches: i32,
    reason: i32,
    source: u64,
    decoded: u64,
    zeros: u64,
    offset: u64,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(path: *const u16) -> *mut std::ffi::c_void;
    fn GetProcAddress(
        module: *mut std::ffi::c_void,
        name: *const std::ffi::c_char,
    ) -> *mut std::ffi::c_void;
    fn FreeLibrary(module: *mut std::ffi::c_void) -> i32;
}

struct Oracle(*mut std::ffi::c_void);
impl Oracle {
    fn enabled() -> bool {
        std::env::var_os("DVDA_FORMATS_ORACLE").is_some()
    }

    fn load() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..\\..\\..\\build\\c-rust-migration-oracle\\dvda-formats.dll");
        assert!(
            path.is_file(),
            "frozen format oracle missing: {}",
            path.display()
        );
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: the NUL-terminated path exists, and the owned library stays loaded until drop.
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        assert!(
            !module.is_null(),
            "could not load frozen format DLL: {}",
            path.display()
        );
        Self(module)
    }
    unsafe fn symbol<T>(&self, name: &'static std::ffi::CStr) -> T {
        // SAFETY: all requested exports have their exact header ABI and the module is live.
        let pointer = unsafe { GetProcAddress(self.0, name.as_ptr()) };
        assert!(!pointer.is_null(), "missing oracle export: {name:?}");
        unsafe { std::mem::transmute_copy(&pointer) }
    }
}
impl Drop for Oracle {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.0);
        }
    }
}

#[test]
fn frozen_dll_mlp_and_checksum_differential() {
    if !Oracle::enabled() {
        eprintln!("set DVDA_FORMATS_ORACLE=1 to compare against the frozen format DLL");
        return;
    }
    let oracle = Oracle::load();
    type Inspect = unsafe extern "C" fn(*const u8, usize, *mut OracleInspection) -> i32;
    type Align = unsafe extern "C" fn(
        *const u8,
        usize,
        *mut *mut u8,
        *mut usize,
        *mut OracleAlignment,
    ) -> i32;
    type Free = unsafe extern "C" fn(*mut std::ffi::c_void);
    type Crc16 = unsafe extern "C" fn(*const u8, usize, *mut u16) -> i32;
    type Crc8 = unsafe extern "C" fn(*const u8, usize, *mut u8) -> i32;
    let inspect: Inspect = unsafe { oracle.symbol(c"dvda_formats_mlp_inspect_buffer") };
    let align: Align = unsafe { oracle.symbol(c"dvda_formats_mlp_align_buffer") };
    let free: Free = unsafe { oracle.symbol(c"dvda_formats_free") };
    let crc16: Crc16 = unsafe { oracle.symbol(c"dvda_formats_checksum16") };
    let crc8: Crc8 = unsafe { oracle.symbol(c"dvda_formats_checksum8") };
    let mut seed = 0x8b2d_197a_ee30_145au64;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed as u8
    };
    for length in 2..260 {
        let data: Vec<u8> = (0..length).map(|_| random()).collect();
        let mut expected = 0;
        assert_eq!(
            unsafe { crc16(data.as_ptr(), data.len(), &mut expected) },
            0
        );
        assert_eq!(
            mlp_formats::checksum16(&data),
            expected,
            "CRC16 length {length}"
        );
        let mut expected8 = 0;
        assert_eq!(
            unsafe { crc8(data.as_ptr(), data.len(), &mut expected8) },
            0
        );
        assert_eq!(
            mlp_formats::checksum8(&data),
            expected8,
            "CRC8 length {length}"
        );
    }
    let mut cases = vec![
        vec![],
        fixture(true, true),
        fixture(true, false),
        fixture(false, true),
    ];
    let mut many = Vec::new();
    for i in 0..33 {
        many.extend(fixture(i % 8 == 0, i == 32));
    }
    cases.push(many);
    let valid = fixture(true, true);
    let mut invalid = valid.clone();
    invalid[5] = 0xf0;
    cases.push(invalid);
    let mut damaged = valid.clone();
    damaged[18] = 0;
    damaged[20] = 2;
    damaged[30] ^= 1;
    cases.push(damaged);
    let mut parity = valid.clone();
    parity[0] ^= 0x10;
    cases.push(parity);
    let mut bad_header = valid.clone();
    bad_header[32] = 0xff;
    cases.push(bad_header);
    for cut in 0..valid.len() {
        cases.push(valid[..cut].to_vec());
    }
    for case in cases {
        let mut raw = OracleInspection::default();
        let status = unsafe { inspect(case.as_ptr(), case.len(), &mut raw) };
        let rust = mlp_formats::inspect_buffer(&case);
        assert_eq!(status == 0, rust.is_ok(), "inspect len {}", case.len());
        if let Ok(value) = rust {
            let actual = (
                value.size,
                value.access_unit_count,
                value.major_sync_count,
                value.major_sync_interval,
                value.major_sync_error_count,
                value.access_unit_parity_error_count,
                value.substream_error_count,
                i32::from(value.has_end_of_stream),
                value.peak_bitrate_raw,
                value.extended_substream_info,
                value.sample_rate,
                i32::from(value.is_valid),
            );
            let expected = (
                raw.size,
                raw.units,
                raw.majors,
                raw.interval,
                raw.major_errors,
                raw.parity_errors,
                raw.sub_errors,
                raw.eos,
                raw.peak,
                raw.extended,
                raw.rate,
                raw.valid,
            );
            assert_eq!(actual, expected, "inspect len {}", case.len());
            let error = raw.error;
            assert_eq!(value.error_code, error);
        }
        let mut pointer = std::ptr::null_mut();
        let mut length = 0;
        let mut alignment = OracleAlignment::default();
        let status = unsafe {
            align(
                case.as_ptr(),
                case.len(),
                &mut pointer,
                &mut length,
                &mut alignment,
            )
        };
        let rust = NativeFormats.align(&case);
        assert_eq!(status == 0, rust.is_ok(), "align len {}", case.len());
        if let Ok(result) = rust {
            let expected = unsafe { std::slice::from_raw_parts(pointer, length) };
            assert_eq!(result.data, expected, "align bytes len {}", case.len());
            assert_eq!(
                (
                    result.peak_changes,
                    result.extended_changes,
                    result.checksum_changes,
                    i32::from(result.inserted_end_of_stream),
                    result.old_header,
                    result.new_header
                ),
                (
                    alignment.peaks,
                    alignment.extended,
                    alignment.checksums,
                    alignment.inserted,
                    alignment.old,
                    alignment.new
                )
            );
        }
        if !pointer.is_null() {
            unsafe { free(pointer.cast()) };
        }
    }
}

#[test]
fn frozen_dll_pcm_and_pts_differential() {
    if !Oracle::enabled() {
        eprintln!("set DVDA_FORMATS_ORACLE=1 to compare against the frozen format DLL");
        return;
    }
    let oracle = Oracle::load();
    type Compare = unsafe extern "C" fn(
        *const std::ffi::c_char,
        *const std::ffi::c_char,
        u32,
        u32,
        *mut OracleComparison,
    ) -> i32;
    type Pts = unsafe extern "C" fn(*const u8, usize, *mut i64) -> i32;
    let compare: Compare = unsafe { oracle.symbol(c"dvda_formats_pcm_compare_files") };
    let parse: Pts = unsafe { oracle.symbol(c"dvda_formats_parse_pts") };
    let root = Scratch::new();
    let source = root.0.join("source.raw");
    let decoded = root.0.join("decoded.raw");
    let src = std::ffi::CString::new(source.to_str().unwrap()).unwrap();
    let dst = std::ffi::CString::new(decoded.to_str().unwrap()).unwrap();
    let bytes: Vec<_> = (0..300000u32).map(|i| i as u8).collect();
    for (left, right, frame, max) in [
        (vec![], vec![], 0, 0),
        (vec![1, 2, 3, 4], vec![1, 2, 3, 4], 0, 0),
        (vec![1, 2, 3, 4], vec![1, 2, 3, 4, 0, 0], 2, 1),
        (vec![1, 2, 3, 4], vec![1, 2, 3, 4, 0, 1], 2, 1),
        (vec![1, 2, 3, 4], vec![1, 2, 3], 0, 0),
        (vec![1, 2, 3, 4], vec![1, 2, 7, 4], 0, 0),
        (
            bytes.clone(),
            {
                let mut r = bytes.clone();
                r[199999] ^= 1;
                r
            },
            0,
            0,
        ),
        (vec![1], vec![1], 0, 1),
    ] {
        fs::write(&source, left).unwrap();
        fs::write(&decoded, right).unwrap();
        let mut raw = OracleComparison::default();
        let status = unsafe { compare(src.as_ptr(), dst.as_ptr(), frame, max, &mut raw) };
        let result = NativeFormats.compare_pcm(&source, &decoded, frame, max);
        assert_eq!(status == 0, result.is_ok());
        if let Ok(value) = result {
            assert_eq!(
                (
                    i32::from(value.matches),
                    value.reason_code,
                    value.source_bytes,
                    value.decoded_bytes,
                    value.trailing_zero_bytes,
                    value.first_mismatch_offset
                ),
                (
                    raw.matches,
                    raw.reason,
                    raw.source,
                    raw.decoded,
                    raw.zeros,
                    raw.offset
                )
            );
        }
    }
    for n in 0..60u8 {
        let data = [
            n,
            n.wrapping_mul(17),
            n.wrapping_mul(31),
            n.wrapping_mul(37),
            n.wrapping_mul(41),
        ];
        let mut raw = 0;
        assert_eq!(unsafe { parse(data.as_ptr(), data.len(), &mut raw) }, 0);
        assert_eq!(NativeFormats.parse_pts(&data).unwrap(), raw);
    }
}

#[test]
fn frozen_dll_real_mlp_file_differential() {
    if !Oracle::enabled() {
        eprintln!(
            "set DVDA_FORMATS_ORACLE=1 and DVDA_FORMATS_REAL_MLP=<file> to compare a real MLP"
        );
        return;
    }
    let Some(path) = std::env::var_os("DVDA_FORMATS_REAL_MLP") else {
        eprintln!("set DVDA_FORMATS_REAL_MLP=<file> to opt into real-sample differential testing");
        return;
    };
    let path = PathBuf::from(path);
    assert!(
        path.is_file(),
        "real MLP sample missing: {}",
        path.display()
    );
    let oracle = Oracle::load();
    type InspectFile = unsafe extern "C" fn(*const std::ffi::c_char, *mut OracleInspection) -> i32;
    let inspect: InspectFile = unsafe { oracle.symbol(c"dvda_formats_mlp_inspect_file") };
    let name =
        std::ffi::CString::new(path.to_str().expect("real sample path must be UTF-8")).unwrap();
    let mut raw = OracleInspection::default();
    let status = unsafe { inspect(name.as_ptr(), &mut raw) };
    let rust = NativeFormats.inspect_file(&path);
    assert_eq!(
        status == 0,
        rust.is_ok(),
        "real MLP inspection: {}",
        path.display()
    );
    if let Ok(value) = rust {
        let actual = (
            value.size,
            value.access_unit_count,
            value.major_sync_count,
            value.major_sync_interval,
            value.major_sync_error_count,
            value.access_unit_parity_error_count,
            value.substream_error_count,
            i32::from(value.has_end_of_stream),
            value.peak_bitrate_raw,
            value.extended_substream_info,
            value.sample_rate,
            i32::from(value.is_valid),
        );
        let expected = (
            raw.size,
            raw.units,
            raw.majors,
            raw.interval,
            raw.major_errors,
            raw.parity_errors,
            raw.sub_errors,
            raw.eos,
            raw.peak,
            raw.extended,
            raw.rate,
            raw.valid,
        );
        assert_eq!(actual, expected, "real MLP inspection: {}", path.display());
        let error = raw.error;
        assert_eq!(value.error_code, error);
    }
}

#[test]
fn frozen_dll_encoded_mlp_profiles_differential() {
    if !Oracle::enabled() {
        eprintln!("set DVDA_FORMATS_ORACLE=1 to encode and compare real MLP profiles");
        return;
    }
    use crate::encoder::{Encoder, Request, Stamp, Stream};
    use std::io;

    struct DeterministicPcm {
        frames: usize,
        channels: usize,
        bits: u32,
        position: usize,
        output: Vec<u8>,
    }
    impl Stream for DeterministicPcm {
        fn read(&mut self, samples: &mut [i32]) -> io::Result<usize> {
            let count = (self.frames - self.position).min(samples.len() / self.channels);
            for frame in 0..count {
                for channel in 0..self.channels {
                    let position = self.position + frame;
                    let level = if position < 17 || position >= self.frames - 13 {
                        0
                    } else {
                        ((position as i32 * 37 + channel as i32 * 733) & 1023) - 512
                    };
                    samples[frame * self.channels + channel] =
                        (level << (self.bits - 10)) << (24 - self.bits);
                }
            }
            self.position += count;
            Ok(count)
        }
        fn write(&mut self, data: &[u8]) -> io::Result<()> {
            self.output.extend_from_slice(data);
            Ok(())
        }
    }

    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..\\..\\..");
    let encoder_path = repository.join("native\\mlp-encoder\\win-x64\\mlp_encoder.dll");
    assert!(
        encoder_path.is_file(),
        "pinned MLP encoder absent: {}",
        encoder_path.display()
    );
    let encoder = Encoder::load(&encoder_path).expect("load pinned MLP encoder");
    let oracle = Oracle::load();
    type InspectFile = unsafe extern "C" fn(*const std::ffi::c_char, *mut OracleInspection) -> i32;
    type InspectBuffer = unsafe extern "C" fn(*const u8, usize, *mut OracleInspection) -> i32;
    type Align = unsafe extern "C" fn(
        *const u8,
        usize,
        *mut *mut u8,
        *mut usize,
        *mut OracleAlignment,
    ) -> i32;
    type Free = unsafe extern "C" fn(*mut std::ffi::c_void);
    let inspect_file: InspectFile = unsafe { oracle.symbol(c"dvda_formats_mlp_inspect_file") };
    let inspect_buffer: InspectBuffer =
        unsafe { oracle.symbol(c"dvda_formats_mlp_inspect_buffer") };
    let align: Align = unsafe { oracle.symbol(c"dvda_formats_mlp_align_buffer") };
    let free: Free = unsafe { oracle.symbol(c"dvda_formats_free") };
    let outputs = repository.join("build\\formats-real-parity");
    fs::create_dir_all(&outputs).unwrap();
    // Channel assignments are the same mapping exercised by the pinned encoder fixture.
    let profiles = [
        (44_100, 16, 1, 0),
        (48_000, 20, 2, 1),
        (48_000, 16, 6, 12),
        (48_000, 20, 6, 12),
        (88_200, 20, 1, 0),
        (96_000, 16, 2, 1),
        (96_000, 24, 6, 12),
        (176_400, 24, 2, 1),
        (192_000, 24, 1, 0),
    ];
    for (rate, bits, channels, assignment) in profiles {
        let frames = rate as usize / 8 + 37;
        let mut stream = DeterministicPcm {
            frames,
            channels: channels as usize,
            bits,
            position: 0,
            output: Vec::new(),
        };
        let request = Request {
            sample_rate: rate,
            bits,
            channels,
            frames: frames as u64,
            assignment,
            metadata: vec![Stamp {
                start: 0,
                packet: vec![0, 0, 0x40, 0],
                valid_bits: 0,
            }],
        };
        let result = encoder
            .encode(&request, &mut stream)
            .expect("pinned encoder callback");
        let name = format!("{rate}_{bits}_{channels}");
        assert_eq!(result.status, 0, "{name}: {}", result.error);
        assert_eq!(result.input_frames, frames as u64, "{name}");
        assert_eq!(result.output_bytes, stream.output.len() as u64, "{name}");
        assert!(result.access_units > 0, "{name}");
        let path = outputs.join(format!("{name}.mlp"));
        fs::write(&path, &stream.output).unwrap();
        let name_c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        let mut from_file = OracleInspection::default();
        let mut from_buffer = OracleInspection::default();
        let file_status = unsafe { inspect_file(name_c.as_ptr(), &mut from_file) };
        let buffer_status = unsafe {
            inspect_buffer(
                stream.output.as_ptr(),
                stream.output.len(),
                &mut from_buffer,
            )
        };
        let rust_file = NativeFormats.inspect_file(&path);
        let rust_buffer = mlp_formats::inspect_buffer(&stream.output);
        assert_eq!(file_status == 0, rust_file.is_ok(), "{name} file status");
        assert_eq!(
            buffer_status == 0,
            rust_buffer.is_ok(),
            "{name} buffer status"
        );
        let fields = |raw: OracleInspection| {
            (
                raw.size,
                raw.units,
                raw.majors,
                raw.interval,
                raw.major_errors,
                raw.parity_errors,
                raw.sub_errors,
                raw.eos,
                raw.peak,
                raw.extended,
                raw.rate,
                raw.valid,
            )
        };
        let rust_fields = |inspection: &MlpInspection| {
            (
                inspection.size,
                inspection.access_unit_count,
                inspection.major_sync_count,
                inspection.major_sync_interval,
                inspection.major_sync_error_count,
                inspection.access_unit_parity_error_count,
                inspection.substream_error_count,
                i32::from(inspection.has_end_of_stream),
                inspection.peak_bitrate_raw,
                inspection.extended_substream_info,
                inspection.sample_rate,
                i32::from(inspection.is_valid),
            )
        };
        let file_error = from_file.error;
        let buffer_error = from_buffer.error;
        if let Ok(inspection) = &rust_file {
            assert_eq!(
                rust_fields(inspection),
                fields(from_file),
                "{name} file inspection"
            );
            assert_eq!(inspection.error_code, file_error, "{name} file error code");
        }
        if let Ok(inspection) = &rust_buffer {
            assert_eq!(
                rust_fields(inspection),
                fields(from_buffer),
                "{name} buffer inspection"
            );
            assert_eq!(
                inspection.error_code, buffer_error,
                "{name} buffer error code"
            );
        }
        let mut pointer = std::ptr::null_mut();
        let mut length = 0;
        let mut aligned = OracleAlignment::default();
        let status = unsafe {
            align(
                stream.output.as_ptr(),
                stream.output.len(),
                &mut pointer,
                &mut length,
                &mut aligned,
            )
        };
        let rust = NativeFormats.align(&stream.output);
        assert_eq!(status == 0, rust.is_ok(), "{name} align status");
        if let Ok(value) = rust {
            assert!(!pointer.is_null(), "{name} oracle output");
            let bytes = unsafe { std::slice::from_raw_parts(pointer, length) };
            assert_eq!(value.data, bytes, "{name} aligned bytes");
            assert_eq!(
                (
                    value.peak_changes,
                    value.extended_changes,
                    value.checksum_changes,
                    i32::from(value.inserted_end_of_stream),
                    value.old_header,
                    value.new_header
                ),
                (
                    aligned.peaks,
                    aligned.extended,
                    aligned.checksums,
                    aligned.inserted,
                    aligned.old,
                    aligned.new
                ),
                "{name} alignment fields"
            );
            fs::write(outputs.join(format!("{name}.aligned.mlp")), &value.data).unwrap();
        }
        if !pointer.is_null() {
            unsafe { free(pointer.cast()) };
        }
        eprintln!(
            "{name}: {frames} frames, {} access units, {} MLP bytes, inspection + align matched",
            result.access_units, result.output_bytes
        );
    }
}
