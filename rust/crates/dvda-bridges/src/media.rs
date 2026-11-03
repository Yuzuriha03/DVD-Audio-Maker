use crate::{Call, Cancel, Emit, ffmpeg::*};
use std::{
    ffi::{CStr, c_char, c_void},
    io::Write,
    ptr,
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Request {
    pub size: u32,
    pub abi: u32,
    pub operation: u32,
    pub rate: u32,
    pub bits: u32,
    pub output_format: u32,
    pub soxr: u32,
    pub dither: u32,
    pub compression: u32,
    pub cover: u32,
    pub tag_count: u32,
    pub input: *const c_char,
    pub output: *const c_char,
    pub tags: *const *const c_char,
}
thread_local! {
    static ACTIVE: std::cell::Cell<*const Call> = const { std::cell::Cell::new(ptr::null()) };
}
unsafe extern "C" fn logger(
    context: *mut c_void,
    level: i32,
    format: *const c_char,
    arguments: *mut c_char,
) {
    unsafe {
        if level > 16 {
            return;
        }
        ACTIVE.with(|active| {
            let call = active.get();
            if call.is_null() {
                return;
            }
            let mut buffer = [0i8; 2048];
            let mut print_prefix = 0;
            av_log_format_line2(
                context,
                level,
                format,
                arguments,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
                &mut print_prefix,
            );
            buffer[2047] = 0;
            if let Some(emit) = (*call).emit {
                emit((*call).state, 2, buffer.as_ptr());
            }
        });
    }
}
struct ActiveGuard(*const Call);
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.set(self.0));
    }
}
pub(crate) unsafe extern "C" fn interrupted(state: *mut c_void) -> i32 {
    unsafe { (*(state.cast::<Call>())).cancelled() as i32 }
}
fn quoted(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ch if ch < ' ' => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}
/// # Safety
/// `metadata` must be NULL or a valid `AVDictionary` that stays alive and unmodified for the
/// call. Entries are copied into the returned string.
unsafe fn dictionary(metadata: *const c_void) -> String {
    unsafe {
        let mut entries = Vec::new();
        let mut entry = ptr::null();
        loop {
            entry = av_dict_iterate(metadata, entry);
            if entry.is_null() {
                break;
            }
            entries.push(format!(
                "{}:{}",
                quoted(&text((*entry).key)),
                quoted(&text((*entry).value))
            ));
        }
        format!("{{{}}}", entries.join(","))
    }
}
/// # Safety
/// `format` must be an open `AVFormatContext` whose stream information has been read, owned
/// by the caller and alive for the call. `call` must be the active request.
unsafe fn probe(call: &Call, format: *mut Format) -> Result<(), i32> {
    unsafe {
        let mut records = Vec::new();
        for (index, stream) in streams(format).into_iter().enumerate() {
            let stream = &*stream;
            let p = &*stream.codecpar();
            let base = stream.time_base();
            let bits = if p.bits_per_raw_sample() != 0 {
                p.bits_per_raw_sample()
            } else {
                av_get_bits_per_sample(p.codec_id())
            };
            let duration = if stream.duration() == i64::MIN {
                String::from("null")
            } else {
                stream.duration().to_string()
            };
            records.push(format!("{{\"index\":{index},\"codec_type\":{},\"codec_name\":{},\"sample_rate\":{},\"channels\":{},\"bits_per_raw_sample\":{bits},\"width\":{},\"height\":{},\"duration_ts\":{duration},\"time_base\":\"{}/{}\",\"tags\":{}}}",quoted(&text(av_get_media_type_string(p.codec_type()))),quoted(&text(avcodec_get_name(p.codec_id()))),p.sample_rate(),p.ch_layout().nb_channels(),p.width(),p.height(),base.num,base.den,dictionary(stream.metadata())));
        }
        let duration = if (*format).duration() == i64::MIN {
            String::from("null")
        } else {
            format!("{:.9}", (*format).duration() as f64 / 1_000_000.0)
        };
        call.message(
            1,
            &format!(
                "{{\"streams\":[{}],\"format\":{{\"duration\":{duration},\"tags\":{}}}}}",
                records.join(","),
                dictionary((*format).metadata())
            ),
        );
        Ok(())
    }
}
/// # Safety
/// `format` must be an open `AVFormatContext` whose stream information has been read, owned
/// by the caller and alive for the call. The packet allocated here is freed before return.
unsafe fn packets(call: &Call, format: *mut Format) -> Result<(), i32> {
    unsafe {
        let streams = streams(format);
        let index = streams
            .iter()
            .position(|s| (*(**s).codecpar()).codec_type() == 1)
            .ok_or(-1381258232)?;
        let base = (*streams[index]).time_base();
        let time = base.num as f64 / base.den as f64;
        let mut packet = av_packet_alloc();
        if packet.is_null() {
            return Err(ENOMEM);
        }
        let result = (|| {
            loop {
                if call.cancelled() {
                    return Err(EXIT);
                }
                let status = av_read_frame(format, packet);
                if status == EOF {
                    return Ok(());
                }
                done(status)?;
                if (*packet).stream_index() == index as i32 {
                    let pts = if (*packet).pts() == i64::MIN {
                        0.0
                    } else {
                        (*packet).pts() as f64 * time
                    };
                    call.message(
                        1,
                        &format!(
                            "{pts:.9},{:.9},{},{}",
                            (*packet).duration() as f64 * time,
                            (*packet).size(),
                            (*packet).pos()
                        ),
                    );
                }
                av_packet_unref(packet);
            }
        })();
        av_packet_free(&mut packet);
        result
    }
}
/// # Safety
/// `format` must be an open `AVFormatContext` whose stream information has been read, owned
/// by the caller and alive for the call. `request.output` must be a valid NUL-terminated path.
/// The attached picture bytes are only read during the call.
unsafe fn cover(request: &Request, format: *mut Format) -> Result<(), i32> {
    unsafe {
        if request.output.is_null() {
            return Err(EINVAL);
        }
        for stream in streams(format) {
            if (*stream).disposition() & AV_DISPOSITION_ATTACHED_PIC == 0 {
                continue;
            }
            let packet = (*stream).attached_pic();
            if packet.size() < 0 || packet.data().is_null() {
                return Err(INVALID);
            }
            let mut file = std::fs::File::create(text(request.output))
                .map_err(crate::media_convert::open_error)?;
            file.write_all(std::slice::from_raw_parts(
                packet.data(),
                packet.size() as usize,
            ))
            .map_err(|_| EIO)?;
            file.flush().map_err(|_| EIO)?;
            return Ok(());
        }
        Err(-1381258232)
    }
}
/// # Safety
/// `request.input` must not be NULL and must be a valid NUL-terminated string for the call.
/// `request` must be readable for the call. `call` is registered as the interrupt callback's
/// opaque state, so it must stay alive and must not move until `run` returns.
unsafe fn run(request: &Request, call: &mut Call) -> Result<(), i32> {
    unsafe {
        if call.cancelled() {
            return Err(EXIT);
        }
        if !matches!(request.operation, 1..=5) {
            return Err(EINVAL);
        }
        let path = text(request.input);
        let wave_reader = if let Ok(mut source) = std::fs::File::open(&path) {
            use std::io::{Read, Seek, SeekFrom};
            let mut signature = [0; 12];
            if source.read_exact(&mut signature).is_ok()
                && matches!(&signature[..4], b"RIFF" | b"RF64")
                && &signature[8..] == b"WAVE"
            {
                source
                    .seek(SeekFrom::Start(0))
                    .map_err(crate::media_convert::open_error)?;
                Some(
                    dvda_audio::wav::Reader::new(source)
                        .map_err(crate::media_convert::open_error)?,
                )
            } else {
                None
            }
        } else {
            None
        };
        let mut format = avformat_alloc_context();
        if format.is_null() {
            return Err(ENOMEM);
        }
        *(*format).interrupt_callback_ptr() = Interrupt {
            callback: Some(interrupted),
            opaque: (call as *mut Call).cast(),
        };
        let forced = if CStr::from_ptr(request.input).to_bytes().ends_with(b".mlp")
            || text(request.input).to_ascii_lowercase().ends_with(".mlp")
        {
            av_find_input_format(c"mlp".as_ptr())
        } else {
            ptr::null()
        };
        let result = (|| {
            if let Some(reader) = wave_reader {
                use dvda_audio::pcm::Encoding;
                let spec = reader.format;
                let stream = avformat_new_stream(format, ptr::null());
                if stream.is_null() {
                    return Err(ENOMEM);
                }
                let p = (*stream).codecpar();
                *(*p).codec_type_ptr() = 1;
                *(*p).codec_id_ptr() = match spec.encoding {
                    Encoding::Signed8 => AV_CODEC_ID_PCM_S8,
                    Encoding::Unsigned8 => AV_CODEC_ID_PCM_U8,
                    Encoding::Signed16 => AV_CODEC_ID_PCM_S16LE,
                    Encoding::Signed24 => AV_CODEC_ID_PCM_S24LE,
                    Encoding::Signed32 => AV_CODEC_ID_PCM_S32LE,
                    Encoding::Float32 => AV_CODEC_ID_PCM_F32LE,
                    Encoding::Float64 => AV_CODEC_ID_PCM_F64LE,
                };
                *(*p).sample_rate_ptr() = spec.sample_rate as i32;
                *(*p).bits_per_raw_sample_ptr() = spec.valid_bits as i32;
                if spec.channel_mask != 0 {
                    done(av_channel_layout_from_mask(
                        (*p).ch_layout_ptr(),
                        spec.channel_mask.into(),
                    ))?;
                } else {
                    av_channel_layout_default((*p).ch_layout_ptr(), spec.channels.into());
                }
                *(*stream).time_base_ptr() = Rational {
                    num: 1,
                    den: spec.sample_rate as i32,
                };
                *(*stream).duration_ptr() = i64::try_from(reader.frames()).map_err(|_| INVALID)?;
                *(*format).duration_ptr() =
                    av_rescale((*stream).duration(), 1_000_000, spec.sample_rate.into());
                for (key, value) in &reader.tags {
                    let key = std::ffi::CString::new(key.as_str()).map_err(|_| INVALID)?;
                    let value = std::ffi::CString::new(value.as_str()).map_err(|_| INVALID)?;
                    done(av_dict_set(
                        (*format).metadata_ptr(),
                        key.as_ptr(),
                        value.as_ptr(),
                        0,
                    ))?;
                }
                return match request.operation {
                    1 => probe(call, format),
                    2 => {
                        let align = spec.block_align() as u64;
                        let chunk = (4096 / align).max(1) * align;
                        let mut offset = 0;
                        while offset < reader.data_bytes {
                            if call.cancelled() {
                                return Err(EXIT);
                            }
                            let size = chunk.min(reader.data_bytes - offset);
                            call.message(
                                1,
                                &format!(
                                    "{:.9},{:.9},{},{}",
                                    (offset / align) as f64 / f64::from(spec.sample_rate),
                                    (size / align) as f64 / f64::from(spec.sample_rate),
                                    size,
                                    reader.data_offset + offset
                                ),
                            );
                            offset += size;
                        }
                        Ok(())
                    }
                    3 => crate::media_convert::audio_input(request, call, format, Some(reader)),
                    _ => Err(-1381258232),
                };
            }
            done(avformat_open_input(
                &mut format,
                request.input,
                forced,
                ptr::null_mut(),
            ))?;
            done(avformat_find_stream_info(format, ptr::null_mut()))?;
            match request.operation {
                1 => probe(call, format),
                2 => packets(call, format),
                3 => crate::media_convert::audio(request, call, format),
                4 => crate::media_convert::video(request, call, format),
                5 => cover(request, format),
                _ => Err(EINVAL),
            }
        })();
        avformat_close_input(&mut format);
        result
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
pub unsafe extern "C" fn dvdamedia_run(
    request: *const Request,
    emit: Emit,
    cancel: Cancel,
    state: *mut c_void,
) -> i32 {
    unsafe {
        if request.is_null()
            || (*request).size as usize != std::mem::size_of::<Request>()
            || (*request).abi != 2
            || (*request).input.is_null()
            || emit.is_none()
        {
            return EINVAL;
        }
        let mut call = Call {
            emit,
            cancel,
            state,
        };
        av_log_set_callback(Some(logger));
        let _active = ActiveGuard(ACTIVE.with(|active| active.replace(&call)));
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&*request, &mut call)))
                .unwrap_or(Err(EIO));
        match result {
            Ok(()) => 0,
            Err(error) => {
                if error != EXIT {
                    let mut message = [0i8; 128];
                    av_strerror(error, message.as_mut_ptr(), message.len());
                    call.message(
                        2,
                        &format!(
                            "Error while decoding or processing media: {}",
                            text(message.as_ptr())
                        ),
                    );
                }
                error
            }
        }
    }
}
