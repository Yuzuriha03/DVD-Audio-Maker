//! Typed API for the project's in-process C media implementation.
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CStr, CString, c_char, c_void},
    io,
    panic::{AssertUnwindSafe, catch_unwind},
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
    #[serde(default = "default_dither")]
    pub dither: bool,
    pub compression: u32,
    pub cover: bool,
    pub input: String,
    pub output: Option<String>,
    pub tags: Vec<(String, String)>,
}

#[must_use]
pub const fn default_dither() -> bool {
    true
}

impl Request {
    /// Validate the media request before passing it to the native bridge.
    ///
    /// # Errors
    /// Rejects invalid geometry, operation codes, inconsistent outputs, and invalid tag lists.
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
    if let Ok(value) = catch_unwind(AssertUnwindSafe(|| state.callbacks.cancelled())) {
        i32::from(value)
    } else {
        state.panicked = true;
        1
    }
}

/// Return the native media module and its resolved dependency paths.
///
/// # Errors
/// Propagates module enumeration and Windows path-query failures.
#[cfg(windows)]
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
                &raw mut needed,
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
        let capacity = u32::try_from(path.len())
            .map_err(|_| io::Error::other("Module path buffer exceeds u32"))?;
        let length = unsafe { GetModuleFileNameW(module, path.as_mut_ptr(), capacity) } as usize;
        if length == 0 {
            let error = io::Error::last_os_error();
            // A delay-loaded DLL can unload after the module snapshot was captured.
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

#[cfg(not(windows))]
/// Android/Linux has no delay-load DLL list; the static FFmpeg image is already linked.
///
/// # Errors
/// Never fails on this platform.
pub fn loaded_dependencies() -> io::Result<Vec<std::path::PathBuf>> {
    Ok(Vec::new())
}

pub struct Media;
impl Media {
    #[must_use]
    pub fn linked() -> Arc<Self> {
        Arc::new(Self)
    }

    /// Execute a validated media request with synchronous callbacks.
    ///
    /// # Errors
    /// Propagates request validation, path conversion, media processing, and callback failures.
    pub fn run(&self, request: &Request, callbacks: &mut dyn Callbacks) -> io::Result<i32> {
        fn string(text: &str) -> io::Result<CString> {
            CString::new(text).map_err(|_| io::Error::from_raw_os_error(123))
        }
        request.validate()?;
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
        direct.dither = request.dither;
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
                state: (&raw mut state).cast(),
            })
        };
        if state.panicked {
            return Err(io::Error::other("Panic caught in media callback"));
        }
        Ok(result)
    }

    /// Execute a media conversion and stream normalized PCM into a sink.
    ///
    /// # Errors
    /// Propagates request validation, sink initialization, PCM conversion, and callback failures.
    pub fn run_pcm(
        &self,
        request: &Request,
        callbacks: &mut dyn Callbacks,
        sink: &mut dyn dvda_bridges::PcmSink,
    ) -> io::Result<i32> {
        request.validate()?;
        let mut direct =
            dvda_bridges::direct::MediaRequest::new(request.operation as u32, &request.input, "")
                .map_err(|_| io::Error::from_raw_os_error(123))?;
        direct.output = None;
        direct.rate = request.rate;
        direct.bits = request.bits;
        direct.output_format = request.output_format as u32;
        direct.soxr = request.soxr;
        direct.dither = request.dither;
        let mut state = CallbackState {
            callbacks,
            panicked: false,
        };
        let result = unsafe {
            direct.run_pcm_with_callbacks(
                dvda_bridges::Call {
                    emit: Some(emit),
                    cancel: Some(cancel),
                    state: (&raw mut state).cast(),
                },
                sink,
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
    fn dependency_identity_contains_actual_loaded_ffmpeg() {
        assert_ne!(unsafe { dvda_bridges::ffmpeg::avcodec_version() }, 0);
        let paths = loaded_dependencies().unwrap();
        assert!(
            paths
                .iter()
                .all(|path| path.is_absolute() && path.is_file())
        );
        let loaded = paths.iter().any(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("avcodec-")
        });
        assert_eq!(loaded, !dvda_bridges::ffmpeg::STATIC_LINKED);
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
