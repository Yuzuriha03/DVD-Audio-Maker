use crate::{Call, ffmpeg::*, media::Request, win};
use std::{ffi::c_void, fs::File, io::Write, ptr};
pub(crate) fn open_error(error: std::io::Error) -> i32 {
    match error.kind() {
        std::io::ErrorKind::NotFound => -2,
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::IsADirectory => -13,
        std::io::ErrorKind::AlreadyExists => -17,
        std::io::ErrorKind::InvalidInput => EINVAL,
        _ => EIO,
    }
}
#[derive(Clone, Copy)]
pub struct PcmFormat {
    pub sample_rate: u32,
    pub bits: u32,
    pub channels: u32,
    pub channel_mask: u32,
}

pub trait PcmSink {
    fn start(&mut self, format: PcmFormat) -> Result<(), i32>;
    fn write(&mut self, pcm: &[i32]) -> Result<(), i32>;
    fn finish(&mut self) -> Result<(), i32>;
}

struct Audio<'a> {
    decoder: *mut Context,
    frame: *mut Frame,
    packet: *mut Packet,
    swr: *mut c_void,
    md5: *mut c_void,
    file: Option<File>,
    wave: Option<dvda_audio::writer::Writer<File>>,
    wave_tags: Vec<(String, String)>,
    rate: i32,
    bits: i32,
    channels: i32,
    format: i32,
    /// Sample format and rate of the first frame, which the resampler was set up for.
    input_format: i32,
    input_rate: i32,
    initialized: bool,
    source_pcm: bool,
    source_bits: u16,
    source_float: bool,
    dither: Option<dvda_audio::dither::Tpdf>,
    samples: i64,
    progress: i64,
    encoder: *mut Context,
    output: *mut Format,
    fifo: *mut c_void,
    input: *mut Format,
    pcm_sink: Option<&'a mut dyn PcmSink>,
}
impl Drop for Audio<'_> {
    fn drop(&mut self) {
        unsafe {
            if !self.output.is_null() {
                avio_closep((*self.output).pb_ptr());
                avformat_free_context(self.output);
            }
            av_audio_fifo_free(self.fifo);
            avcodec_free_context(&mut self.encoder);
            avcodec_free_context(&mut self.decoder);
            av_frame_free(&mut self.frame);
            av_packet_free(&mut self.packet);
            swr_free(&mut self.swr);
            av_free(self.md5);
        }
    }
}
impl Audio<'_> {
    /// # Safety
    /// `self.encoder` must be an open encoder owned by `self`. `frame` is sent to it as-is: it
    /// must be a valid frame for that encoder, or NULL to flush. The caller keeps ownership of
    /// `frame`.
    unsafe fn encode(&mut self, frame: *mut Frame) -> Result<(), i32> {
        unsafe {
            done(avcodec_send_frame(self.encoder, frame))?;
            let mut packet = av_packet_alloc();
            if packet.is_null() {
                return Err(ENOMEM);
            }
            let result = (|| {
                loop {
                    let status = avcodec_receive_packet(self.encoder, packet);
                    if status == EOF || status == EAGAIN {
                        return Ok(());
                    }
                    done(status)?;
                    av_packet_rescale_ts(
                        packet,
                        (*self.encoder).time_base(),
                        (*streams(self.output)[0]).time_base(),
                    );
                    *(*packet).stream_index_ptr() = 0;
                    done(av_interleaved_write_frame(self.output, packet))?;
                    av_packet_unref(packet);
                }
            })();
            av_packet_free(&mut packet);
            result
        }
    }
    /// # Safety
    /// `self.encoder` and `self.fifo` must be the live objects set up by this converter's
    /// initialization. Frames created here are freed before return.
    unsafe fn encode_fifo(&mut self, call: &Call, flush: bool) -> Result<(), i32> {
        unsafe {
            let size = if (*self.encoder).frame_size() == 0 {
                4096
            } else {
                (*self.encoder).frame_size()
            };
            loop {
                let remaining = av_audio_fifo_size(self.fifo);
                if remaining < size && (!flush || remaining == 0) {
                    return Ok(());
                }
                if call.cancelled() {
                    return Err(EXIT);
                }
                let count = size.min(remaining);
                let mut frame = av_frame_alloc();
                if frame.is_null() {
                    return Err(ENOMEM);
                }
                let result = (|| {
                    *(*frame).nb_samples_ptr() = count;
                    *(*frame).format_ptr() = if self.bits == 16 {
                        AV_SAMPLE_FMT_S16
                    } else {
                        AV_SAMPLE_FMT_S32
                    };
                    *(*frame).sample_rate_ptr() = self.rate;
                    *(*frame).pts_ptr() = if (*self.encoder).frame_num() != 0 {
                        self.samples - remaining as i64
                    } else {
                        0
                    };
                    done(av_channel_layout_copy(
                        (*frame).ch_layout_ptr(),
                        (*self.encoder).ch_layout_ptr(),
                    ))?;
                    done(av_frame_get_buffer(frame, 0))?;
                    if av_audio_fifo_read(self.fifo, (*frame).data().as_mut_ptr().cast(), count)
                        != count
                    {
                        return Err(EIO);
                    }
                    self.encode(frame)
                })();
                av_frame_free(&mut frame);
                result?;
            }
        }
    }
    /// # Safety
    /// `input` must be an open `AVFormatContext` whose stream information has been read. `frame`
    /// must be a valid frame for `self`'s encoder. Both are owned by the caller and live for the call.
    /// `request.output` must not be NULL. `request.tags` must point to readable entries for
    /// `request.tag_count` items when that count is non-zero.
    unsafe fn container(
        &mut self,
        request: &Request,
        call: &Call,
        input: *mut Format,
        frame: *mut Frame,
    ) -> Result<(), i32> {
        unsafe {
            if request.output.is_null() || (request.tag_count > 0 && request.tags.is_null()) {
                return Err(EINVAL);
            }
            let flac = request.output_format == 6;
            if !flac {
                let mut mask = 0u32;
                for index in 0..self.channels as u32 {
                    let channel =
                        av_channel_layout_channel_from_index((*frame).ch_layout_ptr(), index);
                    if !(0..32).contains(&channel) {
                        mask = 0;
                        break;
                    }
                    mask |= 1 << channel;
                }
                for index in 0..request.tag_count as usize {
                    self.wave_tags.push((
                        text(*request.tags.add(index * 2)),
                        text(*request.tags.add(index * 2 + 1)),
                    ));
                }
                let file = File::create(text(request.output)).map_err(open_error)?;
                self.wave = Some(
                    dvda_audio::writer::Writer::new(
                        file,
                        dvda_audio::wav::Format {
                            encoding: if self.bits == 16 {
                                dvda_audio::pcm::Encoding::Signed16
                            } else {
                                dvda_audio::pcm::Encoding::Signed24
                            },
                            channels: self.channels.try_into().map_err(|_| EINVAL)?,
                            sample_rate: self.rate as u32,
                            valid_bits: if self.bits == 16 { 16 } else { 24 },
                            channel_mask: mask,
                        },
                    )
                    .map_err(open_error)?,
                );
                return Ok(());
            }
            let codec = avcodec_find_encoder(AV_CODEC_ID_FLAC);
            if codec.is_null() {
                return Err(-1129203192);
            }
            self.encoder = avcodec_alloc_context3(codec);
            if self.encoder.is_null() {
                return Err(ENOMEM);
            }
            *(*self.encoder).sample_fmt_ptr() = if self.bits == 16 {
                AV_SAMPLE_FMT_S16
            } else {
                AV_SAMPLE_FMT_S32
            };
            *(*self.encoder).sample_rate_ptr() = self.rate;
            *(*self.encoder).bits_per_raw_sample_ptr() = self.bits;
            *(*self.encoder).time_base_ptr() = Rational {
                num: 1,
                den: self.rate,
            };
            *(*self.encoder).thread_count_ptr() = win::workers();
            done(av_channel_layout_copy(
                (*self.encoder).ch_layout_ptr(),
                (*frame).ch_layout_ptr(),
            ))?;
            done(avformat_alloc_output_context2(
                &mut self.output,
                ptr::null(),
                c"flac".as_ptr(),
                request.output,
            ))?;
            *(*self.output).interrupt_callback_ptr() = Interrupt {
                callback: Some(crate::media::interrupted),
                opaque: (call as *const Call as *mut Call).cast(),
            };
            if (*(*self.output).oformat()).flags() & AVFMT_GLOBALHEADER != 0 {
                *(*self.encoder).flags_ptr() |= AV_CODEC_FLAG_GLOBAL_HEADER;
            }
            let mut options = ptr::null_mut();
            av_dict_set_int(
                &mut options,
                c"compression_level".as_ptr(),
                request.compression as i64,
                0,
            );
            let status = avcodec_open2(self.encoder, codec, &mut options);
            av_dict_free(&mut options);
            done(status)?;
            let stream = avformat_new_stream(self.output, ptr::null());
            if stream.is_null() {
                return Err(ENOMEM);
            }
            *(*stream).time_base_ptr() = (*self.encoder).time_base();
            done(avcodec_parameters_from_context(
                (*stream).codecpar(),
                self.encoder,
            ))?;
            for i in 0..request.tag_count as usize {
                done(av_dict_set(
                    (*self.output).metadata_ptr(),
                    *request.tags.add(i * 2),
                    *request.tags.add(i * 2 + 1),
                    0,
                ))?;
            }
            let mut cover = ptr::null_mut();
            if request.cover != 0 {
                for source in streams(input) {
                    if (*source).disposition() & AV_DISPOSITION_ATTACHED_PIC == 0 {
                        continue;
                    }
                    let picture = avformat_new_stream(self.output, ptr::null());
                    if picture.is_null() {
                        return Err(ENOMEM);
                    }
                    done(avcodec_parameters_copy(
                        (*picture).codecpar(),
                        (*source).codecpar(),
                    ))?;
                    // The minimal audio build has no image decoders, so FFmpeg may
                    // leave attached-picture dimensions unset. Keep the packet intact.
                    let parameters = (*picture).codecpar();
                    if (*parameters).width() <= 0 || (*parameters).height() <= 0 {
                        let packet = (*source).attached_pic();
                        if packet.size() <= 0 || packet.data().is_null() {
                            return Err(EINVAL);
                        }
                        let bytes =
                            std::slice::from_raw_parts(packet.data(), packet.size() as usize);
                        let (dimensions, _) =
                            dvda_image::codec::metadata(std::io::Cursor::new(bytes))
                                .map_err(|_| EINVAL)?;
                        *(*parameters).width_ptr() = dimensions.width() as i32;
                        *(*parameters).height_ptr() = dimensions.height() as i32;
                    }
                    *(*picture).disposition_ptr() = AV_DISPOSITION_ATTACHED_PIC;
                    *(*picture).time_base_ptr() = Rational { num: 1, den: 90000 };
                    cover = source;
                    break;
                }
            }
            done(avio_open2(
                (*self.output).pb_ptr(),
                request.output,
                2,
                (*self.output).interrupt_callback_ptr(),
                ptr::null_mut(),
            ))?;
            let status = avformat_write_header(self.output, &mut options);
            av_dict_free(&mut options);
            done(status)?;
            if !cover.is_null() {
                let mut picture = av_packet_clone((*cover).attached_pic_ptr());
                if picture.is_null() {
                    return Err(ENOMEM);
                }
                *(*picture).stream_index_ptr() = 1;
                *(*picture).pts_ptr() = 0;
                *(*picture).dts_ptr() = 0;
                let status = av_interleaved_write_frame(self.output, picture);
                av_packet_free(&mut picture);
                done(status)?;
            }
            self.fifo = av_audio_fifo_alloc(
                if self.bits == 16 {
                    AV_SAMPLE_FMT_S16
                } else {
                    AV_SAMPLE_FMT_S32
                },
                self.channels,
                4096,
            );
            if self.fifo.is_null() {
                return Err(ENOMEM);
            }
            Ok(())
        }
    }
    /// # Safety
    /// `frame` must be a frame produced by `self.decoder`. It may be NULL only while the converter
    /// is still uninitialized, before the first frame arrives. The caller keeps ownership of `frame`.
    unsafe fn convert(
        &mut self,
        request: &Request,
        call: &Call,
        frame: *mut Frame,
    ) -> Result<(), i32> {
        unsafe {
            if !self.initialized {
                if frame.is_null() {
                    return Ok(());
                }
                self.channels = (*frame).ch_layout().nb_channels();
                self.rate = if request.rate == 0 {
                    (*frame).sample_rate()
                } else {
                    request.rate as i32
                };
                if self.source_bits == 0 {
                    self.source_bits = (*self.decoder).bits_per_raw_sample().max(0) as u16;
                }
                self.bits = if request.bits == 0 {
                    if self.source_bits > 16 || self.source_float {
                        24
                    } else {
                        16
                    }
                } else {
                    request.bits as i32
                };
                if self.channels <= 0 || self.rate <= 0 || !matches!(self.bits, 16 | 20 | 24) {
                    return Err(EINVAL);
                }
                // Raw 24-bit and 32-bit outputs read the packed buffer as 32-bit samples.
                if self.bits == 16 && matches!(request.output_format, 2 | 4) {
                    return Err(EINVAL);
                }
                self.input_format = (*frame).format();
                self.input_rate = (*frame).sample_rate();
                let source_float = self.source_float
                    || (!self.source_pcm
                        && self.source_bits == 0
                        && matches!(
                            (*frame).format(),
                            AV_SAMPLE_FMT_FLT
                                | AV_SAMPLE_FMT_FLTP
                                | AV_SAMPLE_FMT_DBL
                                | AV_SAMPLE_FMT_DBLP
                        ));
                if dvda_audio::dither::enabled(
                    request.dither != 0 && request.output_format != 5,
                    self.source_bits,
                    source_float,
                    self.bits as u16,
                ) {
                    use std::hash::{BuildHasher, Hasher};
                    let seed = std::collections::hash_map::RandomState::new()
                        .build_hasher()
                        .finish();
                    self.dither = Some(dvda_audio::dither::Tpdf::new(seed));
                    call.message(2, "16-bit output: applying Rust TPDF dither");
                }
                self.format = if self.dither.is_some() {
                    AV_SAMPLE_FMT_DBL
                } else if self.bits == 20 {
                    AV_SAMPLE_FMT_DBLP
                } else if self.bits == 16 {
                    AV_SAMPLE_FMT_S16
                } else {
                    AV_SAMPLE_FMT_S32
                };
                done(swr_alloc_set_opts2(
                    &mut self.swr,
                    (*frame).ch_layout_ptr(),
                    self.format,
                    self.rate,
                    (*frame).ch_layout_ptr(),
                    (*frame).format(),
                    (*frame).sample_rate(),
                    0,
                    ptr::null_mut(),
                ))?;
                done(av_opt_set(
                    self.swr,
                    c"dither_method".as_ptr(),
                    c"none".as_ptr(),
                    0,
                ))?;
                if request.soxr != 0 {
                    done(av_opt_set(
                        self.swr,
                        c"resampler".as_ptr(),
                        c"soxr".as_ptr(),
                        0,
                    ))?;
                }
                done(swr_init(self.swr))?;
                if let Some(sink) = self.pcm_sink.as_deref_mut() {
                    let mut mask = 0u32;
                    for index in 0..self.channels as u32 {
                        let channel =
                            av_channel_layout_channel_from_index((*frame).ch_layout_ptr(), index);
                        if !(0..32).contains(&channel) {
                            mask = 0;
                            break;
                        }
                        mask |= 1 << channel;
                    }
                    sink.start(PcmFormat {
                        sample_rate: self.rate as u32,
                        bits: self.bits as u32,
                        channels: self.channels as u32,
                        channel_mask: mask,
                    })?;
                } else if matches!(request.output_format, 1 | 6) {
                    self.container(request, call, self.input, frame)?;
                } else if request.output_format == 5 {
                    self.md5 = av_md5_alloc();
                    if self.md5.is_null() {
                        return Err(ENOMEM);
                    }
                    av_md5_init(self.md5);
                } else if request.output_format != 0 {
                    if request.output.is_null() {
                        return Err(EINVAL);
                    }
                    self.file = Some(File::create(text(request.output)).map_err(open_error)?);
                }
                self.initialized = true;
            }
            if !frame.is_null()
                && ((*frame).sample_rate() != self.input_rate
                    || (*frame).ch_layout().nb_channels() != self.channels
                    || (*frame).format() != self.input_format)
            {
                return Err(-1668179713);
            }
            let input_count = if frame.is_null() {
                0
            } else {
                (*frame).nb_samples()
            };
            let capacity = swr_get_out_samples(self.swr, input_count);
            done(capacity)?;
            let mut data = ptr::null_mut();
            done(av_samples_alloc_array_and_samples(
                &mut data,
                ptr::null_mut(),
                self.channels,
                capacity.max(1),
                self.format,
                0,
            ))?;
            let result = (|| {
                let count = swr_convert(
                    self.swr,
                    data,
                    capacity.max(1),
                    if frame.is_null() {
                        ptr::null()
                    } else {
                        (*frame).extended_data().cast()
                    },
                    input_count,
                );
                done(count)?;
                if count == 0 {
                    return Ok(());
                }
                let values = count as usize * self.channels as usize;
                let mut converted = Vec::<i32>::new();
                let mut quantized = Vec::<i16>::new();
                let packed = if let Some(dither) = &mut self.dither {
                    let plane = *data;
                    if plane.is_null() {
                        return Err(INVALID);
                    }
                    quantized.reserve(values);
                    for index in 0..values {
                        quantized.push(
                            dither.quantize(ptr::read_unaligned(plane.cast::<f64>().add(index))),
                        );
                    }
                    quantized.as_ptr().cast::<u8>()
                } else if self.bits == 20 {
                    converted.reserve(values);
                    for sample in 0..count as usize {
                        for channel in 0..self.channels as usize {
                            let plane = *data.add(channel);
                            if plane.is_null() {
                                return Err(INVALID);
                            }
                            let value = ptr::read_unaligned(plane.cast::<f64>().add(sample));
                            converted.push(
                                (value * 524288.0 + 0.5).floor().clamp(-524288.0, 524287.0) as i32
                                    * 4096,
                            );
                        }
                    }
                    converted.as_ptr().cast::<u8>()
                } else {
                    *data
                };
                if packed.is_null() {
                    return Err(INVALID);
                }
                self.samples += count as i64;
                if let Some(wave) = &mut self.wave {
                    if self.bits == 16 {
                        wave.write_frames(std::slice::from_raw_parts(packed, values * 2))
                            .map_err(open_error)?;
                    } else {
                        let mut bytes = Vec::with_capacity(values * 3);
                        for index in 0..values {
                            let value = ptr::read_unaligned(packed.cast::<u32>().add(index)) >> 8;
                            bytes.extend_from_slice(&value.to_le_bytes()[..3]);
                        }
                        wave.write_frames(&bytes).map_err(open_error)?;
                    }
                }
                if !self.fifo.is_null() {
                    let mut plane = packed as *mut u8;
                    if av_audio_fifo_write(self.fifo, (&mut plane as *mut *mut u8).cast(), count)
                        != count
                    {
                        return Err(ENOMEM);
                    }
                    self.encode_fifo(call, false)?;
                }
                if !self.md5.is_null() {
                    av_md5_update(self.md5, packed, values * 2);
                }
                if let Some(file) = &mut self.file {
                    if request.output_format == 2 {
                        let mut bytes = Vec::with_capacity(values * 3);
                        for i in 0..values {
                            let value = ptr::read_unaligned(packed.cast::<u32>().add(i)) >> 8;
                            bytes.extend_from_slice(&value.to_le_bytes()[..3]);
                        }
                        file.write_all(&bytes).map_err(|_| EIO)?;
                    } else {
                        file.write_all(std::slice::from_raw_parts(
                            packed,
                            values * if request.output_format == 3 { 2 } else { 4 },
                        ))
                        .map_err(|_| EIO)?;
                    }
                }
                if let Some(sink) = self.pcm_sink.as_deref_mut() {
                    if self.bits == 16 {
                        let mut pcm = Vec::with_capacity(values);
                        for index in 0..values {
                            pcm.push(
                                i32::from(ptr::read_unaligned(packed.cast::<i16>().add(index)))
                                    << 8,
                            );
                        }
                        sink.write(&pcm)?;
                    } else {
                        sink.write(std::slice::from_raw_parts(packed.cast::<i32>(), values))?;
                    }
                }
                if self.samples - self.progress >= i64::from((self.rate / 4).max(1)) {
                    self.progress = self.samples;
                    call.message(
                        3,
                        &format!(
                            "out_time_us={}",
                            av_rescale(self.samples, 1_000_000, self.rate as i64)
                        ),
                    );
                }
                Ok(())
            })();
            av_freep(data.cast());
            av_freep((&mut data as *mut *mut *mut u8).cast());
            result
        }
    }
    /// # Safety
    /// `self.decoder` and `self.frame` must be the live objects of this converter. `request` and
    /// `call` must be valid for the call.
    unsafe fn pcm_packet(&mut self, request: &Request, call: &Call, codec: i32) -> Result<(), i32> {
        use dvda_audio::pcm::Encoding;
        unsafe {
            let encoding = match codec {
                AV_CODEC_ID_PCM_S8 => Encoding::Signed8,
                AV_CODEC_ID_PCM_U8 => Encoding::Unsigned8,
                AV_CODEC_ID_PCM_S16LE => Encoding::Signed16,
                AV_CODEC_ID_PCM_S24LE => Encoding::Signed24,
                AV_CODEC_ID_PCM_S32LE => Encoding::Signed32,
                AV_CODEC_ID_PCM_F32LE => Encoding::Float32,
                AV_CODEC_ID_PCM_F64LE => Encoding::Float64,
                _ => return Err(EINVAL),
            };
            let channels = (*self.decoder).ch_layout().nb_channels();
            let size = (*self.packet).size();
            if channels <= 0
                || size < 0
                || !(size as usize).is_multiple_of(channels as usize * encoding.bytes())
            {
                return Err(INVALID);
            }
            if size == 0 {
                return Ok(());
            }
            if (*self.packet).data().is_null() {
                return Err(INVALID);
            }
            self.pcm_bytes(
                request,
                call,
                encoding,
                std::slice::from_raw_parts((*self.packet).data(), size as usize),
            )
        }
    }
    unsafe fn pcm_bytes(
        &mut self,
        request: &Request,
        call: &Call,
        encoding: dvda_audio::pcm::Encoding,
        bytes: &[u8],
    ) -> Result<(), i32> {
        unsafe {
            let channels = (*self.decoder).ch_layout().nb_channels();
            if channels <= 0
                || !bytes
                    .len()
                    .is_multiple_of(channels as usize * encoding.bytes())
            {
                return Err(INVALID);
            }
            let mut samples = Vec::new();
            encoding.decode(bytes, &mut samples).map_err(|_| INVALID)?;
            av_frame_unref(self.frame);
            *(*self.frame).format_ptr() = AV_SAMPLE_FMT_DBL;
            *(*self.frame).sample_rate_ptr() = (*self.decoder).sample_rate();
            *(*self.frame).nb_samples_ptr() = (samples.len() / channels as usize) as i32;
            done(av_channel_layout_copy(
                (*self.frame).ch_layout_ptr(),
                (*self.decoder).ch_layout_ptr(),
            ))?;
            done(av_frame_get_buffer(self.frame, 0))?;
            std::ptr::copy_nonoverlapping(
                samples.as_ptr().cast::<u8>(),
                (*self.frame).data()[0],
                samples.len() * 8,
            );
            self.convert(request, call, self.frame)?;
            av_frame_unref(self.frame);
            Ok(())
        }
    }
    unsafe fn receive(&mut self, request: &Request, call: &Call) -> Result<(), i32> {
        unsafe {
            loop {
                let result = avcodec_receive_frame(self.decoder, self.frame);
                if result == EAGAIN || result == EOF {
                    return Ok(());
                }
                done(result)?;
                if call.cancelled() {
                    return Err(EXIT);
                }
                let result = self.convert(request, call, self.frame);
                av_frame_unref(self.frame);
                result?;
            }
        }
    }
}
/// # Safety
/// `input` must be an open `AVFormatContext` whose stream information has been read, owned by
/// the caller and alive for the call. `request` and `call` must be valid for the call.
pub(crate) unsafe fn audio_input(
    request: &Request,
    call: &Call,
    input: *mut Format,
    mut wave: Option<dvda_audio::wav::Reader<std::fs::File>>,
    pcm_sink: Option<&mut dyn PcmSink>,
) -> Result<(), i32> {
    unsafe {
        if request.output_format > 6 || (pcm_sink.is_some() && request.output_format != 0) {
            return Err(EINVAL);
        }
        let streams = streams(input);
        let stream = streams
            .iter()
            .position(|s| (*(**s).codecpar()).codec_type() == 1)
            .ok_or(-1381258232)?;
        let parameters = (*streams[stream]).codecpar();
        let pcm = matches!(
            (*parameters).codec_id(),
            AV_CODEC_ID_PCM_S8
                | AV_CODEC_ID_PCM_U8
                | AV_CODEC_ID_PCM_S16LE
                | AV_CODEC_ID_PCM_S24LE
                | AV_CODEC_ID_PCM_S32LE
                | AV_CODEC_ID_PCM_F32LE
                | AV_CODEC_ID_PCM_F64LE
        );
        let codec = avcodec_find_decoder((*parameters).codec_id());
        if codec.is_null() && !pcm {
            return Err(-1128613112);
        }
        let mut a = Audio {
            decoder: avcodec_alloc_context3(codec),
            frame: av_frame_alloc(),
            packet: av_packet_alloc(),
            swr: ptr::null_mut(),
            md5: ptr::null_mut(),
            file: None,
            wave: None,
            wave_tags: Vec::new(),
            rate: 0,
            bits: 0,
            channels: 0,
            format: 0,
            input_format: 0,
            input_rate: 0,
            initialized: false,
            source_pcm: pcm,
            source_bits: match (*parameters).bits_per_raw_sample() {
                bits if bits > 0 => bits as u16,
                _ => match (*parameters).codec_id() {
                    AV_CODEC_ID_PCM_S8 | AV_CODEC_ID_PCM_U8 => 8,
                    AV_CODEC_ID_PCM_S16LE => 16,
                    AV_CODEC_ID_PCM_S24LE => 24,
                    AV_CODEC_ID_PCM_S32LE | AV_CODEC_ID_PCM_F32LE => 32,
                    AV_CODEC_ID_PCM_F64LE => 64,
                    _ => 0,
                },
            },
            source_float: matches!(
                (*parameters).codec_id(),
                AV_CODEC_ID_PCM_F32LE | AV_CODEC_ID_PCM_F64LE
            ),
            dither: None,
            samples: 0,
            progress: 0,
            encoder: ptr::null_mut(),
            output: ptr::null_mut(),
            fifo: ptr::null_mut(),
            input,
            pcm_sink,
        };
        if a.decoder.is_null() || a.frame.is_null() || a.packet.is_null() {
            return Err(ENOMEM);
        }
        done(avcodec_parameters_to_context(
            a.decoder,
            (*streams[stream]).codecpar(),
        ))?;
        *(*a.decoder).thread_count_ptr() = win::workers();
        *(*a.decoder).err_recognition_ptr() = AV_EF_CRCCHECK | AV_EF_EXPLODE;
        if !pcm {
            done(avcodec_open2(a.decoder, codec, ptr::null_mut()))?;
        }
        if let Some(reader) = &mut wave {
            let mut bytes = vec![0; reader.format.block_align() * 4096];
            loop {
                if call.cancelled() {
                    return Err(EXIT);
                }
                let count = reader.read_frames(&mut bytes).map_err(open_error)?;
                if count == 0 {
                    break;
                }
                a.pcm_bytes(request, call, reader.format.encoding, &bytes[..count])?;
            }
        } else {
            loop {
                if call.cancelled() {
                    return Err(EXIT);
                }
                let status = av_read_frame(input, a.packet);
                if status == EOF {
                    break;
                }
                done(status)?;
                if (*a.packet).stream_index() == stream as i32 {
                    if pcm {
                        a.pcm_packet(request, call, (*parameters).codec_id())?;
                    } else {
                        done(avcodec_send_packet(a.decoder, a.packet))?;
                        a.receive(request, call)?;
                    }
                }
                av_packet_unref(a.packet);
            }
        }
        if !pcm {
            done(avcodec_send_packet(a.decoder, ptr::null()))?;
            a.receive(request, call)?;
        }
        a.convert(request, call, ptr::null_mut())?;
        if !a.initialized {
            return Err(INVALID);
        }
        if !a.encoder.is_null() {
            a.encode_fifo(call, true)?;
            a.encode(ptr::null_mut())?;
            done(av_write_trailer(a.output))?;
            done(avio_closep((*a.output).pb_ptr()))?;
        }
        if let Some(wave) = a.wave.take() {
            wave.finish_with_tags(&a.wave_tags).map_err(open_error)?;
        }
        if let Some(file) = &mut a.file {
            file.flush().map_err(|_| EIO)?;
        }
        if let Some(sink) = a.pcm_sink.as_deref_mut() {
            sink.finish()?;
        }
        call.message(
            3,
            &format!(
                "out_time_us={}",
                av_rescale(a.samples, 1_000_000, a.rate as i64)
            ),
        );
        if !a.md5.is_null() {
            let mut digest = [0u8; 16];
            av_md5_final(a.md5, digest.as_mut_ptr());
            call.message(
                1,
                &format!(
                    "MD5={}",
                    digest
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                ),
            );
        }
        call.message(2, &format!("Number of samples: {}", a.samples));
        Ok(())
    }
}
fn rgb24_raster(
    size: dvda_image::geometry::Dimensions,
    stride: usize,
    data: &[u8],
) -> Result<dvda_image::raster::Raster, i32> {
    let row_bytes = (size.width() as usize).checked_mul(3).ok_or(INVALID)?;
    let required = (size.height() as usize - 1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(row_bytes))
        .ok_or(INVALID)?;
    if stride < row_bytes || data.len() < required {
        return Err(INVALID);
    }
    let mut image = dvda_image::raster::Raster::solid(size, [0, 0, 0, 255]).map_err(|_| ENOMEM)?;
    for y in 0..size.height() {
        let start = y as usize * stride;
        for (x, pixel) in data[start..start + row_bytes]
            .as_chunks::<3>()
            .0
            .iter()
            .enumerate()
        {
            image.blend_pixel(x as i32, y as i32, [pixel[0], pixel[1], pixel[2], 255]);
        }
    }
    Ok(image)
}

/// # Safety
/// `input` must be an open `AVFormatContext` whose stream information has been read, owned by
/// the caller and alive for the call. `request` and `call` must be valid for the call.
pub unsafe fn video(request: &Request, call: &Call, input: *mut Format) -> Result<(), i32> {
    let (image, output) = unsafe {
        if request.output.is_null() {
            return Err(EINVAL);
        }
        let mut codec = ptr::null();
        let stream = av_find_best_stream(input, 0, -1, -1, &mut codec, 0);
        done(stream)?;
        let mut decoder = avcodec_alloc_context3(codec);
        let mut frame = av_frame_alloc();
        let mut rgb = av_frame_alloc();
        let mut packet = av_packet_alloc();
        let mut scale = ptr::null_mut();
        let result = (|| {
            if decoder.is_null() || frame.is_null() || rgb.is_null() || packet.is_null() {
                return Err(ENOMEM);
            }
            done(avcodec_parameters_to_context(
                decoder,
                (*streams(input)[stream as usize]).codecpar(),
            ))?;
            *(*decoder).thread_count_ptr() = win::workers();
            done(avcodec_open2(decoder, codec, ptr::null_mut()))?;
            loop {
                if call.cancelled() {
                    return Err(EXIT);
                }
                let status = av_read_frame(input, packet);
                if status != EOF {
                    done(status)?;
                }
                if status == EOF || (*packet).stream_index() == stream {
                    done(avcodec_send_packet(
                        decoder,
                        if status == EOF { ptr::null() } else { packet },
                    ))?;
                    let receive = avcodec_receive_frame(decoder, frame);
                    if receive >= 0 {
                        break;
                    }
                    if receive != EAGAIN {
                        return Err(receive);
                    }
                    if status == EOF {
                        return Err(INVALID);
                    }
                }
                av_packet_unref(packet);
            }
            *(*rgb).format_ptr() = AV_PIX_FMT_RGB24;
            *(*rgb).width_ptr() = (*frame).width();
            *(*rgb).height_ptr() = (*frame).height();
            done(av_frame_get_buffer(rgb, 0))?;
            scale = sws_getContext(
                (*frame).width(),
                (*frame).height(),
                (*frame).format(),
                (*frame).width(),
                (*frame).height(),
                AV_PIX_FMT_RGB24,
                4,
                ptr::null(),
                ptr::null(),
                ptr::null(),
            );
            if scale.is_null() {
                return Err(ENOMEM);
            }
            if sws_scale(
                scale,
                (*frame).data().as_ptr().cast(),
                (*frame).linesize().as_ptr(),
                0,
                (*frame).height(),
                (*rgb).data().as_ptr(),
                (*rgb).linesize().as_ptr(),
            ) != (*frame).height()
            {
                return Err(EIO);
            }
            let size = dvda_image::geometry::Dimensions::new(
                (*rgb).width() as u32,
                (*rgb).height() as u32,
            )
            .map_err(|_| INVALID)?;
            let row_bytes = (size.width() as usize).checked_mul(3).ok_or(ENOMEM)?;
            let stride = (*rgb).linesize()[0];
            if stride < 0 || (stride as usize) < row_bytes || (*rgb).data()[0].is_null() {
                return Err(INVALID);
            }
            let image = rgb24_raster(
                size,
                stride as usize,
                std::slice::from_raw_parts(
                    (*rgb).data()[0],
                    (size.height() as usize - 1)
                        .checked_mul(stride as usize)
                        .and_then(|offset| offset.checked_add(row_bytes))
                        .ok_or(INVALID)?,
                ),
            )?;
            Ok((image, text(request.output)))
        })();
        sws_freeContext(scale);
        avcodec_free_context(&mut decoder);
        av_frame_free(&mut frame);
        av_frame_free(&mut rgb);
        av_packet_free(&mut packet);
        result
    }?;
    let mut file = File::create(output).map_err(open_error)?;
    dvda_image::codec::write_png(&image, &mut file).map_err(|_| EIO)?;
    file.flush().map_err(|_| EIO)?;
    Ok(())
}

#[cfg(test)]
mod video_png_tests {
    use super::*;
    use dvda_image::{codec, geometry::Dimensions};

    #[test]
    fn rgb24_padding_and_png_pixels() {
        let size = Dimensions::new(2, 2).unwrap();
        let data = [255, 0, 1, 2, 128, 254, 99, 99, 0, 3, 255, 127, 64, 32];
        let image = rgb24_raster(size, 8, &data).unwrap();
        let expected = [
            255, 0, 1, 255, 2, 128, 254, 255, 0, 3, 255, 255, 127, 64, 32, 255,
        ];
        assert_eq!(image.rgba(), expected);
        let mut png = Vec::new();
        codec::write_png(&image, &mut png).unwrap();
        let decoded = codec::decode(std::io::Cursor::new(png)).unwrap();
        assert_eq!(decoded.dimensions(), size);
        assert_eq!(decoded.rgba(), expected);
    }

    #[test]
    fn rgb24_rejects_invalid_layout() {
        let size = Dimensions::new(2, 2).unwrap();
        for (stride, data) in [(5, &[0; 14][..]), (8, &[0; 13][..]), (usize::MAX, &[][..])] {
            assert_eq!(rgb24_raster(size, stride, data).unwrap_err(), INVALID);
        }
    }
}
