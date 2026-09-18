#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""核验每轨 PTS_length 是否与源音频实际时长一致。

方法：
  1. 从构建日志解析 dvda-author 的命令行，得到每张盘每个组的文件顺序
  2. 解析轨道表（记录顺序与文件顺序一致）
  3. 用 ffprobe 取对应 WAV 的时长，计算期望 tick = round(dur * 90000)
  4. 与 PTS_length 比对
"""
import pathlib
import os
import re
import subprocess
import sys

BUILD = os.environ.get("DVDA_BUILD_DIR", "/root/dvda-build")

LOG = pathlib.Path(BUILD, "rebuild-final.log")
if not LOG.exists():
    for cand in ("finalrebuild.log", "build.log"):
        if pathlib.Path(BUILD, cand).exists():
            LOG = pathlib.Path(BUILD, cand)
            break
text = LOG.read_text(encoding="utf-8", errors="replace")
text = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", text)   # 剥离 ANSI

# ---------- 解析每张盘的组与文件顺序（按 dvda-author 命令行） ----------
PFX = os.environ.get("DVDA_MLP_DIR", os.path.join(BUILD, "mlp")) + "/"
cmds = []
for line in text.splitlines():
    if not line.startswith("+ ") or " -g " not in line:
        continue
    m = re.search(r" -o (\S+) ", line)
    if not m:
        continue
    disc = m.group(1).split("/")[-1]
    groups = []
    for seg in line.split(" -g ")[1:]:
        seg = seg.split(" -o ")[0].split(" -D ")[0]
        files = []
        for chunk in seg.split(PFX)[1:]:
            end = chunk.find(".mlp")
            if end >= 0:
                files.append(PFX + chunk[:end + 4])
        groups.append(files)
    if groups:
        cmds.append((disc, groups))

print("解析到 %d 条 dvda-author 命令" % len(cmds))
for disc, groups in cmds:
    print("  %s: %d 个组, %d 轨"
          % (disc, len(groups), sum(len(g) for g in groups)))

# ---------- 解析轨道表 ----------
ROWPAT = re.compile(
    r"^\s*(\d+)\s+(\d+)/(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s*$",
    re.M)
rows = []
for m in ROWPAT.finditer(text):
    g = [int(x) for x in m.groups()]
    rows.append({
        "group": g[0], "track": g[3],
        "first_sect": g[4], "last_sect": g[5],
        "first_pts": g[6], "pts_len": g[7], "cga": g[8],
    })
print()
print("解析到轨道行数: %d" % len(rows))

expected = []
for disc, groups in cmds:
    for g in groups:
        expected.extend(g)
print("期望文件数: %d" % len(expected))
if len(expected) != len(rows):
    print("!! 数量不一致，无法逐轨比对")
    sys.exit(1)


def wav_of(mlp):
    name = mlp.split("/")[-1].replace("__", "/", 1)
    if name.endswith(".mlp"):
        name = name[:-4] + ".wav"
    return os.environ.get("DVDA_WORK", os.path.join(BUILD, "wav")).rstrip("/") + "/" + name


def dur(wav):
    r = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration",
         "-of", "default=nw=1:nk=1", wav], capture_output=True, text=True)
    try:
        return float(r.stdout.strip())
    except ValueError:
        return None


print()
print("逐轨核对 PTS_length（容差 2 tick）:")
bad, nodur, same = [], [], 0
for i, (mlp, row) in enumerate(zip(expected, rows)):
    wav = wav_of(mlp)
    d = dur(wav)
    if d is None:
        nodur.append((i, wav))
        continue
    exp = round(d * 90000)
    diff = row["pts_len"] - exp
    if abs(diff) > 2:
        bad.append((i, diff, pathlib.Path(mlp).name, d, row["pts_len"]))
    if i > 0 and rows[i]["pts_len"] == rows[i - 1]["pts_len"]:
        same += 1

print("  相邻轨 PTS_length 相同的对数: %d" % same)
print("  取不到时长的轨数: %d" % len(nodur))
for i, w in nodur[:5]:
    print("     #%d %s" % (i, w))
print("  数值不匹配的轨数: %d" % len(bad))
for i, diff, name, d, act in bad[:15]:
    print("     #%-3d 差 %7d tick (%6.1f ms)  源 %.3f 秒  表 %d"
          % (i, diff, diff / 90.0, d, act))

print()
print("组3（44.1kHz）逐轨:")
for r in rows:
    if r["group"] == 3:
        print("   轨%2d  扇区 %6d..%-6d First_PTS=%-4d PTS_length=%-10d (%.3f 秒)"
              % (r["track"], r["first_sect"], r["last_sect"],
                 r["first_pts"], r["pts_len"], r["pts_len"] / 90000))

if not bad and not nodur:
    print()
    print("结论: 全部 147 轨的 PTS_length 与源音频时长一致 ✔")
