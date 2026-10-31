#[cfg(test)]
use super::input::MatrixInput;
use super::input::PcmInput;
use crate::analysis::decorrelation::{
    MlpMatrixAnalysis, mlp_matrix_analysis_add, mlp_matrix_analysis_reset, mlp_matrix_select,
};
use crate::analysis::matrix::{
    MlpMatrixPrimitive, mlp_matrix_apply_blocks, mlp_matrix_apply_interval,
};
use crate::analysis::scale::{MlpScalePlan, mlp_scale_analyze};
use crate::analysis::search::{MlpSearchPlan, MlpSearchPool, mlp_search_interval};

#[derive(Default)]
pub(crate) struct MatrixInterval {
    pub pcm: Vec<i32>,
    pub bypass: Vec<u8>,
    pub lengths: Vec<usize>,
    pub checks: Vec<u32>,
    pub slot: usize,
    pub offset: usize,
    pub bits: u32,
    pub count: usize,
    pub selected_matrix: bool,
    pub matrix: [MlpMatrixPrimitive; 6],
    pub analysis: MlpMatrixAnalysis,
    pub scale: MlpScalePlan,
    pub original_scale: bool,
    pub enable_matrix: bool,
    pub joint_search: bool,
    pub search_ready: [bool; 6],
    pub prediction: [MlpSearchPlan; 6],
    pub pool: [MlpSearchPool; 6],
    pub search_rng: u32,
}

pub(crate) fn lossless(mut check: u32, pcm: &[i32], count: usize, channels: usize) -> u32 {
    for n in 0..count {
        for ch in 0..channels {
            let v = (pcm[n * channels + ch] as u32 & 0xffffff) << ch;
            check ^= (v ^ (v >> 8) ^ (v >> 16) ^ (v >> 24)) & 255;
        }
    }
    check
}

pub(crate) fn prepare_matrix(
    input: &mut impl PcmInput,
    m: &mut MatrixInterval,
    interval: usize,
    allow_bypass: bool,
    truncated: &mut u32,
) -> Result<(), ()> {
    let channels = input.channels();
    if !(1..=6).contains(&channels)
        || !(1..=128).contains(&interval)
        || ![16, 20, 24].contains(&input.bits())
        || !(8..=160).contains(&input.au_samples())
        || input.position() > input.frames()
    {
        return Err(());
    }
    let mut qss = [0; 6];
    let mut scale = [0; 6];
    let required = [0; 6];
    let mut summary = [0u32; 32 * 6];
    let mut ratio = [0.0; 6];
    let mut total = 0;
    m.lengths.clear();
    m.checks.clear();
    m.pcm.clear();
    m.bypass.clear();
    m.slot = 0;
    m.offset = 0;
    mlp_matrix_analysis_reset(&mut m.analysis)?;
    for i in 0..interval {
        let remaining = input.frames() - input.position();
        if remaining == 0 {
            break;
        }
        let mut capacity = input.au_samples();
        if remaining > capacity && remaining < capacity + 8 {
            capacity = remaining - 8;
        }
        let count = capacity.min(remaining);
        let buffer = input.read(count)?;
        if buffer.len() != count * channels {
            return Err(());
        }
        let p = &buffer;
        m.lengths.push(count);
        m.checks.push(lossless(0, p, count, channels));
        if m.original_scale {
            if i >= 32 {
                return Err(());
            }
            for frame in p.chunks_exact(channels) {
                for (magnitude, sample) in summary[i * 6..i * 6 + channels].iter_mut().zip(frame) {
                    *magnitude |= sample.unsigned_abs();
                }
            }
        }
        m.pcm.extend_from_slice(p);
        total += count;
    }
    m.bypass.resize(total, 0);
    if m.original_scale {
        if mlp_scale_analyze(
            &summary,
            m.lengths.len(),
            channels,
            0,
            &required,
            &mut m.scale,
        ) != 0
        {
            return Err(());
        }
        for &shift in &m.scale.shift[..channels] {
            if shift > 7 {
                return Err(());
            }
        }
    }
    let mut offset = 0;
    let shifts = m.scale.shift;
    for &length in &m.lengths {
        mlp_matrix_analysis_add(
            &mut m.analysis,
            &mut m.pcm[offset * channels..],
            length,
            channels,
            if m.original_scale { Some(&shifts) } else { None },
        )?;
        offset += length;
    }
    for ch in 0..channels {
        qss[ch] = if m.original_scale {
            m.scale.qss[0][ch]
        } else {
            24 - input.bits()
        };
        scale[ch] = if m.original_scale {
            m.scale.scale_count[ch]
        } else {
            u32::from(allow_bypass)
        };
        ratio[ch] = m.analysis.covariance[ch * 7] / m.analysis.energy[ch];
    }
    if m.enable_matrix {
        mlp_matrix_select(crate::analysis::decorrelation::MatrixSelection {
            covariance: &m.analysis.covariance,
            ratio: &ratio,
            channels,
            fixed: 0,
            samples: total,
            qss: &qss,
            scale_count: &scale,
            allow_bypass: (allow_bypass || m.original_scale) && input.au_samples() != 160,
            matrix: &mut m.matrix,
            primitives: &mut m.count,
            bypass_bits: &mut m.bits,
        })?;
        m.selected_matrix = m.count != 0;
        let rc = if m.original_scale {
            mlp_matrix_apply_blocks(crate::analysis::matrix::MatrixIntervalArgs {
                matrix: &m.matrix,
                primitives: &mut m.count,
                channels,
                qss: &m.scale.qss,
                samples: &mut m.pcm,
                count: 0,
                bypass: &mut m.bypass,
                bypass_bits: &mut m.bits,
                lengths: Some(&m.lengths),
                blocks: 0,
                prefix: 0,
            })
        } else {
            mlp_matrix_apply_interval(crate::analysis::matrix::MatrixIntervalArgs {
                matrix: &m.matrix,
                primitives: &mut m.count,
                channels,
                qss: std::slice::from_ref(&qss),
                samples: &mut m.pcm,
                count: total,
                bypass: &mut m.bypass,
                bypass_bits: &mut m.bits,
                lengths: None,
                blocks: 0,
                prefix: 0,
            })
        };
        if rc < 0 {
            return Err(());
        }
        *truncated += u32::from(rc != 0);
        for bypass in &mut m.bypass {
            *bypass = (u32::from(*bypass) << (8 - m.bits)) as u8;
        }
    } else {
        m.count = 0;
        m.bits = 0;
        m.selected_matrix = false;
    }
    if m.joint_search {
        let mut mono = vec![0; total];
        for ch in 0..channels {
            let limited = input.au_samples() == 160;
            for (sample, frame) in mono.iter_mut().zip(m.pcm.chunks_exact(channels)) {
                *sample = frame[ch];
            }
            m.pool[ch].rng = m.search_rng;
            let mut rc = mlp_search_interval(
                Some(&mut m.pool[ch]),
                &mono,
                &m.lengths,
                crate::analysis::search::SearchOptions {
                    maximum_a: if limited { 4 } else { 8 },
                    maximum_b: if !limited && total >= 40 { 4 } else { 0 },
                    lag: 32,
                    perturbations: 16,
                },
                if limited { 2 } else { 0 },
                &mut m.prediction[ch],
            );
            m.search_rng = m.pool[ch].rng;
            if rc != 0 {
                rc = mlp_search_interval(
                    None,
                    &mono,
                    &m.lengths,
                    crate::analysis::search::SearchOptions {
                        maximum_a: if limited { 4 } else { 8 },
                        maximum_b: 0,
                        lag: 8,
                        perturbations: 16,
                    },
                    if limited { 2 } else { 0 },
                    &mut m.prediction[ch],
                );
            }
            m.search_ready[ch] = rc == 0;
        }
    }
    Ok(())
}

#[test]
fn matrix_interval_tail_and_lossless() {
    let mut m = MatrixInterval {
        original_scale: true,
        enable_matrix: true,
        joint_search: true,
        search_rng: 1,
        ..Default::default()
    };
    crate::analysis::decorrelation::mlp_matrix_analysis_init(&mut m.analysis, 2).unwrap();
    for pool in &mut m.pool {
        crate::analysis::search::mlp_search_pool_init(pool, 1);
    }
    let pcm: Vec<_> = (0..85).flat_map(|i| [i * 256, i * 256]).collect();
    let mut input = MatrixInput {
        pcm: &pcm,
        channels: 2,
        bits: 16,
        au_samples: 80,
        position: 0,
    };
    let mut truncated = 0;
    prepare_matrix(&mut input, &mut m, 16, true, &mut truncated).unwrap();
    assert_eq!(m.lengths, [77, 8]);
    assert_eq!(input.position(), 85);
    assert_eq!(m.checks[0], lossless(0, &pcm, 77, 2));
    assert_eq!(m.checks[1], lossless(0, &pcm[154..], 8, 2));
    assert!(m.search_ready[..2].iter().all(|&ready| !ready));
}
