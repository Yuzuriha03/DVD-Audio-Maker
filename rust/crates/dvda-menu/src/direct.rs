//! Direct static vendor boundary. The lock covers reset, callbacks, and cleanup.
use std::{
    cell::Cell,
    ffi::{CStr, c_char, c_int},
    sync::Mutex,
};
pub type ReadRgba =
    unsafe extern "C" fn(*const c_char, *mut u8, usize, *mut u32, *mut u32) -> c_int;
#[repr(C)]
pub struct MenuRequest {
    pub size: u32,
    pub xml: *const c_char,
    pub input: *const c_char,
    pub output: *const c_char,
    pub read_rgba: Option<ReadRgba>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuError(pub c_int);
static SESSION: Mutex<()> = Mutex::new(());
thread_local! { static NAV: Cell<bool> = const { Cell::new(false) }; static ACTIVE: Cell<bool> = const { Cell::new(false) }; }
unsafe extern "C" {
    fn dvda_spu_dvda_menu_run(request: *const MenuRequest) -> c_int;
    fn dvda_nav_dvda_menu_run(request: *const MenuRequest) -> c_int;
}
#[cfg(not(test))]
unsafe extern "C" {
    fn dvda_spu_menu_callback(callback: Option<unsafe extern "C" fn()>) -> c_int;
    fn dvda_nav_menu_callback(callback: Option<unsafe extern "C" fn()>) -> c_int;
    fn dvda_spu_menu_attribute(
        callback: Option<unsafe extern "C" fn(*const c_char)>,
        value: *const c_char,
    ) -> c_int;
    fn dvda_nav_menu_attribute(
        callback: Option<unsafe extern "C" fn(*const c_char)>,
        value: *const c_char,
    ) -> c_int;
    fn dvda_spu_menu_body(value: *const c_char) -> c_int;
    fn dvda_nav_menu_body(value: *const c_char) -> c_int;
    fn dvda_spu_menu_accepts_body() -> c_int;
    fn dvda_nav_menu_accepts_body() -> c_int;
}
#[cfg(not(test))]
pub(super) unsafe fn menu_callback(callback: Option<unsafe extern "C" fn()>) -> c_int {
    unsafe {
        if NAV.get() {
            dvda_nav_menu_callback(callback)
        } else {
            dvda_spu_menu_callback(callback)
        }
    }
}
#[cfg(not(test))]
pub(super) unsafe fn menu_attribute(
    callback: Option<unsafe extern "C" fn(*const c_char)>,
    value: *const c_char,
) -> c_int {
    unsafe {
        if NAV.get() {
            dvda_nav_menu_attribute(callback, value)
        } else {
            dvda_spu_menu_attribute(callback, value)
        }
    }
}
#[cfg(not(test))]
pub(super) unsafe fn menu_body(value: *const c_char) -> c_int {
    unsafe {
        if NAV.get() {
            dvda_nav_menu_body(value)
        } else {
            dvda_spu_menu_body(value)
        }
    }
}
#[cfg(not(test))]
pub(super) unsafe fn menu_accepts_body() -> c_int {
    unsafe {
        if NAV.get() {
            dvda_nav_menu_accepts_body()
        } else {
            dvda_spu_menu_accepts_body()
        }
    }
}
struct Active;
impl Drop for Active {
    fn drop(&mut self) {
        NAV.set(false);
        ACTIVE.set(false);
    }
}
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
        NAV.set(navigation);
        unsafe {
            if navigation {
                dvda_nav_dvda_menu_run(request)
            } else {
                dvda_spu_dvda_menu_run(request)
            }
        }
    })
    .unwrap_or(1)
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
