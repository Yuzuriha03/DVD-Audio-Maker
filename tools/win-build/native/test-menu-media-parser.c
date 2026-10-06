#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/error.h>

#include <stdio.h>

int main(int argc, char **argv)
{
    AVFormatContext *format = NULL;
    AVCodecContext *codec = NULL;
    AVCodecParserContext *parser = NULL;
    AVPacket *packet = NULL;
    const AVCodec *decoder;
    int video_index = -1;
    int frames_before_eof = 0;
    int sequence_end_seen = 0;
    int expect_sequence_end_id = 0;
    int previous_zeros = 0;
    int result;
    unsigned i;

    if (argc != 2) {
        fprintf(stderr, "usage: menu-parser-test <program-stream.mpg>\n");
        return 2;
    }
    result = avformat_open_input(&format, argv[1], NULL, NULL);
    if (result < 0) goto fail;
    result = avformat_find_stream_info(format, NULL);
    if (result < 0) goto fail;
    for (i = 0; i < format->nb_streams; ++i) {
        if (format->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO &&
            format->streams[i]->codecpar->codec_id == AV_CODEC_ID_MPEG2VIDEO) {
            video_index = (int)i;
            break;
        }
    }
    if (video_index < 0) {
        result = AVERROR_STREAM_NOT_FOUND;
        goto fail;
    }
    decoder = avcodec_find_decoder(AV_CODEC_ID_MPEG2VIDEO);
    codec = avcodec_alloc_context3(decoder);
    packet = av_packet_alloc();
    if (!decoder || !codec || !packet) {
        result = AVERROR(ENOMEM);
        goto fail;
    }
    result = avcodec_parameters_to_context(codec,
                                            format->streams[video_index]->codecpar);
    if (result < 0) goto fail;
    parser = av_parser_init(AV_CODEC_ID_MPEG2VIDEO);
    if (!parser) {
        result = AVERROR(ENOMEM);
        goto fail;
    }

    while ((result = av_read_frame(format, packet)) >= 0) {
        if (packet->stream_index == video_index) {
            uint8_t *data = packet->data;
            int remaining = packet->size;
            while (remaining > 0) {
                uint8_t *frame = NULL;
                int frame_size = 0;
                int byte_index;
                int consumed = av_parser_parse2(parser, codec, &frame, &frame_size,
                                                data, remaining, packet->pts,
                                                packet->dts, packet->pos);
                if (consumed < 0) {
                    result = consumed;
                    goto fail;
                }
                if (frame_size > 0) ++frames_before_eof;
                if (consumed == 0 && frame_size == 0) {
                    result = AVERROR_INVALIDDATA;
                    goto fail;
                }
                for (byte_index = 0; byte_index < consumed; ++byte_index) {
                    uint8_t byte = data[byte_index];
                    if (expect_sequence_end_id && byte == 0xB7)
                        sequence_end_seen = 1;
                    expect_sequence_end_id = byte == 0x01 && previous_zeros >= 2;
                    previous_zeros = byte == 0 ? previous_zeros + 1 : 0;
                }
                data += consumed;
                remaining -= consumed;
            }
        }
        av_packet_unref(packet);
    }
    if (result != AVERROR_EOF) goto fail;
    if (frames_before_eof < 1) {
        fprintf(stderr, "MPEG-2 parser produced no complete frame before EOF\n");
        result = AVERROR_INVALIDDATA;
        goto fail;
    }
    if (!sequence_end_seen) {
        fprintf(stderr, "MPEG-2 video stream has no sequence_end_code (B7)\n");
        result = AVERROR_INVALIDDATA;
        goto fail;
    }
    printf("PASS: parser emitted %d complete frame(s) before EOF\n",
           frames_before_eof);
    result = 0;
    goto done;

fail:
    {
        char message[AV_ERROR_MAX_STRING_SIZE];
        av_strerror(result, message, sizeof(message));
        fprintf(stderr, "MPEG-2 parser check failed: %s\n", message);
    }
done:
    if (parser) av_parser_close(parser);
    av_packet_free(&packet);
    avcodec_free_context(&codec);
    avformat_close_input(&format);
    return result < 0 ? 1 : 0;
}
