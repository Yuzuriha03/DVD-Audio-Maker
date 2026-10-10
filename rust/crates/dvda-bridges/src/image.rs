#![allow(unsafe_op_in_unsafe_fn)]
use std::{
    ffi::{CStr, CString, c_char, c_int},
    fs::File,
    io::Write,
    path::Path,
    sync::{Mutex, OnceLock},
};
#[repr(C)]
struct Wand {
    _private: [u8; 0],
}
type Size = usize;
#[link(name = "MagickWand-7.Q16HDRI")]
unsafe extern "C" {
    fn MagickCoreGenesis(path: *const c_char, signals: c_int);
    fn NewMagickWand() -> *mut Wand;
    fn DestroyMagickWand(w: *mut Wand) -> *mut Wand;
    fn MagickReadImage(w: *mut Wand, path: *const c_char) -> c_int;
    fn MagickGetImageWidth(w: *mut Wand) -> Size;
    fn MagickGetImageHeight(w: *mut Wand) -> Size;
    fn MagickExportImagePixels(
        w: *mut Wand,
        x: Size,
        y: Size,
        width: Size,
        height: Size,
        map: *const c_char,
        storage: c_int,
        pixels: *mut u8,
    ) -> c_int;
    fn MagickTransformImageColorspace(w: *mut Wand, colorspace: c_int) -> c_int;
}
const CHAR_PIXEL: c_int = 1;
const SRGB_COLORSPACE: c_int = 23;
static GATE: OnceLock<Mutex<()>> = OnceLock::new();
unsafe extern "C" {
    fn _putenv_s(name: *const c_char, value: *const c_char) -> c_int;
}
fn init() {
    GATE.get_or_init(|| {
        unsafe {
            let mut module = std::ptr::null_mut();
            let mut path = vec![0u16; 32768];
            let found =
                crate::win::GetModuleHandleExW(6, init as *const () as *const u16, &mut module);
            let len = if found != 0 {
                crate::win::GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32)
            } else {
                0
            };
            path.truncate(len as usize);
            let filename = String::from_utf16_lossy(&path);
            let configuration =
                configuration_directory(&filename, std::env::var_os("DVDA_IMAGE_NATIVE_DIR"));
            if let Some(directory) = configuration.as_deref() {
                let directory = directory.to_string_lossy();
                crate::win::SetEnvironmentVariableW(
                    crate::win::wide("MAGICK_CONFIGURE_PATH").as_ptr(),
                    crate::win::wide(&directory).as_ptr(),
                );
                crate::win::SetEnvironmentVariableW(
                    crate::win::wide("MAGICK_THREAD_LIMIT").as_ptr(),
                    crate::win::wide("1").as_ptr(),
                );
                _putenv_s(
                    c"MAGICK_CONFIGURE_PATH".as_ptr(),
                    c(&directory).unwrap().as_ptr(),
                );
                _putenv_s(c"MAGICK_THREAD_LIMIT".as_ptr(), c"1".as_ptr());
            }
            MagickCoreGenesis(c(&filename).unwrap().as_ptr(), 0);
            SetWarningHandler(Some(report));
            SetErrorHandler(Some(report));
            SetFatalErrorHandler(Some(report));
        }
        Mutex::new(())
    });
}
fn configuration_directory(
    module_filename: &str,
    runtime: Option<std::ffi::OsString>,
) -> Option<std::path::PathBuf> {
    // A directly linked bridge lives in the EXE, but its font configuration lives in the runtime cache.
    runtime
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| Path::new(module_filename).parent().map(Path::to_path_buf))
}

#[cfg(test)]
mod configuration_tests {
    use super::*;

    #[test]
    fn linked_images_use_runtime_configuration_not_executable_directory() {
        assert_eq!(
            configuration_directory(
                r"C:\Program Files\DVD-Audio-Maker\DVD-Audio-Maker.exe",
                Some(r"C:\缓存 目录\runtime\verified".into()),
            ),
            Some(r"C:\缓存 目录\runtime\verified".into())
        );
    }

    #[test]
    fn standalone_bridge_keeps_module_local_configuration() {
        for runtime in [None, Some("".into())] {
            assert_eq!(
                configuration_directory(r"C:\images\dvda-image.dll", runtime),
                Some(r"C:\images".into())
            );
        }
    }
}

fn c(text: &str) -> Result<CString, ()> {
    CString::new(text).map_err(|_| ())
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
pub unsafe extern "C" fn dvda_image_read_rgba(
    input: *const c_char,
    rgba: *mut u8,
    capacity: usize,
    width: *mut u32,
    height: *mut u32,
) -> c_int {
    if input.is_null() || rgba.is_null() || width.is_null() || height.is_null() {
        return -1;
    }
    let Ok(_source) = CStr::from_ptr(input).to_str() else {
        return -1;
    };
    init();
    let _gate = GATE.get().unwrap().lock().unwrap();
    let w = NewMagickWand();
    if w.is_null() {
        return -1;
    }
    let ok = MagickReadImage(w, input) != 0;
    let x = MagickGetImageWidth(w);
    let y = MagickGetImageHeight(w);
    let pixels = x.checked_mul(y).and_then(|n| n.checked_mul(4));
    let result = if ok
        && x > 0
        && y > 0
        && x <= 720
        && y <= 576
        && pixels.is_some_and(|n| n <= capacity)
        && MagickExportImagePixels(w, 0, 0, x, y, c("RGBA").unwrap().as_ptr(), CHAR_PIXEL, rgba)
            != 0
    {
        *width = x as u32;
        *height = y as u32;
        0
    } else {
        -1
    };
    DestroyMagickWand(w);
    drop(_gate);
    result
}
fn byte(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}
fn y(r: u8, g: u8, b: u8) -> u8 {
    byte(((66 * r as i32 + 129 * g as i32 + 25 * b as i32 + 128) >> 8) + 16)
}
fn u(r: u8, g: u8, b: u8) -> u8 {
    byte(((-38 * r as i32 - 74 * g as i32 + 112 * b as i32 + 128) >> 8) + 128)
}
fn v(r: u8, g: u8, b: u8) -> u8 {
    byte(((112 * r as i32 - 94 * g as i32 - 18 * b as i32 + 128) >> 8) + 128)
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
pub unsafe extern "C" fn dvda_image_write_y4m(
    input: *const c_char,
    output: *const c_char,
    rate: *const c_char,
    aspect: *const c_char,
) -> c_int {
    if input.is_null() || output.is_null() || rate.is_null() || aspect.is_null() {
        return -1;
    }
    let Ok(_i) = CStr::from_ptr(input).to_str() else {
        return -1;
    };
    let Ok(o) = CStr::from_ptr(output).to_str() else {
        return -1;
    };
    let Ok(rate) = CStr::from_ptr(rate).to_str() else {
        return -1;
    };
    let Ok(aspect) = CStr::from_ptr(aspect).to_str() else {
        return -1;
    };
    if !matches!(rate, "25" | "30") || !matches!(aspect, "1:1" | "4:3" | "16:9" | "2.21:1") {
        return -1;
    }
    init();
    let _gate = GATE.get().unwrap().lock().unwrap();
    let w = NewMagickWand();
    if w.is_null() {
        return -1;
    }
    let mut result = -1;
    if MagickReadImage(w, input) == 0 || MagickTransformImageColorspace(w, SRGB_COLORSPACE) == 0 {
        DestroyMagickWand(w);
        return -1;
    }
    let (width, height) = (MagickGetImageWidth(w), MagickGetImageHeight(w));
    let n = width.checked_mul(height);
    if width == 0
        || height == 0
        || width % 2 != 0
        || height % 2 != 0
        || width > 16384
        || height > 16384
        || n.is_none()
    {
        DestroyMagickWand(w);
        return -1;
    }
    let n = n.unwrap();
    let mut rgb = vec![0; n * 3];
    if MagickExportImagePixels(
        w,
        0,
        0,
        width,
        height,
        c("RGB").unwrap().as_ptr(),
        CHAR_PIXEL,
        rgb.as_mut_ptr(),
    ) == 0
    {
        DestroyMagickWand(w);
        return -1;
    }
    let mut out = match File::create(Path::new(o)) {
        Ok(x) => x,
        Err(_) => {
            DestroyMagickWand(w);
            return -1;
        }
    };
    let header = format!(
        "YUV4MPEG2 W{width} H{height} F{} Ip A{aspect} C420jpeg\nFRAME\n",
        if rate == "30" { "30000:1001" } else { "25:1" }
    );
    let mut yy = vec![0; n];
    let mut uu = vec![0; n / 4];
    let mut vv = vec![0; n / 4];
    for row in 0..height {
        for col in 0..width {
            let p = &rgb[(row * width + col) * 3..];
            yy[row * width + col] = y(p[0], p[1], p[2]);
        }
    }
    for row in (0..height).step_by(2) {
        for col in (0..width).step_by(2) {
            let mut sum = [0u32; 3];
            for dy in 0..2 {
                for dx in 0..2 {
                    let p = &rgb[((row + dy) * width + col + dx) * 3..];
                    for k in 0..3 {
                        sum[k] += p[k] as u32
                    }
                }
            }
            let (r, g, b) = ((sum[0] + 2) / 4, (sum[1] + 2) / 4, (sum[2] + 2) / 4);
            let at = (row / 2) * (width / 2) + col / 2;
            uu[at] = u(r as u8, g as u8, b as u8);
            vv[at] = v(r as u8, g as u8, b as u8);
        }
    }
    if out
        .write_all(header.as_bytes())
        .and_then(|_| out.write_all(&yy))
        .and_then(|_| out.write_all(&uu))
        .and_then(|_| out.write_all(&vv))
        .and_then(|_| out.flush())
        .and_then(|_| out.sync_all())
        .is_ok()
    {
        result = 0
    }
    drop(out);
    if result != 0 {
        let _ = std::fs::remove_file(o);
    }
    DestroyMagickWand(w);
    result
}

include!(concat!(env!("OUT_DIR"), "\\image-layout.rs"));
use crate::{Call, Cancel, Emit};
use std::{cell::Cell, ffi::c_void, ptr, time::Duration};
type Command =
    unsafe extern "C" fn(*mut c_void, i32, *mut *mut c_char, *mut *mut c_char, *mut c_void) -> i32;
unsafe extern "C" {
    fn AcquireExceptionInfo() -> *mut c_void;
    fn DestroyExceptionInfo(value: *mut c_void) -> *mut c_void;
    fn AcquireImageInfo() -> *mut c_void;
    fn DestroyImageInfo(value: *mut c_void) -> *mut c_void;
    fn DestroyString(value: *mut c_char) -> *mut c_char;
    fn RelinquishMagickMemory(value: *mut c_void) -> *mut c_void;
    fn SetImageInfoProgressMonitor(
        info: *mut c_void,
        monitor: Option<unsafe extern "C" fn(*const c_char, i64, u64, *mut c_void) -> i32>,
        state: *mut c_void,
    );
    fn MagickCommandGenesis(
        info: *mut c_void,
        command: Command,
        count: i32,
        args: *mut *mut c_char,
        metadata: *mut *mut c_char,
        exception: *mut c_void,
    ) -> i32;
    fn IdentifyImageCommand(
        info: *mut c_void,
        count: i32,
        args: *mut *mut c_char,
        metadata: *mut *mut c_char,
        exception: *mut c_void,
    ) -> i32;
    fn ConvertImageCommand(
        info: *mut c_void,
        count: i32,
        args: *mut *mut c_char,
        metadata: *mut *mut c_char,
        exception: *mut c_void,
    ) -> i32;
    fn MogrifyImageCommand(
        info: *mut c_void,
        count: i32,
        args: *mut *mut c_char,
        metadata: *mut *mut c_char,
        exception: *mut c_void,
    ) -> i32;
    fn MagickImageCommand(
        info: *mut c_void,
        count: i32,
        args: *mut *mut c_char,
        metadata: *mut *mut c_char,
        exception: *mut c_void,
    ) -> i32;
    fn GetTypeList(
        pattern: *const c_char,
        length: *mut usize,
        exception: *mut c_void,
    ) -> *mut *mut c_char;
    fn SetWarningHandler(
        handler: Option<unsafe extern "C" fn(i32, *const c_char, *const c_char)>,
    ) -> *mut c_void;
    fn SetErrorHandler(
        handler: Option<unsafe extern "C" fn(i32, *const c_char, *const c_char)>,
    ) -> *mut c_void;
    fn SetFatalErrorHandler(
        handler: Option<unsafe extern "C" fn(i32, *const c_char, *const c_char)>,
    ) -> *mut c_void;
}
thread_local! {static ACTIVE:Cell<*const Call>=const{Cell::new(ptr::null())};}
unsafe extern "C" fn report(_severity: i32, reason: *const c_char, description: *const c_char) {
    ACTIVE.with(|active| {
        let call = active.get();
        if call.is_null() {
            return;
        }
        (*call).message(
            2,
            if reason.is_null() {
                ""
            } else {
                CStr::from_ptr(reason).to_str().unwrap_or("")
            },
        );
        if !description.is_null() && *description != 0 {
            (*call).message(2, ": ");
            (*call).message(2, &CStr::from_ptr(description).to_string_lossy());
        }
        (*call).message(2, "\n");
    });
}
unsafe extern "C" fn monitor(
    _tag: *const c_char,
    _offset: i64,
    _span: u64,
    state: *mut c_void,
) -> i32 {
    (!(*(state.cast::<Call>())).cancelled()) as i32
}
struct CliResources {
    info: *mut c_void,
    exception: *mut c_void,
    metadata: *mut c_char,
    files: Vec<std::path::PathBuf>,
}
impl Drop for CliResources {
    fn drop(&mut self) {
        unsafe {
            if !self.metadata.is_null() {
                DestroyString(self.metadata);
            }
            if !self.exception.is_null() {
                DestroyExceptionInfo(self.exception);
            }
            if !self.info.is_null() {
                DestroyImageInfo(self.info);
            }
        }
        for path in &self.files {
            let _ = std::fs::remove_file(path);
        }
        ACTIVE.with(|active| active.set(ptr::null()));
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
pub unsafe extern "C" fn dvda_image_run(
    count: i32,
    arguments: *const *const c_char,
    emit: Emit,
    cancel: Cancel,
    state: *mut c_void,
) -> i32 {
    if !(2..=8192).contains(&count) || arguments.is_null() {
        return 2;
    }
    init();
    let call = Call {
        emit,
        cancel,
        state,
    };
    let _guard = loop {
        match GATE.get().unwrap().try_lock() {
            Ok(guard) => break guard,
            Err(std::sync::TryLockError::WouldBlock) => {
                if call.cancelled() {
                    return 130;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return 2,
        }
    };
    ACTIVE.with(|active| active.set(&call));
    let mut resources = CliResources {
        info: AcquireImageInfo(),
        exception: AcquireExceptionInfo(),
        metadata: ptr::null_mut(),
        files: vec![],
    };
    if resources.info.is_null() || resources.exception.is_null() {
        return 1;
    }
    SetImageInfoProgressMonitor(
        resources.info,
        Some(monitor),
        (&call as *const Call as *mut Call).cast(),
    );
    if call.cancelled() {
        return 130;
    }
    let mut owned = Vec::new();
    for i in 0..count as usize {
        let arg = *arguments.add(i);
        if arg.is_null() {
            return 1;
        }
        let text = CStr::from_ptr(arg);
        if text.to_bytes().eq_ignore_ascii_case(b"info:")
            || text.to_bytes().eq_ignore_ascii_case(b"info:-")
        {
            let folder = match std::env::current_dir() {
                Ok(path) => path.join("build"),
                Err(_) => return 1,
            };
            if std::fs::create_dir_all(&folder).is_err() {
                return 1;
            }
            static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = loop {
                let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let path = folder.join(format!(
                    "bridge-info-{}-{sequence}-{i}.txt",
                    std::process::id()
                ));
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                {
                    Ok(_) => break path,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(_) => return 1,
                }
            };
            let replacement = c(&format!("info:{}", path.display()));
            resources.files.push(path);
            let Ok(replacement) = replacement else {
                return 1;
            };
            owned.push(replacement);
        } else {
            owned.push(text.to_owned());
        }
    }
    let mut mutable: Vec<_> = owned
        .iter()
        .map(|arg| arg.as_bytes_with_nul().to_vec())
        .collect();
    let mut args: Vec<_> = mutable
        .iter_mut()
        .map(|arg| arg.as_mut_ptr().cast::<c_char>())
        .chain([ptr::null_mut()])
        .collect();
    let severity = || {
        ptr::read_unaligned(
            resources
                .exception
                .cast::<u8>()
                .add(EXCEPTION_SEVERITY)
                .cast::<i32>(),
        )
    };
    let mut result =
        if count == 3 && owned[1].to_bytes() == b"-list" && owned[2].to_bytes() == b"font" {
            let mut length = 0;
            let fonts = GetTypeList(c"*".as_ptr(), &mut length, resources.exception);
            if fonts.is_null() && length != 0 {
                return 1;
            }
            for i in 0..length {
                let font = *fonts.add(i);
                call.message(1, "Font: ");
                call.message(1, &CStr::from_ptr(font).to_string_lossy());
                call.message(1, "\n");
                RelinquishMagickMemory(font.cast());
            }
            RelinquishMagickMemory(fonts.cast());
            if severity() < 400 { 0 } else { 1 }
        } else {
            let command: Command = match owned[0].to_bytes() {
                b"identify" => IdentifyImageCommand,
                b"convert" => ConvertImageCommand,
                b"mogrify" => MogrifyImageCommand,
                b"magick" => MagickImageCommand,
                _ => {
                    call.message(2, "Unsupported image command.\n");
                    return 1;
                }
            };
            let metadata = if owned[0].to_bytes() == b"identify" {
                &mut resources.metadata
            } else {
                ptr::null_mut()
            };
            let ok = MagickCommandGenesis(
                resources.info,
                command,
                count,
                args.as_mut_ptr(),
                metadata,
                resources.exception,
            );
            let mut status = if ok != 0 && severity() < 400 { 0 } else { 1 };
            if !resources.metadata.is_null() {
                call.message(1, &CStr::from_ptr(resources.metadata).to_string_lossy());
            }
            for path in &resources.files {
                match std::fs::read(path) {
                    Ok(bytes) => {
                        for chunk in bytes.chunks(4095) {
                            call.message(1, &String::from_utf8_lossy(chunk));
                        }
                    }
                    Err(_) => status = 1,
                }
            }
            status
        };
    if severity() != 0 {
        let reason = ptr::read_unaligned(
            resources
                .exception
                .cast::<u8>()
                .add(EXCEPTION_REASON)
                .cast::<*const c_char>(),
        );
        let description = ptr::read_unaligned(
            resources
                .exception
                .cast::<u8>()
                .add(EXCEPTION_DESCRIPTION)
                .cast::<*const c_char>(),
        );
        report(severity(), reason, description);
    }
    if call.cancelled() {
        result = 130
    }
    result
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
pub unsafe extern "C" fn dvda_image_command(command: *const c_char) -> i32 {
    if command.is_null() {
        return -1;
    }
    let Ok(line) = CStr::from_ptr(command).to_str() else {
        return -1;
    };
    let line = crate::win::wide(line.trim_start_matches([' ', '\t', '\r', '\n']));
    let mut count = 0;
    let parsed = crate::win::CommandLineToArgvW(line.as_ptr(), &mut count);
    if parsed.is_null() {
        return -1;
    }
    let mut args = Vec::new();
    for i in 0..count.max(0) as usize {
        let text = String::from_utf16_lossy(&crate::win::wtext(*parsed.add(i)));
        let Ok(arg) = c(&text) else {
            crate::win::LocalFree(parsed.cast());
            return -1;
        };
        args.push(arg);
    }
    crate::win::LocalFree(parsed.cast());
    let pointers: Vec<_> = args.iter().map(|arg| arg.as_ptr()).collect();
    if dvda_image_run(count, pointers.as_ptr(), None, None, ptr::null_mut()) == 0 {
        0
    } else {
        -1
    }
}
