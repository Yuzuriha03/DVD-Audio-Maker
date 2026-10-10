#[cfg(windows)]
mod windows {
    use dvda_mlp::ffi::*;
    use std::{
        ffi::{CString, c_void},
        path::PathBuf,
        process::Command,
    };
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryA(name: *const u8) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
        fn FreeLibrary(module: *mut c_void) -> i32;
    }
    type Encode = unsafe extern "C" fn(
        *const EncoderConfig,
        Option<Read>,
        *mut c_void,
        Option<Write>,
        *mut c_void,
        *mut EncoderResult,
    ) -> i32;
    type Layout = unsafe extern "C" fn(
        *const EncoderConfig,
        u32,
        Option<Read>,
        *mut c_void,
        Option<Write>,
        *mut c_void,
        *mut EncoderResult,
    ) -> i32;
    type Depths = unsafe extern "C" fn(
        *const EncoderConfig,
        u32,
        u32,
        Option<Read>,
        *mut c_void,
        Option<Write>,
        *mut c_void,
        *mut EncoderResult,
    ) -> i32;
    type Groups = unsafe extern "C" fn(
        *const GroupsConfig,
        Option<Read>,
        *mut c_void,
        Option<Read>,
        *mut c_void,
        Option<Write>,
        *mut c_void,
        *mut EncoderResult,
    ) -> i32;
    struct State {
        pcm: Vec<i32>,
        channels: usize,
        at: usize,
        bytes: Vec<u8>,
        read_error: bool,
        write_error: bool,
        read_limit: usize,
        write_limit: usize,
        writes: usize,
        first_write_at: usize,
        nested: Option<Encode>,
        bad_read: bool,
    }
    unsafe extern "C" fn read(
        p: *mut c_void,
        pcm: *mut i32,
        capacity: usize,
        frames: *mut usize,
    ) -> i32 {
        let s = unsafe { &mut *p.cast::<State>() };
        if s.bad_read {
            unsafe {
                *frames = capacity + 1;
            }
            return 0;
        }
        if s.read_error || s.at >= s.read_limit {
            return 1;
        }
        let n = capacity.min(13).min(s.pcm.len() / s.channels - s.at);
        unsafe {
            std::ptr::copy_nonoverlapping(
                s.pcm.as_ptr().add(s.at * s.channels),
                pcm,
                n * s.channels,
            );
            *frames = n;
        }
        s.at += n;
        0
    }
    unsafe extern "C" fn write(p: *mut c_void, bytes: *const u8, count: usize) -> i32 {
        let s = unsafe { &mut *p.cast::<State>() };
        if s.write_error || s.writes >= s.write_limit {
            return 1;
        }
        if s.writes == 0 {
            s.first_write_at = s.at;
        }
        if let Some(encode) = s.nested.take() {
            nested_encode(encode);
        }
        s.writes += 1;
        s.bytes
            .extend_from_slice(unsafe { std::slice::from_raw_parts(bytes, count) });
        0
    }
    fn state(pcm: Vec<i32>, channels: usize) -> State {
        State {
            pcm,
            channels,
            at: 0,
            bytes: Vec::new(),
            read_error: false,
            write_error: false,
            read_limit: usize::MAX,
            write_limit: usize::MAX,
            writes: 0,
            first_write_at: 0,
            nested: None,
            bad_read: false,
        }
    }
    fn nested_encode(encode: Encode) {
        let packet = [0, 0, 64, 0];
        let record = StampRecord {
            start: 0,
            size: 4,
            packet: packet.as_ptr(),
            valid_bits: 0,
        };
        let c = EncoderConfig {
            struct_size: size_of::<EncoderConfig>() as u32,
            abi_version: 1,
            sample_rate: 48000,
            bits: 16,
            channels: 2,
            restart_interval: 8,
            frames: 401,
            metadata: &record,
            metadata_count: 1,
        };
        let mut s = state(vec![0; 802], 2);
        let p = (&mut s as *mut State).cast();
        let mut r = EncoderResult::default();
        assert_eq!(
            unsafe { encode(&c, Some(read), p, Some(write), p, &mut r) },
            0
        );
        assert_eq!(r.output_bytes, s.bytes.len() as u64);
    }
    fn major_crc(bytes: &[u8]) -> u16 {
        let mut r = u32::from(u16::from_be_bytes([bytes[0], bytes[1]]));
        for pair in bytes[2..26].as_chunks::<2>().0 {
            let word = u32::from(u16::from_be_bytes([pair[0], pair[1]]));
            for last in [0, word] {
                let t = r >> 8;
                let v = (((r & 255) << 3) ^ (t & 0x7ff)) << 2;
                let v = (v ^ (t & 0x1fff)) << 1;
                r = ((v ^ (t & 0x3fff)) << 2) ^ ((t ^ last) & 65535);
            }
        }
        r as u16
    }
    fn decode(bytes: &[u8], tag: &str) -> Vec<u8> {
        let base = PathBuf::from("build").join(format!("mlp-differential-{tag}"));
        let mlp = base.with_extension("mlp");
        let raw = base.with_extension("pcm");
        let mut normalized = bytes.to_vec();
        let mut at = 0;
        while at < normalized.len() {
            let count =
                ((usize::from(normalized[at] & 15) << 8) | usize::from(normalized[at + 1])) * 2;
            if normalized.get(at + 4..at + 8) == Some(&[0xf8, 0x72, 0x6f, 0xbb]) {
                assert_eq!(
                    major_crc(&normalized[at + 4..at + 32]),
                    u16::from_be_bytes([normalized[at + 30], normalized[at + 31]]),
                    "original major CRC {tag}"
                );
            }
            if normalized.get(at + 4..at + 8) == Some(&[0xf8, 0x72, 0x6f, 0xbb])
                && normalized[at + 9] & 15 != normalized[at + 9] >> 4
                && normalized[at + 9] & 15 != 15
            {
                assert_eq!(tag, "groups", "unexpected differing group rates");
                // Diagnostic decoding only: FFmpeg rejects the native-rate declaration,
                // although the payload contains the reconstructed full-rate samples.
                normalized[at + 9] = (normalized[at + 9] & 240) | (normalized[at + 9] >> 4);
                let mut r = u32::from(u16::from_be_bytes([normalized[at + 4], normalized[at + 5]]));
                for i in 1..13 {
                    let word = u32::from(u16::from_be_bytes([
                        normalized[at + 4 + i * 2],
                        normalized[at + 5 + i * 2],
                    ]));
                    for last in [0, word] {
                        let t = r >> 8;
                        let v = (((r & 255) << 3) ^ (t & 0x7ff)) << 2;
                        let v = (v ^ (t & 0x1fff)) << 1;
                        r = ((v ^ (t & 0x3fff)) << 2) ^ ((t ^ last) & 65535);
                    }
                }
                normalized[at + 30..at + 32].copy_from_slice(&(r as u16).to_be_bytes());
            }
            at += count;
        }
        std::fs::write(&mlp, &normalized).unwrap();
        let status = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "mlp", "-i"])
            .arg(&mlp)
            .args(["-f", "s32le", "-acodec", "pcm_s32le"])
            .arg(&raw)
            .status()
            .unwrap();
        assert!(status.success(), "decode {tag}");
        let data = std::fs::read(&raw).unwrap();
        std::fs::remove_file(mlp).unwrap();
        std::fs::remove_file(raw).unwrap();
        data
    }
    pub fn run() {
        let path = std::env::args().nth(1).expect("frozen C DLL path required");
        let path = CString::new(path).unwrap();
        let dll = unsafe { LoadLibraryA(path.as_ptr().cast()) };
        assert!(!dll.is_null());
        let symbol = |name: &str| {
            let n = CString::new(name).unwrap();
            let p = unsafe { GetProcAddress(dll, n.as_ptr().cast()) };
            assert!(!p.is_null(), "{name}");
            p
        };
        let old: Encode = unsafe { std::mem::transmute(symbol("mlp_encode_stream")) };
        let layout: Layout = unsafe { std::mem::transmute(symbol("mlp_encode_stream_layout")) };
        let depths: Depths = unsafe { std::mem::transmute(symbol("mlp_encode_stream_depths")) };
        let groups: Groups = unsafe { std::mem::transmute(symbol("mlp_encode_stream_groups")) };
        let rust_encode: Encode = mlp_encode_stream;
        let rust_layout: Layout = mlp_encode_stream_layout;
        let rust_depths: Depths = mlp_encode_stream_depths;
        let rust_groups: Groups = mlp_encode_stream_groups;
        if std::env::args().any(|arg| arg == "--benchmark") {
            let pcm: Vec<i32> = (0..48000 * 2)
                .map(|n| ((n * 7919i64 % (1 << 24)) - (1 << 23)) as i32)
                .collect();
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
                bits: 24,
                channels: 2,
                restart_interval: 8,
                frames: 48000,
                metadata: &record,
                metadata_count: 1,
            };
            let mut outputs = Vec::new();
            let mut timings = [Vec::new(), Vec::new()];
            for round in 0..6 {
                for index in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
                    let (name, encode) = [("C", old), ("Rust", rust_encode)][index];
                    let mut s = state(pcm.clone(), 2);
                    let p = (&mut s as *mut State).cast();
                    let mut result = EncoderResult::default();
                    let start = std::time::Instant::now();
                    assert_eq!(
                        unsafe { encode(&config, Some(read), p, Some(write), p, &mut result) },
                        0
                    );
                    let elapsed = start.elapsed().as_secs_f64();
                    if outputs.is_empty() {
                        outputs.push(s.bytes.clone());
                    }
                    assert_eq!(
                        s.bytes, outputs[0],
                        "benchmark raw bytes {name} round {round}"
                    );
                    use sha2::{Digest, Sha256};
                    assert_eq!(Sha256::digest(&s.bytes), Sha256::digest(&outputs[0]));
                    if round != 0 {
                        timings[index].push(elapsed);
                    }
                }
            }
            for (index, name) in ["C", "Rust"].iter().enumerate() {
                timings[index].sort_by(f64::total_cmp);
                let median = timings[index][2];
                println!(
                    "{name}: median {median:.6}s, {:.0} frames/s, {:.2}x realtime; five measured rounds after warmup",
                    48000.0 / median,
                    1.0 / median
                );
            }
            println!(
                "Rust/C median elapsed ratio: {:.3}; every round raw bytes and SHA256 identical",
                timings[1][2] / timings[0][2]
            );
            let _decoded = decode(&outputs[0], "benchmark");
            unsafe {
                FreeLibrary(dll);
            }
            return;
        }
        assert_eq!(mlp_encoder_abi_version(), 1);
        let packet = [0, 0, 0x40, 0];
        let records = [
            StampRecord {
                start: 0,
                size: 4,
                packet: packet.as_ptr(),
                valid_bits: 0,
            },
            StampRecord {
                start: 42,
                size: 4,
                packet: packet.as_ptr(),
                valid_bits: 0,
            },
        ];
        let standard_only = std::env::args().any(|arg| arg == "--standard-only");
        let groups_only = std::env::args().any(|arg| arg == "--groups-only");
        let mut tested = 0;
        if !groups_only {
            for rate in [44100, 48000, 88200, 96000, 176400, 192000] {
                for bits in [16, 20, 24] {
                    for channels in 1..=if rate > 96000 { 2 } else { 6 } {
                        let frames = 401;
                        let pcm: Vec<i32> = (0..frames * channels)
                            .map(|n| (((n * 73 % 1001) as i32) - 500) << (24 - bits))
                            .collect();
                        let c = EncoderConfig {
                            struct_size: size_of::<EncoderConfig>() as u32,
                            abi_version: 1,
                            sample_rate: rate,
                            bits,
                            channels,
                            restart_interval: 8,
                            frames: u64::from(frames),
                            metadata: records.as_ptr(),
                            metadata_count: 1,
                        };
                        let mut a = state(pcm.clone(), channels as usize);
                        let mut b = state(pcm.clone(), channels as usize);
                        let pa = (&mut a as *mut State).cast();
                        let pb = (&mut b as *mut State).cast();
                        let mut ra = EncoderResult::default();
                        let mut rb = EncoderResult::default();
                        assert_eq!(
                            unsafe { old(&c, Some(read), pa, Some(write), pa, &mut ra) },
                            0,
                            "C profile {rate}/{bits}/{channels}"
                        );
                        assert_eq!(
                            unsafe { rust_encode(&c, Some(read), pb, Some(write), pb, &mut rb) },
                            0,
                            "Rust profile {rate}/{bits}/{channels}"
                        );
                        let first_difference =
                            a.bytes.iter().zip(&b.bytes).position(|(a, b)| a != b);
                        assert!(
                            a.bytes == b.bytes,
                            "raw MLP mismatch for {rate}/{bits}/{channels}: first differing byte {first_difference:?}, C length {}, Rust length {}",
                            a.bytes.len(),
                            b.bytes.len()
                        );
                        assert_eq!(
                            decode(&a.bytes, "c"),
                            decode(&b.bytes, "rust"),
                            "profile {rate}/{bits}/{channels}"
                        );
                        assert_eq!(ra.encoded_frames, rb.encoded_frames);
                        tested += 1;
                    }
                }
            }
            for assignment in 0..21 {
                let channels = dvda_mlp::format::SPEAKERS[assignment].len();
                let frames = 401;
                let c = EncoderConfig {
                    struct_size: size_of::<EncoderConfig>() as u32,
                    abi_version: 1,
                    sample_rate: 48000,
                    bits: 24,
                    channels: channels as u32,
                    restart_interval: 8,
                    frames,
                    metadata: records.as_ptr(),
                    metadata_count: 1,
                };
                let pcm: Vec<i32> = (0..frames as usize * channels)
                    .map(|n| (n % 257) as i32 * 16)
                    .collect();
                for distinct in [false, true] {
                    if distinct && dvda_mlp::format::FIRST[assignment] == channels {
                        continue;
                    }
                    let mut a = state(pcm.clone(), channels);
                    let mut b = state(pcm.clone(), channels);
                    let pa = (&mut a as *mut State).cast();
                    let pb = (&mut b as *mut State).cast();
                    let mut ra = EncoderResult::default();
                    let mut rb = EncoderResult::default();
                    let (sa, sb) = unsafe {
                        if distinct {
                            (
                                depths(
                                    &c,
                                    assignment as u32,
                                    20,
                                    Some(read),
                                    pa,
                                    Some(write),
                                    pa,
                                    &mut ra,
                                ),
                                rust_depths(
                                    &c,
                                    assignment as u32,
                                    20,
                                    Some(read),
                                    pb,
                                    Some(write),
                                    pb,
                                    &mut rb,
                                ),
                            )
                        } else {
                            (
                                layout(
                                    &c,
                                    assignment as u32,
                                    Some(read),
                                    pa,
                                    Some(write),
                                    pa,
                                    &mut ra,
                                ),
                                rust_layout(
                                    &c,
                                    assignment as u32,
                                    Some(read),
                                    pb,
                                    Some(write),
                                    pb,
                                    &mut rb,
                                ),
                            )
                        }
                    };
                    assert_eq!(sa, 0);
                    assert_eq!(sb, 0);
                    assert_eq!(
                        decode(&a.bytes, "c"),
                        decode(&b.bytes, "rust"),
                        "layout {assignment} distinct {distinct}"
                    );
                    tested += 1;
                }
            }
            if std::env::args().any(|arg| arg == "--stress") {
                for restart in [1, 8, 32] {
                    for pattern in [0, 1, 2] {
                        let frames = 70001usize;
                        let mut seed = 0x12345678u32;
                        let pcm: Vec<i32> = (0..frames * 2)
                            .map(|n| {
                                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                                match pattern {
                                    0 => (n as i32 / 2 % 10000 - 5000) * 256,
                                    1 => ((seed >> 16) as i32 - 32768) * 256,
                                    _ => {
                                        if n % 2 == 0 {
                                            -8388608
                                        } else {
                                            8388352
                                        }
                                    }
                                }
                            })
                            .collect();
                        let c = EncoderConfig {
                            struct_size: size_of::<EncoderConfig>() as u32,
                            abi_version: 1,
                            sample_rate: 48000,
                            bits: 16,
                            channels: 2,
                            restart_interval: restart,
                            frames: frames as u64,
                            metadata: records.as_ptr(),
                            metadata_count: 1,
                        };
                        let mut decoded = Vec::new();
                        for f in [old, rust_encode] {
                            let mut s = state(pcm.clone(), 2);
                            let p = (&mut s as *mut State).cast();
                            let mut result = EncoderResult::default();
                            assert_eq!(
                                unsafe { f(&c, Some(read), p, Some(write), p, &mut result) },
                                0,
                                "stress pattern {pattern} restart {restart}"
                            );
                            let output = decode(&s.bytes, "stress");
                            let expected: Vec<u8> =
                                pcm.iter().flat_map(|&x| (x << 8).to_le_bytes()).collect();
                            assert_eq!(&output[..expected.len()], expected);
                            assert!(output[expected.len()..].iter().all(|&x| x == 0));
                            decoded.push(output);
                        }
                        assert_eq!(decoded[0], decoded[1]);
                        tested += 1;
                    }
                }
            }
            for assignment in [12usize, 18, 20] {
                let channels = dvda_mlp::format::SPEAKERS[assignment].len();
                for pattern in 0..3 {
                    let frames = 9602;
                    let c = EncoderConfig {
                        struct_size: size_of::<EncoderConfig>() as u32,
                        abi_version: 1,
                        sample_rate: 96000,
                        bits: 24,
                        channels: channels as u32,
                        restart_interval: 8,
                        frames: frames as u64,
                        metadata: records.as_ptr(),
                        metadata_count: 1,
                    };
                    let pcm: Vec<_> = (0..frames)
                        .flat_map(|n| {
                            (0..channels).map(move |ch| {
                                let x = ((n * 971) % 16001) as i32 - 8000;
                                match pattern {
                                    0 => (x + ch as i32) * 256,
                                    1 => {
                                        if n % 101 == 0 {
                                            8388607 - ch as i32
                                        } else {
                                            0
                                        }
                                    }
                                    _ => {
                                        if (n + ch) % 2 == 0 {
                                            8388607
                                        } else {
                                            -8388608
                                        }
                                    }
                                }
                            })
                        })
                        .collect();
                    for f in [layout, rust_layout] {
                        let mut s = state(pcm.clone(), channels);
                        let p = (&mut s as *mut State).cast();
                        let mut r = EncoderResult::default();
                        assert_eq!(
                            unsafe {
                                f(&c, assignment as u32, Some(read), p, Some(write), p, &mut r)
                            },
                            0
                        );
                        let decoded = decode(&s.bytes, "multichannel");
                        let expected: Vec<_> =
                            pcm.iter().flat_map(|&x| (x << 8).to_le_bytes()).collect();
                        assert_eq!(
                            &decoded[..expected.len()],
                            expected,
                            "multichannel matrix/impulse/extreme assignment{assignment}"
                        );
                        assert!(decoded[expected.len()..].iter().all(|&x| x == 0));
                    }
                    tested += 1;
                }
            }
            println!("{tested} standard/layout/depth differential cases passed");
        }
        if !standard_only {
            let path = std::env::var("MLP_GROUP_REFERENCE")
                .expect("MLP_GROUP_REFERENCE must name the independent native adapter oracle");
            let path = CString::new(path).unwrap();
            let reference_dll = unsafe { LoadLibraryA(path.as_ptr().cast()) };
            assert!(!reference_dll.is_null());
            type Reference = unsafe extern "C" fn(
                u32,
                u32,
                u32,
                u32,
                u32,
                usize,
                *const i32,
                *const i32,
                *mut i32,
            ) -> i32;
            let reference: Reference = unsafe {
                std::mem::transmute(GetProcAddress(
                    reference_dll,
                    c"group_reference".as_ptr().cast(),
                ))
            };
            let mut grouped = 0;
            for rate in [88200, 96000] {
                for assignment in 2..21 {
                    let first = dvda_mlp::format::FIRST[assignment];
                    let channels = dvda_mlp::format::SPEAKERS[assignment].len();
                    let second = channels - first;
                    for bits in [16, 20, 24] {
                        for ratio in [1, 2] {
                            let frames = if std::env::args().any(|arg| arg == "--stress") {
                                9602
                            } else {
                                802
                            };
                            let c = EncoderConfig {
                                struct_size: size_of::<EncoderConfig>() as u32,
                                abi_version: 1,
                                sample_rate: rate,
                                bits: 24,
                                channels: channels as u32,
                                restart_interval: 8,
                                frames: frames as u64,
                                metadata: records.as_ptr(),
                                metadata_count: 1,
                            };
                            let g = GroupsConfig {
                                struct_size: size_of::<GroupsConfig>() as u32,
                                primary: c,
                                assignment: assignment as u32,
                                group2_sample_rate: rate / ratio,
                                group2_bits: bits,
                                group2_frames: frames as u64 / u64::from(ratio),
                            };
                            let a: Vec<i32> = (0..frames * first)
                                .map(|n| ((n % 311) as i32 - 155) * 4096)
                                .collect();
                            let b: Vec<i32> = (0..frames / ratio as usize * second)
                                .map(|n| ((n % 199) as i32 - 99) << (24 - bits))
                                .collect();
                            let mut expected = vec![0i32; frames * channels];
                            assert_eq!(
                                unsafe {
                                    reference(
                                        rate,
                                        24,
                                        assignment as u32,
                                        rate / ratio,
                                        bits,
                                        frames,
                                        a.as_ptr(),
                                        b.as_ptr(),
                                        expected.as_mut_ptr(),
                                    )
                                },
                                0
                            );
                            let expected_bytes: Vec<u8> = expected
                                .iter()
                                .flat_map(|&x| (x << 8).to_le_bytes())
                                .collect();
                            let mut outputs = Vec::new();
                            let mut raw_outputs = Vec::new();
                            for f in [groups, rust_groups] {
                                let mut x = state(a.clone(), first);
                                let mut y = state(b.clone(), second);
                                let mut out = state(Vec::new(), channels);
                                let mut r = EncoderResult::default();
                                assert_eq!(
                                    unsafe {
                                        f(
                                            &g,
                                            Some(read),
                                            (&mut x as *mut State).cast(),
                                            Some(read),
                                            (&mut y as *mut State).cast(),
                                            Some(write),
                                            (&mut out as *mut State).cast(),
                                            &mut r,
                                        )
                                    },
                                    0,
                                    "group {rate}/{assignment}/{bits}/{ratio}"
                                );
                                // Validate the original (unmodified) native-rate major header,
                                // AU parity, lengths and metadata independently of diagnostic decode.
                                let mut at = 0;
                                while at < out.bytes.len() {
                                    let words = ((usize::from(out.bytes[at] & 15) << 8)
                                        | usize::from(out.bytes[at + 1]))
                                        * 2;
                                    assert!(words >= 8 && at + words <= out.bytes.len());
                                    let major = out.bytes.get(at + 4..at + 8)
                                        == Some(&[0xf8, 0x72, 0x6f, 0xbb]);
                                    let directory = if major {
                                        let rate_code = if rate == 88200 { 9 } else { 1 };
                                        assert_eq!(
                                            out.bytes[at + 9],
                                            (rate_code << 4) | (rate_code - u8::from(ratio == 2))
                                        );
                                        assert_eq!(
                                            out.bytes[at + 8],
                                            0x20 | ((bits - 16) / 4) as u8
                                        );
                                        assert_eq!(out.bytes[at + 11], assignment as u8);
                                        32
                                    } else {
                                        4
                                    };
                                    let mut parity = 0u8;
                                    for &byte in &out.bytes[at..at + 4] {
                                        parity ^= byte >> 4;
                                        parity ^= byte & 15;
                                    }
                                    for &byte in &out.bytes[at + directory..at + directory + 2] {
                                        parity ^= byte >> 4;
                                        parity ^= byte & 15;
                                    }
                                    assert_eq!(parity, 15);
                                    at += words;
                                }
                                let decoded = decode(&out.bytes, "groups");
                                assert_eq!(
                                    &decoded[..expected_bytes.len()],
                                    expected_bytes,
                                    "independent interpolation oracle {rate}/{assignment}/{bits}/{ratio}"
                                );
                                assert!(decoded[expected_bytes.len()..].iter().all(|&x| x == 0));
                                raw_outputs.push(out.bytes);
                                outputs.push(decoded);
                            }
                            assert!(
                                raw_outputs[0] == raw_outputs[1],
                                "raw groups {rate}/{assignment}/{bits}/{ratio}; first differing byte {:?}",
                                raw_outputs[0]
                                    .iter()
                                    .zip(&raw_outputs[1])
                                    .position(|(a, b)| a != b)
                            );
                            use sha2::{Digest, Sha256};
                            assert_eq!(
                                Sha256::digest(&raw_outputs[0]),
                                Sha256::digest(&raw_outputs[1])
                            );
                            assert_eq!(outputs[0], outputs[1]);
                            for failure in 0..3 {
                                for f in [groups, rust_groups] {
                                    let mut x = state(a.clone(), first);
                                    let mut y = state(b.clone(), second);
                                    let mut out = state(Vec::new(), channels);
                                    match failure {
                                        0 => x.read_error = true,
                                        1 => y.read_error = true,
                                        _ => out.write_error = true,
                                    }
                                    let mut r = EncoderResult::default();
                                    let expected = if failure == 2 { -4 } else { -3 };
                                    assert_eq!(
                                        unsafe {
                                            f(
                                                &g,
                                                Some(read),
                                                (&mut x as *mut State).cast(),
                                                Some(read),
                                                (&mut y as *mut State).cast(),
                                                Some(write),
                                                (&mut out as *mut State).cast(),
                                                &mut r,
                                            )
                                        },
                                        expected
                                    );
                                }
                            }
                            grouped += 1;
                            tested += 1;
                        }
                    }
                }
            }
            for ratio in [1, 2] {
                std::thread::scope(|scope| {
                    let mut handles = Vec::new();
                    for _ in 0..8 {
                        handles.push(scope.spawn(move || {
                            let packet = [0, 0, 64, 0];
                            let record = StampRecord {
                                start: 0,
                                size: 4,
                                packet: packet.as_ptr(),
                                valid_bits: 0,
                            };
                            let g = GroupsConfig {
                                struct_size: size_of::<GroupsConfig>() as u32,
                                primary: EncoderConfig {
                                    struct_size: size_of::<EncoderConfig>() as u32,
                                    abi_version: 1,
                                    sample_rate: 96000,
                                    bits: 24,
                                    channels: 6,
                                    restart_interval: 8,
                                    frames: 802,
                                    metadata: &record,
                                    metadata_count: 1,
                                },
                                assignment: 20,
                                group2_sample_rate: 96000 / ratio,
                                group2_bits: 20,
                                group2_frames: 802 / u64::from(ratio),
                            };
                            let first = dvda_mlp::format::FIRST[20];
                            let second = 6 - first;
                            let mut x = state(
                                (0..802 * first).map(|n| (n as i32 - 401) * 257).collect(),
                                first,
                            );
                            let mut y = state(
                                (0..802 / ratio as usize * second)
                                    .map(|n| (n as i32 - 201) * 16)
                                    .collect(),
                                second,
                            );
                            let mut out = state(Vec::new(), 6);
                            let mut r = EncoderResult::default();
                            assert_eq!(
                                unsafe {
                                    rust_groups(
                                        &g,
                                        Some(read),
                                        (&mut x as *mut State).cast(),
                                        Some(read),
                                        (&mut y as *mut State).cast(),
                                        Some(write),
                                        (&mut out as *mut State).cast(),
                                        &mut r,
                                    )
                                },
                                0
                            );
                            assert_eq!(r.input_frames, 802);
                            assert_eq!(r.output_bytes, out.bytes.len() as u64);
                            out.bytes
                        }));
                    }
                    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
                    assert!(results.iter().all(|bytes| bytes == &results[0]));
                });
            }
            println!(
                "eight concurrent equal-rate and half-rate grouped Rust DLL encodes deterministic"
            );
            unsafe {
                FreeLibrary(reference_dll);
            }
            println!(
                "{grouped} group cases: complete raw MLP bytes/SHA256 plus original header/parity and exact PCM against unchanged native interpolation oracle"
            );
        }
        if !groups_only {
            let packet = [0, 2, 0x40, 0, 255, 255];
            for malformed in [false, true] {
                let records = [
                    StampRecord {
                        start: 0,
                        size: 6,
                        packet: packet.as_ptr(),
                        valid_bits: 0,
                    },
                    StampRecord {
                        start: if malformed { 59 } else { 60 },
                        size: 6,
                        packet: packet.as_ptr(),
                        valid_bits: 45,
                    },
                ];
                let c = EncoderConfig {
                    struct_size: size_of::<EncoderConfig>() as u32,
                    abi_version: 1,
                    sample_rate: 48000,
                    bits: 16,
                    channels: 2,
                    restart_interval: 8,
                    frames: 105 * 40,
                    metadata: records.as_ptr(),
                    metadata_count: 2,
                };
                let mut stamp_streams = Vec::new();
                for f in [old, rust_encode] {
                    let mut s = state(vec![0; c.frames as usize * 2], 2);
                    let p = (&mut s as *mut State).cast();
                    let mut r = EncoderResult::default();
                    assert_eq!(
                        unsafe { f(&c, Some(read), p, Some(write), p, &mut r) },
                        if malformed { -1 } else { 0 }
                    );
                    if malformed {
                        assert_eq!(s.at, 0);
                        assert!(s.bytes.is_empty());
                        continue;
                    }
                    assert_eq!(r.input_frames, c.frames);
                    assert_eq!(r.encoded_frames, c.frames);
                    assert_eq!(r.access_units, 105);
                    assert_eq!(r.output_bytes, s.bytes.len() as u64);
                    let mut at = 0;
                    let mut bits = Vec::new();
                    while at < s.bytes.len() {
                        let words = ((s.bytes[at] & 15) as usize * 256) + s.bytes[at + 1] as usize;
                        let directory = if s.bytes[at + 4..at + 8] == [248, 114, 111, 187] {
                            32
                        } else {
                            4
                        };
                        bits.push((s.bytes[at + directory] >> 4) & 1);
                        at += words * 2;
                    }
                    stamp_streams.push(bits);
                }
                if !malformed {
                    assert_eq!(stamp_streams[0], stamp_streams[1]);
                }
            }
            println!(
                "stuffed multi-record/partial metadata bits and malformed early admission passed"
            );
            std::thread::scope(|scope| {
                let mut handles = Vec::new();
                for _ in 0..8 {
                    handles.push(scope.spawn(move || {
                        let packet = [0, 0, 64, 0];
                        let record = StampRecord {
                            start: 0,
                            size: 4,
                            packet: packet.as_ptr(),
                            valid_bits: 0,
                        };
                        let c = EncoderConfig {
                            struct_size: size_of::<EncoderConfig>() as u32,
                            abi_version: 1,
                            sample_rate: 48000,
                            bits: 16,
                            channels: 2,
                            restart_interval: 8,
                            frames: 401,
                            metadata: &record,
                            metadata_count: 1,
                        };
                        let mut s = state(vec![0; 802], 2);
                        let p = (&mut s as *mut State).cast();
                        let mut r = EncoderResult::default();
                        assert_eq!(
                            unsafe { rust_encode(&c, Some(read), p, Some(write), p, &mut r) },
                            0
                        );
                        s.bytes
                    }));
                }
                let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
                assert!(results.iter().all(|b| b == &results[0]));
            });
            println!("eight concurrent Rust DLL encodes deterministic");
            for restart_interval in [1, 8, 32] {
                for mode in 0..4 {
                    let c = EncoderConfig {
                        struct_size: size_of::<EncoderConfig>() as u32,
                        abi_version: 1,
                        sample_rate: 48000,
                        bits: 16,
                        channels: 2,
                        restart_interval,
                        frames: 70001,
                        metadata: records.as_ptr(),
                        metadata_count: 1,
                    };
                    for f in [old, rust_encode] {
                        let mut s = state(vec![0; c.frames as usize * 2], 2);
                        match mode {
                            1 => s.read_limit = 52000,
                            2 => s.write_limit = 17,
                            3 => s.nested = Some(f),
                            _ => {}
                        }
                        let p = (&mut s as *mut State).cast();
                        let mut r = EncoderResult::default();
                        let expected = match mode {
                            1 => -3,
                            2 => -4,
                            _ => 0,
                        };
                        assert_eq!(
                            unsafe { f(&c, Some(read), p, Some(write), p, &mut r) },
                            expected
                        );
                        assert_eq!(r.status, expected);
                        if expected == 0 || f as usize == rust_encode as usize {
                            assert_eq!(r.output_bytes, s.bytes.len() as u64);
                            assert_eq!(r.access_units as usize, s.writes);
                            assert_eq!(
                                r.encoded_frames,
                                if expected == 0 {
                                    u64::from(r.access_units) * 40
                                } else {
                                    0
                                }
                            );
                            assert_eq!(r.input_frames, s.at as u64);
                        }
                        assert!(s.first_write_at >= 1200 * 40);
                        assert!(s.first_write_at <= (1201 + restart_interval as usize) * 40);
                        if mode == 2 {
                            assert!(s.at < c.frames as usize);
                        }
                    }
                }
            }
            println!(
                "bounded first-output pacing, late read/write cancellation, counters and nested reentrancy passed against C"
            );
            for test in 0..13 {
                for f in [old, rust_encode] {
                    let mut c = EncoderConfig {
                        struct_size: size_of::<EncoderConfig>() as u32,
                        abi_version: 1,
                        sample_rate: 48000,
                        bits: 16,
                        channels: 2,
                        restart_interval: 8,
                        frames: 401,
                        metadata: records.as_ptr(),
                        metadata_count: 1,
                    };
                    let mut s = state(vec![0; 802], 2);
                    let mut expected = -1;
                    match test {
                        0 => c.struct_size = 0,
                        1 => c.abi_version = 2,
                        2 => c.sample_rate = 32000,
                        3 => c.bits = 18,
                        4 => c.channels = 7,
                        5 => c.restart_interval = 33,
                        6 => c.frames = 0,
                        7 => c.metadata_count = 0,
                        8 => c.metadata = std::ptr::null(),
                        9 => {
                            s.pcm[0] = 1;
                            expected = -3
                        }
                        10 => {
                            s.bad_read = true;
                            expected = -3
                        }
                        _ => {}
                    }
                    let p = (&mut s as *mut State).cast();
                    let mut r = EncoderResult::default();
                    let config = if test == 11 { std::ptr::null() } else { &c };
                    let reader = if test == 12 { None } else { Some(read as Read) };
                    assert_eq!(
                        unsafe { f(config, reader, p, Some(write), p, &mut r) },
                        expected,
                        "admission {test}"
                    );
                    assert_eq!(r.status, expected);
                    assert!(s.bytes.is_empty());
                }
            }
            println!("13 malformed configuration/callback/precision cases match frozen C");
            for (read_error, write_error, expected) in [(true, false, -3), (false, true, -4)] {
                for f in [old, rust_encode] {
                    let c = EncoderConfig {
                        struct_size: size_of::<EncoderConfig>() as u32,
                        abi_version: 1,
                        sample_rate: 48000,
                        bits: 16,
                        channels: 2,
                        restart_interval: 8,
                        frames: 401,
                        metadata: records.as_ptr(),
                        metadata_count: 1,
                    };
                    let mut s = state(vec![0; 802], 2);
                    s.read_error = read_error;
                    s.write_error = write_error;
                    let p = (&mut s as *mut State).cast();
                    let mut r = EncoderResult::default();
                    assert_eq!(
                        unsafe { f(&c, Some(read), p, Some(write), p, &mut r) },
                        expected
                    );
                    assert_eq!(r.status, expected);
                }
            }
        }
        unsafe {
            FreeLibrary(dll);
        }
        println!(
            "{tested} frozen-C PCM differential cases passed; input/output cancellation statuses matched"
        );
    }
}
fn main() {
    #[cfg(windows)]
    windows::run();
}
