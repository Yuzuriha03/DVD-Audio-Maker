#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""菜单素材生成：分页、菜单文字链、每页背景图、每轨静图列表。

## 输出

`build_menu()` 返回一份 `MenuPlan`，`args()` 再把它变成 dvda-author 的参数。
素材文件落在 `<BUILD_DIR>/menu/discN/`：
    blankscreen.png   文字浮层底图（全透明 → 背景视频透出）
    bg<N>.jpg         第 N 页的背景（该页专辑封面拼图，压暗后）
    still<N>.jpg      第 N 张「播放时封面」（每专辑一张）
    stillpics.txt     `--stillpics` 的长参数（回车分隔、调用方拼成一行）

## 为什么页数要按音频组分别算

菜单按钮是 `jump group G track K`，而 dvda-author 的画页循环
（menu.c / xml.c 的非分层分支）在 `--ncolumns=1` 时**一页只能装一个音频组**
（`do { ... } while ((group < img->ncolumns) && ...)`，group 每完成一个组 +1）。
每页行数 `R = min(32, ceil(总轨数 / 页数))`（见 `docs/DVDA-AUTHOR-CHANGES.md`）。

所以页数必须满足 `sum_g ceil(n_g / R) <= 页数`，否则排在后面的组的曲目
**不会出现在菜单里**（不报错）。这里用迭代求满足条件的最小页数。

## 分组顺序 = 菜单顺序

`group_by_rate()` 在每个 (采样率, 位深) 桶内保持全局顺序，且在专辑边界切分，
所以「组 1 全部轨 → 组 2 全部轨 → …」这个顺序里，专辑仍然是连续的。
菜单顺序就必须用这个顺序（而不是全局顺序），否则按钮的
`jump group G track K` 会对不上。
"""

import os
import re
import shutil
import subprocess

# ---- 画面常量（与 dvda-author 一致：PAL 720x576） ----
FRAME_W, FRAME_H = 720, 576          # 菜单 / 静图画面
MAX_BUTTONS = 32            # MAX_BUTTON_Y_NUMBER - 2
MIN_POINTSIZE = 7
# 上限 30：小标题（专辑名）是 `0.8 × 字号` 画的，而大标题（光盘标题）在
# menu.c 里硬编码为 DEFAULT_POINTSIZE=25。限到 30 才能保证「大 > 小」。
MAX_POINTSIZE = 30
# 每页最多曲目行数：再大字号会小于 7pt（行距算不出来就重叠 → spumux 失败）。
# 由 labelheight=(480-span*12)/span >= 5 反推：span=rows+4 <= 28 → rows <= 24。
MAX_MENU_ROWS = 24
ALBUM_TEXT_Y0 = 48          # 专辑标题基线（commonvars.h）
TEXT_BUDGET_PX = 660        # 每行文字可占宽度（按钮 x0=33..x1=708）

# ---- 一级菜单：专辑索引页（缩略图网格）--------------------------------
# 4x3 是量出来的：格子 180x140，缩略图 100x100；
# disc1（28 张）→ 3 页、disc2（17 张）→ 2 页。
#
# ⚠️ 顶部 `INDEX_TOP`(60) 是**留给大标题（光盘标题）的带子**：
# 标题由 dvda-author 的 prepare_overlay_img() 在**每一页**画在 y≈28..54，
# 网格从 0 开始就会压在缩略图上（实测过：标题墨迹 362x26 在 (47,28)，
# 而第一行缩略图是 y=5..109）。
#
# ⚠️ 网格占 60..480，底部 `INDEX_ARROW_Y0..Y1`(496..552) 是翻页箭头带 ——
# 索引页也要能翻页（专辑多于一页时没箭头就走不掉）。
#
# ⚠️ 格子几何必须与 C 侧一致（menu.h / menu.c / xml.c 里的同名常量）：
#    按钮矩形 = 整个格子内容区
#               (col*CELL_W + INSET, INDEX_TOP + row*CELL_H + INSET)
#               尺寸 CELL_W-2*INSET x CELL_H-2*INSET
#    箭头矩形 = INDEX_NEXT_X0/INDEX_PREV_X0, INDEX_ARROW_Y0..Y1
# 注意：**缩略图占多大、专辑名画在哪，C 侧不关心** —— 按钮是整格，
# 名称也落在可点区里。所以下面这几个尺寸只影响 Python 的排版。
INDEX_COLS, INDEX_ROWS = 4, 3
INDEX_CELL_W = FRAME_W // INDEX_COLS            # 180
INDEX_CELL_H = 140                              # 与 C 侧一致（60 + 3*140 = 480）
INDEX_TOP = 60                                  # 大标题带高度（与 C 侧一致）
INDEX_INSET = 5                                 # 格子内容与格子边缘的间隙
INDEX_PER_PAGE = INDEX_COLS * INDEX_ROWS        # 12
# 格子内部：上面正方缩略图，下面专辑名（换行 + 不够宽/高时自动缩字号）。
#   100 + 2 + 28 = 130 = INDEX_CELL_H - 2*INDEX_INSET
# 封面实测全是 3000x3000，所以缩略图取**正方形且不裁切** ——
# 若铺满 170x130（1.3:1），正方形封面会被裁掉上下约 23% 的高度。
INDEX_THUMB = 100                               # 缩略图边长（正方形）
INDEX_THUMB_GAP = 2                             # 缩略图与名称条的间隙
INDEX_LABEL_H = 28                              # 名称条高度
INDEX_LABEL_MAX_POINTS = 17                     # 名称字号上限（一行放不下就缩）
INDEX_LABEL_MIN_POINTS = 9                      # 名称字号下限（再小看不清）
# 翻页箭头带（与 C 侧 INDEX_ARROW_Y0/Y1 一致）。网格底是 480，
# 再加上下裕量 —— 这段区间里除了箭头不可能有别的墨迹，
# 构建期自检靠它判断「箭头到底画了没有」。
INDEX_ARROW_BAND = (488, 560)
# 专辑名的阴影偏移（与 C 侧 TEXT_SHADOW_DX/DY 无关 —— 名称是 Python 画进
# 背景图的，C 侧只管按钮矩形）。白字 + 黑阴影在模糊彩底上最清楚。
INDEX_LABEL_SHADOW = 2
# 缩略图的描边。画在**背景图**上（不是子画面），所以不影响按钮遮罩。
# 用**不透明深灰**而不是「半透明白」：
#   · 半透明白在亮封面上会消失（实测：亮图内侧 108 vs 框 58，反而更暗）；
#   · 深灰描边不管封面明暗都能把它从背景（~74）里分出来；
#   · `rgba(0,0,0,0.x)` 的 alpha 在灰度图上会被忽略成纯黑，别用。
INDEX_THUMB_BORDER = "#2a2a2a"
INDEX_THUMB_BORDER_W = 2
# 索引页背景（模糊拼贴）的压暗 = 二级页封面压暗值 × 这个比例。
# 背景是**重度模糊**的，可以比二级页亮得多（二级页要压暗才读得清字），
# 取 0.4：默认 35 → 14，页内空隙均值约 74，而对角的缩略图约 116 ——
# 背景看得出是「图」但又明显退到后面去；白字靠黑阴影也够清楚。
# 想更亮/更暗就调这个比例，不必碰两个值。
INDEX_BACKDROP_DIM_RATIO = 0.4
# 索引页在 `--screentext` 里的「段标题」。名称是 Python 画进背景图的，
# 所以这个字不会被显示，只是为了让 screentext 的段格式合法。
INDEX_LABEL = "选择专辑"

_CJK_LO, _CJK_HI = 0x2E80, 0x9FFF        # 常用 CJK 区段
_FULLWIDTH = 0xFF00


def _is_wide(ch):
    o = ord(ch)
    return _CJK_LO <= o <= _CJK_HI or o >= _FULLWIDTH


def text_px(s, pointsize):
    """粗略估算 ImageMagick 画出来的像素宽度。

    Droid Sans Fallback 这类字体：汉字/全角字约等于字号，ASCII 约 0.55 倍。
    只用来决定截断与下划线长度，不要求精确。
    """
    wide = sum(1 for c in s if _is_wide(c))
    narrow = len(s) - wide
    return int(wide * pointsize + narrow * pointsize * 0.55)


def truncate_px(s, pointsize, budget=TEXT_BUDGET_PX):
    """按像素预算截断，超出时以 ~ 结尾（单字节，避免字体缺字）。"""
    if text_px(s, pointsize) <= budget:
        return s
    out = s
    while out and text_px(out + "~", pointsize) > budget:
        out = out[:-1]
    return (out.rstrip() + "~") if out else ""


# `--screentext` 用 `=`、`:`、`,` 三个字符分层，而**没有转义机制**
# （fn_strtok 就是简单按分隔符切）。曲名/专辑名里出现这三个字符就会把
# 一条标签切成两条 —— 之后所有标签整体错位一格，看起来像「一首歌被拆成
# 两个按钮」。实测本项目有 4 首曲名含 ASCII 逗号（如「繁星、新生,与你」）。
#
# 处理：换成**视觉等价**的全角形式（中文语境里更自然）；
# 纯 ASCII 文本没有全角形式，改用间隔号 `·`。
_DELIM_FIX = {",": "，", ":": "：", "=": "＝"}
_DELIM_ASCII = {",": "·", ":": "·", "=": "·"}


def sanitize(s):
    """把 `--screentext` 的保留字符换成安全等价字符。

    返回 (净化后的字符串, 是否改过)。
    """
    if not any(c in s for c in _DELIM_FIX):
        return s, False
    wide = any(_is_wide(c) for c in s)
    table = _DELIM_FIX if wide else _DELIM_ASCII
    out = "".join(table.get(c, c) for c in s)
    return out, True


def short_album(name, max_chars=24):
    """专辑名取「主标题」：截到第一个括号/方括号之前。

    音源里的专辑名形如 `星炬不熄(游戏《鸣潮》原声音乐) - EP`，主标题足够辨识。
    max_chars 只是防拖尾的兜底 —— 真正限宽交给 truncate_px（按像素）。
    """
    s = re.split(r"[(\[（【]", name, 1)[0]
    s = s.strip().rstrip("-–—").strip()
    if not s:
        s = name
    if len(s) > max_chars:
        s = s[:max_chars]
    return s


# ---------------------------------------------------------------- 分页 ---------
def rows_per_page(total_tracks, pages):
    """dvda-author **自己**会用的每页行数。

    必须逐字复刻它的算法（`docs/DVDA-AUTHOR-CHANGES.md` 改过的那两处）：
        maxbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, ceil(totntracks / nmenus))
    因为分页是 dvda-author 自己做的（逐页填满 R 行，不按专辑边界断页），
    我们只是把每页的背景与文字对上它。两边算法不一致就会错位。
    """
    if pages < 1:
        return MAX_BUTTONS
    return min(MAX_BUTTONS, max(1, -(-total_tracks // pages)))


def pages_for(groups_pages, pages):
    """给定页数，返回 (R, P)：R = 每页行数，P = 实际会画出的页数。"""
    total = sum(groups_pages)
    r = rows_per_page(total, pages)
    return r, sum(-(-n // r) for n in groups_pages if n > 0)


def group_pages(group_sizes, max_rows=None):
    """求页数与每页行数。

    约束（三个都要满足）：
      1. `sum_g ceil(n_g / R) <= 页数` —— 否则排在后面的组进不了菜单
         （dvda-author 不报错，只是点不到）
      2. `R <= max_rows` —— 可读性上限，越小字越大、页越多
      3. **`sum_g ceil(n_g / R) == 页数`** —— 页数必须与实际画出的页数相等。
         否则我按页数生成 N 张背景，但 dvda-author 只用其中 P 张，
         第 P..N-1 张是废的（且 AMG IFO 会声明多余菜单）。

    其中 `R = Min(32, ceil(总轨数 / 页数))`。页数越大 R 越小、左边越大，
    所以从小到大试即可。

    返回 (页数, R)。找不到完全相等时退到「能满足 1、2 的最小页数」。
    """
    total = sum(group_sizes)
    if total == 0:
        return 1, 1
    cap = max_rows or MAX_BUTTONS
    fallback = None
    for pages in range(1, total + 1):
        r, p = pages_for(group_sizes, pages)
        if r > cap or p > pages:
            continue
        if p == pages:
            return pages, r
        if fallback is None:
            fallback = (pages, r)
    if fallback:
        return fallback
    return total, 1


def compute_fontsize(rows):
    """按每页行数算字号：必须小于行距，否则文字与下划线重叠。

    menu.c 的 y()：labelheight = (576-96-(R+4)*12)/(R+4)，行距 = labelheight+12。
    下划线画在基线下 4~6 px，故在行距上留 10 px 余量。

    一页一个专辑后各页行数不等，这里取**最大页**的值作全局字号
    （行数少的页行距更大，不会重叠）。
    """
    span = rows + 4
    labelheight = (FRAME_H - 56 - 40 - span * 12) // span
    spacing = labelheight + 12
    return max(MIN_POINTSIZE, min(spacing - 10, MAX_POINTSIZE))


def album_blocks(album_of):
    """把曲目序列按「连续同专辑」切块，返回 [(专辑名, 起始, 结束), ...]。"""
    blocks = []
    for i, a in enumerate(album_of):
        if blocks and blocks[-1][0] == a:
            blocks[-1][2] = i + 1
        else:
            blocks.append([a, i, i + 1])
    return [(a, s, e) for a, s, e in blocks]


def album_pages(album_of, row_cap=MAX_MENU_ROWS):
    """**一页一个专辑**：返回 [(专辑名, 起始, 结束, 是否续页), ...]。

    菜单的 `--screentext` 每段对应一页（见 build_menu 里的说明），所以
    「文字段 = 专辑」就等于「页 = 专辑」，新专辑自动换页。

    专辑曲目数超过 row_cap 时切成多页（续页标题加「（续）」）——
    屏幕放不下也只能拆。
    """
    pages = []
    for a, s, e in album_blocks(album_of):
        i, first = s, True
        while i < e:
            stop = min(i + row_cap, e)
            pages.append((a, i, stop, not first))
            first = False
            i = stop
    return pages


def compute_fontwidth(texts, pointsize):
    """按文字的中英混合比例算下划线宽度系数。

    menu.c 画下划线：deltax1 = fontwidth * pointsize * strlen(text) / 10
    （strlen 是**字节数**，所以汉字算 3 字节）。要让下划线贴合文字，
    需 fontwidth ≈ 10 * Σ(汉字 + 0.5*ASCII) / Σ(字节数)。
    """
    wide_px = narrow_px = nbytes = 0
    for s in texts:
        for c in s:
            if _is_wide(c):
                wide_px += 1
                nbytes += 3
            else:
                narrow_px += 1
                nbytes += 1
    if not nbytes:
        return 5
    # fontwidth * pointsize * nbytes / 10 = (wide + 0.5*narrow) * pointsize
    return max(1, min(10, round(10.0 * (wide_px + 0.5 * narrow_px) / nbytes)))


# ------------------------------------------------------------ 图片生成 ---------
def _run(cmd):
    r = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if r.returncode != 0:
        raise RuntimeError("命令失败: %s\n%s"
                           % (" ".join(cmd),
                              r.stdout.decode("utf-8", "replace")))


def have_magick():
    return bool(shutil.which("convert") or shutil.which("magick"))


def _magick(*args):
    """用 ImageMagick 处理图片（IM7 优先 magick，IM6 用 convert）。"""
    exe = shutil.which("magick") or shutil.which("convert")
    if not exe:
        raise RuntimeError("找不到 ImageMagick（convert/magick）")
    _run([exe] + list(args))


def _magick_exe():
    return shutil.which("magick") or shutil.which("convert")


_IM_FONTS = None


def im_fonts():
    """ImageMagick 能按名字找到的字体集合（`magick -list font`）。

    必须区分「字体存在」与「名字不认识」：给 `-font` 一个不存在的名字时
    ImageMagick **不报错**，而是悄悄回退到默认字体 —— 于是检测出来
    「拉丁可画」，实际用的是别的字体；而汉字仍然画不出来。
    """
    global _IM_FONTS
    if _IM_FONTS is None:
        _IM_FONTS = set()
        exe = _magick_exe()
        if exe:
            r = subprocess.run([exe, "-list", "font"],
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
            for line in r.stdout.decode("utf-8", "replace").splitlines():
                s = line.strip()
                if s.startswith("Font:"):
                    _IM_FONTS.add(s.split(":", 1)[1].strip())
    return _IM_FONTS


def _norm(s):
    return re.sub(r"[^a-z0-9]", "", (s or "").lower())


def font_exists(name):
    """这个 -font 名字（或文件路径）真的可用吗。"""
    if not name or any(c.isspace() for c in name):
        return False
    if os.path.isfile(name):
        return True
    n = _norm(name)
    for f in im_fonts():
        fn = _norm(f)
        if fn == n or (n and (n in fn or fn in n)):
            return True
    return False


def _ink(text, font, size=20):
    """渲一小块文字，返回平均 alpha —— 0 表示这个字体画不出这些字。

    只检查「字体名能否解析」是不够的：Ubuntu 的 fonts-droid-fallback 是
    **精简版**（拉丁字形已被删掉，因为假定 DejaVu 提供），字体信息里仍写着
    覆盖 Basic Latin，但 ImageMagick 画 ASCII 时一个像素都不出
    —— 而且不报错，菜单上就是一片空白。
    """
    exe = _magick_exe()
    if not exe:
        return 0.0
    r = subprocess.run(
        [exe, "-size", "160x48", "xc:none", "-font", font, "-pointsize",
         str(size), "-fill", "white", "-annotate", "+2+32", text,
         "-format", "%[fx:mean.a]", "info:"],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    try:
        return float(r.stdout.decode().strip() or 0)
    except ValueError:
        return 0.0


def font_coverage(name):
    """返回该字体**实际能画出**的字符集集合。

    检查方式是真的渲一小块文字看有没有墨迹（不是查字体表）：
    Ubuntu 的 fonts-droid-fallback 在字体信息里声称覆盖 Basic Latin，
    实际画 ASCII 一个像素都不出；而它反过来又没有韩文。
    """
    if not font_exists(name):
        return set()
    return {s for s, probe in SCRIPT_PROBES if _ink(probe, name) > 0}


def script_of(ch):
    """把一个字符归到需要的字符集名；不关心的（符号/空白）返回 None。"""
    if ch.isspace():
        return None
    o = ord(ch)
    if o < 0x80:
        return "ASCII"
    if 0x3040 <= o <= 0x309F:
        return "平假名"
    if 0x30A0 <= o <= 0x30FF:
        return "片假名"
    if 0xAC00 <= o <= 0xD7AF or 0x1100 <= o <= 0x11FF:
        return "韩文"
    if 0x3400 <= o <= 0x4DBF or 0x4E00 <= o <= 0x9FFF:
        return "汉字"
    if 0x3000 <= o <= 0x303F or 0xFF00 <= o <= 0xFFEF:
        return "CJK标点"
    return None


def needed_scripts(texts):
    """这批文字实际用到了哪些字符集。

    本项目音源同时含中文、日文（假名）、韩文曲名，缺任何一个都会在菜单上
    变成空白，所以必须按实际用到的集合去挑字体。
    """
    got = set()
    for t in texts or ():
        for ch in t:
            s = script_of(ch)
            if s:
                got.add(s)
    return got


# 字符集 → 探测用样本。同一个样本里的字必须属于同一字符集，
# 否则「部分缺失」会掩盖成「有墨迹」。
SCRIPT_PROBES = (
    ("ASCII", "Ag("),
    ("汉字", "\u4e2d\u6587"),
    ("平假名", "\u3042\u3044\u3046"),
    ("片假名", "\u30a2\u30a4\u30a6"),
    ("韩文", "\ud55c\uae00"),
    ("CJK标点", "\u3001\u300a\u300b"),
)


# CJK 字体候选（ImageMagick 的字体名，空格换成连字符）。
# 前置条件：必须覆盖菜单实际用到的**全部**字符集 —— 本项目曲名同时含
# 中文、日文假名、韩文，精简版字体往往只覆盖一半：
#   · fonts-droid-fallback：有汉字/假名/CJK标点，**没有 ASCII、没有韩文**
#   · fonts-wqy-microhei：有 ASCII + 汉字/假名，**没有韩文**
#   · fonts-noto-cjk：中/日/韩 + ASCII 全覆盖（推荐）
FONT_CANDIDATES = (
    "Noto-Sans-CJK-SC",          # fonts-noto-cjk，覆盖最全
    "Noto-Sans-CJK-JP",
    "Noto-Sans-CJK-KR",
    "Noto-Sans-CJK-TC",
    "NotoSansCJK-Regular",
    "Source-Han-Sans-CN",
    "WenQuanYi-Micro-Hei",       # fonts-wqy-microhei：1.7 MB，但没有韩文
    "WenQuanYi-Zen-Hei",
    "AR-PL-UMing-CN",            # fonts-arphic-uming
    "Droid-Sans-Fallback",       # 精简版：有汉字/假名，无 ASCII、无韩文
    "DejaVu-Sans",               # 只有 ASCII
)

# 想自动扫描 IM 字体表时，只考虑名字像 CJK 字体的那些（全表扫描要几十秒）
_CJK_HINT = re.compile(r"cjk|hei|ming|song|kai|wqy|wenquan|arphic|ar-?pl|droid|"
                       r"han|zen|uming|ukai|noto", re.I)


def pick_font(preferred, texts, log=print):
    """挑一个能画出 `texts` 里**全部**字符集的字体名。

    texts 是菜单实际要显示的文字（曲名/专辑名/光盘标题）。
    返回 (字体名, 仍缺的字符集集合)。仍缺时调用方应告警 ——
    缺字在菜单上就是空白，不会报错。
    """
    need = needed_scripts(texts)
    if not need:
        return (preferred or ""), set()

    def missing_of(name):
        return need - font_coverage(name)

    if preferred:
        miss = missing_of(preferred)
        if not miss:
            return preferred, set()
        if font_exists(preferred):
            log("[菜单][警告] 字体 %r 画不出: %s，改用候选字体"
                % (preferred, "、".join(sorted(miss))))
        else:
            log("[菜单][警告] 字体 %r 不可用（不存在，或名字里有空格 —— "
                "那会让 mogrify 命令断开），改用候选字体" % preferred)

    best, best_miss, best_score = "", need, -1
    tried = set()
    for f in list(FONT_CANDIDATES) + sorted(im_fonts()):
        if f in tried or not _CJK_HINT.search(f):
            continue
        tried.add(f)
        if not font_exists(f):
            continue
        miss = missing_of(f)
        if not miss:
            return f, set()
        score = len(need) - len(miss)
        if score > best_score:
            best, best_miss, best_score = f, miss, score

    if best:
        log("[菜单][警告] 找不到覆盖 %s 的字体，暂用 %s；"
            "这些字在菜单上会是空白: %s"
            % ("、".join(sorted(need)), best, "、".join(sorted(best_miss))))
        log("           处理: sudo apt install fonts-noto-cjk"
            "（覆盖中/日/韩 + ASCII）")
    else:
        log("[菜单][警告] 找不到可用字体，菜单文字将不会显示")
        log("           处理: sudo apt install fonts-noto-cjk")
    return best, best_miss


def make_blankscreen(path):
    """全透明 720x576 PNG —— 菜单文字画在它上面，背景视频从下面透出来。"""
    _magick("-size", "%dx%d" % (FRAME_W, FRAME_H), "xc:none",
            "-depth", "8", "PNG32:" + path)


def make_still(cover, path):
    """播放时显示的封面：720x576，封面按 1:1 居中留黑边。

    尺寸必须与 menu.c 里静图的编码制式一致（jpeg2yuv/mpeg2enc 按该尺寸编码）。
    """
    _magick(cover, "-resize", "%dx%d" % (FRAME_H, FRAME_H),
            "-background", "black", "-gravity", "center",
            "-extent", "%dx%d" % (FRAME_W, FRAME_H), "-quality", "90", path)
    w, h = image_size(path)
    if (w, h) != (FRAME_W, FRAME_H):
        raise RuntimeError("%s 尺寸为 %dx%d，应为 %dx%d"
                           % (path, w, h, FRAME_W, FRAME_H))
    return path


def _cell(cover, w, h):
    """一张封面填满 w×h（居中裁剪）。"""
    if cover:
        return ["(", cover, "-resize", "%dx%d^" % (w, h),
                "-gravity", "center", "-extent", "%dx%d" % (w, h), ")"]
    return ["(", "-size", "%dx%d" % (w, h), "xc:black", ")"]


def make_background(covers, path, dim):
    """每页背景：该页各专辑封面拼成网格，再整体压暗。

    dim 是亮度下降百分比（0~100，越大越暗）—— 压暗是为了让白字读得清。
    网格用**横向** `+append` 拼每行、再纵向 `-append` 拼各行；
    误用纵向拼接会得到非 720x576 的图，jpeg2yuv/mpeg2enc 会失败。
    """
    dim = max(0, min(100, int(dim)))
    n = max(1, len(covers))
    cols = 1 if n == 1 else 2
    rows = max(1, -(-n // cols))
    cw, ch = FRAME_W // cols, FRAME_H // rows

    rowfiles = []
    for r in range(rows):
        cells = [_cell(covers[r * cols + c], cw, ch)
                 for c in range(cols) if r * cols + c < n]
        while len(cells) < cols:          # 末行不足时补黑格，保证等宽
            cells.append(_cell(None, cw, ch))
        rf = path + (".row%d.png" % r)
        _magick(*([a for cell in cells for a in cell] + ["+append", rf]))
        rowfiles.append(rf)

    if len(rowfiles) == 1:
        _magick(rowfiles[0], path)
    else:
        _magick(*(rowfiles + ["-append", path]))
    for rf in rowfiles:
        try:
            os.remove(rf)
        except OSError:
            pass

    # 尺寸必须与菜单一致：非 720x576 会让每页背景编码失败，
    # 而 dvda-author 只在后面报一句「找不到 background_movie_N.mpg」。
    w, h = image_size(path)
    if (w, h) != (FRAME_W, FRAME_H):
        raise RuntimeError("%s 尺寸为 %dx%d，应为 %dx%d"
                           % (path, w, h, FRAME_W, FRAME_H))

    if dim:
        _magick(path, "-brightness-contrast", "-%dx0" % dim,
                "-quality", "88", path)
    return path


def _caption_height(text, width, font, points):
    """`caption:` 在这个宽度/字号下**自动换行**后的高度（px）。出错返回 None。

    交给 ImageMagick 自己排版来量，比在 Python 里估算字符宽度靠得住 ——
    中英混排（如 `Lulala! Lululala!`）按字符数猜行数必错。
    `-size Wx`（高度留空）就是「按宽度换行，高度自适应」。
    """
    exe = _magick_exe()
    if not exe:
        return None
    r = subprocess.run(
        [exe, "-background", "none", "-fill", "white", "-font", font,
         "-pointsize", str(points), "-size", "%dx" % width,
         "caption:" + text, "-format", "%h", "info:"],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    if r.returncode != 0:
        return None
    try:
        return int(r.stdout.decode().strip())
    except ValueError:
        return None


_INDEX_FIT = {}


def fit_index_label(text, width, height, font):
    """给专辑名挑 `(字号, 实际显示的文本)`。

    **先换行、行数超了再缩字号**：从上限逐级往下试，取第一个
    「换行后自然高度 <= height」的。只缩字号是不够的 ——
    `Running For Your Life` 在 18pt 下换行后高 56px（>28），要降到 17pt。

    ① 名称条只有 28px 高，所以**装得下两行**的情况很少（两行至少 30px），
       实测本项目最长的专辑名（21 字符）在 17pt 单行刚好压线，所以
       正常都是单行 + 满字号；换行只在名称特别长时才发生。
    ② 连下限字号都塞不下时**截断加省略号** —— 否则 `caption:` 会直接把
       下半行裁掉，看起来像「少了一半的字」而且不会报错。

    同一个名字在多页上重复出现（如「星炬不熄」），结果按
    (文本, 宽, 高, 字体) 缓存，省掉重复调 IM 的开销。
    """
    key = (text, width, height, font)
    if key in _INDEX_FIT:
        return _INDEX_FIT[key]

    pick, shown = INDEX_LABEL_MIN_POINTS, text
    for p in range(INDEX_LABEL_MAX_POINTS, INDEX_LABEL_MIN_POINTS - 1, -1):
        h = _caption_height(text, width, font, p)
        if h is None or h <= height:     # None = 量不出来，按最小字号画
            pick = p
            break
    else:
        while shown and _caption_height(shown + "…", width, font,
                                        INDEX_LABEL_MIN_POINTS) > height:
            shown = shown[:-1]
        shown = (shown + "…") if shown else ""

    _INDEX_FIT[key] = (pick, shown)
    return pick, shown


def _covers_backdrop(cells, path, dim):
    """把本页封面拼贴 → 重度模糊 → 压暗，当作背景（`DVDA_MENU_INDEX_BG=covers`）。

    好处：每页底色都不一样，且与内容相关。坏处：一页里封面风格差异大时
    底色会花。默认**不用**这个，改用 `make_index_backdrop()` 画的设计背景。
    """
    cs = [c for _n, c in cells if c]
    if not cs:
        _magick("-size", "%dx%d" % (FRAME_W, FRAME_H), "xc:black",
                "-quality", "90", path)
        return path

    cw, ch = FRAME_W // INDEX_COLS, FRAME_H // INDEX_ROWS     # 180 x 192
    args = ["-size", "%dx%d" % (FRAME_W, FRAME_H), "xc:black"]
    for i in range(INDEX_PER_PAGE):
        args += ["(", cs[i % len(cs)], "-resize", "%dx%d^" % (cw, ch),
                 "-gravity", "center", "-extent", "%dx%d" % (cw, ch),
                 "-repage", "+%d+%d" % ((i % INDEX_COLS) * cw,
                                        (i // INDEX_COLS) * ch), ")"]
    args += ["-flatten", "-blur", "0x28",
             "-brightness-contrast", "-%dx0" % max(0, min(100, int(dim))),
             "-quality", "90", path]
    _magick(*args)
    return path


# ---- 设计背景的三个可调参数（不用外部素材，构建时现画）----
# 对角线渐变的两端：左上亮、右下暗，给画面一个方向感。
# 实测缩略图亮度在 35~143（中位 75），所以背景整体压在均值 ~54、
# 局部最高 ~78 —— 是「一面墙」而不是「和照片抢亮度」。
BACKDROP_FROM = "#8c8c8c"
BACKDROP_TO = "#1a1a1a"
# 与 4x3 格子对齐的细网格（白、低透明度）—— 让背景和版式有关系，
# 看起来是「设计过的」而不是随手一张图。实测线比底色亮约 15 级，够含蓄。
BACKDROP_GRID_ALPHA = 0.14
# 径向暗角：中心不动、四周乘到这个灰度。压住四角，视线收拢到中间，
# 同时保证边缘的字有对比度。
# 缩略图外加一圈细边框（``INDEX_THUMB_BORDER``），把它们从背景里「托」出来
# —— 封面本身明暗差很大（35~143），只靠背景深浅托不住所有图。
BACKDROP_VIGNETTE = "#7a7a7a"


def make_index_backdrop(path):
    """生成索引页的**设计背景**（720x576，自带素材，不依赖任何图片文件）。

    三层叠出来：

      1. **对角线渐变** `BACKDROP_FROM` → `BACKDROP_TO`
         用 `-sparse-color bilinear` 而不是「渐变图 + `-rotate`」——
         后者会在旋转后新露出的画布角上填 `-background`（默认**白**），
         实测中心裁切后仍有 14% 的像素是纯白，把整页顶亮。
      2. **格子网格** 线画在 `INDEX_CELL_W/H` 的分界上、以及网格区
         上下边界 —— 与按钮版式对齐。
      3. **径向暗角**（`-compose multiply`）压住四角。

    为什么程序生成而不是「上网找一张」：
      · 授权干净：网上找的图不能随工程一起分发；
      · 仓库不放二进制：构建时现画，改上面三个常量就换样式；
      · 明度可控：正好落在「看得出是图、又不抢缩略图」的区间
        （实测均值 73，缩略图区均值 ~116，白字+黑阴影在其上很清楚）。

    想用自己的图：`DVDA_MENU_INDEX_BG=/path/to/img.jpg`（见 README）。
    """
    lines = []
    for c in range(1, INDEX_COLS):
        x = c * INDEX_CELL_W
        lines.append("rectangle %d,0 %d,%d" % (x, x, FRAME_H - 1))
    for r in range(INDEX_ROWS + 1):
        y = INDEX_TOP + r * INDEX_CELL_H
        lines.append("rectangle 0,%d %d,%d" % (y, FRAME_W - 1, y))

    _magick("-size", "%dx%d" % (FRAME_W, FRAME_H), "xc:" + BACKDROP_FROM,
            "-sparse-color", "bilinear",
            "0,0 %s %d,%d %s" % (BACKDROP_FROM, FRAME_W - 1, FRAME_H - 1,
                                 BACKDROP_TO),
            "-fill", "rgba(255,255,255,%s)" % BACKDROP_GRID_ALPHA,
            "-draw", " ".join(lines),
            "(", "-size", "%dx%d" % (FRAME_W, FRAME_H),
            "radial-gradient:#ffffff-%s" % BACKDROP_VIGNETTE, ")",
            "-compose", "multiply", "-composite",
            "-quality", "92", path)
    return path


def fit_backdrop(src, path, dim):
    """把用户给的图 `src` 铺满画面（填满再裁）并压暗 `dim`%，写到 `path`。"""
    _magick("(", src, "-resize", "%dx%d^" % (FRAME_W, FRAME_H),
            "-gravity", "center", "-extent", "%dx%d" % (FRAME_W, FRAME_H), ")",
            "-brightness-contrast", "-%dx0" % max(0, min(100, int(dim))),
            "-quality", "92", path)
    return path


def make_index_page(cells, path, font, dim=35, bg="auto"):
    """一级菜单（专辑索引页）：`4x3` = 正方缩略图 + 专辑名，整幅 720x576。

    cells 是本页最多 `INDEX_PER_PAGE`(12) 个 `(专辑名, 封面路径)`：
    封面为 None 的格子留黑，专辑名为空则不画名称。

    格子内部（从 `col*180+5, INDEX_TOP + row*140+5` 起，共 170x130）：
      · 缩略图：正方形 `INDEX_THUMB`(100)，水平居中
      · 名称  ：缩略图下方 `INDEX_THUMB_GAP`(2) px，宽 170、高 28，
                居中，自动换行 + 缩字号，**白字 + 黑阴影**
    背景由 `bg` 决定（`DVDA_MENU_INDEX_BG`）：
      `auto`（默认） → `make_index_backdrop()` 画的设计背景
      `covers`       → 本页封面的模糊拼贴（`_covers_backdrop()`）
      图片路径        → 用这张图（`fit_backdrop()`，按 `dim`% 压暗）
    顶部 60 px 留给 dvda-author 画的**大标题**（它自带阴影）；底部 480
    以下留给翻页箭头。

    按钮区 = **整个格子内容区**（含名称），由 C 侧 xml.c 输出 ——
    所以这里改缩略图/名称的尺寸**不会**影响点击区。

    ⚠️ `-repage` 必须写在**括号内**：它是**算子**（operator）而不是设置项，
    不加括号时 IM 会对「当前图像列表里的每一张」生效 —— 包括开头那张
    720x576 的背景。结果背景也被挪到最后一格的位置，画布露出
    `-flatten` 的默认白底，于是整页只剩右下角一张封面、其余全白。
    """
    tw = INDEX_THUMB
    lw = INDEX_CELL_W - 2 * INDEX_INSET          # 名称条宽 = 170
    tx = INDEX_INSET + (lw - tw) // 2            # 缩略图左边距（居中）
    dx = INDEX_LABEL_SHADOW

    bpath = path + ".bg.jpg"
    if bg and bg != "auto" and bg != "covers":
        fit_backdrop(bg, bpath, dim * INDEX_BACKDROP_DIM_RATIO)
    elif bg == "covers":
        _covers_backdrop(cells, bpath, dim * INDEX_BACKDROP_DIM_RATIO)
    else:
        make_index_backdrop(bpath)

    args = [bpath]
    for i, (name, cov) in enumerate(cells[:INDEX_PER_PAGE]):
        ox = (i % INDEX_COLS) * INDEX_CELL_W
        oy = INDEX_TOP + (i // INDEX_COLS) * INDEX_CELL_H
        if cov:
            # `^` + `-extent` 是「填满再裁」：封面本来就是 1:1，等于不裁；
            # 万一是非方形也不会撑破格子的正方形版式。
            args += ["(", cov, "-resize", "%dx%d^" % (tw, tw),
                     "-gravity", "center", "-extent", "%dx%d" % (tw, tw),
                     "-repage", "+%d+%d" % (ox + tx, oy + INDEX_INSET), ")"]
        if name:
            p, shown = fit_index_label(name, lw, INDEX_LABEL_H, font)
            ly = oy + INDEX_INSET + tw + INDEX_THUMB_GAP
            # 先黑阴影、后白字（flatten 里后画的在上面）；两层都用
            # `caption:`，所以换行位置完全一致。
            for sdx, sdy, fill in ((dx, dx, "black"), (0, 0, "white")):
                args += ["(", "-background", "none", "-fill", fill,
                         "-font", font, "-pointsize", str(p),
                         "-size", "%dx%d" % (lw, INDEX_LABEL_H),
                         "-gravity", "center", "caption:" + shown,
                         "-repage", "+%d+%d" % (ox + INDEX_INSET + sdx,
                                                ly + sdy), ")"]
    args += ["-flatten", "-quality", "90", path]
    _magick(*args)

    # 缩略图细边框：**必须单独一遍画在已合成的成品上**。
    # 拼在一起画不行：`-draw` 会作用到**图像列表里的每一张**（背景 + 每个
    # 缩略图 + 每个名称层），而 `-flatten` 按列表顺序叠 —— 画在背景层上的
    # 边框会被后面的缩略图整个盖掉；画在缩略图层上的又会因为那张图只有
    # 100x100、坐标系不同而落到别处（实测边框像素取到 0 = 黑）。
    borders = []
    for i, (_n, cov) in enumerate(cells[:INDEX_PER_PAGE]):
        if not cov:
            continue
        bx = (i % INDEX_COLS) * INDEX_CELL_W + tx
        by = INDEX_TOP + (i // INDEX_COLS) * INDEX_CELL_H + INDEX_INSET
        borders.append("rectangle %d,%d %d,%d"
                       % (bx, by, bx + tw - 1, by + tw - 1))
    if borders:
        _magick(path, "-fill", "none", "-stroke", INDEX_THUMB_BORDER,
                "-strokewidth", str(INDEX_THUMB_BORDER_W),
                "-draw", " ".join(borders), "-quality", "90", path)

    try:
        os.remove(bpath)
    except OSError:
        pass

    size = image_size(path)
    if size != (FRAME_W, FRAME_H):
        raise RuntimeError("%s 尺寸为 %dx%d，应为 %dx%d"
                           % (path, size[0], size[1], FRAME_W, FRAME_H))
    return path


def image_size(path):
    """返回 (宽, 高)。"""
    exe = shutil.which("identify")
    if not exe:
        return FRAME_W, FRAME_H
    r = subprocess.run([exe, "-format", "%w %h", path],
                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    parts = r.stdout.decode("utf-8", "replace").split()
    if r.returncode != 0 or len(parts) != 2:
        raise RuntimeError("无法读取图片尺寸: %s" % path)
    return int(parts[0]), int(parts[1])


# ---------------------------------------------------------------- 主流程 -------
class MenuPlan:
    """一张盘的菜单方案。"""

    def __init__(self):
        self.pages = 0
        self.drawn_pages = 0
        self.rows = 0
        # ⚠️ rows_of_page / album_of_page / pages_list 都只描述**专辑页**
        #    （不含前面的索引页）。校验脚本用 sum(rows_of_page) == 总轨数，
        #    索引页没有曲目，所以不能进去。
        self.rows_of_page = []      # 每页曲目数（一页一个专辑）
        self.album_of_page = []     # 每页的专辑名
        self.pages_list = []        # [(专辑, 起始, 结束, 是否续页), ...]
        # ---- 一级菜单（专辑索引页）----
        # index_pages   : 开头的索引页数（0 = 关闭二级菜单）
        # index_albums  : 每页索引页放哪些专辑（顺序 = 格子顺序）
        self.index_pages = 0
        self.index_albums = []
        self.points = 0
        self.font = ""
        self.font_missing = set()
        self.sanitized = []      # 因含分隔符而被换字的曲名
        self.fontwidth = 5
        self.screentext = ""
        self.backgrounds = []       # bg<N>.jpg，长度 = pages
        self.stills = []            # 每轨一项：图片路径或 ""（沿用上一张）
        self.album_of_track = []

    def args(self, blankscreen, fontname, datadir, bindir):
        """生成追加到 dvda-author 命令行的参数列表。"""
        a = ["--topmenu", "--nmenus=%d" % self.pages,
             "--blankscreen", blankscreen,
             "--background", ",".join(self.backgrounds),
             "--screentext", self.screentext,
             "--bindir", bindir, "--datadir", datadir]
        if self.index_pages:
            # 一级菜单：前 N 页是专辑索引页（缩略图网格）。
            # C 侧据此把这几页当索引页：不画文字、按钮是 `jump menu`。
            a += ["--index-pages", str(self.index_pages)]
        if fontname or self.font:
            a += ["--fontname", fontname or self.font]
        if self.points:
            a += ["--fontsize", str(self.points)]
        a += ["--fontwidth", str(self.fontwidth)]
        if any(self.stills):
            a += ["--stillpics", ":".join(self.stills)]
        return a


def _album_of(track):
    """曲目所属专辑：源文件路径的上一级目录名。"""
    src = track.get("src") or ""
    d = os.path.dirname(src)
    return os.path.basename(d) if d else (track.get("album") or "")


def find_cover(album_dir):
    for ext in ("jpg", "jpeg", "png", "webp"):
        p = os.path.join(album_dir, "cover." + ext)
        if os.path.exists(p):
            return p
    return None


def build_menu(groups, outdir, cfg, log=print, album_dir_of=None):
    """生成菜单素材并返回 MenuPlan。

    groups : [[track, ...], ...] —— **菜单顺序**（每个子列表是一个音频组，
             组内保持全局顺序）。与 02_build.py 传给 dvda-author 的 `-g`
             顺序必须完全一致。
    outdir : 素材输出目录（会被清空重建）
    cfg    : dvda_config.Config
    album_dir_of : 可选，track -> 专辑目录绝对路径（默认由 src 推）
    """
    if os.path.exists(outdir):
        shutil.rmtree(outdir)
    os.makedirs(outdir)


    flat = [t for g in groups for t in g]
    album_of = [(_album_of(t) if album_dir_of is None
                 else os.path.basename(album_dir_of(t))) for t in flat]

    # ---- 分页：**一页一个专辑** ----
    # 为什么能这样：menu.c 在 ncolumns=1 时的画页循环是「一次只画一个
    # 文字段，段内曲目画完才换页」。所以让 --screentext 每段 = 一个专辑，
    # 页数就等于专辑数，新专辑自动换页。
    # 各页行数写在 --screentext 里，dvda-author 会按**该页实际曲目数**
    # 取行距与按钮矩形（见 `docs/DVDA-AUTHOR-CHANGES.md`）。
    row_cap = min(cfg.menu_tracks_per_page, MAX_MENU_ROWS)
    pages_list = album_pages(album_of, row_cap)
    n_albums = len(pages_list)
    rows = max((e - s for _a, s, e, _c in pages_list), default=1)
    points = compute_fontsize(rows)

    # ---- 一级菜单：专辑索引页 ----
    # 页序 = [索引页 0..I-1] + [专辑页 0..A-1]。
    # 索引页 p 的第 k 个格子 → 专辑号 a = p*INDEX_PER_PAGE+k
    #   → 其内容页 (0-based) = I+a → 按钮 `jump menu (I+a+1)`（1-based）。
    # 这套映射是**纯位置**的，C 侧只拿到 --index-pages=I 就能算出全部目标，
    # 不必再传一张表。
    idx_pages = -(-n_albums // INDEX_PER_PAGE) if n_albums else 0
    cnt = cfg.menu_index_min_albums      # 少于这么多专辑就不做一级菜单
    if idx_pages and n_albums < cnt:
        log("[菜单] 只有 %d 张专辑（少于 %d），跳过一级菜单" % (n_albums, cnt))
        idx_pages = 0
    index_albums = []
    for p in range(idx_pages):
        s = p * INDEX_PER_PAGE
        index_albums.append([a for a, _s, _e, _c in
                             pages_list[s:s + INDEX_PER_PAGE]])

    if album_dir_of is None:
        def album_dir_of(t):
            return os.path.dirname(t.get("src") or "")

    plan = MenuPlan()
    plan.pages = idx_pages + n_albums        # 总页数 = 索引页 + 专辑页
    plan.drawn_pages = idx_pages + n_albums
    plan.index_pages = idx_pages
    plan.index_albums = index_albums
    plan.rows = rows
    plan.rows_of_page = [e - s for _a, s, e, _c in pages_list]
    plan.album_of_page = [a for a, _s, _e, _c in pages_list]
    plan.points = points
    plan.album_of_track = album_of
    plan.pages_list = pages_list
    # ---- 每页背景 + 静图 ----
    covers = {}          # 专辑名 -> 封面路径（可能为 None）
    for a in album_of:
        if a in covers:
            continue
        src = next((t for t in flat if _album_of(t) == a), None)
        d = album_dir_of(src) if src else ""
        covers[a] = find_cover(d) if d and os.path.isdir(d) else None

    missing = sorted({a for a, c in covers.items() if not c})
    if missing:
        log("[菜单] %d 张专辑没有 cover.jpg，这些页面背景将留黑：%s"
            % (len(missing), "、".join(missing[:3])
               + ("…" if len(missing) > 3 else "")))

    # 页序：先索引页（缩略图网格），再专辑页（该专辑封面）。
    # 顺序必须与 screentext 的段序、以及 C 侧算出的 jump 目标一致。
    #
    # ⚠️ 索引页要画**专辑名**，而字体是下面 pick_font() 才定下来的
    # （字体缺字 = 名称一片空白且不报错），所以这里只**登记路径**，
    # 真正的绘制挪到 pick_font() 之后。
    plan.backgrounds = []
    index_paths = []
    for pi in range(idx_pages):
        path = os.path.join(outdir, "idx%d.jpg" % pi)
        index_paths.append(path)
        plan.backgrounds.append(path)

    # 专辑页背景 = 该专辑封面（压暗是为了让白字读得清）。缺封面时留黑。
    for pi, a in enumerate(plan.album_of_page):
        cov = covers.get(a)
        path = os.path.join(outdir, "bg%d.jpg" % pi)
        make_background([cov] if cov else [], path,
                        cfg.menu_cover_dim if cov else 0)
        plan.backgrounds.append(path)

    # ---- 静图：**每轨一张**（严格对齐 Enigma）----
    # Enigma《15 Years After》99 轨 → ASVS 里 99 张图，按 title 分组：
    #   title1（15 轨）→ 记录「图数=15, 起始图号=1」
    #   title2（12 轨）→ 记录「图数=12, 起始图号=16」
    # 即**每轨都有自己的一张图**，同专辑的轨只是画面相同。
    #
    # 不能用 `--stillpics` 的空项（表示「沿用上一张」）：那样这些轨的
    # `img->npics` 为 0，会被 atsi2.c 里的
    #   `if (img->npics[trackcount - 1] == 0) continue;`
    # 整个跳过、不写记录，ATS 的静图表就会缺项、ASVS 的「图数」也不是轨数。
    # 同专辑复用同一个 jpg 文件，只是多引用几次 —— 与商业盘结构一致。
    if cfg.menu_stillpics:
        plan.stills = []
        still_path = {}
        for a in album_of:
            if a not in still_path:
                src = covers.get(a)
                path = os.path.join(outdir, "still%d.jpg" % len(still_path))
                ok = False
                if src:
                    try:
                        make_still(src, path)
                        ok = True
                    except RuntimeError as e:
                        log("[菜单][警告] 生成静图失败（%s）：%s" % (a, e))
                still_path[a] = path if ok else ""
            plan.stills.append(still_path[a])

    # ---- 文字链：一页一段，段标题 = 专辑名 ----
    # 格式：`光盘标题=专辑1=轨1,轨2:专辑2=轨3,...`
    #   · 第一个 `=` 之前 → `albumtext`，dvda-author 画在**每一页**顶部
    #     （= 大标题，字号固定 DEFAULT_POINTSIZE=25）
    #   · 每个 `:` 段 → 一页；段内第一个 `=` 之前 → `grouptext[页][0]`，
    #     画在该页曲目之上（= 小标题，字号 = 0.8 × --fontsize）
    # 一页只有一个专辑，所以不再需要给每轨加「专辑名 | 」前缀。
    # 小标题用的专辑名。short_album() 会砍掉括号部分，不同专辑可能撞名
    # （本项目盘2 的「星炬不熄」有两张：原版与毕业合唱版）。撞名时把
    # **能区分它们的那个括号**补回去 —— 否则菜单上两页的小标题一模一样，
    # 分不出是哪张专辑。
    _shorts = {}
    for _a in dict.fromkeys(plan.album_of_page):
        _shorts.setdefault(short_album(_a), []).append(_a)
    _dup = {s for s, v in _shorts.items() if len(v) > 1}

    def menu_album(a):
        s = short_album(a)
        if s not in _dup:
            return s
        for br in re.findall(r"[(\[（【]([^)\]）】]*)[)\]）】]", a):
            if not re.match(r"\s*游戏[《<]", br):     # 跳过「游戏《…》」这类通用词
                return "%s [%s]" % (s, br.strip())
        return s

    chunks = []
    all_texts = []

    # 索引页的段：格式与专辑页相同（`标签=名字1,名字2,...`），
    # 但 C 侧在索引页上**不画文字** —— 这里的名字只是为了给出
    # 「本页几个格子」（= page_ntracks），顺便留作调试参考。
    for albs in index_albums:
        names = []
        for a in albs:
            lbl, fixed = sanitize(menu_album(a))
            if fixed:
                plan.sanitized.append(a)
            names.append(truncate_px(lbl, points))
        label, _ = sanitize(INDEX_LABEL)
        chunks.append(label + "=" + ",".join(names))

    for a, s, e, cont in pages_list:
        gtitle, fixed = sanitize(menu_album(a) + ("（续）" if cont else ""))
        if fixed:
            plan.sanitized.append(a)
        texts = []
        for k in range(s, e):
            title = flat[k].get("title") or os.path.basename(
                flat[k].get("src") or "")
            # ⚠️ 必须净化，否则曲名里的 `,`/`:`/`=` 会把这条标签切开
            # （实测「繁星、新生,与你」会被拆成两条，且之后全部错位一格）
            label, fixed = sanitize(title)
            if fixed:
                plan.sanitized.append(title)
            texts.append(truncate_px(label, points))
        chunks.append(gtitle + "=" + ",".join(texts))
        all_texts.append(gtitle)
        all_texts += texts

    # ⚠️ 格式是 `光盘标题=专辑1=轨1,轨2:专辑2=轨3,轨4:...`
    #    —— **段之间必须用 `:` 分隔**。漏了冒号会让整串只被解析成 1 个段：
    #    段1 的「小标题」变成段1 的轨名列表，而段2 的轨文字根本没定义
    #    （menu.c 之后按 ntracks[段] 索引 tracktext[段][轨] → 越界/段错误）。
    disc_title, _ = sanitize(cfg.title)
    plan.screentext = (disc_title + "=" + ":".join(chunks))

    # 字体必须在文字定下来之后选：要按「实际用到哪些字符集」挑。
    # 本项目曲名同时含中文、日文假名、韩文与 ASCII，缺任何一个都会变空白。
    all_texts.append(cfg.title)
    plan.font, plan.font_missing = pick_font(cfg.menu_font, all_texts, log)

    # ---- 一级菜单：字体定了才画（专辑名要按实际排版换行 + 缩字号）----
    for pi, albs in enumerate(index_albums):
        cells = []
        for a in albs:
            lbl, _fixed = sanitize(menu_album(a))
            cells.append((lbl, covers.get(a)))
        make_index_page(cells, index_paths[pi], plan.font,
                        cfg.menu_cover_dim, cfg.menu_index_bg)

    plan.fontwidth = compute_fontwidth(all_texts, points)

    return plan
