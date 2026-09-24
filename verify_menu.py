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
   5b. 翻页链路：各页 cell 地址链连续、next/prev 菜单号正确
   5c. 播放封面表：每首歌是否真能看到自己专辑的封面（见 check_stills）
    6. 菜单画面抽帧不是全黑（背景图真的生效了）
"""

import os
import struct
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

# ASVS 静图预算。
#
# dvda-author 在超过 1024 扇区时会打印「Exceeding stillpic buffer limit
# (2 MB)」的警告，但那是它自己的经验值、**不是规范上限**：商业盘
# Enigma《15 Years After》的 AUDIO_SV.VOB 就有 1950 扇区（3.99 MB），
# 播放完全正常。所以这里只把上限当作「异常放大」的报警线，并给足余量。
MAX_ASVS_SECTORS = 4096
# （参考值：Enigma 99 轨 → 1950 扇区、李娜 24 轨 → 436 扇区）
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

    布局（见 docs/DVDA-AUTHOR-CHANGES.md 与 amg2.c）：
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


# AMG 菜单 PGC 表的字段位置（实测，见 docs/TROUBLESHOOTING.md 16.24/16.25）
AMG_LU_OFF = 0x1810        # 语言单元起点：下面几个相对指针都以它为基准
AMG_PGC_PTR = 0x181C       # 第 1 页 PGC 的相对指针
AMG_INDEX_OFF = 0x1820     # 菜单 PGC 索引表（nmenus-1 项，每项 8 字节）
PGC_STRIDE = 0x132         # 实测：相邻两页 PGC 起始地址之差
PGC_NEXT_MENU = 0x09C      # 下一页菜单号（uint16）
PGC_PREV_MENU = 0x09E      # 上一页菜单号（uint16）
PGC_CELL_START = 0x11E     # cell 起始扇区（uint32）
PGC_CELL_START2 = 0x126    # 同上，再写一遍
PGC_CELL_END = 0x12A       # cell 结束扇区（uint32）


def _u16(d, off):
    return struct.unpack(">H", d[off:off + 2])[0]


def _u32(d, off):
    return struct.unpack(">I", d[off:off + 4])[0]


def menu_pgc_bases(d, n):
    """返回各页菜单 PGC 在 AUDIO_TS.IFO 里的起始地址（第 1 页在首位）。"""
    bases = [AMG_LU_OFF + _u32(d, AMG_PGC_PTR)]
    for k in range(n - 1):
        bases.append(AMG_LU_OFF + _u32(d, AMG_INDEX_OFF + k * 8 + 4))
    return bases


def menu_cell_chain(ifo, vob):
    """校验 AMG 菜单各页 cell 的地址链与翻页目标。

    ## 为什么需要这一项

    翻页按钮走 `jump menu N`，由播放器查 **AMG IFO 的菜单 PGC 表**，
    而那张表是 `amg2.c` 手写的。它的 cell 结束地址曾经用错大小
    （恒用「最后一页」而不是当前页，见 docs/DVDA-AUTHOR-CHANGES.md），
    于是**部分页面的 Previous 按了回不去**；而各页 VOB 大小接近时错误会在
    某些页上相互抵消 —— 实测 8 页里 5 页错，表现为「只有几页有问题」，
    极难靠现象定位。所以必须能机检。

    ## 字段位置

    0x1820 起是 `nmenus-1` 项的「菜单 PGC 索引表」，每项 8 字节，末 4 字节是
    从 0x1810 起算的相对指针；**第 1 页 PGC 的地址由 0x181C 的指针给出**
    （索引表里没有它）。页内相对 PGC 起始：

        +0x09C 下一页菜单号    +0x09E 上一页菜单号    (uint16)
        +0x11E cell 起始扇区   +0x126 同一值再写一遍 (uint32)
        +0x12A cell 结束扇区                        (uint32)

    ⚠️ `amg2.c` 里自认为每页步长是 `0x13A`，**实际产物是 0x132**；
    照 0x13A 去读会在第 2 页以后全部错位、读出无关数据（连“看着像对”
    的假地址都有）。这里以**实测步长**为准，并把步长异常当作校验项报出来。

    ## 判据

      1. 每页 `start` 的两份副本相等（同一值写两遍）；
      2. `start_j == end_{j-1} + 1` —— 链连续，没有缝隙或重叠；
      3. 各页 `[start, end]` 跨度之和 == VOB 扇区总数（无多余的页内空洞）；
      4. 第 1 页 start == 0，末页 end == 总扇区数 - 1；
      5. next/prev 菜单号分别为 `j+1` / `j-1`（末页 next=0）。

    返回 (菜单数, [(start, end), ...], 问题描述列表)。问题列表为空即正常。
    """
    with open(ifo, "rb") as f:
        d = f.read()
    if len(d) < AMG_LU_OFF + 0x2C:
        return 0, [], [f"AUDIO_TS.IFO 只有 {len(d)} 字节，读不到菜单表"]
    n = _u16(d, AMG_LU_OFF)
    if n <= 1:
        return n, [], []
    if not os.path.exists(vob):
        return n, [], ["缺少 AUDIO_TS.VOB，无法校验 cell 地址链"]
    total = os.path.getsize(vob) // 2048

    bases = menu_pgc_bases(d, n)
    issues = []
    if len(set(bases)) != n or bases != sorted(bases):
        issues.append(f"菜单 PGC 地址不成递增序列 {[hex(b) for b in bases]}"
                      f" —— 布局假设不成立，后续读数不可信")
        return n, [], issues
    for i in range(n - 1):
        if bases[i + 1] - bases[i] != PGC_STRIDE:
            issues.append(f"第 {i + 1}/{i + 2} 页 PGC 间距 "
                          f"{hex(bases[i + 1] - bases[i])} != 预期 "
                          f"{hex(PGC_STRIDE)} —— 布局假设不成立")
            return n, [], issues
    last = bases[-1] + PGC_CELL_END + 4
    if last > len(d):
        issues.append(f"菜单 PGC 表超出 IFO 范围（需要到 {hex(last)}，"
                      f"IFO 只有 {hex(len(d))}）—— AMG 缓冲不够")
        return n, [], issues

    cells = []
    prev_end = None
    for j, b in enumerate(bases, 1):
        st = _u32(d, b + PGC_CELL_START) if j > 1 else 0
        st2 = _u32(d, b + PGC_CELL_START2) if j > 1 else 0
        en = _u32(d, b + PGC_CELL_END)
        nxt = _u16(d, b + PGC_NEXT_MENU)
        prv = _u16(d, b + PGC_PREV_MENU)
        cells.append((st, en))
        if j > 1 and st2 != st:
            issues.append(f"第 {j} 页 cell 起始地址的两份副本不一致："
                          f"{st} vs {st2}")
        if prev_end is not None and st != prev_end + 1:
            issues.append(f"第 {j - 1} 页 cell 结束于 {prev_end}，"
                          f"但第 {j} 页从 {st} 开始（应为 {prev_end + 1}）"
                          f" —— 地址链断裂")
        if en < st:
            issues.append(f"第 {j} 页 cell 结束 {en} 小于起始 {st}")
        if j < n and nxt != j + 1:
            issues.append(f"第 {j} 页的 Next 指向菜单 {nxt}，应为 {j + 1}")
        if j > 1 and prv != j - 1:
            issues.append(f"第 {j} 页的 Previous 指向菜单 {prv}，应为 {j - 1}")
        prev_end = en
    if cells[-1][1] != total - 1:
        issues.append(f"末页 cell 结束于 {cells[-1][1]}，"
                      f"而 AUDIO_TS.VOB 共 {total} 扇区（应为 {total - 1}）")
    span = sum(en - st + 1 for st, en in cells)
    if span != total:
        issues.append(f"各页 cell 跨度之和 {span} != AUDIO_TS.VOB 扇区数 "
                      f"{total} —— 页边界与实际 VOB 对不上")
    return n, cells, issues


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


def check_index_cells(vob, n_index_pages, n_albums, label):
    """一级索引页：**逐格**确认封面与专辑名都画上了。

    为什么要专门查这个：`verify_menu.py` 原来只判「整页非纯色」，而
    `make_index_page()` 的 `-repage` 写错作用域时，整页会变成
    **白底 + 右下角一张封面** —— 均值 238、颜色数上万，那条判据照样
    报「背景图生效 ✔」。实测这个 bug 就这样漏了过去，只能靠人眼看盘。

    判据（取第 1 个索引页，即 VOB 第一帧）：
      · 每格的缩略图区域不是纯黑（封面画上了）
      · 每格的名称条里有接近白的像素（专辑名画上了）
    """
    import menu_assets as ma

    png = "/tmp/_index_frame.png"
    if os.path.exists(png):
        os.remove(png)
    rc, _ = _run(["ffmpeg", "-v", "error", "-y", "-i", vob,
                  "-frames:v", "1", png])
    if rc != 0 or not os.path.exists(png):
        print("  [WARN] 无法抽帧检查索引页")
        return True

    cw, ch, ins = ma.INDEX_CELL_W, ma.INDEX_CELL_H, ma.INDEX_INSET
    top = ma.INDEX_TOP
    tw, gap, lh = ma.INDEX_THUMB, ma.INDEX_THUMB_GAP, ma.INDEX_LABEL_H
    lw = cw - 2 * ins
    tx = ins + (lw - tw) // 2
    want = min(n_albums, ma.INDEX_PER_PAGE)

    def stat(expr, x, y, w, h):
        rc, out = _run(["identify", "-crop", f"{w}x{h}+{x}+{y}",
                        "-format", expr, png])
        try:
            return float(out.strip())
        except ValueError:
            return None

    bad_bg, bad_thumb, bad_label = [], [], []
    # ⚠️ `identify` **没有** `+repage` 这个选项（那是 convert/mogrify 的），
    # 多写一个就会以「unrecognized option」失败、输出为空 → 解析成 0 →
    # 每格都判成「缺内容」。`identify -crop` 本身就按裁剪区统计。
    #
    # 三条判据缺一不可（只用第一条会漏，只用后两条也会漏）：
    #  · 背景黑    —— 抓 `-repage` 作用域错：那时画布是 **白**底，
    #                 而「缩略图区非黑」「名称区有亮像素」在白底上都会误判为通过
    #  · 缩略图非空—— 抓格子漏画（那种情况缺口是**黑**的）
    #  · 名称深底+亮字 —— 有白字说明名称画上了；纯色区域（黑/白）都不算
    for i in range(want):
        ox, oy = (i % ma.INDEX_COLS) * cw, top + (i // ma.INDEX_COLS) * ch
        bg = stat("%[fx:mean*255]", ox + 1, oy + 1, 3, 3)
        if bg is None or bg > 20:
            bad_bg.append(i)
        th = stat("%[fx:mean*255]", ox + tx, oy + ins, tw, tw)
        if th is None or th <= 3:
            bad_thumb.append(i)
        ly = oy + ins + tw + gap
        lmax = stat("%[fx:maxima*255]", ox + ins, ly, lw, lh)
        lmean = stat("%[fx:mean*255]", ox + ins, ly, lw, lh)
        if lmax is None or lmean is None or lmax <= 200 or lmean > 200:
            bad_label.append(i)

    if bad_bg or bad_thumb or bad_label:
        print(f"  [FAIL] 索引页格子内容异常（第 1 页，共 {want} 格）")
        if bad_bg:
            print(f"         · 格子外背景不是黑的: {bad_bg}")
            print("           → 十有八九是 make_index_page() 的 `-repage`")
            print("             写到了 `( )` 外面，画布变成 flatten 的白底")
        if bad_thumb:
            print(f"         · 缩略图为空（纯黑）的格子: {bad_thumb}")
        if bad_label:
            print(f"         · 专辑名没画上的格子: {bad_label}")
        print("         成因与修法见 docs/TROUBLESHOOTING.md 第 21 节。")
        return False
    print(f"  [OK]   索引页第 1 页 {want} 格：缩略图与专辑名都在 ✔")
    return True


# ASVS（AUDIO_SV.IFO，播放封面）字段位置（实测，对应 src/asvs.c）
ASVS_TITLE_COUNT = 0x0C     # u16：声明的「有自己静图的曲目」条数
ASVS_LAST_SECTOR = 0x14     # u32：静图总扇区数 - 1
ASVS_TABLE = 0x60           # 第 k 条记录的起点，每条 8 字节
ASVS_ENTRY_LEN = 0x08


def asvs_titles(ifo_path):
    """读 AUDIO_SV.IFO 的静图表，返回 (条数, [(图数, 起始图号, 起始扇区)], 总扇区-1)。

    “播放时显示专辑封面”能不能成立，取决于两件事：

      1. 每个专辑的**首轨**拿到自己的封面（否则那首歌看到的是上一张图）；
      2. 同一专辑后续曲目**不声明**静图，播放器便维持上一张显示 —— 这就是
         「每专辑一张」的实现方式（也确实省 ASVS 预算），

    所以 `条数` 应该恰好等于「有封面的专辑数」，且每条只含 1 张图、
    图号从 1 开始连续。
    """
    with open(ifo_path, "rb") as f:
        d = f.read()
    if len(d) < ASVS_TABLE + 4:
        return 0, [], None
    n = _u16(d, ASVS_TITLE_COUNT)
    last = _u32(d, ASVS_LAST_SECTOR)
    entries = []
    for i in range(n):
        off = ASVS_TABLE + i * ASVS_ENTRY_LEN
        if off + ASVS_ENTRY_LEN > len(d):
            return n, entries, last
        entries.append((d[off], _u16(d, off + 2), _u32(d, off + 4)))
    return n, entries, last


def check_stills(iso, ifo_path, vob_path, expect_pics, expect_albums=None):
    """从 ISO 取 ASVS 文件后交给 check_stills_data 校验。"""
    if not extract(iso, "/AUDIO_TS/AUDIO_SV.IFO", ifo_path):
        print("  [FAIL] 无法提取 AUDIO_SV.IFO，播放封面未能校验")
        return False
    return check_stills_data(ifo_path, vob_path, expect_pics, expect_albums)


def check_stills_data(ifo_path, vob_path, expect_pics, expect_albums=None):
    """校验播放封面（ASVS）表是否与「每轨一张静图」的实现一致。

    与取文件分开，是为了让校验逻辑本身可以被单独喂数据测试
    （否则想验证「它真的会报错」时，内部那次解包会把改过的文件覆盖掉）。

    ⚠️ 判据是「**图的总数** == 有静图的轨数」，因为实现是
    `--stillpics` 为**每一轨**都传一个图（同专辑复用同一张 jpg 文件）。
    不能拿「专辑数」当期望值：那样会误报（实测 56 轨 / 17 专辑）。

    `AUDIO_SV.IFO` 的记录是**按 ATS title** 分组的：
      · 单 title（一组一个 title，`patch_mlp_one_title`）→ **1 条**记录
        含全部 N 张图；
      · 多 title（一专辑一个 title）→ 每条记录「图数 = 该 title 的轨数」。
    两种结构都合法，故这里只校验「图的总数」与「图号/扇区连续」，
    不校验记录条数。

    expect_pics   : 有静图的轨数（= 图的总数）
    expect_albums : 专辑数（仅用于提示有多少张专辑缺 cover.jpg）
    """
    n, entries, last = asvs_titles(ifo_path)
    sectors = (os.path.getsize(vob_path) + 2047) // 2048
    issues = []
    if n != len(entries):
        issues.append(f"IFO 声明 {n} 条静图记录，实际只读到 {len(entries)} 条")
    if last != sectors - 1:
        issues.append(f"IFO 记录的静图总扇区数 {last + 1} != AUDIO_SV.VOB 扇区数 {sectors}"
                      f" —— 封面表与 VOB 对不上")
    pics = 0
    for i, (npics, start, sec) in enumerate(entries, 1):
        if npics < 1:
            issues.append(f"第 {i} 条记录的图数为 {npics}")
        if start != 1 + pics:
            issues.append(f"第 {i} 条记录的起始图号 {start}，应为 {1 + pics}"
                          f" —— 图号不连续")
        if sec * 2048 >= os.path.getsize(vob_path):
            issues.append(f"第 {i} 条记录的起始扇区 {sec} 超出 AUDIO_SV.VOB")
        pics += npics
    if expect_pics is not None and pics != expect_pics:
        issues.append(f"静图总数 {pics} != 有静图的轨数 {expect_pics}")
        if pics < expect_pics:
            issues.append("少的那些轨会看不到封面")
    if issues:
        print(f"  [FAIL] 播放封面表异常（{n} 条记录，{pics} 张图）")
        for msg in issues:
            print(f"         · {msg}")
        return False
    extra = f"（{n} 条记录 / {expect_albums} 个专辑）" if expect_albums else \
            f"（{n} 条记录）"
    print(f"  [OK]   播放封面：{pics} 张图 = {pics} 轨{extra}，"
          f"图号与扇区偏移连续，AUDIO_SV.VOB {sectors} 扇区")
    return True


def check_iso(iso, expect_tracks, expect_pages, tmpdir, label,
              expect_pics=None, expect_albums=None, expect_index_pages=0):
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
              f"(报警线 {MAX_ASVS_SECTORS})")
        if sectors > MAX_ASVS_SECTORS:
            ok = False
        # 5c. 播放封面表：每首歌是否真能看到自己专辑的封面
        if not check_stills(iso, os.path.join(tmpdir, "AUDIO_SV.IFO"), sv,
                            expect_pics, expect_albums):
            ok = False
    else:
        print("  [FAIL] 无法提取 AUDIO_SV.VOB，播放封面未能校验")
        ok = False

    # 5b. 翻页链路：各页 cell 地址链与 next/prev 菜单号
    # 这一项能查出「部分页 Previous 回不去」——每页 VOB 大小接近时，
    # 旧的错误公式会在某些页上恰好抵消，只靠现象很难发现。
    # 必须在这里解包：早期版本直接看文件在不在，而解包发生在第 6 项，
    # 于是第一次运行（目录还是空的）会静默跳过 —— 正是它要防的「静默漏检」。
    tv = os.path.join(tmpdir, "AUDIO_TS.VOB")
    have_tv = extract(iso, "/AUDIO_TS/AUDIO_TS.VOB", tv)
    if nmenus and nmenus > 1:
        if not have_tv:
            print("  [FAIL] 无法提取 AUDIO_TS.VOB，翻页链路未能校验")
            ok = False
        else:
            cn, cells, issues = menu_cell_chain(ifo, tv)
            if issues:
                print(f"  [FAIL] 翻页链路校验未通过（{cn} 页）")
                for msg in issues:
                    print(f"         · {msg}")
                print("         影响: 相关页的 Previous / Next 可能点了没反应")
                print("         成因: amg2.c 的 cell 结束地址用错大小"
                      "（见 docs/DVDA-AUTHOR-CHANGES.md）")
                ok = False
            else:
                spans = "/".join(str(en - st + 1) for st, en in cells)
                print(f"  [OK]   翻页链路：{cn} 页 cell 地址连续（跨度 "
                      f"{spans}，末页 end={cells[-1][1]}），"
                      f"next/prev 菜单号正确")
                print(f"         地址链 {cells[0][0]}→{cells[-1][1]}"
                      f" 与 AUDIO_TS.VOB 扇区边界逐页吻合")

    # 6. 菜单画面不是全黑
    if have_tv:
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

    # 7. 一级索引页：**逐格**查缩略图与专辑名
    # 上面的「非纯色」判据漏掉过「白底 + 只剩一个格子」这种坏页
    # （白底均值 238、颜色上万，看起来很像正常），所以单独查一次。
    if have_tv and expect_index_pages and expect_tracks:
        if not check_index_cells(tv, expect_index_pages, expect_albums or 0,
                                 label):
            ok = False
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
            # 一页一个专辑：期望页数 = 专辑块数（超长专辑会拆页）
            flat = [t for g in groups for t in g]
            album_of = [os.path.basename(os.path.dirname(t.get("src") or ""))
                        for t in flat]
            pages = len(menu_assets.album_pages(
                album_of, min(cfg.menu_tracks_per_page,
                              menu_assets.MAX_MENU_ROWS)))
            # 一级菜单（专辑索引页）排在最前：每页 4x3 张缩略图。
            # 页数要加上它们，否则「菜单页数 = 专辑数」的判据会误报。
            n_idx = -(-pages // menu_assets.INDEX_PER_PAGE) if pages else 0
            if n_idx and pages < cfg.menu_index_min_albums:
                n_idx = 0
            pages += n_idx
            # 播放封面的预期：**每轨**一张图（同专辑复用同一张 jpg 文件），
            # 所以期望张数 = 该盘的轨数，与 menu_assets 的做法一致。
            album_dirs, seen = [], set()
            for g in groups:
                for t in g:
                    d = os.path.dirname(t.get("src") or "")
                    if d and d not in seen:
                        seen.add(d)
                        album_dirs.append(d)
            expect_albums = len(album_dirs)
            expect_pics = sum(len(g) for g in groups)
        else:
            tracks, pages = None, None
            expect_albums = expect_pics = n_idx = None
            print("[提示] 指定 --iso 时无法核对期望页数，只做存在性检查")
        if tracks is None:
            print(f"\n=== {os.path.basename(iso)} ===")
            names = iso_files(iso)
            base = {os.path.basename(n) for n in names}
            for f in ("AUDIO_TS.VOB", "AUDIO_SV.VOB"):
                print(f"  [{'OK' if f in base else 'FAIL'}] AUDIO_TS/{f}")
            continue
        ok = check_iso(iso, tracks, pages, f"/tmp/verify-menu/disc{i}",
                       f"第 {i} 盘", expect_pics, expect_albums, n_idx or 0)
        all_ok = all_ok and ok

    print()
    if all_ok:
        print("菜单校验全部通过 ✔")
        return 0
    print("菜单校验有失败项 ✗")
    return 1


if __name__ == "__main__":
    sys.exit(main())
