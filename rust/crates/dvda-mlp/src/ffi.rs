#[cfg(not(target_arch = "x86_64"))]
use crate::Error;
use crate::{Config, format::Profile, metadata::Metadata};
use std::ffi::{c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
#[path = "ffi_layout.rs"]
mod layout;
pub use layout::*;

pub type Read = unsafe extern "C" fn(*mut c_void, *mut i32, usize, *mut usize) -> i32;
pub type Write = unsafe extern "C" fn(*mut c_void, *const u8, usize) -> i32;

#[repr(C)]
pub struct StampRecord {
    pub start: u64,
    pub size: u32,
    pub packet: *const u8,
    pub valid_bits: u32,
}
#[repr(C)]
pub struct EncoderConfig {
    pub struct_size: u32,
    pub abi_version: u32,
    pub sample_rate: u32,
    pub bits: u32,
    pub channels: u32,
    pub restart_interval: u32,
    pub frames: u64,
    pub metadata: *const StampRecord,
    pub metadata_count: usize,
}
#[repr(C)]
pub struct EncoderResult {
    pub status: i32,
    pub access_units: u32,
    pub input_frames: u64,
    pub encoded_frames: u64,
    pub output_bytes: u64,
    pub error: [c_char; 192],
}
impl Default for EncoderResult {
    fn default() -> Self {
        Self {
            status: 0,
            access_units: 0,
            input_frames: 0,
            encoded_frames: 0,
            output_bytes: 0,
            error: [0; 192],
        }
    }
}
fn fail(result: &mut EncoderResult, status: i32, message: &str) -> i32 {
    result.status = status;
    for (slot, byte) in result.error.iter_mut().take(191).zip(message.bytes()) {
        *slot = byte as c_char;
    }
    status
}

#[unsafe(no_mangle)]
pub extern "C" fn mlp_encoder_abi_version() -> u32 {
    1
}

/// Synchronous C ABI for standard layouts, uniform depths, and a complete metadata packet.
///
/// # Safety
/// Config, metadata, result and callback buffers must satisfy mlp_encoder.h pointer contracts.
/// Callbacks must not unwind and must write no more than the requested capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mlp_encode_stream(
    config: *const EncoderConfig,
    read: Option<Read>,
    input: *mut c_void,
    write: Option<Write>,
    output: *mut c_void,
    result: *mut EncoderResult,
) -> i32 {
    if result.is_null() {
        return -1;
    }
    unsafe {
        result.write(EncoderResult::default());
    }
    let result = unsafe { &mut *result };
    let outcome = catch_unwind(AssertUnwindSafe(|| unsafe {
        stream(config, read, input, write, output, result, None)
    }));
    match outcome {
        Ok(status) => status,
        Err(_) => fail(result, -5, "Rust encoder panic"),
    }
}
unsafe fn metadata_records(config: &EncoderConfig) -> Result<Vec<Metadata>, &'static str> {
    if config.metadata_count == 0 || config.metadata.is_null() || config.metadata_count > 4096 {
        return Err("Missing metadata timeline");
    }
    let samples = if config.sample_rate > 96000 {
        160
    } else if config.sample_rate > 48000 {
        80
    } else {
        40
    };
    let aus = config
        .frames
        .checked_add(samples - 1)
        .ok_or("Frame count overflow")?
        / samples;
    if aus > u32::MAX as u64 {
        return Err("Too many access units");
    }
    let mut records = Vec::new();
    for record in unsafe { std::slice::from_raw_parts(config.metadata, config.metadata_count) } {
        if record.packet.is_null() || !(4..=65539).contains(&record.size) {
            return Err("Invalid metadata record");
        }
        records.push(Metadata {
            start: record.start,
            packet: unsafe { std::slice::from_raw_parts(record.packet, record.size as usize) }
                .to_vec(),
            valid_bits: record.valid_bits,
        });
    }
    crate::streaming::Stamps::new(&records, aus as usize)
        .map_err(|_| "Invalid metadata timeline")?;
    Ok(records)
}
unsafe fn stream(
    config: *const EncoderConfig,
    read: Option<Read>,
    input: *mut c_void,
    write: Option<Write>,
    output: *mut c_void,
    result: &mut EncoderResult,
    profile: Option<Profile>,
) -> i32 {
    if config.is_null() {
        return fail(result, -1, "Missing configuration");
    }
    let config = unsafe { &*config };
    let (Some(read), Some(write)) = (read, write) else {
        return fail(result, -1, "Missing callback");
    };
    if config.struct_size != size_of::<EncoderConfig>() as u32
        || config.abi_version != 1
        || ![16, 20, 24].contains(&config.bits)
        || !(1..=6).contains(&config.channels)
        || config.restart_interval > 32
        || config.frames == 0
        || config.frames > u64::MAX - 160
        || ![44100, 48000, 88200, 96000, 176400, 192000].contains(&config.sample_rate)
        || (config.sample_rate > 96000 && config.channels > 2)
    {
        return fail(result, -1, "Invalid configuration");
    }
    let c = Config {
        sample_rate: config.sample_rate,
        bits: config.bits as u8,
        channels: config.channels as u8,
        restart_interval: if config.restart_interval == 0 {
            8
        } else {
            config.restart_interval as usize
        },
        metadata: Vec::new(),
    };
    let profile = profile.unwrap_or_else(|| Profile::standard(&c));
    if !profile.valid(&c) {
        return fail(result, -1, "Invalid layout or group format");
    }
    let records = match unsafe { metadata_records(config) } {
        Ok(records) => records,
        Err(message) => return fail(result, -1, message),
    };
    #[cfg(target_arch = "x86_64")]
    {
        let status = crate::c_host::encode_stream(
            config,
            &c,
            &profile,
            &records,
            crate::c_host::HostIo {
                read,
                input,
                write,
                output,
            },
            result,
        );
        if status == 0 {
            0
        } else {
            fail(result, status, "Encoding failed; discard partial output")
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let channels = config.channels as usize;
        let samples = if config.sample_rate > 96000 {
            160
        } else if config.sample_rate > 48000 {
            80
        } else {
            40
        };
        let count = config.frames.div_ceil(samples as u64) as usize;
        let mut stamps =
            crate::streaming::Stamps::new(&records, count).expect("validated metadata");
        // Buffer one restart interval so matrix decisions are bounded and stable.
        let mut encoder = crate::AuEncoder::new(&c, &profile, 0);
        let mut queue = crate::timing::Queue::new(samples as u32);
        let mut buffer = vec![0; samples * channels * c.restart_interval];
        let deliver = |bytes: Vec<u8>, result: &mut EncoderResult| -> Result<(), i32> {
            if unsafe { write(output, bytes.as_ptr(), bytes.len()) } != 0 {
                return Err(-4);
            }
            result.access_units += 1;
            result.output_bytes += bytes.len() as u64;
            Ok(())
        };
        for base in (0..count).step_by(c.restart_interval) {
            let aus = (count - base).min(c.restart_interval);
            let needed = (config.frames - result.input_frames).min((aus * samples) as u64) as usize;
            let mut at = 0;
            while at < needed {
                let mut received = 0;
                let capacity = (needed - at).min(samples);
                if unsafe {
                    read(
                        input,
                        buffer[at * channels..].as_mut_ptr(),
                        capacity,
                        &mut received,
                    )
                } != 0
                    || received == 0
                    || received > capacity
                {
                    return fail(result, -3, "Input callback failed or cancelled");
                }
                at += received;
                result.input_frames += received as u64;
            }
            encoder.matrix = crate::pair_matrix(&c, &profile, &buffer[..needed * channels]);
            encoder.quantization =
                crate::quantization(&c, &profile, &buffer[..needed * channels], encoder.matrix);
            for offset in 0..aus {
                let au = base + offset;
                let start = offset * samples * channels;
                let end = ((offset + 1) * samples).min(needed) * channels;
                let bytes =
                    match encoder.push(&buffer[start..end], stamps.next(au), au + 1 == count) {
                        Ok(bytes) => bytes,
                        Err(Error::InvalidPcm) => return fail(result, -3, "Invalid PCM precision"),
                        Err(_) => return fail(result, -5, "MLP encoding failed"),
                    };
                match queue.push(bytes) {
                    Ok(Some(bytes)) => {
                        if let Err(status) = deliver(bytes, result) {
                            return fail(result, status, "Output callback failed or cancelled");
                        }
                    }
                    Ok(None) => {}
                    Err(_) => return fail(result, -5, "MLP decoder FIFO overflow"),
                }
            }
        }
        loop {
            match queue.finish() {
                Ok(Some(bytes)) => {
                    if let Err(status) = deliver(bytes, result) {
                        return fail(result, status, "Output callback failed or cancelled");
                    }
                }
                Ok(None) => break,
                Err(_) => return fail(result, -5, "MLP decoder FIFO overflow"),
            }
        }
        result.encoded_frames = count as u64 * samples as u64;
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct State {
        reads: usize,
        writes: usize,
        input_error: bool,
        output_error: bool,
    }
    unsafe extern "C" fn read(
        p: *mut c_void,
        samples: *mut i32,
        capacity: usize,
        frames: *mut usize,
    ) -> i32 {
        let s = unsafe { &mut *p.cast::<State>() };
        s.reads += 1;
        if s.input_error {
            return 1;
        }
        let count = capacity.min(7);
        unsafe {
            std::slice::from_raw_parts_mut(samples, count * 2).fill(0);
            *frames = count;
        }
        0
    }
    unsafe extern "C" fn write(p: *mut c_void, _: *const u8, _: usize) -> i32 {
        let s = unsafe { &mut *p.cast::<State>() };
        s.writes += 1;
        i32::from(s.output_error && s.writes == 2)
    }
    #[test]
    fn callbacks_results_and_failures() {
        let packet = [0, 0, 0x40, 0];
        let record = StampRecord {
            start: 0,
            size: 4,
            packet: packet.as_ptr(),
            valid_bits: 0,
        };
        let config = EncoderConfig {
            struct_size: size_of::<EncoderConfig>() as u32,
            abi_version: 1,
            sample_rate: 48000,
            bits: 16,
            channels: 2,
            restart_interval: 0,
            frames: 41,
            metadata: &record,
            metadata_count: 1,
        };
        for frames in [u64::MAX - 159, u64::MAX] {
            let invalid = EncoderConfig { frames, ..config };
            let mut state = State {
                reads: 0,
                writes: 0,
                input_error: false,
                output_error: false,
            };
            let p = (&mut state as *mut State).cast();
            let mut result = EncoderResult::default();
            assert_eq!(
                unsafe { mlp_encode_stream(&invalid, Some(read), p, Some(write), p, &mut result) },
                -1
            );
            assert_eq!((state.reads, state.writes), (0, 0));
            assert_eq!(result.encoded_frames, 0);
        }
        for (input_error, output_error, status) in
            [(false, false, 0), (true, false, -3), (false, true, -4)]
        {
            let mut state = State {
                reads: 0,
                writes: 0,
                input_error,
                output_error,
            };
            let p = (&mut state as *mut State).cast();
            let mut result = EncoderResult::default();
            assert_eq!(
                unsafe { mlp_encode_stream(&config, Some(read), p, Some(write), p, &mut result) },
                status
            );
            assert_eq!(result.status, status);
            if status == 0 {
                assert_eq!(result.input_frames, 41);
                assert_eq!(result.encoded_frames, 80);
                assert_eq!(result.access_units, 2);
            } else {
                assert_ne!(result.error[0], 0);
                assert_eq!(result.encoded_frames, 0);
                if output_error {
                    assert_eq!(result.access_units, 1);
                    assert!(result.output_bytes > 0);
                } else {
                    assert_eq!(result.output_bytes, 0);
                }
            }
        }
    }
}
