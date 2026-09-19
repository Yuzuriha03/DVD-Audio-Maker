#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
步骤1:扫描音源 + 专辑归一化 + 分组排序 + 解码完整性校验。

运行于 WSL 内部(使用 ffmpeg/ffprobe)。

逻辑:
1. 扫描音源目录全部 FLAC 和 M4A,用 ffprobe 读取采样率/位深/声道/发布日/曲序/标题/专辑
2. 专辑归一化:同一专辑若曲目参数不一致,以「多数采样率 + 该采样率下多数位深」为目标,
   用 soxr 重采样少数曲目(保证整张专辑在同一组内连续播放)
3. 按 (采样率, 位深) 分组,组内按发布日 + 曲序 + 标题排序
4. 解码完整性校验:对每首跑一次「只解码不落盘」的 ffmpeg(-f null -,附 astats),
   统计解码错误行 + 比对解码采样数与源声明采样数(ffmpeg 解码出错时会静默跳过并仍
   返回 0,必须显式校验,见下)
5. 生成 manifest.json 与解码完整性报告

本步骤**不产出音频中间文件** —— MLP 编码在 02_build.py 中直接对源文件进行。

关于校验:
  ALAC 等格式若源文件有问题,ffmpeg 会打印
  "Error submitting packet to decoder / invalid element channel count"
  但退出码仍为 0,单次丢 4096 采样。若不校验,MLP/ISO 会“全部成功”而实际缺失音频。

  MLP 容器**不记录时长**(ffprobe 返回 N/A),无法靠回读时长来核验,故改用 astats:
  在同一次解码中打印 "Number of samples",与「源声明时长 × 目标采样率」比对。
  判定规则:
    - 出现解码错误关键字            -> FAIL
    - 采样数缺失 > 50 ms 对应值     -> FAIL
    - 采样数差异 > 5 ms 对应值      -> WARN,照常继续
    - 读到采样数(校验手段本身失效) -> FAIL,不静默通过
  报告写入 <BUILD_DIR>/decode_report.txt
"""
import os
import sys
import json
import re
import glob
import subprocess
from collections import defaultdict

# 同目录的配置加载器与 ALAC 修复模块
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dvda_config import load as load_config          # noqa: E402

try:
    import alac_endfix
except ImportError:
    alac_endfix = None

# ---- 读取配置（config.sh / 环境变量，见 dvda_config.py） ----
CFG = load_config()
SRC = CFG.src                      # 音源（只读）
MANIFEST = CFG.manifest            # 清单输出
REPORT = CFG.report                # 解码完整性报告
ALAC_FIX_DIR = CFG.alac_fix_dir    # ALAC 修复产物（不修改原文件）
FFMPEG = CFG.ffmpeg
FFPROBE = CFG.ffprobe
ALAC_REPAIR = CFG.alac_repair      # 是否自动修复 Apple ALAC 缺 END 标记

# ---- 解码完整性校验参数 ----
# 背景:ffmpeg 在 ALAC 等解码出错时仍会返回退出码 0,并把损坏处静默跳过,
# 导致 MLP/ISO 全部“成功”但实际缺失音频(实测每次丢 4096 采样)。
# MLP 容器不存时长,无法回读,故本步骤在解码到 null 的同时用 astats 取得
# 实际解码采样数,与「源声明时长 × 目标采样率」比对。
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
SAMPLES_RE = re.compile(r"Number of samples:\s*(\d+)")
LOSS_ERROR_S = CFG.loss_error_s    # 采样数缺失超过此秒数 -> 失败
LOSS_WARN_S = CFG.loss_warn_s      # 采样数差异超过此秒数 -> 警告


def ffprobe_meta(path):
    """用 ffprobe 提取音频参数与标签,返回 dict。

    额外读取源文件「声明的时长」dur(秒),用于后续解码完整性校验:
    ffmpeg 解码出错时会静默丢帧但仍返回 0,只能靠时长差识别。
    """
    out = subprocess.run(
        [FFPROBE, "-v", "error", "-select_streams", "a:0",
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
            # 键名→小写：Vorbis 注释的字段名**大小写不敏感**，而 MP4 标签习惯
            # 小写、FLAC 习惯大写（如 ALBUM vs album）。若按原样精确匹配，
            # 同一个逻辑标签会因来源不同而读不到（曾把整张专辑的 album 读成空，
            # 导致专辑被拆散、归一化静默跳过）。
            tags[k.lower()] = v

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

    d["date"] = tags.get("date") or tags.get("releasetime") or ""
    d["track"] = tags.get("track", "")
    d["title"] = tags.get("title", os.path.splitext(os.path.basename(path))[0])
    d["album"] = tags.get("album", "")
    return d


def decode_check(path, resample_to=None):
    """只解码不落盘;返回 (采样数, 解码错误行数, 前 3 条错误原文)。

    用 -f null - 丢弃输出(不写盘),并复用 MLP 编码将要使用的同一套滤镜链
    (重采样),使校验对象与实际编码对象一致。

    astats 在解码结束时打印 "Number of samples:<N>"。各声道数值相同,
    取第一处匹配即可。取不到时返回 None —— 由调用方判为失败,
    避免「校验手段失效 = 静默通过」。
    """
    filters = []
    if resample_to:
        filters.append(f"aresample={resample_to}:resampler=soxr")
    filters.append("astats=metadata=1")
    cmd = [FFMPEG, "-hide_banner", "-nostdin", "-v", "info", "-y",
           "-i", path, "-af", ",".join(filters), "-f", "null", "-"]
    r = subprocess.run(cmd, capture_output=True, text=True,
                       encoding="utf-8", errors="replace")
    err_n, err_lines = scan_decode_errors(r.stderr)
    m = SAMPLES_RE.search(r.stderr)
    samples = int(m.group(1)) if m else None
    if r.returncode != 0 and err_n == 0:
        err_n = 1
        err_lines = [f"ffmpeg 退出码 {r.returncode}"]
    return samples, err_n, err_lines


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


# ALAC 未压缩帧缺 END 标记的全部报错特征
alac_endfix_importable = alac_endfix is not None


def try_alac_repair(path, expected, src_rate=None):
    """尝试修复 Apple ALAC 缺失 END 标记的问题。

    `expected` 是**重采样后**的期望采样数（用于主校验）。
    `src_rate` 是源文件采样率；修复效果的初步检验必须按源采样率换算
    （用目标采样率算会误判：如 44100→48000 的源，修复后按 48000 算会
    得到「采样数不足」的错误结论）。

    成功时返回 (修复后路径, 补帧数, 逐帧说明)；不适用或未改善时返回
    (None, 0, [])。原文件不做任何修改，修复产物写入 ALAC_FIX_DIR。

    可用 config.sh 的 DVDA_ALAC_REPAIR=0 关闭（届时仅报告，不修复）。
    """
    if alac_endfix is None or not ALAC_REPAIR:
        return None, 0, []
    if not path.lower().endswith((".m4a", ".mp4", ".alac")):
        return None, 0, []
    try:
        bad, cookie = alac_endfix.find_bad_frames(path)
    except Exception as e:                      # 非 ALAC / 解析失败
        print(f"    [ALAC 修复] 跳过（{e}）")
        return None, 0, []
    if not bad:
        print("    [ALAC 修复] 无缺失 END 标记的帧")
        return None, 0, []

    os.makedirs(ALAC_FIX_DIR, exist_ok=True)
    out = os.path.join(ALAC_FIX_DIR, os.path.basename(path))
    try:
        r = alac_endfix.repair(path, out, verbose=False)
    except Exception as e:
        print(f"    [ALAC 修复] 失败: {e}")
        return None, 0, []
    print(f"    [ALAC 修复] 补回 {r['bad']} 帧的 END 标记 -> {out}")
    detail = []
    for b in r["patched"]:
        mm, ss = divmod(b["pts"], 60)
        line = (f"{b['pts']:>10.3f}s ({int(mm)}分{ss:05.2f}秒)  "
                f"标记 {b['cur_bits']:03b} -> 111  ({b['n_samples']} 采样)")
        detail.append(line)
        print(f"       {line}")

    # 验证修复效果：以【源采样率】为准（重采样由后续环节负责）
    n2, e2 = alac_endfix.decode_samples(out)
    rate = src_rate or cookie.get("sample_rate")
    decl = alac_endfix.declared_samples(out)
    if e2 == 0 and (decl is None or n2 == decl):
        return out, r["bad"], detail
    print(f"    [ALAC 修复] 未完全修复（采样 {n2}, 声明 {decl}, 报错 {e2} 行）")
    return None, 0, []


def track_num(t):
    """'1/5' -> 1"""
    m = re.match(r"\s*(\d+)", t or "")
    return int(m.group(1)) if m else 9999


def main():
    # 移除旧 manifest:本步骤若因校验失败而中止,残留的旧 manifest 会
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
    #    先记录源采样率/位深：ALAC 修复效果的初步检验需按源采样率换算
    for t in tracks:
        t["_src_sr"] = t["sr"]
        t["_src_bits"] = t["bits"]
        if t["path"] in resample:
            t["sr"], t["bits"] = resample[t["path"]]

    groups = defaultdict(list)
    for t in tracks:
        groups[(t["sr"], t["bits"])].append(t)

    # 3b. 组内声道数必须一致
    # DVD-Audio 同一音频组内所有曲目须为相同参数。采样率/位深已由归一化处理,
    # 但声道数目前不做转换(单声道 vs 立体声不可无损互转)。若出现混杂,
    # dvda-author 会产出一个参数不一致的非法音频组,必须在此拦下。
    chan_issues = []
    for (sr, bits), items in sorted(groups.items()):
        chans = defaultdict(list)
        for t in items:
            chans[t["ch"]].append(t)
        if len(chans) > 1:
            desc = "; ".join(f"{c} 声道 × {len(v)} 首"
                             for c, v in sorted(chans.items()))
            chan_issues.append({
                "level": "FAIL",
                "title": f"音频组 {sr}Hz/{bits}bit 声道数不一致",
                "path": SRC,
                "reason": desc,
                "detail": [f"     {t['title']}" for t in items][:10],
            })
            print(f"  !! [FAIL] 组 {sr}/{bits} 声道数不一致: {desc}")

    # 4. 解码完整性校验(只解码不落盘) + 组装 manifest
    issues = list(chan_issues)   # 校验发现的问题
    checked = 0
    manifest = {}
    for (sr, bits), items in sorted(groups.items()):
        items.sort(key=lambda x: (x["date"], track_num(x["track"]), x["title"]))
        gname = f"group_{sr}_{bits}"
        files = []
        for i, t in enumerate(items, 1):
            # 用 splitext 去掉扩展名(兼容 .flac / .m4a 等任意长度扩展名)
            safe = re.sub(r'[<>:"/\\|?*]', "_",
                          os.path.splitext(os.path.basename(t["path"]))[0])
            # MLP 缓存键由「组名 / 序号 / 曲名」组成；02_build.py 据此命名缓存文件
            name = f"{gname}/{i:04d}__{safe}"
            rto = sr if t["path"] in resample else None

            # ---- 解码完整性校验 ----
            # ffmpeg 解码出错时退出码仍为 0,需检查错误行与解码采样数
            checked += 1
            expected = round(t["dur"] * sr) if t.get("dur") else None
            samples, err_n, err_lines = decode_check(t["path"], rto)
            repaired = 0

            # 解码异常时先尝试 ALAC END 标记修复（Apple 编码器特征问题）
            # 实测:原文件完全正常,只是帧尾 3 位终结标记被写成 000,
            # ffmpeg 会误判成 SCE 元素并丢掉整帧。修复后采样数精确达标。
            if err_n > 0 or (samples is not None and expected
                             and samples != expected):
                fixed, repaired, rdetail = try_alac_repair(
                    t["path"], expected, src_rate=t.get("_src_sr"))
                if fixed:
                    samples, err_n, err_lines = decode_check(fixed, rto)
                    t["_repaired_from"] = t["path"]
                    t["_repair_detail"] = rdetail
                    t["path"] = fixed

            loss_ms = None
            if samples is not None and expected:
                loss_ms = (expected - samples) / sr * 1000.0

            level, reasons, detail = None, [], []
            if err_n > 0:
                level = "FAIL"
                reasons.append(f"解码报错 {err_n} 处")
                detail += err_lines
            if samples is None:
                # 校验手段本身失效,不允许静默通过
                level = "FAIL"
                reasons.append("未能读到解码采样数(astats 无输出)")
                detail.append("ffmpeg 未输出 'Number of samples',无法校验完整性")
            else:
                detail.append(
                    "源声明 %.3f 秒 → 期望 %s 采样 / 实解 %d 采样 / %s"
                    % (t["dur"], expected if expected else "?",
                       samples,
                       "?" if loss_ms is None else "%s %.0f ms(%+d 采样)"
                       % ("少" if loss_ms > 0 else "多",
                          abs(loss_ms), samples - expected)))
                if loss_ms is not None:
                    if abs(loss_ms) > LOSS_ERROR_S * 1000:
                        level = "FAIL"
                        reasons.append("解码采样数%s %.0f ms(%+d 采样)"
                                       % ("少" if loss_ms > 0 else "多",
                                          abs(loss_ms), samples - expected))
                    elif abs(loss_ms) > LOSS_WARN_S * 1000 and level is None:
                        level = "WARN"
                        reasons.append("解码采样数%s %.0f ms"
                                       % ("少" if loss_ms > 0 else "多",
                                          abs(loss_ms)))
            if repaired:
                detail.insert(0, "已修复 ALAC END 标记 %d 处（原文件未改动）" % repaired)

            if level:
                issues.append({"level": level, "title": t["title"],
                               "path": t["path"],
                               "reason": "; ".join(reasons), "detail": detail})
                mark = "!!" if level == "FAIL" else " ?"
                print(f"  {mark} [{level}] {t['title']}: {'; '.join(reasons)}")
            elif repaired:
                print(f"  ++ [已修复] {t['title']}: ALAC END 标记 {repaired} 处,"
                      f"解码采样数已达标")

            files.append({"n": i, "src": t["path"], "name": name,
                          "title": t["title"], "date": t["date"],
                          "track": t["track"], "album": t["album"],
                          "dur": round(t["dur"], 6),
                          "resample_to": rto,
                          "repaired": repaired,
                          "repair_detail": t.get("_repair_detail"),
                          "orig_src": t.get("_repaired_from")})
        manifest[gname] = {"sr": sr, "bits": bits, "count": len(files), "files": files}
        print(f"{gname}: {len(files)} 首")

    # 5. 解码完整性报告
    fails = [x for x in issues if x["level"] == "FAIL"]
    warns = [x for x in issues if x["level"] == "WARN"]
    # 收集修复记录（按曲目）
    repairs = []
    for _gname, g in manifest.items():
        for f in g["files"]:
            if f.get("repaired"):
                repairs.append({"title": f["title"], "n": f["repaired"],
                                "orig": f.get("orig_src") or f["src"],
                                "fixed": f["src"],
                                "frames": f.get("repair_detail") or []})
    lines = []
    lines.append("音源校验报告")
    lines.append("=" * 68)
    lines.append(f"已校验 {checked} 首；失败 {len(fails)} 首，警告 {len(warns)} 首")
    lines.append("")
    if not issues:
        lines.append("全部通过：无解码错误，解码采样数与源声明一致，组内参数一致。")
    for x in fails + warns:
        lines.append(f"[{x['level']}] {x['title']}")
        lines.append(f"    原因: {x['reason']}")
        lines.append(f"    源文件: {x['path']}")
        for d in x["detail"]:
            lines.append(f"    {d}")
        lines.append("")

    # ALAC END 标记修复记录
    if repairs:
        lines.append("")
        lines.append("-" * 68)
        lines.append("ALAC END 标记修复记录")
        lines.append("-" * 68)
        lines.append("说明：Apple 编码器产出的 ALAC 中，周期性插入的「未压缩帧」"
                     "缺少帧尾")
        lines.append("      END 终结标记（应为 111，实际为其他值），导致 ffmpeg "
                     "误判为")
        lines.append("      SCE 元素而丢弃整帧。原文件音频数据完好，仅补写该 3 位。")
        lines.append("")
        for r in repairs:
            lines.append(f"[已修复 {r['n']} 帧] {r['title']}")
            lines.append(f"    原文件: {r['orig']}")
            lines.append(f"    修复后: {r['fixed']}")
            for d in r["frames"]:
                lines.append(f"    {d}")
            lines.append("")

    os.makedirs(os.path.dirname(REPORT), exist_ok=True)
    with open(REPORT, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")

    print()
    print("=" * 68)
    print("音源校验（解码完整性 + 组内参数一致性）")
    print("=" * 68)
    print(f"  已校验 {checked} 首；失败 {len(fails)} 首，警告 {len(warns)} 首")
    if repairs:
        print(f"  已自动修复 ALAC END 标记: {len(repairs)} 首"
              f"（共 {sum(r['n'] for r in repairs)} 帧）")
    if not issues:
        print("  全部通过：无解码错误，采样数与源声明一致，组内参数一致")
    for x in fails + warns:
        print(f"  [{x['level']}] {x['title']}")
        print(f"         {x['reason']}")
        print(f"         源: {x['path']}")
        for d in x["detail"][:4]:
            print(f"         {d}")
    print(f"  报告已写入: {REPORT}")

    if fails:
        print()
        print(f"[停止] 发现 {len(fails)} 个失败项（音源损坏 / 解码异常 / 组内参数不一致）。")
        print("       未生成 manifest.json；请修复音源后重试。")
        sys.exit(1)

    # 6. 写 manifest
    os.makedirs(os.path.dirname(MANIFEST), exist_ok=True)
    with open(MANIFEST, "w", encoding="utf-8") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2)
    print(f"manifest.json 已生成 -> {MANIFEST}")
    print("总计", sum(len(g["files"]) for g in manifest.values()), "首")


if __name__ == "__main__":
    main()
