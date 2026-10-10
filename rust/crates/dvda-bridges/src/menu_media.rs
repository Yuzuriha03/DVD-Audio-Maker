#![allow(unsafe_op_in_unsafe_fn)]
use crate::ffmpeg::*;
use std::{
    ffi::{CStr, c_char, c_void},
    io::{BufRead, BufReader, Read},
    ptr,
};
struct State {
    frame: *mut Frame,
    video: *mut Context,
    audio: *mut Context,
    output: *mut Format,
}
impl Drop for State {
    fn drop(&mut self) {
        unsafe {
            if !self.output.is_null() {
                avio_closep((*self.output).pb_ptr());
                avformat_free_context(self.output);
            }
            avcodec_free_context(&mut self.video);
            avcodec_free_context(&mut self.audio);
            av_frame_free(&mut self.frame);
        }
    }
}
unsafe fn packets(
    codec: *mut Context,
    output: *mut Format,
    index: usize,
    frame: *mut Frame,
    end: bool,
) -> Result<(), i32> {
    done(avcodec_send_frame(codec, frame))?;
    let mut packet = av_packet_alloc();
    if packet.is_null() {
        return Err(ENOMEM);
    }
    let result = (|| {
        loop {
            let status = avcodec_receive_packet(codec, packet);
            if status == EOF || status == EAGAIN {
                return Ok(());
            }
            done(status)?;
            if end {
                let bytes = std::slice::from_raw_parts((*packet).data(), (*packet).size() as usize);
                if !bytes.ends_with(&[0, 0, 1, 0xb7]) {
                    done(av_grow_packet(packet, 4))?;
                    ptr::copy_nonoverlapping(
                        [0, 0, 1, 0xb7].as_ptr(),
                        (*packet).data().add((*packet).size() as usize - 4),
                        4,
                    );
                }
            }
            *(*packet).stream_index_ptr() = index as i32;
            av_packet_rescale_ts(
                packet,
                (*codec).time_base(),
                (*streams(output)[index]).time_base(),
            );
            done(av_interleaved_write_frame(output, packet))?;
            av_packet_unref(packet);
        }
    })();
    av_packet_free(&mut packet);
    result
}
unsafe fn audio(path: *const c_char, codec: *mut Context, output: *mut Format) -> Result<(), i32> {
    struct Decode {
        input: *mut Format,
        decoder: *mut Context,
        frame: *mut Frame,
        packet: *mut Packet,
        fifo: *mut c_void,
    }
    impl Drop for Decode {
        fn drop(&mut self) {
            unsafe {
                avformat_close_input(&mut self.input);
                avcodec_free_context(&mut self.decoder);
                av_frame_free(&mut self.frame);
                av_packet_free(&mut self.packet);
                av_audio_fifo_free(self.fifo);
            }
        }
    }
    let mut d = Decode {
        input: ptr::null_mut(),
        decoder: ptr::null_mut(),
        frame: av_frame_alloc(),
        packet: av_packet_alloc(),
        fifo: av_audio_fifo_alloc(AV_SAMPLE_FMT_S16, 2, 4096),
    };
    if d.frame.is_null() || d.packet.is_null() || d.fifo.is_null() {
        return Err(ENOMEM);
    }
    done(avformat_open_input(
        &mut d.input,
        path,
        ptr::null(),
        ptr::null_mut(),
    ))?;
    done(avformat_find_stream_info(d.input, ptr::null_mut()))?;
    let index = streams(d.input)
        .iter()
        .position(|stream| (*(**stream).codecpar()).codec_type() == 1)
        .ok_or(INVALID)?;
    let parameters = (*streams(d.input)[index]).codecpar();
    if (*parameters).codec_id() != AV_CODEC_ID_PCM_S16LE {
        return Err(-1);
    }
    let decoder = avcodec_find_decoder((*parameters).codec_id());
    if decoder.is_null() {
        return Err(INVALID);
    }
    d.decoder = avcodec_alloc_context3(decoder);
    if d.decoder.is_null() {
        return Err(ENOMEM);
    }
    done(avcodec_parameters_to_context(d.decoder, parameters))?;
    done(avcodec_open2(d.decoder, decoder, ptr::null_mut()))?;
    if (*d.decoder).sample_rate() != 48000
        || (*d.decoder).sample_fmt() != AV_SAMPLE_FMT_S16
        || (*d.decoder).ch_layout().nb_channels() != 2
    {
        return Err(-1);
    }
    let mut samples = 0i64;
    let consume = |d: &mut Decode, samples: &mut i64| -> Result<(), i32> {
        loop {
            let status = avcodec_receive_frame(d.decoder, d.frame);
            if status == EOF || status == EAGAIN {
                return Ok(());
            }
            done(status)?;
            if (*d.frame).format() != AV_SAMPLE_FMT_S16
                || (*d.frame).sample_rate() != 48000
                || (*d.frame).ch_layout().nb_channels() != 2
            {
                return Err(EINVAL);
            }
            let count = (*d.frame).nb_samples();
            if av_audio_fifo_write(d.fifo, (*d.frame).data().as_mut_ptr().cast(), count) != count {
                return Err(EIO);
            }
            av_frame_unref(d.frame);
            while av_audio_fifo_size(d.fifo) >= audio_frame_size(codec) {
                encode_audio(d.fifo, codec, output, samples)?;
            }
        }
    };
    loop {
        let status = av_read_frame(d.input, d.packet);
        if status == EOF {
            break;
        }
        done(status)?;
        if (*d.packet).flags() & 2 != 0 {
            return Err(INVALID);
        }
        if (*d.packet).stream_index() == index as i32 {
            done(avcodec_send_packet(d.decoder, d.packet))?;
            consume(&mut d, &mut samples)?;
        }
        av_packet_unref(d.packet);
    }
    if !(*d.input).pb().is_null() {
        done((*(*d.input).pb()).error())?;
    }
    done(avcodec_send_packet(d.decoder, ptr::null()))?;
    consume(&mut d, &mut samples)?;
    let remaining = av_audio_fifo_size(d.fifo);
    if remaining > 0 {
        let missing = audio_frame_size(codec) - remaining;
        let mut silence = vec![0u8; missing as usize * 4];
        let mut plane = silence.as_mut_ptr();
        if av_audio_fifo_write(d.fifo, (&mut plane as *mut *mut u8).cast(), missing) != missing {
            return Err(EIO);
        }
        encode_audio(d.fifo, codec, output, &mut samples)?;
    }
    if samples == 0 {
        return Err(EOF);
    }
    packets(codec, output, 1, ptr::null_mut(), false)
}
unsafe fn audio_frame_size(codec: *mut Context) -> i32 {
    let n = (*codec).frame_size();
    if n > 0 { n } else { 1152 }
}
unsafe fn encode_audio(
    fifo: *mut c_void,
    codec: *mut Context,
    output: *mut Format,
    samples: &mut i64,
) -> Result<(), i32> {
    let mut frame = av_frame_alloc();
    if frame.is_null() {
        return Err(ENOMEM);
    }
    let result = (|| {
        let count = audio_frame_size(codec);
        *(*frame).nb_samples_ptr() = count;
        *(*frame).format_ptr() = AV_SAMPLE_FMT_S16;
        *(*frame).sample_rate_ptr() = 48000;
        *(*frame).pts_ptr() = *samples;
        *samples += count as i64;
        done(av_channel_layout_copy(
            (*frame).ch_layout_ptr(),
            (*codec).ch_layout_ptr(),
        ))?;
        done(av_frame_get_buffer(frame, 0))?;
        if av_audio_fifo_read(fifo, (*frame).data().as_mut_ptr().cast(), count) != count {
            return Err(EIO);
        }
        packets(codec, output, 1, frame, false)
    })();
    av_frame_free(&mut frame);
    result
}
unsafe fn create(
    y4m: *const c_char,
    wav: *const c_char,
    path: *const c_char,
    norm: *const c_char,
    aspect: *const c_char,
    still: i32,
) -> Result<(), i32> {
    if y4m.is_null() || path.is_null() || norm.is_null() {
        return Err(EINVAL);
    }
    let norm = CStr::from_ptr(norm).to_str().map_err(|_| EINVAL)?;
    let ntsc = match norm {
        "ntsc" => true,
        "pal" => false,
        _ => return Err(EINVAL),
    };
    let input = CStr::from_ptr(y4m).to_str().map_err(|_| EINVAL)?;
    let mut reader = BufReader::new(std::fs::File::open(input).map_err(|_| -1)?);
    let mut header = String::new();
    reader
        .by_ref()
        .take(4095)
        .read_line(&mut header)
        .map_err(|_| EIO)?;
    if !header.starts_with("YUV4MPEG2 ") {
        return Err(INVALID);
    }
    let token = |key: char| {
        header
            .split_ascii_whitespace()
            .find_map(|word| word.strip_prefix(key))
    };
    let width = token('W')
        .and_then(|v| v.parse::<i32>().ok())
        .ok_or(INVALID)?;
    let height = token('H')
        .and_then(|v| v.parse::<i32>().ok())
        .ok_or(INVALID)?;
    let fraction = token('F').ok_or(INVALID)?;
    let mut fraction = fraction.split(':');
    let num = fraction
        .next()
        .and_then(|v| v.parse::<i32>().ok())
        .ok_or(INVALID)?;
    let den = fraction
        .next()
        .unwrap_or("1")
        .parse::<i32>()
        .map_err(|_| INVALID)?;
    if width != 720
        || height != if ntsc { 480 } else { 576 }
        || num <= 0
        || den <= 0
        || num as i64 * if ntsc { 1001 } else { 1 } != den as i64 * if ntsc { 30000 } else { 25 }
        || !token('C').is_some_and(|v| v.starts_with("420"))
    {
        return Err(INVALID);
    }
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|_| EIO)?;
    if !line.starts_with("FRAME") {
        return Err(INVALID);
    }
    let mut s = State {
        frame: av_frame_alloc(),
        video: ptr::null_mut(),
        audio: ptr::null_mut(),
        output: ptr::null_mut(),
    };
    if s.frame.is_null() {
        return Err(ENOMEM);
    }
    *(*s.frame).format_ptr() = AV_PIX_FMT_YUV420P;
    *(*s.frame).width_ptr() = width;
    *(*s.frame).height_ptr() = height;
    done(av_frame_get_buffer(s.frame, 32))?;
    for plane in 0..3 {
        let w = if plane == 0 { width } else { width / 2 };
        let h = if plane == 0 { height } else { height / 2 };
        for row in 0..h {
            let destination = std::slice::from_raw_parts_mut(
                (*s.frame).data()[plane].add((row * (*s.frame).linesize()[plane]) as usize),
                w as usize,
            );
            reader.read_exact(destination).map_err(|_| EIO)?;
        }
    }
    *(*s.frame).pts_ptr() = 0;
    *(*s.frame).pict_type_ptr() = 1;
    *(*s.frame).flags_ptr() |= 1;
    let video = avcodec_find_encoder(AV_CODEC_ID_MPEG2VIDEO);
    if video.is_null() {
        return Err(INVALID);
    }
    s.video = avcodec_alloc_context3(video);
    if s.video.is_null() {
        return Err(ENOMEM);
    }
    *(*s.video).width_ptr() = width;
    *(*s.video).height_ptr() = height;
    *(*s.video).pix_fmt_ptr() = AV_PIX_FMT_YUV420P;
    *(*s.video).time_base_ptr() = Rational { num: den, den: num };
    *(*s.video).framerate_ptr() = Rational { num, den };
    let aspect = if aspect.is_null() {
        "1"
    } else {
        CStr::from_ptr(aspect).to_str().unwrap_or("1")
    };
    let sar = match aspect {
        "2" | "4:3" => {
            if ntsc {
                (8, 9)
            } else {
                (16, 15)
            }
        }
        "3" | "16:9" => {
            if ntsc {
                (32, 27)
            } else {
                (64, 45)
            }
        }
        "4" | "2.21:1" => {
            if ntsc {
                (40, 27)
            } else {
                (88, 45)
            }
        }
        _ => (1, 1),
    };
    *(*s.video).sample_aspect_ratio_ptr() = Rational {
        num: sar.0,
        den: sar.1,
    };
    *(*s.video).bit_rate_ptr() = 9800000;
    *(*s.video).rc_min_rate_ptr() = 9800000;
    *(*s.video).rc_max_rate_ptr() = 9800000;
    *(*s.video).rc_buffer_size_ptr() = 1835008;
    *(*s.video).qmax_ptr() = 28;
    *(*s.video).gop_size_ptr() = 0;
    *(*s.video).max_b_frames_ptr() = 0;
    *(*s.video).flags_ptr() |= AV_CODEC_FLAG_CLOSED_GOP;
    *(*s.video).profile_ptr() = 4;
    let mut options = ptr::null_mut();
    for (key, value) in [
        (c"video_format", if ntsc { c"ntsc" } else { c"pal" }),
        (c"seq_disp_ext", c"always"),
        (c"intra_vlc", c"1"),
        (c"non_linear_quant", c"1"),
        (c"sc_threshold", c"1000000000"),
    ] {
        av_dict_set(&mut options, key.as_ptr(), value.as_ptr(), 0);
    }
    let status = avcodec_open2(s.video, video, &mut options);
    let unused = av_dict_count(options);
    av_dict_free(&mut options);
    done(status)?;
    if unused != 0 {
        return Err(INVALID);
    }
    if !wav.is_null() && *wav != 0 {
        let codec = avcodec_find_encoder(AV_CODEC_ID_MP2);
        if codec.is_null() {
            return Err(INVALID);
        }
        s.audio = avcodec_alloc_context3(codec);
        if s.audio.is_null() {
            return Err(ENOMEM);
        }
        *(*s.audio).sample_fmt_ptr() = AV_SAMPLE_FMT_S16;
        *(*s.audio).sample_rate_ptr() = 48000;
        *(*s.audio).bit_rate_ptr() = 224000;
        *(*s.audio).time_base_ptr() = Rational { num: 1, den: 48000 };
        done(av_channel_layout_from_mask((*s.audio).ch_layout_ptr(), 3))?;
        done(avcodec_open2(s.audio, codec, ptr::null_mut()))?;
    }
    done(avformat_alloc_output_context2(
        &mut s.output,
        ptr::null(),
        c"dvd".as_ptr(),
        path,
    ))?;
    *(*s.output).packet_size_ptr() = 2048;
    done(av_opt_set_int(
        (*s.output).priv_data(),
        c"preload".as_ptr(),
        120000,
        0,
    ))?;
    for codec in [s.video, s.audio] {
        if codec.is_null() {
            continue;
        }
        let stream = avformat_new_stream(s.output, ptr::null());
        if stream.is_null() {
            return Err(ENOMEM);
        }
        done(avcodec_parameters_from_context((*stream).codecpar(), codec))?;
        *(*stream).time_base_ptr() = (*codec).time_base();
    }
    done(avio_open2(
        (*s.output).pb_ptr(),
        path,
        2,
        ptr::null(),
        ptr::null_mut(),
    ))?;
    let result = (|| {
        done(avformat_write_header(s.output, ptr::null_mut()))?;
        packets(s.video, s.output, 0, s.frame, true)?;
        packets(s.video, s.output, 0, ptr::null_mut(), true)?;
        if !s.audio.is_null() {
            audio(wav, s.audio, s.output)?;
        }
        done(av_write_trailer(s.output))?;
        if still != 0 {
            let io = (*s.output).pb();
            avio_flush(io);
            done((*io).error())?;
            let position = avio_seek(io, 0, 1);
            if position < 0 || position % 2048 != 0 {
                return Err(INVALID);
            }
            let mut sector = [255u8; 2048];
            sector[..4].copy_from_slice(&[0, 0, 1, 0xb9]);
            avio_write(io, sector.as_ptr(), 2048);
            avio_flush(io);
            done((*io).error())?;
        }
        done(avio_closep((*s.output).pb_ptr()))
    })();
    if result.is_err() {
        avio_closep((*s.output).pb_ptr());
        if let Ok(path) = CStr::from_ptr(path).to_str() {
            let _ = std::fs::remove_file(path);
        }
    }
    result
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
pub unsafe extern "C" fn dvda_menu_create_mpg(
    y4m: *const c_char,
    wav: *const c_char,
    path: *const c_char,
    norm: *const c_char,
    aspect: *const c_char,
    still: i32,
) -> i32 {
    let level = av_log_get_level();
    av_log_set_level(16);
    let result = std::panic::catch_unwind(|| create(y4m, wav, path, norm, aspect, still));
    av_log_set_level(level);
    match result {
        Ok(Ok(())) => 0,
        // The legacy menu bridge exposes one generic failure status for all
        // rejected inputs and output paths.
        Ok(Err(_)) => -1,
        Err(_) => -1,
    }
}
