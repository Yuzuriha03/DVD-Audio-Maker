//! Direct static vendor boundary. The lock covers reset, callbacks, and cleanup.
pub use crate::{resources::ReadRgba, session::MenuRequest};
use std::{
    cell::Cell,
    ffi::{CStr, c_int},
    sync::Mutex,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuError(pub c_int);
static SESSION: Mutex<()> = Mutex::new(());
thread_local! { static ACTIVE: Cell<bool> = const { Cell::new(false) }; }
struct Active;
impl Drop for Active {
    fn drop(&mut self) {
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
        let Ok(_lock) = SESSION.lock() else {
            return 1;
        };
        ACTIVE.set(true);
        let _active = Active;
        if navigation {
            unsafe { run_navigation_request(request) }
        } else {
            unsafe { run_spu_request(request) }
        }
    })
    .unwrap_or(1)
}
/// # Safety
/// The request must follow the `MenuRequest` contract: valid size and non-null
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
    let xml = unsafe { CStr::from_ptr(request.xml) }
        .to_str()
        .map(std::path::PathBuf::from);
    let output = unsafe { CStr::from_ptr(request.output) }
        .to_str()
        .map(std::path::PathBuf::from);
    let (Ok(xml), Ok(output)) = (xml, output) else {
        return 1;
    };
    crate::nav::amgm::run(&xml, &output).map_or_else(
        |error| {
            eprintln!("ERR: invalid navigation XML: {error}");
            1
        },
        |()| 0,
    )
}

fn read_image_rgba(path: &std::path::Path, buffer: &mut [u8]) -> Result<(u32, u32), String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let image = dvda_image::codec::decode(std::io::BufReader::new(file))
        .map_err(|error| error.to_string())?;
    if buffer.len() < image.rgba().len() {
        return Err("RGBA buffer is too small".into());
    }
    buffer[..image.rgba().len()].copy_from_slice(image.rgba());
    Ok((image.dimensions().width(), image.dimensions().height()))
}

/// # Safety
/// The request must follow the `MenuRequest` contract: valid size, non-null UTF-8
/// paths and, when supplied, a `read_rgba` callback that obeys the image ABI.
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
    let xml = unsafe { CStr::from_ptr(request.xml) }
        .to_str()
        .map(std::path::PathBuf::from);
    let input = unsafe { CStr::from_ptr(request.input) }
        .to_str()
        .map(std::path::PathBuf::from);
    let output = unsafe { CStr::from_ptr(request.output) }
        .to_str()
        .map(std::path::PathBuf::from);
    let (Ok(xml), Ok(input), Ok(output)) = (xml, input, output) else {
        return 1;
    };
    let read_rgba = request.read_rgba;
    let mut callback = |path: &std::path::Path, buffer: &mut [u8]| -> Result<(u32, u32), String> {
        let Some(read) = read_rgba else {
            return read_image_rgba(path, buffer);
        };
        let name = path.to_str().ok_or("non-UTF-8 image path")?;
        let name = std::ffi::CString::new(name).map_err(|error| error.to_string())?;
        let (mut width, mut height) = (0u32, 0u32);
        let status = unsafe {
            read(
                name.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
                &raw mut width,
                &raw mut height,
            )
        };
        if status != 0 {
            return Err(format!("image callback failed for {}", path.display()));
        }
        Ok((width, height))
    };
    run_spu_files(&xml, &input, &output, &mut callback).map_or(1, |()| 0)
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
///
/// # Errors
/// Returns the native menu status when XML, image, MPEG, or output processing fails.
///
/// # Panics
/// Panics only if the compile-time menu request size cannot fit in `u32`.
pub unsafe fn run_spu(
    xml: &CStr,
    input: &CStr,
    output: &CStr,
    read_rgba: Option<ReadRgba>,
) -> Result<(), MenuError> {
    let request = MenuRequest {
        size: u32::try_from(size_of::<MenuRequest>()).expect("menu request fits in u32"),
        xml: xml.as_ptr(),
        input: input.as_ptr(),
        output: output.as_ptr(),
        read_rgba,
    };
    let status = unsafe { dvda_menu_run_spu(&raw const request) };
    if status == 0 {
        Ok(())
    } else {
        Err(MenuError(status))
    }
}
/// Rust subpicture pipeline entry used by tests and the differential harness.
/// The RGBA callback must fill the provided buffer for the requested image path.
///
/// # Errors
/// Returns `MenuError(1)` for invalid XML, image, MPEG, scheduling, or output failures.
#[cfg(feature = "direct-link")]
pub fn run_spu_files(
    xml: &std::path::Path,
    input: &std::path::Path,
    output: &std::path::Path,
    rgba: &mut crate::spu::RgbaReader<'_>,
) -> Result<(), MenuError> {
    crate::spu::run(xml, input, output, rgba).map_err(|error| {
        eprintln!("ERR: invalid menu XML: {error}");
        MenuError(1)
    })
}

/// Generate DVD navigation packs for an already-authored menu video.
///
/// # Errors
/// Returns the native navigation status for invalid XML, malformed menu data, or output failures.
///
/// # Panics
/// Panics only if the compile-time menu request size cannot fit in `u32`.
pub fn run_navigation(xml: &CStr, output: &CStr) -> Result<(), MenuError> {
    let request = MenuRequest {
        size: u32::try_from(size_of::<MenuRequest>()).expect("menu request fits in u32"),
        xml: xml.as_ptr(),
        input: std::ptr::null(),
        output: output.as_ptr(),
        read_rgba: None,
    };
    let status = unsafe { dvda_menu_run_navigation(&raw const request) };
    if status == 0 {
        Ok(())
    } else {
        Err(MenuError(status))
    }
}

#[cfg(test)]
mod image_tests {
    use super::read_image_rgba;

    #[test]
    fn rust_reader_preserves_alpha_and_checks_capacity() {
        let directory =
            std::env::temp_dir().join(format!("dvda menu image {}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("图层.png");
        let image = dvda_image::raster::Raster::from_rgba(
            dvda_image::geometry::Dimensions::new(2, 1).unwrap(),
            vec![255, 9, 17, 0, 0, 128, 255, 128],
        )
        .unwrap();
        dvda_image::codec::write_png(&image, std::fs::File::create(&path).unwrap()).unwrap();
        let mut small = [77; 7];
        assert!(read_image_rgba(&path, &mut small).is_err());
        assert_eq!(small, [77; 7]);
        let mut output = [77; 12];
        assert_eq!(read_image_rgba(&path, &mut output).unwrap(), (2, 1));
        assert_eq!(&output[..8], image.rgba());
        assert_eq!(&output[8..], &[77; 4]);
        std::fs::write(&path, b"invalid").unwrap();
        assert!(read_image_rgba(&path, &mut output).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
