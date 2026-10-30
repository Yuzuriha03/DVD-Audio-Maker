#[cfg(not(target_arch = "x86_64"))]
use crate::Error;
use crate::{Config, format::Profile, metadata::Metadata};
use std::ffi::{c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
#[path = "ffi_callbacks.rs"]
pub(crate) mod callbacks;
#[path = "ffi_layout.rs"]
mod layout;
pub use layout::*;

/// Reads interleaved PCM frames into a caller-owned buffer.
///
/// # Safety
/// The encoder supplies storage for `capacity * channels` samples and a writable
/// frame count. The callback must initialize the count on success, write at most
/// `capacity` frames, and neither retain buffer pointers nor unwind. Its opaque
/// context must remain valid for the synchronous encoding call. The PCM buffer
/// and frame count are initialized, exclusive borrows for this invocation. Shared
/// opaque contexts are allowed only with sequential access; a callback must not
/// invalidate another callback's context or access encoder-owned data through an alias.
pub type Read = unsafe extern "C" fn(*mut c_void, *mut i32, usize, *mut usize) -> i32;
/// Receives one access unit; the bytes are valid only during the callback.
///
/// # Safety
/// The callback must not mutate or retain the byte buffer, or unwind. Its opaque
/// context must remain valid for the synchronous encoding call. Shared contexts
/// obey the same sequential-access and non-invalidation rules as [`Read`].
pub type Write = unsafe extern "C" fn(*mut c_void, *const u8, usize) -> i32;

pub(crate) struct StreamCall<'a> {
    pub config: *const EncoderConfig,
    pub read: Option<Read>,
    pub input: *mut c_void,
    pub write: Option<Write>,
    pub output: *mut c_void,
    pub profile: Option<Profile>,
    pub stamp_refresh: Option<&'a mut dyn FnMut() -> Result<Option<Metadata>, i32>>,
}

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
pub(crate) fn fail(result: &mut EncoderResult, status: i32, message: &str) -> i32 {
    result.status = status;
    result.error.fill(0);
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
/// Config may be null (rejected), otherwise it must point to a readable, aligned
/// `EncoderConfig`; its metadata array and packet buffers must be valid for their
/// declared lengths throughout the call, initialized and not mutated (including
/// by callbacks or other threads). Result may be null (rejected), otherwise
/// it must point to aligned writable storage for one `EncoderResult`, exclusively
/// accessible to the encoder and disjoint from config, metadata and callback data.
/// Callbacks and opaque contexts must satisfy the [`Read`] and [`Write`] contracts.
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
        stream(
            StreamCall {
                config,
                read,
                input,
                write,
                output,
                profile: None,
                stamp_refresh: None,
            },
            result,
        )
    }));
    match outcome {
        Ok(status) => status,
        Err(_) => fail(result, -5, "Rust encoder panic"),
    }
}
/// # Safety
/// When `metadata_count` is non-zero and `metadata` is non-null, `metadata` must be valid for
/// `metadata_count` reads, and each record's `packet` must be valid for `size` reads. Null pointers
/// and out-of-range counts and sizes are rejected here; the extent of `packet` is not checked.
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
    if aus > u64::from(u32::MAX) {
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
    crate::streaming::MetadataCursor::new(&records, aus as usize)
        .map_err(|_| "Invalid metadata timeline")?;
    Ok(records)
}
/// # Safety
/// `call.config` must be null or point to a readable `EncoderConfig`. `call.read`, `call.write`,
/// `call.input` and `call.output` must follow the `Read`/`Write` callback contracts for the whole call.
/// `result` must be valid for writes and not aliased. A `stamp_refresh` callback is only supported on
/// `x86_64`; other targets reject it with -1.
unsafe fn stream(call: StreamCall<'_>, result: &mut EncoderResult) -> i32 {
    let StreamCall {
        config,
        read,
        input,
        write,
        output,
        profile,
        stamp_refresh,
    } = call;
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
    // SAFETY: stream's caller supplies live callback contexts; validation above
    // establishes the frame width. The adapters never escape this call.
    let mut reader = unsafe { callbacks::Reader::new(read, input, c.channels as usize) };
    let mut writer = unsafe { callbacks::Writer::new(write, output) };
    #[cfg(target_arch = "x86_64")]
    {
        let status = crate::pipeline::host::encode_stream(
            config,
            &c,
            &profile,
            &records,
            crate::pipeline::host::HostIo {
                read: &mut reader,
                write: &mut writer,
                stamp_refresh,
            },
            result,
        );
        if status == 0 {
            0
        } else if result.error[0] != 0 {
            result.status = status;
            status
        } else {
            fail(result, status, "Encoding failed; discard partial output")
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        if stamp_refresh.is_some() {
            return fail(
                result,
                -1,
                "Live metadata refresh requires the x86_64 encoder",
            );
        }
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
            crate::streaming::MetadataCursor::new(&records, count).expect("validated metadata");
        // Buffer one restart interval so matrix decisions are bounded and stable.
        let mut encoder = crate::encoder::AuEncoder::new(&c, &profile, 0);
        let mut queue = crate::timing::Queue::new(samples as u32);
        let mut buffer = vec![0; samples * channels * c.restart_interval];
        let mut deliver = |bytes: Vec<u8>, result: &mut EncoderResult| -> Result<(), i32> {
            if writer.write(&bytes).is_err() {
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
                let capacity = (needed - at).min(samples);
                let Ok(received) = reader.read(&mut buffer[at * channels..], capacity) else {
                    return fail(result, -3, "Input callback failed or cancelled");
                };
                at += received;
                result.input_frames += received as u64;
            }
            encoder.matrix =
                crate::encoder::pair_matrix(&c, &profile, &buffer[..needed * channels]);
            encoder.quantization = crate::encoder::quantization(
                &c,
                &profile,
                &buffer[..needed * channels],
                encoder.matrix,
            );
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

// Callbacks, opaque state and configuration obey the same contracts as stream().
/// # Safety
/// `config` must point to a readable `EncoderConfig`; `read`, `input`, `write` and `output` must follow
/// the `Read`/`Write` callback contracts for the whole call. `stamp_refresh` is only called during this
/// call, and only on `x86_64`.
pub(crate) unsafe fn encode_stream_with_stamp_refresh(
    config: &EncoderConfig,
    profile: Profile,
    read: Read,
    input: *mut c_void,
    write: Write,
    output: *mut c_void,
    stamp_refresh: &mut dyn FnMut() -> Result<Option<Metadata>, i32>,
) -> EncoderResult {
    let mut result = EncoderResult::default();
    let outcome = catch_unwind(AssertUnwindSafe(|| unsafe {
        stream(
            StreamCall {
                config,
                read: Some(read),
                input,
                write: Some(write),
                output,
                profile: Some(profile),
                stamp_refresh: Some(stamp_refresh),
            },
            &mut result,
        )
    }));
    if outcome.is_err() {
        fail(&mut result, -5, "Rust encoder panic");
    }
    result
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
    #[cfg(target_pointer_width = "64")]
    fn abi_v1_layout_is_stable() {
        use std::mem::{align_of, offset_of};
        assert_eq!(mlp_encoder_abi_version(), 1);
        assert_eq!(
            (size_of::<StampRecord>(), align_of::<StampRecord>()),
            (32, 8)
        );
        assert_eq!(offset_of!(StampRecord, start), 0);
        assert_eq!(offset_of!(StampRecord, size), 8);
        assert_eq!(offset_of!(StampRecord, packet), 16);
        assert_eq!(offset_of!(StampRecord, valid_bits), 24);
        assert_eq!(
            (size_of::<EncoderConfig>(), align_of::<EncoderConfig>()),
            (48, 8)
        );
        assert_eq!(offset_of!(EncoderConfig, struct_size), 0);
        assert_eq!(offset_of!(EncoderConfig, abi_version), 4);
        assert_eq!(offset_of!(EncoderConfig, sample_rate), 8);
        assert_eq!(offset_of!(EncoderConfig, bits), 12);
        assert_eq!(offset_of!(EncoderConfig, channels), 16);
        assert_eq!(offset_of!(EncoderConfig, restart_interval), 20);
        assert_eq!(offset_of!(EncoderConfig, frames), 24);
        assert_eq!(offset_of!(EncoderConfig, metadata), 32);
        assert_eq!(offset_of!(EncoderConfig, metadata_count), 40);
        assert_eq!(
            (size_of::<GroupsConfig>(), align_of::<GroupsConfig>()),
            (80, 8)
        );
        assert_eq!(offset_of!(GroupsConfig, struct_size), 0);
        assert_eq!(offset_of!(GroupsConfig, primary), 8);
        assert_eq!(offset_of!(GroupsConfig, assignment), 56);
        assert_eq!(offset_of!(GroupsConfig, group2_sample_rate), 60);
        assert_eq!(offset_of!(GroupsConfig, group2_bits), 64);
        assert_eq!(offset_of!(GroupsConfig, group2_frames), 72);
        assert_eq!(
            (size_of::<EncoderResult>(), align_of::<EncoderResult>()),
            (224, 8)
        );
        assert_eq!(offset_of!(EncoderResult, status), 0);
        assert_eq!(offset_of!(EncoderResult, access_units), 4);
        assert_eq!(offset_of!(EncoderResult, input_frames), 8);
        assert_eq!(offset_of!(EncoderResult, encoded_frames), 16);
        assert_eq!(offset_of!(EncoderResult, output_bytes), 24);
        assert_eq!(offset_of!(EncoderResult, error), 32);
    }

    #[test]
    fn null_boundaries_initialize_results_and_preserve_error_priority() {
        use std::ptr::{null, null_mut};
        for entry in 0..4 {
            let invoke = |result| unsafe {
                match entry {
                    0 => mlp_encode_stream(null(), None, null_mut(), None, null_mut(), result),
                    1 => mlp_encode_stream_layout(
                        null(),
                        0,
                        None,
                        null_mut(),
                        None,
                        null_mut(),
                        result,
                    ),
                    2 => mlp_encode_stream_depths(
                        null(),
                        0,
                        0,
                        None,
                        null_mut(),
                        None,
                        null_mut(),
                        result,
                    ),
                    _ => mlp_encode_stream_groups(
                        null(),
                        None,
                        null_mut(),
                        None,
                        null_mut(),
                        None,
                        null_mut(),
                        result,
                    ),
                }
            };
            assert_eq!(invoke(null_mut()), -1);
            let mut result = std::mem::MaybeUninit::<EncoderResult>::uninit();
            assert_eq!(invoke(result.as_mut_ptr()), -1);
            let result = unsafe { result.assume_init() };
            assert_eq!(result.status, -1);
            assert_eq!(result.access_units, 0);
            assert_eq!(result.input_frames, 0);
            assert_eq!(result.encoded_frames, 0);
            assert_eq!(result.output_bytes, 0);
            let expected = if entry == 3 {
                "Missing group configuration"
            } else {
                "Missing configuration"
            };
            let mut error = [0; 192];
            for (slot, byte) in error.iter_mut().zip(expected.bytes()) {
                *slot = byte as c_char;
            }
            assert_eq!(result.error, error);
        }
    }

    #[test]
    fn failure_text_is_truncated_terminated_and_cleared() {
        let mut result = EncoderResult {
            access_units: 7,
            ..Default::default()
        };
        assert_eq!(fail(&mut result, -4, &"x".repeat(250)), -4);
        assert!(result.error[..191].iter().all(|&c| c == b'x' as c_char));
        assert_eq!(result.error[191], 0);
        assert_eq!(fail(&mut result, -5, "short"), -5);
        assert_eq!(result.status, -5);
        assert_eq!(result.access_units, 7);
        assert!(result.error[5..].iter().all(|&c| c == 0));
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
            metadata: &raw const record,
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
            let p = (&raw mut state).cast();
            let mut result = EncoderResult::default();
            assert_eq!(
                unsafe {
                    mlp_encode_stream(
                        &raw const invalid,
                        Some(read),
                        p,
                        Some(write),
                        p,
                        &raw mut result,
                    )
                },
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
            let p = (&raw mut state).cast();
            let mut result = EncoderResult::default();
            assert_eq!(
                unsafe {
                    mlp_encode_stream(
                        &raw const config,
                        Some(read),
                        p,
                        Some(write),
                        p,
                        &raw mut result,
                    )
                },
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
