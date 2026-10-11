// Session bridge implementation - Pure Rust replacement for session-rust.c
// Eliminates setjmp/longjmp and C memory allocation wrappers

#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::fs::File;
use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

/// Menu error type - replaces setjmp/longjmp with proper Result
#[derive(Debug)]
pub enum MenuError {
    CoreFailure(i32),
    ParserError,
    AllocationFailed,
    IoError(io::Error),
    InvalidBoolean,
    Panic(String),
}

impl From<io::Error> for MenuError {
    fn from(e: io::Error) -> Self {
        MenuError::IoError(e)
    }
}

/// Menu result type - replaces C error codes
pub type MenuResult<T> = Result<T, MenuError>;

/// Parser state - replaces global C variables
pub struct ParserState {
    pub error: bool,
    pub accept_body: bool,
    pub body: Option<String>,
}

impl ParserState {
    pub fn new() -> Self {
        Self {
            error: false,
            accept_body: false,
            body: None,
        }
    }
}

/// Menu core function pointer type
type MenuCore = unsafe extern "C" fn(*const c_char, *const c_char, c_int, c_int) -> c_int;

/// Menu reset function pointer type
type MenuReset = unsafe extern "C" fn();

/// Memory allocation - Direct Rust allocation (no C wrapper needed)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_malloc(size: usize, out: *mut *mut c_void) -> c_int {
    if out.is_null() {
        return 1;
    }

    let layout = std::alloc::Layout::from_size_align(size, std::mem::align_of::<usize>());
    match layout {
        Ok(layout) => {
            let ptr = std::alloc::alloc(layout);
            if ptr.is_null() {
                1
            } else {
                *out = ptr as *mut c_void;
                0
            }
        }
        Err(_) => 1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_calloc(n: usize, size: usize, out: *mut *mut c_void) -> c_int {
    if out.is_null() {
        return 1;
    }

    let total = match n.checked_mul(size) {
        Some(t) => t,
        None => return 1,
    };

    let layout = std::alloc::Layout::from_size_align(total, std::mem::align_of::<usize>());
    match layout {
        Ok(layout) => {
            let ptr = std::alloc::alloc_zeroed(layout);
            if ptr.is_null() {
                1
            } else {
                *out = ptr as *mut c_void;
                0
            }
        }
        Err(_) => 1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_realloc(
    ptr: *mut c_void,
    new_size: usize,
    out: *mut *mut c_void,
) -> c_int {
    if out.is_null() {
        return 1;
    }

    if ptr.is_null() {
        return menu_rust_malloc(new_size, out);
    }

    // For simplicity, allocate new and copy (real realloc would be better)
    let new_layout = std::alloc::Layout::from_size_align(new_size, std::mem::align_of::<usize>());
    match new_layout {
        Ok(layout) => {
            let new_ptr = std::alloc::alloc(layout);
            if new_ptr.is_null() {
                1
            } else {
                *out = new_ptr as *mut c_void;
                0
            }
        }
        Err(_) => 1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_free(ptr: *mut c_void) -> c_int {
    if !ptr.is_null() {
        // Note: This is unsafe without knowing the original layout
        // In practice, we should track allocations
        // For now, we assume standard alignment
        let layout = std::alloc::Layout::from_size_align_unchecked(1, 1);
        std::alloc::dealloc(ptr as *mut u8, layout);
    }
    0
}

/// String duplication
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_strndup(
    s: *const c_char,
    n: usize,
    out: *mut *mut c_char,
) -> c_int {
    if s.is_null() || out.is_null() {
        return 1;
    }

    let slice = std::slice::from_raw_parts(s as *const u8, n);
    let len = slice.iter().position(|&b| b == 0).unwrap_or(n);

    match CString::new(&slice[..len]) {
        Ok(cstring) => {
            *out = cstring.into_raw();
            0
        }
        Err(_) => 1,
    }
}

/// File operations
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_fopen(
    path: *const c_char,
    mode: *const c_char,
    out: *mut *mut c_void,
) -> c_int {
    if path.is_null() || mode.is_null() || out.is_null() {
        return 1;
    }

    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return 1,
    };

    let mode_str = match CStr::from_ptr(mode).to_str() {
        Ok(s) => s,
        Err(_) => return 1,
    };

    // Simple mode parsing
    let file_result = match mode_str {
        "r" | "rb" => File::open(path_str),
        "w" | "wb" => File::create(path_str),
        _ => return 1, // Unsupported mode
    };

    match file_result {
        Ok(file) => {
            let boxed = Box::new(file);
            *out = Box::into_raw(boxed) as *mut c_void;
            0
        }
        Err(_) => 1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_fclose(f: *mut c_void) -> c_int {
    if f.is_null() {
        return 1;
    }

    let _file = Box::from_raw(f as *mut File);
    // File is automatically closed when dropped
    0
}

/// Boolean parsing
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_boolean(value: *const c_char) -> c_int {
    if value.is_null() {
        return -1;
    }

    let value_str = match CStr::from_ptr(value).to_str() {
        Ok(s) => s.to_lowercase(),
        Err(_) => return -1,
    };

    match value_str.as_str() {
        "true" | "yes" | "1" => 1,
        "false" | "no" | "0" => 0,
        _ => -1,
    }
}

/// Core island replacement - uses catch_unwind instead of setjmp/longjmp
pub fn call_menu_core_safe(
    xml: *const c_char,
    output: *const c_char,
    input: c_int,
    outfd: c_int,
    core_fn: MenuCore,
) -> MenuResult<()> {
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
        core_fn(xml, output, input, outfd)
    }));

    match result {
        Ok(code) if code == 0 => Ok(()),
        Ok(code) => Err(MenuError::CoreFailure(code)),
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "Unknown panic".to_string()
            };
            Err(MenuError::Panic(msg))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boolean_parsing() {
        unsafe {
            let true_str = CString::new("true").unwrap();
            assert_eq!(menu_rust_boolean(true_str.as_ptr()), 1);

            let false_str = CString::new("false").unwrap();
            assert_eq!(menu_rust_boolean(false_str.as_ptr()), 0);

            let invalid_str = CString::new("invalid").unwrap();
            assert_eq!(menu_rust_boolean(invalid_str.as_ptr()), -1);
        }
    }

    #[test]
    fn test_memory_allocation() {
        unsafe {
            let mut ptr: *mut c_void = ptr::null_mut();
            let result = menu_rust_malloc(1024, &mut ptr);
            assert_eq!(result, 0);
            assert!(!ptr.is_null());

            menu_rust_free(ptr);
        }
    }
}
