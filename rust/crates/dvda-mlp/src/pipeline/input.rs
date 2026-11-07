use std::borrow::Cow;

pub(crate) struct PcmChunk<'a> {
    pub samples: Cow<'a, [i32]>,
    pub position: usize,
    pub frames: usize,
}

pub(crate) trait PcmInput {
    fn channels(&self) -> usize;
    fn bits(&self) -> u32;
    fn au_samples(&self) -> usize;
    fn frames(&self) -> usize;
    fn position(&self) -> usize;
    fn read_chunk(&mut self, capacity: usize) -> Result<PcmChunk<'_>, ()>;
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
    pub pcm: Vec<i32>,
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
    #[allow(
        clippy::cast_sign_loss,
        reason = "Input parity masks the low 24 bits of signed PCM without changing its two's-complement representation"
    )]
    fn read_chunk(&mut self, capacity: usize) -> Result<PcmChunk<'_>, ()> {
        self.failed = true;
        if capacity == 0 || capacity > 160 || self.position > self.frames {
            return Err(());
        }
        let count = capacity.min(self.frames - self.position);
        let actual = count.min(self.source_frames.saturating_sub(self.position));
        let pcm = &mut self.pcm;
        pcm.clear();
        pcm.resize(count * self.channels, 0);
        let mut done = 0;
        while done < actual {
            let got = (self.callback)(
                &mut pcm[done * self.channels..actual * self.channels],
                actual - done,
            )?;
            if got == 0 {
                if self.source_frames != usize::MAX {
                    return Err(());
                }
                let actual_frames = self.position + done;
                let Some(rounded) = actual_frames.checked_next_multiple_of(self.au_samples) else {
                    return Err(());
                };
                self.source_frames = actual_frames;
                self.frames = rounded;
                pcm[done * self.channels..].fill(0);
                if actual_frames == rounded && done == 0 {
                    self.failed = false;
                    pcm.clear();
                    return Ok(PcmChunk {
                        samples: Cow::Borrowed(pcm),
                        position: self.position,
                        frames: self.frames,
                    });
                }
                self.position = rounded;
                self.failed = false;
                return Ok(PcmChunk {
                    samples: Cow::Borrowed(pcm),
                    position: self.position,
                    frames: self.frames,
                });
            }
            if got > actual - done {
                return Err(());
            }
            done += got;
        }
        let mask = (1u32 << (24 - self.bits)) - 1;
        if pcm[..actual * self.channels]
            .iter()
            .any(|&v| !(-8_388_608..=8_388_607).contains(&v) || v as u32 & mask != 0)
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
        Ok(PcmChunk {
            samples: Cow::Borrowed(pcm),
            position: self.position,
            frames: self.frames,
        })
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
    fn read_chunk(&mut self, capacity: usize) -> Result<PcmChunk<'_>, ()> {
        if !self.pcm.len().is_multiple_of(self.channels) || self.position > self.frames() {
            return Err(());
        }
        let count = capacity.min(self.frames() - self.position);
        let start = self.position * self.channels;
        let end = (self.position + count) * self.channels;
        self.position += count;
        let position = self.position;
        let frames = self.frames();
        Ok(PcmChunk {
            samples: Cow::Borrowed(&self.pcm[start..end]),
            position,
            frames,
        })
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
        pcm: Vec::new(),
        callback: |buffer: &mut [i32], capacity| {
            assert_eq!(capacity, 2 - calls);
            buffer[..2].copy_from_slice(&samples[next..next + 2]);
            next += 2;
            calls += 1;
            Ok(1)
        },
        stamp_refresh: || Ok(None),
    };
    let pcm = input.read_chunk(3).unwrap();
    let allocation = pcm.samples.as_ptr();
    assert_eq!(pcm.samples.as_ref(), [512, 256, 1024, 768, 0, 0]);
    assert_eq!(input.position, 3);
    assert!(!input.failed);
    let pcm = input.read_chunk(1).unwrap();
    assert!(pcm.samples.is_empty());
    assert_eq!(pcm.samples.as_ptr(), allocation);
    assert_eq!(calls, 2);
}

#[test]
fn matrix_input_borrows_the_original_slice() {
    let samples = [1, 2, 3, 4, 5, 6];
    let mut input = MatrixInput {
        pcm: &samples,
        channels: 2,
        bits: 16,
        au_samples: 40,
        position: 0,
    };
    let chunk = input.read_chunk(2).unwrap();
    assert!(matches!(&chunk.samples, Cow::Borrowed(_)));
    assert_eq!(chunk.samples.as_ptr(), samples.as_ptr());
    assert_eq!(chunk.samples.as_ref(), [1, 2, 3, 4]);
    assert_eq!(chunk.position, 2);
    assert_eq!(chunk.frames, 3);
}

#[test]
fn callback_input_failure_does_not_advance_position() {
    for samples in [[256, 1], [256, 8_388_608]] {
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
            pcm: Vec::new(),
            callback: |buffer: &mut [i32], _| {
                buffer.copy_from_slice(&samples);
                Ok(1)
            },
            stamp_refresh: || Ok(None),
        };
        assert!(input.read_chunk(1).is_err());
        assert_eq!(input.position, 0);
        assert!(input.failed);
    }
}
