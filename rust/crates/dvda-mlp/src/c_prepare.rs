use crate::c_analysis::{
    MlpMatrixAnalysis, mlp_matrix_analysis_add, mlp_matrix_analysis_reset, mlp_matrix_select,
};
use crate::matrix::{MlpMatrixPrimitive, mlp_matrix_apply_blocks, mlp_matrix_apply_interval};
use crate::scale::{MlpScalePlan, mlp_scale_analyze};
use crate::search::{MlpSearchPlan, MlpSearchPool, mlp_search_interval};

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

pub(crate) trait PcmInput {
    fn channels(&self) -> usize;
    fn bits(&self) -> u32;
    fn au_samples(&self) -> usize;
    fn frames(&self) -> usize;
    fn position(&self) -> usize;
    fn read(&mut self, capacity: usize) -> Result<Vec<i32>, ()>;
}

pub(crate) struct CallbackInput<F> {
    pub channels: usize,
    pub bits: u32,
    pub depths: [u32; 6],
    pub order: [usize; 6],
    pub au_samples: usize,
    pub frames: usize,
    pub source_frames: usize,
    pub position: usize,
    pub failed: bool,
    pub callback: F,
}
impl<F: FnMut(&mut [i32], usize) -> Result<usize, ()>> PcmInput for CallbackInput<F> {
    fn channels(&self) -> usize {
        self.channels
    }
    fn bits(&self) -> u32 {
        self.bits
    }
    fn au_samples(&self) -> usize {
        self.au_samples
    }
    fn frames(&self) -> usize {
        self.frames
    }
    fn position(&self) -> usize {
        self.position
    }
    fn read(&mut self, capacity: usize) -> Result<Vec<i32>, ()> {
        self.failed = true;
        if capacity == 0 || capacity > 160 || self.position > self.frames {
            return Err(());
        }
        let count = capacity.min(self.frames - self.position);
        let actual = count.min(self.source_frames.saturating_sub(self.position));
        let mut pcm = vec![0; count * self.channels];
        let mut done = 0;
        while done < actual {
            let got = (self.callback)(
                &mut pcm[done * self.channels..actual * self.channels],
                actual - done,
            )?;
            if got == 0 || got > actual - done {
                return Err(());
            }
            done += got;
        }
        let mask = (1u32 << (24 - self.bits)) - 1;
        if pcm[..actual * self.channels]
            .iter()
            .any(|&v| !(-8388608..=8388607).contains(&v) || v as u32 & mask != 0)
        {
            return Err(());
        }
        for frame in pcm.chunks_exact_mut(self.channels) {
            let mut original = [0; 6];
            original[..self.channels].copy_from_slice(frame);
            for ch in 0..self.channels {
                frame[ch] = original[self.order[ch]];
                if frame[ch] as u32 & ((1u32 << (24 - self.depths[ch])) - 1) != 0 {
                    return Err(());
                }
            }
        }
        self.position += count;
        self.failed = false;
        Ok(pcm)
    }
}

#[cfg(test)]
pub(crate) struct MatrixInput<'a> {
    pub pcm: &'a [i32],
    pub channels: usize,
    pub bits: u32,
    pub au_samples: usize,
    pub position: usize,
}
#[cfg(test)]
impl PcmInput for MatrixInput<'_> {
    fn channels(&self) -> usize {
        self.channels
    }
    fn bits(&self) -> u32 {
        self.bits
    }
    fn au_samples(&self) -> usize {
        self.au_samples
    }
    fn frames(&self) -> usize {
        self.pcm.len() / self.channels
    }
    fn position(&self) -> usize {
        self.position
    }
    fn read(&mut self, capacity: usize) -> Result<Vec<i32>, ()> {
        if !self.pcm.len().is_multiple_of(self.channels) || self.position > self.frames() {
            return Err(());
        }
        let count = capacity.min(self.frames() - self.position);
        let pcm = self.pcm[self.position * self.channels..(self.position + count) * self.channels]
            .to_vec();
        self.position += count;
        Ok(pcm)
    }
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
            for n in 0..count {
                for ch in 0..channels {
                    summary[i * 6 + ch] |= p[n * channels + ch].unsigned_abs();
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
        for ch in 0..channels {
            if m.scale.shift[ch] > 7 {
                return Err(());
            }
            for n in 0..total {
                m.pcm[n * channels + ch] /= 1i32 << m.scale.shift[ch];
            }
        }
    }
    let mut offset = 0;
    for &length in &m.lengths {
        mlp_matrix_analysis_add(
            &mut m.analysis,
            &m.pcm[offset * channels..],
            length,
            channels,
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
        mlp_matrix_select(
            &m.analysis.covariance,
            &ratio,
            channels,
            0,
            total,
            &qss,
            &scale,
            (allow_bypass || m.original_scale) && input.au_samples() != 160,
            &mut m.matrix,
            &mut m.count,
            &mut m.bits,
        )?;
        m.selected_matrix = m.count != 0;
        let rc = if m.original_scale {
            mlp_matrix_apply_blocks(
                &m.matrix,
                &mut m.count,
                channels,
                &m.scale.qss,
                &m.lengths,
                &mut m.pcm,
                &mut m.bypass,
                &mut m.bits,
            )
        } else {
            mlp_matrix_apply_interval(
                &m.matrix,
                &mut m.count,
                channels,
                &qss,
                &mut m.pcm,
                total,
                &mut m.bypass,
                &mut m.bits,
            )
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
            for (n, sample) in mono.iter_mut().enumerate() {
                *sample = m.pcm[n * channels + ch];
            }
            m.pool[ch].rng = m.search_rng;
            let mut rc = mlp_search_interval(
                Some(&mut m.pool[ch]),
                &mono,
                &m.lengths,
                if limited { 4 } else { 8 },
                if !limited && total >= 40 { 4 } else { 0 },
                32,
                if limited { 2 } else { 0 },
                &mut m.prediction[ch],
            );
            m.search_rng = m.pool[ch].rng;
            if rc != 0 {
                rc = mlp_search_interval(
                    None,
                    &mono,
                    &m.lengths,
                    if limited { 4 } else { 8 },
                    0,
                    8,
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
        original_scale: true,
        enable_matrix: true,
        joint_search: true,
        search_ready: [false; 6],
        prediction: Default::default(),
        pool: Default::default(),
        search_rng: 1,
    };
    crate::c_analysis::mlp_matrix_analysis_init(&mut m.analysis, 2).unwrap();
    for pool in &mut m.pool {
        crate::search::mlp_search_pool_init(pool, 1);
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
