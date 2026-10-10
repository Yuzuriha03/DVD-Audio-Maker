//! dvd-author's C ABI backed by the directly linked menu implementation.
use std::ffi::c_char;

#[cfg(all(feature = "author-loader", not(feature = "image")))]
unsafe extern "C" fn read_rgba(
    path: *const c_char,
    pixels: *mut u8,
    capacity: usize,
    width: *mut u32,
    height: *mut u32,
) -> i32 {
    unsafe { crate::author_loader::menu_read_rgba(path, pixels, capacity, width, height) }
}

#[unsafe(no_mangle)]
/// # Safety
/// Paths must be terminated UTF-8 strings valid throughout the call.
pub unsafe extern "C" fn dvda_menu_subpictures(
    xml: *const c_char,
    input: *const c_char,
    output: *const c_char,
) -> i32 {
    #[cfg(feature = "image")]
    let callback: Option<dvda_menu::ReadRgba> = Some(crate::image::dvda_image_read_rgba);
    #[cfg(all(feature = "author-loader", not(feature = "image")))]
    let callback: Option<dvda_menu::ReadRgba> = Some(read_rgba);
    #[cfg(not(any(feature = "image", feature = "author-loader")))]
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
