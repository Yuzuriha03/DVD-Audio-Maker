//! Rust ownership for the vendor's private heap and CRT resources.
//! Every export returns normally; only a caller in C may promote a failure to longjmp.
use std::{
    cell::{Cell, RefCell},
    ffi::{CStr, c_char, c_int, c_long, c_void},
    fs::ReadDir,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

pub type ReadRgba =
    unsafe extern "C" fn(*const c_char, *mut u8, usize, *mut u32, *mut u32) -> c_int;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn HeapCreate(options: u32, initial: usize, maximum: usize) -> *mut c_void;
    fn HeapAlloc(heap: *mut c_void, flags: u32, bytes: usize) -> *mut c_void;
    fn HeapReAlloc(heap: *mut c_void, flags: u32, object: *mut c_void, bytes: usize)
    -> *mut c_void;
    fn HeapFree(heap: *mut c_void, flags: u32, object: *mut c_void) -> c_int;
    fn HeapDestroy(heap: *mut c_void) -> c_int;
}
unsafe extern "C" {
    fn _errno() -> *mut c_int;
    fn _wfopen(path: *const u16, mode: *const u16) -> *mut c_void;
    fn fclose(file: *mut c_void) -> c_int;
    fn _wopen(path: *const u16, flags: c_int, ...) -> c_int;
    fn _close(file: c_int) -> c_int;
    fn _filelengthi64(file: c_int) -> i64;
}
const FILE: c_int = 1;
const DESCRIPTOR: c_int = 2;
const DIRECTORY: c_int = 3;
const COM: c_int = 4;
const BINARY: c_int = 0x8000;
thread_local! {
    static HEAP: Cell<*mut c_void> = const { Cell::new(ptr::null_mut()) };
    static PIXELS: Cell<Option<ReadRgba>> = const { Cell::new(None) };
    static RESOURCES: RefCell<Vec<(usize, c_int)>> = const { RefCell::new(Vec::new()) };
}
fn guarded<T>(failure: T, operation: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(operation)).unwrap_or(failure)
}
fn errno(code: c_int) {
    unsafe { *_errno() = code };
}
fn io_errno(error: &std::io::Error) -> c_int {
    match error.kind() {
        std::io::ErrorKind::AlreadyExists => 17,
        std::io::ErrorKind::NotFound => 2,
        std::io::ErrorKind::PermissionDenied => 13,
        std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidFilename => 22,
        std::io::ErrorKind::NotADirectory => 20,
        std::io::ErrorKind::StorageFull => 28,
        std::io::ErrorKind::ReadOnlyFilesystem => 30,
        _ => 5,
    }
}
unsafe fn text<'a>(value: *const c_char) -> Result<&'a str, ()> {
    if value.is_null() {
        errno(22);
        return Err(());
    }
    unsafe { CStr::from_ptr(value) }
        .to_str()
        .map_err(|_| errno(22))
}
unsafe fn wide(value: *const c_char) -> Result<Vec<u16>, ()> {
    Ok(unsafe { text(value)? }
        .encode_utf16()
        .chain(Some(0))
        .collect())
}
unsafe fn allocate(bytes: usize, zero: bool) -> Result<*mut c_void, ()> {
    let heap = HEAP.get();
    if heap.is_null() {
        return Err(());
    }
    let object = unsafe { HeapAlloc(heap, if zero { 8 } else { 0 }, bytes.max(1)) };
    if object.is_null() {
        Err(())
    } else {
        Ok(object)
    }
}
unsafe fn free(object: *mut c_void) -> Result<(), ()> {
    if object.is_null() {
        return Ok(());
    }
    let heap = HEAP.get();
    if heap.is_null() || unsafe { HeapFree(heap, 0, object) } == 0 {
        Err(())
    } else {
        Ok(())
    }
}
fn disown(object: *mut c_void, kind: c_int) {
    RESOURCES.with(|resources| {
        let mut resources = resources.borrow_mut();
        if let Some(index) = resources
            .iter()
            .rposition(|item| *item == (object as usize, kind))
        {
            resources.remove(index);
        }
    });
}
unsafe fn release_com(object: *mut c_void) {
    if !object.is_null() {
        // IUnknown's first three slots are QueryInterface, AddRef, Release.
        let table = unsafe { *object.cast::<*const usize>() };
        let release: unsafe extern "system" fn(*mut c_void) -> u32 =
            unsafe { std::mem::transmute(*table.add(2)) };
        unsafe { release(object) };
    }
}
unsafe fn close_resource(object: *mut c_void, kind: c_int) -> Result<(), ()> {
    match kind {
        FILE => {
            if unsafe { fclose(object) } == 0 {
                Ok(())
            } else {
                Err(())
            }
        }
        DESCRIPTOR => {
            if unsafe { _close(object as isize as c_int) } == 0 {
                Ok(())
            } else {
                Err(())
            }
        }
        DIRECTORY => {
            unsafe { ptr::drop_in_place(object.cast::<Directory>()) };
            unsafe { free(object) }
        }
        COM => {
            unsafe { release_com(object) };
            Ok(())
        }
        _ => Err(()),
    }
}
unsafe fn own(object: *mut c_void, kind: c_int) -> Result<(), ()> {
    let result = RESOURCES.with(|resources| {
        let mut resources = resources.borrow_mut();
        resources.try_reserve(1).map_err(|_| ())?;
        resources.push((object as usize, kind));
        Ok(())
    });
    if result.is_err() {
        // Release a newly opened object before C promotes an ownership failure.
        let _ = unsafe { close_resource(object, kind) };
    }
    result
}
pub(crate) struct Session {
    heap: *mut c_void,
}
impl Session {
    pub(crate) fn begin(pixels: Option<ReadRgba>) -> Result<Self, ()> {
        if !HEAP.get().is_null() || RESOURCES.with(|resources| !resources.borrow().is_empty()) {
            return Err(());
        }
        let heap = unsafe { HeapCreate(0, 0, 0) };
        if heap.is_null() {
            return Err(());
        }
        HEAP.set(heap);
        PIXELS.set(pixels);
        Ok(Self { heap })
    }
    pub(crate) fn finish(&mut self) -> bool {
        if self.heap.is_null() {
            return true;
        }
        let mut succeeded = true;
        loop {
            let resource = RESOURCES.with(|resources| resources.borrow_mut().pop());
            let Some((object, kind)) = resource else {
                break;
            };
            if unsafe { close_resource(object as *mut c_void, kind) }.is_err() {
                succeeded = false
            }
        }
        PIXELS.set(None);
        HEAP.set(ptr::null_mut());
        if unsafe { HeapDestroy(self.heap) } == 0 {
            succeeded = false
        }
        self.heap = ptr::null_mut();
        succeeded
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.finish();
    }
}

/// # Safety
/// `path` must be a terminated UTF-8 string. No C failure jump is performed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_mkdir(path: *const c_char) -> c_int {
    guarded(-1, || {
        let Ok(path) = (unsafe { text(path) }) else {
            return -1;
        };
        match std::fs::create_dir(path) {
            Ok(()) => 0,
            Err(error) => {
                errno(io_errno(&error));
                -1
            }
        }
    })
}
/// # Safety
/// `output` must be writable; vendor pointers belong to the current private heap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_malloc(bytes: usize, output: *mut *mut c_void) -> c_int {
    guarded(1, || match unsafe { allocate(bytes, false) } {
        Ok(object) => {
            unsafe { *output = object };
            0
        }
        Err(()) => 1,
    })
}
/// # Safety
/// Same output lifetime as `menu_rust_malloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_calloc(
    count: usize,
    bytes: usize,
    output: *mut *mut c_void,
) -> c_int {
    guarded(1, || {
        let Some(bytes) = count.checked_mul(bytes) else {
            return 1;
        };
        match unsafe { allocate(bytes, true) } {
            Ok(object) => {
                unsafe { *output = object };
                0
            }
            Err(()) => 1,
        }
    })
}
/// # Safety
/// `object` is null or belongs to the current private heap; `output` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_realloc(
    object: *mut c_void,
    bytes: usize,
    output: *mut *mut c_void,
) -> c_int {
    guarded(1, || {
        let result = if object.is_null() {
            unsafe { allocate(bytes, false) }
        } else if bytes == 0 {
            unsafe { free(object) }.map(|_| ptr::null_mut())
        } else {
            let heap = HEAP.get();
            if heap.is_null() {
                return 1;
            }
            let object = unsafe { HeapReAlloc(heap, 0, object, bytes) };
            if object.is_null() {
                Err(())
            } else {
                Ok(object)
            }
        };
        match result {
            Ok(object) => {
                unsafe { *output = object };
                0
            }
            Err(()) => 1,
        }
    })
}
/// # Safety
/// `object` is null or belongs to the current private heap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_free(object: *mut c_void) -> c_int {
    guarded(1, || c_int::from(unsafe { free(object) }.is_err()))
}
/// # Safety
/// `value` is terminated or readable for `maximum` bytes; `output` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_strndup(
    value: *const c_char,
    maximum: usize,
    output: *mut *mut c_char,
) -> c_int {
    guarded(1, || {
        if value.is_null() {
            return 1;
        }
        let mut length = 0;
        while length < maximum && unsafe { *value.add(length) } != 0 {
            length += 1
        }
        let Some(bytes) = length.checked_add(1) else {
            return 1;
        };
        let Ok(object) = (unsafe { allocate(bytes, false) }) else {
            return 1;
        };
        let object = object.cast::<c_char>();
        unsafe {
            ptr::copy_nonoverlapping(value, object, length);
            *object.add(length) = 0;
            *output = object;
        }
        0
    })
}
/// # Safety
/// Both strings are terminated UTF-8; `output` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_fopen(
    path: *const c_char,
    mode: *const c_char,
    output: *mut *mut c_void,
) -> c_int {
    guarded(1, || {
        let (Ok(path), Ok(mode)) = (unsafe { wide(path) }, unsafe { wide(mode) }) else {
            return 1;
        };
        let file = unsafe { _wfopen(path.as_ptr(), mode.as_ptr()) };
        if !file.is_null() && unsafe { own(file, FILE) }.is_err() {
            return 1;
        }
        unsafe { *output = file };
        0
    })
}
/// # Safety
/// `file` is a registered, open CRT FILE pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_fclose(file: *mut c_void) -> c_int {
    guarded(1, || {
        let result = unsafe { fclose(file) };
        disown(file, FILE);
        result
    })
}
/// # Safety
/// `path` is terminated UTF-8; `output` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_open(
    path: *const c_char,
    flags: c_int,
    mode: c_int,
    output: *mut c_int,
) -> c_int {
    guarded(1, || {
        let Ok(path) = (unsafe { wide(path) }) else {
            return 1;
        };
        let file = unsafe { _wopen(path.as_ptr(), flags | BINARY, mode) };
        if file >= 0 && unsafe { own(file as isize as *mut c_void, DESCRIPTOR) }.is_err() {
            return 1;
        }
        unsafe { *output = file };
        0
    })
}
/// # Safety
/// `file` is a registered, open CRT descriptor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_close(file: c_int) -> c_int {
    guarded(1, || {
        let result = unsafe { _close(file) };
        disown(file as isize as *mut c_void, DESCRIPTOR);
        result
    })
}
pub(crate) unsafe fn file_length(file: c_int) -> Result<u64, ()> {
    u64::try_from(unsafe { _filelengthi64(file) }).map_err(|_| ())
}
// MinGW x64's struct dirent. Retained vendor callers treat DIR itself as opaque.
#[repr(C)]
pub struct Dirent {
    inode: c_long,
    record_length: u16,
    name_length: u16,
    name: [c_char; 260],
}
// session.h independently checks the installed MinGW header at C compile time.
const _: () = {
    assert!(size_of::<Dirent>() == 268);
    assert!(align_of::<Dirent>() == 4);
    assert!(std::mem::offset_of!(Dirent, inode) == 0);
    assert!(std::mem::offset_of!(Dirent, record_length) == 4);
    assert!(std::mem::offset_of!(Dirent, name_length) == 6);
    assert!(std::mem::offset_of!(Dirent, name) == 8);
};
struct Directory {
    entries: ReadDir,
    entry: Dirent,
}
/// # Safety
/// `path` is terminated UTF-8; `output` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_opendir(path: *const c_char, output: *mut *mut c_void) -> c_int {
    guarded(1, || {
        let Ok(path) = (unsafe { text(path) }) else {
            return 1;
        };
        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) => {
                errno(io_errno(&error));
                unsafe { *output = ptr::null_mut() };
                return 0;
            }
        };
        let Ok(object) = (unsafe { allocate(std::mem::size_of::<Directory>(), false) }) else {
            return 1;
        };
        unsafe {
            object.cast::<Directory>().write(Directory {
                entries,
                entry: Dirent {
                    inode: 0,
                    record_length: 0,
                    name_length: 0,
                    name: [0; 260],
                },
            })
        };
        if unsafe { own(object, DIRECTORY) }.is_err() {
            return 1;
        }
        unsafe { *output = object };
        0
    })
}
/// # Safety
/// `directory` is a live registered directory; `output` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_readdir(
    directory: *mut c_void,
    output: *mut *mut Dirent,
) -> c_int {
    guarded(1, || {
        if directory.is_null() {
            errno(22);
            return 1;
        }
        let directory = unsafe { &mut *directory.cast::<Directory>() };
        let Some(entry) = directory.entries.next() else {
            unsafe { *output = ptr::null_mut() };
            return 0;
        };
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errno(io_errno(&error));
                return 1;
            }
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            errno(22);
            return 1;
        };
        if name.len() >= directory.entry.name.len() {
            errno(36);
            return 1;
        }
        directory.entry.name.fill(0);
        for (target, byte) in directory.entry.name.iter_mut().zip(name.bytes()) {
            *target = byte as c_char
        }
        directory.entry.name_length = name.len() as u16;
        unsafe { *output = &mut directory.entry };
        0
    })
}
/// # Safety
/// `directory` is a live registered directory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_closedir(directory: *mut c_void) -> c_int {
    guarded(-1, || {
        if directory.is_null() {
            errno(22);
            return -1;
        }
        disown(directory, DIRECTORY);
        if unsafe { close_resource(directory, DIRECTORY) }.is_ok() {
            0
        } else {
            -1
        }
    })
}
/// # Safety
/// `object` is a live IUnknown. Ownership transfers on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_own_com(object: *mut c_void) -> c_int {
    guarded(1, || c_int::from(unsafe { own(object, COM) }.is_err()))
}
/// # Safety
/// `object` is null or a live registered IUnknown.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_release_com(object: *mut c_void) -> c_int {
    guarded(1, || {
        unsafe { release_com(object) };
        disown(object, COM);
        0
    })
}
/// # Safety
/// Arguments obey the image ABI. A callback may not unwind or longjmp.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_read_rgba(
    path: *const c_char,
    pixels: *mut u8,
    capacity: usize,
    width: *mut u32,
    height: *mut u32,
) -> c_int {
    guarded(-1, || match PIXELS.get() {
        Some(callback) => unsafe { callback(path, pixels, capacity, width, height) },
        None => -1,
    })
}
/// # Safety
/// Parser pointers refer to active vendor globals; `value` is null or terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_body(
    value: *const c_char,
    body: *mut *mut c_char,
    accepts: *mut bool,
) -> c_int {
    guarded(1, || {
        if unsafe { free((*body).cast()) }.is_err() {
            return 1;
        }
        unsafe { *body = ptr::null_mut() };
        if value.is_null() {
            unsafe { *accepts = false };
            0
        } else {
            unsafe { menu_rust_strndup(value, usize::MAX, body) }
        }
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn get_video_format() -> c_int {
    2
}
#[unsafe(no_mangle)]
pub extern "C" fn get_outputdir() -> *mut c_char {
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    #[test]
    fn private_heap_allocation_reallocation_and_overflow() {
        let mut session = Session::begin(None).unwrap();
        let mut object = ptr::null_mut();
        unsafe {
            assert_eq!(menu_rust_malloc(4, &mut object), 0);
            object.cast::<u32>().write(0x12345678);
            assert_eq!(menu_rust_realloc(object, 8, &mut object), 0);
            assert_eq!(object.cast::<u32>().read(), 0x12345678);
            assert_eq!(menu_rust_free(object), 0);
            assert_eq!(menu_rust_calloc(usize::MAX, 2, &mut object), 1);
            assert_eq!(menu_rust_calloc(3, 4, &mut object), 0);
            assert_eq!(
                std::slice::from_raw_parts(object.cast::<u8>(), 12),
                &[0; 12]
            );
            assert_eq!(menu_rust_realloc(object, 0, &mut object), 0);
            assert!(object.is_null());
        }
        assert!(session.finish());
        assert!(HEAP.get().is_null());
    }

    #[test]
    fn unicode_crt_and_directory_resources_close_on_session_drop() {
        let root = std::env::temp_dir().join(format!(
            "menu-resources-中文-日本語-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("音源-日本語.dat");
        std::fs::write(&file, b"resource fixture").unwrap();
        let directory_path = CString::new(root.to_str().unwrap()).unwrap();
        let file_path = CString::new(file.to_str().unwrap()).unwrap();
        {
            let _session = Session::begin(None).unwrap();
            let mut descriptor = -1;
            let mut stream = ptr::null_mut();
            let mut directory = ptr::null_mut();
            let mut entry = ptr::null_mut();
            unsafe {
                assert_eq!(menu_rust_open(file_path.as_ptr(), 0, 0, &mut descriptor), 0);
                assert_eq!(file_length(descriptor).unwrap(), 16);
                assert_eq!(
                    menu_rust_fopen(file_path.as_ptr(), c"rb".as_ptr(), &mut stream),
                    0
                );
                assert!(!stream.is_null());
                assert_eq!(
                    menu_rust_opendir(directory_path.as_ptr(), &mut directory),
                    0
                );
                assert!(!directory.is_null());
                assert_eq!(menu_rust_readdir(directory, &mut entry), 0);
                assert_eq!(
                    CStr::from_ptr((*entry).name.as_ptr()).to_str().unwrap(),
                    "音源-日本語.dat"
                );
                assert_eq!(menu_rust_readdir(directory, &mut entry), 0);
                assert!(entry.is_null());
                let invalid = [0xff_u8, 0];
                assert_eq!(
                    menu_rust_fopen(invalid.as_ptr().cast(), c"rb".as_ptr(), &mut stream),
                    1
                );
            }
            // Deliberately leave FILE, descriptor and directory owned at failure.
            assert_eq!(RESOURCES.with(|resources| resources.borrow().len()), 3);
        }
        assert!(RESOURCES.with(|resources| resources.borrow().is_empty()));
        std::fs::remove_dir_all(&root).unwrap();
        assert!(Session::begin(None).is_ok());
    }
}
