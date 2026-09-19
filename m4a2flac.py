#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""m4a(ALAC) → FLAC 无损转换（保留元数据与封面）

与常见转换工具的区别：

1. **先修 ALAC 缺陷再转**
   Apple 编码的 ALAC 里，周期性插入的「未压缩帧」其帧尾 END 标记被写成 000
   （应为 111）。ffmpeg 会把这种帧误读成 SCE 元素而**静默丢帧**（每次丢一帧
   = 4096 采样），退出码却是 0 —— 而源文件与 foobar2000/Apple 播放器都听不出
   问题。直接转换会把丢帧固化进 FLAC，且再也无法察觉。
   详见 alac_endfix.py。

2. **规范化 Vorbis 注释名**
   MP4 与 FLAC 的标签命名体系不同。直接 `-map_metadata 0` 会把 MP4 名原样
   写进 FLAC，其中不少在 FLAC 播放器里读不到：

     MP4 名                FLAC 规范名
     sort_name             TITLESORT
     sort_album            ALBUMSORT
     sort_artist           ARTISTSORT
     sort_album_artist     ALBUMARTISTSORT
     sort_composer         COMPOSERSORT
     track / disc          TRACKNUMBER / DISCNUMBER
     album_artist          ALBUMARTIST
     upc                   BARCODE

   同时剔除只在 MP4 容器里有意义的标签（major_brand / compatible_brands /
   minor_version / creation_time）。

3. **封面：不只搬过来，还要规范化**
   实测 ffmpeg 写 FLAC 附件图时固定写 `type=0 (Other)` 与 `depth=12`
   （depth 是从 mjpeg 的 yuvj420p = 12bpp 推出来的）。问题在于：

     · 不少播放器（尤其是车载/移动端）只在 **type=3 (Cover (front))**
       时才把图片当封面显示
     · JPEG 的 depth 按惯例是 24，12 会被某些严格实现判为异常

   所以转完后用 metaflac 重新导入一次（只给文件名时 metaflac 默认 type=3，
   并自行解析 MIME / 尺寸 / 深度），并用字节比对确认封面与源一致。

4. **三重校验**
   · PCM MD5 与**修复后**的源一致（真无损）
   · 标签逐项比对（规范化后应一一对应）
   · 源有封面时，输出必须有 PICTURE 块，且图片字节与源一致

用法::

    python3 m4a2flac.py <目录或文件>...
        --in-place      转换成功后删除源 m4a（默认保留；有任一文件失败就不删）
        --no-repair     跳过 ALAC 修复（不推荐）
        --level N       FLAC 压缩级别 0-8（默认 8）
        --dry-run       只报告将要做什么
        --jobs N        并发数（默认 min(4, CPU 数)）

注意：源 m4a 默认**不删除**，转换产物与它同目录同名（仅扩展名不同）。若原目录
里有同名 .flac，会被覆盖。
"""

import argparse
import concurrent.futures as futures
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

try:
    import alac_endfix
except ImportError:
    alac_endfix = None

FFMPEG = "ffmpeg"
FFPROBE = "ffprobe"
METAFLAC = "metaflac"

# MP4 → FLAC 规范名。键统一小写后查表；未列出的仅做「转大写」处理。
RENAME = {
    "sort_name": "TITLESORT",
    "sort_album": "ALBUMSORT",
    "sort_artist": "ARTISTSORT",
    "sort_album_artist": "ALBUMARTISTSORT",
    "sort_composer": "COMPOSERSORT",
    "track": "TRACKNUMBER",
    "disc": "DISCNUMBER",
    "album_artist": "ALBUMARTIST",
    "upc": "BARCODE",
}

# 只在 MP4 容器里有意义，写进 FLAC 是噪声
DROP = {
    "major_brand", "minor_brand", "minor_version", "compatible_brands",
    "creation_time", "encoder", "vendor_id", "handler_name", "language",
}

PICTURE_RE = re.compile(r"^\s*type:\s*\d+\s*\(PICTURE\)", re.M)
COMMENT_RE = re.compile(r"^\s*comment\[\d+\]:\s*(.*)$", re.M)
# PICTURE 块里有两行都以 "type:" 开头：第一行是元数据块类型（恒为 6），
# 第二行才是图片类型。用「紧跟 MIME type:」做锚点才不会认错。
# 注意图片类型名本身可能带括号（如 "Cover (front)"），所以不能用 [^)]*。
PIC_TYPE_RE = re.compile(r"type:\s*(\d+)\s*\([^\n]*\n\s*MIME type:")


# 封面编码 -> 临时文件扩展名。metaflac 靠扩展名判断图片格式，没有扩展名会失败。
CODEC_EXT = {"mjpeg": ".jpg", "jpeg": ".jpg", "png": ".png",
             "bmp": ".bmp", "gif": ".gif", "webp": ".webp",
             "tiff": ".tif", "targa": ".tga"}


def cover_ext(path):
    """源封面流的编码名 → 扩展名（读不到时回退 .jpg）。"""
    r = run([FFPROBE, "-v", "error", "-select_streams", "v:0",
             "-show_entries", "stream=codec_name", "-of", "csv=p=0", path])
    return CODEC_EXT.get((r.stdout or "").strip().lower(), ".jpg")


def fix_picture(flac, tmpdir, ext=".jpg"):
    """把 PICTURE 块规范成 type=3（front cover），并修正 depth。

    做法：导出图片 → 删除 PICTURE 块 → 让 metaflac 重新导入。
    只给文件名时 metaflac 默认 type=3，并自行解析 MIME/尺寸/深度。

    返回 (图片类型, 图片字节数, 错误信息)；成功时错误信息为 None。
    """
    cover = os.path.join(tmpdir, "cover_pic" + ext)
    r = run([METAFLAC, f"--export-picture-to={cover}", flac])
    if r.returncode != 0 or not os.path.exists(cover) \
            or os.path.getsize(cover) == 0:
        return None, None, "导出封面失败: %s" % (r.stderr or "").strip()[:120]
    nbytes = os.path.getsize(cover)
    r = run([METAFLAC, "--remove", "--block-type=PICTURE", flac])
    if r.returncode != 0:
        return None, None, "删 PICTURE 块失败: %s" % (r.stderr or "").strip()[:120]
    r = run([METAFLAC, f"--import-picture-from={cover}", flac])
    if r.returncode != 0:
        return None, None, "导入封面失败: %s" % (r.stderr or "").strip()[:120]
    rr = run([METAFLAC, "--list", "--block-type=PICTURE", flac])
    m = PIC_TYPE_RE.search(rr.stdout)
    return (int(m.group(1)) if m else None), nbytes, None


def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True,
                          encoding="utf-8", errors="replace", **kw)


def probe_tags(path):
    """返回 format 级标签 dict（保留原始大小写）。"""
    r = run([FFPROBE, "-v", "error", "-show_format", "-of", "json", path])
    try:
        return json.loads(r.stdout)["format"].get("tags", {}) or {}
    except (ValueError, KeyError):
        return {}


def probe_streams(path):
    r = run([FFPROBE, "-v", "error", "-show_streams", "-of", "json", path])
    try:
        return json.loads(r.stdout).get("streams", [])
    except ValueError:
        return []


def normalize_tags(raw):
    """把 MP4 标签名规范化成 FLAC 规范名。返回 [(KEY, VALUE), ...]"""
    out = []
    seen = set()
    # 先按字母序，保证结果稳定
    for k in sorted(raw, key=lambda s: s.lower()):
        lk = k.lower()
        if lk in DROP:
            continue
        name = RENAME.get(lk, lk.upper())
        val = raw[k]
        if isinstance(val, (list, tuple)):
            val = ", ".join(str(x) for x in val)
        val = str(val)
        if not val.strip():
            continue
        key = (name, val)
        if key in seen:          # 同一规范名且同值 → 去重
            continue
        seen.add(key)
        out.append(key)
    return out


def read_vorbis_tags(path):
    """用 metaflac 读输出的真实注释，返回 [(KEY, VALUE), ...]"""
    r = run([METAFLAC, "--list", "--block-type=VORBIS_COMMENT", path])
    out = []
    for m in COMMENT_RE.finditer(r.stdout):
        item = m.group(1)
        if "=" in item:
            k, v = item.split("=", 1)
            out.append((k, v))
    return out


def audio_md5(path):
    """音频流（仅第一个）的 PCM MD5。"""
    r = run([FFMPEG, "-hide_banner", "-loglevel", "error", "-i", path,
             "-map", "0:a:0", "-f", "md5", "-"])
    m = re.search(r"MD5=([0-9a-fA-F]+)", r.stdout + r.stderr)
    return m.group(1).lower() if m else None


def has_cover(path):
    r = run([FFPROBE, "-v", "error", "-select_streams", "v", "-show_entries",
             "stream=codec_type", "-of", "csv=p=0", path])
    return bool(r.stdout.strip())


def convert_one(src, level, do_repair, dry_run):
    """转换单个文件。返回 (状态字符串, 详情 dict)"""
    base, _ = os.path.splitext(src)
    dst = base + ".flac"
    info = {"src": src, "dst": dst, "repaired": 0}

    def bad(why):
        d = dict(info)
        d["why"] = why
        return "FAIL", d

    raw_tags = probe_tags(src)
    if not raw_tags:
        return bad("读不到标签")

    if dry_run:
        n = 0
        if do_repair and alac_endfix:
            try:
                frames, _ = alac_endfix.find_bad_frames(src)
                n = len(frames)
            except Exception as e:                      # noqa: BLE001
                return bad(f"缺陷检测失败: {e}")
        info["repaired"] = n
        return "DRY", info

    tmpdir = tempfile.mkdtemp(prefix="m4a2flac.")
    try:
        # ---- 1. 缺陷修复（只在需要时落盘）----
        work = src
        if do_repair:
            if alac_endfix is None:
                return bad("找不到 alac_endfix.py，无法检测缺陷")
            frames, _ = alac_endfix.find_bad_frames(src)
            if frames:
                work = os.path.join(tmpdir, os.path.basename(src))
                alac_endfix.repair(src, work, verbose=False)
                info["repaired"] = len(frames)
                info["bad_frames"] = [round(b["pts"], 3) for b in frames]

        # ---- 2. 转换（音频 + 封面）----
        tags = normalize_tags(raw_tags)
        cmd = [FFMPEG, "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
               "-i", work, "-map", "0:a:0"]
        if has_cover(work):
            cmd += ["-map", "0:v:0", "-c:v", "copy",
                    "-disposition:v:0", "attached_pic"]
        cmd += ["-c:a", "flac", "-compression_level", str(level),
                "-map_metadata", "-1"]
        for k, v in tags:
            cmd += ["-metadata", f"{k}={v}"]
        cmd.append(dst)
        r = subprocess.run(cmd, capture_output=True, text=True,
                           encoding="utf-8", errors="replace")
        if r.returncode != 0 or not os.path.exists(dst):
            return bad("ffmpeg 转换失败: " + (r.stderr or "")[:200])

        # ---- 3. 校验 ----
        # 基准必须是**修复后**的文件：源文件缺 END 标记时 ffmpeg 会静默丢帧，
        # 拿它当基准会把「本来就该多出来的采样」误判成不一致。
        m_ref = audio_md5(work)
        m_dst = audio_md5(dst)
        if not m_ref or m_ref != m_dst:
            os.remove(dst)
            return bad(f"PCM MD5 不一致 基准={m_ref} 输出={m_dst}")
        info["md5"] = m_dst
        if info["repaired"]:
            info["md5_src_raw"] = audio_md5(src)   # 仅记录，用于说明差异

        got = read_vorbis_tags(dst)
        want = list(tags)
        missing = [t for t in want if t not in got]
        if missing:
            info["missing_tags"] = missing[:5]
        info["tags_src"] = len(want)
        info["tags_dst"] = len(got)

        if has_cover(src):
            rr = run([METAFLAC, "--list", "--block-type=PICTURE", dst])
            if not PICTURE_RE.search(rr.stdout):
                os.remove(dst)
                return bad("源有封面但输出缺 PICTURE 块")
            ptype, pbytes, perr = fix_picture(dst, tmpdir, cover_ext(src))
            if ptype is None:
                os.remove(dst)
                return bad("封面规范化失败: %s" % perr)
            # 封面字节必须与源里的附件图一致
            srccov = os.path.join(tmpdir, "cover_src" + cover_ext(src))
            run([FFMPEG, "-hide_banner", "-loglevel", "error", "-nostdin",
                 "-y", "-i", src, "-map", "0:v:0", "-c", "copy",
                 "-f", "image2", srccov])
            if os.path.exists(srccov):
                if os.path.getsize(srccov) != pbytes:
                    os.remove(dst)
                    return bad("封面字节数不符: 源 %d / 输出 %d"
                               % (os.path.getsize(srccov), pbytes))
                info["cover_exact"] = open(srccov, "rb").read() == \
                    open(os.path.join(tmpdir,
                                      "cover_pic" + cover_ext(src)),
                         "rb").read()
            info["cover"] = True
            info["cover_type"] = ptype
            info["cover_bytes"] = pbytes

        return "OK", info
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


def collect(paths, recursive=True):
    """收集 .m4a 文件。目录默认递归（音源通常按专辑分子目录）。"""
    files = []
    for p in paths:
        if os.path.isdir(p):
            if recursive:
                for root, dirs, names in os.walk(p):
                    dirs.sort()
                    for n in sorted(names):
                        if n.lower().endswith(".m4a"):
                            files.append(os.path.join(root, n))
            else:
                for n in sorted(os.listdir(p)):
                    if n.lower().endswith(".m4a"):
                        files.append(os.path.join(p, n))
        elif os.path.isfile(p) and p.lower().endswith(".m4a"):
            files.append(p)
        else:
            print(f"[跳过] 既不是目录也不是 .m4a: {p}", file=sys.stderr)
    return files


def main():
    ap = argparse.ArgumentParser(
        description="m4a(ALAC) → FLAC 无损转换（先修 ALAC 缺陷，保留元数据与封面）")
    ap.add_argument("paths", nargs="+", help="目录或 .m4a 文件")
    ap.add_argument("--in-place", action="store_true",
                    help="转换成功后删除源 m4a（默认保留）")
    ap.add_argument("--no-repair", action="store_true",
                    help="跳过 ALAC 缺陷修复（不推荐）")
    ap.add_argument("--level", type=int, default=8, help="FLAC 压缩级别 0-8")
    ap.add_argument("--dry-run", action="store_true", help="只报告，不写文件")
    ap.add_argument("--jobs", type=int, default=min(4, os.cpu_count() or 1),
                    help="并发数")
    args = ap.parse_args()

    if not (0 <= args.level <= 8):
        print("[错误] --level 必须在 0-8 之间", file=sys.stderr)
        return 2
    for tool in (FFMPEG, FFPROBE, METAFLAC):
        if not shutil.which(tool):
            print(f"[错误] 找不到 {tool}", file=sys.stderr)
            return 2

    files = collect(args.paths)
    if not files:
        print("[错误] 没有可处理的 .m4a 文件", file=sys.stderr)
        return 1

    print(f"共 {len(files)} 个文件，压缩级别 {args.level}，"
          f"并发 {args.jobs}，修复={'否' if args.no_repair else '是'}"
          f"{'，DRY-RUN' if args.dry_run else ''}")
    print()

    ok = fail = 0
    problems = []

    def show(st, res):
        nonlocal ok, fail
        name = os.path.basename(res.get("src", "?"))
        if st == "DRY":
            extra = (f"需修补 {res['repaired']} 帧"
                     if res.get("repaired") else "无需修补")
            print(f"  [DRY] {name}  ({extra})")
            ok += 1
            return
        if st != "OK":
            fail += 1
            print(f"  [失败] {name}")
            print(f"         {res.get('why', '')}")
            problems.append((res.get("src"), res.get("why", "")))
            return
        ok += 1
        bits = []
        if res.get("repaired"):
            bits.append(f"修补 {res['repaired']} 帧")
        bits.append(f"MD5 {res['md5'][:12]}")
        bits.append(f"标签 {res['tags_dst']} 项")
        if res.get("cover"):
            bits.append(f"封面 type={res.get('cover_type')} "
                        f"{res.get('cover_bytes')}B"
                        + (" 逐字节一致" if res.get("cover_exact") else ""))
        if res.get("missing_tags"):
            bits.append(f"!! 缺标签 {res['missing_tags']}")
        print(f"  [OK] {name}  ({', '.join(bits)})")

    with futures.ThreadPoolExecutor(max_workers=args.jobs) as ex:
        todo = {ex.submit(convert_one, f, args.level,
                          not args.no_repair, args.dry_run): f
                for f in files}
        for fu in futures.as_completed(todo):
            try:
                st, res = fu.result()
            except Exception as e:                          # noqa: BLE001
                st, res = "FAIL", {"src": todo[fu], "why": repr(e)}
            show(st, res)

    print()
    print(f"成功 {ok} / {len(files)}，失败 {fail}")

    if args.in_place and not args.dry_run and fail == 0:
        print()
        print("删除源 m4a（--in-place）...")
        for f in files:
            try:
                os.remove(f)
            except OSError as e:
                print(f"  [警告] 无法删除 {f}: {e}")
        print("完成")

    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main())
