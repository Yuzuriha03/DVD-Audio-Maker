pub(crate) trait PcmInput {
    fn channels(&self) -> usize;
    fn bits(&self) -> u32;
    fn au_samples(&self) -> usize;
    fn frames(&self) -> usize;
    fn position(&self) -> usize;
    fn read(&mut self, capacity: usize) -> Result<Vec<i32>, ()>;
    fn refresh_stamp(&mut self) -> Result<Option<crate::metadata::Metadata>, i32> {
        Ok(None)
    }
}

pub(crate) struct CallbackInput<F, S> {
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
    pub stamp_refresh: S,
}
impl<F, S> PcmInput for CallbackInput<F, S>
where
    F: FnMut(&mut [i32], usize) -> Result<usize, ()>,
    S: FnMut() -> Result<Option<crate::metadata::Metadata>, i32>,
{
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
            for ((sample, &source), &depth) in frame.iter_mut().zip(&self.order).zip(&self.depths) {
                *sample = original[source];
                if *sample as u32 & ((1u32 << (24 - depth)) - 1) != 0 {
                    return Err(());
                }
            }
        }
        self.position += count;
        self.failed = false;
        Ok(pcm)
    }
    fn refresh_stamp(&mut self) -> Result<Option<crate::metadata::Metadata>, i32> {
        (self.stamp_refresh)()
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

#[test]
fn callback_input_reorders_partial_reads_and_pads_tail() {
    let samples = [256, 512, 768, 1024];
    let mut next = 0;
    let mut calls = 0;
    let mut input = CallbackInput {
        channels: 2,
        bits: 16,
        depths: [16; 6],
        order: [1, 0, 2, 3, 4, 5],
        au_samples: 40,
        frames: 3,
        source_frames: 2,
        position: 0,
        failed: false,
        callback: |buffer: &mut [i32], capacity| {
            assert_eq!(capacity, 2 - calls);
            buffer[..2].copy_from_slice(&samples[next..next + 2]);
            next += 2;
            calls += 1;
            Ok(1)
        },
        stamp_refresh: || Ok(None),
    };
    assert_eq!(input.read(3), Ok(vec![512, 256, 1024, 768, 0, 0]));
    assert_eq!(input.position, 3);
    assert!(!input.failed);
    assert_eq!(input.read(1), Ok(Vec::new()));
    assert_eq!(calls, 2);
}

#[test]
fn callback_input_failure_does_not_advance_position() {
    for samples in [[256, 1], [256, 8388608]] {
        let mut input = CallbackInput {
            channels: 2,
            bits: 24,
            depths: [16; 6],
            order: [1, 0, 2, 3, 4, 5],
            au_samples: 40,
            frames: 1,
            source_frames: 1,
            position: 0,
            failed: false,
            callback: |buffer: &mut [i32], _| {
                buffer.copy_from_slice(&samples);
                Ok(1)
            },
            stamp_refresh: || Ok(None),
        };
        assert_eq!(input.read(1), Err(()));
        assert_eq!(input.position, 0);
        assert!(input.failed);
    }
}
