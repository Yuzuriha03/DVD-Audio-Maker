use super::*;
use crate::format::{FIRST, SPEAKERS};
#[repr(C)]
pub struct GroupsConfig {
    pub struct_size: u32,
    pub primary: EncoderConfig,
    pub assignment: u32,
    pub group2_sample_rate: u32,
    pub group2_bits: u32,
    pub group2_frames: u64,
}
unsafe fn entry(
    config: *const EncoderConfig,
    profile: Profile,
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
    let r = unsafe { &mut *result };
    match catch_unwind(AssertUnwindSafe(|| unsafe {
        stream(config, read, input, write, output, r, Some(profile))
    })) {
        Ok(status) => status,
        Err(_) => fail(r, -5, "Rust encoder panic"),
    }
}
/// # Safety
/// All pointers and callbacks must obey mlp_encoder.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mlp_encode_stream_layout(
    config: *const EncoderConfig,
    assignment: u32,
    read: Option<Read>,
    input: *mut c_void,
    write: Option<Write>,
    output: *mut c_void,
    result: *mut EncoderResult,
) -> i32 {
    let (bits, rate) = if config.is_null() {
        (0, 0)
    } else {
        let c = unsafe { &*config };
        (c.bits, c.sample_rate)
    };
    unsafe {
        entry(
            config,
            Profile {
                assignment: assignment as usize,
                group2_bits: bits as u8,
                group2_rate: rate,
            },
            read,
            input,
            write,
            output,
            result,
        )
    }
}
/// # Safety
/// All pointers and callbacks must obey mlp_encoder.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mlp_encode_stream_depths(
    config: *const EncoderConfig,
    assignment: u32,
    group2_bits: u32,
    read: Option<Read>,
    input: *mut c_void,
    write: Option<Write>,
    output: *mut c_void,
    result: *mut EncoderResult,
) -> i32 {
    if assignment < 21 && FIRST[assignment as usize] == SPEAKERS[assignment as usize].len()
        || ![16, 20, 24].contains(&group2_bits)
    {
        return unsafe {
            entry(
                std::ptr::null(),
                Profile {
                    assignment: 21,
                    group2_bits: 0,
                    group2_rate: 0,
                },
                read,
                input,
                write,
                output,
                result,
            )
        };
    }
    let rate = if config.is_null() {
        0
    } else {
        unsafe { (*config).sample_rate }
    };
    unsafe {
        entry(
            config,
            Profile {
                assignment: assignment as usize,
                group2_bits: group2_bits as u8,
                group2_rate: rate,
            },
            read,
            input,
            write,
            output,
            result,
        )
    }
}

struct GroupSource {
    primary: (Read, *mut c_void),
    secondary: (Read, *mut c_void),
    frames: usize,
    position: usize,
    loaded: usize,
    ratio: usize,
    first: usize,
    second: usize,
    bits: u32,
    order: Vec<usize>,
    ring: Vec<i32>,
    rng: Vec<u32>,
    panicked: bool,
}
impl GroupSource {
    unsafe fn load(&mut self, need: usize) -> Result<(), ()> {
        let end = need.min(self.frames / self.ratio);
        let mut buffer = vec![0; 160 * self.second];
        while self.loaded < end {
            let capacity = (end - self.loaded).min(160);
            let mut got = 0;
            if unsafe {
                (self.secondary.0)(self.secondary.1, buffer.as_mut_ptr(), capacity, &mut got)
            } != 0
                || got == 0
                || got > capacity
            {
                return Err(());
            }
            for (n, frame) in buffer[..got * self.second]
                .chunks_exact(self.second)
                .enumerate()
            {
                for (ch, &x) in frame.iter().enumerate() {
                    if !(-8388608..=8388607).contains(&x) || x & ((1 << (24 - self.bits)) - 1) != 0
                    {
                        return Err(());
                    }
                    self.ring[((self.loaded + n) % 512) * self.second + ch] = x;
                }
            }
            self.loaded += got;
        }
        Ok(())
    }
    fn sample(&self, n: isize, ch: usize) -> i32 {
        if n < 0 || n >= (self.frames / self.ratio) as isize {
            0
        } else {
            self.ring[(n as usize % 512) * self.second + ch]
        }
    }
    fn midpoint(&mut self, low: usize, ch: usize) -> i32 {
        #[cfg(target_arch = "x86_64")]
        let mut value = crate::extended::Extended::from_f64(0.0);
        #[cfg(not(target_arch = "x86_64"))]
        let mut value = 0.0;
        for (i, &coefficient) in crate::coefficients::COEFFICIENTS.iter().enumerate() {
            let sample = f64::from(
                self.sample(low as isize - i as isize, ch)
                    + self.sample(low as isize + i as isize + 1, ch),
            );
            #[cfg(target_arch = "x86_64")]
            {
                value = value.add(
                    crate::extended::Extended::from_f64(sample)
                        .mul(crate::extended::Extended::from_f64(coefficient)),
                );
            }
            #[cfg(not(target_arch = "x86_64"))]
            {
                value += sample * coefficient;
            }
        }
        let u = ((self.rng[ch] as i32) >> 7) as u32;
        self.rng[ch] = ((((self.rng[ch] & 127) << 11) ^ u) << 5) ^ u;
        let dither =
            f64::from((u >> 8) as i8 as i32 + u as i8 as i32) * 2f64.powi(16 - self.bits as i32);
        let step = 1i64 << (24 - self.bits);
        #[cfg(target_arch = "x86_64")]
        let rounded = value
            .add(crate::extended::Extended::from_f64(dither))
            .to_f64()
            * 2f64.powi(self.bits as i32 - 24)
            + 0.5;
        #[cfg(not(target_arch = "x86_64"))]
        let rounded = (value + dither) * 2f64.powi(self.bits as i32 - 24) + 0.5;
        (rounded.floor() as i64 * step).clamp(-8388608, 8388608 - step) as i32
    }
}
unsafe extern "C" fn group_read(
    p: *mut c_void,
    pcm: *mut i32,
    capacity: usize,
    frames: *mut usize,
) -> i32 {
    match catch_unwind(AssertUnwindSafe(|| unsafe {
        group_read_inner(p, pcm, capacity, frames)
    })) {
        Ok(status) => status,
        Err(_) => {
            unsafe {
                (*p.cast::<GroupSource>()).panicked = true;
            }
            -1
        }
    }
}
unsafe fn group_read_inner(
    p: *mut c_void,
    pcm: *mut i32,
    capacity: usize,
    frames: *mut usize,
) -> i32 {
    let s = unsafe { &mut *p.cast::<GroupSource>() };
    if capacity == 0 || capacity > 160 {
        return -1;
    }
    let count = capacity.min(s.frames - s.position);
    if count == 0 {
        unsafe {
            *frames = 0;
        }
        return 0;
    }
    let mut primary = vec![0; count * s.first];
    let mut done = 0;
    while done < count {
        let mut got = 0;
        if unsafe {
            (s.primary.0)(
                s.primary.1,
                primary[done * s.first..].as_mut_ptr(),
                count - done,
                &mut got,
            )
        } != 0
            || got == 0
            || got > count - done
        {
            return -1;
        }
        done += got;
    }
    let last = (s.position + count - 1) / s.ratio;
    if unsafe { s.load(last + 1 + if s.ratio == 2 { 103 } else { 0 }) }.is_err() {
        return -1;
    }
    #[cfg(target_arch = "x86_64")]
    let _codec = crate::extended::HostFpScope::codec();
    let channels = s.first + s.second;
    let pcm = unsafe { std::slice::from_raw_parts_mut(pcm, count * channels) };
    for n in 0..count {
        let at = s.position + n;
        let low = at / s.ratio;
        for ch in 0..s.first {
            pcm[n * channels + s.order[ch]] = primary[n * s.first + ch];
        }
        for ch in 0..s.second {
            let x = if s.ratio == 2 && at % 2 == 1 {
                s.midpoint(low, ch)
            } else {
                s.sample(low as isize, ch)
            };
            pcm[n * channels + s.order[s.first + ch]] = x;
        }
    }
    s.position += count;
    unsafe {
        *frames = count;
    }
    0
}
/// # Safety
/// All pointers and callbacks must obey mlp_encoder.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mlp_encode_stream_groups(
    config: *const GroupsConfig,
    read1: Option<Read>,
    input1: *mut c_void,
    read2: Option<Read>,
    input2: *mut c_void,
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
    let r = unsafe { &mut *result };
    match catch_unwind(AssertUnwindSafe(|| unsafe {
        groups(config, (read1, input1), (read2, input2), write, output, r)
    })) {
        Ok(status) => status,
        Err(_) => fail(r, -5, "Rust encoder panic"),
    }
}
unsafe fn groups(
    config: *const GroupsConfig,
    first_input: (Option<Read>, *mut c_void),
    second_input: (Option<Read>, *mut c_void),
    write: Option<Write>,
    output: *mut c_void,
    r: &mut EncoderResult,
) -> i32 {
    let (read1, input1) = first_input;
    let (read2, input2) = second_input;
    if config.is_null() {
        return fail(r, -1, "Missing group configuration");
    }
    let g = unsafe { &*config };
    let c = &g.primary;
    let (Some(read1), Some(read2), Some(_)) = (read1, read2, write) else {
        return fail(r, -1, "Missing group callback");
    };
    let profile = Profile {
        assignment: g.assignment as usize,
        group2_bits: g.group2_bits as u8,
        group2_rate: g.group2_sample_rate,
    };
    let core = Config {
        sample_rate: c.sample_rate,
        bits: c.bits as u8,
        channels: c.channels as u8,
        restart_interval: 8,
        metadata: Vec::new(),
    };
    if g.struct_size != size_of::<GroupsConfig>() as u32
        || c.struct_size != size_of::<EncoderConfig>() as u32
        || c.abi_version != 1
        || ![16, 20, 24].contains(&c.bits)
        || ![16, 20, 24].contains(&g.group2_bits)
        || !profile.valid(&core)
        || g.assignment >= 21
        || FIRST[g.assignment as usize] == c.channels as usize
        || c.frames == 0
        || ![44100, 48000, 88200, 96000].contains(&c.sample_rate)
        || c.restart_interval > 32
    {
        return fail(r, -1, "Invalid group format");
    }
    if let Err(message) = unsafe { metadata_records(c) } {
        return fail(r, -1, message);
    }
    let ratio = c.sample_rate / g.group2_sample_rate;
    if c.frames % u64::from(ratio) != 0 || g.group2_frames != c.frames / u64::from(ratio) {
        return fail(r, -1, "Unequal group durations");
    }
    let Ok(frames) = usize::try_from(c.frames) else {
        return fail(r, -2, "Frame overflow");
    };
    let first = FIRST[profile.assignment];
    let second = c.channels as usize - first;
    let mut source = GroupSource {
        primary: (read1, input1),
        secondary: (read2, input2),
        frames,
        position: 0,
        loaded: 0,
        ratio: ratio as usize,
        first,
        second,
        bits: g.group2_bits,
        order: profile.order(),
        ring: vec![0; 512 * second],
        rng: (first..first + second)
            .map(|ch| ((ch as u32 * 214013 + 2531011) >> 16) & 32767)
            .collect(),
        panicked: false,
    };
    let status = unsafe {
        stream(
            c,
            Some(group_read),
            (&mut source as *mut GroupSource).cast(),
            write,
            output,
            r,
            Some(profile),
        )
    };
    if source.panicked {
        fail(r, -5, "Rust group source panic")
    } else {
        status
    }
}
