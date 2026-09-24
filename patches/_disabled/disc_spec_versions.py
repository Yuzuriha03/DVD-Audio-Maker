#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""【已停用】把四个 IFO 的规范版本号从 0x12 对齐到 0x11 / 0x00。

## 状态：不在构建管线里

原先由 `02_build.py` 的 `fix_spec_versions()` 承担，调用处一直写着
`if False and not fix_spec_versions(...)`。2026-09-24 整理脚本时从
`02_build.py` 移出、原文保存在此，以免依据丢失。

## 依据（2026-09-23 实测，五张盘）

| 盘 | AMG@0x21 | ATSI@0x21 | ASVS@0x0E | SAMG@0x0E | 表现 |
|---|---|---|---|---|---|
| 巴赫 | **0x11** | **0x11** | **0x0000** | **0x0000** | ✅ 静图完全正常 |
| 李娜 | **0x11** | **0x11** | **0x0000** | **0x0000** | ✅ 静图完全正常 |
| Enigma | 0x12 | 0x12 | 0x0012 | 0x0012 | ❌ 只有前几个专辑刷新 |
| 本工程 | 0x12 | 0x12 | 0x0012 | 0x0012 | ❌ 同上 + 偶发崩溃 |

`dvda-author` 上游无条件写 0x12（DVD-Audio 1.2），而两张**从不出现任何
问题**的商业盘用的都是 1.1 形态。PowerDVD 8 的 1.2 解析路径有缺陷 ——
崩溃日志（`%LOCALAPPDATA%\\CyberLink\\CLHelper\\PowerDVD8.log`）里反复出现
`CLNavX.ax` 的 0xC0000094（整数除零）、0xC0000005（访问违规）和 `CLVsd.ax`
的 0xC0000005，而测商业盘时从未出现。

## ⚠️ 为什么没有合并进源码

**它的 ASVS 规则与 `patch_asvs_header_mode.py` 是同一个改动**，而后者实测
**会坏静图**（与调色板、静图表递进两个补丁一起被认定为「静图杀手」，
见 `patches/_disabled/patch_asvs_header_mode.py`）。

    patch_asvs_header_mode.py : asvs[0xE] 0x0012 → 0x0000
    本文件 RULES[ASVS]        : 字节 0x0F → 0x00（u16 @0x0E 即 0x0012→0x0000）

同一个改动，一个被认定有害、一个留在管线里 —— 这是整理时发现的矛盾。
既然证据指向「有害」，就一并停用（而不是把待验证的行为悄悄合并进源码）。

## 有据可查的部分

G 版基础上只改这 14 字节（4 个 IFO + 8 份 SAMG 副本）后，用户实测
**不再崩溃**。所以「版本号 0x12 与崩溃相关」有一定证据，但「改成 1.1 后
静图能否显示」没有正面结论。

要重新验证：把本文件放到 `scripts/` 下，在 `02_build.py` 的 `build_disc()`
里打包 ISO 之前调用一次即可（函数是幂等的）。
"""

import os


def fix_spec_versions(ts):
    """把四个 IFO（及其 .BUP）的**规范版本号**对齐到 0x11 / 0x00。

    `ts` 是盘目录（含 AUDIO_TS 系统文件的那一层）。
    返回 False 只在写文件失败时。幂等。
    """
    RULES = {
        b"DVDAUDIO-AMG":  (0x21, 0x11),   # AUDIO_TS.IFO / .BUP
        b"DVDAUDIO-ATS":  (0x21, 0x11),   # ATS_xx_0.IFO / .BUP
        b"DVDAUDIOASVS":  (0x0F, 0x00),   # AUDIO_SV.IFO / .BUP
        b"DVDAUDIOSAPP":  (0x0F, 0x00),   # AUDIO_PP.IFO
    }
    changed = []
    for name in sorted(os.listdir(ts)):
        if not name.endswith((".IFO", ".BUP")):
            continue
        path = os.path.join(ts, name)
        try:
            with open(path, "r+b") as fp:
                head = fp.read(0x30)
                for ident, (off, want) in RULES.items():
                    if not head.startswith(ident) or len(head) <= off:
                        continue
                    if head[off] != want:
                        fp.seek(off)
                        fp.write(bytes([want]))
                        changed.append((name, head[off], want))
                    break
        except OSError as e:
            print(f"[FAIL] 无法改写 {name} 的规范版本号：{e}")
            return False
    if changed:
        print("[版本] 规范版本号对齐 1.1 形态 "
              f"（{len(changed)} 处，如 {changed[0][0]} "
              f"0x{changed[0][1]:02x}→{changed[0][2]:02x}）✔")
    return True


if __name__ == "__main__":
    import sys
    if len(sys.argv) != 2:
        print(__doc__)
        print("用法: python3 disc_spec_versions.py <盘目录>")
        sys.exit(2)
    sys.exit(0 if fix_spec_versions(sys.argv[1]) else 1)
