#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""成品光盘一致性审计。

按「音频组」独立核对（扇区号在各组内从 0 起）：

  A. 组内 AOB 扇区总数 == 该组轨道表最大末扇区 + 1
  B. 组内各轨扇区首尾相接（无缝无叠）
  C. 每个扇区都有 PTS（时间轴完整）
  D. 每个 PTS 下降点的位置恰好是某轨的首个扇区（轨边界）
  E. 各轨起点的 PTS 取值

用法:
  python3 audit_disc.py [ISO 目录] [构建日志]
默认（取自 config.sh）:
  ISO 目录 = DVDA_FINAL_DIR
  构建日志 = <BUILD_DIR>/build.log（在候选里取 mtime 最新者）

  候选（按名字）: build.log、rebuild-final.log、finalrebuild.log
  build.sh 每次运行都会重写 build.log，因此它总是当前这次构建的日志。

  ⚠ 不要按固定顺序取第一个存在的文件 —— 目录里可能残留上一次构建的
    旧日志（如 rebuild-final.log），会导致：
      · 轨道表来自旧构建、AOB 来自新构建 → 误报「扇区数不一致」
      · PTS 下降点与旧轨边界比对 → 误报「未落在轨边界」
    故这里改为按 mtime 取最新，并在输出中打印所用日志及其时间以便核对。
"""
import os
import pathlib
import re
import shutil
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dvda_config import load as load_config          # noqa: E402

CFG = load_config(need=None, quiet=True)
BUILD_DIR = CFG.build_dir
ISO_PREFIX = CFG.iso_prefix
# 第 1 个位置参数是 ISO 目录，第 2 个是构建日志；两者都可省略
ISO_DIR = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else CFG.final_dir)

_LOGS = [
    pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else None,
    pathlib.Path(CFG.build_log),
    pathlib.Path(os.path.join(BUILD_DIR, "rebuild-final.log")),
    pathlib.Path(os.path.join(BUILD_DIR, "finalrebuild.log")),
]
# 显式指定则直接用；否则在候选里取 mtime 最新者（避免读到旧构建日志）
_existing = [p for p in _LOGS if p and p.exists()]
LOG = max(_existing, key=lambda p: p.stat().st_mtime) if _existing else None
if LOG is not None:
    print(f"构建日志: {LOG}  "
          f"(mtime {time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(LOG.stat().st_mtime))}"
          f", {LOG.stat().st_size:,} B)")
    others = [p for p in _existing if p != LOG]
    if others:
        newest_other = max(p.stat().st_mtime for p in others)
        if LOG.stat().st_mtime - newest_other < 60:
            print(f"  ⚠ 另有 {len(others)} 个日志时间相近，"
                  f"请确认所用日志对应本次构建")

WORK = pathlib.Path(os.path.join(BUILD_DIR, "disc-audit"))
ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
# 轨道表行: 组 | 标题号/总数 | 轨号 | 首扇区 | 末扇区 | First_PTS | PTS_length | cga
ROW = re.compile(
    r"^\s*(\d+)\s+(\d+)/(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s*$",
    re.M)


def parse_pts(b):
    """解析 5 字节 PTS/DTS 字段。"""
    return ((((b[0] >> 1) & 0x07) << 30)
            | ((((b[1] << 8) | b[2]) >> 1) << 15)
            | (((b[3] << 8) | b[4]) >> 1))


def dryrun_hint(log_path):
    """日志里没有轨道表时,判断是否因为这是 dry-run 日志,并给出可操作建议。"""
    tips = []
    try:
        head = log_path.read_text(encoding="utf-8", errors="replace")[:4000]
    except OSError:
        head = ""
    if "[DRY-RUN]" in head or "dry-run 未执行 dvda-author" in head:
        tips.append("        该日志是 --dry-run 产生的：dry-run 不执行 dvda-author，"
                    "本来就没有轨道表。")
        tips.append("        解决：运行一次真正的出盘（bash build.sh），"
                    "审计会自动改用新的 build.log。")
    else:
        tips.append("        可能原因：该日志不是出盘时写的"
                    "（例如被 --dry-run 覆盖过，或只跑过 01_prepare/02_build 的一部分）。")
        tips.append("        解决：运行一次真正的 bash build.sh 后再审计。")
    dry = pathlib.Path(os.path.join(BUILD_DIR, "build-dryrun.log"))
    if dry.exists() and dry != log_path:
        tips.append(f"        另注：{dry.name} 是最新一次的 dry-run 日志"
                    f"（mtime {time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(dry.stat().st_mtime))}），"
                    f"它不含轨道表，不能用于审计。")
    return tips


def main():
    if LOG is None:
        print("[跳过] 未找到构建日志，无法解析轨道表")
        print(f"        已查找：{', '.join(str(p) for p in _LOGS if p)}")
        print("        解决：运行一次真正的 bash build.sh 生成日志后再审计。")
        return 2

    text = ANSI.sub("", LOG.read_text(encoding="utf-8", errors="replace"))

    rows = []
    for m in ROW.finditer(text):
        g = [int(x) for x in m.groups()]
        rows.append({"group": g[0], "track": g[3], "first": g[4], "last": g[5],
                     "first_pts": g[6], "pts_len": g[7]})
    if not rows:
        print("[跳过] 构建日志中未解析到轨道表")
        for t in dryrun_hint(LOG):
            print(t)
        return 2

    # 由 dvda-author 命令行确定每张盘的组数与顺序
    cmds = []
    for line in text.splitlines():
        if line.startswith("+ ") and " -g " in line:
            m = re.search(r" -o (\S+) ", line)
            if m:
                cmds.append((m.group(1).split("/")[-1], line.count(" -g ")))
    if not cmds:
        print("[跳过] 未解析到 dvda-author 命令行")
        return 2

    def disc_no(tag):
        """盘序号：discN -> N；无数字时给一个很大的值（排到后面）。"""
        m = re.search(r"(\d+)$", tag)
        return int(m.group(1)) if m else 10 ** 6

    def disc_label(tag):
        """盘显示名：discN -> 第 N 盘；否则原样。"""
        m = re.fullmatch(r"disc(\d+)", tag)
        return f"第 {m.group(1)} 盘" if m else tag

    idx, disc_rows = 0, {}
    for disc, ngrp in cmds:
        groups = []
        for _ in range(ngrp):
            if idx >= len(rows):
                break
            g = rows[idx]["group"]
            grp = []
            while idx < len(rows) and rows[idx]["group"] == g:
                grp.append(rows[idx])
                idx += 1
            groups.append((g, grp))
        disc_rows[disc] = groups

    ok = True
    summary = []

    for disc, groups in sorted(disc_rows.items(), key=lambda kv: disc_no(kv[0])):
        iso = next(ISO_DIR.glob("%s_%d.iso" % (ISO_PREFIX, disc_no(disc))),
                   None)
        print("=" * 72)
        if iso is None:
            print("!! 缺少 %s 的 ISO（应在 %s 下找 %s_%d.iso）"
                  % (disc, ISO_DIR, ISO_PREFIX, disc_no(disc)))
            ok = False
            continue
        print("### %s  (%d 字节)" % (iso.name, iso.stat().st_size))

        d = WORK / disc
        shutil.rmtree(d, ignore_errors=True)
        d.mkdir(parents=True)
        subprocess.run(["xorriso", "-osirrox", "on", "-indev", str(iso),
                        "-extract", "/AUDIO_TS", str(d / "AUDIO_TS")],
                       capture_output=True)
        audio = d / "AUDIO_TS"

        for g, rs in groups:
            aobs = sorted(audio.glob("ATS_%02d_*.AOB" % g))
            if not aobs:
                print("  组%d: !! 未找到 AOB" % g)
                ok = False
                continue

            sect = sum(a.stat().st_size // 2048 for a in aobs)
            declared = max(r["last"] for r in rs) + 1
            print("  组%d: %d 轨 / %d 个 AOB" % (g, len(rs), len(aobs)))

            a_ok = (sect == declared)
            print("     A. 扇区数 AOB=%d 轨道表=%d  %s"
                  % (sect, declared, "一致 ✔" if a_ok else "不一致 ✗"))
            ok &= a_ok

            gaps = [(rs[i - 1]["track"], rs[i]["track"])
                    for i in range(1, len(rs))
                    if rs[i]["first"] != rs[i - 1]["last"] + 1]
            b_ok = not gaps
            print("     B. 轨间连续  %s"
                  % ("全部首尾相接 ✔" if b_ok
                     else "断点 %d 处 ✗ %s" % (len(gaps), gaps[:3])))
            ok &= b_ok

            buf = b"".join(a.read_bytes() for a in aobs)
            n = len(buf) // 2048
            pts, miss = [], 0
            for s in range(n):
                sec = buf[s * 2048:(s + 1) * 2048]
                i2 = sec.find(b"\x00\x00\x01\xBD", 4, 64)
                if i2 < 0 or not (sec[i2 + 7] & 0x80):
                    pts.append(None)
                    miss += 1
                else:
                    pts.append(parse_pts(sec[i2 + 9:i2 + 14]))
            c_ok = (miss == 0)
            print("     C. 缺 PTS 扇区 %d  %s" % (miss, "✔" if c_ok else "✗"))
            ok &= c_ok

            drops = [i for i in range(1, n)
                     if pts[i] is not None and pts[i - 1] is not None
                     and pts[i] < pts[i - 1]]
            starts = {r["first"] for r in rs if r["first"] != 0}
            not_boundary = [i for i in drops if i not in starts]
            missing = [s for s in starts if s < n and s not in drops]
            d_ok = (not not_boundary) and (not missing)
            print("     D. PTS 下降点 %d 个；落在轨边界 %s"
                  % (len(drops), "全部命中 ✔" if d_ok else "异常 ✗"))
            if not_boundary:
                print("        非轨边界的下降点: %s" % not_boundary[:8])
            if missing:
                print("        应下降但未下降的轨起点: %s" % missing[:8])
            ok &= d_ok

            vals = sorted({pts[r["first"]] for r in rs
                           if r["first"] < n and pts[r["first"]] is not None})
            print("     E. 各轨起点 PTS 取值: %s" % vals)

            summary.append((disc, g, len(rs), sect, declared,
                            len(drops), len(starts)))
        print()

    print("=" * 72)
    print("汇总:")
    print("  盘      组   轨数   AOB扇区   轨道表扇区  PTS下降  轨边界")
    for r in summary:
        print("  %-8s %-4d %4d  %9d  %9d  %7d  %6d"
              % (disc_label(r[0]), r[1], r[2], r[3], r[4], r[5], r[6]))
    print()
    print("审计结论:", "全部通过 ✔" if ok else "存在问题 ✗")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
