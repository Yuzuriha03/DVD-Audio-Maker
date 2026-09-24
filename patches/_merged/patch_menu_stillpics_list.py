#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""修 `--stillpics` 的**文件列表模式**（`--stillpics a.jpg:b.jpg:...`）必崩。

`--stillpics` 有两种用法：
  · 目录模式：`--stillpics <目录>`，目录里放 pic_000.jpg、pic_001.jpg …
    → 该分支会把 `stillpicdir` 设成这个目录
  · 文件列表模式：`--stillpics a.jpg:b.jpg:...`（`:` 分轨、`,` 分该轨多张）
    → 该分支**不设** `stillpicdir`

而 `create_stillpic_directory()` 一进来就无条件
`change_directory(globals->settings.stillpicdir, globals)`，
`change_directory` 在路径不存在时 `clean_exit(-1)`。

`stillpicdir` 的默认值是在 main() 里早于命令行解析就填好的
（`dvda-author.c`: `if (!stillpicdir) stillpicdir = strdup(tempdir)`），
那时 `-D/--tempdir` 还没生效，所以它是**编译期默认 tempdir**
（<cwd>/.dvda-author/temp，通常不存在）→
文件列表模式必然以
`[ERR] Impossible to cd to <...>/.dvda-author/temp.` + 退出码 255 结束。

修法：文件列表分支里把 `stillpicdir` 指到**当前生效的 tempdir**。
这同时也让两边路径一致 —— `create_stillpic_directory()` 把图片复制到
`<tempdir>/pic_%03u.jpg`（k 只在非空项递增），而 `create_mpg()` 读的是
`<stillpicdir>/pic_%03u.jpg`；两者必须同一个目录。

修好之后就能用空项表示「本轨复用上一张图」，即
`--stillpics cover0.jpg::cover1.jpg`（第 2 轨沿用第 1 轨的图）——
这是「每专辑只编一张封面」省静图预算的关键（ASVS 上限 1024 扇区/盘）。
"""
import pathlib
import sys

PATH = pathlib.Path(
    "/root/dvda-author-mlp8/src/command_line_parsing.c")

if not PATH.exists():
    print("[FAIL] 找不到 %s" % PATH)
    sys.exit(1)

text = PATH.read_text(encoding="utf-8", errors="surrogateescape")
n_ok = 0

OLD = """      else
        {
          uint32_t size = 0;
          pics_per_track = fn_strtok(stillpic_string, ':',
                                     pics_per_track, &size, 0,
                                     NULL, NULL, globals);
          indir = false;
        }
"""

NEW = """      else
        {
          uint32_t size = 0;
          pics_per_track = fn_strtok(stillpic_string, ':',
                                     pics_per_track, &size, 0,
                                     NULL, NULL, globals);
          indir = false;

          /* 文件列表模式没有可 cd 的目录，而 create_stillpic_directory()
             一进来就 change_directory(stillpicdir)（不存在则 clean_exit(-1)）。
             stillpicdir 的默认值在 main() 里早于命令行解析就已填好，那时
             -D/--tempdir 还没生效，所以它是编译期默认的 <cwd>/.dvda-author/temp
             —— 通常不存在，于是本模式以前必然以退出码 255 结束。
             指到当前 tempdir 既避开 cd 失败，又与 create_stillpic_directory()
             复制图片的目的地、create_mpg() 读取图片的路径三者一致。 */
          free(globals->settings.stillpicdir);
          globals->settings.stillpicdir = strdup(globals->settings.tempdir);
        }
"""

if OLD not in text:
    if "文件列表模式没有可 cd 的目录" in text:
        print("[SKIP] 已应用过")
        sys.exit(0)
    print("[MISS] 未找到目标代码")
    sys.exit(1)

if text.count(OLD) != 1:
    print("[MISS] 匹配到 %d 处，需唯一" % text.count(OLD))
    sys.exit(1)

PATH.write_text(text.replace(OLD, NEW, 1), encoding="utf-8",
                errors="surrogateescape")
print("[OK] 文件列表模式指向 tempdir（修 cd 失败）")
print("\n静图文件列表修复完成，共 1 项")
