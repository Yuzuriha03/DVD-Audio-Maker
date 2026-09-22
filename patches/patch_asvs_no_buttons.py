#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把 ASVS 的 `0x19`（activates buttons）从硬编码的 1 改成 0。

## 依据（2026-09-22 实测三张商业盘）

| 盘 | `0x18` | `0x19` | 结果 |
|---|---|---|---|
| Enigma《15 Years After》 | 0x53 | **0** | 正常 |
| 李娜精选集 | 0x43 | **0** | 商业盘 |
| 巴赫布兰登堡协奏曲 | 0x43 | **0** | 商业盘 |
| 本工程 | 0x53 | **1** | 上一曲/下一曲失效 |

三张商业盘**无一例外都是 0**，只有 dvda-author 写 1。

源码原文（`asvs.c`）：

    asvs[0x19] = 0x1; // activates buttons // number of menus ? // or 0

注释自己就写了「**or 0**」，说明作者也不确定；此处选商业盘一致的值 0。

## 为什么怀疑它

字段名是「**activates buttons**」。我们的静图（`--stillpics` 生成的
720×576 画面）**没有任何按钮**，声明「按钮已激活」与实物不符。
播放器若据此进入「按钮导航」模式，`上一段/下一段` 会被按钮逻辑吞掉
—— 与实测现象（曲目显示 0、上一段无反应、下一段跳回第 1 首）吻合。

## 说明

只改这 1 字节，不动其它任何字段；`0x18`（Enigma 0x53 / 李娜巴赫 0x43）
两者都有商业盘在用，故保留 dvda-author 的 0x53。
"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "商业盘一致为 0"

OLD = """  asvs[0x19] = 0x1; // activates buttons // number of menus ? // or 0
"""

NEW = """  /* 商业盘一致为 0（Enigma 0x53/0x00、李娜 0x43/0x00、巴赫 0x43/0x00）。
     原值 1 的语义是「激活按钮」，而 --stillpics 生成的静图**没有按钮**，
     声明与实物不符；播放器可能据此进入按钮导航模式，把「上一段/下一段」
     吞掉（实测症状：曲目显示 0、上一段无反应、下一段跳回第 1 首）。
     源码注释本身也写了「or 0」。 */
  asvs[0x19] = 0x0;
"""


def main():
    p = SRC / "asvs.c"
    if not p.exists():
        print("[FAIL] 找不到 %s" % p)
        return 1
    t = p.read_text(encoding="utf-8", errors="surrogateescape")
    if MARK in t:
        print("[SKIP] 已应用过（asvs.c 含标记）")
        return 0
    if t.count(OLD) != 1:
        print("[FAIL] 目标行匹配 %d 次（应为 1）" % t.count(OLD))
        return 1
    p.write_text(t.replace(OLD, NEW, 1), encoding="utf-8",
                 errors="surrogateescape")
    print("[OK]   asvs.c: 0x19 activates-buttons 1 → 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
