//! C ABI for the Rust ISO9660/UDF writer used by the existing author.

use std::{
    ffi::{CStr, c_char, c_int},
    io::{self, Write},
    panic::catch_unwind,
    path::Path,
};

fn report(message: impl std::fmt::Display) {
    // Reporting an I/O error must not panic across the C ABI either.
    let _ = writeln!(io::stderr().lock(), "[ERR] ISO writer: {message}");
}

unsafe fn argument<'a>(value: *const c_char, name: &str) -> io::Result<&'a str> {
    if value.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} is NULL"),
        ));
    }
    unsafe { CStr::from_ptr(value) }.to_str().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} must be valid UTF-8"),
        )
    })
}

/// Write an ISO9660/UDF image from a DVD directory tree.
///
/// Returns `0` on success and `-1` on an invalid argument, I/O error, or caught
/// Rust panic. A NULL volume identifier selects the writer's default label.
///
/// # Safety
/// Every non-NULL pointer must address a readable NUL-terminated string that
/// stays alive and is not modified for the duration of this call. Paths and the
/// optional volume identifier must use UTF-8. Source and destination are
/// required; their NULL values are rejected.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dvda_iso_write(
    source_directory: *const c_char,
    destination: *const c_char,
    volume_identifier: *const c_char,
) -> c_int {
    let result = catch_unwind(|| {
        let source = unsafe { argument(source_directory, "source directory") }?;
        let destination = unsafe { argument(destination, "destination") }?;
        let label = if volume_identifier.is_null() {
            None
        } else {
            Some(unsafe { argument(volume_identifier, "volume identifier") }?)
        };
        dvda_author::iso::write(Path::new(source), Path::new(destination), label)
    });
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            report(error);
            -1
        }
        Err(_) => {
            report("Rust panic while writing image");
            -1
        }
    }
}
