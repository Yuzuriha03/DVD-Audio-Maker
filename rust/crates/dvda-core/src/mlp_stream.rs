use crate::media::{Failure, Outcome, Temporary, Timed, temporary_path};
use dvda_mlp::{Config, direct::StreamIo, format::Profile, metadata::Metadata};
use dvda_native::{
    files,
    media::{Callbacks, Operation, OutputFormat, Request},
};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::windows::fs::OpenOptionsExt,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    thread::JoinHandle,
};

const PCM_CHUNK_FRAMES: usize = 8192;
const PCM_QUEUE_CHUNKS: usize = 4;

struct EncoderInput {
    receiver: Receiver<Vec<i32>>,
    current: Vec<i32>,
    offset: usize,
    channels: usize,
    output: File,
    live_stamp: crate::encoder_stamp::LiveStamp,
    cancelled: Arc<AtomicBool>,
    failure: Option<String>,
}

impl EncoderInput {
    fn fail(&mut self, error: impl std::fmt::Display, status: i32) -> i32 {
        self.failure = Some(error.to_string());
        status
    }
}

impl StreamIo for EncoderInput {
    fn read(&mut self, group: usize, pcm: &mut [i32]) -> Result<usize, i32> {
        if group != 0 || !pcm.len().is_multiple_of(self.channels) {
            return Err(self.fail("Invalid streaming PCM request", -3));
        }
        if self.cancelled.load(Ordering::Acquire) {
            return Err(-3);
        }
        let capacity = pcm.len() / self.channels;
        let mut written = 0;
        while written < capacity {
            if self.offset == self.current.len() {
                match self.receiver.recv() {
                    Ok(chunk) => {
                        self.current = chunk;
                        self.offset = 0;
                        if self.current.is_empty()
                            || !self.current.len().is_multiple_of(self.channels)
                        {
                            return Err(self.fail("Invalid streaming PCM block", -3));
                        }
                    }
                    Err(_) => break,
                }
            }
            let available = (self.current.len() - self.offset) / self.channels;
            let count = (capacity - written).min(available);
            let source = self.offset;
            let destination = written * self.channels;
            pcm[destination..destination + count * self.channels]
                .copy_from_slice(&self.current[source..source + count * self.channels]);
            self.offset += count * self.channels;
            written += count;
        }
        Ok(written)
    }

    fn write(&mut self, access_unit: &[u8]) -> Result<(), i32> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(-4);
        }
        self.output.write_all(access_unit).map_err(|error| {
            self.failure = Some(error.to_string());
            -4
        })
    }

    fn refresh_stamp(&mut self) -> Result<Option<Metadata>, i32> {
        self.live_stamp
            .refresh()
            .map(|stamp| {
                stamp.map(|stamp| Metadata {
                    start: stamp.start,
                    packet: stamp.packet,
                    valid_bits: stamp.valid_bits,
                })
            })
            .map_err(|error| self.fail(error.message, -5))
    }

    fn cancelled(&mut self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

struct Pipeline {
    sender: Option<SyncSender<Vec<i32>>>,
    worker: Option<JoinHandle<Result<u64, Failure>>>,
    output: Option<File>,
    channels: usize,
    bits: u32,
    frames: u64,
    cancelled: Arc<AtomicBool>,
    failure: Option<Failure>,
}

impl Pipeline {
    fn new(output: File) -> Self {
        Self {
            sender: None,
            worker: None,
            output: Some(output),
            channels: 0,
            bits: 0,
            frames: 0,
            cancelled: Arc::new(AtomicBool::new(false)),
            failure: None,
        }
    }

    fn join(&mut self) -> Result<u64, Failure> {
        self.sender.take();
        let Some(worker) = self.worker.take() else {
            return Ok(self.frames);
        };
        worker
            .join()
            .unwrap_or_else(|_| Err(Failure::new("InvalidOperation", "MLP 流式编码线程崩溃。")))
    }

    fn finish(&mut self) -> Result<(), Failure> {
        let encoded = self.join()?;
        if self.frames == 0 || encoded != self.frames {
            return Err(Failure::new(
                "InvalidData",
                "MLP 实际输入帧数与解码帧数不一致。",
            ));
        }
        Ok(())
    }

    fn fail(&mut self, kind: &'static str, message: impl Into<String>) -> i32 {
        self.cancelled.store(true, Ordering::Release);
        self.failure = Some(Failure::new(kind, &message.into()));
        dvda_bridges::ffmpeg::EIO
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.join();
    }
}

impl dvda_bridges::PcmSink for Pipeline {
    fn start(&mut self, format: dvda_bridges::PcmFormat) -> Result<(), i32> {
        if self.worker.is_some()
            || self.channels != 0
            || !matches!(
                format.sample_rate,
                44100 | 48000 | 88200 | 96000 | 176_400 | 192_000
            )
            || !matches!(format.bits, 16 | 20 | 24)
            || !(1..=6).contains(&format.channels)
            || (format.sample_rate > 96000 && format.channels > 2)
        {
            return Err(self.fail("InvalidData", "MLP 流式输入格式无效。"));
        }
        let mut mask = format.channel_mask;
        if mask & 0x600 != 0 {
            if mask & (0x30 | 0x100) != 0 {
                return Err(self.fail(
                    "InvalidData",
                    "不能把同时存在的侧置、后置或后中置声道合并为 DVD 环绕声道。",
                ));
            }
            mask = (mask & !0x600) | ((mask & 0x600) >> 5);
        }
        let assignment = match crate::encoder::assignment(mask) {
            Ok(assignment) => assignment,
            Err(error) => return Err(self.fail(error.kind, error.message)),
        };
        let stamp = match crate::encoder_stamp::current() {
            Ok(stamp) => stamp,
            Err(error) => return Err(self.fail(error.kind, error.message)),
        };
        let live_stamp = match crate::encoder_stamp::LiveStamp::new(&stamp) {
            Ok(stamp) => stamp,
            Err(error) => return Err(self.fail(error.kind, error.message)),
        };
        let config = Config {
            sample_rate: format.sample_rate,
            bits: format.bits as u8,
            channels: format.channels as u8,
            restart_interval: 8,
            metadata: Vec::new(),
        };
        let profile = Profile {
            assignment: assignment as usize,
            group2_bits: config.bits,
            group2_rate: config.sample_rate,
        };
        let metadata = [Metadata {
            start: stamp.start,
            packet: stamp.packet,
            valid_bits: stamp.valid_bits,
        }];
        let channels = format.channels as usize;
        let (sender, receiver) = sync_channel(PCM_QUEUE_CHUNKS);
        let cancelled = Arc::clone(&self.cancelled);
        let output = match self.output.take() {
            Some(output) => output,
            None => return Err(self.fail("InvalidOperation", "MLP 输出已关闭。")),
        };
        self.worker = Some(std::thread::spawn(move || {
            let mut input = EncoderInput {
                receiver,
                current: Vec::new(),
                offset: 0,
                channels,
                output,
                live_stamp,
                cancelled,
                failure: None,
            };
            let result = dvda_mlp::direct::encode_stream_with_stamp_refresh_unknown(
                &config, &profile, &metadata, &mut input,
            );
            input.output.flush()?;
            let length = input.output.metadata()?.len();
            if let Some(message) = input.failure {
                return Err(Failure::new("InvalidOperation", &message));
            }
            if result.status != 0
                || result.input_frames == 0
                || result.output_bytes != length
                || length == 0
            {
                let error: Vec<_> = result
                    .error
                    .iter()
                    .take_while(|&&byte| byte != 0)
                    .map(|&byte| byte as u8)
                    .collect();
                return Err(Failure::new(
                    "InvalidOperation",
                    &format!(
                        "MLP 编码器编码失败（{}）：{}",
                        result.status,
                        String::from_utf8_lossy(&error)
                    ),
                ));
            }
            Ok(result.input_frames)
        }));
        self.channels = channels;
        self.bits = format.bits;
        self.sender = Some(sender);
        Ok(())
    }

    fn write(&mut self, pcm: &[i32]) -> Result<(), i32> {
        if self.channels == 0 || self.failure.is_some() || !pcm.len().is_multiple_of(self.channels)
        {
            return Err(self.fail("InvalidData", "MLP 流式 PCM 块无效。"));
        }
        let precision_mask = (1u32 << (24 - self.bits)) - 1;
        let chunk_samples = PCM_CHUNK_FRAMES * self.channels;
        for chunk in pcm.chunks(chunk_samples) {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(dvda_bridges::ffmpeg::EXIT);
            }
            let mut owned = chunk.to_vec();
            if self.bits != 16 {
                for sample in &mut owned {
                    *sample >>= 8;
                }
            }
            if let Some((index, &sample)) = owned.iter().enumerate().find(|(_, sample)| {
                !(-8_388_608..=8_388_607).contains(*sample) || **sample as u32 & precision_mask != 0
            }) {
                return Err(self.fail(
                    "InvalidData",
                    format!(
                        "媒体转换 PCM 精度无效：{} 位，样本 {}，值 {}。",
                        self.bits, index, sample
                    ),
                ));
            }
            self.frames = match self
                .frames
                .checked_add((chunk.len() / self.channels) as u64)
            {
                Some(frames) => frames,
                None => return Err(self.fail("Overflow", "PCM 帧数溢出。")),
            };
            let Some(sender) = &self.sender else {
                return Err(self.fail("InvalidOperation", "MLP 流式输入已关闭。"));
            };
            if sender.send(owned).is_err() {
                self.sender.take();
                let error = self.join().err().unwrap_or_else(|| {
                    Failure::new("InvalidOperation", "MLP 编码器已停止接收 PCM。")
                });
                return Err(self.fail(error.kind, error.message));
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), i32> {
        match self.finish() {
            Ok(()) => Ok(()),
            Err(error) => Err(self.fail(error.kind, error.message)),
        }
    }
}

pub(crate) fn execute(
    library: &Path,
    input: &str,
    destination: &Path,
    rate: i32,
    bits: i32,
    dither: bool,
    caller: &mut dyn Callbacks,
) -> Outcome {
    let mut callbacks = Timed::new(caller, None);
    let mut temporary = Temporary(None);
    let result = (|| -> Result<i32, Failure> {
        callbacks.check()?;
        let input = std::path::absolute(input)?;
        let destination = std::path::absolute(destination)?;
        if files::ordinal_ignore_case(&input.to_string_lossy(), &destination.to_string_lossy()) {
            return Err(Failure::new("Io", "音频输入输出不能是同一文件。"));
        }
        if destination.is_file() {
            return Err(Failure::new("Io", "拒绝覆盖已有的 MLP 输出文件。"));
        }
        let path = temporary_path(&destination);
        let output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&path)?;
        temporary.0 = Some(path.clone());
        let mut pipeline = Pipeline::new(output);
        let status = crate::native_components::media(library)?.run_pcm(
            &Request {
                operation: Operation::Audio,
                rate: rate as u32,
                bits: bits as u32,
                output_format: OutputFormat::None,
                soxr: false,
                dither,
                compression: 8,
                cover: false,
                input: input.to_string_lossy().into_owned(),
                output: None,
                tags: Vec::new(),
            },
            &mut callbacks,
            &mut pipeline,
        )?;
        callbacks.check()?;
        if let Some(error) = pipeline.failure.take() {
            return Err(error);
        }
        if status != 0 {
            return Ok(1);
        }
        pipeline.finish()?;
        files::move_file(&path, &destination, false)?;
        temporary.0 = None;
        Ok(0)
    })();
    let result = temporary.clean().map_err(Failure::from).and(result);
    match result {
        Ok(code) => Outcome {
            exit_code: Some(code),
            failure: None,
        },
        Err(error) => Outcome {
            exit_code: None,
            failure: Some(error),
        },
    }
}
