/* In-process DVD menu media encoder.
 *
 * This is deliberately a small C bridge.  FFmpeg supplies the MPEG-2 video
 * encoder, MP2 encoder and DVD program-stream muxer, while the surrounding
 * author still owns all menu planning, navigation patches and file layout.
 */
#include "dvda-menu-media.h"

#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#endif

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/audio_fifo.h>
#include <libavutil/channel_layout.h>
#include <libavutil/error.h>
#include <libavutil/frame.h>
#include <libavutil/imgutils.h>
#include <libavutil/opt.h>
#include <libavutil/samplefmt.h>

#ifndef AV_FRAME_FLAG_KEY
#define AV_FRAME_FLAG_KEY 1
#endif

typedef struct MenuVideoInput {
    AVFrame *frame;
    int width;
    int height;
    int fps_num;
    int fps_den;
    char aspect[32];
} MenuVideoInput;

static FILE *open_input(const char *path)
{
#ifdef _WIN32
    int count = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, NULL, 0);
    wchar_t *wide;
    FILE *file;
    if (!count) return NULL;
    wide = malloc((size_t)count * sizeof(*wide));
    if (!wide) return NULL;
    MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, wide, count);
    file = _wfopen(wide, L"rb");
    free(wide);
    return file;
#else
    return fopen(path, "rb");
#endif
}

static void remove_output(const char *path)
{
#ifdef _WIN32
    int count = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, NULL, 0);
    wchar_t *wide;
    if (!count) return;
    wide = malloc((size_t)count * sizeof(*wide));
    if (!wide) return;
    MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, wide, count);
    _wremove(wide);
    free(wide);
#else
    remove(path);
#endif
}

static int parse_size(const char *text, int *value)
{
    char *end;
    long number;
    errno = 0;
    number = strtol(text, &end, 10);
    if (errno || end == text || *end || number < 2 || number > 720) return -1;
    *value = (int)number;
    return 0;
}

static int parse_y4m_token(const char *header, char key, char *value, size_t size)
{
    const char *cursor = header;
    if (!header || !value || size < 2) return -1;
    while (*cursor) {
        const char *start;
        size_t length;
        while (*cursor == ' ' || *cursor == '\t' || *cursor == '\r' || *cursor == '\n') ++cursor;
        if (!*cursor) break;
        start = cursor;
        while (*cursor && *cursor != ' ' && *cursor != '\t' && *cursor != '\r' && *cursor != '\n') ++cursor;
        length = (size_t)(cursor - start);
        if (length >= 2 && start[0] == key) {
            size_t copy = length - 1;
            if (copy >= size) return -1;
            memcpy(value, start + 1, copy);
            value[copy] = 0;
            return 0;
        }
    }
    return -1;
}

static int parse_fraction(const char *text, int *numerator, int *denominator)
{
    char *end;
    long n, d = 1;
    if (!text || !numerator || !denominator) return -1;
    errno = 0;
    n = strtol(text, &end, 10);
    if (errno || end == text || n < 1 || n > INT_MAX) return -1;
    if (*end == ':') {
        char *after;
        errno = 0;
        d = strtol(end + 1, &after, 10);
        if (errno || after == end + 1 || *after || d < 1 || d > INT_MAX) return -1;
    } else if (*end) {
        return -1;
    }
    *numerator = (int)n;
    *denominator = (int)d;
    return 0;
}

static int read_plane(FILE *file, uint8_t *destination, int linesize,
                      int rows, int bytes_per_row)
{
    int row;
    if (!file || !destination || linesize < bytes_per_row || rows < 1 || bytes_per_row < 1)
        return -1;
    for (row = 0; row < rows; ++row) {
        if (fread(destination + (size_t)row * (size_t)linesize, 1,
                  (size_t)bytes_per_row, file) != (size_t)bytes_per_row)
            return -1;
    }
    return 0;
}

static int read_y4m(const char *path, MenuVideoInput *input)
{
    FILE *file = NULL;
    char header[4096];
    char token[64];
    int chroma_width, chroma_height;
    int result = -1;

    if (!path || !input) return -1;
    memset(input, 0, sizeof(*input));
    file = open_input(path);
    if (!file) return -1;
    if (!fgets(header, sizeof(header), file) || strncmp(header, "YUV4MPEG2 ", 10) != 0)
        goto done;
    if (parse_y4m_token(header, 'W', token, sizeof(token)) < 0 ||
        parse_size(token, &input->width) < 0 || input->width != 720 ||
        parse_y4m_token(header, 'H', token, sizeof(token)) < 0 ||
        parse_size(token, &input->height) < 0 ||
        (input->height != 480 && input->height != 576) ||
        parse_y4m_token(header, 'F', token, sizeof(token)) < 0 ||
        parse_fraction(token, &input->fps_num, &input->fps_den) < 0 ||
        parse_y4m_token(header, 'C', token, sizeof(token)) < 0 ||
        strncmp(token, "420", 3) != 0)
        goto done;
    if (parse_y4m_token(header, 'A', input->aspect, sizeof(input->aspect)) < 0)
        strcpy(input->aspect, "1:1");
    if (!fgets(header, sizeof(header), file) || strncmp(header, "FRAME", 5) != 0)
        goto done;

    input->frame = av_frame_alloc();
    if (!input->frame) goto done;
    input->frame->format = AV_PIX_FMT_YUV420P;
    input->frame->width = input->width;
    input->frame->height = input->height;
    if (av_frame_get_buffer(input->frame, 32) < 0 ||
        read_plane(file, input->frame->data[0], input->frame->linesize[0],
                   input->height, input->width) < 0) goto done;
    chroma_width = input->width / 2;
    chroma_height = input->height / 2;
    if (read_plane(file, input->frame->data[1], input->frame->linesize[1],
                   chroma_height, chroma_width) < 0 ||
        read_plane(file, input->frame->data[2], input->frame->linesize[2],
                   chroma_height, chroma_width) < 0) goto done;
    input->frame->pts = 0;
    input->frame->pict_type = AV_PICTURE_TYPE_I;
    input->frame->flags |= AV_FRAME_FLAG_KEY;
    result = 0;
done:
    if (file) fclose(file);
    if (result < 0) av_frame_free(&input->frame);
    return result;
}

static AVRational menu_sample_aspect(const char *norm, const char *aspect)
{
    int ntsc = norm && !strcmp(norm, "ntsc");
    if (!aspect || !strcmp(aspect, "1")) return (AVRational){1, 1};
    if (!strcmp(aspect, "2") || !strcmp(aspect, "4:3"))
        return ntsc ? (AVRational){8, 9} : (AVRational){16, 15};
    if (!strcmp(aspect, "3") || !strcmp(aspect, "16:9"))
        return ntsc ? (AVRational){32, 27} : (AVRational){64, 45};
    if (!strcmp(aspect, "4") || !strcmp(aspect, "2.21:1"))
        return ntsc ? (AVRational){40, 27} : (AVRational){88, 45};
    return (AVRational){1, 1};
}

static int receive_video(AVCodecContext *codec, AVFormatContext *output,
                        AVStream *stream, AVPacket *packet,
                        AVPacket *pending, int *has_pending)
{
    int result;
    while ((result = avcodec_receive_packet(codec, packet)) >= 0) {
        packet->stream_index = stream->index;
        av_packet_rescale_ts(packet, codec->time_base, stream->time_base);
        if (*has_pending) {
            result = av_interleaved_write_frame(output, pending);
            av_packet_unref(pending);
            if (result < 0) {
                av_packet_unref(packet);
                return result;
            }
        }
        av_packet_move_ref(pending, packet);
        *has_pending = 1;
    }
    return result == AVERROR(EAGAIN) || result == AVERROR_EOF ? 0 : result;
}

static int encode_video(MenuVideoInput *input, AVCodecContext *codec,
                        AVFormatContext *output, AVStream *stream)
{
    AVPacket *packet = av_packet_alloc();
    AVPacket *pending = av_packet_alloc();
    static const uint8_t sequence_end_code[4] = {0x00, 0x00, 0x01, 0xB7};
    int has_pending = 0;
    int result;
    if (!packet || !pending) {
        av_packet_free(&pending);
        av_packet_free(&packet);
        return AVERROR(ENOMEM);
    }
    result = avcodec_send_frame(codec, input->frame);
    if (result >= 0)
        result = receive_video(codec, output, stream, packet, pending, &has_pending);
    if (result >= 0) {
        result = avcodec_send_frame(codec, NULL);
        if (result >= 0)
            result = receive_video(codec, output, stream, packet, pending, &has_pending);
    }
    if (result >= 0 && !has_pending) result = AVERROR_INVALIDDATA;
    if (result >= 0) {
        int already_ended = pending->size >= (int)sizeof(sequence_end_code) &&
            memcmp(pending->data + pending->size - sizeof(sequence_end_code),
                   sequence_end_code, sizeof(sequence_end_code)) == 0;
        if (!already_ended) {
            result = av_grow_packet(pending, (int)sizeof(sequence_end_code));
            if (result >= 0)
                memcpy(pending->data + pending->size - sizeof(sequence_end_code),
                       sequence_end_code, sizeof(sequence_end_code));
        }
        if (result >= 0) result = av_interleaved_write_frame(output, pending);
    }
    av_packet_free(&pending);
    av_packet_free(&packet);
    return result;
}

static int write_still_program_end(AVIOContext *io)
{
    uint8_t sector[2048];
    if (!io) return AVERROR(EINVAL);
    avio_flush(io);
    if (io->error < 0) return io->error;
    if ((avio_tell(io) % (int64_t)sizeof(sector)) != 0) return AVERROR_INVALIDDATA;
    memset(sector, 0xFF, sizeof(sector));
    sector[0] = 0x00;
    sector[1] = 0x00;
    sector[2] = 0x01;
    sector[3] = 0xB9;
    avio_write(io, sector, sizeof(sector));
    avio_flush(io);
    return io->error < 0 ? io->error : 0;
}

static int encode_audio_frame(AVCodecContext *codec, AVAudioFifo *fifo,
                              AVFormatContext *output, AVStream *stream,
                              int64_t *samples, int flush)
{
    AVFrame *frame = NULL;
    AVPacket *packet = NULL;
    int frame_size = codec->frame_size ? codec->frame_size : 1152;
    int available = av_audio_fifo_size(fifo);
    int count = flush ? available : frame_size;
    int result = 0;
    if (count < frame_size && !flush) return 0;
    if (count <= 0) return 0;
    if (count > frame_size) count = frame_size;
    frame = av_frame_alloc(); packet = av_packet_alloc();
    if (!frame || !packet) { result = AVERROR(ENOMEM); goto done; }
    frame->nb_samples = count;
    frame->format = codec->sample_fmt;
    frame->sample_rate = codec->sample_rate;
    if (av_channel_layout_copy(&frame->ch_layout, &codec->ch_layout) < 0 ||
        av_frame_get_buffer(frame, 0) < 0 ||
        av_audio_fifo_read(fifo, (void **)frame->data, count) != count) {
        result = AVERROR(EIO); goto done;
    }
    frame->pts = *samples;
    *samples += count;
    result = avcodec_send_frame(codec, frame);
    if (result < 0) goto done;
    while ((result = avcodec_receive_packet(codec, packet)) >= 0) {
        packet->stream_index = stream->index;
        av_packet_rescale_ts(packet, codec->time_base, stream->time_base);
        result = av_interleaved_write_frame(output, packet);
        av_packet_unref(packet);
        if (result < 0) goto done;
    }
    if (result == AVERROR(EAGAIN) || result == AVERROR_EOF) result = 0;
done:
    av_packet_free(&packet);
    av_frame_free(&frame);
    return result;
}

static int flush_audio(AVCodecContext *codec, AVFormatContext *output, AVStream *stream)
{
    AVPacket *packet = av_packet_alloc();
    int result;
    if (!packet) return AVERROR(ENOMEM);
    result = avcodec_send_frame(codec, NULL);
    if (result >= 0) {
        while ((result = avcodec_receive_packet(codec, packet)) >= 0) {
            packet->stream_index = stream->index;
            av_packet_rescale_ts(packet, codec->time_base, stream->time_base);
            result = av_interleaved_write_frame(output, packet);
            av_packet_unref(packet);
            if (result < 0) break;
        }
    }
    av_packet_free(&packet);
    return result == AVERROR(EAGAIN) || result == AVERROR_EOF ? 0 : result;
}

static int encode_audio(const char *path, AVCodecContext *codec,
                        AVFormatContext *output, AVStream *stream)
{
    AVFormatContext *input = NULL;
    AVCodecContext *decoder = NULL;
    AVAudioFifo *fifo = NULL;
    AVPacket *packet = NULL;
    AVFrame *frame = NULL;
    const AVCodec *decoder_codec;
    int stream_index = -1;
    int64_t samples = 0;
    int result = -1, succeeded = 0;
    unsigned i;

    if (!path || !*path) return 0;
    if (avformat_open_input(&input, path, NULL, NULL) < 0 ||
        avformat_find_stream_info(input, NULL) < 0) goto done;
    for (i = 0; i < input->nb_streams; ++i)
        if (input->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) {
            stream_index = (int)i; break;
        }
    if (stream_index < 0) goto done;
    decoder_codec = avcodec_find_decoder(input->streams[stream_index]->codecpar->codec_id);
    decoder = avcodec_alloc_context3(decoder_codec);
    frame = av_frame_alloc(); packet = av_packet_alloc();
    if (!decoder_codec || !decoder || !frame || !packet ||
        avcodec_parameters_to_context(decoder, input->streams[stream_index]->codecpar) < 0 ||
        avcodec_open2(decoder, decoder_codec, NULL) < 0)
        goto done;
    if (decoder->codec_id != AV_CODEC_ID_PCM_S16LE ||
        decoder->sample_rate != 48000 || decoder->sample_fmt != AV_SAMPLE_FMT_S16 ||
        decoder->ch_layout.nb_channels != 2)
        goto done;
    fifo = av_audio_fifo_alloc(AV_SAMPLE_FMT_S16, 2, 4096);
    if (!fifo) goto done;

    while ((result = av_read_frame(input, packet)) >= 0) {
        if (packet->flags & AV_PKT_FLAG_CORRUPT) { result = AVERROR_INVALIDDATA; goto done; }
        if (packet->stream_index == stream_index) {
            result = avcodec_send_packet(decoder, packet);
            if (result >= 0) {
                while ((result = avcodec_receive_frame(decoder, frame)) >= 0) {
                    if (frame->format != AV_SAMPLE_FMT_S16 || frame->sample_rate != 48000 ||
                        frame->ch_layout.nb_channels != 2 ||
                        av_audio_fifo_write(fifo, (void **)frame->data, frame->nb_samples) != frame->nb_samples) {
                        result = AVERROR(EINVAL); break;
                    }
                    while (av_audio_fifo_size(fifo) >= (codec->frame_size ? codec->frame_size : 1152)) {
                        result = encode_audio_frame(codec, fifo, output, stream, &samples, 0);
                        if (result < 0) break;
                    }
                    av_frame_unref(frame);
                    if (result < 0) break;
                }
                if (result == AVERROR(EAGAIN) || result == AVERROR_EOF) result = 0;
            }
        }
        av_packet_unref(packet);
        if (result < 0) goto done;
    }
    if (result != AVERROR_EOF || (input->pb && input->pb->error)) goto done;
    if (avcodec_send_packet(decoder, NULL) < 0) goto done;
    while ((result = avcodec_receive_frame(decoder, frame)) >= 0) {
        if (frame->format != AV_SAMPLE_FMT_S16 || frame->sample_rate != 48000 ||
            frame->ch_layout.nb_channels != 2 ||
            av_audio_fifo_write(fifo, (void **)frame->data, frame->nb_samples) != frame->nb_samples)
            goto done;
        while (av_audio_fifo_size(fifo) >= (codec->frame_size ? codec->frame_size : 1152)) {
            if (encode_audio_frame(codec, fifo, output, stream, &samples, 0) < 0) goto done;
        }
        av_frame_unref(frame);
    }
    if (result != AVERROR(EAGAIN) && result != AVERROR_EOF) goto done;
    if (av_audio_fifo_size(fifo) > 0) {
        int frame_size = codec->frame_size ? codec->frame_size : 1152;
        int missing = frame_size - av_audio_fifo_size(fifo);
        if (missing > 0) {
            uint8_t *silence = NULL;
            if (av_samples_alloc(&silence, NULL, 2, missing, AV_SAMPLE_FMT_S16, 0) < 0) goto done;
            av_samples_set_silence(&silence, 0, missing, 2, AV_SAMPLE_FMT_S16);
            if (av_audio_fifo_write(fifo, (void **)&silence, missing) != missing) {
                av_free(silence); goto done;
            }
            av_free(silence);
        }
        if (encode_audio_frame(codec, fifo, output, stream, &samples, 1) < 0) goto done;
    }
    if (!samples || flush_audio(codec, output, stream) < 0) goto done;
    succeeded = 1;
done:
    av_audio_fifo_free(fifo);
    av_packet_free(&packet);
    av_frame_free(&frame);
    avcodec_free_context(&decoder);
    avformat_close_input(&input);
    return succeeded ? 0 : (result < 0 ? result : AVERROR(EIO));
}

int dvda_menu_create_mpg(const char *y4m_path, const char *wav_path,
                         const char *output_path, const char *norm,
                         const char *aspect, int still_picture)
{
    MenuVideoInput input;
    AVCodecContext *video = NULL, *audio = NULL;
    AVFormatContext *output = NULL;
    AVStream *video_stream = NULL, *audio_stream = NULL;
    const AVCodec *video_codec, *audio_codec;
    AVDictionary *options = NULL;
    int result = -1, succeeded = 0;
    int log_level = av_log_get_level();
    int output_created = 0;

    memset(&input, 0, sizeof(input));
    av_log_set_level(AV_LOG_ERROR);
    if (!output_path || !norm || (strcmp(norm, "pal") && strcmp(norm, "ntsc"))) goto done;
    if (read_y4m(y4m_path, &input) < 0) goto done;
    if (input.width != 720 ||
        input.height != (!strcmp(norm, "ntsc") ? 480 : 576) ||
        av_cmp_q((AVRational){input.fps_num, input.fps_den},
                 !strcmp(norm, "ntsc") ? (AVRational){30000,1001} : (AVRational){25,1}))
        goto done;
    video_codec = avcodec_find_encoder(AV_CODEC_ID_MPEG2VIDEO);
    if (!video_codec) goto done;
    video = avcodec_alloc_context3(video_codec);
    if (!video) goto done;
    video->width = input.width;
    video->height = input.height;
    video->pix_fmt = AV_PIX_FMT_YUV420P;
    video->time_base = (AVRational){input.fps_den, input.fps_num};
    video->framerate = (AVRational){input.fps_num, input.fps_den};
    video->sample_aspect_ratio = menu_sample_aspect(norm, aspect);
    video->bit_rate = 9800000;
    video->rc_min_rate = video->bit_rate;
    video->rc_max_rate = video->bit_rate;
    video->rc_buffer_size = 1835008;
    /* FFmpeg's non-linear MPEG-2 quantisation requires qmax <= 28. */
    video->qmax = 28;
    video->gop_size = 0;
    video->max_b_frames = 0;
    video->flags |= AV_CODEC_FLAG_CLOSED_GOP;
    /* Closed GOPs cannot use scene-change detection in FFmpeg's MPEG-2
       encoder.  A static menu frame is already an explicit GOP boundary. */
    video->profile = AV_PROFILE_MPEG2_MAIN;
    av_dict_set(&options, "video_format", norm && !strcmp(norm, "ntsc") ? "ntsc" : "pal", 0);
    av_dict_set(&options, "seq_disp_ext", "always", 0);
    av_dict_set(&options, "intra_vlc", "1", 0);
    av_dict_set(&options, "non_linear_quant", "1", 0);
    av_dict_set(&options, "sc_threshold", "1000000000", 0);
    if (avcodec_open2(video, video_codec, &options) < 0 || av_dict_count(options)) goto done;
    av_dict_free(&options);

    if (wav_path && *wav_path) {
        audio_codec = avcodec_find_encoder(AV_CODEC_ID_MP2);
        if (!audio_codec) goto done;
        audio = avcodec_alloc_context3(audio_codec);
        if (!audio) goto done;
        audio->sample_fmt = AV_SAMPLE_FMT_S16;
        audio->sample_rate = 48000;
        audio->bit_rate = 224000;
        audio->time_base = (AVRational){1, 48000};
        if (av_channel_layout_from_mask(&audio->ch_layout, AV_CH_LAYOUT_STEREO) < 0 ||
            avcodec_open2(audio, audio_codec, NULL) < 0) goto done;
    }

    if (avformat_alloc_output_context2(&output, NULL, "dvd", output_path) < 0 || !output)
        goto done;
    output->packet_size = 2048;
    /* Keep the DVD menu timeline compatible with the mplex stream that the
       authoring/navigation code was written for.  FFmpeg's MPEG program
       stream muxer defaults to a 500 ms initial demux delay; that moves the
       first subtitle/button packet far beyond the first VOBU on short,
       single-frame menus. */
    if (av_opt_set_int(output->priv_data, "preload", 120000, 0) < 0)
        goto done;
    video_stream = avformat_new_stream(output, NULL);
    if (!video_stream || avcodec_parameters_from_context(video_stream->codecpar, video) < 0)
        goto done;
    video_stream->time_base = video->time_base;
    if (audio) {
        audio_stream = avformat_new_stream(output, NULL);
        if (!audio_stream || avcodec_parameters_from_context(audio_stream->codecpar, audio) < 0)
            goto done;
        audio_stream->time_base = audio->time_base;
    }
    if (!(output->oformat->flags & AVFMT_NOFILE) &&
        avio_open(&output->pb, output_path, AVIO_FLAG_WRITE) < 0) goto done;
    output_created = 1;
    if (avformat_write_header(output, NULL) < 0) goto done;
    result = encode_video(&input, video, output, video_stream);
    if (result < 0) goto done;
    if (audio && (result = encode_audio(wav_path, audio, output, audio_stream)) < 0) goto done;
    if ((result = av_write_trailer(output)) < 0) goto done;
    if (still_picture && (result = write_still_program_end(output->pb)) < 0) goto done;
    succeeded = 1;
done:
    av_dict_free(&options);
    if (output) {
        if (!(output->oformat->flags & AVFMT_NOFILE) && avio_closep(&output->pb) < 0) succeeded = 0;
        avformat_free_context(output);
    }
    avcodec_free_context(&audio);
    avcodec_free_context(&video);
    av_frame_free(&input.frame);
    av_log_set_level(log_level);
    if (!succeeded && output_created) remove_output(output_path);
    return succeeded ? 0 : (result < 0 ? result : AVERROR(EIO));
}
