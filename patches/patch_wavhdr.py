#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""修复：MLP 编码器按裸 PCM 读取输入，需跳过 WAV 头。

诊断依据：编码结果的头几个样本等于 "RIF"（RIFF 的 ASCII），
说明从文件偏移 0 开始读取，把 WAV 头当成了音频样本。
增加一个定位 data 块的辅助函数，并在编码前 fseek 到音频起点。
"""
import pathlib

PATH = pathlib.Path("/root/dvda-author-mlp8/src/mlp.c")
text = PATH.read_text(encoding="utf-8", errors="surrogateescape")

HELPER = '''/* 定位 WAV 文件 data 块的起始偏移；非 WAV 输入返回 0（按裸 PCM 处理）。
   MLP 编码器按裸 PCM 读取输入，因此必须跳过 WAV 头。 */
static long wav_data_offset(FILE *fp)
{
  uint8_t hdr[12];

  if (fseek(fp, 0, SEEK_SET) != 0)
    return -1;

  if (fread(hdr, 1, 12, fp) != 12)
    return -1;

  if (memcmp(hdr, "RIFF", 4) != 0 || memcmp(hdr + 8, "WAVE", 4) != 0)
    return 0;   /* 不是 WAV：按裸 PCM 从 0 开始 */

  long off = 12;

  while (1)
    {
      uint8_t ch[8];

      if (fread(ch, 1, 8, fp) != 8)
        return -1;

      uint32_t sz = (uint32_t) ch[4]
                    | ((uint32_t) ch[5] << 8)
                    | ((uint32_t) ch[6] << 16)
                    | ((uint32_t) ch[7] << 24);

      off += 8;

      if (memcmp(ch, "data", 4) == 0)
        return off;

      long skip = (long) sz + (long) (sz & 1);

      if (fseek(fp, skip, SEEK_CUR) != 0)
        return -1;

      off += skip;
    }
}

int encode_mlp_file(fileinfo_t *info, globalData *globals)'''

old_sig = "int encode_mlp_file(fileinfo_t *info, globalData *globals)"
if HELPER.split("int encode_mlp_file")[0].strip() in text:
    print("[SKIP] 辅助函数已存在")
else:
    if old_sig not in text:
        print("[MISS] 找不到 encode_mlp_file 签名")
        raise SystemExit(1)
    text = text.replace(old_sig, HELPER, 1)
    print("[OK] 新增 wav_data_offset 辅助函数")

# 打开输入文件后跳过 WAV 头
OLD = """  FILE *in_fp = fopen(strdup(info->filename), "rb");

  path_t *fn = parse_filepath(info->filename, globals);"""
NEW = """  FILE *in_fp = fopen(strdup(info->filename), "rb");

  /* 跳过 WAV 头，从 data 块开始读取裸 PCM */
  long data_off = 0;

  if (in_fp)
    {
      long off = wav_data_offset(in_fp);

      if (off > 0)
        data_off = off;
      else
        data_off = 0;

      fseek(in_fp, data_off, SEEK_SET);
    }

  path_t *fn = parse_filepath(info->filename, globals);"""

if OLD in text:
    text = text.replace(OLD, NEW, 1)
    print("[OK] 插入跳过 WAV 头逻辑")
else:
    print("[MISS] 找不到 fopen 段")

# 修正「是否整文件读取」的判断（扣除头部）
OLD_CHK = """  if (bytes_read == info->file_size)
    if (globals->debugging) foutput(MSG_TAG "File %s was entirely read (%llu B)", info->filename, info->file_size);
    else
      foutput(MSG_TAG "File %s was not entirely read (%llu B / %llu B)", info->filename, bytes_read, info->file_size);"""
NEW_CHK = """  if (bytes_read == (uint64_t)(info->file_size - data_off))
    {
      if (globals->debugging)
        foutput(MSG_TAG "File %s was entirely read (%llu B, header %ld B)", info->filename, bytes_read, data_off);
    }
  else
    {
      foutput(MSG_TAG "File %s was not entirely read (%llu B / %llu B, header %ld B)",
              info->filename, bytes_read, (uint64_t)(info->file_size - data_off), data_off);
    }"""

if OLD_CHK in text:
    text = text.replace(OLD_CHK, NEW_CHK, 1)
    print("[OK] 修正整文件读取判断")
else:
    print("[MISS] 找不到整文件读取判断")

PATH.write_text(text, encoding="utf-8", errors="surrogateescape")
print("[DONE]")
