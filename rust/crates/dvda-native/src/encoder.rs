//! Streaming interface to the pinned C MLP core; encoded bytes are never rewritten.
use std::{
    collections::HashMap,
    ffi::{c_char, c_void},
    io,
    os::windows::ffi::OsStrExt,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

pub struct Stamp {
    pub start: u64,
    pub packet: Vec<u8>,
    pub valid_bits: u32,
}
pub struct Request {
    pub sample_rate: u32,
    pub bits: u32,
    pub channels: u32,
    pub frames: u64,
    pub assignment: u32,
    pub metadata: Vec<Stamp>,
}
#[derive(Debug)]
pub struct ResultInfo {
    pub status: i32,
    pub access_units: u32,
    pub input_frames: u64,
    pub encoded_frames: u64,
    pub output_bytes: u64,
    pub error: String,
}
/// Both methods run synchronously on the invoking thread, with borrowed buffers.
pub trait Stream {
    /// Returns complete frames, each containing exactly the declared channel count.
    fn read(&mut self, samples: &mut [i32]) -> io::Result<usize>;
    fn write(&mut self, bytes: &[u8]) -> io::Result<()>;
}
#[repr(C)]
struct Config {
    struct_size: u32,
    abi: u32,
    sample_rate: u32,
    bits: u32,
    channels: u32,
    restart_interval: u32,
    frames: u64,
    metadata: *const RawStamp,
    metadata_count: usize,
}
#[repr(C)]
struct RawStamp {
    start: u64,
    size: u32,
    packet: *const u8,
    valid_bits: u32,
}
#[repr(C)]
struct RawResult {
    status: i32,
    access_units: u32,
    input_frames: u64,
    encoded_frames: u64,
    output_bytes: u64,
    error: [u8; 192],
}
type Read = unsafe extern "C" fn(*mut c_void, *mut i32, usize, *mut usize) -> i32;
type Write = unsafe extern "C" fn(*mut c_void, *const u8, usize) -> i32;
type Encode = unsafe extern "C" fn(
    *const Config,
    u32,
    Read,
    *mut c_void,
    Write,
    *mut c_void,
    *mut RawResult,
) -> i32;
type Version = unsafe extern "C" fn() -> u32;

struct State<'a> {
    stream: &'a mut dyn Stream,
    channels: usize,
    failure: Option<io::Error>,
}
impl State<'_> {
    fn perform(&mut self, action: impl FnOnce(&mut Self) -> io::Result<()>) -> i32 {
        if self.failure.is_some() {
            return 1;
        }
        match catch_unwind(AssertUnwindSafe(|| action(self))) {
            Ok(Ok(())) => 0,
            Ok(Err(error)) => {
                self.failure = Some(error);
                1
            }
            Err(_) => {
                self.failure = Some(io::Error::other("Panic caught in MLP callback"));
                1
            }
        }
    }
}
unsafe extern "C" fn read(
    state: *mut c_void,
    pcm: *mut i32,
    capacity: usize,
    frames: *mut usize,
) -> i32 {
    // SAFETY: state is created below, kept alive, and accessed serially by the C core.
    let state = unsafe { &mut *state.cast::<State<'_>>() };
    state.perform(|state| {
        if frames.is_null() || !(frames as usize).is_multiple_of(std::mem::align_of::<usize>()) {
            return Err(io::Error::other("Invalid MLP read count pointer"));
        }
        unsafe {
            *frames = 0;
        }
        let count = capacity
            .checked_mul(state.channels)
            .filter(|n| *n <= isize::MAX as usize / 4)
            .ok_or_else(|| io::Error::other("Invalid MLP read capacity"))?;
        if count == 0 || pcm.is_null() || !(pcm as usize).is_multiple_of(4) {
            return Err(io::Error::other("Invalid MLP PCM buffer"));
        }
        // Initialize foreign writable storage before exposing initialized i32 values.
        // SAFETY: the pinned DLL supplies capacity complete writable frames.
        unsafe {
            std::ptr::write_bytes(pcm, 0, count);
        }
        let samples = unsafe { std::slice::from_raw_parts_mut(pcm, count) };
        let written = state.stream.read(samples)?;
        if written > capacity {
            return Err(io::Error::other("Too many PCM frames returned"));
        }
        unsafe {
            *frames = written;
        }
        Ok(())
    })
}
unsafe extern "C" fn write(state: *mut c_void, bytes: *const u8, count: usize) -> i32 {
    let state = unsafe { &mut *state.cast::<State<'_>>() };
    state.perform(|state| {
        if count > 8190 || (count != 0 && bytes.is_null()) {
            return Err(io::Error::other("MLP output block exceeds ABI limit"));
        }
        let data = if count == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(bytes, count) }
        };
        state.stream.write(data)
    })
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}
pub struct Encoder {
    module: *mut c_void,
    encode: Encode,
}
// Each C invocation owns codec/FP state and restores the caller state around callbacks.
unsafe impl Send for Encoder {}
unsafe impl Sync for Encoder {}
impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.module);
        }
    }
}
impl Encoder {
    pub fn load(path: &Path) -> io::Result<Arc<Self>> {
        static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Encoder>>>> = OnceLock::new();
        let path = std::path::absolute(path)?;
        let mut cache = CACHE
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| io::Error::other("Encoder library cache lock poisoned"))?;
        if let Some(encoder) = cache.get(&path) {
            return Ok(Arc::clone(encoder));
        }
        let mut wide: Vec<_> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(io::Error::from_raw_os_error(123));
        }
        wide.push(0);
        // SAFETY: validated terminated UTF-16 path; dependencies only beside DLL/System32.
        let module = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), 0x100 | 0x800) };
        if module.is_null() {
            return Err(io::Error::last_os_error());
        }
        let entry = unsafe { GetProcAddress(module, c"mlp_encode_stream_layout".as_ptr()) };
        let version = unsafe { GetProcAddress(module, c"mlp_encoder_abi_version".as_ptr()) };
        if entry.is_null() || version.is_null() {
            let error = io::Error::last_os_error();
            unsafe {
                FreeLibrary(module);
            }
            return Err(error);
        }
        // SAFETY: fixed frozen encoder API layouts declared in dvda-mlp/src/ffi.rs.
        let version = unsafe { std::mem::transmute::<*mut c_void, Version>(version) };
        if unsafe { version() } != 1 {
            unsafe {
                FreeLibrary(module);
            }
            return Err(io::Error::other("Unsupported MLP encoder ABI"));
        }
        let encoder = Arc::new(Self {
            module,
            encode: unsafe { std::mem::transmute::<*mut c_void, Encode>(entry) },
        });
        cache.insert(path, Arc::clone(&encoder));
        Ok(encoder)
    }

    pub fn encode(&self, request: &Request, stream: &mut dyn Stream) -> io::Result<ResultInfo> {
        if !(1..=6).contains(&request.channels) {
            return Err(io::Error::other("Invalid channel count"));
        }
        let metadata: Vec<_> = request
            .metadata
            .iter()
            .map(|record| {
                Ok(RawStamp {
                    start: record.start,
                    size: u32::try_from(record.packet.len())
                        .map_err(|_| io::Error::other("Metadata packet too large"))?,
                    packet: record.packet.as_ptr(),
                    valid_bits: record.valid_bits,
                })
            })
            .collect::<io::Result<_>>()?;
        let config = Config {
            struct_size: std::mem::size_of::<Config>() as u32,
            abi: 1,
            sample_rate: request.sample_rate,
            bits: request.bits,
            channels: request.channels,
            restart_interval: 0,
            frames: request.frames,
            metadata: metadata.as_ptr(),
            metadata_count: metadata.len(),
        };
        let mut result = RawResult {
            status: 0,
            access_units: 0,
            input_frames: 0,
            encoded_frames: 0,
            output_bytes: 0,
            error: [0; 192],
        };
        let mut state = State {
            stream,
            channels: request.channels as usize,
            failure: None,
        };
        let pointer = (&mut state as *mut State<'_>).cast();
        // SAFETY: config, metadata, packet storage, state and result live for the synchronous invocation.
        let status = unsafe {
            (self.encode)(
                &config,
                request.assignment,
                read,
                pointer,
                write,
                pointer,
                &mut result,
            )
        };
        if let Some(error) = state.failure {
            return Err(error);
        }
        let length = result
            .error
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(result.error.len());
        Ok(ResultInfo {
            status: if status != 0 { status } else { result.status },
            access_units: result.access_units,
            input_frames: result.input_frames,
            encoded_frames: result.encoded_frames,
            output_bytes: result.output_bytes,
            error: String::from_utf8_lossy(&result.error[..length]).into_owned(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoder_abi_matches_headers() {
        assert_eq!(std::mem::size_of::<Config>(), 48);
        assert_eq!(std::mem::offset_of!(Config, metadata), 32);
        assert_eq!(std::mem::size_of::<RawStamp>(), 32);
        assert_eq!(std::mem::offset_of!(RawStamp, packet), 16);
        assert_eq!(std::mem::size_of::<RawResult>(), 224);
        assert_eq!(std::mem::offset_of!(RawResult, error), 32);
    }
}
