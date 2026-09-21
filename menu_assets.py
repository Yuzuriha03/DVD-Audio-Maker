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
每页行数 `R = min(32, ceil(总轨数 / 页数))`（见 patches/patch_menu_paging.py）。

所以页数必须满足 `sum_g ceil(n_g / R) <= 页数`，否则排在后面的组的曲目
**不会出现在菜单里**（不报错）。这里用迭代求满足条件的最小页数。

## 分组顺序 = 菜单顺序

`group_by_rate()` 在每个 (采样率, 位深) 桶内保持全局顺序，且在专辑边界切分，
所以「组 1 全部轨 → 组 2 全部轨 → …」这个顺序里，专辑仍然是连续的。
菜单顺序就必须用这个顺序（而不是全局顺序），否则按钮的
`jump group G track K` 会对不上。
"""

import math
import os
import re
import shutil
import subprocess

# ---- 画面常量（与 dvda-author 一致：PAL 720x576） ----
FRAME_W, FRAME_H = 720, 576
MAX_BUTTONS = 32            # MAX_BUTTON_Y_NUMBER - 2
MIN_POINTSIZE = 7
MAX_POINTSIZE = 35
ALBUM_TEXT_Y0 = 48          # 专辑标题基线（commonvars.h）
TEXT_BUDGET_PX = 660        # 每行文字可占宽度（按钮 x0=33..x1=708）

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

    必须逐字复刻它的算法（patches/patch_menu_paging.py 改过的那两处）：
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
    """
    span = rows + 4
    labelheight = (FRAME_H - 56 - 40 - span * 12) // span
    spacing = labelheight + 12
    return max(MIN_POINTSIZE, min(spacing - 10, MAX_POINTSIZE))


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

    尺寸必须是 720x576 —— jpeg2yuv/mpeg2enc 按 PAL 尺寸编码，其它尺寸会失败。
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
        self.points = 0
        self.font = ""
        self.font_missing = set()
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

    sizes = [len(g) for g in groups]
    pages, rows = group_pages(sizes, cfg.menu_tracks_per_page)
    points = compute_fontsize(rows)

    flat = [t for g in groups for t in g]
    # ---- 专辑块：按「连续同专辑」切（仅用于统计，不参与分页）----
    album_of = [(_album_of(t) if album_dir_of is None
                 else os.path.basename(album_dir_of(t))) for t in flat]

    if album_dir_of is None:
        def album_dir_of(t):
            return os.path.dirname(t.get("src") or "")

    # ---- 分页：**逐字复刻 dvda-author 的分配** ----
    # dvda-author 自己按「逐页填满 R 行、组内连续、一页不跨组」来排，
    # 我们只能跟着它算，否则背景/标题会和按钮错位。
    # 特别注意：**不能**为了「页内专辑完整」而提前断页 —— 那样页数变多、
    # 传给 --nmenus 的页数变大 → dvda-author 的 R 跟着变小 → 两套分页错开。
    bounds, acc = [], 0
    for n in sizes:
        acc += n
        bounds.append(acc)

    page_of_track = [0] * len(flat)
    page_albums = []
    for start, end in ([(0, bounds[0])] + [(bounds[i], bounds[i + 1])
                                           for i in range(len(bounds) - 1)]
                       if bounds else []):
        i = start
        while i < end:
            stop = min(i + rows, end)
            page = len(page_albums)
            for k in range(i, stop):
                page_of_track[k] = page
            albums, seen = [], set()
            for k in range(i, stop):
                if album_of[k] not in seen:
                    seen.add(album_of[k])
                    albums.append(album_of[k])
            page_albums.append(albums)
            i = stop

    # 兜底：若实际页数少于 dvda-author 要画的页数（极端分组下可能），
    # 用空页补足，保证 --background 的项数与 --nmenus 一致。
    while len(page_albums) < pages:
        page_albums.append([])

    plan = MenuPlan()
    plan.pages = pages
    plan.drawn_pages = len(page_albums)
    plan.rows = rows
    plan.points = points
    plan.album_of_track = album_of
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

    plan.backgrounds = []
    for pi, albums in enumerate(page_albums):
        pics = [covers[a] for a in albums if covers.get(a)]
        path = os.path.join(outdir, "bg%d.jpg" % pi)
        if pics:
            make_background(pics, path, cfg.menu_cover_dim)
        else:
            make_background([], path, 0)
        plan.backgrounds.append(path)

    # ---- 静图：每专辑一张，其余轨留空（沿用上一张）----
    if cfg.menu_stillpics:
        plan.stills = []
        still_index = {}
        for i, a in enumerate(album_of):
            if a in still_index:
                plan.stills.append("")
                continue
            src = covers.get(a)
            path = os.path.join(outdir, "still%d.jpg" % len(still_index))
            ok = False
            if src:
                try:
                    make_still(src, path)
                    ok = True
                except RuntimeError as e:
                    log("[菜单][警告] 生成静图失败（%s）：%s" % (a, e))
            if ok:
                still_index[a] = len(still_index)
                plan.stills.append(path)
            else:
                plan.stills.append("")
                still_index[a] = None

    # ---- 文字链 ----
    # 结构：专辑标题 = 组1标题 = 轨1,轨2,轨3 : 组2标题 = 轨4,轨5 ...
    # 组标题为空（页内专辑已在每轨文字里标出，且页面头部空间有限）。
    chunks = []
    all_texts = []
    idx = 0
    for gi, n in enumerate(sizes):
        texts = []
        prev_alb = None
        prev_page = None
        for k in range(idx, idx + n):
            alb = short_album(album_of[k])
            title = flat[k].get("title") or os.path.basename(
                flat[k].get("src") or "")
            # 同一页里有多张专辑时，在**每张专辑的第一首**前面标出专辑名，
            # 后续同专辑的曲目不重复（否则每行都挂个长前缀）。
            # 换页时重置，使跨页延续的专辑在新页上重新标出。
            page = page_of_track[k]
            if page != prev_page:
                prev_alb, prev_page = None, page
            multi = len(page_albums[page]) > 1
            prefix = (alb + " | ") if (multi and album_of[k] != prev_alb) else ""
            prev_alb = album_of[k]
            texts.append(truncate_px(prefix + title, points))
        # 每段是 `组标题=轨1,轨2,...`；组标题留空（页内专辑已在每轨文字里标出）
        chunks.append("=" + ",".join(texts))
        all_texts += texts
        idx += n

    # ⚠️ 格式是 `专辑标题=组1标题=轨1,轨2:组2标题=轨3,轨4:...`
    #    —— **组之间必须用 `:` 分隔**。漏了冒号会让整串只被解析成 1 个组：
    #    组1 的「组标题」变成组1 的轨名列表，而组2 的轨文字根本没定义
    #    （menu.c 之后按 ntracks[组] 索引 tracktext[组][轨] → 越界/段错误）。
    plan.screentext = (cfg.title + "=" + ":".join(chunks))

    # 字体必须在文字定下来之后选：要按「实际用到哪些字符集」挑。
    # 本项目曲名同时含中文、日文假名、韩文与 ASCII，缺任何一个都会变空白。
    all_texts.append(cfg.title)
    plan.font, plan.font_missing = pick_font(cfg.menu_font, all_texts, log)

    plan.fontwidth = compute_fontwidth(all_texts, points)

    return plan
