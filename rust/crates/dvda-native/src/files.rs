//! Windows file operations used by publication. Moves never replace unless requested.
use std::{ffi::OsStr, io, os::windows::ffi::OsStrExt, path::Path};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
    fn CopyFileW(source: *const u16, destination: *const u16, fail_if_exists: i32) -> i32;
    fn CompareStringOrdinal(
        left: *const u16,
        left_count: i32,
        right: *const u16,
        right_count: i32,
        ignore_case: i32,
    ) -> i32;
}

fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut result: Vec<_> = value.encode_wide().collect();
    if result.contains(&0) {
        return Err(io::Error::from_raw_os_error(123));
    }
    result.push(0);
    Ok(result)
}

pub fn move_file(source: &Path, destination: &Path, replace: bool) -> io::Result<()> {
    let source = wide(source.as_os_str())?;
    let destination = wide(destination.as_os_str())?;
    // COPY_ALLOWED retains File.Move's cross-volume behavior; no replacement by default.
    let flags = 2 | u32::from(replace);
    // SAFETY: both NUL-terminated buffers remain valid for the synchronous call.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn copy_file(source: &Path, destination: &Path, replace: bool) -> io::Result<()> {
    let source = wide(source.as_os_str())?;
    let destination = wide(destination.as_os_str())?;
    // SAFETY: both NUL-terminated buffers remain valid for the synchronous call.
    if unsafe { CopyFileW(source.as_ptr(), destination.as_ptr(), i32::from(!replace)) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn ordinal_ignore_case(left: &str, right: &str) -> bool {
    let left: Vec<_> = left.encode_utf16().collect();
    let right: Vec<_> = right.encode_utf16().collect();
    let (Ok(left_len), Ok(right_len)) = (i32::try_from(left.len()), i32::try_from(right.len()))
    else {
        return false;
    };
    // SAFETY: explicit lengths cover the two live UTF-16 buffers, with no NUL convention.
    unsafe { CompareStringOrdinal(left.as_ptr(), left_len, right.as_ptr(), right_len, 1) == 2 }
}
