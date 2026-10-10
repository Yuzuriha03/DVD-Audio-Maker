#![allow(unsafe_op_in_unsafe_fn)]
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
struct Audio {
    decoder: *mut Context,
    frame: *mut Frame,
    packet: *mut Packet,
    swr: *mut c_void,
    md5: *mut c_void,
    file: Option<File>,
    rate: i32,
    bits: i32,
    channels: i32,
    format: i32,
    initialized: bool,
    samples: i64,
    progress: i64,
    encoder: *mut Context,
    output: *mut Format,
    fifo: *mut c_void,
    input: *mut Format,
}
impl Drop for Audio {
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
impl Audio {
    unsafe fn encode(&mut self, frame: *mut Frame) -> Result<(), i32> {
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
    unsafe fn encode_fifo(&mut self, call: &Call, flush: bool) -> Result<(), i32> {
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
    unsafe fn container(
        &mut self,
        request: &Request,
        call: &Call,
        input: *mut Format,
        frame: *mut Frame,
    ) -> Result<(), i32> {
        if request.output.is_null() || (request.tag_count > 0 && request.tags.is_null()) {
            return Err(EINVAL);
        }
        let flac = request.output_format == 6;
        let codec = avcodec_find_encoder(if flac {
            AV_CODEC_ID_FLAC
        } else if self.bits == 16 {
            AV_CODEC_ID_PCM_S16LE
        } else {
            AV_CODEC_ID_PCM_S24LE
        });
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
        *(*self.encoder).bits_per_raw_sample_ptr() = if flac {
            self.bits
        } else if self.bits == 16 {
            16
        } else {
            24
        };
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
            if flac {
                c"flac".as_ptr()
            } else {
                c"wav".as_ptr()
            },
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
        if flac {
            av_dict_set_int(
                &mut options,
                c"compression_level".as_ptr(),
                request.compression as i64,
                0,
            );
        }
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
        if request.cover != 0 && flac {
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
        if !flac {
            av_dict_set(&mut options, c"rf64".as_ptr(), c"never".as_ptr(), 0);
        }
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
    unsafe fn convert(
        &mut self,
        request: &Request,
        call: &Call,
        frame: *mut Frame,
    ) -> Result<(), i32> {
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
            self.bits = if request.bits == 0 {
                if (*self.decoder).bits_per_raw_sample() > 16 {
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
            self.format = if self.bits == 20 {
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
            if matches!(request.output_format, 1 | 6) {
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
            && ((*frame).sample_rate() != (*self.decoder).sample_rate()
                || (*frame).ch_layout().nb_channels() != self.channels
                || (*frame).format() != (*self.decoder).sample_fmt())
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
            let packed = if self.bits == 20 {
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
    unsafe fn receive(&mut self, request: &Request, call: &Call) -> Result<(), i32> {
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
pub unsafe fn audio(request: &Request, call: &Call, input: *mut Format) -> Result<(), i32> {
    if request.output_format > 6 {
        return Err(EINVAL);
    }
    let streams = streams(input);
    let stream = streams
        .iter()
        .position(|s| (*(**s).codecpar()).codec_type() == 1)
        .ok_or(-1381258232)?;
    let codec = avcodec_find_decoder((*(*streams[stream]).codecpar()).codec_id());
    if codec.is_null() {
        return Err(-1128613112);
    }
    let mut a = Audio {
        decoder: avcodec_alloc_context3(codec),
        frame: av_frame_alloc(),
        packet: av_packet_alloc(),
        swr: ptr::null_mut(),
        md5: ptr::null_mut(),
        file: None,
        rate: 0,
        bits: 0,
        channels: 0,
        format: 0,
        initialized: false,
        samples: 0,
        progress: 0,
        encoder: ptr::null_mut(),
        output: ptr::null_mut(),
        fifo: ptr::null_mut(),
        input,
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
    done(avcodec_open2(a.decoder, codec, ptr::null_mut()))?;
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
            done(avcodec_send_packet(a.decoder, a.packet))?;
            a.receive(request, call)?;
        }
        av_packet_unref(a.packet);
    }
    done(avcodec_send_packet(a.decoder, ptr::null()))?;
    a.receive(request, call)?;
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
    if let Some(file) = &mut a.file {
        file.flush().map_err(|_| EIO)?;
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
pub unsafe fn video(request: &Request, call: &Call, input: *mut Format) -> Result<(), i32> {
    if request.output.is_null() {
        return Err(EINVAL);
    }
    let mut codec = ptr::null();
    let stream = av_find_best_stream(input, 0, -1, -1, &mut codec, 0);
    done(stream)?;
    let mut decoder = avcodec_alloc_context3(codec);
    let mut encoder = ptr::null_mut();
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
        codec = avcodec_find_encoder(AV_CODEC_ID_PNG);
        encoder = avcodec_alloc_context3(codec);
        if encoder.is_null() {
            return Err(ENOMEM);
        }
        *(*encoder).width_ptr() = (*rgb).width();
        *(*encoder).height_ptr() = (*rgb).height();
        *(*encoder).pix_fmt_ptr() = AV_PIX_FMT_RGB24;
        *(*encoder).time_base_ptr() = Rational { num: 1, den: 25 };
        *(*encoder).thread_count_ptr() = 1;
        done(avcodec_open2(encoder, codec, ptr::null_mut()))?;
        av_packet_unref(packet);
        done(avcodec_send_frame(encoder, rgb))?;
        done(avcodec_receive_packet(encoder, packet))?;
        let mut file = File::create(text(request.output)).map_err(open_error)?;
        file.write_all(std::slice::from_raw_parts(
            (*packet).data(),
            (*packet).size() as usize,
        ))
        .map_err(|_| EIO)?;
        file.flush().map_err(|_| EIO)?;
        Ok(())
    })();
    sws_freeContext(scale);
    avcodec_free_context(&mut decoder);
    avcodec_free_context(&mut encoder);
    av_frame_free(&mut frame);
    av_frame_free(&mut rgb);
    av_packet_free(&mut packet);
    result
}
