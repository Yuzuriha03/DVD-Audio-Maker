#[cfg(windows)]
use std::ffi::c_char;
use std::ffi::c_void;
pub type Handle = *mut c_void;
#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn GetActiveProcessorCount(group: u16) -> u32;
    pub fn GetModuleFileNameW(module: Handle, path: *mut u16, size: u32) -> u32;
    pub fn GetModuleHandleExW(flags: u32, address: *const u16, module: *mut Handle) -> i32;
    pub fn SetEnvironmentVariableW(name: *const u16, value: *const u16) -> i32;
    pub fn GetModuleHandleW(name: *const u16) -> Handle;
    pub fn GetCommandLineW() -> *const u16;
    pub fn GetStdHandle(which: u32) -> Handle;
    pub fn GetLastError() -> u32;
    pub fn CloseHandle(h: Handle) -> i32;
    pub fn LocalFree(h: Handle) -> Handle;
    pub fn CreateJobObjectW(attr: *const c_void, name: *const u16) -> Handle;
    pub fn SetInformationJobObject(job: Handle, class: u32, info: *const c_void, size: u32) -> i32;
    pub fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
    pub fn CreateProcessW(
        app: *const u16,
        command: *mut u16,
        pa: *const c_void,
        ta: *const c_void,
        inherit: i32,
        flags: u32,
        env: *const c_void,
        dir: *const u16,
        start: *mut Startup,
        info: *mut Process,
    ) -> i32;
    pub fn ResumeThread(thread: Handle) -> u32;
    pub fn TerminateProcess(process: Handle, status: u32) -> i32;
    pub fn WaitForSingleObject(handle: Handle, time: u32) -> u32;
    pub fn GetExitCodeProcess(process: Handle, code: *mut u32) -> i32;
    pub fn GetProcAddress(module: Handle, name: *const c_char) -> *mut c_void;
}
#[cfg(windows)]
#[link(name = "shell32")]
unsafe extern "system" {
    pub fn CommandLineToArgvW(line: *const u16, count: *mut i32) -> *mut *mut u16;
}
#[cfg(windows)]
#[repr(C)]
pub struct Startup {
    pub cb: u32,
    pub reserved: *mut u16,
    pub desktop: *mut u16,
    pub title: *mut u16,
    pub x: u32,
    pub y: u32,
    pub xsize: u32,
    pub ysize: u32,
    pub xcount: u32,
    pub ycount: u32,
    pub fill: u32,
    pub flags: u32,
    pub show: u16,
    pub cb_reserved: u16,
    pub reserved2: *mut u8,
    pub input: Handle,
    pub output: Handle,
    pub error: Handle,
}
#[cfg(windows)]
#[repr(C)]
pub struct Process {
    pub process: Handle,
    pub thread: Handle,
    pub pid: u32,
    pub tid: u32,
}
#[cfg(windows)]
#[repr(C)]
pub struct Job {
    pub process_time: i64,
    pub job_time: i64,
    pub flags: u32,
    pub min: usize,
    pub max: usize,
    pub active: u32,
    pub affinity: usize,
    pub priority: u32,
    pub scheduling: u32,
    pub io: [u64; 6],
    pub process_memory: usize,
    pub job_memory: usize,
    pub peak_process: usize,
    pub peak_job: usize,
}
#[must_use]
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}
#[cfg(windows)]
#[must_use]
/// Resolve the path of an already-loaded module.
///
/// # Panics
/// Panics only if the fixed 32,768-element buffer cannot fit in `u32`.
pub fn module_path(name: &str) -> Option<String> {
    let wide_name = wide(name);
    let module = unsafe { GetModuleHandleW(wide_name.as_ptr()) };
    if module.is_null() {
        return None;
    }
    let mut buf = vec![0u16; 32768];
    let capacity = u32::try_from(buf.len()).expect("module path buffer fits in u32");
    let len = unsafe { GetModuleFileNameW(module, buf.as_mut_ptr(), capacity) };
    if len == 0 || len as usize >= buf.len() {
        return None;
    }
    buf.truncate(len as usize);
    Some(String::from_utf16_lossy(&buf))
}
#[cfg(not(windows))]
#[must_use]
/// Resolve the path of the running module (Android/Linux: the executable itself).
pub fn module_path(_name: &str) -> Option<String> {
    std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
#[must_use]
pub unsafe fn wtext(p: *const u16) -> Vec<u16> {
    // SAFETY: the caller guarantees a readable, NUL-terminated UTF-16 string.
    unsafe {
        let mut n = 0;
        while *p.add(n) != 0 {
            n += 1;
        }
        std::slice::from_raw_parts(p, n).to_vec()
    }
}
static ACTIVE_MEDIA_JOBS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub(crate) struct MediaJobGuard;
impl Drop for MediaJobGuard {
    fn drop(&mut self) {
        ACTIVE_MEDIA_JOBS.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

pub(crate) fn media_job() -> MediaJobGuard {
    ACTIVE_MEDIA_JOBS.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    MediaJobGuard
}

fn workers_for(processors: usize, jobs: usize) -> i32 {
    let workers = processors
        .max(1)
        .div_ceil(jobs.max(1))
        .min(i32::MAX as usize);
    i32::try_from(workers).expect("worker count is clamped to i32")
}

pub fn workers() -> i32 {
    #[cfg(windows)]
    let processors = unsafe { GetActiveProcessorCount(0xffff) } as usize;
    #[cfg(not(windows))]
    let processors = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let jobs = ACTIVE_MEDIA_JOBS.load(std::sync::atomic::Ordering::Acquire);
    workers_for(processors, jobs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_workers_share_processors_between_active_media_jobs() {
        assert_eq!(workers_for(8, 0), 8);
        assert_eq!(workers_for(8, 1), 8);
        assert_eq!(workers_for(8, 2), 4);
        assert_eq!(workers_for(8, 3), 3);
        assert_eq!(workers_for(8, 8), 1);
        assert_eq!(workers_for(16, 5), 4);
        assert_eq!(workers_for(1, 16), 1);
    }
}
