use crate::c_predict::{MlpPredictFilter, MlpPredictState, mlp_predict_block};
use crate::interval::{
    MLP_INTERVAL_SLOTS, MlpBoundaryState, MlpIntervalSlot, mlp_interval_boundary, mlp_interval_find,
};
use crate::scale::{MlpScalePlan, mlp_scale_analyze};
use crate::search::{
    mlp_search_correlation, mlp_search_correlation_extended, mlp_search_iir_stable,
    mlp_search_rand, mlp_search_reflection,
};
use std::ffi::c_void;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryA(name: *const u8) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

#[test]
fn c_encode_parity() {
    use crate::c_encode::*;
    use crate::c_prepare::*;
    let path = std::ffi::CString::new(
        std::env::var("MLP_PREPARE_REFERENCE").expect("encode parity requires C reference"),
    )
    .unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let reference: unsafe extern "C" fn(
            *const i32,
            usize,
            u32,
            u32,
            u32,
            u32,
            u32,
            *mut u32,
            usize,
            *mut usize,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"encode_reference".as_ptr().cast()));
        let major_reference: unsafe extern "C" fn(u32, u32, *mut u32) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"encode_major".as_ptr().cast()));
        for case in 0..1536usize {
            let channels = 1 + case % 6;
            let bits = [16, 20, 24][(case / 6) % 3];
            let frames = [8, 16, 41, 85, 161, 641, 1282, 2560][(case / 18) % 8];
            let options = (case % 64) as u32;
            let interval = [1, 4, 8, 16][case % 4];
            let cycle = if case % 5 == 0 { 960 } else { 0 };
            let pcm: Vec<i32> = (0..frames * channels)
                .map(|n| (((n * 173 + case) % 32768) as i32 - 16384) * (1 << (24 - bits)))
                .collect();
            let mut expected = vec![0; frames * channels * 4 + 8192];
            let mut written = 0;
            let result = reference(
                pcm.as_ptr(),
                frames,
                channels as u32,
                bits,
                interval,
                cycle,
                options,
                expected.as_mut_ptr(),
                expected.len(),
                &mut written,
            );
            let mut major = [0; 14];
            assert_eq!(
                major_reference(bits, channels as u32, major.as_mut_ptr()),
                0
            );
            let mut input = MatrixInput {
                pcm: &pcm,
                channels,
                bits,
                au_samples: 40,
                position: 0,
            };
            let mut m = MatrixInterval {
                original_scale: options & 2 != 0,
                enable_matrix: options & 4 != 0,
                joint_search: options & 8 != 0,
                ..Default::default()
            };
            let mut pending = IntervalOutput {
                au: Vec::new(),
                maximum_lsbs: 0,
                restart: Default::default(),
                failure: None,
            };
            let mut queue = crate::c_output_queue::MlpOutputQueue::mlp_output_queue_init();
            let mut emitted = Vec::new();
            let records = [crate::metadata::Metadata {
                start: 0,
                packet: vec![0, 1, 0x40, 0, 0xff],
                valid_bits: 0,
            }];
            let mut stamp =
                crate::c_stamp::mlp_stamp_init(&records, frames.div_ceil(40) as u64).unwrap();
            let actual = encode(
                &mut input,
                &OutputFormat {
                    channels,
                    sample_rate: 48000,
                    rate_field: 0x8c80,
                    major,
                },
                interval,
                cycle,
                options & 1 != 0,
                if options & 16 != 0 {
                    Some(&mut m)
                } else {
                    None
                },
                options & 32 != 0,
                &mut pending,
                &mut queue,
                Some(&mut stamp),
                &mut |words| {
                    emitted.extend_from_slice(words);
                    true
                },
            );
            assert_eq!(actual.is_ok(), result != 0, "encode result {case}");
            assert_eq!(emitted, expected[..written], "encode bytes {case}");
        }
        FreeLibrary(module);
    }
}

#[test]
fn c_flush_parity() {
    use crate::c_encode::*;
    use crate::c_restart::MlpRestart;
    use crate::parameters::{MlpCodingParams, MlpParameters};
    let path = std::ffi::CString::new(
        std::env::var("MLP_PREPARE_REFERENCE").expect("flush parity requires C reference"),
    )
    .unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let reference: unsafe extern "C" fn(
            *const MlpParameters,
            *const MlpRestart,
            *const i32,
            u32,
            u32,
            u32,
            u32,
            u32,
            u32,
            *const u32,
            u32,
            u32,
            *mut u32,
            usize,
            *mut u64,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"flush_reference".as_ptr().cast()));
        let major_reference: unsafe extern "C" fn(u32, u32, u32, *mut u32) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"flush_major".as_ptr().cast()));
        for case in 0..256usize {
            let channels = 1 + case % 6;
            let count = [8, 16, 40, 80, 160][case % 5];
            let aus = 1 + case % 4;
            let width = [8, 16, 24][case % 3];
            let rate_field = if case % 7 == 0 { 1 } else { 0x8040 };
            let stamp = (case % 2) as u32;
            let fail = case % 11 == 0;
            let parameters = MlpParameters {
                maximum_channel: channels as u32 - 1,
                blocksize: count as u32,
                flags: 2,
                coding: [MlpCodingParams {
                    mode: 0,
                    total_width: width,
                    offset: 0,
                }; 16],
                ..Default::default()
            };
            let restart = MlpRestart {
                maximum_channel: channels as u32 - 1,
                maximum_bits: 25,
                seed: 2,
                ..Default::default()
            };
            let samples: Vec<i32> = (0..aus * count * channels)
                .map(|n| ((n * 17 + case) % 127) as i32 - 63)
                .collect();
            let mut pending = IntervalOutput {
                au: (0..aus)
                    .map(|au| PlannedAu {
                        initial: parameters.clone(),
                        main: parameters.clone(),
                        previous: parameters.clone(),
                        residual: samples[au * count * channels..(au + 1) * count * channels]
                            .to_vec(),
                        unpredicted: samples[au * count * channels..(au + 1) * count * channels]
                            .to_vec(),
                        bypass: vec![],
                        count,
                        bypass_bits: 0,
                        single_restart: count < 16,
                        end_markers: au + 1 == aus,
                        stamp,
                    })
                    .collect(),
                maximum_lsbs: width as u32,
                restart: restart.clone(),
                failure: None,
            };
            let mut major = [0; 14];
            assert_eq!(
                major_reference(48000, channels as u32, rate_field, major.as_mut_ptr()),
                0
            );
            let mut expected = vec![0u32; 32768];
            let mut state = [0u64; 9];
            let result = reference(
                &parameters,
                &restart,
                samples.as_ptr(),
                channels as u32,
                count as u32,
                aus as u32,
                width as u32,
                48000,
                rate_field,
                major.as_ptr(),
                stamp,
                u32::from(fail),
                expected.as_mut_ptr(),
                expected.len(),
                state.as_mut_ptr(),
            );
            let mut queue = crate::c_output_queue::MlpOutputQueue::mlp_output_queue_init();
            let mut rate = crate::c_rate::mlp_rate_init();
            let mut total = 0;
            let mut emitted = Vec::new();
            let mut emit = |words: &[u32]| {
                if fail {
                    return false;
                }
                emitted.extend_from_slice(words);
                true
            };
            let actual = flush_interval(
                &mut queue,
                &mut pending,
                &OutputFormat {
                    channels,
                    sample_rate: 48000,
                    rate_field,
                    major,
                },
                &mut rate,
                &mut total,
                &mut emit,
            );
            let finished = queue.mlp_output_queue_finish(&mut emit);
            assert_eq!(actual, result != 0, "flush result {case}");
            assert_eq!(
                [
                    total,
                    u64::from(rate.arrival),
                    u64::from(rate.decode),
                    pending.au.len() as u64,
                    u64::from(pending.maximum_lsbs),
                    u64::from(pending.restart.maximum_lsbs),
                    emitted.len() as u64,
                    u64::from(finished),
                    u64::from(pending.failure.is_some())
                ],
                state,
                "flush state {case}"
            );
            assert_eq!(emitted, expected[..state[6] as usize], "flush words {case}");
        }
        FreeLibrary(module);
    }
}

#[test]
fn c_prepare_parity() {
    use crate::c_analysis::MlpMatrixAnalysis;
    use crate::c_prepare::*;
    use crate::matrix::MlpMatrixPrimitive;
    use crate::search::{MlpSearchPlan, MlpSearchPool};
    let path = std::ffi::CString::new(
        std::env::var("MLP_PREPARE_REFERENCE").expect("prepare parity requires C reference"),
    )
    .unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let reference: unsafe extern "C" fn(
            *const i32,
            usize,
            u32,
            u32,
            u32,
            u32,
            u32,
            *mut i32,
            *mut u8,
            *mut u32,
            *mut u32,
            *mut u32,
            *mut MlpMatrixPrimitive,
            *mut MlpMatrixAnalysis,
            *mut MlpScalePlan,
            *mut MlpSearchPlan,
            *mut MlpSearchPool,
        ) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"prepare_reference".as_ptr().cast()));
        for case in 0..272usize {
            let diagnostic = case >= 256;
            let _fp = crate::extended::HostFpScope::codec();
            let channels = if diagnostic { 5 } else { 1 + case % 6 };
            let au_samples = if diagnostic {
                80
            } else {
                [40, 80, 160][case % 3]
            };
            let frames = if diagnostic {
                9680
            } else {
                au_samples * (1 + case % 3) + [0, 2, 8, 16][case % 4]
            };
            let bits = if diagnostic {
                24
            } else {
                [16, 20, 24][case % 3]
            };
            let options = if diagnostic {
                7 | 16 | (((case - 256) as u32) << 8)
            } else {
                (case % 16) as u32
            };
            let interval = if diagnostic { 8 } else { 16 };
            let pcm: Vec<i32> = if diagnostic {
                let group_path = std::ffi::CString::new(
                    std::env::var("MLP_GROUP_REFERENCE")
                        .expect("long prepare parity requires group reference"),
                )
                .unwrap();
                let group_module = LoadLibraryA(group_path.as_ptr().cast());
                assert!(!group_module.is_null());
                let group: unsafe extern "C" fn(
                    u32,
                    u32,
                    u32,
                    u32,
                    u32,
                    usize,
                    *const i32,
                    *const i32,
                    *mut i32,
                ) -> i32 = std::mem::transmute(GetProcAddress(
                    group_module,
                    c"group_reference".as_ptr().cast(),
                ));
                let primary: Vec<i32> = (0..9602 * 4).map(|n| ((n % 311) - 155) * 4096).collect();
                let secondary: Vec<i32> = (0..4801).map(|n| ((n % 199) - 99) << 8).collect();
                let mut external = vec![0; 9602 * channels];
                assert_eq!(
                    group(
                        96000,
                        24,
                        18,
                        48000,
                        16,
                        9602,
                        primary.as_ptr(),
                        secondary.as_ptr(),
                        external.as_mut_ptr()
                    ),
                    0
                );
                FreeLibrary(group_module);
                let order = crate::format::Profile {
                    assignment: 18,
                    group2_bits: 16,
                    group2_rate: 48000,
                }
                .order();
                let mut pcm: Vec<i32> = external
                    .chunks_exact(channels)
                    .flat_map(|frame| order.iter().map(move |&ch| frame[ch]))
                    .collect();
                pcm.resize(frames * channels, 0);
                pcm
            } else {
                (0..frames)
                    .flat_map(|n| {
                        (0..channels).map(move |ch| {
                            let v =
                                ((n as i32 * 173 + ch as i32 * 31 + case as i32) & 32767) - 16384;
                            v * (1 << (24 - bits))
                        })
                    })
                    .collect()
            };
            let mut m = MatrixInterval {
                pcm: Vec::new(),
                bypass: Vec::new(),
                lengths: Vec::new(),
                checks: Vec::new(),
                slot: 0,
                offset: 0,
                bits: 0,
                count: 0,
                selected_matrix: false,
                matrix: Default::default(),
                analysis: Default::default(),
                scale: Default::default(),
                original_scale: options & 1 != 0,
                enable_matrix: options & 2 != 0,
                joint_search: options & 4 != 0,
                search_ready: [false; 6],
                prediction: Default::default(),
                pool: Default::default(),
                search_rng: 1,
            };
            crate::c_analysis::mlp_matrix_analysis_init(&mut m.analysis, channels as u32).unwrap();
            for p in &mut m.pool {
                crate::search::mlp_search_pool_init(p, if diagnostic { m.search_rng } else { 1 });
                if diagnostic {
                    m.search_rng = p.rng;
                }
            }
            let mut input = MatrixInput {
                pcm: &pcm,
                channels,
                bits,
                au_samples,
                position: 0,
            };
            let mut truncated = 0;
            let mut out = vec![0; pcm.len()];
            let mut bypass = vec![0; frames];
            let mut lengths = [0u32; 128];
            let mut checks = [0u32; 128];
            let mut state = [0u32; 15];
            let mut matrix = Default::default();
            let mut analysis = Default::default();
            let mut scale = Default::default();
            let mut prediction: [MlpSearchPlan; 6] = Default::default();
            let mut pools: [MlpSearchPool; 6] = Default::default();
            let rc = reference(
                pcm.as_ptr(),
                frames,
                channels as u32,
                bits,
                au_samples as u32,
                interval,
                options,
                out.as_mut_ptr(),
                bypass.as_mut_ptr(),
                lengths.as_mut_ptr(),
                checks.as_mut_ptr(),
                state.as_mut_ptr(),
                (&mut matrix as *mut [MlpMatrixPrimitive; 6]).cast(),
                &mut analysis,
                &mut scale,
                prediction.as_mut_ptr(),
                pools.as_mut_ptr(),
            );
            let _fp = crate::extended::HostFpScope::codec();
            let mut prepared = false;
            for _ in 0..=options >> 8 {
                prepared = prepare_matrix(
                    &mut input,
                    &mut m,
                    interval as usize,
                    options & 8 != 0,
                    &mut truncated,
                )
                .is_ok();
                if !prepared {
                    break;
                }
            }
            assert_eq!(rc != 0, prepared, "prepare {case}");
            assert_eq!(m.analysis, analysis, "analysis {case}");
            assert_eq!(m.scale, scale, "scale {case}");
            if diagnostic {
                let f: unsafe extern "C" fn(
                    *const f64,
                    *const f64,
                    u32,
                    *mut u32,
                    *mut f64,
                    *mut f64,
                ) -> i32 = std::mem::transmute(GetProcAddress(
                    module,
                    c"prepare_decorrelate".as_ptr().cast(),
                ));
                let ratio =
                    std::array::from_fn(|ch| m.analysis.covariance[ch * 7] / m.analysis.energy[ch]);
                let mut order = [0; 6];
                let mut transform = [0.; 36];
                let mut reduced = [0.; 36];
                assert_eq!(
                    f(
                        m.analysis.covariance.as_ptr(),
                        ratio.as_ptr(),
                        channels as u32,
                        order.as_mut_ptr(),
                        transform.as_mut_ptr(),
                        reduced.as_mut_ptr()
                    ),
                    0
                );
                let (ro, rt, rr) = crate::c_analysis::mlp_matrix_decorrelate(
                    &m.analysis.covariance,
                    &ratio,
                    channels,
                    0,
                    10.,
                )
                .unwrap();
                assert_eq!(ro, order, "order {case}");
                assert_eq!(rt, transform, "transform {case}");
                assert_eq!(rr, reduced, "reduced {case}");
            }
            assert_eq!(m.matrix, matrix, "matrix {case}");
            assert_eq!(m.pcm, out[..m.pcm.len()], "PCM {case}");
            assert_eq!(m.bypass, bypass[..m.bypass.len()], "bypass {case}");
            assert_eq!(
                m.lengths,
                lengths[..m.lengths.len()]
                    .iter()
                    .map(|&n| n as usize)
                    .collect::<Vec<_>>()
            );
            assert_eq!(m.checks, checks[..m.checks.len()]);
            assert_eq!(
                [
                    m.lengths.len() as u32,
                    m.slot as u32,
                    m.offset as u32,
                    m.bits,
                    m.count as u32,
                    u32::from(m.selected_matrix),
                    m.search_rng,
                    truncated,
                    input.position as u32
                ],
                state[..9],
                "state {case}"
            );
            assert_eq!(m.matrix, matrix, "matrix {case}");
            assert_eq!(m.analysis, analysis, "analysis {case}");
            assert_eq!(m.scale, scale, "scale {case}");
            assert_eq!(m.prediction, prediction, "prediction {case}");
            assert_eq!(m.pool, pools, "pools {case}");
            assert_eq!(
                m.search_ready.map(u32::from),
                state[9..],
                "search ready {case}"
            );
        }
        FreeLibrary(module);
    }
}

#[test]
fn c_stamp_parity() {
    use crate::c_stamp::*;
    use crate::metadata::Metadata;
    #[repr(C)]
    struct Record {
        start: u64,
        size: u32,
        packet: *const u8,
        valid_bits: u32,
    }
    let path = std::ffi::CString::new(
        std::env::var("MLP_QUEUE_REFERENCE").expect("stamp parity requires C reference"),
    )
    .unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let trace: unsafe extern "C" fn(*const Record, usize, u64, *mut u32, *mut u32) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"stamp_trace".as_ptr().cast()));
        for case in 0..12000usize {
            let payload = case % 32;
            let mut packet = vec![0, payload as u8, 0x40, 0];
            packet.extend((0..payload).map(|i| ((case * 31 + i * 127) & 255) as u8));
            let period = crate::stamp(&packet).len() as u64;
            let aus = 1 + (case as u64 * 17) % 1000;
            let mut records = vec![Metadata {
                start: 0,
                packet: packet.clone(),
                valid_bits: 0,
            }];
            if case % 3 == 0 && period < aus {
                records.push(Metadata {
                    start: period,
                    packet,
                    valid_bits: 0,
                });
            }
            if case % 4 == 0 {
                let last = records.last_mut().unwrap();
                last.valid_bits = (last.packet.len() * 8 - case % 8) as u32;
            }
            if case % 7 == 0 {
                records[0].packet[2] = 0;
            }
            if case % 11 == 0 && records.len() > 1 {
                records[1].start += 1;
            }
            let cr: Vec<_> = records
                .iter()
                .map(|r| Record {
                    start: r.start,
                    size: r.packet.len() as u32,
                    packet: r.packet.as_ptr(),
                    valid_bits: r.valid_bits,
                })
                .collect();
            let mut bits = vec![99; aus as usize + 1];
            let mut complete = vec![0; bits.len()];
            let result = trace(
                cr.as_ptr(),
                cr.len(),
                aus,
                bits.as_mut_ptr(),
                complete.as_mut_ptr(),
            );
            let mut rust = mlp_stamp_init(&records, aus);
            assert_eq!(result == 0, rust.is_some(), "stamp init {case}");
            if let Some(s) = rust.as_mut() {
                for i in 0..=aus as usize {
                    let mut bit = 99;
                    let ok = mlp_stamp_next(s, &mut bit);
                    assert_eq!(if ok { bit } else { 99 }, bits[i], "stamp {case} bit {i}");
                    assert_eq!(u32::from(mlp_stamp_complete(s)), complete[i]);
                }
            }
        }
        FreeLibrary(module);
    }
}

#[test]
fn c_rate_parity() {
    use crate::c_rate::*;
    let path = std::ffi::CString::new(
        std::env::var("MLP_QUEUE_REFERENCE").expect("rate parity requires C reference"),
    )
    .unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let init: unsafe extern "C" fn(*mut MlpRateState) =
            std::mem::transmute(GetProcAddress(module, c"mlp_rate_init".as_ptr().cast()));
        let update: unsafe extern "C" fn(
            *mut MlpRateState,
            u32,
            u32,
            u32,
            *mut u32,
            *mut u16,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"mlp_rate_update".as_ptr().cast()));
        let mut seed = 1u32;
        let mut random = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            seed
        };
        let mut c = mlp_rate_init();
        init(&mut c);
        let mut r = mlp_rate_init();
        for i in 0..24000 {
            if i % 17 == 0 {
                c.arrival = random() as u16;
                c.decode = random() as u16;
                r = c;
            }
            let samples = random() % 170;
            let words = random() % 4200;
            let rate = random() & 65535;
            let mut cf = random();
            let mut rf = cf;
            let mut ca = random() as u16;
            let mut ra = ca;
            assert_eq!(
                update(&mut c, samples, words, rate, &mut cf, &mut ca),
                mlp_rate_update(&mut r, samples, words, rate, &mut rf, &mut ra)
            );
            assert_eq!((c, cf, ca), (r, rf, ra), "rate case {i}");
        }
        FreeLibrary(module);
    }
}

#[test]
fn c_output_queue_parity() {
    use crate::c_output_queue::MlpOutputQueue;
    struct Sink {
        words: Vec<Vec<u32>>,
        fail_at: usize,
    }
    unsafe extern "C" fn emit(opaque: *mut c_void, words: *const u32, count: usize) -> i32 {
        let sink = unsafe { &mut *opaque.cast::<Sink>() };
        sink.words
            .push(unsafe { std::slice::from_raw_parts(words, count) }.to_vec());
        i32::from(sink.words.len() != sink.fail_at)
    }
    let path = std::ffi::CString::new(
        std::env::var("MLP_QUEUE_REFERENCE").expect("queue parity requires C reference"),
    )
    .unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let create: unsafe extern "C" fn(
            unsafe extern "C" fn(*mut c_void, *const u32, usize) -> i32,
            *mut c_void,
        ) -> *mut c_void =
            std::mem::transmute(GetProcAddress(module, c"queue_create".as_ptr().cast()));
        let destroy: unsafe extern "C" fn(*mut c_void) =
            std::mem::transmute(GetProcAddress(module, c"queue_destroy".as_ptr().cast()));
        let state: unsafe extern "C" fn(*const c_void, *mut u32) =
            std::mem::transmute(GetProcAddress(module, c"queue_state".as_ptr().cast()));
        let push: unsafe extern "C" fn(*mut c_void, *const u32, usize, u32) -> i32 =
            std::mem::transmute(GetProcAddress(
                module,
                c"mlp_output_queue_push".as_ptr().cast(),
            ));
        let finish: unsafe extern "C" fn(*mut c_void) -> i32 = std::mem::transmute(GetProcAddress(
            module,
            c"mlp_output_queue_finish".as_ptr().cast(),
        ));
        let dispose: unsafe extern "C" fn(*mut c_void) = std::mem::transmute(GetProcAddress(
            module,
            c"mlp_output_queue_dispose".as_ptr().cast(),
        ));
        for case in 0..64usize {
            let mut c_sink = Sink {
                words: Vec::new(),
                fail_at: if case % 4 == 0 { 3 } else { usize::MAX },
            };
            let mut r_sink = Sink {
                words: Vec::new(),
                fail_at: c_sink.fail_at,
            };
            let q = create(emit, (&mut c_sink as *mut Sink).cast());
            assert!(!q.is_null());
            let mut rust = MlpOutputQueue::mlp_output_queue_init();
            let mut callback = |words: &[u32]| {
                r_sink.words.push(words.to_vec());
                r_sink.words.len() != r_sink.fail_at
            };
            let streams = 1 + (case % 2);
            let length = [0, 1, 7, 1199, 1200, 1201, 2403, 3610][case % 8];
            for i in 0..length {
                let major = i % 37 == 0;
                let directory = if major { 16 } else { 2 };
                let payload = 2 + (i * 17 + case) % 120;
                let mut words = vec![0u32; directory + streams + payload];
                words[0] = 0xa000 | words.len() as u32;
                words[1] = (i as u32 * 80) & 65535;
                if major {
                    words[2] = 0xf872;
                    words[3] = 0x6fbb;
                    words[9] = 0x8000 | (64 + case as u32);
                    words[10] = (streams as u32) << 12;
                }
                words[directory] = if streams == 1 { payload } else { payload / 2 } as u32;
                words[directory + streams - 1] = payload as u32;
                let samples = [8, 40, 80, 160][(i + case) % 4];
                if case % 5 == 0 && i % 41 == 0 {
                    let mut invalid = words.clone();
                    invalid[0] ^= 1;
                    assert_eq!(
                        push(q, invalid.as_ptr(), invalid.len(), samples) != 0,
                        rust.mlp_output_queue_push(&invalid, samples, &mut callback)
                    );
                }
                assert_eq!(
                    push(q, words.as_ptr(), words.len(), samples) != 0,
                    rust.mlp_output_queue_push(&words, samples, &mut callback),
                    "case {case} AU {i}"
                );
                let mut cs = [0; 9];
                state(q, cs.as_mut_ptr());
                assert_eq!(cs, rust.state(), "case {case} AU {i}");
            }
            assert_eq!(
                finish(q) != 0,
                rust.mlp_output_queue_finish(&mut callback),
                "finish {case}"
            );
            assert_eq!(finish(q) != 0, rust.mlp_output_queue_finish(&mut callback));
            let mut cs = [0; 9];
            state(q, cs.as_mut_ptr());
            assert_eq!(cs, rust.state());
            assert_eq!(c_sink.words, r_sink.words, "emitted words {case}");
            dispose(q);
            rust.mlp_output_queue_dispose();
            state(q, cs.as_mut_ptr());
            assert_eq!(cs, rust.state());
            destroy(q);
        }
        FreeLibrary(module);
    }
}

#[test]
fn c_analysis_parity() {
    use crate::c_analysis::*;
    let path = std::ffi::CString::new(
        std::env::var("MLP_PLANNING_REFERENCE").expect("analysis parity requires C reference"),
    )
    .unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let select: unsafe extern "C" fn(
            *const f64,
            *const f64,
            u32,
            u32,
            usize,
            *const u32,
            *const u32,
            i32,
            *mut crate::matrix::MlpMatrixPrimitive,
            *mut u32,
            *mut u32,
            i32,
        ) -> i32 = std::mem::transmute(GetProcAddress(
            module,
            c"matrix_select_analysis".as_ptr().cast(),
        ));
        let design: unsafe extern "C" fn(
            *const [f64; 6],
            u32,
            *mut crate::c_downmix::MlpDownmixDesign,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"design_downmix".as_ptr().cast()));
        let render: unsafe extern "C" fn(
            *const crate::matrix::MlpMatrixPrimitive,
            u32,
            *const u32,
            *const i32,
            usize,
            u32,
            *const i32,
            *const i32,
            *mut f32,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"render_stereo".as_ptr().cast()));
        let check: unsafe extern "C" fn(
            *mut crate::c_downmix::MlpDownmixCheck,
            *const f32,
            usize,
            u32,
            *const u32,
            *mut i32,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"downmix_pcm".as_ptr().cast()));
        let plan: unsafe extern "C" fn(
            *const crate::matrix::MlpMatrixCandidate,
            *const crate::matrix::MlpMatrixCandidate,
            u32,
            u32,
            *const u32,
            *const u32,
            *const u32,
            *const u32,
            *mut crate::c_downmix::MlpDownmixPlan,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"downmix_plan".as_ptr().cast()));
        let process: unsafe extern "C" fn(
            *mut crate::c_downmix::MlpDownmixState,
            *const crate::c_downmix::MlpDownmixPlan,
            u32,
            *const u32,
            *const u32,
            *const i32,
            usize,
            u32,
            *mut crate::c_downmix::MlpDownmixBlock,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"process_downmix".as_ptr().cast()));
        let log: unsafe extern "C" fn(f64, *mut u8) =
            std::mem::transmute(GetProcAddress(module, c"extended_log".as_ptr().cast()));
        let init: unsafe extern "C" fn(*mut MlpMatrixAnalysis, u32) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"analysis_init".as_ptr().cast()));
        let reset: unsafe extern "C" fn(*mut MlpMatrixAnalysis) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"analysis_reset".as_ptr().cast()));
        let add: unsafe extern "C" fn(*mut MlpMatrixAnalysis, *const i32, usize, u32) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"analysis_add".as_ptr().cast()));
        let decorrelate: unsafe extern "C" fn(
            *const f64,
            *const f64,
            u32,
            u32,
            f64,
            *mut u32,
            *mut f64,
            *mut f64,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"decorrelate".as_ptr().cast()));
        let mut seed = 1u32;
        for precision in [0x0200, 0x0300] {
            let _scope = crate::extended::Extended::precision_scope(precision);
            for case in 0..6000usize {
                let coefficients = std::array::from_fn(|row| {
                    std::array::from_fn(|j| {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        if case % 7 == 0 {
                            if row == j { 1.0 } else { 0.0 }
                        } else {
                            ((seed >> 16) as i32 - 32768) as f64 / 32768.0
                        }
                    })
                });
                let mut expected_design = crate::c_downmix::MlpDownmixDesign::default();
                let rc = design(
                    coefficients.as_ptr(),
                    (case % 15) as u32,
                    &mut expected_design,
                );
                let actual_design =
                    crate::c_downmix::mlp_matrix_design_downmix(&coefficients, (case % 15) as u32);
                assert_eq!(rc == 0, actual_design.is_ok());
                if let Ok(actual) = actual_design {
                    assert_eq!(
                        expected_design, actual,
                        "downmix design case {case} precision {precision:x}"
                    );
                    let channels = case % 5 + 2;
                    let scales = std::array::from_fn(|i| ((case + i) % 3) as u32);
                    let shifts = std::array::from_fn(|i| ((case + i) % 8) as u32);
                    let qss = std::array::from_fn(|i| ((case / 8 + i) % 8) as u32);
                    let mut expected_plan = crate::c_downmix::MlpDownmixPlan::default();
                    let rc = plan(
                        actual.forward.as_ptr(),
                        actual.inverse.as_ptr(),
                        2,
                        channels as u32,
                        scales.as_ptr(),
                        shifts.as_ptr(),
                        qss.as_ptr(),
                        actual.post_shift.as_ptr(),
                        &mut expected_plan,
                    );
                    let actual_plan = crate::c_downmix::mlp_matrix_downmix_plan(
                        &actual.forward,
                        &actual.inverse,
                        channels,
                        &scales,
                        &shifts,
                        &qss,
                        &actual.post_shift,
                    );
                    assert_eq!(rc == 0, actual_plan.is_ok());
                    if let Ok(actual) = actual_plan {
                        assert_eq!(expected_plan, actual, "downmix plan case {case}");
                        let mut cs = crate::c_downmix::MlpDownmixState {
                            forward_seed: 1,
                            inverse_seed: 2,
                            ..Default::default()
                        };
                        let mut rs = cs;
                        for block in 0..3 {
                            let n = (case + block * 17) % 160 + 1;
                            let pcm: Vec<i32> = (0..n * channels)
                                .map(|_| {
                                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                                    (seed >> 8) as i32 - 8388608
                                })
                                .collect();
                            let flags = if block == 0 { 2 } else { 0 };
                            let mut expected = crate::c_downmix::MlpDownmixBlock::default();
                            let rc = process(
                                &mut cs,
                                &expected_plan,
                                channels as u32,
                                shifts.as_ptr(),
                                qss.as_ptr(),
                                pcm.as_ptr(),
                                n,
                                flags,
                                &mut expected,
                            );
                            let output = crate::c_downmix::mlp_matrix_process_downmix(
                                &mut rs, &actual, channels, &shifts, &qss, &pcm, n, flags,
                            );
                            assert_eq!(rc == 0, output.is_ok(), "process status case {case}");
                            assert_eq!(cs, rs);
                            if let Ok(output) = output {
                                assert_eq!(expected, output, "process case {case} block {block}");
                            }
                        }
                    }
                }
                let n = case % 161;
                let pcm: Vec<i32> = (0..n * 2)
                    .map(|_| {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        (seed >> 8) as i32 - 8388608
                    })
                    .collect();
                let noise: Vec<i32> = (0..n).map(|j| (j as i32 - 80) * 65536).collect();
                let mut matrices = vec![crate::matrix::MlpMatrixPrimitive::default(); case % 7];
                for (i, m) in matrices.iter_mut().enumerate() {
                    m.target = (i % 2) as u32;
                    for c in &mut m.coefficient[..4] {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        *c = (seed >> 16) as i32 - 32768;
                    }
                }
                let q = [(case % 16) as u32, ((case / 16) % 16) as u32];
                let mut stereo = vec![0.0f32; n * 2];
                let rc = render(
                    matrices.as_ptr(),
                    matrices.len() as u32,
                    q.as_ptr(),
                    pcm.as_ptr(),
                    n,
                    2,
                    noise.as_ptr(),
                    noise.as_ptr(),
                    stereo.as_mut_ptr(),
                );
                let (actual, overflow) = crate::c_downmix::mlp_matrix_render_stereo(
                    &matrices, &q, &pcm, n, 2, &noise, &noise,
                )
                .unwrap();
                assert_eq!(rc, i32::from(overflow));
                assert_eq!(
                    stereo.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    "render case {case}"
                );
                let shifts = [(case % 3) as u32, ((case / 3) % 3) as u32];
                let mut cs = crate::c_downmix::MlpDownmixCheck::default();
                let mut rs = cs;
                let mut clipped = vec![0i32; n * 2];
                let rc = check(
                    &mut cs,
                    stereo.as_ptr(),
                    n,
                    2,
                    shifts.as_ptr(),
                    clipped.as_mut_ptr(),
                );
                let result =
                    crate::c_downmix::mlp_matrix_downmix_pcm(&mut rs, &actual, n, 2, &shifts);
                assert_eq!(rc >= 0, result.is_ok());
                assert_eq!(cs, rs);
                if let Ok((output, overflow)) = result {
                    assert_eq!(rc, i32::from(overflow));
                    assert_eq!(clipped, output);
                }
                let x = ((case + 1) as f64) / 6001.0;
                let mut expected = [0u8; 10];
                log(x, expected.as_mut_ptr());
                assert_eq!(
                    expected,
                    crate::extended::Extended::from_f64(x).log().bytes(),
                    "log case {case} precision {precision:x}"
                );
                let channels = case % 6 + 1;
                let stride = channels + (case / 6) % (7 - channels);
                let mut c = MlpMatrixAnalysis::default();
                let mut r = c.clone();
                assert_eq!(init(&mut c, channels as u32), 0);
                mlp_matrix_analysis_init(&mut r, channels as u32).unwrap();
                for block in 0..3 {
                    if block == 2 && case % 2 == 0 {
                        assert_eq!(reset(&mut c), 0);
                        mlp_matrix_analysis_reset(&mut r).unwrap();
                    }
                    let count = (case * 13 + block * 37) % 160 + 1;
                    let pcm: Vec<i32> = (0..count * stride)
                        .map(|n| {
                            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                            if case % 3 == 0 {
                                ((n / stride * 113) % 65536) as i32 - 32768
                            } else {
                                (seed >> 8) as i32 - 8388608
                            }
                        })
                        .collect();
                    assert_eq!(add(&mut c, pcm.as_ptr(), count, stride as u32), 0);
                    mlp_matrix_analysis_add(&mut r, &pcm, count, stride).unwrap();
                    assert_eq!(
                        c, r,
                        "analysis case {case} block {block} precision {precision:x}"
                    );
                }
                let ratio = std::array::from_fn(|i| if i < channels { r.energy[i] } else { 0.0 });
                let fixed = case % (channels + 1);
                let qss = std::array::from_fn(|i| if case % 4 == 0 { (i % 3) as u32 } else { 0 });
                let scales = std::array::from_fn(|i| ((case + i) % 9) as u32);
                let mut cm = [crate::matrix::MlpMatrixPrimitive::default(); 6];
                let mut rm = cm;
                let mut cp = 0u32;
                let mut rp = 0usize;
                let mut cb = 0u32;
                let mut rb = 0u32;
                for append in [false, true] {
                    let rc = select(
                        c.covariance.as_ptr(),
                        ratio.as_ptr(),
                        channels as u32,
                        fixed as u32,
                        c.samples,
                        qss.as_ptr(),
                        scales.as_ptr(),
                        (case % 2) as i32,
                        cm.as_mut_ptr(),
                        &mut cp,
                        &mut cb,
                        i32::from(append),
                    );
                    let result = if append {
                        mlp_matrix_select_append(
                            &r.covariance,
                            &ratio,
                            channels,
                            fixed,
                            r.samples,
                            &qss,
                            &scales,
                            case % 2 != 0,
                            &mut rm,
                            &mut rp,
                            &mut rb,
                        )
                    } else {
                        mlp_matrix_select(
                            &r.covariance,
                            &ratio,
                            channels,
                            fixed,
                            r.samples,
                            &qss,
                            &scales,
                            case % 2 != 0,
                            &mut rm,
                            &mut rp,
                            &mut rb,
                        )
                    };
                    assert_eq!(rc == 0, result.is_ok(), "select status case {case}");
                    assert_eq!(
                        cm, rm,
                        "select case {case} precision {precision:x} append {append}"
                    );
                    assert_eq!((cp as usize, cb), (rp, rb));
                }
                let mut order = [0u32; 6];
                let mut transform = [0.0f64; 36];
                let mut reduced = [0.0f64; 36];
                let rc = decorrelate(
                    c.covariance.as_ptr(),
                    ratio.as_ptr(),
                    channels as u32,
                    fixed as u32,
                    1.0,
                    order.as_mut_ptr(),
                    transform.as_mut_ptr(),
                    reduced.as_mut_ptr(),
                );
                let result = mlp_matrix_decorrelate(&r.covariance, &ratio, channels, fixed, 1.0);
                assert_eq!(rc == 0, result.is_ok(), "decorrelate status case {case}");
                if let Ok((ro, rt, rr)) = result {
                    assert_eq!(order, ro);
                    assert_eq!(
                        transform.map(f64::to_bits),
                        rt.map(f64::to_bits),
                        "transform case {case} precision {precision:x}"
                    );
                    assert_eq!(
                        reduced.map(f64::to_bits),
                        rr.map(f64::to_bits),
                        "reduced case {case} precision {precision:x}"
                    );
                }
            }
        }
        FreeLibrary(module);
    }
}

#[test]
fn c_substream_parity() {
    use crate::c_restart::MlpRestart;
    use crate::c_substream::{MlpSubstream, mlp_substream_put};
    use crate::parameters::{MlpCodingParams, MlpParameters};
    #[repr(C)]
    struct Input {
        restart: *const MlpRestart,
        initial: *const MlpParameters,
        main: *const MlpParameters,
        previous: *const MlpParameters,
        residual: *const i32,
        bypass: *const u8,
        count: usize,
        stride: usize,
        bypass_bits: u32,
        primary: i32,
        end_markers: i32,
        single_restart: i32,
    }
    let Ok(path) = std::env::var("MLP_PLANNING_REFERENCE") else {
        return;
    };
    let path = std::ffi::CString::new(path).unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let address = GetProcAddress(module, c"substream_put".as_ptr().cast());
        assert!(!address.is_null());
        let reference: unsafe extern "C" fn(*const Input, *mut u32, *mut usize) -> i32 =
            std::mem::transmute(address);
        for case in 0..12000usize {
            let channels = case % 16 + 1;
            let count = case % 153 + 8;
            let restart = case % 3 != 0;
            let single = count < 16 || case % 3 == 1;
            let mut main = MlpParameters {
                maximum_channel: (channels - 1) as u32,
                blocksize: count as u32,
                flags: 2,
                ..Default::default()
            };
            for ch in 0..channels {
                main.coding[ch] = MlpCodingParams {
                    mode: (case % 4) as i32,
                    total_width: 8,
                    offset: 0,
                };
            }
            let mut initial = main.clone();
            initial.blocksize = 8;
            let previous = main.clone();
            let h = MlpRestart {
                maximum_channel: (channels - 1) as u32,
                maximum_bits: 8,
                ..Default::default()
            };
            let residual: Vec<i32> = (0..count * channels)
                .map(|n| ((n * 31 + case) % 256) as i32 - 128)
                .collect();
            let bypass: Vec<u8> = (0..count).map(|n| n as u8).collect();
            let bits = (case % 9) as u32;
            let s = MlpSubstream {
                main: &main,
                previous: &previous,
                initial: Some(&initial),
                restart: restart.then_some(&h),
                residual: &residual,
                bypass: &bypass,
                count,
                stride: channels,
                bypass_bits: bits,
                primary: case % 2 != 0,
                single_restart: restart && single,
                end_markers: case % 5 == 0,
            };
            let input = Input {
                restart: s.restart.map_or(std::ptr::null(), |h| h),
                initial: &initial,
                main: &main,
                previous: &previous,
                residual: residual.as_ptr(),
                bypass: bypass.as_ptr(),
                count,
                stride: channels,
                bypass_bits: bits,
                primary: s.primary as i32,
                end_markers: s.end_markers as i32,
                single_restart: s.single_restart as i32,
            };
            let mut words = vec![0u32; 32768];
            let mut n = 0;
            let rc = reference(&input, words.as_mut_ptr(), &mut n);
            let mut actual = crate::Bits::default();
            assert_eq!(
                mlp_substream_put(&mut actual, &s).is_ok(),
                rc == 0,
                "case {case}"
            );
            assert_eq!(actual.words, &words[..n], "case {case}");
        }
        FreeLibrary(module);
    }
}

#[test]
fn c_restart_parity() {
    use crate::c_restart::{MlpRestart, mlp_restart_put};
    let Ok(path) = std::env::var("MLP_PLANNING_REFERENCE") else {
        return;
    };
    let path = std::ffi::CString::new(path).unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let address = GetProcAddress(module, c"restart_put".as_ptr().cast());
        assert!(!address.is_null());
        let reference: unsafe extern "C" fn(
            *const MlpRestart,
            i32,
            *mut u32,
            *mut usize,
            *mut u32,
            *mut u32,
        ) -> i32 = std::mem::transmute(address);
        let mut rng = 1u32;
        for case in 0..12000 {
            let mut next = || {
                rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                rng
            };
            let maximum = case % 16;
            let mut h = MlpRestart {
                timing: next() & 65535,
                minimum_channel: next() % (maximum + 1),
                maximum_channel: maximum,
                dither_shift: next() & 15,
                seed: next() & 0x7fffff,
                maximum_shift: next(),
                maximum_lsbs: next() & 31,
                maximum_bits: next() & 31,
                lossless_check: next() & 255,
                assignment: std::array::from_fn(|_| next() & 63),
            };
            if case % 7 == 0 {
                h.seed = 0x800000;
            }
            let primary = case % 2 != 0;
            let mut words = [0u32; 16];
            let mut n = 0;
            let mut pending = 0;
            let mut width = 0;
            let rc = reference(
                &h,
                primary as i32,
                words.as_mut_ptr(),
                &mut n,
                &mut pending,
                &mut width,
            );
            let mut actual = crate::Bits::default();
            assert_eq!(
                mlp_restart_put(&mut actual, &h, primary).is_ok(),
                rc == 0,
                "case {case}"
            );
            assert_eq!(actual.words, &words[..n], "case {case}");
            assert_eq!(
                (actual.pending, actual.width),
                (pending, width),
                "case {case}"
            );
        }
        FreeLibrary(module);
    }
}

#[test]
fn c_parameters_parity() {
    use crate::parameters::{MlpParameters, mlp_parameters_put};
    let Ok(path) = std::env::var("MLP_PLANNING_REFERENCE") else {
        return;
    };
    let path = std::ffi::CString::new(path).unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let address = GetProcAddress(module, c"parameters_put".as_ptr().cast());
        assert!(!address.is_null());
        let reference: unsafe extern "C" fn(
            *const MlpParameters,
            *const MlpParameters,
            i32,
            i32,
            *mut u32,
            *mut usize,
            *mut u32,
            *mut u32,
        ) -> i32 = std::mem::transmute(address);
        let mut rng = 1u32;
        for case in 0..12000usize {
            let mut current = MlpParameters {
                maximum_channel: (case % 16) as u32,
                flags: 2,
                blocksize: 40 + (case % 121) as u32,
                matrix_changed: 1,
                matrix_count: (case % 16) as u32,
                ..Default::default()
            };
            let mut old = MlpParameters::default();
            for p in current
                .matrix
                .iter_mut()
                .take(current.matrix_count as usize)
            {
                p.target = (case % 16) as u32;
                p.bypass = (case % 2) as u32;
                for v in &mut p.coefficient {
                    rng = rng.wrapping_mul(214013).wrapping_add(2531011);
                    *v = (rng as i16) as i32;
                }
            }
            for i in 0..=current.maximum_channel as usize {
                current.output_shift[i] = ((case + i) % 16) as i32 - 8;
                current.qss[i] = ((case + i) % 16) as u32;
                current.coding[i].mode = ((case + i) % 4) as i32;
                current.coding[i].total_width = ((case + i) % 32) as i32;
                current.coding[i].offset = ((case * 13 + i) % 32768) as i32 - 16384;
                old.coding[i] = current.coding[i];
                if case % 3 != 0 {
                    old.coding[i].offset = 0;
                }
                for f in [&mut current.a[i], &mut current.b[i]] {
                    f.changed = (case % 2) as u32;
                    f.precision = 8 + (case % 8) as u32;
                    f.order = ((case + i) % 9) as u32;
                    for c in &mut f.coefficient {
                        rng = rng.wrapping_mul(214013).wrapping_add(2531011);
                        *c = (rng as i16) as f64 / 65536.0;
                    }
                }
                current.state[i].changed = 1;
                current.state[i].count = current.b[i].order;
                for v in &mut current.state[i].value {
                    rng = rng.wrapping_mul(214013).wrapping_add(2531011);
                    *v = (rng as i16 as i32) / 16 * 16;
                }
            }
            if case % 10 == 0 {
                current.a[0].precision = 16;
            }
            let restart = case % 2 != 0;
            let initial = case % 4 >= 2;
            let width = (case % 16) as u32;
            let pending = (case as u32) & ((1 << width) - 1);
            let mut writer = crate::Bits {
                words: vec![0x1234],
                pending,
                width,
            };
            let mut storage = [0u32; 2048];
            let mut count = 0;
            let mut c_pending = pending;
            let mut c_width = width;
            let rc = reference(
                &current,
                &old,
                i32::from(restart),
                i32::from(initial),
                storage.as_mut_ptr(),
                &mut count,
                &mut c_pending,
                &mut c_width,
            );
            let result = mlp_parameters_put(&mut writer, &current, &old, restart, initial);
            assert_eq!(result.is_ok(), rc == 0, "case {case}");
            if rc == 0 {
                assert_eq!(&writer.words[1..], &storage[..count], "words case {case}");
                assert_eq!(
                    (writer.pending, writer.width),
                    (c_pending, c_width),
                    "pending case {case}"
                );
            } else {
                assert_eq!(writer.words, vec![0x1234]);
                assert_eq!((writer.pending, writer.width), (pending, width));
            }
        }
        FreeLibrary(module);
    }
}

#[test]
fn c_matrix_apply_parity() {
    use crate::matrix::{MlpMatrixPrimitive, mlp_matrix_apply};
    let Ok(path) = std::env::var("MLP_PLANNING_REFERENCE") else {
        return;
    };
    let path = std::ffi::CString::new(path).unwrap();
    unsafe {
        let module = LoadLibraryA(path.as_ptr().cast());
        assert!(!module.is_null());
        let address = GetProcAddress(module, c"matrix_apply".as_ptr().cast());
        assert!(!address.is_null());
        let reference: unsafe extern "C" fn(
            *const MlpMatrixPrimitive,
            u32,
            u32,
            *const u32,
            *mut i32,
            usize,
            *const i32,
            *const i32,
            *mut u8,
            i32,
            i32,
        ) -> i32 = std::mem::transmute(address);
        let interval_address = GetProcAddress(module, c"matrix_interval".as_ptr().cast());
        let blocks_address = GetProcAddress(module, c"matrix_blocks".as_ptr().cast());
        assert!(!interval_address.is_null() && !blocks_address.is_null());
        let interval: unsafe extern "C" fn(
            *const MlpMatrixPrimitive,
            *mut u32,
            u32,
            *const u32,
            *mut i32,
            usize,
            *mut u8,
            *mut u32,
        ) -> i32 = std::mem::transmute(interval_address);
        let blocks: unsafe extern "C" fn(
            *const MlpMatrixPrimitive,
            u32,
            *mut u32,
            u32,
            *const [u32; 6],
            *const u32,
            u32,
            *mut i32,
            *mut u8,
            *mut u32,
        ) -> i32 = std::mem::transmute(blocks_address);
        let noise_address = GetProcAddress(module, c"matrix_noise".as_ptr().cast());
        assert!(!noise_address.is_null());
        let noise: unsafe extern "C" fn(*mut u32, usize, u32, *mut i32, *mut i32) -> i32 =
            std::mem::transmute(noise_address);
        let dither_address = GetProcAddress(module, c"matrix_dither".as_ptr().cast());
        assert!(!dither_address.is_null());
        let dither: unsafe extern "C" fn(
            *mut MlpMatrixPrimitive,
            u32,
            u32,
            u32,
            *const u32,
            *mut u32,
        ) -> i32 = std::mem::transmute(dither_address);
        use crate::matrix::{
            MlpMatrixCandidate, mlp_matrix_build_forward, mlp_matrix_build_reverse,
        };
        let forward_address = GetProcAddress(module, c"matrix_forward".as_ptr().cast());
        let reverse_address = GetProcAddress(module, c"matrix_reverse".as_ptr().cast());
        assert!(!forward_address.is_null() && !reverse_address.is_null());
        let forward: unsafe extern "C" fn(
            *const MlpMatrixCandidate,
            u32,
            u32,
            *const u32,
            *const u32,
            *mut MlpMatrixPrimitive,
            *mut u32,
            *mut u32,
            *mut i32,
            *mut u32,
        ) -> i32 = std::mem::transmute(forward_address);
        let reverse: unsafe extern "C" fn(
            *const MlpMatrixCandidate,
            u32,
            *const i32,
            *const u32,
            *mut MlpMatrixPrimitive,
            *mut u32,
        ) -> i32 = std::mem::transmute(reverse_address);
        let mut rng = 7u32;
        for case in 0..12000 {
            let channels = 1 + case % 6;
            let mut candidates = vec![MlpMatrixCandidate::default(); channels];
            let mut scales = [0; 6];
            let mut shifts = [0; 6];
            for (i, p) in candidates.iter_mut().enumerate() {
                p.target = ((i + case) % channels) as u32;
                scales[i] = ((case + i) % 9) as u32;
                shifts[i] = ((case + i * 3) % 32) as u32;
                for v in &mut p.coefficient {
                    rng = rng.wrapping_mul(214013).wrapping_add(2531011);
                    *v = rng as i32;
                }
                if case % 3 == 0 {
                    p.coefficient[p.target as usize] = 16384;
                }
            }
            let mut actual_forward = [MlpMatrixPrimitive::default(); 15];
            let mut expected_forward = actual_forward;
            let mut actual_count = 999;
            let mut expected_count = actual_count;
            let mut actual_bits = 999;
            let mut expected_bits = actual_bits;
            let mut actual_signs = [1; 6];
            let mut expected_signs = actual_signs;
            let mut actual_dims = shifts;
            let mut expected_dims = actual_dims;
            let n = case % (channels + 1);
            let rc = forward(
                candidates.as_ptr(),
                n as u32,
                channels as u32,
                scales.as_ptr(),
                shifts.as_ptr(),
                expected_forward.as_mut_ptr(),
                &mut expected_count,
                &mut expected_bits,
                expected_signs.as_mut_ptr(),
                expected_dims.as_mut_ptr(),
            );
            assert_eq!(
                mlp_matrix_build_forward(
                    &candidates[..n],
                    channels,
                    &scales,
                    &shifts,
                    &mut actual_forward,
                    &mut actual_count,
                    &mut actual_bits,
                    &mut actual_signs,
                    &mut actual_dims
                ),
                rc
            );
            assert_eq!(
                (
                    actual_forward,
                    actual_count,
                    actual_bits,
                    actual_signs,
                    actual_dims
                ),
                (
                    expected_forward,
                    expected_count,
                    expected_bits,
                    expected_signs,
                    expected_dims
                )
            );
            let mut actual_reverse = [MlpMatrixPrimitive::default(); 6];
            let mut expected_reverse = actual_reverse;
            let mut actual_final = [999; 6];
            let mut expected_final = actual_final;
            let rc = reverse(
                candidates.as_ptr(),
                channels as u32,
                actual_signs.as_ptr(),
                actual_dims.as_ptr(),
                expected_reverse.as_mut_ptr(),
                expected_final.as_mut_ptr(),
            );
            assert_eq!(
                mlp_matrix_build_reverse(
                    &candidates,
                    channels,
                    &actual_signs,
                    &actual_dims,
                    &mut actual_reverse,
                    &mut actual_final
                ),
                rc
            );
            assert_eq!(
                (actual_reverse, actual_final),
                (expected_reverse, expected_final)
            );
            let count = 1 + case % 160;
            let mut seed = rng;
            let mut expected_seed = seed;
            let mut first = vec![0; count];
            let mut second = first.clone();
            let mut expected_first = first.clone();
            let mut expected_second = first.clone();
            let shift = (case % 32) as u32;
            assert_eq!(
                noise(
                    &mut expected_seed,
                    count,
                    shift,
                    expected_first.as_mut_ptr(),
                    expected_second.as_mut_ptr()
                ),
                crate::matrix::mlp_matrix_noise(&mut seed, count, shift, &mut first, &mut second)
            );
            assert_eq!(
                (seed, first, second),
                (expected_seed, expected_first, expected_second)
            );
            let primitives = case % 7;
            let mut matrix = vec![MlpMatrixPrimitive::default(); primitives];
            let mut qss = [0u32; 6];
            for (ch, q) in qss.iter_mut().enumerate().take(channels) {
                *q = ((case + ch) % 16) as u32;
            }
            for (i, p) in matrix.iter_mut().enumerate() {
                p.target = (i % channels) as u32;
                p.bypass = u32::from((case + i) % 3 == 0);
                if p.bypass != 0 {
                    qss[p.target as usize] = 0;
                }
                for coefficient in &mut p.coefficient[..channels + 2] {
                    rng = rng.wrapping_mul(214013).wrapping_add(2531011);
                    *coefficient = ((rng >> 16) as i32 - 32768) / 4;
                }
                p.coefficient[p.target as usize] = if p.bypass != 0 { -32768 } else { -16384 };
            }
            let mut dimensions = [0u32; 6];
            for (i, v) in dimensions.iter_mut().enumerate() {
                *v = ((case + i * 7) % 32) as u32;
            }
            let mut actual_dither = matrix.clone();
            actual_dither.push(MlpMatrixPrimitive::default());
            let mut expected_dither = actual_dither.clone();
            let mut actual_shift = 999;
            let mut expected_shift = actual_shift;
            assert_eq!(
                dither(
                    expected_dither.as_mut_ptr(),
                    primitives as u32,
                    channels as u32,
                    channels as u32,
                    dimensions.as_ptr(),
                    &mut expected_shift
                ),
                crate::matrix::mlp_matrix_add_dither(
                    &mut actual_dither,
                    primitives,
                    channels,
                    channels,
                    &dimensions,
                    &mut actual_shift
                )
            );
            assert_eq!(
                (actual_dither, actual_shift),
                (expected_dither, expected_shift)
            );
            let mut pcm = vec![0; count * channels];
            for sample in &mut pcm {
                rng = rng.wrapping_mul(214013).wrapping_add(2531011);
                *sample = (rng as i32 >> 8) / if case % 2 == 0 { 1 } else { 32 };
            }
            let extra0 = vec![12345; count];
            let extra1 = vec![-23456; count];
            let mut bits = vec![case as u8; count];
            let mut expected = pcm.clone();
            let mut expected_bits = bits.clone();
            let inverse = case % 2 != 0;
            let clip = case % 4 >= 2;
            let rc = reference(
                matrix.as_ptr(),
                primitives as u32,
                channels as u32,
                qss.as_ptr(),
                expected.as_mut_ptr(),
                count,
                extra0.as_ptr(),
                extra1.as_ptr(),
                expected_bits.as_mut_ptr(),
                i32::from(inverse),
                i32::from(clip),
            );
            assert_eq!(
                mlp_matrix_apply(
                    &matrix,
                    channels,
                    &qss,
                    &mut pcm,
                    count,
                    Some(&extra0),
                    Some(&extra1),
                    Some(&mut bits),
                    inverse,
                    clip
                ),
                rc,
                "case {case}"
            );
            assert_eq!(pcm, expected, "PCM case {case}");
            assert_eq!(bits, expected_bits, "bypass case {case}");
            for p in &mut matrix {
                p.coefficient[channels..].fill(0);
            }
            let lengths = [count, count];
            let c_lengths = [count as u32, count as u32];
            let rows = [qss, qss];
            let mut input = pcm.clone();
            input.extend_from_slice(&pcm);
            // An overflowing un-clipped primitive is valid output above, but not valid interval input.
            for sample in &mut input {
                *sample = (*sample).clamp(-8388608, 8388607);
            }
            for mode in 0..3 {
                let mut actual = input.clone();
                let mut expected = input.clone();
                let mut actual_bits = vec![0; count * 2];
                let mut expected_bits = actual_bits.clone();
                let mut actual_count = primitives;
                let mut expected_count = primitives as u32;
                let mut actual_used = 0;
                let mut expected_used = 0;
                let expected_rc = if mode == 0 {
                    interval(
                        matrix.as_ptr(),
                        &mut expected_count,
                        channels as u32,
                        qss.as_ptr(),
                        expected.as_mut_ptr(),
                        count * 2,
                        expected_bits.as_mut_ptr(),
                        &mut expected_used,
                    )
                } else {
                    blocks(
                        matrix.as_ptr(),
                        0,
                        &mut expected_count,
                        channels as u32,
                        rows.as_ptr(),
                        c_lengths.as_ptr(),
                        2,
                        expected.as_mut_ptr(),
                        expected_bits.as_mut_ptr(),
                        &mut expected_used,
                    )
                };
                let actual_rc = match mode {
                    0 => crate::matrix::mlp_matrix_apply_interval(
                        &matrix,
                        &mut actual_count,
                        channels,
                        &qss,
                        &mut actual,
                        count * 2,
                        &mut actual_bits,
                        &mut actual_used,
                    ),
                    1 => crate::matrix::mlp_matrix_apply_blocks(
                        &matrix,
                        &mut actual_count,
                        channels,
                        &rows,
                        &lengths,
                        &mut actual,
                        &mut actual_bits,
                        &mut actual_used,
                    ),
                    _ => crate::matrix::mlp_matrix_apply_suffix_blocks(
                        &matrix,
                        0,
                        &mut actual_count,
                        channels,
                        &rows,
                        &lengths,
                        &mut actual,
                        &mut actual_bits,
                        &mut actual_used,
                    ),
                };
                assert_eq!(
                    (actual_rc, actual_count, actual_used),
                    (expected_rc, expected_count as usize, expected_used),
                    "interval case {case} mode {mode}"
                );
                assert_eq!(actual, expected, "interval PCM case {case} mode {mode}");
                assert_eq!(
                    actual_bits, expected_bits,
                    "interval bits case {case} mode {mode}"
                );
            }
        }
        FreeLibrary(module);
    }
}

#[test]
fn c_planning_parity() {
    let Ok(path) = std::env::var("MLP_PLANNING_REFERENCE") else {
        return;
    };
    let path = std::ffi::CString::new(path).unwrap();
    let module = unsafe { LoadLibraryA(path.as_ptr().cast()) };
    assert!(!module.is_null());
    unsafe {
        let boundary: unsafe extern "C" fn(*mut MlpBoundaryState, u32, u32, u32) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"interval_boundary".as_ptr().cast()));
        let find: unsafe extern "C" fn(*mut MlpIntervalSlot, u32, *mut u32) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"interval_find".as_ptr().cast()));
        let scale: unsafe extern "C" fn(
            *const u32,
            u32,
            u32,
            u32,
            *const u32,
            *mut MlpScalePlan,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"scale_analyze".as_ptr().cast()));
        let predict: unsafe extern "C" fn(
            *mut MlpPredictFilter,
            *mut MlpPredictFilter,
            *mut MlpPredictState,
            u32,
            *const i32,
            usize,
            *mut i32,
            u32,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"predict_block".as_ptr().cast()));
        let correlation: unsafe extern "C" fn(
            *const i32,
            *const usize,
            usize,
            u32,
            *mut f64,
        ) -> i32 = std::mem::transmute(GetProcAddress(
            module,
            c"search_correlation".as_ptr().cast(),
        ));
        let reflection: unsafe extern "C" fn(*const f64, u32, *mut f64, *mut f64) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"search_reflection".as_ptr().cast()));
        let stable: unsafe extern "C" fn(*const f64, u32) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"search_stable".as_ptr().cast()));
        let random: unsafe extern "C" fn(*mut u32) -> u32 =
            std::mem::transmute(GetProcAddress(module, c"search_rand".as_ptr().cast()));
        let random_filter: unsafe extern "C" fn(
            *mut u32,
            u32,
            u32,
            u32,
            *mut crate::search::MlpWireFilter,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"search_random".as_ptr().cast()));
        let evaluate: unsafe extern "C" fn(
            *const f64,
            u32,
            *const f64,
            u32,
            u32,
            *mut f64,
            *mut f64,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"search_evaluate".as_ptr().cast()));
        let pool_init: unsafe extern "C" fn(*mut crate::search::MlpSearchPool, u32) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"search_pool_init".as_ptr().cast()));
        let pool_select: unsafe extern "C" fn(
            *mut crate::search::MlpSearchPool,
            *const f64,
            u32,
            u32,
            u32,
            u32,
            *mut crate::search::MlpWireFilter,
            *mut f64,
            *mut f64,
        ) -> i32 = std::mem::transmute(GetProcAddress(
            module,
            c"search_pool_select".as_ptr().cast(),
        ));
        let joint: unsafe extern "C" fn(
            *mut crate::search::MlpSearchPool,
            *const f64,
            u32,
            u32,
            u32,
            u32,
            f64,
            *mut crate::search::MlpWireFilter,
            *mut crate::search::MlpWireFilter,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"search_joint".as_ptr().cast()));
        let fir: unsafe extern "C" fn(*const f64, u32, *mut crate::search::MlpWireFilter) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"search_fir".as_ptr().cast()));
        let feedback: unsafe extern "C" fn(
            *const i32,
            *const crate::search::MlpWireFilter,
            *const crate::search::MlpWireFilter,
            u32,
            *mut i32,
            *mut u32,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"search_feedback".as_ptr().cast()));
        let interval: unsafe extern "C" fn(
            *mut crate::search::MlpSearchPool,
            *const i32,
            *const usize,
            usize,
            u32,
            u32,
            u32,
            u32,
            *mut crate::search::MlpSearchPlan,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"search_interval".as_ptr().cast()));
        let mut rng = 1u32;
        for case in 0..12000usize {
            let mut next = || {
                rng = rng.wrapping_mul(214013).wrapping_add(2531011);
                rng
            };
            let lengths = [8, 32, 40, 80, 160];
            let search_pcm: Vec<i32> = (0..320).map(|_| next() as i32 >> 8).collect();
            let lag = case % 50;
            let mut actual_r = [0.0; 50];
            let mut expected_r = [0.0; 50];
            assert_eq!(
                mlp_search_correlation_extended(&search_pcm, &lengths, lag, &mut actual_r),
                correlation(
                    search_pcm.as_ptr(),
                    lengths.as_ptr(),
                    lengths.len(),
                    lag as u32,
                    expected_r.as_mut_ptr()
                )
            );
            assert_eq!(actual_r, expected_r, "correlation case {case}");
            let mut r = [0.0; 9];
            assert_eq!(mlp_search_correlation(&search_pcm, &lengths, 8, &mut r), 0);
            let mut ff = crate::search::MlpWireFilter::default();
            let mut cff = ff.clone();
            assert_eq!(
                crate::search::mlp_search_fir(&r, case % 9, &mut ff),
                fir(r.as_ptr(), (case % 9) as u32, &mut cff)
            );
            assert_eq!(ff, cff, "FIR case {case}");
            let mut k = [0.0; 8];
            let mut ck = k;
            let mut error = 0.0;
            let mut ce = 0.0;
            assert_eq!(
                mlp_search_reflection(&r, case % 9, &mut k, &mut error),
                reflection(r.as_ptr(), (case % 9) as u32, ck.as_mut_ptr(), &mut ce)
            );
            assert_eq!(k, ck, "reflection case {case}");
            assert_eq!(error, ce, "reflection error case {case}");
            let coeff: [f64; 4] = std::array::from_fn(|_| (next() as i32 >> 20) as f64 / 1024.0);
            assert_eq!(
                mlp_search_iir_stable(&coeff, case % 6),
                stable(coeff.as_ptr(), (case % 6) as u32)
            );
            let mut seed = next();
            let mut cseed = seed;
            assert_eq!(mlp_search_rand(&mut seed), random(&mut cseed));
            assert_eq!(seed, cseed);
            let mut filter = crate::search::MlpWireFilter::default();
            let mut cf = filter.clone();
            assert_eq!(
                crate::search::mlp_search_iir_random(
                    &mut seed,
                    (case % 5) as u32,
                    (case % 4 + 1) as u32,
                    100,
                    &mut filter
                ),
                random_filter(
                    &mut cseed,
                    (case % 5) as u32,
                    (case % 4 + 1) as u32,
                    100,
                    &mut cf
                )
            );
            assert_eq!(seed, cseed, "random state case {case}");
            assert_eq!(filter, cf, "random filter case {case}");
            k.fill(0.0);
            ck = k;
            error = 0.0;
            ce = 0.0;
            assert_eq!(
                crate::search::mlp_search_iir_evaluate(
                    &actual_r,
                    lag,
                    &filter.coefficient,
                    filter.order as usize,
                    case % 9,
                    &mut k,
                    &mut error
                ),
                evaluate(
                    actual_r.as_ptr(),
                    lag as u32,
                    filter.coefficient.as_ptr(),
                    filter.order,
                    (case % 9) as u32,
                    ck.as_mut_ptr(),
                    &mut ce
                )
            );
            assert_eq!(k, ck, "evaluate reflection case {case}");
            assert_eq!(error, ce, "evaluate score case {case}");
            if case % 12 == 0 {
                let mut pool = crate::search::MlpSearchPool::default();
                let mut cp = pool.clone();
                assert_eq!(
                    crate::search::mlp_search_pool_init(&mut pool, seed),
                    pool_init(&mut cp, seed)
                );
                assert_eq!(pool, cp, "pool init case {case}");
                for step in 0..4 {
                    assert_eq!(
                        crate::search::mlp_search_pool_select(
                            &mut pool,
                            &actual_r,
                            lag,
                            4,
                            8,
                            step,
                            &mut filter,
                            &mut k,
                            &mut error
                        ),
                        pool_select(
                            &mut cp,
                            actual_r.as_ptr(),
                            lag as u32,
                            4,
                            8,
                            step,
                            &mut cf,
                            ck.as_mut_ptr(),
                            &mut ce
                        )
                    );
                    assert_eq!(pool, cp, "pool state case {case} step {step}");
                    assert_eq!(filter, cf, "pool filter case {case} step {step}");
                    assert_eq!(k, ck, "pool reflection case {case} step {step}");
                    assert_eq!(error, ce, "pool score case {case} step {step}");
                    let mut af = crate::search::MlpWireFilter::default();
                    let mut caf = af.clone();
                    let probability = [0.0, 0.001, 0.5, 1.0][step as usize];
                    assert_eq!(
                        crate::search::mlp_search_joint(
                            &mut pool,
                            &actual_r,
                            lag,
                            4,
                            8,
                            step,
                            probability,
                            &mut af,
                            &mut filter
                        ),
                        joint(
                            &mut cp,
                            actual_r.as_ptr(),
                            lag as u32,
                            4,
                            8,
                            step,
                            probability,
                            &mut caf,
                            &mut cf
                        )
                    );
                    assert_eq!(pool, cp, "joint pool case {case} step {step}");
                    assert_eq!(af, caf, "joint A case {case} step {step}");
                    assert_eq!(filter, cf, "joint B case {case} step {step}");
                    let mut state = [0i32; 4];
                    let mut cstate = state;
                    let mut q = 0u32;
                    let mut cq = q;
                    let first40 = search_pcm[..40].try_into().unwrap();
                    assert_eq!(
                        crate::search::mlp_search_feedback_state(
                            first40, &af, &filter, 64, &mut state, &mut q
                        ),
                        feedback(
                            search_pcm.as_ptr(),
                            &caf,
                            &cf,
                            64,
                            cstate.as_mut_ptr(),
                            &mut cq
                        )
                    );
                    assert_eq!(state, cstate, "feedback state case {case} step {step}");
                    assert_eq!(q, cq, "feedback quantization case {case} step {step}");
                    let mut plan = crate::search::MlpSearchPlan::default();
                    let mut cplan = plan.clone();
                    assert_eq!(
                        crate::search::mlp_search_interval(
                            Some(&mut pool),
                            &search_pcm,
                            &lengths,
                            8,
                            4,
                            lag,
                            step,
                            &mut plan
                        ),
                        interval(
                            &mut cp,
                            search_pcm.as_ptr(),
                            lengths.as_ptr(),
                            lengths.len(),
                            8,
                            4,
                            lag as u32,
                            step,
                            &mut cplan
                        )
                    );
                    assert_eq!(plan, cplan, "interval plan case {case} step {step}");
                    assert_eq!(pool, cp, "interval pool case {case} step {step}");
                }
            }
            let count = (case % 80 + 1) * 2;
            let input: Vec<i32> = (0..count).map(|_| (next() as i32) >> 8).collect();
            let mut a = MlpPredictFilter {
                changed: 1,
                order: 8,
                coefficient: [0.0; 8],
            };
            let mut b = MlpPredictFilter {
                changed: 1,
                order: 4,
                coefficient: [0.0; 8],
            };
            for v in &mut a.coefficient {
                *v = (next() as i32 >> 20) as f64 / 2048.0;
            }
            for v in &mut b.coefficient[..4] {
                *v = (next() as i32 >> 20) as f64 / 4096.0;
            }
            let mut ca = a.clone();
            let mut cb = b.clone();
            let mut state = MlpPredictState::default();
            for v in &mut state.input {
                *v = ((next() as i32) >> 8) as f32;
            }
            for v in &mut state.residual {
                *v = ((next() as i32) >> 8) as f32;
            }
            let mut cs = state;
            let mut output = vec![0; count];
            let mut expected_output = output.clone();
            let use_b = case % 2 == 0;
            let qss = (case % 16) as u32;
            let flag = (case % 3) as u32;
            assert_eq!(
                mlp_predict_block(
                    &mut a,
                    if use_b { Some(&mut b) } else { None },
                    &mut state,
                    qss,
                    &input,
                    &mut output,
                    flag
                ),
                predict(
                    &mut ca,
                    if use_b { &mut cb } else { std::ptr::null_mut() },
                    &mut cs,
                    qss,
                    input.as_ptr(),
                    count,
                    expected_output.as_mut_ptr(),
                    flag
                )
            );
            assert_eq!(a, ca, "predict A case {case}");
            assert_eq!(b, cb, "predict B case {case}");
            assert_eq!(state, cs, "predict state case {case}");
            assert_eq!(output, expected_output, "predict output case {case}");
            let channels = case % 6 + 1;
            let blocks = case % 32 + 1;
            let candidates = case % (channels + 1);
            let mut summary = vec![0u32; blocks * 6];
            for v in &mut summary {
                *v = (next() & 0xffffff) >> (case % 24);
            }
            if case % 13 == 0 {
                summary.fill(0);
            }
            let mut required = [0; 6];
            for v in &mut required {
                *v = next() % 32;
            }
            let mut actual = MlpScalePlan::default();
            let mut expected = actual.clone();
            assert_eq!(
                mlp_scale_analyze(
                    &summary,
                    blocks,
                    channels,
                    candidates,
                    &required,
                    &mut actual
                ),
                scale(
                    summary.as_ptr(),
                    blocks as u32,
                    channels as u32,
                    candidates as u32,
                    required.as_ptr(),
                    &mut expected
                )
            );
            assert_eq!(actual, expected, "scale case {case}");
            let cycle = next() % 2048;
            let span = next() % 1024 + 1;
            let mut actual = MlpBoundaryState {
                position: next(),
                index: next(),
                segments: next(),
            };
            let mut expected = actual;
            for step in 0..40 {
                let samples = if step % 17 == 0 { 0 } else { next() % 161 };
                assert_eq!(
                    mlp_interval_boundary(&mut actual, cycle, span, samples),
                    boundary(&mut expected, cycle, span, samples)
                );
                assert_eq!(actual, expected, "boundary case {case} step {step}");
            }
            let mut slots = [MlpIntervalSlot::default(); MLP_INTERVAL_SLOTS];
            for slot in &mut slots {
                slot.samples = (next() % 162) as i32 - 1;
                slot.flags = next() % 16;
            }
            let mut reference = slots;
            let write = next() % MLP_INTERVAL_SLOTS as u32;
            let mut cursor = (next() % MLP_INTERVAL_SLOTS as u32) as usize;
            let mut expected_cursor = cursor as u32;
            assert_eq!(
                mlp_interval_find(&mut slots, write as usize, &mut cursor),
                find(reference.as_mut_ptr(), write, &mut expected_cursor)
            );
            assert_eq!(cursor, expected_cursor as usize);
            assert_eq!(slots, reference, "find case {case}");
        }
        FreeLibrary(module);
    }
}
