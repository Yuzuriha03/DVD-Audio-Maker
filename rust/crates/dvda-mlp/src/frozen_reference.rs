use crate::ffi::{EncoderConfig, EncoderResult, StampRecord};
use sha2::{Digest, Sha256};
use std::ffi::{CString, c_void};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryA(name: *const u8) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}
struct Input {
    pcm: Vec<i32>,
    channels: usize,
    position: usize,
}
unsafe extern "C" fn read(
    opaque: *mut c_void,
    pcm: *mut i32,
    capacity: usize,
    frames: *mut usize,
) -> i32 {
    let input = unsafe { &mut *opaque.cast::<Input>() };
    let count = capacity
        .min(input.pcm.len() / input.channels - input.position)
        .min(17);
    unsafe {
        std::ptr::copy_nonoverlapping(
            input.pcm.as_ptr().add(input.position * input.channels),
            pcm,
            count * input.channels,
        );
        frames.write(count);
    }
    input.position += count;
    0
}
unsafe extern "C" fn write(opaque: *mut c_void, bytes: *const u8, count: usize) -> i32 {
    let output = unsafe { &mut *opaque.cast::<Vec<u8>>() };
    output.extend_from_slice(unsafe { std::slice::from_raw_parts(bytes, count) });
    0
}

#[test]
fn frozen_stream_bytes() {
    frozen_bytes(false);
}

#[test]
fn frozen_layout_depth_bytes() {
    frozen_bytes(true);
}

#[test]
fn frozen_group_bytes() {
    frozen_groups(false);
}

#[test]
fn frozen_group_signal_boundaries() {
    frozen_groups(true);
}

fn frozen_groups(signal_boundaries: bool) {
    let path = CString::new(std::env::var("MLP_FROZEN_REFERENCE").unwrap()).unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let address = GetProcAddress(module, c"mlp_encode_stream_groups".as_ptr().cast());
        assert!(!address.is_null());
        let reference: unsafe extern "C" fn(
            *const crate::ffi::GroupsConfig,
            Option<crate::ffi::Read>,
            *mut c_void,
            Option<crate::ffi::Read>,
            *mut c_void,
            Option<crate::ffi::Write>,
            *mut c_void,
            *mut EncoderResult,
        ) -> i32 = std::mem::transmute(address);
        let mut verified = 0usize;
        for assignment in 0..21 {
            let channels = crate::format::SPEAKERS[assignment].len();
            let first = crate::format::FIRST[assignment];
            if first == channels {
                continue;
            }
            for rate in [44100, 48000, 88200, 96000] {
                for primary_bits in [16, 20, 24] {
                    for ratio in [1, 2] {
                        if ratio == 2 && rate < 88200 {
                            continue;
                        }
                        for bits in [16, 20, 24] {
                            if bits > primary_bits {
                                continue;
                            }
                            let au = if rate > 48000 { 80 } else { 40 };
                            let lengths = if signal_boundaries {
                                [au - 2, au, au + 2, au * 8 - 2, au * 8, au * 8 + 2]
                            } else {
                                [2usize, 82, 642, 4098, 0, 0]
                            };
                            for signal in 0..if signal_boundaries { 6 } else { 1 } {
                                for frames in lengths {
                                    if frames == 0 {
                                        continue;
                                    }
                                    let packet = [0, 1, 0x40, 0, 0xff];
                                    let record = StampRecord {
                                        start: 0,
                                        size: 5,
                                        packet: packet.as_ptr(),
                                        valid_bits: 0,
                                    };
                                    let config = crate::ffi::GroupsConfig {
                                        struct_size: std::mem::size_of::<crate::ffi::GroupsConfig>()
                                            as u32,
                                        primary: EncoderConfig {
                                            struct_size: std::mem::size_of::<EncoderConfig>()
                                                as u32,
                                            abi_version: 1,
                                            sample_rate: rate,
                                            bits: primary_bits,
                                            channels: channels as u32,
                                            restart_interval: 8,
                                            frames: frames as u64,
                                            metadata: &record,
                                            metadata_count: 1,
                                        },
                                        assignment: assignment as u32,
                                        group2_sample_rate: rate / ratio,
                                        group2_bits: bits,
                                        group2_frames: frames as u64 / ratio as u64,
                                    };
                                    let make = |channels, frames: usize, depth: u32| Input {
                                        pcm: (0..frames * channels)
                                            .map(|n| {
                                                if signal_boundaries {
                                                    let frame = n / channels;
                                                    let channel = n % channels;
                                                    let max = (1i32 << (depth - 1)) - 1;
                                                    let min = -(1i32 << (depth - 1));
                                                    let value = match signal {
                                                        0 => 0,
                                                        1 => {
                                                            if frame % 2 == 0 {
                                                                min
                                                            } else {
                                                                max
                                                            }
                                                        }
                                                        2 => {
                                                            if frame == 0 || frame + 1 == frames {
                                                                max
                                                            } else {
                                                                0
                                                            }
                                                        }
                                                        3 => (frame as i32 % 31 - 15) * (max / 16),
                                                        4 => {
                                                            (frame as i32 % 31 - 15)
                                                                * (max / 16)
                                                                * if channel % 2 == 0 {
                                                                    1
                                                                } else {
                                                                    -1
                                                                }
                                                        }
                                                        _ => {
                                                            let mut x =
                                                                (n as u32).wrapping_add(0x9e3779b9);
                                                            x ^= x >> 16;
                                                            x = x.wrapping_mul(0x85ebca6b);
                                                            x ^= x >> 13;
                                                            x = x.wrapping_mul(0xc2b2ae35);
                                                            x ^= x >> 16;
                                                            (x & ((1u32 << depth) - 1)) as i32 + min
                                                        }
                                                    };
                                                    value * (1 << (24 - depth))
                                                } else {
                                                    (((n * 7919 + assignment * 131) % 65536) as i32
                                                        - 32768)
                                                        * (1 << (24 - depth))
                                                }
                                            })
                                            .collect(),
                                        channels,
                                        position: 0,
                                    };
                                    let mut outputs = [Vec::<u8>::new(), Vec::new()];
                                    let mut results =
                                        [EncoderResult::default(), EncoderResult::default()];
                                    for backend in 0..2 {
                                        let mut primary = make(first, frames, primary_bits);
                                        let mut secondary =
                                            make(channels - first, frames / ratio as usize, bits);
                                        let encode = if backend == 0 {
                                            reference
                                        } else {
                                            crate::ffi::mlp_encode_stream_groups
                                        };
                                        assert_eq!(
                                            encode(
                                                &config,
                                                Some(read),
                                                (&mut primary as *mut Input).cast(),
                                                Some(read),
                                                (&mut secondary as *mut Input).cast(),
                                                Some(write),
                                                (&mut outputs[backend] as *mut Vec<u8>).cast(),
                                                &mut results[backend]
                                            ),
                                            0,
                                            "group status assignment {assignment} rate {rate} ratio {ratio} depths {primary_bits}/{bits} frames {frames}"
                                        );
                                    }
                                    assert_eq!(
                                        outputs[0], outputs[1],
                                        "group bytes assignment {assignment} rate {rate} ratio {ratio} depths {primary_bits}/{bits} frames {frames}"
                                    );
                                    assert_eq!(
                                        Sha256::digest(&outputs[0]),
                                        Sha256::digest(&outputs[1])
                                    );
                                    assert_eq!(results[0].input_frames, results[1].input_frames);
                                    assert_eq!(
                                        results[0].encoded_frames,
                                        results[1].encoded_frames
                                    );
                                    assert_eq!(results[0].access_units, results[1].access_units);
                                    assert_eq!(results[0].output_bytes, results[1].output_bytes);
                                    verified += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(verified, if signal_boundaries { 24624 } else { 2736 });
        println!("{verified} frozen grouped cases passed (signal boundaries: {signal_boundaries})");
        FreeLibrary(module);
    }
}

fn frozen_bytes(layouts: bool) {
    let path = CString::new(
        std::env::var("MLP_FROZEN_REFERENCE").expect("frozen ABI parity requires original C DLL"),
    )
    .unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let address = GetProcAddress(module, c"mlp_encode_stream".as_ptr().cast());
        assert!(!address.is_null());
        let reference: unsafe extern "C" fn(
            *const EncoderConfig,
            Option<crate::ffi::Read>,
            *mut c_void,
            Option<crate::ffi::Write>,
            *mut c_void,
            *mut EncoderResult,
        ) -> i32 = std::mem::transmute(address);
        let depths: unsafe extern "C" fn(
            *const EncoderConfig,
            u32,
            u32,
            Option<crate::ffi::Read>,
            *mut c_void,
            Option<crate::ffi::Write>,
            *mut c_void,
            *mut EncoderResult,
        ) -> i32 = std::mem::transmute(GetProcAddress(
            module,
            c"mlp_encode_stream_depths".as_ptr().cast(),
        ));
        let layout: unsafe extern "C" fn(
            *const EncoderConfig,
            u32,
            Option<crate::ffi::Read>,
            *mut c_void,
            Option<crate::ffi::Write>,
            *mut c_void,
            *mut EncoderResult,
        ) -> i32 = std::mem::transmute(GetProcAddress(
            module,
            c"mlp_encode_stream_layout".as_ptr().cast(),
        ));
        let mut c_time = std::time::Duration::ZERO;
        let mut rust_time = std::time::Duration::ZERO;
        let mut verified = 0usize;
        for case in 0..if layouts { 756 } else { 648 } {
            let channels = if layouts {
                crate::format::SPEAKERS[case % 21].len()
            } else {
                1 + case % 6
            };
            let bits = if layouts {
                24
            } else {
                [16, 20, 24][case / 6 % 3]
            };
            let rate = [44100, 48000, 88200, 96000, 176400, 192000][if layouts {
                case / 21 % 6
            } else {
                case / 18 % 6
            }];
            if rate > 96000 && channels > 2 {
                continue;
            }
            let frames = [1, 41, 641, 1282, 2560, 4097][case / 108 % 6];
            let config = crate::Config {
                sample_rate: rate,
                bits,
                channels: channels as u8,
                restart_interval: 8,
                metadata: vec![0, 1, 0x40, 0, 0xff],
            };
            let profile = if layouts {
                crate::format::Profile {
                    assignment: case % 21,
                    group2_bits: [16, 20, 24][case / 126 % 3],
                    group2_rate: rate,
                }
            } else {
                crate::format::Profile::standard(&config)
            };
            let source: Vec<i32> = (0..frames * channels)
                .map(|n| {
                    (((n * 173 + case) % 32768) as i32 - 16384)
                        * (1 << (24 - if layouts { profile.group2_bits } else { bits }))
                })
                .collect();
            let mut input = Input {
                pcm: source.clone(),
                channels,
                position: 0,
            };
            let packet = [0, 1, 0x40, 0, 0xff];
            let record = StampRecord {
                start: 0,
                size: packet.len() as u32,
                packet: packet.as_ptr(),
                valid_bits: 0,
            };
            let c = EncoderConfig {
                struct_size: std::mem::size_of::<EncoderConfig>() as u32,
                abi_version: 1,
                sample_rate: rate,
                bits: bits as u32,
                channels: channels as u32,
                restart_interval: 8,
                frames: frames as u64,
                metadata: &record,
                metadata_count: 1,
            };
            let mut expected = Vec::new();
            let mut result = EncoderResult::default();
            let started = std::time::Instant::now();
            assert_eq!(
                if layouts {
                    if crate::format::FIRST[profile.assignment] < channels {
                        depths(
                            &c,
                            profile.assignment as u32,
                            profile.group2_bits as u32,
                            Some(read),
                            (&mut input as *mut Input).cast(),
                            Some(write),
                            (&mut expected as *mut Vec<u8>).cast(),
                            &mut result,
                        )
                    } else {
                        layout(
                            &c,
                            profile.assignment as u32,
                            Some(read),
                            (&mut input as *mut Input).cast(),
                            Some(write),
                            (&mut expected as *mut Vec<u8>).cast(),
                            &mut result,
                        )
                    }
                } else {
                    reference(
                        &c,
                        Some(read),
                        (&mut input as *mut Input).cast(),
                        Some(write),
                        (&mut expected as *mut Vec<u8>).cast(),
                        &mut result,
                    )
                },
                0,
                "C status case {case}"
            );
            c_time += started.elapsed();
            let samples = 40usize << (u32::from(rate > 48000) + u32::from(rate > 96000));
            let padded = frames.div_ceil(samples) * samples;
            let mut rust_input = Input {
                pcm: source,
                channels,
                position: 0,
            };
            let mut actual = Vec::new();
            let mut rust_result = EncoderResult::default();
            let started = std::time::Instant::now();
            assert_eq!(
                if layouts {
                    if crate::format::FIRST[profile.assignment] < channels {
                        crate::ffi::mlp_encode_stream_depths(
                            &c,
                            profile.assignment as u32,
                            profile.group2_bits as u32,
                            Some(read),
                            (&mut rust_input as *mut Input).cast(),
                            Some(write),
                            (&mut actual as *mut Vec<u8>).cast(),
                            &mut rust_result,
                        )
                    } else {
                        crate::ffi::mlp_encode_stream_layout(
                            &c,
                            profile.assignment as u32,
                            Some(read),
                            (&mut rust_input as *mut Input).cast(),
                            Some(write),
                            (&mut actual as *mut Vec<u8>).cast(),
                            &mut rust_result,
                        )
                    }
                } else {
                    crate::ffi::mlp_encode_stream(
                        &c,
                        Some(read),
                        (&mut rust_input as *mut Input).cast(),
                        Some(write),
                        (&mut actual as *mut Vec<u8>).cast(),
                        &mut rust_result,
                    )
                },
                0,
                "Rust status case {case}"
            );
            rust_time += started.elapsed();
            verified += 1;
            assert_eq!(rust_result.input_frames, result.input_frames);
            assert_eq!(rust_result.encoded_frames, result.encoded_frames);
            assert_eq!(rust_result.access_units, result.access_units);
            assert_eq!(rust_result.output_bytes, result.output_bytes);
            assert_eq!(
                actual, expected,
                "raw MLP case {case}: {rate}/{bits}/{channels}, frames {frames}"
            );
            assert_eq!(Sha256::digest(&actual), Sha256::digest(&expected));
            assert_eq!(result.input_frames, frames as u64);
            assert_eq!(result.encoded_frames, padded as u64);
        }
        eprintln!(
            "Frozen parity layouts={layouts}: {verified} cases, C={c_time:?}, Rust={rust_time:?}, Rust/C={:.3}",
            rust_time.as_secs_f64() / c_time.as_secs_f64()
        );
        FreeLibrary(module);
    }
}
