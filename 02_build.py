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
4. 按专辑边界贪心分盘:每张不超过单层 DVD5 上限(专辑绝不跨盘、绝不拆散)
5. 每张盘内部按 (采样率, 位深) 分组(DVD-Audio 同组内参数须一致)
6. 每组超过 GROUP_TRACK_LIMIT 轨时按专辑边界再拆一组
   (dvda-author 的 ATSI 表缓冲仅 3 扇区,轨数过多会栈溢出)
7. dvda-author 以 MLP 为输入生成 AUDIO_TS + mkisofs 打包 + 复制到 D 盘
8. 输出 mlp_index.json(MLP→源文件/时长/重采样),供 verify.sh 与
   verify_pts_length.py 使用(MLP 容器不记录时长,无法回读)

为什么不经过 WAV:
  实测「源 --[重采样]--> MLP」与「源 --[重采样]--> WAV --> MLP」输出逐字节一致,
  WAV 仅为中转。去掉后可省约 9 GB 落盘与一轮读写 I/O。

注: 使用自编译的 dvda-author(链接系统 FFmpeg 8, 已适配 ch_layout 等 API),
    可对 24-bit 音频做无损 MLP 编码。
"""

import os
import re
import glob
import json
import shutil
import subprocess
from collections import OrderedDict

# ---- 路径配置（均可用环境变量覆盖） ----
_E = os.environ.get
BUILD_DIR = _E("DVDA_BUILD_DIR", "/root/dvda-build")
MANIFEST_CANDIDATES = [
    _E("DVDA_MANIFEST", os.path.join(BUILD_DIR, "manifest.json")),
    "/mnt/c/Users/yyz57/dvda_work2/manifest.json",
]
OUT_ROOT = _E("DVDA_OUT_ROOT", os.path.join(BUILD_DIR, "out"))
TMP_ROOT = _E("DVDA_TMP_ROOT", os.path.join(BUILD_DIR, "tmp"))
ISO_DIR = _E("DVDA_ISO_DIR", os.path.join(BUILD_DIR, "iso"))
FINAL_DIR = _E("DVDA_FINAL_DIR", "/mnt/d/鸣潮DVD_Audio")
MLP_DIR = _E("DVDA_MLP_DIR", os.path.join(BUILD_DIR, "mlp"))   # MLP 缓存目录
MLP_INDEX = _E("DVDA_MLP_INDEX",
               os.path.join(BUILD_DIR, "mlp_index.json"))  # MLP → 源/时长/重采样
ISO_PREFIX = _E("DVDA_ISO_PREFIX", "Wuthering_Waves_Singles_EPs")

# 带 MLP 编码支持的 dvda-author（链接系统 FFmpeg 8，已迁移 API）
DVDA = _E("DVDA_AUTHOR", "/root/dvda-author-mlp8/src/dvda-author-dev")
MKISOFS = _E("DVDA_MKISOFS",
             "/opt/dvda-author/local.ubuntu.20.10/bin/mkisofs")
FFMPEG = _E("DVDA_FFMPEG", "ffmpeg")

MAX_TRACKS = 99              # DVD-Audio 每组协议上限
# dvda-author 的 ATSI 表缓冲固定为 3 扇区(6144 字节)，每轨约 52 字节，
# 超过约 70 轨会栈溢出（stack smashing）。这里取 64 作为安全上限，
# 超出时按专辑边界再拆一个组（专辑仍不拆散）。
GROUP_TRACK_LIMIT = 64
DISC_CAP = 4.7 * 1024**3     # 单层 DVD 容量(字节,十进制 4.7GB)
DVD5_BYTES = 4707319808      # 单层 DVD 物理上限(4.37 GiB)
# MLP 进入 AOB 后的实测开销系数（实测 80,021,504 / 78,337,762 = 1.02150）
AOB_OVERHEAD = 1.025         # 留少量安全余量
ISO_SAFETY = 8 * 1024**2     # ISO 文件系统与 IFO 预留
NUM_DISCS = 2                # 目标盘数


def to_wsl(p):
    p = p.replace("\\", "/")
    if re.match(r"^[a-zA-Z]:", p):
        p = "/mnt/" + p[0].lower() + p[2:]
    return p


def run(cmd):
    print("+", " ".join(cmd))
    return subprocess.run(cmd)


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
    与旧版「WAV 路径去掉 /root/dvda-build/wav/ 后把 / 换成 __」的结果完全一致,
    因此可以复用此前已生成的 MLP 缓存。
    """
    return os.path.join(MLP_DIR, track["name"].replace("/", "__") + ".mlp")


def mlp_sample_fmt(bits):
    """按目标位深给出 MLP 编码器的采样格式名。

    MLP 编码器只接受 planer 格式:s16p(16-bit) / s32p(24-bit)。
    必须显式指定,否则 FFmpeg 会沿用源的位深:
      - 44.1k/16 的源重采样到 48k 后仍是 16-bit
      - 会被编码成 16-bit MLP,混进 24-bit 的音频组
        (旧流程靠 -c:a pcm_s24le 中转间接强制了位深,直连后会丢失该约束)
    实测:加此参数后输出与旧「WAV + pcm_s24le」路径逐字节一致。
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


def build_disc(disc_name, volid, groups):
    out = os.path.join(OUT_ROOT, disc_name)
    tmp = os.path.join(TMP_ROOT, disc_name)
    for d in (out, tmp):
        if os.path.exists(d):
            shutil.rmtree(d)
        os.makedirs(d)

    args = [DVDA]
    for _, _, files in groups:
        args += ["-g"] + [f["mlp"] for f in files]
    # 注意：非 core 构建下 -9/-X 会触发 make_absolute 返回 NULL 而崩溃，故不传。
    args += ["-o", out, "-D", tmp, "-W", "-P0", "-n"]
    r = run(args)
    if r.returncode != 0:
        print(f"[FAIL] dvda-author 生成 {disc_name} 失败")
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

    iso = os.path.join(ISO_DIR, f"{disc_name}.iso")
    os.makedirs(ISO_DIR, exist_ok=True)
    if os.path.exists(iso):
        os.remove(iso)
    r = run([MKISOFS, "-dvd-audio", "-V", volid, "-o", iso, out])
    if r.returncode != 0:
        print(f"[FAIL] mkisofs 打包 {disc_name} 失败")
        return False

    iso_size = os.path.getsize(iso)
    if iso_size <= DVD5_BYTES:
        print(f"[容量] {disc_name} ISO {iso_size} 字节, "
              f"余 {DVD5_BYTES - iso_size} 字节, 可刻入 DVD5")
    else:
        print(f"[容量][警告] {disc_name} ISO {iso_size} 字节, "
              f"超出 DVD5 上限 {iso_size - DVD5_BYTES} 字节")

    os.makedirs(FINAL_DIR, exist_ok=True)
    final = os.path.join(FINAL_DIR, f"{ISO_PREFIX}_{disc_name[-1]}.iso")
    try:
        shutil.copy2(iso, final)
    except PermissionError:
        alt = os.path.join(FINAL_DIR,
                           f"{ISO_PREFIX}_{disc_name[-1]}_new.iso")
        shutil.copy2(iso, alt)
        final = alt
    print(f"[OK] {final} ({os.path.getsize(final) / 1024**3:.2f} GB)")

    # 清理 dvda-author 临时目录(生成已完成,不再需要)
    if os.path.exists(tmp):
        shutil.rmtree(tmp)
        print(f"[清理] 临时目录 {tmp} 已删除")
    return True


def main():
    dry_run = "--dry-run" in __import__("sys").argv
    mp = next((p for p in MANIFEST_CANDIDATES if os.path.exists(p)), None)
    if not mp:
        print("[FAIL] 找不到 manifest.json,请先运行 prepare.py")
        return
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

    # 按专辑边界分盘:盘1 尽量填满(不拆专辑),考虑 AOB 开销
    disc_limit = DVD5_BYTES - ISO_SAFETY
    album_list = list(albums.items())
    discs = []
    cur = []
    cur_size = 0
    for album, trks in album_list:
        sz = sum(t["mlp_size"] for t in trks)
        # 当前已有内容且加本专辑后超出单层上限时切盘
        if cur and (cur_size + sz) * AOB_OVERHEAD > disc_limit:
            discs.append(cur)
            cur = []
            cur_size = 0
        cur.append((album, trks))
        cur_size += sz
    if cur:
        discs.append(cur)

    print(f"\n=== 分盘结果 ({len(discs)} 张, 盘1填满) ===")
    for i, d in enumerate(discs, 1):
        n = sum(len(trks) for _, trks in d)
        ms = sum(t["mlp_size"] for _, trks in d for t in trks)
        print(f"  盘{i}: {n} 首, MLP {ms/1024**3:.2f} GiB -> 估AOB {ms*AOB_OVERHEAD/1024**3:.2f} GiB")

    if dry_run:
        print("\n[DRY-RUN] 仅预览,不实际生成。")
        for i, d in enumerate(discs, 1):
            groups = group_by_rate(d)
            print(f"\n--- 盘{i}: {len(groups)} 个组 ---")
            for sr, bits, files in groups:
                print(f"    {sr}/{bits}: {len(files)} 首")
        return

    for i, d in enumerate(discs, 1):
        disc_name = f"盘{i}"
        volid = f"Wuthering Waves Singles & EPs {i}"
        groups = group_by_rate(d)
        print(f"\n--- 盘{i}: {len(groups)} 个组 ---")
        for sr, bits, files in groups:
            print(f"    {sr}/{bits}: {len(files)} 首")
        if not build_disc(disc_name, volid, groups):
            print(f"[FAIL] 盘{i} 制作失败")
            return

    print("\n全部完成。")


if __name__ == "__main__":
    main()
