//! Safe calls into the source-built C image component. No executable is launched.
use crate::media::{CallbackState, Callbacks, cancel, emit};
#[cfg(not(feature = "direct-bridges"))]
use std::ffi::{c_char, c_void};
#[cfg(not(feature = "direct-bridges"))]
use std::{
    collections::HashMap,
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use std::{ffi::CString, io, path::Path, sync::Arc};

#[cfg(not(feature = "direct-bridges"))]
type Run = unsafe extern "C" fn(
    i32,
    *const *const c_char,
    crate::media::Emit,
    crate::media::Cancel,
    *mut c_void,
) -> i32;
#[cfg(not(feature = "direct-bridges"))]
type ReadRgba = unsafe extern "C" fn(*const c_char, *mut u8, usize, *mut u32, *mut u32) -> i32;
#[cfg(not(feature = "direct-bridges"))]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}
#[cfg(feature = "direct-bridges")]
pub struct Images {}
#[cfg(not(feature = "direct-bridges"))]
pub struct Images {
    module: *mut c_void,
    run: Run,
    read_rgba: ReadRgba,
}
// The C implementation serializes its global state; callbacks remain per request.
unsafe impl Send for Images {}
unsafe impl Sync for Images {}
impl Drop for Images {
    fn drop(&mut self) {
        #[cfg(not(feature = "direct-bridges"))]
        unsafe {
            FreeLibrary(self.module);
        }
    }
}
impl Images {
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
        static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Images>>>> = OnceLock::new();
        let path = std::path::absolute(path)?;
        let mut cache = CACHE
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| io::Error::other("Image library cache lock poisoned"))?;
        if let Some(images) = cache.get(&path) {
            return Ok(Arc::clone(images));
        }
        let mut wide: Vec<_> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(io::Error::from_raw_os_error(123));
        }
        wide.push(0);
        // SAFETY: path is terminated and live; dependencies resolve locally/System32 only.
        let module = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), 0x100 | 0x800) };
        if module.is_null() {
            return Err(io::Error::last_os_error());
        }
        let entry = unsafe { GetProcAddress(module, c"dvda_image_run".as_ptr()) };
        let rgba = unsafe { GetProcAddress(module, c"dvda_image_read_rgba".as_ptr()) };
        if entry.is_null() || rgba.is_null() {
            let error = io::Error::last_os_error();
            unsafe {
                FreeLibrary(module);
            }
            return Err(error);
        }
        // SAFETY: exact ABI from tools/win-build/native/dvda-image.c.
        let images = Arc::new(Self {
            module,
            run: unsafe { std::mem::transmute::<*mut c_void, Run>(entry) },
            read_rgba: unsafe { std::mem::transmute::<*mut c_void, ReadRgba>(rgba) },
        });
        cache.insert(path, Arc::clone(&images));
        Ok(images)
    }
    #[cfg(feature = "direct-bridges")]
    pub fn read_rgba(&self, path: &Path) -> io::Result<(u32, u32, Vec<u8>)> {
        let path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| io::Error::from_raw_os_error(123))?;
        let image = dvda_bridges::direct::read_rgba(path.as_c_str()).map_err(|status| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Could not decode menu RGBA pixels (status {status})"),
            )
        })?;
        if image.width == 0 || image.width > 720 || image.height == 0 || image.height > 576 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Could not decode menu RGBA pixels",
            ));
        }
        Ok((image.width, image.height, image.pixels))
    }

    #[cfg(not(feature = "direct-bridges"))]
    pub fn read_rgba(&self, path: &Path) -> io::Result<(u32, u32, Vec<u8>)> {
        let path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| io::Error::from_raw_os_error(123))?;
        let mut pixels = vec![0; 720 * 576 * 4];
        let (mut width, mut height) = (0, 0);
        // SAFETY: caller-owned buffer follows the fixed bounds of the C image API.
        let result = unsafe {
            (self.read_rgba)(
                path.as_ptr(),
                pixels.as_mut_ptr(),
                pixels.len(),
                &mut width,
                &mut height,
            )
        };
        if result != 0 || width == 0 || width > 720 || height == 0 || height > 576 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Could not decode menu RGBA pixels",
            ));
        }
        pixels.truncate(width as usize * height as usize * 4);
        Ok((width, height, pixels))
    }
    #[cfg(feature = "direct-bridges")]
    pub fn run(&self, arguments: &[String], callbacks: &mut dyn Callbacks) -> io::Result<i32> {
        if arguments.len() > 8192 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Too many image arguments",
            ));
        }
        let strings: Vec<_> = arguments
            .iter()
            .map(|text| CString::new(text.as_str()).map_err(|_| io::Error::from_raw_os_error(123)))
            .collect::<io::Result<_>>()?;
        let mut state = CallbackState {
            callbacks,
            panicked: false,
        };
        let result = unsafe {
            dvda_bridges::direct::image_run_with_callbacks(
                &strings,
                dvda_bridges::Call {
                    emit: Some(emit),
                    cancel: Some(cancel),
                    state: (&mut state as *mut CallbackState<'_>).cast(),
                },
            )
        };
        if state.panicked {
            return Err(io::Error::other("Panic caught in image callback"));
        }
        Ok(result)
    }

    #[cfg(not(feature = "direct-bridges"))]
    /// Arguments target the project's C image API, never a command shell or subprocess.
    pub fn run(&self, arguments: &[String], callbacks: &mut dyn Callbacks) -> io::Result<i32> {
        if arguments.len() > 8192 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Too many image arguments",
            ));
        }
        let strings: Vec<_> = arguments
            .iter()
            .map(|text| CString::new(text.as_str()).map_err(|_| io::Error::from_raw_os_error(123)))
            .collect::<io::Result<_>>()?;
        let pointers: Vec<_> = strings.iter().map(|s| s.as_ptr()).collect();
        let mut state = CallbackState {
            callbacks,
            panicked: false,
        };
        // SAFETY: strings, pointer array and synchronous callback state remain live.
        let result = unsafe {
            (self.run)(
                pointers.len() as i32,
                pointers.as_ptr(),
                emit,
                cancel,
                (&mut state as *mut CallbackState<'_>).cast(),
            )
        };
        if state.panicked {
            return Err(io::Error::other("Panic caught in image callback"));
        }
        Ok(result)
    }
}
