//! Windows Compression API, using the operating system's Cabinet component.
use std::{ffi::c_void, io, ptr};
#[link(name = "cabinet")]
unsafe extern "system" {
    fn CreateCompressor(algorithm: u32, allocator: *const c_void, handle: *mut *mut c_void) -> i32;
    fn CreateDecompressor(
        algorithm: u32,
        allocator: *const c_void,
        handle: *mut *mut c_void,
    ) -> i32;
    fn CloseCompressor(handle: *mut c_void) -> i32;
    fn CloseDecompressor(handle: *mut c_void) -> i32;
    fn Compress(
        handle: *mut c_void,
        input: *const u8,
        length: usize,
        output: *mut u8,
        capacity: usize,
        written: *mut usize,
    ) -> i32;
    fn Decompress(
        handle: *mut c_void,
        input: *const u8,
        length: usize,
        output: *mut u8,
        capacity: usize,
        written: *mut usize,
    ) -> i32;
}
struct Handle {
    value: *mut c_void,
    compress: bool,
}
impl Handle {
    fn new(compress: bool) -> io::Result<Self> {
        let mut value = ptr::null_mut();
        // SAFETY: valid output pointer, default allocator, XPRESS_HUFF algorithm.
        let ok = unsafe {
            if compress {
                CreateCompressor(4, ptr::null(), &mut value)
            } else {
                CreateDecompressor(4, ptr::null(), &mut value)
            }
        };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self { value, compress })
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: exclusive ownership of a handle from the matching create call.
        unsafe {
            if self.compress {
                CloseCompressor(self.value);
            } else {
                CloseDecompressor(self.value);
            }
        }
    }
}
pub fn compress(input: &[u8]) -> io::Result<Vec<u8>> {
    let handle = Handle::new(true)?;
    let mut needed = 0;
    // SAFETY: sizing call; input is valid and a null zero-size output is supported.
    unsafe {
        Compress(
            handle.value,
            input.as_ptr(),
            input.len(),
            ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    if needed == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut output = vec![0; needed];
    // SAFETY: buffers are disjoint and both sizes match their allocations.
    if unsafe {
        Compress(
            handle.value,
            input.as_ptr(),
            input.len(),
            output.as_mut_ptr(),
            output.len(),
            &mut needed,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    output.truncate(needed);
    Ok(output)
}
pub fn decompress(input: &[u8], size: usize) -> io::Result<Vec<u8>> {
    let handle = Handle::new(false)?;
    let mut output = vec![0; size];
    let mut written = 0;
    // SAFETY: output size was bounded and verified by the archive reader.
    if unsafe {
        Decompress(
            handle.value,
            input.as_ptr(),
            input.len(),
            output.as_mut_ptr(),
            output.len(),
            &mut written,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if written != size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Runtime size mismatch",
        ));
    }
    Ok(output)
}
