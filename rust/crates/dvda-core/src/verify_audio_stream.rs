use dvda_bridges::{PcmFormat, PcmSink};
use dvda_native::media::{Callbacks, Operation, OutputFormat, Request};
use std::{
    collections::VecDeque,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    time::Duration,
};

const CHUNK_FRAMES: usize = 8192;
const QUEUE_EVENTS: usize = 8;

/// Estimated PCM allocations for one comparison, with independent recycling
/// pools for the source and decoded sides. The shared event queue is accounted
/// separately because either side can occupy its data events.
pub(crate) struct PcmWorkingBytes {
    pub(crate) source: usize,
    pub(crate) decoded: usize,
    pub(crate) shared: usize,
}

impl PcmWorkingBytes {
    pub(crate) const fn total(&self) -> usize {
        self.source
            .saturating_add(self.decoded)
            .saturating_add(self.shared)
    }
}

pub(crate) const fn pcm_working_bytes(channels: usize) -> PcmWorkingBytes {
    // Each side has its own recycle queue, a producer block, and comparison
    // fragments. Data events in the shared queue are additional allocations.
    const SIDE_BUFFERS: usize = QUEUE_EVENTS + 2;
    let chunk = CHUNK_FRAMES
        .saturating_mul(channels)
        .saturating_mul(size_of::<i32>());
    PcmWorkingBytes {
        source: chunk.saturating_mul(SIDE_BUFFERS),
        decoded: chunk.saturating_mul(SIDE_BUFFERS),
        shared: chunk.saturating_mul(QUEUE_EVENTS),
    }
}

#[derive(Clone, Copy)]
enum Side {
    Source,
    Decoded,
}
impl Side {
    fn index(self) -> usize {
        match self {
            Self::Source => 0,
            Self::Decoded => 1,
        }
    }
}

struct Chunk {
    samples: Vec<i32>,
    offset: usize,
}

enum Event {
    Data(Side, Vec<i32>),
    Message(i32, String),
    Progress(u64, u64),
    Finished(Side, Result<u64, String>),
}

struct WorkerCallbacks {
    stop: Arc<AtomicBool>,
    send: SyncSender<Event>,
}
impl WorkerCallbacks {
    fn send(&self, event: Event) {
        if self.send.send(event).is_err() {
            self.stop.store(true, Ordering::Release);
        }
    }
}
impl Callbacks for WorkerCallbacks {
    fn emit(&mut self, stream: i32, text: &str) {
        self.send(Event::Message(stream, text.into()));
    }
    fn cancelled(&mut self) -> bool {
        self.stop.load(Ordering::Acquire)
    }
    fn progress(&mut self, completed: u64, total: u64) {
        self.send(Event::Progress(completed, total));
    }
}

struct DecoderSink {
    side: Side,
    channels: usize,
    rate: u32,
    bits: u32,
    started: bool,
    frames: u64,
    send: SyncSender<Event>,
    recycled: mpsc::Receiver<Vec<i32>>,
}
impl DecoderSink {
    fn new(
        side: Side,
        channels: u32,
        rate: u32,
        bits: u32,
        send: SyncSender<Event>,
        recycled: mpsc::Receiver<Vec<i32>>,
    ) -> Self {
        Self {
            side,
            channels: channels as usize,
            rate,
            bits,
            started: false,
            frames: 0,
            send,
            recycled,
        }
    }
}
impl PcmSink for DecoderSink {
    fn start(&mut self, format: PcmFormat) -> Result<(), i32> {
        if self.started
            || format.channels as usize != self.channels
            || format.sample_rate != self.rate
            || format.bits != self.bits
        {
            return Err(dvda_bridges::ffmpeg::EINVAL);
        }
        self.started = true;
        Ok(())
    }

    fn write(&mut self, pcm: &[i32]) -> Result<(), i32> {
        if !self.started || !pcm.len().is_multiple_of(self.channels) {
            return Err(dvda_bridges::ffmpeg::EINVAL);
        }
        let chunk_samples = CHUNK_FRAMES * self.channels;
        for chunk in pcm.chunks(chunk_samples) {
            let mut samples = self
                .recycled
                .try_recv()
                .unwrap_or_else(|_| Vec::with_capacity(chunk_samples));
            samples.clear();
            samples.extend_from_slice(chunk);
            if self.bits != 16 {
                for sample in &mut samples {
                    *sample >>= 8;
                }
            }
            self.frames = self
                .frames
                .checked_add((chunk.len() / self.channels) as u64)
                .ok_or(dvda_bridges::ffmpeg::EIO)?;
            self.send
                .send(Event::Data(self.side, samples))
                .map_err(|_| dvda_bridges::ffmpeg::EIO)?;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), i32> {
        if self.started && self.frames != 0 {
            Ok(())
        } else {
            Err(dvda_bridges::ffmpeg::EINVAL)
        }
    }
}

struct Comparison {
    chunks: [VecDeque<Chunk>; 2],
    positions: [u64; 2],
    received: [u64; 2],
    decoded_tail: VecDeque<i32>,
    tail_samples: usize,
    mismatch: Option<(u8, u64)>,
}
impl Comparison {
    fn new(channels: u32, max_padding_frames: u32) -> Self {
        Self {
            chunks: [VecDeque::new(), VecDeque::new()],
            positions: [0, 0],
            received: [0, 0],
            decoded_tail: VecDeque::new(),
            tail_samples: max_padding_frames as usize * channels as usize,
            mismatch: None,
        }
    }

    fn push(&mut self, side: Side, samples: Vec<i32>, recycled: &[SyncSender<Vec<i32>>; 2]) {
        let index = side.index();
        self.received[index] += samples.len() as u64;
        if matches!(side, Side::Decoded) && self.tail_samples != 0 {
            for &sample in &samples {
                if self.decoded_tail.len() == self.tail_samples {
                    self.decoded_tail.pop_front();
                }
                self.decoded_tail.push_back(sample);
            }
        }
        self.chunks[index].push_back(Chunk { samples, offset: 0 });
        self.compare(recycled);
    }

    fn compare(&mut self, recycled: &[SyncSender<Vec<i32>>; 2]) {
        if self.mismatch.is_some() {
            return;
        }
        while let (Some(left), Some(right)) = (self.chunks[0].front(), self.chunks[1].front()) {
            let available =
                (left.samples.len() - left.offset).min(right.samples.len() - right.offset);
            let lhs = &left.samples[left.offset..left.offset + available];
            let rhs = &right.samples[right.offset..right.offset + available];
            if let Some(index) = lhs.iter().zip(rhs).position(|(a, b)| a != b) {
                self.mismatch = Some((2, (self.positions[0] + index as u64) * 3));
                return;
            }
            self.positions[0] += available as u64;
            self.positions[1] += available as u64;
            let left_done = left.offset + available == left.samples.len();
            let right_done = right.offset + available == right.samples.len();
            if left_done {
                let consumed = self.chunks[0].pop_front().unwrap();
                let _ = recycled[0].try_send(consumed.samples);
            } else {
                self.chunks[0].front_mut().unwrap().offset += available;
            }
            if right_done {
                let consumed = self.chunks[1].pop_front().unwrap();
                let _ = recycled[1].try_send(consumed.samples);
            } else {
                self.chunks[1].front_mut().unwrap().offset += available;
            }
            if available == 0 {
                break;
            }
        }
    }

    fn finish(&self, channels: u32, max_padding_frames: u32) -> Result<(), String> {
        let source_frames = self.received[0] / u64::from(channels);
        let decoded_frames = self.received[1] / u64::from(channels);
        let source_bytes = source_frames * u64::from(channels) * 3;
        let decoded_bytes = decoded_frames * u64::from(channels) * 3;
        let extra = decoded_frames.saturating_sub(source_frames);
        let padding = max_padding_frames != 0
            && extra != 0
            && extra <= u64::from(max_padding_frames)
            && decoded_frames >= source_frames;
        if let Some((reason, offset)) = self.mismatch {
            return Err(mismatch(reason, offset, source_bytes, decoded_bytes));
        }
        if source_frames != decoded_frames && !padding {
            return Err(mismatch(1, 0, source_bytes, decoded_bytes));
        }
        if padding {
            let tail = usize::try_from(extra)
                .expect("padding frames are bounded by u32 max_padding_frames")
                * channels as usize;
            let start = self.decoded_tail.len() - tail;
            if let Some(index) = self.decoded_tail.iter().skip(start).position(|&v| v != 0) {
                return Err(mismatch(
                    3,
                    source_bytes + index as u64 * 3,
                    source_bytes,
                    decoded_bytes,
                ));
            }
        }
        Ok(())
    }
}

fn mismatch(reason: u8, offset: u64, source: u64, decoded: u64) -> String {
    format!("PCM 不一致：原因 {reason}，偏移 {offset}，源 {source} 字节，解码 {decoded} 字节。")
}

struct DecodeJob<'a> {
    side: Side,
    library: &'a Path,
    input: &'a Path,
    rate: u32,
    bits: u32,
    channels: u32,
}
pub(crate) struct CompareJob<'a> {
    pub library: &'a Path,
    pub source: &'a Path,
    pub decoded: &'a Path,
    pub rate: u32,
    pub bits: u32,
    pub channels: u32,
    pub builtin: bool,
}
fn decode(
    job: &DecodeJob<'_>,
    stop: Arc<AtomicBool>,
    send: &SyncSender<Event>,
    recycled: mpsc::Receiver<Vec<i32>>,
) {
    let DecodeJob {
        side,
        library,
        input,
        rate,
        bits,
        channels,
    } = *job;
    let mut callbacks = WorkerCallbacks {
        stop,
        send: send.clone(),
    };
    let mut sink = DecoderSink::new(side, channels, rate, bits, send.clone(), recycled);
    let result = (|| {
        let media = crate::native_components::media(library);
        let status = media
            .run_pcm(
                &Request {
                    operation: Operation::Audio,
                    rate,
                    bits,
                    output_format: OutputFormat::None,
                    soxr: false,
                    dither: false,
                    compression: 8,
                    cover: false,
                    input: input.to_string_lossy().into_owned(),
                    output: None,
                    tags: Vec::new(),
                },
                &mut callbacks,
                &mut sink,
            )
            .map_err(|error| error.to_string())?;
        if status != 0 {
            return Err(format!("无法解码音频进行 PCM 校验：{}", input.display()));
        }
        Ok(sink.frames)
    })();
    let _ = send.send(Event::Finished(side, result));
}

pub(crate) fn compare(job: &CompareJob<'_>, caller: &mut dyn Callbacks) -> Result<(), String> {
    let CompareJob {
        library,
        source,
        decoded,
        rate,
        bits,
        channels,
        builtin,
    } = *job;
    let stop = Arc::new(AtomicBool::new(false));
    let max_padding_frames = if builtin { (rate - 1) / 1000 } else { 0 };
    let mut comparison = Comparison::new(channels, max_padding_frames);
    let (send, receive) = mpsc::sync_channel(QUEUE_EVENTS);
    let (source_recycled, source_recycle) = mpsc::sync_channel(QUEUE_EVENTS);
    let (decoded_recycled, decoded_recycle) = mpsc::sync_channel(QUEUE_EVENTS);
    let recycled = [source_recycled, decoded_recycled];
    let recycle = [source_recycle, decoded_recycle];
    let mut errors = [None, None];
    let mut mismatch_stopped = false;
    std::thread::scope(|scope| {
        for ((side, input, actual_bits), recycle) in
            [(Side::Source, source, bits), (Side::Decoded, decoded, 24)]
                .into_iter()
                .zip(recycle)
        {
            let stop = Arc::clone(&stop);
            let send = send.clone();
            scope.spawn(move || {
                decode(
                    &DecodeJob {
                        side,
                        library,
                        input,
                        rate,
                        bits: actual_bits,
                        channels,
                    },
                    stop,
                    &send,
                    recycle,
                );
            });
        }
        drop(send);
        let mut finished = [false; 2];
        while !finished.iter().all(|value| *value) {
            if caller.cancelled() {
                stop.store(true, Ordering::Release);
            }
            match receive.recv_timeout(Duration::from_millis(20)) {
                Ok(Event::Data(side, samples)) => {
                    comparison.push(side, samples, &recycled);
                    if comparison.mismatch.is_some() && !mismatch_stopped {
                        mismatch_stopped = true;
                        stop.store(true, Ordering::Release);
                    }
                }
                Ok(Event::Message(stream, text)) => caller.emit(stream, &text),
                Ok(Event::Progress(completed, total)) => caller.progress(completed, total),
                Ok(Event::Finished(side, result)) => {
                    let index = side.index();
                    finished[index] = true;
                    if let Err(error) = result {
                        errors[index] = Some(error);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
    if caller.cancelled() {
        return Err("成品验证已取消。".into());
    }
    if !mismatch_stopped && let Some(error) = errors.into_iter().flatten().next() {
        return Err(error);
    }
    comparison.finish(channels, max_padding_frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    type RecycleChannels = ([SyncSender<Vec<i32>>; 2], [mpsc::Receiver<Vec<i32>>; 2]);

    fn recycle_channels() -> RecycleChannels {
        let (source_send, source_receive) = mpsc::sync_channel(QUEUE_EVENTS);
        let (decoded_send, decoded_receive) = mpsc::sync_channel(QUEUE_EVENTS);
        (
            [source_send, decoded_send],
            [source_receive, decoded_receive],
        )
    }

    #[test]
    fn pcm_budget_separates_source_decoded_and_shared_queues() {
        let chunk = CHUNK_FRAMES * 6 * size_of::<i32>();
        let working = pcm_working_bytes(6);
        assert_eq!(working.source, chunk * (QUEUE_EVENTS + 2));
        assert_eq!(working.decoded, chunk * (QUEUE_EVENTS + 2));
        assert_eq!(working.shared, chunk * QUEUE_EVENTS);
        assert_eq!(working.total(), chunk * (QUEUE_EVENTS * 3 + 4));
        assert_eq!(pcm_working_bytes(usize::MAX).total(), usize::MAX);
    }

    #[test]
    fn streaming_comparison_handles_fragmentation_padding_and_mismatch_offsets() {
        let (recycled, _recycle) = recycle_channels();
        let mut comparison = Comparison::new(2, 2);
        comparison.push(Side::Source, vec![1], &recycled);
        comparison.push(Side::Source, vec![2, 3], &recycled);
        comparison.push(Side::Decoded, vec![1], &recycled);
        comparison.push(Side::Decoded, vec![2, 3, 0, 0], &recycled);
        comparison.finish(2, 2).unwrap();
        assert_eq!(comparison.positions, [3, 3]);

        let mut comparison = Comparison::new(2, 2);
        comparison.push(Side::Source, vec![1, 2, 3, 4], &recycled);
        comparison.push(Side::Decoded, vec![1, 2, 9, 4], &recycled);
        assert_eq!(comparison.finish(2, 2).unwrap_err(), mismatch(2, 6, 12, 12));

        let mut comparison = Comparison::new(2, 2);
        comparison.push(Side::Source, vec![1, 2], &recycled);
        comparison.push(Side::Decoded, vec![1, 2, 0, 1], &recycled);
        assert_eq!(comparison.finish(2, 2).unwrap_err(), mismatch(3, 9, 6, 12));
    }

    #[test]
    fn streaming_comparison_returns_consumed_fragments_without_merging() {
        let (recycled, recycle) = recycle_channels();
        let mut comparison = Comparison::new(2, 0);
        let source = vec![1, 2];
        let source_allocation = source.as_ptr();
        comparison.push(Side::Source, source, &recycled);
        comparison.push(Side::Decoded, vec![1, 2], &recycled);
        let returned = recycle[0].try_recv().unwrap();
        assert_eq!(returned.as_ptr(), source_allocation);
        assert!(recycle[1].try_recv().is_ok());

        comparison.push(Side::Source, vec![3], &recycled);
        let appended = vec![4];
        let appended_allocation = appended.as_ptr();
        comparison.push(Side::Source, appended, &recycled);
        assert!(recycle[0].try_recv().is_err());
        comparison.push(Side::Decoded, vec![3, 4], &recycled);
        assert_eq!(recycle[0].try_recv().unwrap(), [3]);
        let returned = recycle[0].try_recv().unwrap();
        assert_eq!(returned, [4]);
        assert_eq!(returned.as_ptr(), appended_allocation);
        assert!(recycle[1].try_recv().is_ok());
        comparison.finish(2, 0).unwrap();
    }

    #[test]
    fn decoder_sink_reuses_allocation_and_preserves_sample_conversion() {
        let (send, events) = mpsc::sync_channel(1);
        let (recycled, recycle) = mpsc::sync_channel(1);
        let mut sink = DecoderSink::new(Side::Source, 2, 48_000, 24, send, recycle);
        sink.start(PcmFormat {
            sample_rate: 48_000,
            bits: 24,
            channels: 2,
            channel_mask: 3,
        })
        .unwrap();
        let spare = Vec::with_capacity(CHUNK_FRAMES * 2);
        let allocation = spare.as_ptr();
        recycled.send(spare).unwrap();
        sink.write(&[256, -256, 512, -512]).unwrap();
        let Event::Data(Side::Source, samples) = events.recv().unwrap() else {
            panic!("expected PCM data");
        };
        assert_eq!(samples, [1, -1, 2, -2]);
        assert_eq!(samples.as_ptr(), allocation);
        recycled.send(samples).unwrap();
        sink.write(&[768, -768]).unwrap();
        let Event::Data(Side::Source, samples) = events.recv().unwrap() else {
            panic!("expected recycled PCM data");
        };
        assert_eq!(samples, [3, -3]);
        assert_eq!(samples.as_ptr(), allocation);
        sink.finish().unwrap();
    }
}
