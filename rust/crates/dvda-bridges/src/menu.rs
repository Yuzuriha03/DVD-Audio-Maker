//! dvd-author's C ABI backed by the directly linked menu implementation.
use std::{ffi::c_char, path::Path};

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
    unsafe { dvda_menu::dvda_menu_run_spu(&raw const request) }
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
    unsafe { dvda_menu::dvda_menu_run_navigation(&raw const request) }
}

#[must_use]
pub fn subpictures(xml: &Path, input: &Path, output: &Path) -> i32 {
    let mut read_rgba = |path: &Path, buffer: &mut [u8]| {
        let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
        let image = dvda_image::codec::decode(std::io::BufReader::new(file))
            .map_err(|error| error.to_string())?;
        if buffer.len() < image.rgba().len() {
            return Err("RGBA buffer is too small".into());
        }
        buffer[..image.rgba().len()].copy_from_slice(image.rgba());
        Ok((image.dimensions().width(), image.dimensions().height()))
    };
    dvda_menu::run_spu_files(xml, input, output, &mut read_rgba).map_or(1, |()| 0)
}

#[must_use]
pub fn navigation(xml: &Path, output: &Path) -> i32 {
    let cstring =
        |path: &Path| std::ffi::CString::new(path.to_string_lossy().as_bytes()).map_err(|_| 1);
    let (Ok(xml), Ok(output)) = (cstring(xml), cstring(output)) else {
        return 1;
    };
    dvda_menu::run_navigation(&xml, &output).map_or(1, |()| 0)
}
