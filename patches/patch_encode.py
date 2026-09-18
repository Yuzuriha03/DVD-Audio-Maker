#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把 mlp.c 的编码端迁移到 FFmpeg 8 API，并启用 24-bit。

- MLP 编码器在 FFmpeg 8 使用 planer 采样格式(s16p/s32p)，
  原实现按 packed 填充 frame->data[0]，需按 plane 分别填充。
- 24-bit 原被直接拒绝；FFmpeg 8 支持 s32p，故放开并做 <<8 对齐。
"""
import pathlib
import re
import sys

PATH = pathlib.Path("/root/dvda-author-mlp8/src/mlp.c")
text = PATH.read_text(encoding="utf-8", errors="surrogateescape")
n_ok = 0


def replace_function(src, signature, new_code, label):
    """把以 signature 开头的整个函数替换为 new_code。"""
    idx = src.find(signature)
    if idx < 0:
        print("[MISS] %s（找不到函数签名）" % label)
        return src, False
    # 找到函数体结束：从函数名后的第一个 '{' 开始做括号配对
    brace = src.find("{", idx)
    if brace < 0:
        print("[MISS] %s（找不到函数体）" % label)
        return src, False
    depth = 0
    i = brace
    while i < len(src):
        if src[i] == "{":
            depth += 1
        elif src[i] == "}":
            depth -= 1
            if depth == 0:
                break
        i += 1
    end = i + 1
    print("[OK] %s" % label)
    return src[:idx] + new_code + src[end:], True


def rep(old, new, label, count=1):
    global text, n_ok
    if old not in text:
        print("[MISS] %s" % label)
        return False
    text = text.replace(old, new, count)
    print("[OK] %s" % label)
    n_ok += 1
    return True


NEW_S32 = '''inline static uint64_t encode_fmt_s32(AVCodecContext *c, AVFrame *frame, AVPacket *pkt, FILE *in_fp, FILE *out_fp, globalData *globals)
{
  /* 24-bit PCM → s32p：planer 格式，需按通道 plane 分别填充；
     小端下 24-bit 样本存放在 4 字节样本的高 3 字节（等价 <<8）。 */
  const int nch    = c->ch_layout.nb_channels;
  const int nframe = c->frame_size;
  static uint64_t bytes_read;
  uint64_t read_this_call = 0;
  int eof = 0;

  for (int ch = 0; ch < nch; ++ch)
    memset(frame->data[ch], 0, (size_t) nframe * 4);

  for (int s = 0; s < nframe && !eof; ++s)
    for (int ch = 0; ch < nch && !eof; ++ch)
      {
        uint8_t *dst = (uint8_t *) frame->data[ch] + (size_t) s * 4 + 1;
        size_t n = fread(dst, 1, 3, in_fp);
        read_this_call += n;
        if (n < 3) eof = 1;
      }

  if (read_this_call)
    {
      encode(c, frame, pkt, out_fp, globals);
      bytes_read += read_this_call;

      if (globals->maxverbose)
        foutput(MSG_TAG "Bytes read per frame: %lu\\n", (unsigned long) read_this_call);
    }

  return bytes_read;
}'''

NEW_S16 = '''inline static uint64_t encode_fmt_s16(AVCodecContext *c, AVFrame *frame, AVPacket *pkt, FILE *in_fp, FILE *out_fp, globalData *globals)
{
  /* 16-bit PCM → s16p：planer 格式，按通道 plane 填充 */
  const int nch    = c->ch_layout.nb_channels;
  const int nframe = c->frame_size;
  static uint64_t bytes_read;
  uint64_t read_this_call = 0;
  int eof = 0;

  for (int ch = 0; ch < nch; ++ch)
    memset(frame->data[ch], 0, (size_t) nframe * 2);

  for (int s = 0; s < nframe && !eof; ++s)
    for (int ch = 0; ch < nch && !eof; ++ch)
      {
        uint8_t *dst = (uint8_t *) frame->data[ch] + (size_t) s * 2;
        size_t n = fread(dst, 1, 2, in_fp);
        read_this_call += n;
        if (n < 2) eof = 1;
      }

  if (read_this_call)
    {
      encode(c, frame, pkt, out_fp, globals);
      bytes_read += read_this_call;

      if (globals->maxverbose)
        foutput(MSG_TAG "Bytes read per frame: %lu\\n", (unsigned long) read_this_call);
    }

  return bytes_read;
}'''

text, ok1 = replace_function(text, "inline static uint64_t encode_fmt_s32(", NEW_S32, "encode_fmt_s32 改为 planer")
n_ok += ok1
text, ok2 = replace_function(text, "inline static uint64_t encode_fmt_s16(", NEW_S16, "encode_fmt_s16 改为 planer")
n_ok += ok2

# 放开 24-bit 限制
rep(
    """  if (info->bitspersample == 24)
    {
      foutput(WAR "Currently 24-bit MLP support is still under development. Balking at concerting file %s\\n", info->filename);
      return 0;
    }
""",
    """  /* [FFmpeg8 迁移] 原先 24-bit 被直接跳过；编码器已改用 s32p，故放开。
     注意：若此处提前 return，mlp_filename 会保持为空并在后续崩溃。 */
  if (info->bitspersample != 16 && info->bitspersample != 24)
    {
      foutput(ERR "Unsupported bit depth for MLP: %d\\n", info->bitspersample);
      return -1;
    }
""",
    "放开 24-bit 限制",
)

# 采样格式：packed → planer
rep(
    "  c->sample_fmt = info->bitspersample == 16 ? AV_SAMPLE_FMT_S16 : AV_SAMPLE_FMT_S32;",
    "  c->sample_fmt = info->bitspersample == 16 ? AV_SAMPLE_FMT_S16P : AV_SAMPLE_FMT_S32P;",
    "sample_fmt 改为 planer",
)

# 声道布局：mask → AVChannelLayout
rep(
    """  c->sample_rate    = info->samplerate;
  c->channel_layout = select_channel_layout(info, globals);
  c->channels       = info->channels;""",
    """  c->sample_rate    = info->samplerate;

  uint64_t chmask = select_channel_layout(info, globals);

  if (chmask == 0 || av_channel_layout_from_mask(&c->ch_layout, chmask) < 0)
    av_channel_layout_default(&c->ch_layout, info->channels);""",
    "ch_layout 设置",
)

# frame 的声道布局
rep(
    "  frame->channel_layout = c->channel_layout;",
    "  av_channel_layout_copy(&frame->ch_layout, &c->ch_layout);",
    "frame->ch_layout 设置",
)

# tempdir 空指针防御
rep(
    "  char *out_filename = (char *) calloc(strlen(globals->settings.tempdir) + 1 + strlen(fn->rawfilename) + 12 + 1, sizeof(char));",
    "  const char *tempdir = globals->settings.tempdir ? globals->settings.tempdir : \"/tmp\";\n"
    "  char *out_filename = (char *) calloc(strlen(tempdir) + 1 + strlen(fn->rawfilename) + 12 + 1, sizeof(char));",
    "tempdir 空指针防御",
)
rep(
    '  sprintf(out_filename, "%s%s%s%s", globals->settings.tempdir, SEPARATOR, fn->rawfilename, "_enc_wav.mlp");',
    '  sprintf(out_filename, "%s%s%s%s", tempdir, SEPARATOR, fn->rawfilename, "_enc_wav.mlp");',
    "out_filename 使用 tempdir",
)

# transport 空指针防御
rep(
    "  info->filename = strdup(info->mlp_filename);",
    """  if (info->mlp_filename == NULL)
    {
      foutput(ERR "MLP file was not created for %s\\n", info->filename);
      EXITING
    }
  info->filename = strdup(info->mlp_filename);""",
    "mlp_filename 空指针防御",
)

PATH.write_text(text, encoding="utf-8", errors="surrogateescape")
print("\n编码端迁移完成，共 %d 项" % n_ok)
