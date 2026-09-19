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
8. 输出 mlp_index.json(MLP→源文件/时长/重采样),供 verify.sh 与
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
MLP_INDEX = CFG.mlp_index             # MLP → 源/时长/重采样

# 带 MLP 编码支持的 dvda-author（链接系统 FFmpeg 8，已迁移 API）
DVDA = CFG.dvda
MKISOFS = CFG.mkisofs
FFMPEG = CFG.ffmpeg

# dvda-author 的 ATSI 表缓冲固定为 3 扇区(6144 字节)，每轨约 52 字节，
# 超过约 70 轨会栈溢出（stack smashing）。config.sh 里可调，上限 70。
GROUP_TRACK_LIMIT = CFG.group_track_limit
DISC_LIMIT_BYTES = CFG.disc_bytes     # 单盘容量（默认单层 DVD5）
NUM_DISCS = CFG.max_discs             # 目标盘数；0 = 不限制


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
        logf = open(CFG.build_log, "a", encoding="utf-8")
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


def ffprobe_mlp(path):
    """读 MLP 的 (sample_rate, bits_per_raw_sample)。

    注意:MLP 容器不记录时长(duration=N/A),但采样率与位深可读,
    可用于复核编码结果是否与音频组参数一致。
    """
    r = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "a:0",
         "-show_entries", "stream=sample_rate,bits_per_raw_sample",
         "-of", "default=nw=1:nk=1", path],
        capture_output=True, text=True, encoding="utf-8", errors="replace")
    nums = []
    for line in r.stdout.splitlines():
        try:
            nums.append(int(line.strip()))
        except ValueError:
            nums.append(0)
    while len(nums) < 2:
        nums.append(0)
    return nums[0], nums[1]


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
        if os.path.getmtime(mlp) >= os.path.getmtime(track["src"]):
            return mlp
    cmd = [FFMPEG, "-hide_banner", "-loglevel", "error", "-y",
           "-i", track["src"]]
    if track.get("resample_to"):
        # MLP 只存音频,容器不带标签,无需 -map_metadata -1
        cmd += ["-af", f"aresample={track['resample_to']}:resampler=soxr"]
    cmd += ["-sample_fmt", mlp_sample_fmt(track["bits"]),
            "-c:a", "mlp", "-strict", "-2", mlp]
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
    return mlp


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
    # log_output=True：dvda-author 会把轨道表打印到 stdout，
    # 校验脚本要读它，所以这里捕获并写入构建日志。
    r = run(args, log_output=True)
    if r.returncode != 0:
        print(f"[FAIL] dvda-author 生成第 {disc_index} 盘失败")
        return False

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
    except PermissionError:
        alt = os.path.join(FINAL_DIR, CFG.iso_name(disc_index)[:-4] + "_new.iso")
        shutil.copy2(iso, alt)
        final = alt
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

    # 构建日志：写入本次运行的分隔头与工具路径。
    # audit_disc.py / verify_pts_length.py 依赖此日志解析 dvda-author 命令行，
    # 所以无论通过 build.sh 还是直接运行本脚本，都必须留下日志。
    try:
        os.makedirs(BUILD_DIR, exist_ok=True)
        with open(CFG.build_log, "a", encoding="utf-8") as f:
            f.write("\n" + "=" * 60 + "\n")
            f.write("[02_build.py] %s\n" % time.strftime("%Y-%m-%d %H:%M:%S"))
            f.write("  dvda-author : %s\n" % DVDA)
            f.write("  mkisofs     : %s\n" % MKISOFS)
            f.write("  output      : %s\n" % FINAL_DIR)
            f.write("  iso prefix  : %s\n" % CFG.iso_prefix)
            f.write("  title       : %s\n" % CFG.title)
            f.write("  max discs   : %s\n" % (NUM_DISCS or "unlimited"))
            f.write("=" * 60 + "\n")
    except OSError as e:
        print(f"[警告] 无法写入构建日志 {CFG.build_log}: {e}")

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
    print("开始无损 MLP 编码（已缓存则跳过）...")

    done = 0
    for t in tracks:
        t["mlp"] = ensure_mlp(t)
        t["mlp_size"] = os.path.getsize(t["mlp"])
        done += 1
        if done % 20 == 0 or done == len(tracks):
            print(f"  已处理 {done}/{len(tracks)}")

    mlp_total = sum(t["mlp_size"] for t in tracks)
    print(f"MLP 合计 {mlp_total/1024**3:.2f} GiB "
          f"(相对源压缩 {(1 - mlp_total/src_total)*100:.2f}%, "
          f"预计 AOB {mlp_total*AOB_OVERHEAD/1024**3:.2f} GiB)")

    # 输出 MLP 索引：MLP 容器不记录时长，校验脚本需借助此表回溯源文件
    idx = {t["mlp"]: {"src": t["src"], "dur": t["dur"],
                      "sr": t["sr"], "bits": t["bits"],
                      "resample_to": t["resample_to"],
                      "title": t["title"]}
           for t in tracks}
    with open(MLP_INDEX, "w", encoding="utf-8") as f:
        json.dump(idx, f, ensure_ascii=False, indent=2)
    print(f"MLP 索引已写入: {MLP_INDEX} ({len(idx)} 条)")

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
    for i, d in enumerate(discs, 1):
        n = sum(len(trks) for _, trks in d)
        ms = sum(t["mlp_size"] for _, trks in d for t in trks)
        print(f"  第 {i} 盘: {n} 首, MLP {ms/1024**3:.2f} GiB "
              f"-> 估AOB {ms*AOB_OVERHEAD/1024**3:.2f} GiB  "
              f"卷标 \"{CFG.volid(i)}\"")

    if dry_run:
        print("\n[DRY-RUN] 仅预览,不实际生成。")
        for i, d in enumerate(discs, 1):
            groups = group_by_rate(d)
            print(f"\n--- 第 {i} 盘: {len(groups)} 个组 ---")
            for sr, bits, files in groups:
                print(f"    {sr}/{bits}: {len(files)} 首")
        return 0

    for i, d in enumerate(discs, 1):
        groups = group_by_rate(d)
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
