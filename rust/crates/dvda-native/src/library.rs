//! Owned Windows dynamic-library handles for native compatibility oracles.
use std::{
    ffi::{CStr, c_char, c_void},
    io,
    os::windows::ffi::OsStrExt,
    path::Path,
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

pub(crate) struct Library {
    handle: *mut c_void,
}

impl Library {
    /// Load a library while restricting dependencies to its directory and System32.
    pub(crate) fn load(path: &Path) -> io::Result<Self> {
        let mut wide: Vec<_> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(io::Error::from_raw_os_error(123));
        }
        wide.push(0);
        // SAFETY: path is live, NUL-terminated UTF-16; the flags preserve the
        // existing dependency-resolution policy.
        let handle = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), 0x100 | 0x800) };
        if handle.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self { handle })
        }
    }

    /// Resolve a function pointer with the exact ABI required by its caller.
    ///
    /// # Safety
    /// `T` must be a function-pointer type matching the exported symbol's C ABI.
    pub(crate) unsafe fn symbol<T: Copy>(&self, name: &CStr) -> io::Result<T> {
        assert_eq!(
            size_of::<T>(),
            size_of::<*mut c_void>(),
            "dynamic symbols must be function pointers"
        );
        // SAFETY: the owned module is loaded and name is NUL-terminated.
        let address = unsafe { GetProcAddress(self.handle, name.as_ptr()) };
        if address.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the size check above and the caller's ABI contract make this
        // the same function-pointer conversion formerly done at each call site.
        Ok(unsafe { std::mem::transmute_copy(&address) })
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: this object uniquely owns a successful LoadLibraryExW handle.
        unsafe {
            FreeLibrary(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_nul_and_missing_paths_without_leaking_handles() {
        let nul = Library::load(Path::new("invalid\0library.dll"));
        assert_eq!(nul.err().and_then(|e| e.raw_os_error()), Some(123));

        let missing = std::env::temp_dir().join(format!(
            "dvda-native-missing-library-{}.dll",
            std::process::id()
        ));
        assert!(Library::load(&missing).is_err());
    }
}
