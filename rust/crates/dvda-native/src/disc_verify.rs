//! Direct wrapper for the project-built DVD-Audio payload verifier.
//!
//! The verifier is a small C17 DLL in `tools/menu-native`. It only receives
//! UTF-8 source paths and 2048-byte aligned ISO chunks; it never launches a
//! process and never changes either input.
use std::{
    ffi::{CStr, CString, OsStr, c_char, c_void},
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    ptr,
};

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct ResultInfo {
    pub code: i32,
    pub track: i32,
    pub offset: u64,
    pub sectors: u64,
    pub bytes: u64,
}

type ReadChunk = unsafe extern "C" fn(*mut c_void, *mut u8, u32) -> i32;
type VerifyMlp =
    unsafe extern "C" fn(*const *const c_char, u32, ReadChunk, *mut c_void, *mut ResultInfo) -> i32;
type VerifyLpcm = unsafe extern "C" fn(
    *const *const c_char,
    *const u8,
    u32,
    ReadChunk,
    *mut c_void,
    *mut ResultInfo,
) -> i32;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(path: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

pub struct NativeDiscVerifier {
    module: *mut c_void,
    mlp: VerifyMlp,
    lpcm: VerifyLpcm,
}

unsafe impl Send for NativeDiscVerifier {}
unsafe impl Sync for NativeDiscVerifier {}

impl Drop for NativeDiscVerifier {
    fn drop(&mut self) {
        // SAFETY: this value owns the module returned by LoadLibraryW.
        unsafe { FreeLibrary(self.module) };
    }
}

impl NativeDiscVerifier {
    pub fn load() -> Result<Self, String> {
        let path = locate_library().ok_or_else(|| {
            "dvda-disc-verify.dll was not found; set DVDA_DISC_VERIFY_LIBRARY or DVDA_MENU_NATIVE_DIR"
                .to_owned()
        })?;
        let wide: Vec<u16> = OsStr::new(&path).encode_wide().chain(Some(0)).collect();
        // SAFETY: the path is a live, NUL-terminated UTF-16 buffer.
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        if module.is_null() {
            return Err(format!("LoadLibraryW failed for {}", path.display()));
        }
        let result = unsafe {
            Ok(Self {
                module,
                mlp: export(module, c"dvda_verify_mlp_payload")?,
                lpcm: export(module, c"dvda_verify_lpcm_payload")?,
            })
        };
        if result.is_err() {
            // SAFETY: module was just loaded and is not shared by another value.
            unsafe { FreeLibrary(module) };
        }
        result
    }

    pub fn verify_mlp<I>(&self, sources: &[PathBuf], chunks: I) -> Result<ResultInfo, String>
    where
        I: Iterator<Item = Result<Vec<u8>, String>>,
    {
        self.verify(sources, None, chunks)
    }

    pub fn verify_lpcm<I>(
        &self,
        sources: &[PathBuf],
        title_ends: &[u8],
        chunks: I,
    ) -> Result<ResultInfo, String>
    where
        I: Iterator<Item = Result<Vec<u8>, String>>,
    {
        if title_ends.len() != sources.len() {
            return Err("LPCM title boundary count does not match source count".into());
        }
        self.verify(sources, Some(title_ends), chunks)
    }

    fn verify<I>(
        &self,
        sources: &[PathBuf],
        title_ends: Option<&[u8]>,
        chunks: I,
    ) -> Result<ResultInfo, String>
    where
        I: Iterator<Item = Result<Vec<u8>, String>>,
    {
        if sources.is_empty() {
            return Err("At least one source track is required".into());
        }
        let strings: Vec<CString> = sources
            .iter()
            .map(|path| {
                CString::new(path.to_string_lossy().as_bytes())
                    .map_err(|_| format!("Source path contains NUL: {}", path.display()))
            })
            .collect::<Result<_, _>>()?;
        let pointers: Vec<*const c_char> = strings.iter().map(|value| value.as_ptr()).collect();
        let mut state = ReaderState {
            chunks,
            current: Vec::new(),
            position: 0,
            failure: None,
        };
        let mut result = ResultInfo::default();
        let status = unsafe {
            match title_ends {
                Some(ends) => (self.lpcm)(
                    pointers.as_ptr(),
                    ends.as_ptr(),
                    pointers.len() as u32,
                    read_chunk::<I>,
                    (&mut state as *mut ReaderState<I>).cast(),
                    &mut result,
                ),
                None => (self.mlp)(
                    pointers.as_ptr(),
                    pointers.len() as u32,
                    read_chunk::<I>,
                    (&mut state as *mut ReaderState<I>).cast(),
                    &mut result,
                ),
            }
        };
        if let Some(error) = state.failure {
            return Err(error);
        }
        if status != 0 {
            return Err(format!(
                "DVD audio payload verification failed: status {}, track {}, offset {}, sectors {}",
                status,
                result.track + 1,
                result.offset,
                result.sectors
            ));
        }
        Ok(result)
    }
}

struct ReaderState<I> {
    chunks: I,
    current: Vec<u8>,
    position: usize,
    failure: Option<String>,
}

unsafe extern "C" fn read_chunk<I>(state: *mut c_void, output: *mut u8, capacity: u32) -> i32
where
    I: Iterator<Item = Result<Vec<u8>, String>>,
{
    // SAFETY: the caller supplies the ReaderState pointer created for this call.
    let state = unsafe { &mut *state.cast::<ReaderState<I>>() };
    let capacity = capacity as usize;
    if output.is_null() || capacity == 0 {
        state.failure = Some("Native verifier supplied an invalid read buffer".into());
        return -1;
    }
    loop {
        if state.position < state.current.len() {
            let available = &state.current[state.position..];
            let count = available.len().min(capacity);
            if count % 2048 != 0 && count != available.len() {
                state.failure = Some("Native verifier requested a partial ISO sector".into());
                return -1;
            }
            // SAFETY: output points to `capacity` writable bytes supplied by C.
            unsafe { ptr::copy_nonoverlapping(available.as_ptr(), output, count) };
            state.position += count;
            if state.position == state.current.len() {
                state.current.clear();
                state.position = 0;
            }
            return count as i32;
        }
        let next = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.chunks.next()));
        let next = match next {
            Ok(next) => next,
            Err(_) => {
                state.failure = Some("ISO reader callback panicked".into());
                return -1;
            }
        };
        match next {
            Some(Ok(chunk)) if chunk.is_empty() => continue,
            Some(Ok(chunk)) if !chunk.len().is_multiple_of(2048) => {
                state.failure = Some("ISO verifier chunk is not sector aligned".into());
                return -1;
            }
            Some(Ok(chunk)) => {
                state.current = chunk;
                state.position = 0;
            }
            Some(Err(error)) => {
                state.failure = Some(error);
                return -1;
            }
            None => return 0,
        }
    }
}

unsafe fn export<T>(module: *mut c_void, name: &'static CStr) -> Result<T, String> {
    // SAFETY: module is live for the duration of this call.
    let pointer = unsafe { GetProcAddress(module, name.as_ptr()) };
    if pointer.is_null() {
        return Err(format!("Missing native export: {}", name.to_string_lossy()));
    }
    // SAFETY: each requested export has the exact C ABI represented by T.
    Ok(unsafe { std::mem::transmute_copy(&pointer) })
}

fn locate_library() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("DVDA_DISC_VERIFY_LIBRARY") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(value) = std::env::var_os("DVDA_MENU_NATIVE_DIR") {
        let path = PathBuf::from(value);
        let candidate = if path.extension().is_some() {
            path
        } else {
            path.join("dvda-disc-verify.dll")
        };
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let base = std::env::current_exe().ok()?.parent()?.to_owned();
    [
        base.join("dvda-disc-verify.dll"),
        base.join("menu-bin/dvda-disc-verify.dll"),
        base.join("menu-native/dvda-disc-verify.dll"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verifier_result_matches_c_layout() {
        assert_eq!(std::mem::size_of::<ResultInfo>(), 32);
        assert_eq!(std::mem::offset_of!(ResultInfo, offset), 8);
    }
}
