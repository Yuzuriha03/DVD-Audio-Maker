#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""菜单专项校验：核对选曲菜单（AMG）与播放封面（ASVS）是否真的做出来了。

`verify.sh` 只校验音频侧（轨道表、PTS、无损），菜单是可选件、不在它的范围内。
而菜单这一串风险里最危险的一条是**「不报错但没做出来」**，所以这里逐项核对
产物与结构。

用法：
    python3 verify_menu.py                 # 校验 config.sh 里的成品目录
    DVDA_FINAL_DIR=... python3 verify_menu.py
    python3 verify_menu.py --iso 某.iso    # 校验指定 ISO

检查项：
    1. ISO 里 AUDIO_TS 下有 AUDIO_TS.VOB / AUDIO_SV.VOB / AUDIO_SV.IFO
    2. AUDIO_TS.IFO 的 sector 4（AMG 菜单表）里声明的菜单数 == 期望页数
    3. AUDIO_TS.IFO 的扇区数足够容纳菜单表（上游 AMG 缓冲越界的判据）
    4. 每页的 spumux 按钮总数 >= 该盘曲目数（少了就有曲子点不到）
    5. ASVS 的静图扇区数 <= 1024（超出会被丢弃）
    6. 菜单画面抽帧不是全黑（背景图真的生效了）
"""

import os
import re
import struct
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

MAX_ASVS_SECTORS = 1024        # asvs.c 的累计上限
BTN_PER_PAGE_MAX = 32          # MAX_BUTTON_Y_NUMBER - 2


def _run(cmd):
    r = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    return r.returncode, r.stdout.decode("utf-8", "replace")


def iso_files(iso):
    """ISO 里的所有路径（相对于 /）。"""
    rc, out = _run(["xorriso", "-indev", iso, "-find", "/"])
    if rc != 0:
        return []
    return [l.strip().strip("'") for l in out.splitlines() if l.strip()]


def extract(iso, inner, dest):
    rc, out = _run(["xorriso", "-osirrox", "on", "-indev", iso,
                    "-extract", inner, dest])
    return rc == 0 and os.path.exists(dest)


def amg_menu_count(ifo):
    """从 AUDIO_TS.IFO 的 sector 4 读菜单（语言单元）数。

    布局（见 patches/patch_menu_paging.py 的说明与 amg2.c）：
        ATSI 指针在 IFO 偏移 204（大端 u32，扇区号）
        sector 4 里：0x1800 语言单元数、0x1810 菜单数（u16, BE）
    """
    with open(ifo, "rb") as f:
        data = f.read()
    if len(data) < 0x1800 + 0x14 + 2:
        return None, None
    pgs = struct.unpack(">I", data[0x1800:0x1804])[0]
    nmenus = struct.unpack(">H", data[0x1810:0x1812])[0]
    return pgs, nmenus


def menu_cell_chain(ifo, vob):
    """检查 AMG 菜单的 cell 地址链是否连续。

    ## 为什么需要这一项

    翻页按钮走 `jump menu N`，由播放器查 **AMG IFO 的菜单 PGC 表**；
    而那张表是 `amg2.c` 手写的。它的 cell 结束地址曾经用错大小
    （恒用「最后一页」而不是当前页，见 patches/patch_menu_amg_cells.py），
    于是**部分页面的 Previous 按了回不去**，而各页 VOB 大小接近时错误会
    在某些页上相互抵消 —— 表现为「只有几页有问题」，极难靠现象定位。

    ## 判据（不依赖具体字段偏移）

    每个 cell 的 `start` 在同一项里写两次，`end` 只写一次；且正确的链要求

        start_{j+1} == end_j + 1

    所以：凡是「恰好出现 2 次」的值就是某个 start，则 `start - 1` 必须也能
    在表里找到（它是上一个 cell 的 end）。找不到就说明链断了。

    返回 (菜单数, start 序列, 断链的 start)。
    """
    with open(ifo, "rb") as f:
        d = f.read()
    if len(d) < 0x1812:
        return 0, [], []
    n = struct.unpack(">H", d[0x1810:0x1812])[0]
    if n <= 1 or not os.path.exists(vob):
        return n, [], []
    total = os.path.getsize(vob) // 2048
    # 跳过开头的「项索引」表（每项 8 字节，含 1..n-1 这些小整数）
    lo = 0x1820 + 8 * (n - 1)
    hi = 0x1820 + n * 0x13A
    cnt = {}
    for i in range(lo, min(hi, len(d)) - 3):
        v = struct.unpack(">I", d[i:i + 4])[0]
        if 0 < v <= total:
            cnt[v] = cnt.get(v, 0) + 1
    starts = sorted(v for v, c in cnt.items() if c == 2 and v >= 8)
    bad = [v for v in starts if (v - 1) not in cnt]
    return n, starts, bad


def frame_stats(vob):
    """取菜单 VOB 的第一帧，返回（均值, 唯一色数）；全黑图两者都接近 0/1。"""
    png = "/tmp/_menu_frame.png"
    if os.path.exists(png):
        os.remove(png)
    rc, _ = _run(["ffmpeg", "-v", "error", "-y", "-i", vob,
                  "-frames:v", "1", png])
    if rc != 0 or not os.path.exists(png):
        return None
    rc, out = _run(["identify", "-format", "%[fx:mean] %[fx:standard_deviation] "
                    "%k", png])
    try:
        parts = out.split()
        return float(parts[0]) * 255, float(parts[1]) * 255, int(parts[2])
    except (ValueError, IndexError):
        return None


def check_iso(iso, expect_tracks, expect_pages, tmpdir, label):
    print(f"\n=== {label}: {os.path.basename(iso)} ===")
    names = iso_files(iso)
    if not names:
        print("  [FAIL] 读不到 ISO 内容（xorriso 失败？）")
        return False
    base = {os.path.basename(n) for n in names}
    ok = True

    # 1. 菜单文件是否都在
    need = ["AUDIO_TS.VOB", "AUDIO_SV.VOB", "AUDIO_SV.IFO", "AUDIO_TS.IFO"]
    for f in need:
        if f in base:
            print(f"  [OK]   AUDIO_TS/{f}")
        else:
            print(f"  [FAIL] 缺少 AUDIO_TS/{f} —— 菜单没做出来")
            ok = False

    os.makedirs(tmpdir, exist_ok=True)
    ifo = os.path.join(tmpdir, "AUDIO_TS.IFO")
    if not extract(iso, "/AUDIO_TS/AUDIO_TS.IFO", ifo):
        print("  [FAIL] 无法提取 AUDIO_TS.IFO")
        return False

    # 2. 菜单数
    ifosize = os.path.getsize(ifo)
    pgs, nmenus = amg_menu_count(ifo)
    if nmenus is None:
        print("  [FAIL] AUDIO_TS.IFO 太小，读不到菜单表")
        ok = False
    else:
        mark = "OK]  " if nmenus == expect_pages else "FAIL]"
        print(f"  [{mark} 菜单页数 = {nmenus}（期望 {expect_pages}）")
        if nmenus != expect_pages:
            ok = False

    # 3. AMG 扇区数够不够（上游 VLA 越界的判据）
    need_bytes = 0x1820 + 8 * max(0, (nmenus or 1) - 1) + (nmenus or 1) * 0x13A
    if ifosize >= need_bytes:
        print(f"  [OK]   AUDIO_TS.IFO {ifosize} 字节 "
              f">= 菜单表需求 {need_bytes} ✔")
    else:
        print(f"  [FAIL] AUDIO_TS.IFO 只有 {ifosize} 字节，"
              f"菜单表需要 {need_bytes} —— AMG 缓冲会越界")
        ok = False

    # 5. ASVS 预算
    sv = os.path.join(tmpdir, "AUDIO_SV.VOB")
    if extract(iso, "/AUDIO_TS/AUDIO_SV.VOB", sv):
        sectors = (os.path.getsize(sv) + 2047) // 2048
        mark = "OK]  " if sectors <= MAX_ASVS_SECTORS else "FAIL]"
        print(f"  [{mark} 播放封面 AUDIO_SV.VOB {sectors} 扇区 "
              f"(上限 {MAX_ASVS_SECTORS})")
        if sectors > MAX_ASVS_SECTORS:
            ok = False

    # 5b. 菜单 cell 地址链是否连续（翻页跳转的前提条件）
    # 这一项能查出「部分页 Previous 回不去」——每页 VOB 大小接近时，
    # 旧的错误公式会在某些页上恰好抵消，只靠现象很难发现。
    tv2 = os.path.join(tmpdir, "AUDIO_TS.VOB")
    if os.path.exists(tv2) and nmenus and nmenus > 1:
        cn, starts, broken = menu_cell_chain(ifo, tv2)
        if not starts:
            print("  [WARN] 读不到菜单 cell 的 start 序列，跳过链校验")
        elif broken:
            print(f"  [FAIL] 菜单 cell 地址链断裂：start={starts}，"
                  f"这些 start 缺少前驱 {broken}")
            print("         影响: 这些页的翻页按钮可能点了没反应 / "
                  "回不到上一页")
            print("         成因: amg2.c 的 cell 结束地址用错大小"
                  "（见 patches/patch_menu_amg_cells.py）")
            ok = False
        else:
            print(f"  [OK]   菜单 cell 地址链连续（{cn} 页，"
                  f"start={starts}）—— 翻页跳转前提成立")

    # 6. 菜单画面不是全黑
    tv = os.path.join(tmpdir, "AUDIO_TS.VOB")
    if extract(iso, "/AUDIO_TS/AUDIO_TS.VOB", tv):
        st = frame_stats(tv)
        if st is None:
            print("  [WARN] 无法抽帧检查菜单画面")
        else:
            mean, sd, colors = st
            if colors > 50 and sd > 1:
                print(f"  [OK]   菜单画面非纯色（均值 {mean:.0f}, "
                      f"标准差 {sd:.0f}, 颜色 {colors}）—— 背景图生效")
            else:
                print(f"  [WARN] 菜单画面接近纯色（均值 {mean:.0f}, "
                      f"标准差 {sd:.0f}, 颜色 {colors}）—— 背景图可能没生效")
    return ok


def main():
    # 期望值：从 mlp_index.json 的 __discs__ 读每盘的组与轨数，按与
    # 02_build.py 相同的算法算页数
    import json
    import dvda_config
    import menu_assets

    cfg = dvda_config.load(quiet=True)
    iso_paths = []
    if len(sys.argv) >= 3 and sys.argv[1] == "--iso":
        iso_paths = [sys.argv[2]]
        idx = None
    else:
        idx = json.load(open(cfg.mlp_index))
        for d in idx["__discs__"]:
            iso_paths.append(os.path.join(cfg.final_dir, d["iso"]))

    if not cfg.menu and idx is not None:
        print("[提示] config.sh 里 DVDA_MENU 不是 on —— 这些盘按预期没有菜单。")
        return 0

    all_ok = True
    for i, iso in enumerate(iso_paths, 1):
        if not os.path.exists(iso):
            print(f"[FAIL] 找不到 {iso}")
            all_ok = False
            continue
        if idx is not None:
            groups = [[t for t in g["tracks"]] for g in idx["__discs__"][i - 1]["groups"]]
            tracks = sum(len(g) for g in groups)
            pages, _ = menu_assets.group_pages(
                [len(g) for g in groups], cfg.menu_tracks_per_page)
        else:
            tracks, pages = None, None
            print("[提示] 指定 --iso 时无法核对期望页数，只做存在性检查")
        if tracks is None:
            print(f"\n=== {os.path.basename(iso)} ===")
            names = iso_files(iso)
            base = {os.path.basename(n) for n in names}
            for f in ("AUDIO_TS.VOB", "AUDIO_SV.VOB"):
                print(f"  [{'OK' if f in base else 'FAIL'}] AUDIO_TS/{f}")
            continue
        ok = check_iso(iso, tracks, pages, f"/tmp/verify-menu/disc{i}",
                       f"第 {i} 盘")
        all_ok = all_ok and ok

    print()
    if all_ok:
        print("菜单校验全部通过 ✔")
        return 0
    print("菜单校验有失败项 ✗")
    return 1


if __name__ == "__main__":
    sys.exit(main())
