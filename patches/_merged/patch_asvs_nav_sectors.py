#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把静图的导航扇区从 mplex 的「空壳」改成商业盘形态（去掉空 DSI）。

> ## 🚫 本脚本已作废 —— 不要运行
>
> 它的效果**已经改由 C 实现**：`src/menu.c` 的
> `dvda_rewrite_nav_sector()`，在 `generate_background_mpg()` 的静图循环里
> 紧跟 `dvda_pad_program_end()` 调用。
>
> 本脚本要修改的目标（`02_build.py` 的 `build_disc()` 里的 AOB 补零段）
> **已被删除**，所以它现在必定报 `[FAIL] 找不到 AOB 补零段`。
> 下面的实现细节只是历史记录；要重新应用请照它写 C 代码，别跑这个脚本。

## 依据（2026-09-23 实测，五张盘）

`AUDIO_SV.VOB` 里每张静图的**第 1 个扇区是导航扇区**：

| 盘 | 起始码 | PCI 长度 | DSI |
|---|---|---|---|
| 巴赫 | `ba, bb@0x0e, bf@0x23, be@0x312` | 745 | ❌ 无 |
| 李娜 | 同上**逐字节相同** | 745 | ❌ 无 |
| Enigma | 同上**逐字节相同** | 745 | ❌ 无 |
| 本工程 | `ba, bb@0x0e, bf@0x26, **bf@0x400**` | **980** | ✅ **有** |

`mplex -f 8` 的手册自己就写了它写的是空壳：

> 8 - DVD (with NAV sectors). Don't get too excited. This is really a very
> minimal mux format. It includes **empty versions** of the peculiar VOBU
> start sectorsDVD VOB's include.

DSI（Data Search Information）位于扇区内**固定偏移 0x400**，装的是「前后 VOBU
的扇区指针」。mplex 写的是**全零** → 等于声明「下一个 VOBU 在扇区 0」。
实测从菜单跳到第 50 曲时播放器直接崩溃。

三张商业盘的导航扇区**各自盘内恒定**（第 1 张与第 2 张逐字节相同），
且 PCI 里只有 11 个非零字节（Enigma 只剩 1 个）→ **播放器不看 PCI 内容**，
只要导航扇区存在且不含误导性的 DSI。

## 改法

在 `02_build.py` 的 `build_disc()` 里、**打包 ISO 之前**调用
`fix_asvs_nav_sectors()`：按 `AUDIO_SV.IFO` 的 ASVU 记录（0x60 起，每条固定
8 字节）+ 每图偏移表（0x378 起，每条 2×图数，**两个独立游标**）定位每张静图的
导航扇区，整块改写成商业盘形态：

    pack_header(14B, 与本工程原本逐字节相同)
    + 系统头(15B) + PCI(745B) + 0xBE 填充包 + 0xFF 补满 2048

扇区数不变，故 ASVS 偏移表、每图大小、其它任何表都不用动。**幂等**。

⚠️ xorriso 解出来的文件是只读的（`-r-xr-xr-x`），测试时要先 `chmod u+w`。
"""
import pathlib
import sys

PATH = pathlib.Path("/home/yyz57/dvda/scripts/02_build.py")
MARK = "fix_asvs_nav_sectors"

FUNC = '''# ---- 静图导航扇区（AUDIO_SV.VOB 里每张静图的第 1 个扇区） ----------------
#
# mplex 的 `-f 8`（其手册原文: "DVD (with NAV sectors) … includes **empty
# versions** of the peculiar VOBU start sectors"）写的是**空壳导航包**：
# PCI 980 字节之后再挂一个整 1018 字节的 DSI，而 PCI/DSI 的内容基本全零。
#
# DSI（Data Search Information）装的是「前后 VOBU 的扇区指针」。
# 全零即「下一个 VOBU 在扇区 0」→ 播放器一寻址就跳回 VOB 开头读垃圾。
# 实测：从菜单跳到第 50 曲时播放器直接崩溃。
#
# 三张商业盘（巴赫 / 李娜 / Enigma）静图的导航扇区**逐字节相同**，
# 形态为「PCI 745 字节 + 0xBE 填充」，**没有 DSI**：
#
#   00 00 01 BA | 00 00 01 BB 系统头(15B) | 00 00 01 BF 02 E9 PCI(745B)
#   | 00 00 01 BE 04 E8 + 1256×0xFF
#
# 而它们的 PCI 里只有 11 个非零字节（Enigma 更只剩 1 个），
# 且**第 1 张与第 2 张逐字节相同** —— 说明播放器根本不看 PCI 的内容，
# 只要导航扇区「存在且不含误导性的 DSI」。故这里整块改写成商业盘形态。
#
# 扇区数不变（都是 1 扇区），所以 ASVS 偏移表、每图大小、其余任何表都不用动。
_ASVS_NAV_TAIL = (
    b"\\x00\\x00\\x01\\xbb\\x00\\x0f"                       # 系统头，长 15
    b"\\x80\\xc4\\xe1\\x00\\x61\\x7f\\xb9\\xe0\\xe8\\xbd\\xe0\\x34\\xbf\\xe0\\x01"
    b"\\x00\\x00\\x01\\xbf\\x02\\xe9"                       # PCI 包，数据 745 字节
    b"\\x02" + b"\\x00" * 4 + b"\\x8c\\xa0" + b"\\xff" * 8 + b"\\x00" * 730
    + b"\\x00\\x00\\x01\\xbe\\x04\\xe8" + b"\\xff" * 1256    # 填充包，数据 1256
)


def fix_asvs_nav_sectors(ts):
    """把 AUDIO_SV.VOB 里每张静图的导航扇区改成商业盘形态（去掉空 DSI）。

    返回 True 表示成功（或无需处理）。只改每个静图第 1 个扇区的内容，
    不改长度，故对其它任何表都没有影响。
    """
    ifo = os.path.join(ts, "AUDIO_SV.IFO")
    vob = os.path.join(ts, "AUDIO_SV.VOB")
    if not (os.path.exists(ifo) and os.path.exists(vob)):
        return True

    with open(ifo, "rb") as f:
        h = f.read(4096)
    with open(vob, "r+b") as f:
        data = bytearray(f.read())

    # ASVU 记录从 0x60 起（每条固定 8 字节），每图偏移表从 0x378 起
    # （每条 2×图数）。两者是**独立游标**——与 asvs.c 的写出顺序一一对应。
    n_asvu = int.from_bytes(h[0x0C:0x0E], "big")
    k, t, total, fixed = 0x60, 0x378, 0, 0
    for _ in range(n_asvu):
        n_pics = h[k]
        base = int.from_bytes(h[k + 4:k + 8], "big")
        for i in range(n_pics):
            sect = base + int.from_bytes(h[t + 2 * i:t + 2 * i + 2], "big")
            off = sect * 2048
            if off + 2048 > len(data):
                print(f"[FAIL] 静图导航扇区 {sect} 超出 AUDIO_SV.VOB")
                return False
            total += 1
            # 判据：PCI 之后紧接着 DSI（0x400 起 00 00 01 BF）= mplex 空壳
            if data[off + 0x400:off + 0x404] == b"\\x00\\x00\\x01\\xbf":
                ref = bytes(data[off:off + 0x0E]) + _ASVS_NAV_TAIL
                if len(ref) != 2048:
                    print(f"[FAIL] 参考导航扇区长度 {len(ref)} != 2048")
                    return False
                data[off:off + 2048] = ref
                fixed += 1
        t += 2 * n_pics
        k += 8

    if fixed == 0 and total:
        print(f"[静图] 导航扇区已是商业盘形态（{total} 张）✔")
        return True

    with open(vob, "r+b") as f:
        f.write(data)
    print(f"[静图] 导航扇区改写 {fixed}/{total} 张（去掉 mplex 的空 DSI，"
          f"对齐商业盘）✔")
    return True


'''

CALL_OLD = """    for aob in sorted(glob.glob(os.path.join(out, "AUDIO_TS", "*.AOB"))):
        size = os.path.getsize(aob)
        rem = size % 2048
        if rem:
            with open(aob, "ab") as fp:
                fp.write(b"\\x00" * (2048 - rem))
            print(f"[补齐] {os.path.basename(aob)} 补 {2048 - rem} 字节至扇区边界")
"""

CALL_NEW = CALL_OLD + """
    # 静图导航扇区：mplex 写的是带空 DSI 的"空壳"，会让播放器跳曲时崩溃。
    # 必须在打包 ISO 之前做（只改扇区内容、不改长度，故不影响任何表）。
    if not fix_asvs_nav_sectors(os.path.join(out, "AUDIO_TS")):
        return False
"""


def main():
    if not PATH.exists():
        print("[FAIL] 找不到 %s" % PATH)
        return 1
    t = PATH.read_text(encoding="utf-8", errors="surrogateescape")
    if MARK in t:
        print("[SKIP] 已应用过")
        return 0

    # 1) 插函数：放在 build_disc 定义之前
    anchor = "\ndef build_disc(disc_index, groups):"
    if t.count(anchor) != 1:
        print("[FAIL] 找不到 build_disc 定义（匹配 %d 次）" % t.count(anchor))
        return 1
    t = t.replace(anchor, "\n" + FUNC + "def build_disc(disc_index, groups):", 1)

    # 2) 插调用：AOB 补零之后、mkisofs 之前
    if t.count(CALL_OLD) != 1:
        print("[FAIL] 找不到 AOB 补零段（匹配 %d 次）" % t.count(CALL_OLD))
        return 1
    t = t.replace(CALL_OLD, CALL_NEW, 1)

    PATH.write_text(t, encoding="utf-8", errors="surrogateescape")
    print("[OK]   02_build.py: 新增 fix_asvs_nav_sectors + 在打包前调用")
    return 0


if __name__ == "__main__":
    sys.exit(main())
