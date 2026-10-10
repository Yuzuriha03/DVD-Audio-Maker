//! Typed API for the project's in-process C media implementation.
use serde::{Deserialize, Serialize};
#[cfg(not(feature = "direct-bridges"))]
use std::{
    collections::HashMap,
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use std::{
    ffi::{CStr, CString, c_char, c_void},
    io,
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(u32)]
pub enum Operation {
    Probe = 1,
    Packets = 2,
    Audio = 3,
    VideoFrame = 4,
    Cover = 5,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(u32)]
pub enum OutputFormat {
    None = 0,
    Wave = 1,
    S24 = 2,
    S16 = 3,
    S32 = 4,
    Md5 = 5,
    Flac = 6,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Request {
    pub operation: Operation,
    pub rate: u32,
    pub bits: u32,
    pub output_format: OutputFormat,
    pub soxr: bool,
    pub compression: u32,
    pub cover: bool,
    pub input: String,
    pub output: Option<String>,
    pub tags: Vec<(String, String)>,
}

impl Request {
    pub fn validate(&self) -> io::Result<()> {
        let invalid = |message| io::Error::new(io::ErrorKind::InvalidInput, message);
        if self.input.is_empty() || self.rate > i32::MAX as u32 {
            return Err(invalid("Invalid media input path or sample rate"));
        }
        if matches!(self.operation, Operation::Audio) {
            let bits_valid = match self.output_format {
                OutputFormat::S24 | OutputFormat::S32 => matches!(self.bits, 20 | 24),
                OutputFormat::S16 | OutputFormat::Md5 => self.bits == 16,
                _ => matches!(self.bits, 0 | 16 | 20 | 24),
            };
            if !bits_valid {
                return Err(invalid(
                    "PCM precision does not match the native output layout",
                ));
            }
        }
        let requires_output = matches!(self.operation, Operation::VideoFrame | Operation::Cover)
            || (matches!(self.operation, Operation::Audio)
                && matches!(
                    self.output_format,
                    OutputFormat::Wave
                        | OutputFormat::S24
                        | OutputFormat::S16
                        | OutputFormat::S32
                        | OutputFormat::Flac
                ));
        if requires_output && self.output.as_ref().is_none_or(String::is_empty) {
            return Err(invalid("Media output path is required"));
        }
        Ok(())
    }
}

#[cfg(any(test, not(feature = "direct-bridges")))]
#[repr(C)]
struct RawRequest {
    size: u32,
    abi: u32,
    operation: u32,
    rate: u32,
    bits: u32,
    output_format: u32,
    soxr: u32,
    compression: u32,
    cover: u32,
    tag_count: u32,
    input: *const c_char,
    output: *const c_char,
    tags: *const *const c_char,
}
#[cfg(not(feature = "direct-bridges"))]
pub(crate) type Emit = unsafe extern "C" fn(*mut c_void, i32, *const c_char);
#[cfg(not(feature = "direct-bridges"))]
pub(crate) type Cancel = unsafe extern "C" fn(*mut c_void) -> i32;
#[cfg(not(feature = "direct-bridges"))]
type Run = unsafe extern "C" fn(*const RawRequest, Emit, Cancel, *mut c_void) -> i32;

/// Callbacks are synchronous and run on the calling thread. No borrowed state is retained.
pub trait Callbacks {
    fn emit(&mut self, stream: i32, text: &str);
    fn cancelled(&mut self) -> bool;
    fn progress(&mut self, _completed: u64, _total: u64) {}
}

pub struct ProgressScope<'a> {
    callbacks: &'a mut dyn Callbacks,
    start: u64,
    end: u64,
    last: u64,
}

impl<'a> ProgressScope<'a> {
    pub fn new(callbacks: &'a mut dyn Callbacks, start: u64, end: u64) -> Self {
        Self {
            callbacks,
            start: start.min(100),
            end: end.min(100).max(start.min(100)),
            last: start.min(100),
        }
    }
}

impl Callbacks for ProgressScope<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        self.callbacks.emit(stream, text);
    }

    fn cancelled(&mut self) -> bool {
        self.callbacks.cancelled()
    }

    fn progress(&mut self, completed: u64, total: u64) {
        let portion = completed
            .min(total)
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(100);
        let span = self.end - self.start;
        self.last = self
            .last
            .max(self.start + span.saturating_mul(portion) / 100);
        self.callbacks.progress(self.last, 100);
    }
}
pub(crate) struct CallbackState<'a> {
    pub(crate) callbacks: &'a mut dyn Callbacks,
    pub(crate) panicked: bool,
}
pub(crate) unsafe extern "C" fn emit(state: *mut c_void, stream: i32, text: *const c_char) {
    // SAFETY: only passed to dvdamedia_run during the live stack frame below.
    let state = unsafe { &mut *state.cast::<CallbackState<'_>>() };
    if state.panicked {
        return;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let text = if text.is_null() {
            std::borrow::Cow::Borrowed("")
        } else {
            unsafe { CStr::from_ptr(text) }.to_string_lossy()
        };
        state.callbacks.emit(stream, &text);
    }));
    state.panicked |= result.is_err();
}
pub(crate) unsafe extern "C" fn cancel(state: *mut c_void) -> i32 {
    // SAFETY: same stack lifetime as emit; the C implementation invokes callbacks serially.
    let state = unsafe { &mut *state.cast::<CallbackState<'_>>() };
    if state.panicked {
        return 1;
    }
    match catch_unwind(AssertUnwindSafe(|| state.callbacks.cancelled())) {
        Ok(value) => i32::from(value),
        Err(_) => {
            state.panicked = true;
            1
        }
    }
}

#[cfg(not(feature = "direct-bridges"))]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

#[cfg(feature = "direct-bridges")]
pub fn loaded_dependencies() -> io::Result<Vec<std::path::PathBuf>> {
    use std::os::windows::ffi::OsStringExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn K32EnumProcessModules(
            process: *mut c_void,
            modules: *mut *mut c_void,
            bytes: u32,
            needed: *mut u32,
        ) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, path: *mut u16, size: u32) -> u32;
    }
    let mut modules = vec![std::ptr::null_mut(); 128];
    loop {
        let bytes = u32::try_from(modules.len() * std::mem::size_of::<*mut c_void>())
            .map_err(|_| io::Error::other("Too many loaded modules"))?;
        let mut needed = 0;
        if unsafe {
            K32EnumProcessModules(
                GetCurrentProcess(),
                modules.as_mut_ptr(),
                bytes,
                &mut needed,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if needed > bytes {
            modules.resize(
                (needed as usize / std::mem::size_of::<*mut c_void>()) + 32,
                std::ptr::null_mut(),
            );
            continue;
        }
        modules.truncate(needed as usize / std::mem::size_of::<*mut c_void>());
        break;
    }
    let mut paths = Vec::new();
    for module in modules {
        let mut path = vec![0u16; 32768];
        let length =
            unsafe { GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) } as usize;
        if length == 0 {
            let error = io::Error::last_os_error();
            // An oracle DLL can unload after the module snapshot was captured.
            if error.raw_os_error() == Some(126) {
                continue;
            }
            return Err(error);
        }
        if length >= path.len() {
            return Err(io::Error::other("Loaded module path was truncated"));
        }
        let path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&path[..length]));
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        {
            paths.push(path);
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

pub struct Media {
    #[cfg(not(feature = "direct-bridges"))]
    module: *mut c_void,
    #[cfg(not(feature = "direct-bridges"))]
    run: Run,
}
// The C module uses a per-call/TLS state and thread-safe one-time initialization.
unsafe impl Send for Media {}
unsafe impl Sync for Media {}
impl Drop for Media {
    fn drop(&mut self) {
        #[cfg(not(feature = "direct-bridges"))]
        unsafe {
            FreeLibrary(self.module);
        }
    }
}
impl Media {
    #[cfg(feature = "direct-bridges")]
    pub fn linked() -> Arc<Self> {
        Arc::new(Self {})
    }

    /// Compatibility entry point; direct builds use the linked implementation, not this path.
    #[cfg(feature = "direct-bridges")]
    pub fn load(_path: &Path) -> io::Result<Arc<Self>> {
        Ok(Self::linked())
    }

    #[cfg(not(feature = "direct-bridges"))]
    pub fn load(path: &Path) -> io::Result<Arc<Self>> {
        static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Media>>>> = OnceLock::new();
        let path = std::path::absolute(path)?;
        let mut cache = CACHE
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| io::Error::other("Media library cache lock poisoned"))?;
        if let Some(media) = cache.get(&path) {
            return Ok(Arc::clone(media));
        }
        let mut wide: Vec<_> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(io::Error::from_raw_os_error(123));
        }
        wide.push(0);
        // Only resolve dependent DLLs beside this library and in System32, never PATH/CWD.
        let module = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), 0x100 | 0x800) };
        if module.is_null() {
            return Err(io::Error::last_os_error());
        }
        let entry = unsafe { GetProcAddress(module, c"dvdamedia_run".as_ptr()) };
        if entry.is_null() {
            let error = io::Error::last_os_error();
            unsafe {
                FreeLibrary(module);
            }
            return Err(error);
        }
        // SAFETY: dvdamedia_run is the fixed C ABI defined in native/dvda-media.c.
        let media = Arc::new(Self {
            module,
            run: unsafe { std::mem::transmute::<*mut c_void, Run>(entry) },
        });
        cache.insert(path, Arc::clone(&media));
        Ok(media)
    }

    #[cfg(feature = "direct-bridges")]
    pub fn run(&self, request: &Request, callbacks: &mut dyn Callbacks) -> io::Result<i32> {
        request.validate()?;
        fn string(text: &str) -> io::Result<CString> {
            CString::new(text).map_err(|_| io::Error::from_raw_os_error(123))
        }
        let mut direct = dvda_bridges::direct::MediaRequest::new(
            request.operation as u32,
            &request.input,
            request.output.as_deref().unwrap_or(""),
        )
        .map_err(|_| io::Error::from_raw_os_error(123))?;
        direct.output = request.output.as_deref().map(string).transpose()?;
        direct.rate = request.rate;
        direct.bits = request.bits;
        direct.output_format = request.output_format as u32;
        direct.soxr = request.soxr;
        direct.compression = request.compression;
        direct.cover = request.cover;
        direct.tags = request
            .tags
            .iter()
            .flat_map(|(key, value)| [key.as_str(), value.as_str()])
            .map(string)
            .collect::<io::Result<_>>()?;
        let mut state = CallbackState {
            callbacks,
            panicked: false,
        };
        let result = unsafe {
            direct.run_with_callbacks(dvda_bridges::Call {
                emit: Some(emit),
                cancel: Some(cancel),
                state: (&mut state as *mut CallbackState<'_>).cast(),
            })
        };
        if state.panicked {
            return Err(io::Error::other("Panic caught in media callback"));
        }
        Ok(result)
    }

    #[cfg(not(feature = "direct-bridges"))]
    pub fn run(&self, request: &Request, callbacks: &mut dyn Callbacks) -> io::Result<i32> {
        request.validate()?;
        fn string(text: &str) -> io::Result<CString> {
            CString::new(text).map_err(|_| io::Error::from_raw_os_error(123))
        }
        let input = string(&request.input)?;
        let output = request.output.as_deref().map(string).transpose()?;
        let tags: Vec<CString> = request
            .tags
            .iter()
            .flat_map(|(key, value)| [key.as_str(), value.as_str()])
            .map(string)
            .collect::<io::Result<_>>()?;
        let pointers: Vec<_> = tags.iter().map(|tag| tag.as_ptr()).collect();
        let raw = RawRequest {
            size: std::mem::size_of::<RawRequest>() as u32,
            abi: 1,
            operation: request.operation as u32,
            rate: request.rate,
            bits: request.bits,
            output_format: request.output_format as u32,
            soxr: u32::from(request.soxr),
            compression: request.compression,
            cover: u32::from(request.cover),
            tag_count: u32::try_from(request.tags.len())
                .map_err(|_| io::Error::other("Too many media tags"))?,
            input: input.as_ptr(),
            output: output.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
            tags: if pointers.is_empty() {
                std::ptr::null()
            } else {
                pointers.as_ptr()
            },
        };
        let mut state = CallbackState {
            callbacks,
            panicked: false,
        };
        // SAFETY: every pointer remains live until this synchronous function returns.
        let result = unsafe {
            (self.run)(
                &raw,
                emit,
                cancel,
                (&mut state as *mut CallbackState<'_>).cast(),
            )
        };
        if state.panicked {
            return Err(io::Error::other("Panic caught in media callback"));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Events(Vec<(u64, u64)>);

    impl Callbacks for Events {
        fn emit(&mut self, _: i32, _: &str) {}

        fn cancelled(&mut self) -> bool {
            false
        }

        fn progress(&mut self, completed: u64, total: u64) {
            self.0.push((completed, total));
        }
    }

    #[test]
    fn media_abi_matches_c_header() {
        assert_eq!(std::mem::size_of::<RawRequest>(), 64);
        assert_eq!(std::mem::offset_of!(RawRequest, input), 40);
        assert_eq!(std::mem::offset_of!(RawRequest, tags), 56);
    }

    #[cfg(feature = "direct-bridges")]
    #[test]
    fn dependency_identity_contains_actual_loaded_ffmpeg() {
        assert_ne!(unsafe { dvda_bridges::ffmpeg::avcodec_version() }, 0);
        let paths = loaded_dependencies().unwrap();
        assert!(
            paths
                .iter()
                .all(|path| path.is_absolute() && path.is_file())
        );
        assert!(paths.iter().any(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("avcodec-")
        }));
    }

    #[test]
    fn progress_scopes_map_ranges_and_never_move_backwards() {
        let mut events = Events::default();
        {
            let mut scope = ProgressScope::new(&mut events, 20, 80);
            scope.progress(1, 4);
            scope.progress(0, 4);
            scope.progress(4, 4);
        }
        assert_eq!(events.0, [(35, 100), (35, 100), (80, 100)]);
    }
}
