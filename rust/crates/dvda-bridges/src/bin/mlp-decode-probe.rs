use dvda_bridges::ffmpeg::{
    AV_CODEC_ID_MLP, AV_SAMPLE_FMT_S16, AV_SAMPLE_FMT_S32, EAGAIN, EINVAL, EIO, ENOMEM, EOF,
    INVALID, av_codec_iterate, av_demuxer_iterate, av_find_best_stream, av_frame_alloc,
    av_frame_free, av_frame_unref, av_get_packed_sample_fmt, av_muxer_iterate, av_packet_alloc,
    av_packet_free, av_packet_unref, av_read_frame, av_sample_fmt_is_planar,
    avcodec_alloc_context3, avcodec_find_decoder, avcodec_free_context, avcodec_open2,
    avcodec_parameters_to_context, avcodec_receive_frame, avcodec_send_packet, avcodec_version,
    avformat_close_input, avformat_find_stream_info, avformat_open_input, avformat_version,
    avio_enum_protocols, avutil_version, done, streams,
};
use std::{ffi::CString, fs::File, io::Write, ptr};
/// # Safety
/// `FFmpeg`'s libraries must be linked and usable on this thread.
/// `input` is copied into a C string here. Every format, context, frame and packet
/// allocated in this function is released before it returns.
#[expect(
    clippy::too_many_lines,
    reason = "The diagnostic probe keeps the FFmpeg decode loop and its resource cleanup together"
)]
unsafe fn decode(input: &str, output: &str) -> Result<(u64, i32, i32, i32), i32> {
    unsafe {
        let path = CString::new(input).map_err(|_| EINVAL)?;
        let mut format = ptr::null_mut();
        let mut context = ptr::null_mut();
        let mut frame = ptr::null_mut();
        let mut packet = ptr::null_mut();
        let result = (|| {
            done(avformat_open_input(
                &raw mut format,
                path.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
            ))?;
            done(avformat_find_stream_info(format, ptr::null_mut()))?;
            let mut codec = ptr::null();
            let stream = av_find_best_stream(format, 1, -1, -1, &raw mut codec, 0);
            done(stream)?;
            context = avcodec_alloc_context3(codec);
            frame = av_frame_alloc();
            packet = av_packet_alloc();
            if context.is_null() || frame.is_null() || packet.is_null() {
                return Err(ENOMEM);
            }
            done(avcodec_parameters_to_context(
                context,
                (*streams(format)[usize::try_from(stream).map_err(|_| INVALID)?]).codecpar(),
            ))?;
            done(avcodec_open2(context, codec, ptr::null_mut()))?;
            let mut file = File::create(output).map_err(|_| EIO)?;
            let mut count = 0u64;
            let mut channels = 0;
            let mut rate = 0;
            let mut bits = 0;
            loop {
                let read = av_read_frame(format, packet);
                if read < 0 && read != EOF {
                    return Err(read);
                }
                if read == EOF || (*packet).stream_index() == stream {
                    done(avcodec_send_packet(
                        context,
                        if read == EOF { ptr::null() } else { packet },
                    ))?;
                    loop {
                        let received = avcodec_receive_frame(context, frame);
                        if received == EAGAIN || received == EOF {
                            break;
                        }
                        done(received)?;
                        let ch = (*frame).ch_layout().nb_channels();
                        let sr = (*frame).sample_rate();
                        let fmt = (*frame).format();
                        let base_format = av_get_packed_sample_fmt(fmt);
                        let depth = if base_format == AV_SAMPLE_FMT_S16 {
                            16
                        } else if base_format == AV_SAMPLE_FMT_S32 {
                            32
                        } else {
                            return Err(INVALID);
                        };
                        if ch <= 0 || sr <= 0 || (*frame).nb_samples() < 0 {
                            return Err(INVALID);
                        }
                        if channels == 0 {
                            channels = ch;
                            rate = sr;
                            bits = (*context).bits_per_raw_sample();
                        }
                        if channels != ch || rate != sr {
                            return Err(INVALID);
                        }
                        let planar = av_sample_fmt_is_planar(fmt) != 0;
                        let data = (*frame).extended_data();
                        if data.is_null() {
                            return Err(INVALID);
                        }
                        let sample_count =
                            usize::try_from((*frame).nb_samples()).map_err(|_| INVALID)?;
                        let channel_count = usize::try_from(ch).map_err(|_| INVALID)?;
                        for sample in 0..sample_count {
                            for channel in 0..channel_count {
                                let plane = *data.add(if planar { channel } else { 0 });
                                if plane.is_null() {
                                    return Err(INVALID);
                                }
                                let index = if planar {
                                    sample
                                } else {
                                    sample * channel_count + channel
                                };
                                #[allow(
                                    clippy::cast_ptr_alignment,
                                    reason = "FFmpeg sample planes may be byte-aligned; both typed pointer casts are used only with read_unaligned"
                                )]
                                let value = if depth == 16 {
                                    i32::from(ptr::read_unaligned(plane.cast::<i16>().add(index)))
                                        * 65536
                                } else {
                                    ptr::read_unaligned(plane.cast::<i32>().add(index))
                                };
                                file.write_all(&value.to_le_bytes()).map_err(|_| EIO)?;
                            }
                        }
                        count += u64::try_from((*frame).nb_samples()).map_err(|_| INVALID)?;
                        av_frame_unref(frame);
                    }
                }
                av_packet_unref(packet);
                if read == EOF {
                    break;
                }
            }
            file.flush().map_err(|_| EIO)?;
            if count == 0 {
                return Err(INVALID);
            }
            Ok((count, channels, rate, bits))
        })();
        av_packet_free(&raw mut packet);
        av_frame_free(&raw mut frame);
        avcodec_free_context(&raw mut context);
        avformat_close_input(&raw mut format);
        result
    }
}
fn dll_paths() -> Result<(), i32> {
    for name in ["avcodec-63.dll", "avformat-63.dll", "avutil-61.dll"] {
        match dvda_bridges::win::module_path(name) {
            Some(path) => eprintln!("DLL {name} {path}"),
            None => return Err(10),
        }
    }
    Ok(())
}
fn capabilities(minimal: bool) -> i32 {
    unsafe {
        let mut iterator = ptr::null_mut();
        let mut codecs = 0;
        while let Some(codec) = av_codec_iterate(&raw mut iterator).as_ref() {
            codecs += 1;
            if minimal && codec.id() != AV_CODEC_ID_MLP {
                return 11;
            }
        }
        iterator = ptr::null_mut();
        let mut demuxers = 0;
        while !av_demuxer_iterate(&raw mut iterator).is_null() {
            demuxers += 1;
        }
        iterator = ptr::null_mut();
        let mut muxers = 0;
        while !av_muxer_iterate(&raw mut iterator).is_null() {
            muxers += 1;
        }
        iterator = ptr::null_mut();
        let mut protocols = 0;
        while !avio_enum_protocols(&raw mut iterator, 0).is_null() {
            protocols += 1;
        }
        println!(
            "{{\"codec_version\":{},\"format_version\":{},\"util_version\":{},\"codecs\":{},\"demuxers\":{},\"muxers\":{},\"input_protocols\":{}}}",
            avcodec_version(),
            avformat_version(),
            avutil_version(),
            codecs,
            demuxers,
            muxers,
            protocols
        );
        if (minimal && codecs == 1 && demuxers == 1 && muxers == 1 && protocols == 2)
            || (!minimal
                && !avcodec_find_decoder(AV_CODEC_ID_MLP).is_null()
                && demuxers >= 1
                && muxers >= 1
                && protocols == 2)
        {
            0
        } else {
            12
        }
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if let Err(status) = dll_paths() {
        std::process::exit(status);
    }
    if args.len() == 2 && args[1] == "--capabilities" {
        std::process::exit(capabilities(true));
    }
    if args.len() == 2 && args[1] == "--capabilities-shared" {
        std::process::exit(capabilities(false));
    }
    if args.len() != 3 {
        eprintln!("usage: mlp-decode-probe input output");
        std::process::exit(2);
    }
    match unsafe { decode(&args[1], &args[2]) } {
        Ok((frames, channels, sample_rate, bits)) => println!(
            "{{\"frames\":{frames},\"channels\":{channels},\"sample_rate\":{sample_rate},\"bits\":{bits}}}"
        ),
        Err(error) => {
            eprintln!("decode failed: {error}");
            std::process::exit(1);
        }
    }
}
