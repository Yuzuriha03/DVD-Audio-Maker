#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把 dvda-author 的 mlp.c 从 FFmpeg 4.x API 迁移到 FFmpeg 8.x API。

要点：
- channels / channel_layout → ch_layout
- AVFrame 的 pkt_pos / pkt_duration / pkt_size 已移除：
  改为自行记录输入包位置（MLP 扇区布局依赖 pkt_pos）
- 读包方式：av_parser_* → av_read_frame（位置信息更可靠）
- avcodec_close → avcodec_free_context
- 编码端改用 planer 格式（s16p/s32p），并允许 24-bit
"""
import pathlib
import sys

PATH = pathlib.Path("/root/dvda-author-mlp8/src/mlp.c")
text = PATH.read_text(encoding="utf-8", errors="surrogateescape")
hits = []


def rep(old, new, label, count=1):
    global text
    if old not in text:
        print("[MISS] %s" % label)
        return False
    text = text.replace(old, new, count)
    print("[OK] %s" % label)
    hits.append(label)
    return True


# ---------- 0. 头部新增：记录输入包位置与最近一帧的采样数 ----------
rep(
    "#define AUDIO_INBUF_SIZE 20480\n#define AUDIO_REFILL_THRESH 4096\n",
    "#define AUDIO_INBUF_SIZE 20480\n#define AUDIO_REFILL_THRESH 4096\n\n"
    "/* FFmpeg 8 已移除 AVFrame.pkt_pos，这里自行记录当前输入包的位置，\n"
    "   供 MLP 扇区布局计算使用。 */\n"
    "static int64_t g_last_pkt_pos = 0;\n\n"
    "/* 关键修复：avcodec_receive_frame() 在返回 EAGAIN/EOF 前会 av_frame_unref(frame)，\n"
    "   导致循环外读取 frame->nb_samples 恒为 0，进而使 MLP 时间轴全部失效\n"
    "   (PTS_length=0、每扇区 PTS 恒定 → 进度条不可用、播放变速)。\n"
    "   故在成功收到帧的当下立即保存采样数。 */\n"
    "static int g_last_nb_samples = 0;\n",
    "新增 g_last_pkt_pos / g_last_nb_samples",
)

# ---------- 0b. decode()：收到帧时立即记录采样数 ----------
# 注意：必须插在 `if/else` 链之外，否则会孤立 else 分支。
rep(
    "      int data_size = av_get_bytes_per_sample(context->sample_fmt);\n",
    "      /* 必须在 avcodec_receive_frame 返回 EAGAIN 之前取走 nb_samples：\n"
    "         EAGAIN 前 frame 会被 unref，循环外读到的一律为 0（见文件头说明）。 */\n"
    "      g_last_nb_samples = frame->nb_samples;\n"
    "\n"
    "      int data_size = av_get_bytes_per_sample(context->sample_fmt);\n",
    "decode: 记录 g_last_nb_samples",
)

# ---------- 1. decode()：channels 迁移 ----------
rep(
    "unpadded_linesize = frame->channels * sampleSize * frame->nb_samples;",
    "unpadded_linesize = frame->ch_layout.nb_channels * sampleSize * frame->nb_samples;",
    "decode: frame->channels",
)
rep(
    "for (int c = 0; c < codecpar->channels; ++c)\n"
    "                    {\n"
    "                      uint32_t val = ((int32_t *) frame->extended_data[0])[s * codecpar->channels + c];",
    "for (int c = 0; c < codecpar->ch_layout.nb_channels; ++c)\n"
    "                    {\n"
    "                      uint32_t val = ((int32_t *) frame->extended_data[0])[s * codecpar->ch_layout.nb_channels + c];",
    "decode: codecpar->channels",
)

# ---------- 2. decode()：maxverbose 中已移除字段 ----------
rep(
    '''          fprintf(stderr,
                  "Bytes_written: %d Nb samples: %d FR_PTS: %ld PKT_POS: %ld PKT_DURATION: %ld FR_PKT_SIZE; %ld\\n",
                  cumbytes_written,
                  frame->nb_samples,
                  frame->pts,
                  frame->pkt_pos,
                  frame->pkt_duration,
                  frame->pkt_size);''',
    '''          fprintf(stderr,
                  "Bytes_written: %ld Nb samples: %d FR_PTS: %ld PKT_POS: %ld\\n",
                  (long) cumbytes_written,
                  frame->nb_samples,
                  (long) frame->pts,
                  (long) packet->pos);''',
    "decode: maxverbose 字段",
)

# ---------- 3. 声明：AVPacket packet -> 指针 ----------
rep(
    "  AVPacket packet;\n",
    "",
    "移除栈上 AVPacket 声明",
)

# ---------- 4. 读取准备：parser -> av_packet_alloc ----------
rep(
    """  // prepare to read data

  av_init_packet(&packet);

  int64_t cumbytes_written = 0;

  parser = av_parser_init(codec->id);
  if (!parser)
    {
      fprintf(stderr, "Parser not found\\n");
      exit(1);
    }
""",
    """  // prepare to read data

  AVPacket *packet = av_packet_alloc();
  if (!packet)
    {
      fprintf(stderr, "Could not allocate packet\\n");
      exit(1);
    }

  int64_t cumbytes_written = 0;
""",
    "读取准备改为 av_packet_alloc",
)

# ---------- 5. 布局循环：parser -> av_read_frame ----------
rep(
    """      while (data_size > 0)
        {
          if (! frame)
            {
              if (!(frame = av_frame_alloc()))
                {
                  fprintf(stderr, "Could not allocate audio frame\\n");
                  EXIT_ON_RUNTIME_ERROR
                }
            }

          int ret;

          ret = av_parser_parse2(parser, context,
                                 &packet.data, &packet.size,
                                 data, data_size,
                                 AV_NOPTS_VALUE, AV_NOPTS_VALUE, 0);

          if (ret < 0)
            {
              fprintf(stderr, "Error while parsing\\n");
              exit(1);
            }

          data      += ret;
          data_size -= ret;

          if (packet.size)
            cumbytes_written = decode(context, codec, codecpar,
                                      &packet, frame, NULL,
                                      info, cumbytes_written,
                                      globals);

          if (data_size < AUDIO_REFILL_THRESH)
            {
              if (info->file_size > AUDIO_INBUF_SIZE)
                {
                  if (data_size < AUDIO_INBUF_SIZE)
                    data_size += AUDIO_INBUF_SIZE - data_size;
                }
              else
                data_size += AUDIO_INBUF_SIZE - info->file_size;
            }
""",
    """      while (av_read_frame(format, packet) >= 0)
        {
          if (packet->stream_index != stream_index)
            {
              av_packet_unref(packet);
              continue;
            }

          if (! frame)
            {
              if (!(frame = av_frame_alloc()))
                {
                  fprintf(stderr, "Could not allocate audio frame\\n");
                  EXIT_ON_RUNTIME_ERROR
                }
            }

          g_last_pkt_pos = packet->pos;

          if (packet->size)
            cumbytes_written = decode(context, codec, codecpar,
                                      packet, frame, NULL,
                                      info, cumbytes_written,
                                      globals);

          av_packet_unref(packet);
""",
    "布局循环改为 av_read_frame",
)

# ---------- 6. 布局中的 pkt_pos ----------
rep(
    "              PKT_POS_SECT = frame->pkt_pos + HEADER_OFFSET;",
    "              PKT_POS_SECT = g_last_pkt_pos + HEADER_OFFSET;",
    "PKT_POS_SECT 改用 g_last_pkt_pos",
)
rep(
    "              totnbsamples += frame->nb_samples;",
    "              totnbsamples += g_last_nb_samples;",
    "布局累积改用 g_last_nb_samples",
)
rep(
    """                  info->mlp_layout[rank].pkt_pos    = frame->pkt_pos;""",
    """                  info->mlp_layout[rank].pkt_pos    = g_last_pkt_pos;""",
    "布局条目 pkt_pos(1)",
)
rep(
    '''                  fprintf(stderr, "Sect: %lu samples_written: %d Nb samples: %d FR_PTS: %ld PKT_POS: %ld PKT_DURATION: %ld FR_PKT_SIZE; %ld\\n",
                          SECT_RANK,
                          totnbsamples,
                          frame->nb_samples,
                          frame->pts,
                          frame->pkt_pos,
                          frame->pkt_duration,
                          frame->pkt_size);''',
    '''                  fprintf(stderr, "Sect: %lu samples_written: %d Nb samples: %d FR_PTS: %ld PKT_POS: %ld\\n",
                          SECT_RANK,
                          totnbsamples,
                          frame->nb_samples,
                          (long) frame->pts,
                          (long) g_last_pkt_pos);''',
    "布局 maxverbose 字段",
)
rep(
    "      info->mlp_layout[rank].pkt_pos = frame->pkt_pos;",
    "      info->mlp_layout[rank].pkt_pos = g_last_pkt_pos;",
    "布局条目 pkt_pos(2)",
)

# ---------- 7. 清理：avcodec_close ----------
rep(
    "  av_frame_free(&frame);\n  avcodec_close(context);\n  avformat_free_context(format);",
    "  av_frame_free(&frame);\n  av_packet_free(&packet);\n  avcodec_free_context(&context);\n  avformat_free_context(format);",
    "avcodec_close → avcodec_free_context",
)

# ---------- 8. WAV 头中的 channel_layout（仅提取路径使用） ----------
rep(
    """          header.dwChannelMask   = (codecpar->channel_layout < 21
                                    && codecpar->channel_layout > 0) ?
                                   cga2wav_channels[codecpar->channel_layout] : 0;""",
    """          header.dwChannelMask   = (codecpar->ch_layout.nb_channels > 0
                                    && codecpar->ch_layout.nb_channels < 21) ?
                                   cga2wav_channels[codecpar->ch_layout.nb_channels] : 0;""",
    "header: dwChannelMask",
)
rep(
    "          header.channels        = codecpar->channels;",
    "          header.channels        = codecpar->ch_layout.nb_channels;",
    "header: channels",
)
rep(
    "          header.nBlockAlign     = (codecpar->channels * codecpar->bits_per_raw_sample) / 8 ;",
    "          header.nBlockAlign     = (codecpar->ch_layout.nb_channels * codecpar->bits_per_raw_sample) / 8 ;",
    "header: nBlockAlign",
)

PATH.write_text(text, encoding="utf-8", errors="surrogateescape")
print("\n应用成功 %d 项" % len(hits))
