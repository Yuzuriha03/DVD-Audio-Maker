#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""修复 ats.c 的 pack 边界错位：MLP 每轨最后一个 pack 可能短 1~6 字节。

write_pes_padding() 在 length 为 1~6 时只打印一句错误就 return，**一个字节都不写**。
调用方（MLP 最后一包）传的是「补到 2048 边界还需的字节数」，该值落到 1~6 时：

  · 该轨最后一个 pack 短 1~6 字节 → 文件不再按 2048 对齐
  · 下一轨的 pack 头因此落在扇区中间（实测偏移 2042 / 2043）
  · 读盘端按 IFO 给的扇区号取该轨时，该扇区不以 pack 头开头 →
    拿不到 stream id → 整首被判为无效而丢弃
    （实测 foo_input_dvda 每张盘少 1 轨，且少的正是紧随短包头之后的那首）

原先的判据 `length > 6` 还顺带把 length == 6 也判为错误 —— 而 6 字节恰好
就是一个空 PES 填充包（3 字节起始码 + 1 字节 stream id + 2 字节长度），
本可以正常写出。故一并放开。

改动
  1. length < 6：补零到边界（与 length == 0 分支同样的处理）
  2. length >= 6：正常写 PES 填充包
  3. ff_buf 改为 length + 1 大小，避免 length == 0 时的零长数组
"""
import pathlib

PATH = pathlib.Path("/root/dvda-author-mlp8/src/ats.c")
text = PATH.read_text(encoding="utf-8", errors="surrogateescape")
n_ok = 0


def rep(old, new, label, count=1):
    global text, n_ok
    if old not in text:
        print("[MISS] %s" % label)
        return False
    text = text.replace(old, new, count)
    print("[OK] %s" % label)
    n_ok += 1
    return True


# 1) 放开 length == 6，并处理 1~5（补零）
#    只匹配 write_pes_padding 里的那段 —— read_pes_padding 少了 maxverbose 那行，
#    且其错误消息带两个前导空格，不会误匹配。
rep(
    """  if (length > 6)
    {
      length -= 6; // We have 6 bytes of PES header.
      if (globals->maxverbose)
        foutput("%s %d %s\\n", INF " Padding with ", length, " bytes.");
    }
  else
    {
      foutput("%s\\n", ERR "pes_padding length must be higher than 6;");
      return;
    }
""",
    """  if (length < 6)
    {
      /* 不足一个 PES 填充包的头部（6 字节），只能补零。
         原先此处只报错并直接 return，一个字节都不写：该轨最后一个 pack 会短
         1~5 字节，文件不再按 2048 对齐，其后第一轨的 pack 头落进扇区中间，
         读盘端取不到 pack 头会把整首丢掉。 */
      uint8_t zero[6];
      memset(zero, 0, length);
      fwrite(zero, length, 1, fp);
      return;
    }

  length -= 6; // We have 6 bytes of PES header.
  if (globals->maxverbose)
    foutput("%s %d %s\\n", INF " Padding with ", length, " bytes.");
""",
    "放开 length==6，1~5 改为补零",
)

# 2) length 现在可能为 0，零长数组不可移植
rep(
    """  uint8_t ff_buf[length];

  memset(ff_buf, 0xff, length);

  /* offset_count += 3 */ fwrite(packet_start_code_prefix, 3, 1, fp);""",
    """  uint8_t ff_buf[length + 1]; /* length 可为 0，多留 1 字节避免零长数组 */

  memset(ff_buf, 0xff, length);

  /* offset_count += 3 */ fwrite(packet_start_code_prefix, 3, 1, fp);""",
    "ff_buf 改为 length+1",
)

PATH.write_text(text, encoding="utf-8", errors="surrogateescape")
print("\npack 边界修复完成，共 %d 项" % n_ok)
