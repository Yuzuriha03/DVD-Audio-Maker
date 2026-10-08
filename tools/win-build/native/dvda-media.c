/* In-process Windows x64 media bridge. No command line executable is spawned. */
#include <windows.h>
#include <stdint.h>
#include <limits.h>
#include <stdio.h>
#include <stdarg.h>
#include <math.h>
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/audio_fifo.h>
#include <libavutil/bprint.h>
#include <libavutil/md5.h>
#include <libavutil/opt.h>
#include <libswresample/swresample.h>
#include <libswscale/swscale.h>

typedef void (__cdecl *Emit)(void *, int, const char *);
typedef int (__cdecl *Cancel)(void *);
typedef struct Request {
    uint32_t size, abi, operation, rate, bits, output_format, soxr, compression, cover, tag_count;
    const char *input, *output;
    const char *const *tags;
} Request;
typedef struct Call { const Request *request; Emit emit; Cancel cancel; void *state; } Call;
static _Thread_local Call *active;
static INIT_ONCE once = INIT_ONCE_STATIC_INIT;

static int worker_count(void)
{
    DWORD processors = GetActiveProcessorCount(ALL_PROCESSOR_GROUPS);
    if (!processors) processors = 1;
    return processors > INT_MAX / 2 ? INT_MAX : (int)processors * 2;
}

static int cancelled(void *opaque) { Call *call = opaque; return call->cancel && call->cancel(call->state); }
static void message(Call *call, int stream, const char *format, ...)
{
    char text[2048]; va_list args; va_start(args, format);
    vsnprintf(text, sizeof(text), format, args); va_end(args);
    call->emit(call->state, stream, text);
}
static void logger(void *context, int level, const char *format, va_list args)
{
    (void)context;
    if (active && level <= AV_LOG_ERROR) {
        char text[2048]; vsnprintf(text, sizeof(text), format, args);
        active->emit(active->state, 2, text);
    }
}
static BOOL CALLBACK initialize(PINIT_ONCE unused, PVOID arg, PVOID *context)
{
    (void)unused; (void)arg; (void)context;
    av_log_set_callback(logger); return TRUE;
}
static void quoted(AVBPrint *buffer, const char *text)
{
    av_bprintf(buffer, "\"");
    for (const unsigned char *p = (const unsigned char *)(text ? text : ""); *p; ++p) {
        if (*p == '"' || *p == '\\') av_bprintf(buffer, "\\%c", *p);
        else if (*p < 32) av_bprintf(buffer, "\\u%04x", *p);
        else av_bprint_chars(buffer, *p, 1);
    }
    av_bprintf(buffer, "\"");
}
static void dictionary(AVBPrint *buffer, AVDictionary *metadata)
{
    const AVDictionaryEntry *entry = NULL; int count = 0;
    av_bprintf(buffer, "{");
    while ((entry = av_dict_iterate(metadata, entry))) {
        if (count++) av_bprintf(buffer, ",");
        quoted(buffer, entry->key); av_bprintf(buffer, ":"); quoted(buffer, entry->value);
    }
    av_bprintf(buffer, "}");
}
static int open_input(Call *call, AVFormatContext **format)
{
    *format = avformat_alloc_context();
    if (!*format) return AVERROR(ENOMEM);
    (*format)->interrupt_callback = (AVIOInterruptCB){cancelled, call};
    const char *extension = strrchr(call->request->input, '.');
    const AVInputFormat *forced = extension && !_stricmp(extension, ".mlp") ? av_find_input_format("mlp") : NULL;
    int result = avformat_open_input(format, call->request->input, forced, NULL);
    return result < 0 ? result : avformat_find_stream_info(*format, NULL);
}
static int probe(Call *call, AVFormatContext *format)
{
    AVBPrint buffer; av_bprint_init(&buffer, 4096, AV_BPRINT_SIZE_UNLIMITED);
    av_bprintf(&buffer, "{\"streams\":[");
    for (unsigned i = 0; i < format->nb_streams; ++i) {
        AVStream *stream = format->streams[i]; AVCodecParameters *p = stream->codecpar;
        if (i) av_bprintf(&buffer, ",");
        av_bprintf(&buffer, "{\"index\":%u,\"codec_type\":", i); quoted(&buffer, av_get_media_type_string(p->codec_type));
        av_bprintf(&buffer, ",\"codec_name\":"); quoted(&buffer, avcodec_get_name(p->codec_id));
        av_bprintf(&buffer, ",\"sample_rate\":%d,\"channels\":%d,\"bits_per_raw_sample\":%d,\"width\":%d,\"height\":%d,\"duration_ts\":",
                   p->sample_rate, p->ch_layout.nb_channels, p->bits_per_raw_sample ? p->bits_per_raw_sample : av_get_bits_per_sample(p->codec_id), p->width, p->height);
        if (stream->duration == AV_NOPTS_VALUE) av_bprintf(&buffer, "null");
        else av_bprintf(&buffer, "%lld", (long long)stream->duration);
        av_bprintf(&buffer, ",\"time_base\":\"%d/%d\",\"tags\":", stream->time_base.num, stream->time_base.den);
        dictionary(&buffer, stream->metadata); av_bprintf(&buffer, "}");
    }
    av_bprintf(&buffer, "],\"format\":{\"duration\":");
    if (format->duration == AV_NOPTS_VALUE) av_bprintf(&buffer, "null");
    else av_bprintf(&buffer, "%.9f", (double)format->duration / AV_TIME_BASE);
    av_bprintf(&buffer, ",\"tags\":"); dictionary(&buffer, format->metadata); av_bprintf(&buffer, "}}");
    int result = av_bprint_is_complete(&buffer) ? 0 : AVERROR(ENOMEM);
    if (!result) call->emit(call->state, 1, buffer.str);
    av_bprint_finalize(&buffer, NULL); return result;
}
static int packets(Call *call, AVFormatContext *format)
{
    int stream = AVERROR_STREAM_NOT_FOUND;
    for (unsigned i = 0; i < format->nb_streams; ++i)
        if (format->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) { stream = (int)i; break; }
    if (stream < 0) return stream;
    AVPacket *packet = av_packet_alloc(); if (!packet) return AVERROR(ENOMEM);
    int result = AVERROR_EXIT;
    while (!cancelled(call) && (result = av_read_frame(format, packet)) >= 0) {
        if (packet->stream_index == stream)
            message(call, 1, "%.9f,%.9f,%d,%lld", packet->pts == AV_NOPTS_VALUE ? 0.0 : packet->pts * av_q2d(format->streams[stream]->time_base),
                    packet->duration * av_q2d(format->streams[stream]->time_base), packet->size, (long long)packet->pos);
        av_packet_unref(packet);
    }
    av_packet_free(&packet);
    return cancelled(call) ? AVERROR_EXIT : result == AVERROR_EOF ? 0 : result;
}
static FILE *utf8_write(const char *path)
{
    int size = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, NULL, 0);
    if (!size) return NULL;
    wchar_t *wide = av_malloc_array(size, sizeof(wchar_t)); if (!wide) return NULL;
    MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, wide, size);
    FILE *file = _wfopen(wide, L"wb"); av_free(wide); return file;
}
static int cover(Call *call, AVFormatContext *format)
{
    for (unsigned i = 0; i < format->nb_streams; ++i) {
        AVStream *stream = format->streams[i];
        if (!(stream->disposition & AV_DISPOSITION_ATTACHED_PIC)) continue;
        FILE *file = utf8_write(call->request->output); if (!file) return AVERROR(errno);
        int okay = fwrite(stream->attached_pic.data, 1, stream->attached_pic.size, file) == (size_t)stream->attached_pic.size;
        if (fclose(file)) okay = 0;
        return okay ? 0 : AVERROR(EIO);
    }
    return AVERROR_STREAM_NOT_FOUND;
}
typedef struct Audio {
    Call *call; AVFormatContext *input, *output; AVCodecContext *decoder, *encoder;
    SwrContext *swr; AVAudioFifo *fifo; AVFrame *frame; AVPacket *packet;
    FILE *raw; struct AVMD5 *md5; int rate, channels, bits, stream, output_kind, cover_stream;
    enum AVSampleFormat format; int64_t samples, progress_samples; int initialized;
} Audio;
static void report_audio_progress(Audio *audio, int force)
{
    int64_t interval = audio->rate > 0 ? audio->rate / 4 : 1;
    if (force || audio->samples - audio->progress_samples >= interval) {
        audio->progress_samples = audio->samples;
        /* Per-frame callbacks can overwhelm the GUI and its log file. */
        message(audio->call, 3, "out_time_us=%lld", (long long)av_rescale(audio->samples, AV_TIME_BASE, audio->rate));
    }
}
static int encode_frame(Audio *audio, AVFrame *frame)
{
    int result = avcodec_send_frame(audio->encoder, frame);
    if (result < 0) return result;
    AVPacket *packet = av_packet_alloc(); if (!packet) return AVERROR(ENOMEM);
    while ((result = avcodec_receive_packet(audio->encoder, packet)) >= 0) {
        av_packet_rescale_ts(packet, audio->encoder->time_base, audio->output->streams[0]->time_base);
        packet->stream_index = 0;
        result = av_interleaved_write_frame(audio->output, packet);
        av_packet_unref(packet); if (result < 0) break;
    }
    av_packet_free(&packet);
    return result == AVERROR(EAGAIN) || result == AVERROR_EOF ? 0 : result;
}
static int encode_fifo(Audio *audio, int flush)
{
    int size = audio->encoder->frame_size ? audio->encoder->frame_size : 4096;
    while (av_audio_fifo_size(audio->fifo) >= size || (flush && av_audio_fifo_size(audio->fifo))) {
        if (cancelled(audio->call)) return AVERROR_EXIT;
        int count = FFMIN(size, av_audio_fifo_size(audio->fifo));
        AVFrame *frame = av_frame_alloc(); if (!frame) return AVERROR(ENOMEM);
        frame->nb_samples = count; frame->format = audio->format; frame->sample_rate = audio->rate;
        frame->pts = audio->encoder->frame_num ? audio->samples - av_audio_fifo_size(audio->fifo) : 0;
        int result = av_channel_layout_copy(&frame->ch_layout, &audio->encoder->ch_layout);
        if (result >= 0) result = av_frame_get_buffer(frame, 0);
        if (result >= 0 && av_audio_fifo_read(audio->fifo, (void **)frame->data, count) != count) result = AVERROR(EIO);
        if (result >= 0) result = encode_frame(audio, frame);
        av_frame_free(&frame); if (result < 0) return result;
    }
    return 0;
}
static int initialize_audio(Audio *audio, AVFrame *frame)
{
    const Request *request = audio->call->request;
    audio->channels = frame->ch_layout.nb_channels;
    audio->rate = request->rate ? (int)request->rate : frame->sample_rate;
    audio->bits = request->bits ? (int)request->bits : (audio->decoder->bits_per_raw_sample > 16 ? 24 : 16);
    audio->format = audio->bits == 16 ? AV_SAMPLE_FMT_S16 : AV_SAMPLE_FMT_S32;
    enum AVSampleFormat resampled = audio->bits == 20 ? AV_SAMPLE_FMT_DBLP : audio->format;
    int result = swr_alloc_set_opts2(&audio->swr, &frame->ch_layout, resampled, audio->rate,
                                   &frame->ch_layout, frame->format, frame->sample_rate, 0, NULL);
    if (result < 0) return result;
    if ((result = av_opt_set(audio->swr, "dither_method", "none", 0)) < 0) return result;
    if (request->soxr && (result = av_opt_set(audio->swr, "resampler", "soxr", 0)) < 0) return result;
    if ((result = swr_init(audio->swr)) < 0) return result;
    if (audio->output_kind == 1 || audio->output_kind == 6) {
        const AVCodec *codec = avcodec_find_encoder(audio->output_kind == 6 ? AV_CODEC_ID_FLAC :
                                                  audio->bits == 16 ? AV_CODEC_ID_PCM_S16LE : AV_CODEC_ID_PCM_S24LE);
        if (!codec) return AVERROR_ENCODER_NOT_FOUND;
        audio->encoder = avcodec_alloc_context3(codec); if (!audio->encoder) return AVERROR(ENOMEM);
        audio->encoder->sample_fmt = audio->format; audio->encoder->sample_rate = audio->rate;
        audio->encoder->bits_per_raw_sample = audio->output_kind == 6 ? audio->bits : audio->bits == 16 ? 16 : 24;
        audio->encoder->time_base = (AVRational){1, audio->rate}; audio->encoder->thread_count = worker_count();
        if ((result = av_channel_layout_copy(&audio->encoder->ch_layout, &frame->ch_layout)) < 0) return result;
        if ((result = avformat_alloc_output_context2(&audio->output, NULL, audio->output_kind == 6 ? "flac" : "wav", request->output)) < 0) return result;
        audio->output->interrupt_callback = (AVIOInterruptCB){cancelled, audio->call};
        if (audio->output->oformat->flags & AVFMT_GLOBALHEADER) audio->encoder->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
        AVDictionary *options = NULL;
        if (audio->output_kind == 6) av_dict_set_int(&options, "compression_level", request->compression, 0);
        result = avcodec_open2(audio->encoder, codec, &options); av_dict_free(&options); if (result < 0) return result;
        AVStream *stream = avformat_new_stream(audio->output, NULL); if (!stream) return AVERROR(ENOMEM);
        stream->time_base = audio->encoder->time_base;
        if ((result = avcodec_parameters_from_context(stream->codecpar, audio->encoder)) < 0) return result;
        for (unsigned i = 0; i < request->tag_count; ++i)
            av_dict_set(&audio->output->metadata, request->tags[2 * i], request->tags[2 * i + 1], 0);
        if (request->cover && audio->output_kind == 6) {
            for (unsigned i = 0; i < audio->input->nb_streams; ++i) {
                AVStream *source = audio->input->streams[i];
                if (!(source->disposition & AV_DISPOSITION_ATTACHED_PIC)) continue;
                AVStream *picture = avformat_new_stream(audio->output, NULL); if (!picture) return AVERROR(ENOMEM);
                if ((result = avcodec_parameters_copy(picture->codecpar, source->codecpar)) < 0) return result;
                picture->disposition = AV_DISPOSITION_ATTACHED_PIC; picture->time_base = (AVRational){1, 90000};
                audio->cover_stream = (int)i;
                break;
            }
        }
        if ((result = avio_open2(&audio->output->pb, request->output, AVIO_FLAG_WRITE, &audio->output->interrupt_callback, NULL)) < 0) return result;
        if (audio->output_kind == 1) av_dict_set(&options, "rf64", "never", 0);
        result = avformat_write_header(audio->output, &options); av_dict_free(&options); if (result < 0) return result;
        if (audio->cover_stream >= 0) {
            AVPacket *picture = av_packet_clone(&audio->input->streams[audio->cover_stream]->attached_pic);
            if (!picture) return AVERROR(ENOMEM);
            picture->stream_index = 1; picture->pts = picture->dts = 0;
            result = av_interleaved_write_frame(audio->output, picture); av_packet_free(&picture); if (result < 0) return result;
        }
        audio->fifo = av_audio_fifo_alloc(audio->format, audio->channels, 4096);
        if (!audio->fifo) return AVERROR(ENOMEM);
    } else if (audio->output_kind == 5) {
        audio->md5 = av_md5_alloc(); if (!audio->md5) return AVERROR(ENOMEM); av_md5_init(audio->md5);
    } else if (audio->output_kind != 0) {
        audio->raw = utf8_write(request->output); if (!audio->raw) return AVERROR(errno);
    }
    audio->initialized = 1; return 0;
}
static int output_samples(Audio *audio, uint8_t **data, int count)
{
    if (count <= 0) return 0;
    int result = 0; uint8_t *converted = NULL;
    if (audio->bits == 20) {
        converted = av_malloc_array((size_t)count * audio->channels, sizeof(int32_t)); if (!converted) return AVERROR(ENOMEM);
        int32_t *pcm = (int32_t *)converted;
        for (int i = 0; i < count; ++i) for (int ch = 0; ch < audio->channels; ++ch) {
            double value = floor(((double *)data[ch])[i] * 524288.0 + 0.5);
            pcm[i * audio->channels + ch] = (int32_t)fmax(-524288.0, fmin(524287.0, value)) * 4096;
        }
        data = &converted;
    }
    audio->samples += count;
    if (audio->fifo) {
        if (av_audio_fifo_write(audio->fifo, (void **)data, count) != count) result = AVERROR(ENOMEM);
        else result = encode_fifo(audio, 0);
    } else if (audio->md5) av_md5_update(audio->md5, data[0], (size_t)count * audio->channels * 2);
    else if (audio->raw) {
        size_t values = (size_t)count * audio->channels;
        if (audio->output_kind == 2) {
            const int32_t *pcm = (const int32_t *)data[0];
            uint8_t *packed = av_malloc_array(values, 3);
            if (!packed) result = AVERROR(ENOMEM);
            else {
                for (size_t i = 0; i < values; ++i) {
                    uint32_t value = (uint32_t)pcm[i] >> 8;
                    packed[i * 3] = value; packed[i * 3 + 1] = value >> 8; packed[i * 3 + 2] = value >> 16;
                }
                if (fwrite(packed, 3, values, audio->raw) != values) result = AVERROR(EIO);
                av_free(packed);
            }
        } else {
            size_t bytes = audio->output_kind == 3 ? 2 : 4;
            if (fwrite(data[0], bytes, values, audio->raw) != values) result = AVERROR(EIO);
        }
    }
    report_audio_progress(audio, 0);
    av_free(converted); return result;
}
static int resample(Audio *audio, AVFrame *frame)
{
    if (!audio->initialized) {
        if (!frame) return 0;
        int result = initialize_audio(audio, frame); if (result < 0) return result;
    }
    if (frame && (frame->sample_rate != audio->decoder->sample_rate || frame->ch_layout.nb_channels != audio->channels || frame->format != audio->decoder->sample_fmt)) return AVERROR_INPUT_CHANGED;
    int capacity = swr_get_out_samples(audio->swr, frame ? frame->nb_samples : 0);
    if (capacity < 0) return capacity;
    uint8_t **data = NULL;
    int result = av_samples_alloc_array_and_samples(&data, NULL, audio->channels, FFMAX(capacity, 1), audio->bits == 20 ? AV_SAMPLE_FMT_DBLP : audio->format, 0);
    if (result < 0) return result;
    int count = swr_convert(audio->swr, data, FFMAX(capacity, 1), frame ? (const uint8_t **)frame->extended_data : NULL, frame ? frame->nb_samples : 0);
    result = count < 0 ? count : output_samples(audio, data, count);
    av_freep(&data[0]); av_freep(&data); return result;
}
static int receive_audio(Audio *audio)
{
    int result;
    while ((result = avcodec_receive_frame(audio->decoder, audio->frame)) >= 0) {
        if (cancelled(audio->call)) return AVERROR_EXIT;
        result = resample(audio, audio->frame); av_frame_unref(audio->frame); if (result < 0) return result;
    }
    return result == AVERROR(EAGAIN) || result == AVERROR_EOF ? 0 : result;
}
static int audio(Call *call, AVFormatContext *format)
{
    Audio a = {.call = call, .input = format, .cover_stream = -1, .output_kind = (int)call->request->output_format};
    a.stream = AVERROR_STREAM_NOT_FOUND;
    for (unsigned i = 0; i < format->nb_streams; ++i)
        if (format->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) { a.stream = (int)i; break; }
    if (a.stream < 0) return a.stream;
    const AVCodec *codec = avcodec_find_decoder(format->streams[a.stream]->codecpar->codec_id);
    if (!codec) return AVERROR_DECODER_NOT_FOUND;
    int result;
    a.decoder = avcodec_alloc_context3(codec); a.frame = av_frame_alloc(); a.packet = av_packet_alloc();
    if (!a.decoder || !a.frame || !a.packet) { result = AVERROR(ENOMEM); goto done; }
    if ((result = avcodec_parameters_to_context(a.decoder, format->streams[a.stream]->codecpar)) < 0) goto done;
    a.decoder->thread_count = worker_count(); a.decoder->err_recognition = AV_EF_CRCCHECK | AV_EF_EXPLODE;
    if ((result = avcodec_open2(a.decoder, codec, NULL)) < 0) goto done;
    while (!cancelled(call) && (result = av_read_frame(format, a.packet)) >= 0) {
        if (a.packet->stream_index == a.stream) {
            result = avcodec_send_packet(a.decoder, a.packet);
            if (result >= 0) result = receive_audio(&a);
        }
        av_packet_unref(a.packet); if (result < 0) goto done;
    }
    if (cancelled(call)) { result = AVERROR_EXIT; goto done; }
    if (result != AVERROR_EOF) goto done;
    if ((result = avcodec_send_packet(a.decoder, NULL)) < 0 || (result = receive_audio(&a)) < 0 || (result = resample(&a, NULL)) < 0) goto done;
    if (!a.initialized) { result = AVERROR_INVALIDDATA; goto done; }
    if (a.encoder && ((result = encode_fifo(&a, 1)) < 0 || (result = encode_frame(&a, NULL)) < 0 || (result = av_write_trailer(a.output)) < 0)) goto done;
    report_audio_progress(&a, 1);
    if (a.md5) {
        uint8_t digest[16]; char hex[33]; av_md5_final(a.md5, digest);
        for (int i = 0; i < 16; ++i) sprintf(hex + 2 * i, "%02x", digest[i]);
        message(call, 1, "MD5=%s", hex);
    }
    message(call, 2, "Number of samples: %lld", (long long)a.samples);
    result = 0;
done:
    if (a.raw && fclose(a.raw) && result >= 0) result = AVERROR(EIO);
    if (a.output) { int closed = avio_closep(&a.output->pb); if (closed < 0 && result >= 0) result = closed; avformat_free_context(a.output); }
    swr_free(&a.swr); av_audio_fifo_free(a.fifo); av_free(a.md5);
    avcodec_free_context(&a.encoder); avcodec_free_context(&a.decoder); av_frame_free(&a.frame); av_packet_free(&a.packet);
    return result;
}
static int video(Call *call, AVFormatContext *format)
{
    const AVCodec *codec = NULL; int stream = av_find_best_stream(format, AVMEDIA_TYPE_VIDEO, -1, -1, &codec, 0);
    if (stream < 0) return stream;
    AVCodecContext *decoder = avcodec_alloc_context3(codec), *encoder = NULL;
    AVFrame *frame = av_frame_alloc(), *rgb = av_frame_alloc(); AVPacket *packet = av_packet_alloc();
    struct SwsContext *scale = NULL; FILE *file = NULL; int result = AVERROR(ENOMEM), received = 0;
    if (!decoder || !frame || !rgb || !packet) goto done;
    if ((result = avcodec_parameters_to_context(decoder, format->streams[stream]->codecpar)) < 0) goto done;
    decoder->thread_count = worker_count();
    if ((result = avcodec_open2(decoder, codec, NULL)) < 0) goto done;
    while (!received && !cancelled(call)) {
        result = av_read_frame(format, packet);
        if (result < 0 && result != AVERROR_EOF) goto done;
        if (result == AVERROR_EOF || packet->stream_index == stream) {
            if ((result = avcodec_send_packet(decoder, result == AVERROR_EOF ? NULL : packet)) < 0) goto done;
            result = avcodec_receive_frame(decoder, frame);
            if (result >= 0) received = 1;
            else if (result != AVERROR(EAGAIN)) goto done;
        }
        av_packet_unref(packet);
    }
    if (cancelled(call)) { result = AVERROR_EXIT; goto done; }
    rgb->format = AV_PIX_FMT_RGB24; rgb->width = frame->width; rgb->height = frame->height;
    if ((result = av_frame_get_buffer(rgb, 0)) < 0) goto done;
    scale = sws_getContext(frame->width, frame->height, frame->format, frame->width, frame->height, AV_PIX_FMT_RGB24, SWS_BICUBIC, NULL, NULL, NULL);
    if (!scale) { result = AVERROR(ENOMEM); goto done; }
    if (sws_scale(scale, (const uint8_t *const *)frame->data, frame->linesize, 0, frame->height, rgb->data, rgb->linesize) != frame->height) { result = AVERROR(EIO); goto done; }
    codec = avcodec_find_encoder(AV_CODEC_ID_PNG); encoder = avcodec_alloc_context3(codec);
    if (!encoder) { result = AVERROR(ENOMEM); goto done; }
    encoder->width = rgb->width; encoder->height = rgb->height; encoder->pix_fmt = AV_PIX_FMT_RGB24; encoder->time_base = (AVRational){1, 25}; encoder->thread_count = worker_count();
    if ((result = avcodec_open2(encoder, codec, NULL)) < 0 || (result = avcodec_send_frame(encoder, rgb)) < 0 || (result = avcodec_receive_packet(encoder, packet)) < 0) goto done;
    file = utf8_write(call->request->output); if (!file) { result = AVERROR(errno); goto done; }
    result = fwrite(packet->data, 1, packet->size, file) == (size_t)packet->size ? 0 : AVERROR(EIO);
done:
    if (file && fclose(file) && result >= 0) result = AVERROR(EIO);
    sws_freeContext(scale); avcodec_free_context(&encoder); avcodec_free_context(&decoder);
    av_frame_free(&rgb); av_frame_free(&frame); av_packet_free(&packet); return result;
}
__declspec(dllexport) int __cdecl dvdamedia_run(const Request *request, Emit emit, Cancel cancel, void *state)
{
    if (!request || request->size != sizeof(Request) || request->abi != 1 || !request->input || !emit) return AVERROR(EINVAL);
    Call call = {request, emit, cancel, state}; InitOnceExecuteOnce(&once, initialize, NULL, NULL); active = &call;
    AVFormatContext *format = NULL; int result = cancelled(&call) ? AVERROR_EXIT : open_input(&call, &format);
    if (result >= 0) switch (request->operation) {
        case 1: result = probe(&call, format); break;
        case 2: result = packets(&call, format); break;
        case 3: result = audio(&call, format); break;
        case 4: result = video(&call, format); break;
        case 5: result = cover(&call, format); break;
        default: result = AVERROR(EINVAL); break;
    }
    if (result < 0 && result != AVERROR_EXIT) {
        char error[AV_ERROR_MAX_STRING_SIZE]; av_strerror(result, error, sizeof(error));
        message(&call, 2, "Error while decoding or processing media: %s", error);
    }
    avformat_close_input(&format); active = NULL; return result;
}
