use crate::win::*;
use std::{
    ffi::{c_char, c_void},
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
type Command = unsafe extern "C" fn(*const c_char) -> i32;
type Y4m = unsafe extern "C" fn(*const c_char, *const c_char, *const c_char, *const c_char) -> i32;
type Rgba = unsafe extern "C" fn(*const c_char, *mut u8, usize, *mut u32, *mut u32) -> i32;
struct Image {
    command: Command,
    y4m: Y4m,
    rgba: Rgba,
}
static IMAGE: OnceLock<Image> = OnceLock::new();
static IMAGE_LOAD: Mutex<()> = Mutex::new(());
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(path: *const u16, file: Handle, flags: u32) -> Handle;
    fn FreeLibrary(module: Handle) -> i32;
}
/// # Safety
/// Calls the Win32 module-file query with a null module handle and a buffer it owns.
/// There are no pointer obligations; the function is unsafe only because it calls Win32.
unsafe fn directory() -> Option<PathBuf> {
    unsafe {
        let mut path = vec![0u16; 32768];
        let n = GetModuleFileNameW(std::ptr::null_mut(), path.as_mut_ptr(), 32768);
        if n == 0 || n == 32768 {
            return None;
        }
        path.truncate(n as usize);
        let p = PathBuf::from(String::from_utf16_lossy(&path));
        p.parent().map(PathBuf::from)
    }
}
/// # Safety
/// `path` is converted to a NUL-terminated wide string that lives for the call.
/// The returned module is not freed here. The caller must pair a non-NULL result with
/// `FreeLibrary` and must not use any procedure address taken from it afterwards.
unsafe fn load(path: &std::path::Path) -> Handle {
    unsafe {
        LoadLibraryExW(
            wide(&path.to_string_lossy()).as_ptr(),
            std::ptr::null_mut(),
            0x1100,
        )
    }
}
/// # Safety
/// The runtime `dvda-image.dll` must export `dvda_image_command`, `dvda_image_write_y4m`
/// and `dvda_image_read_rgba` with the signatures of `Command`, `Y4m` and `Rgba`.
/// A successful load is kept for the life of the process, so the returned reference is
/// `'static`. A failed load caches nothing and may be retried.
unsafe fn image() -> Result<&'static Image, ()> {
    unsafe {
        if let Some(image) = IMAGE.get() {
            return Ok(image);
        }
        // Failed loads must remain retryable, as the author can install its runtime later.
        let _guard = IMAGE_LOAD.lock().map_err(|_| ())?;
        if let Some(image) = IMAGE.get() {
            return Ok(image);
        }
        let loaded = (|| {
            let dir = directory().ok_or(())?;
            let mut path = dir.join("dvda-image.dll");
            if !path.is_file() {
                path = dir
                    .parent()
                    .ok_or(())?
                    .join("image-native")
                    .join("dvda-image.dll")
            };
            let module = load(&path);
            if module.is_null() {
                eprintln!(
                    "[ERR] Cannot load bundled x64 image runtime (Windows error {}).",
                    GetLastError()
                );
                return Err(());
            }
            let command = GetProcAddress(module, c"dvda_image_command".as_ptr());
            let y4m = GetProcAddress(module, c"dvda_image_write_y4m".as_ptr());
            let rgba = GetProcAddress(module, c"dvda_image_read_rgba".as_ptr());
            if command.is_null() || y4m.is_null() || rgba.is_null() {
                FreeLibrary(module);
                return Err(());
            }
            Ok(Image {
                command: std::mem::transmute::<*mut c_void, Command>(command),
                y4m: std::mem::transmute::<*mut c_void, Y4m>(y4m),
                rgba: std::mem::transmute::<*mut c_void, Rgba>(rgba),
            })
        })()?;
        IMAGE.set(loaded).map_err(|_| ())?;
        IMAGE.get().ok_or(())
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
pub unsafe extern "C" fn dvda_image_command(args: *const c_char) -> i32 {
    unsafe { image().map_or(-1, |i| (i.command)(args)) }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
pub unsafe extern "C" fn dvda_image_write_y4m(
    input: *const c_char,
    output: *const c_char,
    rate: *const c_char,
    aspect: *const c_char,
) -> i32 {
    unsafe { image().map_or(-1, |i| (i.y4m)(input, output, rate, aspect)) }
}
#[cfg(not(feature = "menu"))]
#[repr(C)]
struct Request {
    size: u32,
    xml: *const c_char,
    input: *const c_char,
    output: *const c_char,
    rgba: Rgba,
}
#[cfg(feature = "menu")]
pub(crate) fn menu_read_rgba(
    path: *const c_char,
    pixels: *mut u8,
    capacity: usize,
    width: *mut u32,
    height: *mut u32,
) -> i32 {
    image().map_or(-1, |i| (i.rgba)(path, pixels, capacity, width, height))
}

#[cfg(not(feature = "menu"))]
/// # Safety
/// `xml` and `output` must be valid NUL-terminated strings that stay live for the call.
/// `input` may be NULL. The module `name` must export `dvda_menu_run` with the signature
/// `unsafe extern "C" fn(*const Request)`. It is freed before this function returns.
unsafe fn menu(name: &str, xml: *const c_char, input: *const c_char, output: *const c_char) -> i32 {
    unsafe {
        let Ok(image) = image() else { return -1 };
        let Some(dir) = directory() else { return -1 };
        let module = load(&dir.join(name));
        if module.is_null() {
            eprintln!("[ERR] Cannot load menu library ({}).", GetLastError());
            return -1;
        }
        let entry = GetProcAddress(module, c"dvda_menu_run".as_ptr());
        let result = if entry.is_null() {
            -1
        } else {
            let run: unsafe extern "C" fn(*const Request) -> i32 = std::mem::transmute(entry);
            run(&Request {
                size: std::mem::size_of::<Request>() as u32,
                xml,
                input,
                output,
                rgba: image.rgba,
            })
        };
        FreeLibrary(module);
        result
    }
}
#[cfg(not(feature = "menu"))]
#[unsafe(no_mangle)]
/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
pub unsafe extern "C" fn dvda_menu_subpictures(
    xml: *const c_char,
    input: *const c_char,
    output: *const c_char,
) -> i32 {
    unsafe { menu("dvda-menu-spu.dll", xml, input, output) }
}
#[cfg(not(feature = "menu"))]
#[unsafe(no_mangle)]
/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
pub unsafe extern "C" fn dvda_menu_navigation(xml: *const c_char, output: *const c_char) -> i32 {
    unsafe { menu("dvda-menu-nav.dll", xml, std::ptr::null(), output) }
}
