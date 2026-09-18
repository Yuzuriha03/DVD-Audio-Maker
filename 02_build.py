#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
步骤2:读取 manifest → 逐曲做无损 MLP 编码 → 按专辑日期序分盘 → dvda-author + mkisofs。

运行于 WSL 内部。

逻辑:
1. 读 manifest(自动探测 WSL 内部或 Windows 侧),wav 路径转 WSL
2. 每曲用 FFmpeg 编码为 MLP(Meridian Lossless Packing, 无损),结果缓存复用
3. 全部曲目按 (发布日, 曲序) 全局排序
4. 按专辑边界贪心分盘:每张不超过单层 DVD5 上限(专辑绝不跨盘、绝不拆散)
5. 每张盘内部按 (采样率, 位深) 分组(DVD-Audio 同组内参数须一致)
6. 每组超过 GROUP_TRACK_LIMIT 轨时按专辑边界再拆一组
   (dvda-author 的 ATSI 表缓冲仅 3 扇区,轨数过多会栈溢出)
7. dvda-author 以 MLP 为输入生成 AUDIO_TS + mkisofs 打包 + 复制到 D 盘

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

# ---- 路径配置 ----
# 下列路径均可用同名环境变量覆盖（详见 README「路径配置」）：
#   DVDA_WORK  DVDA_MANIFEST  DVDA_FINAL_DIR  DVDA_AUTHOR  DVDA_MKISOFS  DVDA_FFMPEG
WORK = os.environ.get("DVDA_WORK", "/root/dvda-build/wav").rstrip("/")
MANIFEST_CANDIDATES = [
    os.environ.get("DVDA_MANIFEST", "/root/dvda-build/manifest.json"),
    "/mnt/c/Users/yyz57/dvda_work2/manifest.json",
]
OUT_ROOT = os.environ.get("DVDA_OUT_ROOT", "/root/dvda-build/out")
TMP_ROOT = os.environ.get("DVDA_TMP_ROOT", "/root/dvda-build/tmp")
ISO_DIR = os.environ.get("DVDA_ISO_DIR", "/root/dvda-build/iso")
FINAL_DIR = os.environ.get("DVDA_FINAL_DIR", "/mnt/d/鸣潮DVD_Audio")
MLP_DIR = os.environ.get("DVDA_MLP_DIR", "/root/dvda-build/mlp")   # MLP 缓存目录

# 带 MLP 编码支持的 dvda-author（链接系统 FFmpeg 8，已迁移 API）
DVDA = os.environ.get("DVDA_AUTHOR", "/root/dvda-author-mlp8/src/dvda-author-dev")
MKISOFS = os.environ.get("DVDA_MKISOFS", "/opt/dvda-author/local.ubuntu.20.10/bin/mkisofs")
FFMPEG = os.environ.get("DVDA_FFMPEG", "ffmpeg")

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


def mlp_path_for(track):
    """MLP 缓存文件名：以原 WAV 路径可读地派生。"""
    flat = track["wav"].replace(WORK + "/", "")
    flat = flat.replace("/", "__")
    return os.path.join(MLP_DIR, flat[:-4] + ".mlp")


def ensure_mlp(track):
    """确保该曲目已生成无损 MLP；返回 MLP 路径。已存在且不旧于 WAV 则复用。"""
    os.makedirs(MLP_DIR, exist_ok=True)
    mlp = mlp_path_for(track)
    if os.path.exists(mlp) and os.path.getsize(mlp) > 0:
        if os.path.getmtime(mlp) >= os.path.getmtime(track["wav"]):
            return mlp
    r = run([FFMPEG, "-hide_banner", "-loglevel", "error", "-y",
             "-i", track["wav"], "-c:a", "mlp", "-strict", "-2", mlp])
    if r.returncode != 0 or not os.path.exists(mlp):
        raise RuntimeError(f"MLP 编码失败: {track['wav']}")
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
    final = os.path.join(FINAL_DIR, f"Wuthering_Waves_Singles_EPs_{disc_name[-1]}.iso")
    try:
        shutil.copy2(iso, final)
    except PermissionError:
        alt = os.path.join(FINAL_DIR, f"Wuthering_Waves_Singles_EPs_{disc_name[-1]}_new.iso")
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

    # 扁平化 + 全局排序 + 记录大小
    tracks = []
    for gname, g in manifest.items():
        sr, bits = g["sr"], g["bits"]
        for f in g["files"]:
            w = to_wsl(f["wav"])
            tracks.append({
                "date": f["date"], "track": f["track"], "title": f["title"],
                "album": f["album"] or f["title"], "sr": sr, "bits": bits,
                "wav": w, "size": os.path.getsize(w),
            })

    def track_num(t):
        m = re.match(r"\s*(\d+)", t or "")
        return int(m.group(1)) if m else 9999
    tracks.sort(key=lambda x: (x["date"], track_num(x["track"]), x["title"]))

    # 逐曲生成无损 MLP（缓存复用），并记录 MLP 体积用于分盘
    wav_total = sum(t["size"] for t in tracks)
    print(f"总曲目 {len(tracks)} 首, WAV 合计 {wav_total/1024**3:.2f} GiB")
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
          f"(压缩 {(1 - mlp_total/wav_total)*100:.2f}%, "
          f"预计 AOB {mlp_total*AOB_OVERHEAD/1024**3:.2f} GiB)")

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
