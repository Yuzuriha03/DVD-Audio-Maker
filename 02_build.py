#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
步骤2:读取 manifest → 逐曲做无损 MLP 编码 → 按专辑日期序分盘 → dvda-author + mkisofs。

运行于 WSL 内部。

逻辑:
1. 读 manifest,每首记录「源文件路径 / MLP 缓存名 / 重采样目标」
2. 每曲直接用 FFmpeg 对**源文件**编码为 MLP(Meridian Lossless Packing, 无损),
   需要归一化的曲目在同一命令内完成 soxr 重采样;结果缓存复用
3. 全部曲目按 (发布日, 曲序) 全局排序
4. 按专辑边界分盘:每张不超过单盘上限(专辑绝不跨盘、绝不拆散)
5. 每张盘内部按 (采样率, 位深) 分组(DVD-Audio 同组内参数须一致)
6. 每组超过 GROUP_TRACK_LIMIT 轨时按专辑边界再拆一组
   (dvda-author 的 ATSI 表缓冲仅 3 扇区,轨数过多会栈溢出)
7. dvda-author 以 MLP 为输入生成 AUDIO_TS + mkisofs 打包 + 复制到输出目录
8. 输出 mlp_index.json(MLP→源文件/时长/重采样 + 分盘计划),供 verify.sh 与
   verify_pts_length.py 使用(MLP 容器不记录时长,无法回读)

注: 使用自编译的 dvda-author(链接系统 FFmpeg 8, 已适配 ch_layout 等 API),
    可对 24-bit 音频做无损 MLP 编码。
"""

import os
import re
import glob
import json
import shutil
import subprocess
import sys
import time
from collections import OrderedDict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dvda_config import load as load_config          # noqa: E402
from dvda_config import (DVD5_BYTES, MAX_TRACKS,     # noqa: E402
                         AOB_OVERHEAD, ISO_SAFETY)

# ---- 读取配置（config.sh / 环境变量，见 dvda_config.py） ----
CFG = load_config()
BUILD_DIR = CFG.build_dir
MANIFEST = CFG.manifest
OUT_ROOT = CFG.out_root
TMP_ROOT = CFG.tmp_root
ISO_DIR = CFG.iso_dir
FINAL_DIR = CFG.final_dir
MLP_DIR = CFG.mlp_dir                 # MLP 缓存目录
MLP_INDEX = CFG.mlp_index             # MLP → 源/时长/重采样 + 分盘计划

# 本次运行的构建日志。main() 里按是否 --dry-run 重定向：
# dry-run 不执行 dvda-author，日志里不会有轨道表；若覆盖 build.log，
# audit_disc.py / verify.sh 就取不到上次真出盘的审计依据。
BUILD_LOG = CFG.build_log

# 带 MLP 编码支持的 dvda-author（链接系统 FFmpeg 8，已迁移 API）
DVDA = CFG.dvda
MKISOFS = CFG.mkisofs
FFMPEG = CFG.ffmpeg

# dvda-author 的 ATSI 表缓冲固定为 3 扇区(6144 字节)，每轨约 52 字节，
# 超过约 70 轨会栈溢出（stack smashing）。config.sh 里可调，上限 70。
GROUP_TRACK_LIMIT = CFG.group_track_limit
DISC_LIMIT_BYTES = CFG.disc_bytes     # 单盘容量（默认单层 DVD5）
NUM_DISCS = CFG.max_discs             # 目标盘数；0 = 不限制

# MLP 来源：ffmpeg = 本工具链编码；external = 用外部编码器（如 SurCode）的产出
#
# 外部模式的意义：SurCode 是 MLP 的参考实现，其输出在合规性上更可信
# （实测它 147/147 都写了 END_OF_STREAM，ffmpeg 是 0/147）。但外部编码器
# 可能改采样率/位深（SurCode 会把全部曲目统一到 48000/24），所以外部模式下
# **以实际探测到的参数为准**来分组与分盘，不迷信 manifest。
USE_EXTERNAL_MLP = CFG.use_external_mlp
MLP_EXTERNAL_DIR = CFG.mlp_external_dir

# MLP 头部对齐（ffmpeg 模式）——**自动且强制，没有开关也不需参数**。
#
# 只要走 ffmpeg 编码，就必然做这三件事：
#   1. `-max_interval 8`：major sync（解码器重同步点）间隔对齐参考实现 SurCode。
#      编码器默认 16；设 8 后实测 access unit 数与 major sync 数与 SurCode
#      **完全相同**（每 8.0 个一个）。代价：体积约 +3.9%。
#   2. 编码后调 mlp_align.align_bytes() 做纯字节修补（不重编码）：
#      peak_bitrate 改向上取整（使 (raw*sr+8)>>4 往返精确）、
#      extended_substream_info 置 1、重算 major sync 校验和、
#      末尾补 END_OF_STREAM 并修正 AU 长度/奇偶与子流校验。
#   3. 对齐后自检（校验和/奇偶/子流/结束标记全重算校验），不过就报错。
#
# 为什么不给开关：ffmpeg 的 mlp 编码器不写 END_OF_STREAM(0xD234D234)，
# 而参考实现会写 —— 关掉只会产出更不规范的流；而修补带自检，
# 不存在“关了更安全”的情形。
# 外部模式（DVDA_MLP_SOURCE=external）不经过这段代码，不受影响。
MLP_MAX_INTERVAL = 8

# 缓存命中统计（每轮重置）
_cache_stats = {"hit": 0, "stale": 0}

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import mlp_align                                       # noqa: E402
import menu_assets                                     # noqa: E402

# 选曲菜单（AMG 菜单 + ASVS 播放封面）。config.sh 的 DVDA_MENU 打开时，
# build_disc() 会先生成菜单素材（分页 / 文字 / 每页背景 / 每轨封面）再调
# dvda-author。菜单是 DVD-Audio 规范自带的 AMG 菜单，产出仍是纯 DVD-Audio。
MENU_ON = CFG.menu
MENU_DIR = os.path.join(BUILD_DIR, "menu")
# 菜单所需的外部程序：dvdauthor/spumux 由 build_dvda_author_mlp.sh 编好后
# 链接进 menu-bin，其余（mjpegtools / ImageMagick）来自系统包。
MENU_BINS = ("dvdauthor", "spumux", "jpeg2yuv", "mpeg2enc", "mplex",
             "mp2enc", "mogrify", "convert")

_SYNC_MAJOR = b"\xf8\x72\x6f"
_EOS = b"\xd2\x34\xd2\x34"


def to_wsl(p):
    p = p.replace("\\", "/")
    if re.match(r"^[a-zA-Z]:", p):
        p = "/mnt/" + p[0].lower() + p[2:]
    return p


def run(cmd, log_output=False):
    """执行命令、回显，并把输出一并写入构建日志。

    **为什么要写日志**：verify 侧的 `audit_disc.py` 与
    `verify_pts_length.py` 需要从日志里解析两样东西 ——
      · dvda-author 的命令行（据此得知每张盘有几个组、轨序如何）
      · dvda-author 打印的轨道表（First_Sect / Last_Sect / PTS_length）
    因此无论通过 `build.sh` 还是直接运行本脚本，日志都必须落盘。
    `build.sh` 只是额外做了一份终端镜像。

    log_output=True 时捕获 stdout/stderr 并同时写到日志与终端
    （用于 dvda-author —— 它的轨道表在 stdout）。
    """
    line = "+ " + " ".join(cmd)
    print(line)
    logf = None
    try:
        os.makedirs(BUILD_DIR, exist_ok=True)
        logf = open(BUILD_LOG, "a", encoding="utf-8")
        logf.write(line + "\n")
        logf.flush()
    except OSError:
        logf = None

    if not log_output:
        r = subprocess.run(cmd)
        if logf:
            logf.close()
        return r

    # 捕获输出：边打印边写日志
    r = subprocess.run(cmd, stdout=subprocess.PIPE,
                       stderr=subprocess.STDOUT)
    text = r.stdout.decode("utf-8", errors="replace")
    sys.stdout.write(text)
    sys.stdout.flush()
    if logf:
        logf.write(text)
        logf.write("\n")
        logf.flush()
    # 返回一个带 returncode 的轻量对象即可（调用方只用到 returncode）
    return r if False else type("R", (), {"returncode": r.returncode})()


def probe_mlp_params(path):
    """读 MLP 的 {sample_rate, channels, bits}。

    注意:MLP 容器不记录时长(duration=N/A),但采样率/声道/位深可读，
    可用于复核编码结果是否与音频组参数一致。
    """
    r = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "a:0",
         "-show_entries", "stream=sample_rate,channels,bits_per_raw_sample",
         "-of", "default=nw=1:nk=1", path],
        capture_output=True, text=True, encoding="utf-8", errors="replace")
    nums = []
    for line in r.stdout.splitlines():
        try:
            nums.append(int(line.strip()))
        except ValueError:
            nums.append(0)
    while len(nums) < 3:
        nums.append(0)
    return {"sr": nums[0], "ch": nums[1], "bits": nums[2]}


def ffprobe_mlp(path):
    """读 MLP 的 (sample_rate, bits_per_raw_sample) —— 仅用于内部复核。"""
    p = probe_mlp_params(path)
    return p["sr"], p["bits"]


def mlp_path_for(track):
    """MLP 缓存文件路径。

    缓存键由 01_prepare.py 给出(track["name"],形如 group_48000_24/0001__标题),
    这里把 / 换成 __ 得到平坦的文件名。缓存可以跨次复用 —— 源文件未变时
    不重新编码，换源后按 mtime 自动失效。
    """
    return os.path.join(MLP_DIR, track["name"].replace("/", "__") + ".mlp")


def mlp_sample_fmt(bits):
    """按目标位深给出 MLP 编码器的采样格式名。

    MLP 编码器只接受 planer 格式:s16p(16-bit) / s32p(24-bit)。

    **必须显式指定**，否则 FFmpeg 会沿用源的位深：例如 44.1k/16 的源经
    aresample 重采样到 48k 后**仍是 16-bit**（aresample 只改采样率，
    不改位深），于是被编成 16-bit MLP 混进 24-bit 的音频组。
    dvda-author 遇到这种参数不一致**不会报错**，照样出盘，
    所以必须在这里强制指定位深，并在编码后复核（见 ensure_mlp）。
    """
    return "s16p" if bits == 16 else "s32p"


def _mlp_interval(head):
    """从前两个 major sync 的 AU 下标差算出刷新间隔；不足 2 个则返回 None。"""
    pos, idx, n = 0, [], 0
    while pos + 4 <= len(head) and len(idx) < 2:
        h = int.from_bytes(head[pos:pos + 2], "big")
        ln = (h & 0x0FFF) * 2
        if ln < 4 or pos + ln > len(head):
            return None
        if head[pos + 4:pos + 7] == _SYNC_MAJOR:
            idx.append(n)
        n += 1
        pos += ln
    if len(idx) < 2:
        return None
    return idx[1] - idx[0]


def _mlp_cache_ok(path):
    """校验缓存文件的**实际编码参数**是否符合当前设置。

    只读首 128 KiB 与末尾 64 字节，代价极小。

    直接验文件本身，而不是比对一份「参数快照」：快照只能记录“上次跑时想要
    什么”，不能证明“磁盘上这些文件是用什么编的”。
    """
    try:
        size = os.path.getsize(path)
        if size < 4096:
            return False
        with open(path, "rb") as f:
            head = f.read(128 * 1024)
            f.seek(max(0, size - 64))
            tail = f.read()
    except OSError:
        return False

    if not head.startswith(b"\x00\x00\x00\x00") and head[4:7] != _SYNC_MAJOR:
        return False

    # 1) major sync 刷新间隔（恒为 MLP_MAX_INTERVAL）
    iv = _mlp_interval(head)
    if iv is not None and iv != MLP_MAX_INTERVAL:
        return False

    # 2) 头部字段与末尾结束标记（对齐是强制的，故总是校）
    if head[4:7] != _SYNC_MAJOR:
        return False
    b = head[4:32]
    ratebits = (b[5] >> 4) & 0x0F
    sr = (44100 if (ratebits & 8) else 48000) << (ratebits & 7)
    v = int.from_bytes(b[14:16], "big")
    if (v & 0x7FFF) != mlp_align.peak_bitrate_raw(sr):
        return False
    if (b[16] & 3) != 1:
        return False
    if _EOS not in tail:
        return False
    return True


def ensure_mlp(track):
    """确保该曲目已生成无损 MLP;返回 MLP 路径。已存在且不旧于源文件则复用。

    直接以**源文件**为输入,需要归一化的曲目在同一命令内重采样,
    与 01_prepare.py 的校验滤镜链保持一致。

    新编码完成后用 ffprobe 复核采样率与位深是否与音频组一致 ——
    不一致说明归一化失效,会生成参数混杂的非法音频组,故直接报错。
    """
    os.makedirs(MLP_DIR, exist_ok=True)
    mlp = mlp_path_for(track)
    if os.path.exists(mlp) and os.path.getsize(mlp) > 0:
        fresh = os.path.getmtime(mlp) >= os.path.getmtime(track["src"])
        if fresh and _mlp_cache_ok(mlp):
            _cache_stats["hit"] += 1
            return mlp
        # 源已更新或编码参数不符 → 删掉重编（缓存可以重建，不能将就）
        _cache_stats["stale"] += 1
        try:
            os.remove(mlp)
        except OSError:
            pass
    cmd = [FFMPEG, "-hide_banner", "-loglevel", "error", "-y",
           "-i", track["src"]]
    if track.get("resample_to"):
        # MLP 只存音频,容器不带标签,无需 -map_metadata -1
        cmd += ["-af", f"aresample={track['resample_to']}:resampler=soxr"]
    cmd += ["-sample_fmt", mlp_sample_fmt(track["bits"])]
    # major sync 间隔：固定对齐参考实现（见文件头说明）
    cmd += ["-max_interval", str(MLP_MAX_INTERVAL)]
    cmd += ["-c:a", "mlp", "-strict", "-2", mlp]
    r = run(cmd)
    if r.returncode != 0 or not os.path.exists(mlp):
        raise RuntimeError(f"MLP 编码失败: {track['src']}")

    # 复核:位深与采样率必须与所属音频组一致
    got = ffprobe_mlp(mlp)
    exp = (track["sr"], track["bits"])
    if got != exp:
        os.remove(mlp)
        raise RuntimeError(
            f"MLP 参数不符: {track['title']} 期望 {exp[0]}Hz/{exp[1]}bit, "
            f"实际 {got[0]}Hz/{got[1]}bit  ({track['src']})")

    # 头部对齐（无条件执行；不重编码。自检不过就报错，不交出未验证的流）
    _align_in_place(mlp)
    return mlp


def _align_in_place(path):
    """对刚编码出的 MLP 做头部对齐，并自检结果。"""
    data = open(path, "rb").read()
    new, _info = mlp_align.align_bytes(data)
    if new != data:
        with open(path, "wb") as f:
            f.write(new)
    # 自检：所有 major sync 校验和、每个 AU 头奇偶、子流 parity/checksum
    r = mlp_align.inspect(new)
    bad = (r["ms_errors"] or r["au_parity_errors"] or r["sub_errors"])
    if bad or not r["eos"]:
        raise RuntimeError(
            "MLP 对齐后自检失败: %s (ms=%d au=%d sub=%d eos=%s)"
            % (os.path.basename(path), len(r["ms_errors"]),
               len(r["au_parity_errors"]), len(r["sub_errors"]), r["eos"]))


def _report_cache():
    """报告 MLP 缓存的命中与失效情况（失效由 _mlp_cache_ok 逐件判定）。"""
    hit, stale = _cache_stats["hit"], _cache_stats["stale"]
    if stale:
        print(f"[缓存] 复用 {hit} 个，{stale} 个因「源已更新或编码参数不符」"
              f"被丢弃重编")
    elif hit:
        print(f"[缓存] 全部复用 {hit} 个（编码参数与源均未变）")


# ---------------------------------------------------------------------------
# 外部 MLP（如 SurCode 产出）
# ---------------------------------------------------------------------------
_EXT_INDEX = None       # {basename(无扩展): 完整路径}，按需构建


def external_mlp_for(track):
    """推导该曲目在 DVDA_MLP_EXTERNAL_DIR 里的 MLP 路径。

    首选「镜像路径」：把音源路径的 <DVDA_SRC> 前缀换成外部目录，扩展名换 .mlp
    （要求外部产出的目录结构与音源一一对应）。

    镜像路径不存在时退回「按文件名全局搜」—— 外部工具可能把文件平铺在别的
    层级下。两次都找不到则返回 (镜像路径, None)，由调用方报错。

    返回 (镜像路径, 命中路径或 None)
    """
    global _EXT_INDEX
    src = track["src"]
    root = CFG.src.rstrip("/")
    if src.startswith(root + "/"):
        rel = src[len(root) + 1:]
    else:
        rel = os.path.basename(src)
    mirror = os.path.join(MLP_EXTERNAL_DIR,
                          os.path.splitext(rel)[0] + ".mlp")
    if os.path.exists(mirror):
        return mirror, mirror

    if _EXT_INDEX is None:
        _EXT_INDEX = {}
        for r, _dirs, names in os.walk(MLP_EXTERNAL_DIR):
            for n in names:
                if n.lower().endswith(".mlp"):
                    _EXT_INDEX.setdefault(os.path.splitext(n)[0],
                                          os.path.join(r, n))
    return mirror, _EXT_INDEX.get(os.path.splitext(os.path.basename(src))[0])


def probe_audio_params(path):
    """探测任意音频文件的 {sr, ch, bits}。

    用于在外部模式下确定**源文件自身的原生参数**。
    不能拿 manifest 的 resample_to 当源参数 —— 那个字段是**目标**采样率
    （本组的归一化目标），不是源的原生采样率。
    """
    r = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "a:0",
         "-show_entries", "stream=sample_rate,channels,bits_per_raw_sample",
         "-of", "default=nw=1:nk=1", path],
        capture_output=True, text=True, encoding="utf-8", errors="replace")
    nums = []
    for line in r.stdout.splitlines():
        try:
            nums.append(int(line.strip()))
        except ValueError:
            nums.append(0)
    while len(nums) < 3:
        nums.append(0)
    return {"sr": nums[0], "ch": nums[1], "bits": nums[2]}


def collect_external_mlps(tracks):
    """外部模式：定位并探测每首的 MLP，用**实际参数**覆盖 manifest 的值。

    外部编码器可能改采样率/位深，所以分组与分盘必须以探测结果为准。
    同时探测**源文件的原生参数**，用于判断外部编码器到底改了什么
    （比对比 manifest 的组参数更有意义：manifest 记的是归一化目标，不是原生值）。

    返回 (变更列表, 缺失列表, 声道分布)
    """
    changes, missing = [], []
    chans = {}
    for t in tracks:
        mirror, hit = external_mlp_for(t)
        if hit is None:
            missing.append((t["title"], mirror))
            continue
        p = probe_mlp_params(hit)
        if not p["sr"] or not p["bits"]:
            missing.append((t["title"], hit))
            continue
        o = probe_audio_params(t["src"])

        t["src_rate"] = o["sr"]
        t["src_bits"] = o["bits"]
        t["mlp"] = hit
        t["mlp_size"] = os.path.getsize(hit)
        t["sr"] = p["sr"]
        t["bits"] = p["bits"]
        t["ch"] = p["ch"]
        # 供 verify 侧构造重采样链：MLP 采样率若与源原生值不同就得重采样
        t["resample_to"] = p["sr"] if p["sr"] != o["sr"] else None
        t["mlp_source"] = "external"
        t["ext_resampled"] = bool(o["sr"] and p["sr"] != o["sr"])
        t["ext_rebitded"] = bool(o["bits"] and p["bits"] != o["bits"])
        t["param_changed"] = t["ext_resampled"] or t["ext_rebitded"]
        if t["param_changed"]:
            changes.append((t["title"],
                            (o["sr"], o["bits"]),
                            (p["sr"], p["bits"])))
        chans.setdefault(p["ch"], []).append(t["title"])
    return changes, missing, chans


def group_by_rate(disc_albums):
    """把一张盘的专辑按 (sr, bits) 分组,组内保持全局顺序。

    组内轨数超过 GROUP_TRACK_LIMIT 时,在专辑边界再拆一组
    (dvda-author 的 ATSI 表缓冲只有 3 扇区,过多轨会栈溢出)。
    """
    groups = OrderedDict()
    for album, trks in disc_albums:
        for t in trks:
            key = (t["sr"], t["bits"])
            groups.setdefault(key, []).append(t)
    result = []
    for (sr, bits), trks in groups.items():
        chunks = []
        cur = []
        cur_album = None
        for t in trks:
            a = t["album"] or t["title"]
            if a != cur_album and cur and len(cur) >= GROUP_TRACK_LIMIT:
                chunks.append(cur)
                cur = []
            cur.append(t)
            cur_album = a
        if cur:
            chunks.append(cur)
        for c in chunks:
            result.append((sr, bits, c))
    return result


def split_discs(album_list, disc_limit, num_discs):
    """按专辑边界分盘，专辑绝不拆散。

    album_list : [(专辑名, MLP 字节数, 曲目列表), ...]（保持全局顺序）
    disc_limit : 单盘容量上限（字节，已扣除 ISO 预留）
    num_discs  : 期望的盘数上限；0 表示不限制。仅用于「是否放得下」的判断
                 与提示，**不参与切分**。

    返回 (discs, msgs)：
      discs = [[(专辑名, 曲目列表), ...], ...]   每张盘的专辑序列
      msgs  = 面向用户的提示行

    分盘策略：**逐盘填满**（贪心）——依次把专辑放进当前盘，放不下就开新盘。
    专辑不拆散，所以这是「用最少盘数装下全部内容」的做法。

    为什么不按盘数均分：
      均分要把每张盘的目标定成「总量 ÷ 盘数」。但单盘实际能装更多时，
      这个目标会让前面的盘提前停手，剩下的内容反而挤出一个新的盘。
      例如总量 7.22 GiB、单盘上限 4.38 GiB：
        · 逐盘填满 → 2 张（约 4.3 + 2.9 GiB）
        · 均分 2 份 → 每张 3.61 GiB 就停，剩 0.10 GiB 变成第 3 张
    """
    msgs = []
    total = sum(sz for _, sz, _ in album_list)
    total_aob = total * AOB_OVERHEAD
    cnt = len(album_list)

    # 逐盘填满
    discs = []
    cur = []
    cur_size = 0
    for album, sz, trks in album_list:
        if cur and (cur_size + sz) * AOB_OVERHEAD > disc_limit:
            discs.append(cur)
            cur, cur_size = [], 0
        cur.append((album, trks))
        cur_size += sz
    if cur:
        discs.append(cur)

    # 单张专辑就超上限的情况（无法通过分盘解决）
    over = 0
    for i, d in enumerate(discs, 1):
        sz = sum(t["mlp_size"] for _, trks in d for t in trks)
        aob = sz * AOB_OVERHEAD
        if aob > disc_limit and len(d) == 1:
            over += 1
            msgs.append(
                f"[分盘][警告] 第 {i} 盘只有一张专辑却已估 AOB "
                f"{aob/1024**3:.2f} GiB，超出上限 "
                f"{disc_limit/1024**3:.2f} GiB —— 专辑不可拆分，"
                f"请减少该专辑曲目或改用更大容量的光盘")

    # 与期望盘数比较
    if num_discs and num_discs > 0 and len(discs) > num_discs:
        msgs.append(
            f"[分盘] 内容估 AOB {total_aob/1024**3:.2f} GiB，"
            f"按单盘 {disc_limit/1024**3:.2f} GiB 需 {len(discs)} 张，"
            f"超出期望的 {num_discs} 张")
        msgs.append(
            "[分盘] 若必须控制在 "
            f"{num_discs} 张，可用 DVDA_DISC_BYTES 换更大容量"
            "（如 DVD-9 = 8540123136），或精简内容")
    elif over == 0:
        extra = f"（期望不超过 {num_discs} 张）" if num_discs > 0 else ""
        msgs.append(f"[分盘] {cnt} 张专辑 -> {len(discs)} 张盘{extra}，"
                    f"每张均在 {disc_limit/1024**3:.2f} GiB 以内 ✔")
    else:
        msgs.append(f"[分盘] {cnt} 张专辑 -> {len(discs)} 张盘，"
                    f"其中 {over} 张超出上限（见上方警告）✗")
    return discs, msgs


def menu_args(disc_index, groups):
    """准备选曲菜单素材，返回要追加到 dvda-author 的参数。

    groups 的顺序必须与传给 dvda-author 的 `-g` 完全一致 ——
    菜单按钮写的是 `jump group G track K`，顺序错位就会点错曲目。

    返回 (args, plan)；菜单关闭时返回 ([], None)。
    """
    if not MENU_ON:
        return [], None

    # 依赖检查：先给出可操作的提示，而不是等 dvda-author 中途失败
    datadir = CFG.author_src
    if not os.path.isdir(os.path.join(datadir, "menu")):
        print(f"[菜单][FAIL] 找不到 {datadir}/menu（dvda-author 的素材目录）")
        print("           请确认 config.sh 的 DVDA_AUTHOR_SRC 指向源码根目录")
        return None, None
    bindir = CFG.menu_bindir
    missing = [b for b in MENU_BINS
               if not (os.path.exists(os.path.join(bindir, b))
                       or shutil.which(b))]
    if missing:
        print(f"[菜单][FAIL] 缺少辅助程序: {', '.join(missing)}")
        print(f"           查找目录: {bindir}")
        print("           处理: 先跑 bash build_dvda_author_mlp.sh"
              "（会编出 dvdauthor/spumux 并链接进来）")
        print("                 再确认已装 mjpegtools 与 imagemagick")
        return None, None
    if not menu_assets.have_magick():
        print("[菜单][FAIL] 找不到 ImageMagick（convert/magick）")
        print("           处理: sudo apt install imagemagick")
        return None, None

    menu_groups = [[t for t in files] for _, _, files in groups]
    outdir = os.path.join(MENU_DIR, f"disc{disc_index}")
    menulog = []          # 收集本模块的提示行，与 dvda-author 的输出分开
    plan = menu_assets.build_menu(menu_groups, outdir, CFG,
                                  log=lambda s: menulog.append(s))
    for line in menulog:
        print(line)

    blankscreen = os.path.join(outdir, "blankscreen.png")
    menu_assets.make_blankscreen(blankscreen)

    if not plan.font:
        print("[菜单][警告] 没有可用字体，菜单文字将不会显示")
    if plan.font_missing:
        print(f"[菜单][警告] 字体 {plan.font} 仍缺: "
              f"{'、'.join(sorted(plan.font_missing))} —— 这些字会是空白")
    # 自检：页数必须与 dvda-author 实际画出的页数一致，否则背景/文字会错位
    r2, drawn = menu_assets.pages_for([len(g) for g in menu_groups], plan.pages)
    if r2 != plan.rows or drawn != plan.pages:
        print(f"[菜单][警告] 分页不自洽：按 --nmenus={plan.pages} 推得 "
              f"{drawn} 页 x {r2} 行，本模块按 {plan.pages} 页 x "
              f"{plan.rows} 行排版 → 背景可能与按钮错位")
    print(f"[菜单] {plan.pages} 页, 每页 {plan.rows} 首, "
          f"字号 {plan.points}, 下划线宽 {plan.fontwidth}, "
          f"字体 {plan.font or '(无)'}")
    print(f"[菜单] 分组: "
          + ", ".join(f"组{i + 1}={len(g)}首" for i, g in enumerate(menu_groups))
          + f" → 各组页数 "
          + ", ".join(str(-(-len(g) // plan.rows)) for g in menu_groups))
    print(f"[菜单] 每页背景 {plan.pages} 张"
          + (f", 播放封面 {sum(1 for s in plan.stills if s)} 张"
             if any(plan.stills) else ""))

    args = plan.args(blankscreen, CFG.menu_font, datadir, bindir)
    return args, plan


def build_disc(disc_index, groups):
    """生成第 disc_index 张盘（从 1 起）并打包为 ISO。

    盘标识与卷标均取自 config.sh（DVDA_TITLE / DVDA_ISO_PREFIX）。
    临时目录用 ASCII 名（discN），避开中文路径在某些工具链下的编码问题。
    """
    tag = f"disc{disc_index}"
    volid = CFG.volid(disc_index)
    out = os.path.join(OUT_ROOT, tag)
    tmp = os.path.join(TMP_ROOT, tag)
    for d in (out, tmp):
        if os.path.exists(d):
            shutil.rmtree(d)
        os.makedirs(d)

    args = [DVDA]
    for _, _, files in groups:
        args += ["-g"] + [f["mlp"] for f in files]
    # 注意：非 core 构建下 -9/-X 会触发 make_absolute 返回 NULL 而崩溃，故不传。
    args += ["-o", out, "-D", tmp, "-W", "-P0", "-n"]

    # 选曲菜单（AMG）+ 播放封面（ASVS）。素材在 dvda-author 之前生成；
    # out/tmp 必须已经存在（dvda-author 自己建不出来）。
    margs, plan = menu_args(disc_index, groups)
    if margs is None and MENU_ON:
        print(f"[FAIL] 第 {disc_index} 盘菜单素材生成失败")
        print("       如暂不需要菜单，把 config.sh 的 DVDA_MENU 改为 off")
        return False
    args += margs

    # log_output=True：dvda-author 会把轨道表打印到 stdout，
    # 校验脚本要读它，所以这里捕获并写入构建日志。
    r = run(args, log_output=True)
    if r.returncode != 0:
        print(f"[FAIL] dvda-author 生成第 {disc_index} 盘失败")
        return False

    # 菜单是可选件：dvda-author 即使菜单环节出问题也可能照样退出 0，
    # 所以这里显式核对菜单文件是否真的产出了。
    if MENU_ON and plan is not None:
        ts = os.path.join(out, "AUDIO_TS")
        vob = os.path.join(ts, "AUDIO_TS.VOB")
        if not os.path.exists(vob):
            print(f"[FAIL] 第 {disc_index} 盘菜单文件 {vob} 未生成")
            print("       （dvda-author 未报错，但菜单没做出来）")
            print("       常见原因: 字号过大导致字幕遮罩无法识别、"
                  "字体不可用、图片尺寸不是 720x576")
            return False
        print(f"[菜单] {os.path.basename(vob)} "
              f"{os.path.getsize(vob):,} 字节 ✔")
        sv = os.path.join(ts, "AUDIO_SV.VOB")
        if os.path.exists(sv):
            sectors = os.path.getsize(sv) // 2048
            print(f"[菜单] {os.path.basename(sv)} {os.path.getsize(sv):,} 字节 "
                  f"({sectors} 扇区 / 上限 1024)")
        elif any(plan.stills):
            print("[菜单][警告] 未生成 AUDIO_SV.VOB（播放封面缺失）")

    # 末轨最后一个 pack 可能少写几字节填充，导致 AOB 不是 2048 的整数倍。
    # IFO 已按整扇区声明，故此处补零至扇区边界，使文件与声明严格一致。
    # （实测仅影响文件尾 4 字节，音频数据完整。）
    for aob in sorted(glob.glob(os.path.join(out, "AUDIO_TS", "*.AOB"))):
        size = os.path.getsize(aob)
        rem = size % 2048
        if rem:
            with open(aob, "ab") as fp:
                fp.write(b"\x00" * (2048 - rem))
            print(f"[补齐] {os.path.basename(aob)} 补 {2048 - rem} 字节至扇区边界")

    iso = os.path.join(ISO_DIR, f"{tag}.iso")
    os.makedirs(ISO_DIR, exist_ok=True)
    if os.path.exists(iso):
        os.remove(iso)
    r = run([MKISOFS, "-dvd-audio", "-V", volid, "-o", iso, out])
    if r.returncode != 0:
        print(f"[FAIL] mkisofs 打包第 {disc_index} 盘失败")
        return False

    iso_size = os.path.getsize(iso)
    limit = DISC_LIMIT_BYTES
    if iso_size <= limit:
        print(f"[容量] 第 {disc_index} 盘 ISO {iso_size:,} 字节, "
              f"余 {limit - iso_size:,} 字节, 可刻入")
    else:
        print(f"[容量][警告] 第 {disc_index} 盘 ISO {iso_size:,} 字节, "
              f"超出单盘上限 {iso_size - limit:,} 字节")

    os.makedirs(FINAL_DIR, exist_ok=True)
    final = os.path.join(FINAL_DIR, CFG.iso_name(disc_index))
    try:
        shutil.copy2(iso, final)
    except PermissionError as e:
        # 目标被占用（Windows 侧播放器/资源管理器打开了它，或刻录软件持句柄）。
        # 改名写 _new.iso 并把原因告知用户，避免留下两份同名 ISO 而不知用哪份。
        alt = os.path.join(FINAL_DIR, CFG.iso_name(disc_index)[:-4] + "_new.iso")
        shutil.copy2(iso, alt)
        final = alt
        print(f"[警告] 无法覆盖 {CFG.iso_name(disc_index)}：目标被占用")
        print(f"       原因: {e}")
        print(f"       已改写到 {os.path.basename(alt)}")
        print(f"       处理: 关闭占用该文件的程序后，把它改名/替换回")
        print(f"             {CFG.iso_name(disc_index)}")
    print(f"[OK] {final} ({os.path.getsize(final) / 1024**3:.2f} GB)")

    # 清理 dvda-author 临时目录(生成已完成,不再需要)
    if os.path.exists(tmp):
        shutil.rmtree(tmp)
        print(f"[清理] 临时目录 {tmp} 已删除")
    return True


def main():
    dry_run = "--dry-run" in sys.argv
    mp = MANIFEST
    if not os.path.exists(mp):
        print(f"[FAIL] 找不到 manifest.json: {mp}")
        print("       请先运行: python3 01_prepare.py")
        return 1

    # dry-run 不执行 dvda-author，日志里不会有轨道表。若覆盖 build.log，
    # audit_disc.py / verify.sh 就会拿不到上次真出盘的轨道表而误报，
    # 因此 dry-run 单独写一份日志。
    global BUILD_LOG
    if dry_run:
        BUILD_LOG = os.path.join(BUILD_DIR, "build-dryrun.log")
    else:
        BUILD_LOG = CFG.build_log

    # 构建日志：写入本次运行的分隔头与工具路径。
    # audit_disc.py / verify_pts_length.py 依赖此日志解析 dvda-author 命令行，
    # 所以无论通过 build.sh 还是直接运行本脚本，都必须留下日志。
    try:
        os.makedirs(BUILD_DIR, exist_ok=True)
        with open(BUILD_LOG, "a", encoding="utf-8") as f:
            f.write("\n" + "=" * 60 + "\n")
            f.write("[02_build.py]%s %s\n"
                    % (" [DRY-RUN]" if dry_run else "",
                       time.strftime("%Y-%m-%d %H:%M:%S")))
            f.write("  dvda-author : %s\n" % DVDA)
            f.write("  mkisofs     : %s\n" % MKISOFS)
            f.write("  output      : %s\n" % FINAL_DIR)
            f.write("  iso prefix  : %s\n" % CFG.iso_prefix)
            f.write("  title       : %s\n" % CFG.title)
            f.write("  max discs   : %s\n" % (NUM_DISCS or "unlimited"))
            if dry_run:
                f.write("  注: dry-run 未执行 dvda-author，本文件不含轨道表；\n")
                f.write("      审计请用 build.log（上次真出盘）。\n")
            f.write("=" * 60 + "\n")
    except OSError as e:
        print(f"[警告] 无法写入构建日志 {BUILD_LOG}: {e}")
    print(f"[日志] {BUILD_LOG}")

    with open(mp, encoding="utf-8") as f:
        manifest = json.load(f)

    # 扁平化 + 全局排序 + 记录源文件大小
    tracks = []
    for gname, g in manifest.items():
        sr, bits = g["sr"], g["bits"]
        for f in g["files"]:
            src = to_wsl(f["src"])
            tracks.append({
                "date": f["date"], "track": f["track"], "title": f["title"],
                "album": f["album"] or f["title"], "sr": sr, "bits": bits,
                "src": src, "name": f["name"], "dur": f.get("dur"),
                "resample_to": f.get("resample_to"),
                "size": os.path.getsize(src),
            })

    def track_num(t):
        m = re.match(r"\s*(\d+)", t or "")
        return int(m.group(1)) if m else 9999
    tracks.sort(key=lambda x: (x["date"], track_num(x["track"]), x["title"]))

    # 逐曲生成无损 MLP（缓存复用），并记录 MLP 体积用于分盘
    src_total = sum(t["size"] for t in tracks)
    print(f"总曲目 {len(tracks)} 首, 源文件合计 {src_total/1024**3:.2f} GiB")

    if USE_EXTERNAL_MLP:
        # ---- 外部模式：跳过编码，取外部编码器的产出 ----
        print(f"MLP 来源: external（{MLP_EXTERNAL_DIR}）—— 跳过编码")
        if not MLP_EXTERNAL_DIR or not os.path.isdir(MLP_EXTERNAL_DIR):
            print(f"[FAIL] DVDA_MLP_EXTERNAL_DIR 无效: "
                  f"{MLP_EXTERNAL_DIR or '(未设置)'}")
            return 1
        changes, missing, chans = collect_external_mlps(tracks)
        print(f"已定位 {len(tracks) - len(missing)}/{len(tracks)} 个外部 MLP")
        if missing:
            print(f"[FAIL] 有 {len(missing)} 首找不到外部 MLP：")
            for title, want in missing[:8]:
                print(f"    {title}")
                print(f"      期望: {want}")
            if len(missing) > 8:
                print(f"    ...（还有 {len(missing) - 8} 首）")
            return 1

        if len(chans) > 1:
            print("[FAIL] 外部 MLP 的声道数不一致：")
            for ch, titles in chans.items():
                print(f"    {ch} 声道: {len(titles)} 首，例如 {titles[0]}")
            print("       DVD-Audio 同批次必须同声道数，本工具链不做声道转换。")
            return 1

        if changes:
            print(f"\n[提示] {len(changes)} 首的参数被外部编码器改过"
                  f"（以下是「源原生 -> 外部 MLP」）：")
            for title, old, new in changes[:12]:
                print(f"    {title[:44]:<46} "
                      f"{old[0]}/{old[1]}bit -> {new[0]}/{new[1]}bit")
            if len(changes) > 12:
                print(f"    ...（还有 {len(changes) - 12} 首）")
            print("    已按**实际参数**分组与分盘；verify 侧对改过采样率的曲目"
                  "改为核对采样数，不强行逐字节比对。")
    else:
        print("开始无损 MLP 编码（已缓存则跳过）...")
        done = 0
        for t in tracks:
            t["mlp"] = ensure_mlp(t)
            t["mlp_size"] = os.path.getsize(t["mlp"])
            t["mlp_source"] = "ffmpeg"
            done += 1
            if done % 20 == 0 or done == len(tracks):
                print(f"  已处理 {done}/{len(tracks)}")
        _report_cache()

    mlp_total = sum(t["mlp_size"] for t in tracks)
    d_gib = mlp_total / 1024**3
    if src_total:
        print(f"MLP 合计 {d_gib:.2f} GiB "
              f"(相对源 {100 * mlp_total / src_total - 100:+.2f}%, "
              f"预计 AOB {mlp_total*AOB_OVERHEAD/1024**3:.2f} GiB)")
    else:
        print(f"MLP 合计 {d_gib:.2f} GiB "
              f"(预计 AOB {mlp_total*AOB_OVERHEAD/1024**3:.2f} GiB)")

    # MLP 索引的逐曲部分：MLP 容器不记录时长，校验脚本需借助此表回溯源文件
    idx_tracks = {t["mlp"]: {"src": t["src"], "dur": t["dur"],
                             "sr": t["sr"], "bits": t["bits"],
                             "ch": t.get("ch"),
                             "src_rate": t.get("src_rate"),
                             "src_bits": t.get("src_bits"),
                             "resample_to": t["resample_to"],
                             "mlp_source": t.get("mlp_source", "ffmpeg"),
                             "ext_resampled": t.get("ext_resampled", False),
                             "ext_rebitded": t.get("ext_rebitded", False),
                             "param_changed": t.get("param_changed", False),
                             "title": t["title"]}
                  for t in tracks}

    # 按专辑聚合(保持全局顺序)
    albums = OrderedDict()
    for t in tracks:
        albums.setdefault(t["album"], []).append(t)
    print(f"共 {len(albums)} 张专辑")

    # 按专辑边界分盘（专辑绝不拆散）
    disc_limit = DISC_LIMIT_BYTES - ISO_SAFETY
    album_list = [(a, sum(t["mlp_size"] for t in trks), trks)
                  for a, trks in albums.items()]
    discs, msgs = split_discs(album_list, disc_limit, NUM_DISCS)
    for m in msgs:
        print(m)

    print(f"\n=== 分盘结果 ({len(discs)} 张) ===")
    disc_groups = []
    for i, d in enumerate(discs, 1):
        groups = group_by_rate(d)
        disc_groups.append(groups)
        n = sum(len(trks) for _, trks in d)
        ms = sum(t["mlp_size"] for _, trks in d for t in trks)
        print(f"  第 {i} 盘: {n} 首, MLP {ms/1024**3:.2f} GiB "
              f"-> 估AOB {ms*AOB_OVERHEAD/1024**3:.2f} GiB  "
              f"卷标 \"{CFG.volid(i)}\"")

    # 输出 MLP 索引。
    # 除逐曲元信息外，还记录分盘构成：每盘有哪几组、每组有哪几轨。
    # 意义：ATS_01_N.AOB 存的就是第 N 组，校验脚本据此能**直接定位**
    # 某张盘某组的第 1 轨对应哪个源 MLP，无需解析构建日志
    # （日志被 dry-run 重建或已轮替时，回退到「按文件名排序取第 1 条」
    # 会选到另一组而误报不一致）。
    idx_plan = []
    for i, groups in enumerate(disc_groups, 1):
        idx_plan.append({
            "disc": i,
            "volid": CFG.volid(i),
            "iso": CFG.iso_name(i),
            "groups": [
                {"group": gi, "sr": sr, "bits": bits,
                 "aob": f"ATS_01_{gi}.AOB",
                 "tracks": [{"mlp": t["mlp"], "src": t["src"],
                             "title": t["title"],
                             "resample_to": t["resample_to"]}
                            for t in files]}
                for gi, (sr, bits, files) in enumerate(groups, 1)
            ],
        })
    idx = {"__meta__": {"generated": time.strftime("%Y-%m-%d %H:%M:%S"),
                        "build_log": BUILD_LOG,
                        "dry_run": dry_run,
                        "mlp_source": CFG.mlp_source,
                        "mlp_external_dir": MLP_EXTERNAL_DIR or None,
                        "discs": len(discs), "tracks": len(tracks),
                        "aob_layout": "第 N 组 -> AUDIO_TS/ATS_01_N.AOB"},
           "__discs__": idx_plan}
    idx.update(idx_tracks)
    with open(MLP_INDEX, "w", encoding="utf-8") as f:
        json.dump(idx, f, ensure_ascii=False, indent=2)
    print(f"MLP 索引已写入: {MLP_INDEX} "
          f"({len(idx_tracks)} 条曲目 + {len(idx_plan)} 张盘计划)")

    if dry_run:
        print("\n[DRY-RUN] 仅预览,不实际生成。")
        for i, groups in enumerate(disc_groups, 1):
            print(f"\n--- 第 {i} 盘: {len(groups)} 个组 ---")
            for sr, bits, files in groups:
                print(f"    {sr}/{bits}: {len(files)} 首")
        return 0

    for i, _ in enumerate(discs, 1):
        groups = disc_groups[i - 1]
        print(f"\n--- 第 {i} 盘: {len(groups)} 个组 ---")
        for sr, bits, files in groups:
            print(f"    {sr}/{bits}: {len(files)} 首")
        if not build_disc(i, groups):
            print(f"[FAIL] 第 {i} 盘 制作失败")
            return 1

    print("\n全部完成。")
    print(f"产物目录: {FINAL_DIR}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
