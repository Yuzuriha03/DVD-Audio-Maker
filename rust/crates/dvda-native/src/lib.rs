//! Safe, narrow Rust wrappers for the existing C17 format DLL.
pub mod encoder;
pub mod files;
pub mod images;
pub mod media;
pub mod process;

use std::{
    ffi::{CStr, CString, OsStr, c_char, c_void},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr, slice,
};

#[repr(C, packed(1))]
#[derive(Clone, Copy, Default)]
struct RawInspection {
    size: u64,
    access_unit_count: u32,
    major_sync_count: u32,
    major_sync_interval: f64,
    major_sync_error_count: i32,
    access_unit_parity_error_count: i32,
    substream_error_count: i32,
    has_end_of_stream: i32,
    peak_bitrate_raw: i32,
    extended_substream_info: i32,
    sample_rate: i32,
    is_valid: i32,
    error_code: i32,
}

#[repr(C, packed(1))]
#[derive(Clone, Copy, Default)]
struct RawPcmComparison {
    match_value: i32,
    reason_code: i32,
    source_bytes: u64,
    decoded_bytes: u64,
    trailing_zero_bytes: u64,
    first_mismatch_offset: u64,
}

#[repr(C, packed(1))]
#[derive(Clone, Copy, Default)]
struct RawAlignment {
    peak_changes: i32,
    extended_changes: i32,
    checksum_changes: i32,
    inserted_end_of_stream: i32,
    old_header: i32,
    new_header: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MlpInspection {
    pub size: u64,
    pub access_unit_count: u32,
    pub major_sync_count: u32,
    pub major_sync_interval: f64,
    pub major_sync_error_count: i32,
    pub access_unit_parity_error_count: i32,
    pub substream_error_count: i32,
    pub has_end_of_stream: bool,
    pub peak_bitrate_raw: i32,
    pub extended_substream_info: i32,
    pub sample_rate: i32,
    pub is_valid: bool,
    pub error_code: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PcmComparison {
    pub matches: bool,
    pub reason_code: i32,
    pub source_bytes: u64,
    pub decoded_bytes: u64,
    pub trailing_zero_bytes: u64,
    pub first_mismatch_offset: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MlpAlignment {
    pub data: Vec<u8>,
    pub peak_changes: i32,
    pub extended_changes: i32,
    pub checksum_changes: i32,
    pub inserted_end_of_stream: bool,
    pub old_header: i32,
    pub new_header: i32,
}

type InspectFile = unsafe extern "C" fn(*const c_char, *mut RawInspection) -> i32;
type ComparePcm =
    unsafe extern "C" fn(*const c_char, *const c_char, u32, u32, *mut RawPcmComparison) -> i32;
type ParsePts = unsafe extern "C" fn(*const u8, usize, *mut i64) -> i32;
type AlignBuffer =
    unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize, *mut RawAlignment) -> i32;
type Free = unsafe extern "C" fn(*mut c_void);

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(path: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

pub struct NativeFormats {
    module: *mut c_void,
    inspect_file: InspectFile,
    compare_pcm: ComparePcm,
    parse_pts: ParsePts,
    align_buffer: AlignBuffer,
    free: Free,
}

pub fn sample_pts() -> [i64; 4] {
    [0, 1, 90_000, 0x1_FFFF_FFFFi64]
}

impl NativeFormats {
    pub fn load() -> Result<Self, String> {
        let path = locate_library().ok_or_else(|| {
            "dvda-formats.dll was not found; set DVDA_FORMATS_NATIVE_DIR or DVDA_FORMATS_NATIVE_LIBRARY".to_owned()
        })?;
        let wide: Vec<u16> = OsStr::new(&path).encode_wide().chain(Some(0)).collect();
        // SAFETY: the path is NUL terminated and alive during the call.
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        if module.is_null() {
            return Err(format!("LoadLibraryW failed for {}", path.display()));
        }
        // SAFETY: the module remains loaded until NativeFormats is dropped.
        let result = unsafe {
            Ok(Self {
                module,
                inspect_file: export(module, c"dvda_formats_mlp_inspect_file")?,
                compare_pcm: export(module, c"dvda_formats_pcm_compare_files")?,
                parse_pts: export(module, c"dvda_formats_parse_pts")?,
                align_buffer: export(module, c"dvda_formats_mlp_align_buffer")?,
                free: export(module, c"dvda_formats_free")?,
            })
        };
        if result.is_err() {
            // SAFETY: module came from LoadLibraryW and is not owned elsewhere.
            unsafe { FreeLibrary(module) };
        }
        result
    }

    pub fn inspect_file(&self, path: &Path) -> Result<MlpInspection, String> {
        let path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| "MLP path contains NUL".to_owned())?;
        let mut raw = RawInspection::default();
        // SAFETY: both pointers remain valid for this call.
        let status = unsafe { (self.inspect_file)(path.as_ptr(), &mut raw) };
        if status != 0 {
            return Err(format!("MLP inspection failed: {status}"));
        }
        Ok(MlpInspection {
            size: raw.size,
            access_unit_count: raw.access_unit_count,
            major_sync_count: raw.major_sync_count,
            major_sync_interval: raw.major_sync_interval,
            major_sync_error_count: raw.major_sync_error_count,
            access_unit_parity_error_count: raw.access_unit_parity_error_count,
            substream_error_count: raw.substream_error_count,
            has_end_of_stream: raw.has_end_of_stream != 0,
            peak_bitrate_raw: raw.peak_bitrate_raw,
            extended_substream_info: raw.extended_substream_info,
            sample_rate: raw.sample_rate,
            is_valid: raw.is_valid != 0,
            error_code: raw.error_code,
        })
    }

    pub fn compare_pcm(
        &self,
        source: &Path,
        decoded: &Path,
        bytes_per_frame: u32,
        max_zero_frames: u32,
    ) -> Result<PcmComparison, String> {
        let source = CString::new(source.to_string_lossy().as_bytes())
            .map_err(|_| "PCM source path contains NUL".to_owned())?;
        let decoded = CString::new(decoded.to_string_lossy().as_bytes())
            .map_err(|_| "PCM decoded path contains NUL".to_owned())?;
        let mut raw = RawPcmComparison::default();
        // SAFETY: C strings and result storage remain valid for this call.
        let status = unsafe {
            (self.compare_pcm)(
                source.as_ptr(),
                decoded.as_ptr(),
                bytes_per_frame,
                max_zero_frames,
                &mut raw,
            )
        };
        if status != 0 {
            return Err(format!("PCM comparison failed: {status}"));
        }
        Ok(PcmComparison {
            matches: raw.match_value != 0,
            reason_code: raw.reason_code,
            source_bytes: raw.source_bytes,
            decoded_bytes: raw.decoded_bytes,
            trailing_zero_bytes: raw.trailing_zero_bytes,
            first_mismatch_offset: raw.first_mismatch_offset,
        })
    }

    pub fn parse_pts(&self, data: &[u8]) -> Result<i64, String> {
        let mut value = 0i64;
        // SAFETY: the slice and output remain valid for this call.
        let status = unsafe { (self.parse_pts)(data.as_ptr(), data.len(), &mut value) };
        if status != 0 {
            return Err(format!("PTS parsing failed: {status}"));
        }
        Ok(value)
    }

    pub fn align(&self, data: &[u8]) -> Result<MlpAlignment, String> {
        let mut pointer = ptr::null_mut();
        let mut length = 0usize;
        let mut raw = RawAlignment::default();
        // SAFETY: input and output pointers are valid for this call.
        let status = unsafe {
            (self.align_buffer)(
                data.as_ptr(),
                data.len(),
                &mut pointer,
                &mut length,
                &mut raw,
            )
        };
        if status != 0 {
            return Err(format!("MLP alignment failed: {status}"));
        }
        if pointer.is_null() && length != 0 {
            return Err("Native alignment returned a null buffer".into());
        }
        // SAFETY: the DLL returned length readable bytes and its matching free is used below.
        let aligned = if length == 0 {
            Vec::new()
        } else {
            unsafe { slice::from_raw_parts(pointer, length).to_vec() }
        };
        // SAFETY: pointer is owned by this DLL and must be released by its export.
        unsafe { (self.free)(pointer.cast()) };
        Ok(MlpAlignment {
            data: aligned,
            peak_changes: raw.peak_changes,
            extended_changes: raw.extended_changes,
            checksum_changes: raw.checksum_changes,
            inserted_end_of_stream: raw.inserted_end_of_stream != 0,
            old_header: raw.old_header,
            new_header: raw.new_header,
        })
    }
}

impl Drop for NativeFormats {
    fn drop(&mut self) {
        // SAFETY: this instance owns the module and drops it exactly once.
        unsafe { FreeLibrary(self.module) };
    }
}
unsafe impl Send for NativeFormats {}
unsafe impl Sync for NativeFormats {}

unsafe fn export<T>(module: *mut c_void, name: &'static CStr) -> Result<T, String> {
    // SAFETY: module is live for the caller's lifetime.
    let pointer = unsafe { GetProcAddress(module, name.as_ptr()) };
    if pointer.is_null() {
        return Err(format!("Missing native export: {}", name.to_string_lossy()));
    }
    // SAFETY: each requested export has its corresponding exact C ABI type.
    Ok(unsafe { std::mem::transmute_copy(&pointer) })
}

fn locate_library() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("DVDA_FORMATS_NATIVE_LIBRARY") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(value) = std::env::var_os("DVDA_FORMATS_NATIVE_DIR") {
        let path = PathBuf::from(value);
        let path = if path.extension().is_some() {
            path
        } else {
            path.join("dvda-formats.dll")
        };
        if path.is_file() {
            return Some(path);
        }
    }
    let base = std::env::current_exe().ok()?.parent()?.to_owned();
    [
        base.join("dvda-formats.dll"),
        base.join("formats-native/dvda-formats.dll"),
        base.join("menu-bin/dvda-formats.dll"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;
    #[test]
    fn abi_layout_matches_header() {
        assert_eq!(size_of::<RawInspection>(), 60);
        assert_eq!(size_of::<RawPcmComparison>(), 40);
        assert_eq!(size_of::<RawAlignment>(), 24);
    }
}
