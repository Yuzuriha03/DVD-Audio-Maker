#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
步骤1:FLAC/M4A → WAV 转换 + 专辑归一化 + 分组排序。

运行于 WSL 内部(使用 WSL 的 ffmpeg/ffprobe),全部中间文件写入 ext4 提速。

逻辑:
1. 扫描音源目录全部 FLAC 和 M4A,用 ffprobe 读取采样率/位深/声道/发布日/曲序/标题/专辑
2. 专辑归一化:同一专辑若曲目参数不一致,以「多数采样率 + 该采样率下多数位深」为目标,
   用 soxr 重采样少数曲目(保证整张专辑在同一组内连续播放)
3. 按 (采样率, 位深) 分组,组内按发布日 + 曲序 + 标题排序
4. 用 ffmpeg 转 WAV(-map_metadata -1 去掉 LIST 块,dvda-author 旧解析器需要)
5. 解码完整性校验:统计解码错误行 + 比对输出时长与源声明时长
   (ffmpeg 解码出错时会静默跳过并仍返回 0,必须显式校验,见下)
6. 生成 manifest.json 与解码完整性报告

关于校验:
  ALAC 等格式若源文件损坏,ffmpeg 会打印
  "Error submitting packet to decoder / invalid element channel count"
  但退出码仍为 0,单次丢 4096 采样。若不校验,WAV/MLP/ISO 会“全部成功”
  而实际缺失音频。故本脚本:
    - 时长缺失 > 50 ms 或出现解码错误 -> FAIL,不生成 manifest 并以非零码退出
    - 时长差异 > 5 ms                   -> WARN,照常继续
  报告写入 /root/dvda-build/decode_report.txt
"""
import os
import sys
import json
import re
import glob
import shutil
import subprocess
from collections import defaultdict

# ---- 路径配置 ----
# 下列路径均可用同名环境变量覆盖（详见 README「路径配置」）：
#   DVDA_SRC  DVDA_WORK  DVDA_MANIFEST  DVDA_REPORT
SRC = os.environ.get("DVDA_SRC", "/mnt/c/Users/yyz57/Music/鸣潮先约电台")                # 音源(只读)
WORK = os.environ.get("DVDA_WORK", "/root/dvda-build/wav").rstrip("/")                    # WAV 工作目录
MANIFEST = os.environ.get("DVDA_MANIFEST", "/root/dvda-build/manifest.json")              # 清单输出
REPORT = os.environ.get("DVDA_REPORT", "/root/dvda-build/decode_report.txt")              # 解码完整性报告

# ---- 解码完整性校验参数 ----
# 背景:ffmpeg 在 ALAC 等解码出错时仍会返回退出码 0,并把损坏处静默跳过,
# 导致 WAV/MLP/ISO 全部“成功”但实际缺失音频(实测每次丢 4096 采样)。
# 故此处按「解码错误行数 + 输出时长与源声明时长之差」双重判定。
DECODE_ERR_KEYWORDS = (
    "error submitting packet to decoder",
    "invalid element",
    "error while decoding",
    "invalid data found",
    "channel element",
    "crc mismatch",
    "corrupt",
    "not implemented",
    "not yet implemented",
)
LOSS_ERROR_S = 0.05    # 时长缺失超过 50 ms -> 失败
LOSS_WARN_S = 0.005    # 时长差异超过 5 ms  -> 警告


def ffprobe_meta(path):
    """用 ffprobe 提取音频参数与标签,返回 dict。

    额外读取源文件「声明的时长」dur(秒),用于后续解码完整性校验:
    ffmpeg 解码出错时会静默丢帧但仍返回 0,只能靠时长差识别。
    """
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "a:0",
         "-show_entries",
         "stream=sample_rate,bits_per_raw_sample,channels,duration_ts,time_base",
         "-show_entries", "format=duration",
         "-show_entries", "format_tags", "-of", "default=noprint_wrappers=1", path],
        capture_output=True, text=True, encoding="utf-8", errors="replace").stdout
    d = {"sr": 0, "bits": 0, "ch": 0, "date": "", "track": "",
         "title": "", "album": "", "dur": 0.0}
    tags = {}
    dts = tb = None
    fmt_dur = None
    for line in out.splitlines():
        line = line.strip()
        if line.startswith("sample_rate="):
            d["sr"] = int(line.split("=", 1)[1] or 0)
        elif line.startswith("bits_per_raw_sample="):
            d["bits"] = int(line.split("=", 1)[1] or 0)
        elif line.startswith("channels="):
            d["ch"] = int(line.split("=", 1)[1] or 0)
        elif line.startswith("duration_ts="):
            try:
                dts = int(line.split("=", 1)[1])
            except ValueError:
                pass
        elif line.startswith("time_base="):
            tb = line.split("=", 1)[1]
        elif line.startswith("duration=") and not line.startswith("TAG:"):
            try:
                fmt_dur = float(line.split("=", 1)[1])
            except ValueError:
                pass
        elif line.startswith("TAG:") and "=" in line:
            k, v = line[4:].split("=", 1)
            tags[k] = v

    # 优先用流级 duration_ts * time_base,回退到容器 duration
    if dts and tb and "/" in tb:
        try:
            num, den = tb.split("/")
            if int(den):
                d["dur"] = dts * int(num) / int(den)
        except (ValueError, ZeroDivisionError):
            pass
    if not d["dur"] and fmt_dur:
        d["dur"] = fmt_dur

    d["date"] = tags.get("date") or tags.get("RELEASETIME") or ""
    d["track"] = tags.get("track", "")
    d["title"] = tags.get("title", os.path.splitext(os.path.basename(path))[0])
    d["album"] = tags.get("album", "")
    return d


def probe_duration(path):
    """读取已完成文件的时长(秒);失败返回 None。"""
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration",
         "-of", "default=nw=1:nk=1", path],
        capture_output=True, text=True, encoding="utf-8", errors="replace").stdout
    try:
        return float(out.strip())
    except ValueError:
        return None


def scan_decode_errors(stderr):
    """统计 ffmpeg stderr 中的解码错误行数,并返回前 3 条原文。"""
    n = 0
    samples = []
    for line in stderr.splitlines():
        low = line.lower()
        if any(k in low for k in DECODE_ERR_KEYWORDS):
            n += 1
            if len(samples) < 3:
                samples.append(line.strip())
    return n, samples


def track_num(t):
    """'1/5' -> 1"""
    m = re.match(r"\s*(\d+)", t or "")
    return int(m.group(1)) if m else 9999


def main():
    # 清空旧的 WAV 工作目录,避免累积已被删除专辑的残留
    if os.path.exists(WORK):
        shutil.rmtree(WORK)
    os.makedirs(WORK, exist_ok=True)
    print(f"[清理] WAV 工作目录已清空重建: {WORK}")

    # 同时移除旧 manifest:本步骤若因校验失败而中止,残留的旧 manifest 会
    # 让 02_build.py 仍能出盘,从而掩盖问题。
    if os.path.exists(MANIFEST):
        os.remove(MANIFEST)
        print(f"[清理] 旧 manifest 已移除: {MANIFEST}")

    # 扫描 FLAC 和 M4A(AAC/ALAC 等)
    sources = sorted(
        glob.glob(os.path.join(SRC, "**", "*.flac"), recursive=True)
        + glob.glob(os.path.join(SRC, "**", "*.m4a"), recursive=True)
    )
    print(f"发现 {len(sources)} 个音频文件 (FLAC/M4A)")

    # 1. 读取元数据
    tracks = []
    for p in sources:
        m = ffprobe_meta(p)
        m["path"] = p
        tracks.append(m)

    # 2. 专辑归一化
    albums = defaultdict(list)
    for t in tracks:
        albums[t["album"] or t["title"]].append(t)

    resample = {}  # path -> (target_sr, target_bits)
    for alb, trks in albums.items():
        if len(trks) < 2:
            continue
        combos = defaultdict(int)
        for t in trks:
            combos[(t["sr"], t["bits"])] += 1
        if len(combos) == 1:
            continue  # 参数一致,无需处理
        sr_count = defaultdict(int)
        for t in trks:
            sr_count[t["sr"]] += 1
        majority_sr = max(sr_count, key=sr_count.get)
        bits_count = defaultdict(int)
        for t in trks:
            if t["sr"] == majority_sr:
                bits_count[t["bits"]] += 1
        majority_bits = max(bits_count, key=bits_count.get)
        for t in trks:
            if (t["sr"], t["bits"]) != (majority_sr, majority_bits):
                resample[t["path"]] = (majority_sr, majority_bits)
                print(f"  [重采样] {t['title']}: {t['sr']}/{t['bits']} -> {majority_sr}/{majority_bits}")

    print(f"共需重采样 {len(resample)} 首")

    # 3. 应用重采样目标参数并分组
    for t in tracks:
        if t["path"] in resample:
            t["sr"], t["bits"] = resample[t["path"]]

    groups = defaultdict(list)
    for t in tracks:
        groups[(t["sr"], t["bits"])].append(t)

    # 4. 转换 WAV(含解码完整性校验)
    issues = []      # 校验发现的问题
    checked = 0
    manifest = {}
    for (sr, bits), items in sorted(groups.items()):
        items.sort(key=lambda x: (x["date"], track_num(x["track"]), x["title"]))
        gname = f"group_{sr}_{bits}"
        gdir = os.path.join(WORK, gname)
        os.makedirs(gdir, exist_ok=True)
        codec = "pcm_s16le" if bits == 16 else "pcm_s24le"
        files = []
        for i, t in enumerate(items, 1):
            # 用 splitext 去掉扩展名(兼容 .flac / .m4a 等任意长度扩展名)
            safe = re.sub(r'[<>:"/\\|?*]', "_", os.path.splitext(os.path.basename(t["path"]))[0])
            outpath = os.path.join(gdir, f"{i:04d}__{safe}.wav")
            cmd = ["ffmpeg", "-y", "-v", "error", "-i", t["path"]]
            if t["path"] in resample:
                cmd += ["-af", f"aresample={sr}:resampler=soxr"]
            cmd += ["-map_metadata", "-1", "-c:a", codec, outpath]
            r = subprocess.run(cmd, capture_output=True, text=True,
                               encoding="utf-8", errors="replace")
            if r.returncode != 0:
                print(f"[FAIL] {t['title']}: {r.stderr[:200]}")
                issues.append({"level": "FAIL", "title": t["title"],
                               "path": t["path"], "reason": "ffmpeg 转换失败",
                               "detail": [r.stderr.strip()[:300]]})
                continue

            # ---- 解码完整性校验 ----
            # ffmpeg 解码出错时退出码仍为 0,需检查错误行与时长差
            checked += 1
            err_n, err_lines = scan_decode_errors(r.stderr)
            actual = probe_duration(outpath)
            loss = None
            if actual is not None and t.get("dur"):
                loss = t["dur"] - actual

            level, reasons, detail = None, [], []
            if err_n > 0:
                level = "FAIL"
                reasons.append(f"解码报错 {err_n} 处")
                detail += err_lines
            if loss is not None:
                detail.append("源声明 %.3f 秒 / 实际 %.3f 秒 / 差 %+.0f ms"
                              % (t["dur"], actual, loss * 1000))
                if abs(loss) > LOSS_ERROR_S:
                    level = "FAIL"
                    reasons.append(f"时长缺失 {loss * 1000:+.0f} ms")
                elif abs(loss) > LOSS_WARN_S and level is None:
                    level = "WARN"
                    reasons.append(f"时长差异 {loss * 1000:+.0f} ms")

            if level:
                issues.append({"level": level, "title": t["title"],
                               "path": t["path"],
                               "reason": "; ".join(reasons), "detail": detail})
                mark = "!!" if level == "FAIL" else " ?"
                print(f"  {mark} [{level}] {t['title']}: {'; '.join(reasons)}")

            files.append({"n": i, "wav": outpath, "title": t["title"],
                          "date": t["date"], "track": t["track"], "album": t["album"]})
        manifest[gname] = {"sr": sr, "bits": bits, "count": len(files), "files": files}
        print(f"{gname}: {len(files)} 首")

    # 5. 解码完整性报告
    fails = [x for x in issues if x["level"] == "FAIL"]
    warns = [x for x in issues if x["level"] == "WARN"]
    lines = []
    lines.append("解码完整性校验报告")
    lines.append("=" * 68)
    lines.append(f"已校验 {checked} 首；失败 {len(fails)} 首，警告 {len(warns)} 首")
    lines.append("")
    if not issues:
        lines.append("全部通过：无解码错误，输出时长与源声明一致。")
    for x in fails + warns:
        lines.append(f"[{x['level']}] {x['title']}")
        lines.append(f"    原因: {x['reason']}")
        lines.append(f"    源文件: {x['path']}")
        for d in x["detail"]:
            lines.append(f"    {d}")
        lines.append("")

    os.makedirs(os.path.dirname(REPORT), exist_ok=True)
    with open(REPORT, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")

    print()
    print("=" * 68)
    print("解码完整性校验")
    print("=" * 68)
    print(f"  已校验 {checked} 首；失败 {len(fails)} 首，警告 {len(warns)} 首")
    if not issues:
        print("  全部通过：无解码错误，时长与源一致")
    for x in fails + warns:
        print(f"  [{x['level']}] {x['title']}")
        print(f"         {x['reason']}")
        print(f"         源: {x['path']}")
        for d in x["detail"][:4]:
            print(f"         {d}")
    print(f"  报告已写入: {REPORT}")

    if fails:
        print()
        print(f"[停止] {len(fails)} 首音源解码失败(源文件损坏或格式不受支持)。")
        print("       未生成 manifest.json；请更换音源后重试。")
        sys.exit(1)

    # 6. 写 manifest
    os.makedirs(os.path.dirname(MANIFEST), exist_ok=True)
    with open(MANIFEST, "w", encoding="utf-8") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2)
    print(f"manifest.json 已生成 -> {MANIFEST}")
    print("总计", sum(len(g["files"]) for g in manifest.values()), "首")


if __name__ == "__main__":
    main()
