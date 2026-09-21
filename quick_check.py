#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""快速结构校验：不用解开 AOB，几秒出结果。

只做三件事（都只读盘上很小的一部分）：

  1. 构建日志里不得出现 pack 补齐失败的记录
     dvda-author 的 write_pes_padding() 在需要补 1~6 字节时只报错不写，会让该轨
     最后一个 pack 短几字节、下一轨的 pack 头落进扇区中间，读盘端会把那一首整个
     丢掉。日志里搜 'pes_padding length must be higher'，命中即判失败。

  2. 每个音频组 IFO 里声明的轨数之和 == 音源曲目数
     按 foo_input_dvda 的解析方式读 ATS_xx_0.IFO：ats_pgcit 指向 PGCI 所在扇区，
     其中 nr_of_titles 与各 title 的 tracks。

  3. 每轨首扇区必须以 pack 头 00 00 01 BA 开头
     直接按 IFO 给的扇区号去 ISO 里 dd 那一个扇区，逐个确认。

IFO 与单个扇区都极小，全程只读几百 KB，故可在每次出盘后随手跑。

用法:
    python3 quick_check.py [ISO 目录] [构建日志]
默认取自 config.sh。
"""
import json
import os
import re
import struct
import subprocess
import sys
import pathlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from dvda_config import load as load_config          # noqa: E402

CFG = load_config(need=None, quiet=True)
ISO_DIR = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else CFG.final_dir)
LOG = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else pathlib.Path(CFG.build_log)
BUILD_DIR = pathlib.Path(CFG.build_dir)
MANIFEST = BUILD_DIR / "manifest.json"

ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
PACK = b"\x00\x00\x01\xBA"


def u16(b, o):
    return struct.unpack(">H", b[o:o + 2])[0]


def u32(b, o):
    return struct.unpack(">I", b[o:o + 4])[0]


def iso_read(path, lba, nsec):
    """按扇区读 ISO 里的一小段（不挂载、不解整盘）。"""
    r = subprocess.run(["dd", "if=%s" % path, "bs=2048",
                        "skip=%d" % lba, "count=%d" % nsec, "status=none"],
                       capture_output=True)
    return r.stdout


def iso_lba(path, name):
    """用 xorriso 查某个文件的起始 LBA 与块数。"""
    r = subprocess.run(["xorriso", "-indev", str(path), "-find", "/AUDIO_TS",
                        "-name", name, "-exec", "report_lba"],
                       capture_output=True, text=True)
    for line in (r.stdout or "").splitlines():
        if "File data lba:" in line:
            f = [x.strip() for x in line.split(",")]
            try:
                return int(f[1]), int(f[2])
            except (IndexError, ValueError):
                return None, None
    return None, None


def check_log(ok):
    print("[1] 构建日志不得有 pack 补齐失败")
    if not LOG.exists():
        print("    (未找到 %s，跳过)" % LOG)
        return ok
    t = ANSI.sub("", LOG.read_text(encoding="utf-8", errors="replace"))
    hits = [l for l in t.splitlines() if "pes_padding length must be higher" in l]
    if hits:
        print("    ✗ 出现 %d 次 —— 至少一轨的 pack 未补齐到扇区边界，"
              "读盘端会丢掉紧随其后的那一首" % len(hits))
        for h in hits[:3]:
            print("        %s" % h.strip()[:110])
        return False
    print("    无 ✔")
    return ok


def main():
    ok = True
    print("=================== 快速结构校验 ===================")
    print("ISO 目录: %s" % ISO_DIR)
    print("构建日志: %s" % LOG)
    print()

    ok = check_log(ok)

    # 期望曲目数（来自 manifest）
    want = None
    if MANIFEST.exists():
        m = json.load(open(MANIFEST, encoding="utf-8"))
        want = sum(len(g["files"]) for g in m.values() if isinstance(g, dict))
    print()
    print("[2] IFO 声明轨数 / [3] 每轨首扇区是 pack 头")
    print("    期望曲目数: %s" % (want if want is not None else "?"))
    print()

    isos = sorted(ISO_DIR.glob("*.iso"))
    if not isos:
        print("    (未找到 ISO)")
        return 1

    total = 0
    for iso in isos:
        print("### %s  (%d 字节)" % (iso.name, iso.stat().st_size))
        # 列出 AUDIO_TS 下的 IFO 与 AOB
        r = subprocess.run(["xorriso", "-indev", str(iso), "-find", "/AUDIO_TS",
                            "-name", "*.IFO", "-exec", "report_lba"],
                           capture_output=True, text=True)
        ifos = {}
        for line in (r.stdout or "").splitlines():
            mm = re.search(r"(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*'([^']+)'", line)
            if mm:
                ifos[os.path.basename(mm.group(4))] = (int(mm.group(1)),
                                                       int(mm.group(2)))
        ifos = {k: v for k, v in ifos.items()
                if re.match(r"ATS_\d+_0\.IFO$", k) and "BUP" not in k}
        if not ifos:
            print("    ✗ 未找到 ATS_xx_0.IFO")
            ok = False
            continue

        group_tracks = {}
        group_titles = {}
        still_refs = {}
        all_bounds = []
        for name in sorted(ifos):
            lba, _n = ifos[name]
            g = int(name[4:6])
            # IFO 只有 2~3 个扇区，读 8 个足够
            b = iso_read(iso, lba, 8)
            p = u32(b, 204) * 2048
            if p + 8 > len(b):
                print("    ✗ %s: ats_pgcit 指向扇区 %d 越界" % (name, p // 2048))
                ok = False
                continue
            pgc = b[p:p + u32(b, p + 4) + 1]
            cnt = u16(pgc, 0)
            tr = 0
            for i in range(cnt):
                tto = u32(pgc, 8 + i * 8 + 4)
                if tto + 16 > len(pgc):
                    print("    ✗ %s: title %d 越界" % (name, i))
                    ok = False
                    break
                n = pgc[tto + 2]
                tr += n
                sr = tto + u16(pgc, tto + 12)
                for j in range(n):
                    if sr + j * 12 + 12 > len(pgc):
                        break
                    all_bounds.append(u32(pgc, sr + j * 12 + 4))

                # 一个 title（PGC）内的**时间轴必须连续**：所有 cell 的
                # first_pts 严格递增，且末 cell 的结束对得上标题长度
                # len_in_pts。违反时播放器按时间轴寻址任何一首都会落到 PTS
                # 起点 = 第 1 首（症状：不管哪首按「下一曲」都跳回曲目 1）。
                if n > 1:
                    fps = []
                    for j in range(n):
                        o = tto + 16 + 20 * j
                        if o + 20 > len(pgc):
                            break
                        fps.append((u32(pgc, o + 6), u32(pgc, o + 10)))
                    tl = u32(pgc, tto + 4)
                    if len(fps) != n:
                        print("    ✗ %s: title %d 的 cell 时间戳表越界"
                              % (name, i + 1))
                        ok = False
                    elif not all(fps[k][0] < fps[k + 1][0]
                                 for k in range(n - 1)):
                        print("    ✗ %s: title %d 的 PGC 时间轴不连续"
                              "（%d 个 cell 的 first_pts 非递增）"
                              % (name, i + 1, n))
                        print("       后果: 播放器按时间轴寻址任何一首都会落到"
                              "第 1 首")
                        print("       成因: MLP 的 pts[] 是按轨的，合并成一个"
                              " title 后没加累计偏移")
                        print("             （见 patches/patch_mlp_one_title.py）")
                        ok = False
                    elif abs(fps[-1][0] + fps[-1][1] - tl) > 90000:
                        print("    ✗ %s: title %d 末 cell 结束 %d 与标题长度 %d"
                              " 相差超过 1 秒"
                              % (name, i + 1, fps[-1][0] + fps[-1][1], tl))
                        ok = False
                    else:
                        print("    [OK] %s: title %d 时间轴连续（%d 个 cell，"
                              "PTS %d..%d）"
                              % (name, i + 1, n, fps[0][0],
                                 fps[-1][0] + fps[-1][1]))
            group_tracks[g] = tr
            group_titles[g] = cnt
            # ATSI 静图记录引用的「ASVS 记录号」最大值。它必须 <= ASVS 的记录
            # 条数，否则播放器按越界记录号取封面 → **直接崩**（真机上「跳到
            # 后面的曲目闪退」就是这个）。这项检查就是为了在出盘前拦住它。
            max_still = 0
            for i in range(cnt):
                tto = u32(pgc, 8 + i * 8 + 4)
                if tto + 16 > len(pgc):
                    break
                picptr = u16(pgc, tto + 14)
                if not picptr:
                    continue
                for t in range(pgc[tto + 2]):
                    o = tto + picptr + 6 * t
                    if o + 6 > len(pgc):
                        break
                    if pgc[o] > max_still:
                        max_still = pgc[o]
            still_refs[g] = max_still or None
            print("    组%d: %2d 轨, %d 个 title  (%s)%s"
                  % (g, tr, cnt, name,
                     "，静图最大引用号 %d" % max_still if max_still else ""))

        # 「下一段 / 上一段」是在**同一个 title 内**换轨，所以只要一组里有
        # 多个 title，在 title 边界上按「下一段」就会停住（上游原本因
        # 「MLP 不能无缝接轨」的猜测让每轨自成 title，见
        # patches/patch_mlp_one_title.py）。这里把结构报出来，>1 就提示。
        multi = {g: (t, group_tracks[g]) for g, t in group_titles.items() if t > 1}
        if multi:
            print("    [提示] 有 %d 个组的 title 数 > 1：%s"
                  % (len(multi),
                     "、".join("组%d(%d title/%d 轨)" % (g, t, n)
                               for g, (t, n) in sorted(multi.items()))))
            print("           组内有多个 title 时，「下一段」在 title 边界不会继续，")
            print("           可能是组内音频属性中途变化（正常），"
                  "也可能是 patch_mlp_one_title 未生效。")
        else:
            print("    [OK]   每个组都是单 title（「下一段」可在组内逐轨前进）")

        # 静图引用号 vs ASVS 记录条数：必须引用值 <= 条数，否则越界取图 → 崩溃
        refs = [v for v in still_refs.values() if v]
        if refs:
            sv_lba, _ = iso_lba(iso, "AUDIO_SV.IFO")
            if sv_lba is not None:
                sv = iso_read(iso, sv_lba, 2)
                if len(sv) >= 0xE:
                    sv_n = u16(sv, 0xC)
                    mx = max(refs)
                    if mx <= sv_n:
                        print("    [OK] 静图引用号 %d <= AUDIO_SV.IFO 记录数 "
                              "%d ✔" % (mx, sv_n))
                    else:
                        print("    ✗ 静图引用号 %d > AUDIO_SV.IFO 记录数 %d"
                              % (mx, sv_n))
                        print("       后果: 播放器按越界记录号取封面 → "
                              "**崩溃**（跳到后面的曲目时闪退）")
                        print("       成因: ATSI 与 ASVS 的「按 title / 按轨」"
                              "不一致，两者必须成对改")
                        print("             （见 patches/patch_asvs_per_track.py）")
                        ok = False
        n_tr = sum(group_tracks.values())
        total += n_tr
        print("    合计 %d 轨" % n_tr)

        # 逐轨首扇区检查：需要知道每个扇区落在哪个 AOB 的哪个偏移
        r = subprocess.run(["xorriso", "-indev", str(iso), "-find", "/AUDIO_TS",
                            "-name", "*.AOB", "-exec", "report_lba"],
                           capture_output=True, text=True)
        aobs = []
        for line in (r.stdout or "").splitlines():
            mm = re.search(r"(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*'([^']+)'", line)
            if mm:
                aobs.append((os.path.basename(mm.group(4)), int(mm.group(1)),
                             int(mm.group(2))))
        if not aobs:
            print("    ✗ 未找到 AOB")
            ok = False
            continue

        bad = []
        # 各组的 AOB 按分卷号拼起来，得到「组内扇区号 → (AOB, 盘内 LBA)」
        per_group = {}
        for name, lba, blocks in aobs:
            mm = re.match(r"ATS_(\d+)_(\d+)\.AOB$", name)
            if mm:
                per_group.setdefault(int(mm.group(1)), []).append(
                    (int(mm.group(2)), name, lba, blocks))
        grp_base = {}
        for g, lst in per_group.items():
            lst.sort()
            off = 0
            base = []
            for _i, name, lba, blocks in lst:
                base.append((off, off + blocks, name, lba))
                off += blocks
            grp_base[g] = base

        # 重新按组遍历 IFO，得到每组的起点列表
        check_cnt = 0
        for name in sorted(ifos):
            lba, _n = ifos[name]
            g = int(name[4:6])
            b = iso_read(iso, lba, 8)
            p = u32(b, 204) * 2048
            pgc = b[p:p + u32(b, p + 4) + 1]
            cnt = u16(pgc, 0)
            bounds = []
            for i in range(cnt):
                tto = u32(pgc, 8 + i * 8 + 4)
                sr = tto + u16(pgc, tto + 12)
                for j in range(pgc[tto + 2]):
                    bounds.append(u32(pgc, sr + j * 12 + 4))
            base = grp_base.get(g, [])
            for sec in bounds:
                for o0, o1, aname, alba in base:
                    if o0 <= sec < o1:
                        l = alba + (sec - o0)
                        blk = iso_read(iso, l, 1)
                        check_cnt += 1
                        if blk[0:4] != PACK:
                            bad.append((g, sec, blk[0:8].hex(" ")))
                        break
        print("    抽查 %d 轨首扇区" % check_cnt)
        if bad:
            print("    ✗ %d 轨的首扇区不是 pack 头 —— 读盘端会丢掉这些曲目"
                  % len(bad))
            for g, sec, h in bad[:5]:
                print("        组%d 扇区 %d 开头: %s" % (g, sec, h))
            ok = False
        else:
            print("    全部以 pack 头开头 ✔")
        print()

    print("=" * 60)
    print("全部盘合计 %d 轨" % total)
    if want is not None:
        if total != want:
            print("✗ 与音源曲目数不符：%d vs %d（差 %d）"
                  % (total, want, total - want))
            ok = False
        else:
            print("与音源曲目数一致（%d）✔" % want)
    print()
    print("快速校验", "全部通过 ✔" if ok else "存在问题 ✗")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
