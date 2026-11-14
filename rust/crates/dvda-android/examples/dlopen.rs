use std::ffi::{c_char, c_void, CString};

#[link(name = "dl")]
extern "C" {
    fn dlopen(name: *const c_char, flag: i32) -> *mut c_void;
    fn dlerror() -> *mut c_char;
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: dlopen <so>");
    let name = CString::new(path).unwrap();
    // SAFETY: name is a valid NUL-terminated string.
    let handle = unsafe { dlopen(name.as_ptr(), 1 | 2) };
    if handle.is_null() {
        let error = unsafe { dlerror() };
        let message = if error.is_null() {
            "unknown".to_string()
        } else {
            unsafe { std::ffi::CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned()
        };
        println!("dlopen failed: {message}");
        std::process::exit(1);
    }
    println!("dlopen ok");
}
