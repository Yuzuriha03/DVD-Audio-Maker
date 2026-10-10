//! Safe, synchronous direct-link streaming API. No DLL loading is involved.
use crate::{Config, ffi, format::Profile, metadata::Metadata};
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// PCM is signed 24-bit, left-aligned for 16/20-bit precision. Reads return frames,
/// not samples, and may fragment a request. Returning zero or `Err` cancels input.
/// Grouped reads use DVD assignment order within each group; uniform reads use
/// canonical speaker order. Output is one access unit per call.
pub trait StreamIo {
    fn read(&mut self, group: usize, pcm: &mut [i32]) -> Result<usize, i32>;
    fn write(&mut self, access_unit: &[u8]) -> Result<(), i32>;
    fn cancelled(&mut self) -> bool {
        false
    }
}

/// Counts and statuses follow the differential ABI: 0 success, -1 invalid
/// configuration, -2 allocation/size failure, -3 input/cancellation, -4 output,
/// -5 encoding failure/panic. Discard all output on any nonzero final status.
/// Allocation exhaustion in internal infallible allocations can still abort.
pub type StreamResult = ffi::EncoderResult;

struct Context<'a, I> {
    io: &'a mut I,
    channels: [usize; 2],
    panicked: bool,
}
unsafe extern "C" fn read<I: StreamIo>(
    p: *mut c_void,
    pcm: *mut i32,
    capacity: usize,
    frames: *mut usize,
) -> i32 {
    unsafe { read_group::<I>(p, pcm, capacity, frames, 0) }
}
unsafe extern "C" fn read_second<I: StreamIo>(
    p: *mut c_void,
    pcm: *mut i32,
    capacity: usize,
    frames: *mut usize,
) -> i32 {
    unsafe { read_group::<I>(p, pcm, capacity, frames, 1) }
}
unsafe fn read_group<I: StreamIo>(
    p: *mut c_void,
    pcm: *mut i32,
    capacity: usize,
    frames: *mut usize,
    group: usize,
) -> i32 {
    let context = unsafe { &mut *p.cast::<Context<'_, I>>() };
    match catch_unwind(AssertUnwindSafe(|| {
        if context.io.cancelled() {
            return Err(-3);
        }
        let buffer =
            unsafe { std::slice::from_raw_parts_mut(pcm, capacity * context.channels[group]) };
        context.io.read(group, buffer)
    })) {
        Ok(Ok(count)) if count <= capacity => {
            unsafe {
                *frames = count;
            }
            0
        }
        Ok(_) => -3,
        Err(_) => {
            context.panicked = true;
            -3
        }
    }
}
unsafe extern "C" fn write<I: StreamIo>(p: *mut c_void, bytes: *const u8, size: usize) -> i32 {
    let context = unsafe { &mut *p.cast::<Context<'_, I>>() };
    match catch_unwind(AssertUnwindSafe(|| {
        if context.io.cancelled() {
            return Err(-4);
        }
        context
            .io
            .write(unsafe { std::slice::from_raw_parts(bytes, size) })
    })) {
        Ok(Ok(())) => 0,
        Ok(Err(_)) => -4,
        Err(_) => {
            context.panicked = true;
            -4
        }
    }
}

/// Encode with bounded restart/FIFO buffering and shared ABI validation/timing.
/// `secondary_frames = Some(n)` selects independent native-rate group input;
/// `None` selects interleaved uniform-rate input, including mixed-depth layouts.
/// Callback panics are caught before crossing an extern boundary and return -5.
/// Calls are independent and may run concurrently or reentrantly.
pub fn encode_stream<I: StreamIo>(
    config: &Config,
    profile: &Profile,
    frames: u64,
    secondary_frames: Option<u64>,
    metadata: &[Metadata],
    io: &mut I,
) -> StreamResult {
    let mut result = StreamResult::default();
    if !(1..=6).contains(&config.channels)
        || !profile.valid(config)
        || config.restart_interval > 32
        || metadata.len() > 4096
    {
        result.status = -1;
        return result;
    }
    let mut records = Vec::new();
    if records.try_reserve_exact(metadata.len()).is_err() {
        result.status = -2;
        return result;
    }
    for record in metadata {
        let Ok(size) = u32::try_from(record.packet.len()) else {
            result.status = -1;
            return result;
        };
        records.push(ffi::StampRecord {
            start: record.start,
            size,
            packet: record.packet.as_ptr(),
            valid_bits: record.valid_bits,
        });
    }
    let primary = ffi::EncoderConfig {
        struct_size: size_of::<ffi::EncoderConfig>() as u32,
        abi_version: 1,
        sample_rate: config.sample_rate,
        bits: config.bits.into(),
        channels: config.channels.into(),
        restart_interval: config.restart_interval as u32,
        frames,
        metadata: records.as_ptr(),
        metadata_count: records.len(),
    };
    let first = crate::format::FIRST[profile.assignment];
    let mut context = Context {
        io,
        channels: if secondary_frames.is_some() {
            [first, config.channels as usize - first]
        } else {
            [config.channels as usize, 0]
        },
        panicked: false,
    };
    let p = (&mut context as *mut Context<'_, I>).cast();
    unsafe {
        if let Some(group2_frames) = secondary_frames {
            let groups = ffi::GroupsConfig {
                struct_size: size_of::<ffi::GroupsConfig>() as u32,
                primary,
                assignment: profile.assignment as u32,
                group2_sample_rate: profile.group2_rate,
                group2_bits: profile.group2_bits.into(),
                group2_frames,
            };
            ffi::mlp_encode_stream_groups(
                &groups,
                Some(read::<I>),
                p,
                Some(read_second::<I>),
                p,
                Some(write::<I>),
                p,
                &mut result,
            );
        } else if profile.group2_rate != config.sample_rate {
            result.status = -1;
        } else if first < config.channels as usize {
            ffi::mlp_encode_stream_depths(
                &primary,
                profile.assignment as u32,
                profile.group2_bits.into(),
                Some(read::<I>),
                p,
                Some(write::<I>),
                p,
                &mut result,
            );
        } else {
            ffi::mlp_encode_stream_layout(
                &primary,
                profile.assignment as u32,
                Some(read::<I>),
                p,
                Some(write::<I>),
                p,
                &mut result,
            );
        }
    }
    if context.panicked {
        result.status = -5;
        result.error.fill(0);
        for (slot, byte) in result.error.iter_mut().zip(b"Rust stream callback panic") {
            *slot = *byte as _;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Io {
        mode: u8,
        output: Vec<u8>,
    }
    impl StreamIo for Io {
        fn read(&mut self, _: usize, pcm: &mut [i32]) -> Result<usize, i32> {
            if self.mode == 1 {
                panic!("input panic");
            }
            pcm.fill(0);
            Ok(if self.mode == 2 {
                usize::MAX
            } else {
                pcm.len() / 2
            })
        }
        fn write(&mut self, bytes: &[u8]) -> Result<(), i32> {
            if self.mode == 3 {
                panic!("output panic");
            }
            if self.mode == 4 {
                return Err(1);
            }
            self.output.extend_from_slice(bytes);
            Ok(())
        }
        fn cancelled(&mut self) -> bool {
            self.mode == 5
        }
    }
    #[test]
    fn grouped_direct_streams_are_bounded_and_independent() {
        struct Groups {
            channels: [usize; 2],
            reads: [usize; 2],
            panic_group: Option<usize>,
            output: Vec<u8>,
        }
        impl StreamIo for Groups {
            fn read(&mut self, group: usize, pcm: &mut [i32]) -> Result<usize, i32> {
                assert_ne!(self.panic_group, Some(group), "group callback panic");
                assert_eq!(pcm.len() % self.channels[group], 0);
                let frames = (pcm.len() / self.channels[group]).min(7);
                pcm[..frames * self.channels[group]].fill(0);
                self.reads[group] += frames;
                Ok(frames)
            }
            fn write(&mut self, bytes: &[u8]) -> Result<(), i32> {
                self.output.extend_from_slice(bytes);
                Ok(())
            }
        }
        let run = |rate, panic_group| {
            let config = Config {
                sample_rate: 96000,
                bits: 24,
                channels: 6,
                restart_interval: 8,
                metadata: Vec::new(),
            };
            let profile = Profile {
                assignment: 20,
                group2_bits: 20,
                group2_rate: rate,
            };
            let first = crate::format::FIRST[profile.assignment];
            let mut io = Groups {
                channels: [first, 6 - first],
                reads: [0; 2],
                panic_group,
                output: Vec::new(),
            };
            let secondary = 802 / (96000 / rate) as u64;
            let records = [Metadata {
                start: 0,
                packet: vec![0, 0, 64, 0],
                valid_bits: 0,
            }];
            let result = encode_stream(&config, &profile, 802, Some(secondary), &records, &mut io);
            if panic_group.is_some() {
                assert_eq!(result.status, -5);
            } else {
                assert_eq!(result.status, 0);
                assert_eq!(result.input_frames, 802);
                assert_eq!(io.reads, [802, secondary as usize]);
                assert_eq!(result.output_bytes, io.output.len() as u64);
            }
            let invalid = encode_stream(
                &config,
                &profile,
                802,
                Some(secondary + 1),
                &records,
                &mut io,
            );
            assert_eq!(invalid.status, -1);
            io.output
        };
        for rate in [48000, 96000] {
            let expected = run(rate, None);
            std::thread::scope(|scope| {
                let calls: Vec<_> = (0..8).map(|_| scope.spawn(|| run(rate, None))).collect();
                for call in calls {
                    assert_eq!(call.join().unwrap(), expected);
                }
            });
            run(rate, Some(0));
            run(rate, Some(1));
        }
    }

    #[test]
    fn bounds_panics_cancellation_and_success() {
        let config = Config {
            sample_rate: 48000,
            bits: 24,
            channels: 2,
            restart_interval: 8,
            metadata: Vec::new(),
        };
        let profile = Profile::standard(&config);
        let records = [Metadata {
            start: 0,
            packet: vec![0, 0, 64, 0],
            valid_bits: 0,
        }];
        for (mode, status) in [(0, 0), (1, -5), (2, -3), (3, -5), (4, -4), (5, -3)] {
            let mut io = Io {
                mode,
                output: Vec::new(),
            };
            let r = encode_stream(&config, &profile, 41, None, &records, &mut io);
            assert_eq!(r.status, status);
            if status == 0 {
                assert_eq!(r.input_frames, 41);
                assert_eq!(r.encoded_frames, 80);
                assert_eq!(r.output_bytes, io.output.len() as u64);
            }
        }
    }
}
