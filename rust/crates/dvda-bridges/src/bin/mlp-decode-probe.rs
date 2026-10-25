use dvda_bridges::ffmpeg::*;
use std::{ffi::CString, fs::File, io::Write, ptr};
/// # Safety
/// FFmpeg's libraries must be linked and usable on this thread.
/// `input` is copied into a C string here. Every format, context, frame and packet
/// allocated in this function is released before it returns.
unsafe fn decode(input: &str, output: &str) -> Result<(u64, i32, i32, i32), i32> {
    unsafe {
        let path = CString::new(input).map_err(|_| EINVAL)?;
        let mut format = ptr::null_mut();
        let mut context = ptr::null_mut();
        let mut frame = ptr::null_mut();
        let mut packet = ptr::null_mut();
        let result = (|| {
            done(avformat_open_input(
                &mut format,
                path.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
            ))?;
            done(avformat_find_stream_info(format, ptr::null_mut()))?;
            let mut codec = ptr::null();
            let stream = av_find_best_stream(format, 1, -1, -1, &mut codec, 0);
            done(stream)?;
            context = avcodec_alloc_context3(codec);
            frame = av_frame_alloc();
            packet = av_packet_alloc();
            if context.is_null() || frame.is_null() || packet.is_null() {
                return Err(ENOMEM);
            }
            done(avcodec_parameters_to_context(
                context,
                (*streams(format)[stream as usize]).codecpar(),
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
                        let packed = av_get_packed_sample_fmt(fmt);
                        let depth = if packed == AV_SAMPLE_FMT_S16 {
                            16
                        } else if packed == AV_SAMPLE_FMT_S32 {
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
                        for sample in 0..(*frame).nb_samples() as usize {
                            for channel in 0..ch as usize {
                                let plane = *data.add(if planar { channel } else { 0 });
                                if plane.is_null() {
                                    return Err(INVALID);
                                }
                                let index = if planar {
                                    sample
                                } else {
                                    sample * ch as usize + channel
                                };
                                let value = if depth == 16 {
                                    ptr::read_unaligned(plane.cast::<i16>().add(index)) as i32
                                        * 65536
                                } else {
                                    ptr::read_unaligned(plane.cast::<i32>().add(index))
                                };
                                file.write_all(&value.to_le_bytes()).map_err(|_| EIO)?;
                            }
                        }
                        count += (*frame).nb_samples() as u64;
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
        av_packet_free(&mut packet);
        av_frame_free(&mut frame);
        avcodec_free_context(&mut context);
        avformat_close_input(&mut format);
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
        while let Some(codec) = av_codec_iterate(&mut iterator).as_ref() {
            codecs += 1;
            if minimal && codec.id() != AV_CODEC_ID_MLP {
                return 11;
            }
        }
        iterator = ptr::null_mut();
        let mut demuxers = 0;
        while !av_demuxer_iterate(&mut iterator).is_null() {
            demuxers += 1;
        }
        iterator = ptr::null_mut();
        let mut muxers = 0;
        while !av_muxer_iterate(&mut iterator).is_null() {
            muxers += 1;
        }
        iterator = ptr::null_mut();
        let mut protocols = 0;
        while !avio_enum_protocols(&mut iterator, 0).is_null() {
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
        if (minimal && codecs == 2 && demuxers == 1 && muxers == 1 && protocols == 2)
            || (!minimal
                && !avcodec_find_decoder(AV_CODEC_ID_MLP).is_null()
                && !avcodec_find_encoder(AV_CODEC_ID_MLP).is_null()
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
