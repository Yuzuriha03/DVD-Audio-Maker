//! Explicit external-oracle access; production image operations use dvda-image.
use crate::library::Library;
use crate::media::{CallbackState, Callbacks, cancel, emit};
use sha2::{Digest, Sha256};
use std::ffi::{c_char, c_void};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use std::{ffi::CString, io, path::Path, sync::Arc};

fn validate_oracle(bytes: &[u8]) -> io::Result<()> {
    let digest = Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut text, byte| {
            let _ = std::fmt::Write::write_fmt(&mut text, format_args!("{byte:02x}"));
            text
        });
    if digest != "ac8afb08c95b980a16e0212538403058b1b71340c78159dc19d6642cacedf1db" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Image oracle SHA256 does not match the frozen migration reference",
        ));
    }
    Ok(())
}

type Run = unsafe extern "C" fn(
    i32,
    *const *const c_char,
    unsafe extern "C" fn(*mut c_void, i32, *const c_char),
    unsafe extern "C" fn(*mut c_void) -> i32,
    *mut c_void,
) -> i32;
type ReadRgba = unsafe extern "C" fn(*const c_char, *mut u8, usize, *mut u32, *mut u32) -> i32;
pub struct Images {
    _module: Library,
    run: Run,
    read_rgba: ReadRgba,
}
// The C implementation serializes its global state; callbacks remain per request.
unsafe impl Send for Images {}
unsafe impl Sync for Images {}
impl Images {
    /// Validate, load, and cache the external image oracle.
    ///
    /// # Errors
    /// Returns an error for invalid paths, failed validation, missing symbols, or cache-lock failures.
    pub fn load(path: &Path) -> io::Result<Arc<Self>> {
        static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Images>>>> = OnceLock::new();
        let path = std::path::absolute(path)?;
        validate_oracle(&std::fs::read(&path)?)?;
        let mut cache = CACHE
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| io::Error::other("Image library cache lock poisoned"))?;
        if let Some(images) = cache.get(&path) {
            return Ok(Arc::clone(images));
        }
        let module = Library::load(&path)?;
        // SAFETY: exact ABIs from tools/win-build/native/dvda-image.c.
        let run: Run = unsafe { module.symbol(c"dvda_image_run") }?;
        // SAFETY: exact ABIs from tools/win-build/native/dvda-image.c.
        let read_rgba: ReadRgba = unsafe { module.symbol(c"dvda_image_read_rgba") }?;
        let images = Arc::new(Self {
            _module: module,
            run,
            read_rgba,
        });
        cache.insert(path, Arc::clone(&images));
        Ok(images)
    }
    /// Decode an image into the bounded RGBA buffer used by menu authoring.
    ///
    /// # Errors
    /// Rejects invalid paths, decode failures, and dimensions outside the DVD menu bounds.
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
                &raw mut width,
                &raw mut height,
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
    /// Arguments target the explicit external image oracle, never a command shell.
    ///
    /// # Errors
    /// Rejects excessive arguments and embedded NUL values, and propagates process or callback failures.
    ///
    /// # Panics
    /// Panics only if the validated argument count cannot fit in `i32`.
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
                i32::try_from(pointers.len()).expect("image argument count was bounded"),
                pointers.as_ptr(),
                emit,
                cancel,
                (&raw mut state).cast(),
            )
        };
        if state.panicked {
            return Err(io::Error::other("Panic caught in image callback"));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_reference_oracle() {
        for bytes in [b"".as_slice(), b"not the frozen image oracle".as_slice()] {
            assert_eq!(
                validate_oracle(bytes).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }

    #[test]
    fn load_rejects_invalid_library_before_loading() {
        let path =
            std::env::temp_dir().join(format!("dvda-image-invalid-{}.dll", std::process::id()));
        std::fs::write(&path, b"not a DLL").unwrap();
        let result = Images::load(&path);
        std::fs::remove_file(path).unwrap();
        assert!(matches!(result, Err(error) if error.kind() == io::ErrorKind::InvalidData));
    }
}
