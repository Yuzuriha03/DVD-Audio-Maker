#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""核验每轨 PTS_length 是否与源音频实际时长一致。

方法：
  1. 从构建日志解析 dvda-author 的命令行，得到每张盘每个组的文件顺序
  2. 解析轨道表（记录顺序与文件顺序一致）
  3. 用 mlp_index.json 查到对应源音频的时长，计算期望 tick = round(dur * 90000)
  4. 与 PTS_length 比对

注：MLP 容器不记录时长（ffprobe 返回 N/A），无法直接回读，
    故时长由 01_prepare.py 写入 manifest，再由 02_build.py 汇总成
    mlp_index.json。

    构建日志取候选列表中**修改时间最新**者 —— 目录里可能残留上次构建的
    旧日志（rebuild-final.log），按固定顺序取会拿到陈旧轨道表。
"""
import json
import os
import pathlib
import re
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dvda_config import load as load_config          # noqa: E402

CFG = load_config(need=None, quiet=True)
BUILD_DIR = CFG.build_dir
_cands = [os.environ.get("DVDA_BUILD_LOG"),
          CFG.build_log,
          os.path.join(BUILD_DIR, "rebuild-final.log"),
          os.path.join(BUILD_DIR, "finalrebuild.log")]
_exist = [pathlib.Path(p) for p in _cands if p and pathlib.Path(p).exists()]
if not _exist:
    print("!! 找不到构建日志")
    sys.exit(1)
LOG = max(_exist, key=lambda p: p.stat().st_mtime)
print(f"构建日志: {LOG}  "
      f"(mtime {time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(LOG.stat().st_mtime))})")
text = LOG.read_text(encoding="utf-8", errors="replace")
text = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", text)   # 剥离 ANSI

INDEX = pathlib.Path(CFG.mlp_index)
if not INDEX.exists():
    print("!! 未找到 %s，请先运行 02_build.py" % INDEX)
    sys.exit(1)
mlp_index = json.loads(INDEX.read_text(encoding="utf-8"))
# 索引里除逐曲条目外还有 __meta__ / __discs__ 两个元数据段，计数时排除
_n_tracks = sum(1 for k in mlp_index if not k.startswith("__"))
_n_discs = len(mlp_index.get("__discs__") or [])
print("mlp_index.json: %d 条曲目%s"
      % (_n_tracks, "，%d 张盘计划" % _n_discs if _n_discs else ""))

# ---------- 解析每张盘的组与文件顺序（按 dvda-author 命令行） ----------
PFX = CFG.mlp_dir.rstrip("/") + "/"
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


def dur_of(mlp):
    """从 mlp_index.json 取源音频时长（秒）；查不到返回 None。"""
    e = mlp_index.get(mlp)
    if not e:
        return None
    d = e.get("dur")
    return float(d) if d else None


print()
print("逐轨核对 PTS_length（容差 2 tick）:")
bad, nodur, same = [], [], 0
for i, (mlp, row) in enumerate(zip(expected, rows)):
    d = dur_of(mlp)
    if d is None:
        nodur.append((i, mlp))
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
print("逐轨明细（按组）：")
_groups = sorted({r["group"] for r in rows})
for _g in _groups:
    _rs = [r for r in rows if r["group"] == _g]
    print("  --- 组 %d: %d 轨 ---" % (_g, len(_rs)))
    for r in _rs:
        print("   轨%2d  扇区 %8d..%-8d First_PTS=%-4d PTS_length=%-10d (%.3f 秒)"
              % (r["track"], r["first_sect"], r["last_sect"],
                 r["first_pts"], r["pts_len"], r["pts_len"] / 90000))

if not bad and not nodur:
    print()
    print("结论: 全部 %d 轨的 PTS_length 与源音频时长一致 ✔" % len(rows))
else:
    sys.exit(1)
