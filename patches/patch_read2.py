#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""迁移 decode_mlp_file 中「提取分支」的第二个解析循环到 av_read_frame。"""
import pathlib

PATH = pathlib.Path("/root/dvda-author-mlp8/src/mlp.c")
text = PATH.read_text(encoding="utf-8", errors="surrogateescape")

OLD = """      while (data_size > 0)
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
            cumbytes_written = decode(context, codec,
                                      codecpar, &packet,
                                      frame, fp_out,
                                      info, cumbytes_written,
                                      globals);

          if (data_size < AUDIO_REFILL_THRESH)
            {
              memmove(inbuf, data, data_size);
              data = inbuf;
              int len = fread(data + data_size, 1,
                              AUDIO_INBUF_SIZE - data_size, fp_in);
              if (len > 0)
                data_size += len;
            }
        }

      // Flush

      packet.data = NULL;
      packet.size = 0;

      decode(context, codec,
             codecpar, &packet,
             frame, fp_out,
             info, cumbytes_written,
             globals);
"""

NEW = """      while (av_read_frame(format, packet) >= 0)
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
            cumbytes_written = decode(context, codec,
                                      codecpar, packet,
                                      frame, fp_out,
                                      info, cumbytes_written,
                                      globals);

          av_packet_unref(packet);
        }

      // Flush：av_read_frame 到 EOF 后 packet 为空，用于排空解码器

      decode(context, codec,
             codecpar, packet,
             frame, fp_out,
             info, cumbytes_written,
             globals);
"""

if OLD not in text:
    if "while (av_read_frame(format, packet) >= 0)" in text and "cumbytes_written = decode(context, codec,\n                                      codecpar, packet," in text:
        print("[SKIP] 提取分支解析循环已迁移")
        raise SystemExit(0)
    print("[MISS] 提取分支解析循环")
    raise SystemExit(1)

text = text.replace(OLD, NEW, 1)
PATH.write_text(text, encoding="utf-8", errors="surrogateescape")
print("[OK] 提取分支解析循环已迁移")
