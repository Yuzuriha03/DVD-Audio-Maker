/* Maintenance-only probe. Never included in the GUI release. */
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/samplefmt.h>

static int dump_frames(AVCodecContext *context, AVFrame *frame, FILE *out,
                       int64_t *frames, int *channels, int *rate)
{
    int result;
    while ((result = avcodec_receive_frame(context, frame)) >= 0) {
        int count = frame->ch_layout.nb_channels;
        int planar = av_sample_fmt_is_planar(frame->format);
        enum AVSampleFormat packed = av_get_packed_sample_fmt(frame->format);
        if (packed != AV_SAMPLE_FMT_S16 && packed != AV_SAMPLE_FMT_S32) return -1;
        if (*channels && (*channels != count || *rate != frame->sample_rate)) return -1;
        *channels = count;
        *rate = frame->sample_rate;
        for (int sample = 0; sample < frame->nb_samples; ++sample) {
            for (int channel = 0; channel < count; ++channel) {
                const uint8_t *data = frame->extended_data[planar ? channel : 0];
                int index = planar ? sample : sample * count + channel;
                int32_t value = packed == AV_SAMPLE_FMT_S16
                    ? (int32_t)((const int16_t *)data)[index] * 65536
                    : ((const int32_t *)data)[index];
                if (fwrite(&value, sizeof(value), 1, out) != 1) return -1;
            }
        }
        *frames += frame->nb_samples;
        av_frame_unref(frame);
    }
    return result == AVERROR(EAGAIN) || result == AVERROR_EOF ? 0 : result;
}

int main(int argc, char **argv)
{
    const char *libraries[] = {"avcodec-63.dll", "avformat-63.dll", "avutil-61.dll"};
    for (unsigned i = 0; i < sizeof(libraries) / sizeof(libraries[0]); ++i) {
        char path[32768];
        if (!GetModuleFileNameA(GetModuleHandleA(libraries[i]), path, sizeof(path))) return 10;
        fprintf(stderr, "DLL %s %s\n", libraries[i], path);
    }
    if (argc == 2 && strcmp(argv[1], "--capabilities") == 0) {
        void *iterator = NULL;
        const AVCodec *codec;
        int codecs = 0, formats = 0, muxers = 0, protocols = 0;
        while ((codec = av_codec_iterate(&iterator))) {
            ++codecs;
            if (codec->id != AV_CODEC_ID_MLP) return 11;
        }
        iterator = NULL;
        while (av_demuxer_iterate(&iterator)) ++formats;
        iterator = NULL;
        while (av_muxer_iterate(&iterator)) ++muxers;
        iterator = NULL;
        while (avio_enum_protocols(&iterator, 0)) ++protocols;
        printf("{\"codec_version\":%u,\"format_version\":%u,\"util_version\":%u,"
               "\"codecs\":%d,\"demuxers\":%d,\"muxers\":%d,\"input_protocols\":%d}\n",
               avcodec_version(), avformat_version(), avutil_version(), codecs, formats, muxers, protocols);
        return codecs == 2 && formats == 1 && muxers == 1 && protocols == 2 ? 0 : 12;
    }
    if (argc != 3) return 2;
    AVFormatContext *format = NULL;
    AVCodecContext *context = NULL;
    AVFrame *frame = NULL;
    AVPacket *packet = NULL;
    FILE *out = NULL;
    int result = 1, channels = 0, rate = 0;
    int64_t frames = 0;
    if (avformat_open_input(&format, argv[1], NULL, NULL) < 0 ||
        avformat_find_stream_info(format, NULL) < 0) goto done;
    int stream = av_find_best_stream(format, AVMEDIA_TYPE_AUDIO, -1, -1, NULL, 0);
    if (stream < 0) goto done;
    const AVCodec *codec = avcodec_find_decoder(format->streams[stream]->codecpar->codec_id);
    context = avcodec_alloc_context3(codec);
    if (!context || avcodec_parameters_to_context(context, format->streams[stream]->codecpar) < 0 ||
        avcodec_open2(context, codec, NULL) < 0) goto done;
    packet = av_packet_alloc();
    frame = av_frame_alloc();
    out = fopen(argv[2], "wb");
    if (!packet || !frame || !out) goto done;
    int read_status;
    while ((read_status = av_read_frame(format, packet)) >= 0) {
        if (packet->stream_index == stream &&
            (avcodec_send_packet(context, packet) < 0 || dump_frames(context, frame, out, &frames, &channels, &rate) < 0)) goto done;
        av_packet_unref(packet);
    }
    if (read_status != AVERROR_EOF || avcodec_send_packet(context, NULL) < 0 ||
        dump_frames(context, frame, out, &frames, &channels, &rate) < 0) goto done;
    if (fflush(out) != 0) goto done;
    printf("{\"frames\":%lld,\"channels\":%d,\"sample_rate\":%d,\"bits\":%d}\n",
           (long long)frames, channels, rate, context->bits_per_raw_sample);
    result = frames > 0 ? 0 : 1;
done:
    if (out && fclose(out) != 0) result = 1;
    av_frame_free(&frame);
    av_packet_free(&packet);
    avcodec_free_context(&context);
    avformat_close_input(&format);
    return result;
}
