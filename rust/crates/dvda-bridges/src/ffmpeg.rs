#![allow(non_snake_case, dead_code)]
use std::ffi::{c_char, c_void};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Rational {
    pub num: i32,
    pub den: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Interrupt {
    pub callback: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub opaque: *mut c_void,
}
include!(concat!(env!("OUT_DIR"), "/ffmpeg-layout.rs"));
pub const EOF: i32 = -541478725;
pub const EXIT: i32 = -1414092869;
pub const INVALID: i32 = -1094995529;
pub const EAGAIN: i32 = -11;
pub const ENOMEM: i32 = -12;
pub const EIO: i32 = -5;
pub const EINVAL: i32 = -22;
unsafe extern "C" {
    pub fn avformat_alloc_context() -> *mut Format;
    pub fn avformat_open_input(
        f: *mut *mut Format,
        path: *const c_char,
        fmt: *const c_void,
        opts: *mut *mut c_void,
    ) -> i32;
    pub fn avformat_find_stream_info(f: *mut Format, opts: *mut *mut c_void) -> i32;
    pub fn avformat_close_input(f: *mut *mut Format);
    pub fn av_find_input_format(name: *const c_char) -> *const c_void;
    pub fn av_read_frame(f: *mut Format, p: *mut Packet) -> i32;
    pub fn av_find_best_stream(
        f: *mut Format,
        kind: i32,
        wanted: i32,
        related: i32,
        codec: *mut *const Codec,
        flags: i32,
    ) -> i32;
    pub fn avformat_alloc_output_context2(
        f: *mut *mut Format,
        fmt: *const c_void,
        name: *const c_char,
        path: *const c_char,
    ) -> i32;
    pub fn avformat_new_stream(f: *mut Format, codec: *const Codec) -> *mut Stream;
    pub fn avformat_free_context(f: *mut Format);
    pub fn avformat_write_header(f: *mut Format, opts: *mut *mut c_void) -> i32;
    pub fn av_write_trailer(f: *mut Format) -> i32;
    pub fn av_interleaved_write_frame(f: *mut Format, p: *mut Packet) -> i32;
    pub fn avio_open2(
        io: *mut *mut Io,
        path: *const c_char,
        flags: i32,
        cb: *const Interrupt,
        opts: *mut *mut c_void,
    ) -> i32;
    pub fn avio_closep(io: *mut *mut Io) -> i32;
    pub fn avio_flush(io: *mut Io);
    pub fn avio_write(io: *mut Io, buf: *const u8, size: i32);
    pub fn avio_seek(io: *mut Io, offset: i64, whence: i32) -> i64;
    pub fn avcodec_find_decoder(id: i32) -> *const Codec;
    pub fn avcodec_find_encoder(id: i32) -> *const Codec;
    pub fn avcodec_alloc_context3(codec: *const Codec) -> *mut Context;
    pub fn avcodec_free_context(c: *mut *mut Context);
    pub fn avcodec_open2(c: *mut Context, codec: *const Codec, opts: *mut *mut c_void) -> i32;
    pub fn avcodec_parameters_to_context(c: *mut Context, p: *const Parameters) -> i32;
    pub fn avcodec_parameters_from_context(p: *mut Parameters, c: *const Context) -> i32;
    pub fn avcodec_parameters_copy(p: *mut Parameters, s: *const Parameters) -> i32;
    pub fn avcodec_send_packet(c: *mut Context, p: *const Packet) -> i32;
    pub fn avcodec_receive_frame(c: *mut Context, f: *mut Frame) -> i32;
    pub fn avcodec_send_frame(c: *mut Context, f: *const Frame) -> i32;
    pub fn avcodec_receive_packet(c: *mut Context, p: *mut Packet) -> i32;
    pub fn avcodec_get_name(id: i32) -> *const c_char;
    pub fn av_packet_alloc() -> *mut Packet;
    pub fn av_packet_free(p: *mut *mut Packet);
    pub fn av_packet_unref(p: *mut Packet);
    pub fn av_packet_clone(p: *const Packet) -> *mut Packet;
    pub fn av_packet_move_ref(d: *mut Packet, s: *mut Packet);
    pub fn av_grow_packet(p: *mut Packet, size: i32) -> i32;
    pub fn av_packet_rescale_ts(p: *mut Packet, src: Rational, dst: Rational);
    pub fn av_frame_alloc() -> *mut Frame;
    pub fn av_frame_free(f: *mut *mut Frame);
    pub fn av_frame_unref(f: *mut Frame);
    pub fn av_frame_get_buffer(f: *mut Frame, align: i32) -> i32;
    pub fn av_channel_layout_copy(d: *mut Layout, s: *const Layout) -> i32;
    pub fn av_channel_layout_from_mask(d: *mut Layout, mask: u64) -> i32;
    pub fn av_dict_set(
        d: *mut *mut c_void,
        key: *const c_char,
        value: *const c_char,
        flags: i32,
    ) -> i32;
    pub fn av_dict_set_int(d: *mut *mut c_void, key: *const c_char, value: i64, flags: i32) -> i32;
    pub fn av_dict_free(d: *mut *mut c_void);
    pub fn av_dict_count(d: *const c_void) -> i32;
    pub fn av_dict_iterate(d: *const c_void, entry: *const DictEntry) -> *const DictEntry;
    pub fn av_opt_set(o: *mut c_void, key: *const c_char, value: *const c_char, flags: i32) -> i32;
    pub fn av_opt_set_int(o: *mut c_void, key: *const c_char, value: i64, flags: i32) -> i32;
    pub fn av_get_media_type_string(kind: i32) -> *const c_char;
    pub fn av_get_bits_per_sample(id: i32) -> i32;
    pub fn av_strerror(err: i32, buf: *mut c_char, size: usize) -> i32;
    pub fn av_rescale(a: i64, b: i64, c: i64) -> i64;
    pub fn av_log_set_callback(
        callback: Option<unsafe extern "C" fn(*mut c_void, i32, *const c_char, *mut c_char)>,
    );
    pub fn av_log_set_level(level: i32);
    pub fn av_log_get_level() -> i32;
    pub fn swr_alloc_set_opts2(
        s: *mut *mut c_void,
        out: *const Layout,
        of: i32,
        or: i32,
        input: *const Layout,
        inf: i32,
        inr: i32,
        offset: i32,
        log: *mut c_void,
    ) -> i32;
    pub fn swr_init(s: *mut c_void) -> i32;
    pub fn swr_free(s: *mut *mut c_void);
    pub fn swr_get_out_samples(s: *mut c_void, input: i32) -> i32;
    pub fn swr_convert(
        s: *mut c_void,
        out: *mut *mut u8,
        out_count: i32,
        input: *const *const u8,
        input_count: i32,
    ) -> i32;
    pub fn av_samples_alloc_array_and_samples(
        data: *mut *mut *mut u8,
        line: *mut i32,
        ch: i32,
        count: i32,
        fmt: i32,
        align: i32,
    ) -> i32;
    pub fn av_free(p: *mut c_void);
    pub fn av_freep(p: *mut c_void);
    pub fn av_audio_fifo_alloc(fmt: i32, ch: i32, size: i32) -> *mut c_void;
    pub fn av_audio_fifo_free(f: *mut c_void);
    pub fn av_audio_fifo_size(f: *mut c_void) -> i32;
    pub fn av_audio_fifo_write(f: *mut c_void, data: *mut *mut c_void, count: i32) -> i32;
    pub fn av_audio_fifo_read(f: *mut c_void, data: *mut *mut c_void, count: i32) -> i32;
    pub fn av_md5_alloc() -> *mut c_void;
    pub fn av_md5_init(p: *mut c_void);
    pub fn av_md5_update(p: *mut c_void, data: *const u8, len: usize);
    pub fn av_md5_final(p: *mut c_void, out: *mut u8);
    pub fn sws_getContext(
        w: i32,
        h: i32,
        fmt: i32,
        ow: i32,
        oh: i32,
        of: i32,
        flags: i32,
        a: *const c_void,
        b: *const c_void,
        p: *const f64,
    ) -> *mut c_void;
    pub fn sws_scale(
        s: *mut c_void,
        input: *const *const u8,
        lines: *const i32,
        start: i32,
        height: i32,
        out: *const *mut u8,
        olines: *const i32,
    ) -> i32;
    pub fn sws_freeContext(s: *mut c_void);
    pub fn av_sample_fmt_is_planar(f: i32) -> i32;
    pub fn av_get_packed_sample_fmt(f: i32) -> i32;
    pub fn av_codec_iterate(i: *mut *mut c_void) -> *const Codec;
    pub fn av_demuxer_iterate(i: *mut *mut c_void) -> *const c_void;
    pub fn av_muxer_iterate(i: *mut *mut c_void) -> *const c_void;
    pub fn avio_enum_protocols(i: *mut *mut c_void, out: i32) -> *const c_char;
    pub fn avcodec_version() -> u32;
    pub fn avformat_version() -> u32;
    pub fn avutil_version() -> u32;
}
#[repr(C)]
pub struct DictEntry {
    pub key: *const c_char,
    pub value: *const c_char,
}
/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
pub unsafe fn streams(f: *mut Format) -> Vec<*mut Stream> {
    unsafe { std::slice::from_raw_parts((*f).streams(), (*f).nb_streams() as usize).to_vec() }
}
/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
pub unsafe fn text(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned() }
    }
}
/// # Safety
/// Pointer arguments must be valid for their documented ABI and remain live throughout the call.
pub unsafe fn done(v: i32) -> Result<(), i32> {
    if v < 0 { Err(v) } else { Ok(()) }
}
