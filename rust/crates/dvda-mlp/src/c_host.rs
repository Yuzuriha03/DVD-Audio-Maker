use crate::ffi::{EncoderConfig, EncoderResult, Read, Write};
use crate::{Config, format::Profile, metadata::Metadata};
use std::cell::Cell;
use std::ffi::c_void;

pub(crate) struct HostIo {
    pub read: Read,
    pub input: *mut c_void,
    pub write: Write,
    pub output: *mut c_void,
}

pub(crate) fn encode_stream(
    config: &EncoderConfig,
    c: &Config,
    profile: &Profile,
    records: &[Metadata],
    io: HostIo,
    result: &mut EncoderResult,
) -> i32 {
    use crate::c_encode::{IntervalOutput, OutputFormat, encode};
    use crate::c_prepare::{CallbackInput, MatrixInterval};
    let channels = c.channels as usize;
    let shift = u32::from(c.sample_rate > 48000) + u32::from(c.sample_rate > 96000);
    let samples = 40usize << shift;
    let Ok(source_frames) = usize::try_from(config.frames) else {
        return -1;
    };
    let Some(frames) = source_frames
        .checked_add(samples - 1)
        .map(|n| n / samples * samples)
    else {
        return -1;
    };
    if frames / samples > u32::MAX as usize {
        return -1;
    }
    let rate_code = (u32::from([44100, 88200, 176400].contains(&c.sample_rate)) * 8) | shift;
    let rate_field = 0x8000 | ((if rate_code & 8 != 0 { 3482 } else { 3200 }) >> shift);
    let format = OutputFormat {
        channels,
        sample_rate: c.sample_rate,
        rate_field,
        major: crate::major(c, profile, rate_code, rate_field, samples as u32),
    };
    let mut order = [0; 6];
    order[..channels].copy_from_slice(&profile.order());
    let mut depths = [0; 6];
    for (ch, depth) in depths.iter_mut().enumerate().take(channels) {
        *depth = profile.depth(c, ch) as u32;
    }
    let fp = crate::extended::HostFpScope::codec();
    let status = Cell::new(0);
    let input_frames = Cell::new(0u64);
    let mut pcm = CallbackInput {
        channels,
        bits: c.bits as u32,
        depths,
        order,
        au_samples: samples,
        frames,
        source_frames,
        position: 0,
        failed: false,
        callback: |buffer: &mut [i32], capacity: usize| {
            let mut received = 0;
            let rc = fp.with_caller(|| unsafe {
                (io.read)(io.input, buffer.as_mut_ptr(), capacity, &mut received)
            });
            if rc != 0 || received == 0 || received > capacity {
                status.set(-3);
                return Err(());
            }
            input_frames.set(input_frames.get() + received as u64);
            Ok(received)
        },
    };
    let mut matrix = MatrixInterval {
        original_scale: true,
        enable_matrix: true,
        joint_search: true,
        ..Default::default()
    };
    let mut pending = IntervalOutput {
        au: Vec::new(),
        maximum_lsbs: 0,
        restart: Default::default(),
        failure: None,
    };
    let mut queue = crate::c_output_queue::MlpOutputQueue::mlp_output_queue_init();
    let Some(mut stamp) = crate::c_stamp::mlp_stamp_init(records, (frames / samples) as u64) else {
        return -1;
    };
    let ok = encode(
        &mut pcm,
        &format,
        c.restart_interval as u32,
        c.sample_rate / samples as u32,
        true,
        Some(&mut matrix),
        false,
        &mut pending,
        &mut queue,
        Some(&mut stamp),
        &mut |words| {
            if words.len() > 4095 {
                return false;
            }
            let bytes: Vec<u8> = words
                .iter()
                .flat_map(|w| (*w as u16).to_be_bytes())
                .collect();
            let rc =
                fp.with_caller(|| unsafe { (io.write)(io.output, bytes.as_ptr(), bytes.len()) });
            if rc != 0 {
                status.set(-4);
                return false;
            }
            result.access_units += 1;
            result.output_bytes += bytes.len() as u64;
            true
        },
    );
    queue.mlp_output_queue_dispose();
    result.input_frames = input_frames.get();
    if ok.is_ok() {
        result.encoded_frames = frames as u64;
        0
    } else if status.get() != 0 {
        status.get()
    } else if pcm.failed {
        -3
    } else {
        -5
    }
}
