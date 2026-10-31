//! Direct static vendor boundary. The lock covers reset, callbacks, and cleanup.
pub use crate::{resources::ReadRgba, session::MenuRequest};
use std::{
    cell::Cell,
    ffi::{CStr, c_char, c_int},
    sync::Mutex,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuError(pub c_int);
static SESSION: Mutex<()> = Mutex::new(());
thread_local! { static NAV: Cell<bool> = const { Cell::new(false) }; static ACTIVE: Cell<bool> = const { Cell::new(false) }; }
#[allow(dead_code)] // C bridge symbols are kept until the vendor navigation library is deleted
unsafe extern "C" {
    fn dvda_nav_dvda_menu_run(request: *const MenuRequest) -> c_int;
    fn dvda_nav_menu_callback(callback: Option<unsafe extern "C" fn()>) -> c_int;
    fn dvda_nav_menu_attribute(
        callback: Option<unsafe extern "C" fn(*const c_char)>,
        value: *const c_char,
    ) -> c_int;
    fn dvda_nav_menu_body(value: *const c_char) -> c_int;
    fn dvda_nav_menu_accepts_body() -> c_int;
}
/// # Safety
/// `callback`, when present, must not unwind, longjmp, or retain pointers, as for
/// the vendor menu API.
#[cfg(not(test))]
pub(super) unsafe fn menu_callback(callback: Option<unsafe extern "C" fn()>) -> c_int {
    unsafe { dvda_nav_menu_callback(callback) }
}
/// # Safety
/// `value` must be null or a NUL-terminated string valid for the call. `callback`,
/// when present, has the same obligations as in `menu_callback`.
#[cfg(not(test))]
pub(super) unsafe fn menu_attribute(
    callback: Option<unsafe extern "C" fn(*const c_char)>,
    value: *const c_char,
) -> c_int {
    unsafe { dvda_nav_menu_attribute(callback, value) }
}
/// # Safety
/// `value` must be null or a NUL-terminated string valid for the call.
#[cfg(not(test))]
pub(super) unsafe fn menu_body(value: *const c_char) -> c_int {
    unsafe { dvda_nav_menu_body(value) }
}
/// # Safety
/// Takes no pointers. It queries the vendor library, so it is only meaningful inside
/// a menu run that dispatches to that library (the same session as `run`).
#[cfg(not(test))]
pub(super) unsafe fn menu_accepts_body() -> c_int {
    unsafe { dvda_nav_menu_accepts_body() }
}
struct Active;
impl Drop for Active {
    fn drop(&mut self) {
        NAV.set(false);
        ACTIVE.set(false);
    }
}
/// # Safety
/// `request` must satisfy the contract of `dvda_menu_run_spu` when `navigation` is
/// false, and of `dvda_menu_run_navigation` when it is true.
unsafe fn run(request: *const MenuRequest, navigation: bool) -> c_int {
    std::panic::catch_unwind(|| {
        if ACTIVE.get() {
            return 1;
        }
        let _lock = match SESSION.lock() {
            Ok(lock) => lock,
            Err(_) => return 1,
        };
        ACTIVE.set(true);
        let _active = Active;
        if navigation {
            NAV.set(true);
            unsafe { run_navigation_request(request) }
        } else {
            unsafe { run_spu_request(request) }
        }
    })
    .unwrap_or(1)
}
/// # Safety
/// The request must follow the MenuRequest contract: valid size and non-null
/// UTF-8 xml/output paths.
unsafe fn run_navigation_request(request: *const MenuRequest) -> c_int {
    let Some(request) = (unsafe { request.as_ref() }) else {
        return 1;
    };
    if request.size as usize != size_of::<MenuRequest>()
        || request.xml.is_null()
        || request.output.is_null()
    {
        return 1;
    }
    let xml = unsafe { CStr::from_ptr(request.xml) }.to_str().map(std::path::PathBuf::from);
    let output = unsafe { CStr::from_ptr(request.output) }.to_str().map(std::path::PathBuf::from);
    let (Ok(xml), Ok(output)) = (xml, output) else {
        return 1;
    };
    crate::nav::amgm::run(&xml, &output).map_or_else(
        |error| {
            eprintln!("ERR: invalid navigation XML: {error}");
            1
        },
        |_| 0,
    )
}

/// # Safety
/// The request must follow the MenuRequest contract: valid size, non-null UTF-8
/// paths and a read_rgba callback that obeys the image ABI.
unsafe fn run_spu_request(request: *const MenuRequest) -> c_int {
    let Some(request) = (unsafe { request.as_ref() }) else {
        return 1;
    };
    if request.size as usize != size_of::<MenuRequest>()
        || request.xml.is_null()
        || request.input.is_null()
        || request.output.is_null()
    {
        return 1;
    }
    let xml = unsafe { CStr::from_ptr(request.xml) }.to_str().map(std::path::PathBuf::from);
    let input = unsafe { CStr::from_ptr(request.input) }.to_str().map(std::path::PathBuf::from);
    let output = unsafe { CStr::from_ptr(request.output) }.to_str().map(std::path::PathBuf::from);
    let (Ok(xml), Ok(input), Ok(output)) = (xml, input, output) else {
        return 1;
    };
    let read_rgba = request.read_rgba;
    let mut callback = |path: &std::path::Path, buffer: &mut [u8]| -> Result<(u32, u32), String> {
        let Some(read) = read_rgba else {
            return Err("missing RGBA callback".into());
        };
        let name = path.to_str().ok_or("non-UTF-8 image path")?;
        let name = std::ffi::CString::new(name).map_err(|error| error.to_string())?;
        let (mut width, mut height) = (0u32, 0u32);
        let status = unsafe {
            read(name.as_ptr(), buffer.as_mut_ptr(), buffer.len(), &mut width, &mut height)
        };
        if status != 0 {
            return Err(format!("image callback failed for {}", path.display()));
        }
        Ok((width, height))
    };
    run_spu_files(&xml, &input, &output, &mut callback).map_or(1, |_| 0)
}

/// # Safety
/// Request and UTF-8 C strings must remain valid for the call. The callback must
/// not unwind, longjmp, or retain pointers. On failure discard partial outputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dvda_menu_run_spu(request: *const MenuRequest) -> c_int {
    unsafe { run(request, false) }
}
/// # Safety
/// Same lifetime and failure contract as `dvda_menu_run_spu`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dvda_menu_run_navigation(request: *const MenuRequest) -> c_int {
    unsafe { run(request, true) }
}
/// # Safety
/// `read_rgba`, when present, must accept the image ABI, obey its buffer capacity,
/// and not unwind, longjmp, or retain any supplied pointer. Paths must be UTF-8.
/// Discard partial outputs when this call fails.
pub unsafe fn run_spu(
    xml: &CStr,
    input: &CStr,
    output: &CStr,
    read_rgba: Option<ReadRgba>,
) -> Result<(), MenuError> {
    let request = MenuRequest {
        size: size_of::<MenuRequest>() as u32,
        xml: xml.as_ptr(),
        input: input.as_ptr(),
        output: output.as_ptr(),
        read_rgba,
    };
    let status = unsafe { dvda_menu_run_spu(&request) };
    if status == 0 {
        Ok(())
    } else {
        Err(MenuError(status))
    }
}
/// Rust subpicture pipeline entry used by tests and the differential harness.
/// The RGBA callback must fill the provided buffer for the requested image path.
#[cfg(feature = "direct-link")]
pub fn run_spu_files(
    xml: &std::path::Path,
    input: &std::path::Path,
    output: &std::path::Path,
    rgba: &mut dyn FnMut(&std::path::Path, &mut [u8]) -> Result<(u32, u32), String>,
) -> Result<(), MenuError> {
    crate::spu::run(xml, input, output, rgba).map_err(|error| {
        eprintln!("ERR: invalid menu XML: {error}");
        MenuError(1)
    })
}

pub fn run_navigation(xml: &CStr, output: &CStr) -> Result<(), MenuError> {
    let request = MenuRequest {
        size: size_of::<MenuRequest>() as u32,
        xml: xml.as_ptr(),
        input: std::ptr::null(),
        output: output.as_ptr(),
        read_rgba: None,
    };
    let status = unsafe { dvda_menu_run_navigation(&request) };
    if status == 0 {
        Ok(())
    } else {
        Err(MenuError(status))
    }
}
