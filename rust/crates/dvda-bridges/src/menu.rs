//! dvd-author's C ABI backed by the directly linked menu implementation.
use std::ffi::c_char;

#[unsafe(no_mangle)]
/// # Safety
/// Paths must be terminated UTF-8 strings valid throughout the call.
pub unsafe extern "C" fn dvda_menu_subpictures(
    xml: *const c_char,
    input: *const c_char,
    output: *const c_char,
) -> i32 {
    let callback = None;
    let request = dvda_menu::MenuRequest {
        size: std::mem::size_of::<dvda_menu::MenuRequest>() as u32,
        xml,
        input,
        output,
        read_rgba: callback,
    };
    unsafe { dvda_menu::dvda_menu_run_spu(&request) }
}

#[unsafe(no_mangle)]
/// # Safety
/// Paths must be terminated UTF-8 strings valid throughout the call.
pub unsafe extern "C" fn dvda_menu_navigation(xml: *const c_char, output: *const c_char) -> i32 {
    let request = dvda_menu::MenuRequest {
        size: std::mem::size_of::<dvda_menu::MenuRequest>() as u32,
        xml,
        input: std::ptr::null(),
        output,
        read_rgba: None,
    };
    unsafe { dvda_menu::dvda_menu_run_navigation(&request) }
}
