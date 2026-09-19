#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""修复 Apple ALAC「未压缩帧」缺少 END 终结标记的问题。

## 症状

Apple Music / Apple 编码器产出的 ALAC（`.m4a`）在 ffmpeg 下解码会报：

    [alac] invalid element channel count
    Error submitting packet to decoder: Invalid data found when processing input
    [alac] Syntax element 4 is not implemented.

同时**静默丢帧**（每次丢 4096 采样 ≈ 85 ms @48k），退出码仍为 0。
但同一文件在 Apple 的播放器 / foobar2000 里播放完全正常。

## 根因

Apple 的 ALAC 编码器会周期性插入**未压缩帧**（raw PCM，用于随机访问定位），
特征：`is_compressed=0`、`extra_bits=0`、包大小 = 4 + 采样数×声道×位深/8。
实测出现间隔为整 32.000 秒（每 375 帧一个）。

这类帧的位数恰好是：

    帧头 23 位 + 采样数据 (n_samples × channels × sample_size) 位

其后应放置 END 元素（3 位 `111`）表示帧结束，但 Apple 写的是 `000`。
ffmpeg 因此把它读成一个 SCE（单声道）元素：

    element = get_bits(&alac->gb, 3);          // 读到 000 = TYPE_SCE
    channels = (element == TYPE_CPE) ? 2 : 1;  // → 1
    if (ch + channels > alac->channels ...)    // 第一次 1+1 <= 2，通过
    // 第二次循环 ch 已为 1 → 1+1 > 2 → "invalid element channel count"

这就是「报错条数与异常帧数一一对应」的原因。

**音频数据本身没有任何问题**，错的只是帧尾那 3 位（位于填充区）。

## 修复

把 END 标记（`111`）写回正确位置。**不触碰任何样本数据。**

修复后（实测）：

| 文件 | 原始采样 | 修复后 | 容器声明 | 报错 |
|------|----------|--------|----------|------|
| 日文版 | 10,253,856 | 10,266,144 | 10,266,144 | 6 → 0 |
| 英文版 | 10,262,048 | 10,266,144 | 10,266,144 | 2 → 0 |
| 韩文版 |  9,364,628 |  9,393,300 |  9,393,300 | 14 → 0 |

## 用法

    python3 alac_endfix.py <输入.m4a> <输出.m4a>     # 修复到新文件
    python3 alac_endfix.py --check <文件.m4a>        # 只检测，不修改

也可作为模块调用：

    from alac_endfix import probe, find_bad_frames, repair
"""
import os
import re
import struct
import subprocess
import sys

# ALAC 帧头比特布局（libavcodec/alac.c: alac_decode_frame / decode_element）
#   bit  0.. 2  element        3 位  0=SCE 1=CPE 2=CCE 3=LFE 4=DSE 5=PCE 6=FIL 7=END
#   bit  3.. 6  instance tag   4 位
#   bit  7..18  unused        12 位
#   bit 19      has_size       1 位
#   bit 20..21  extra_bits     2 位（<<3）
#   bit 22      is_compressed  1 位（取反后使用）
HDR_BITS = 3 + 4 + 12 + 1 + 2 + 1          # = 23
TYPE_END = 0b111

# 帧头掩码（针对包内第 3 字节，从 0 起算）
M_HAS_SIZE = 0x10
M_EXTRA_BITS = 0x0C
M_IS_COMPRESSED = 0x02


# --------------------------------------------------------------------------
# ALAC magic cookie
# --------------------------------------------------------------------------
# ALAC 配置（24 字节）布局，见 ffmpeg alac_set_info()：
#   0..3   max_samples_per_frame   4..4   compatible version
#   5      sample_size             6      history mult
#   7      initial history         8      rice param limit
#   9      channels               10..11  maxRun
#   12..15 max coded frame size   16..19  average bitrate
#   20..23 sample rate
VALID_SIZES = (16, 20, 24, 32)
VALID_RATES = (8000, 11025, 16000, 22050, 24000, 32000, 44100, 48000,
               64000, 88200, 96000, 176400, 192000, 352800, 384000)


def _cookie_from_bytes(buf):
    """从 24 字节配置里解析出字段；校验通过则返回 dict，否则 None。"""
    if len(buf) < 24:
        return None
    msf = struct.unpack(">I", buf[0:4])[0]
    ssize = buf[5]
    chans = buf[9]
    rate = struct.unpack(">I", buf[20:24])[0]
    if not (512 <= msf <= 16384):
        return None
    if ssize not in VALID_SIZES:
        return None
    if not (1 <= chans <= 8):
        return None
    if rate not in VALID_RATES:
        return None
    return {"max_samples_per_frame": msf, "sample_size": ssize,
            "channels": chans, "sample_rate": rate,
            "max_coded_frame_size": struct.unpack(">I", buf[12:16])[0]}


def read_magic_cookie(path):
    """读出 ALAC magic cookie（max_samples_per_frame / sample_size / ...）。

    实现方式：在文件里扫描 `alac` 签名，对每个候选位置尝试两种常见偏移
    （跳过 version/flags 的 4 字节或 8 字节），并用字段合理性校验筛选。
    比递归解析 MP4 box 树更稳健（stsd 的 payload 前 8 字节不是子 box）。
    失败返回 None。
    """
    with open(path, "rb") as f:
        data = f.read()
    idx = 0
    while True:
        i = data.find(b"alac", idx)
        if i < 0:
            break
        idx = i + 1
        for skip in (8, 12, 4):
            off = i + skip
            if off + 24 > len(data):
                continue
            cfg = _cookie_from_bytes(data[off:off + 24])
            if cfg:
                cfg["cookie_offset"] = off
                return cfg
    return None


# --------------------------------------------------------------------------
# 包枚举 / 问题帧识别
# --------------------------------------------------------------------------
def list_packets(path):
    """用 ffprobe 枚举音频包（解复用正常，只有解码受影响）。

    返回 [{"pts","dur","size","pos"}, ...]
    """
    r = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "a:0",
         "-show_entries", "packet=pts_time,duration_time,size,pos",
         "-of", "csv=p=0", path],
        capture_output=True, text=True, encoding="utf-8", errors="replace")
    out = []
    for line in r.stdout.strip().splitlines():
        f = line.split(",")
        if len(f) < 4 or not f[3]:
            continue
        try:
            out.append({"pts": float(f[0] or 0), "dur": float(f[1] or 0),
                        "size": int(f[2]), "pos": int(f[3])})
        except ValueError:
            continue
    return out


def find_bad_frames(path, cookie=None, channels=None, sample_size=None):
    """找出「未压缩帧且 END 标记缺失」的包。

    返回 (列表, cookie)。列表元素：
        {"index","pts","pos","size","n_samples","end_bit","cur_bits"}
    """
    if cookie is None:
        cookie = read_magic_cookie(path)
    if cookie is None:
        raise RuntimeError("无法读取 ALAC magic cookie，可能不是 ALAC 文件")
    ch = channels or cookie["channels"]
    ss = sample_size or cookie["sample_size"]
    msf = cookie["max_samples_per_frame"]

    pkts = list_packets(path)
    bad = []
    with open(path, "rb") as f:
        for i, p in enumerate(pkts):
            if p["size"] < 4:
                continue
            f.seek(p["pos"])
            hdr = f.read(4)
            if len(hdr) < 3:
                continue
            b2 = hdr[2]
            # 只关心未压缩帧
            if not (b2 & M_IS_COMPRESSED):
                continue
            has_size = (b2 & M_HAS_SIZE) != 0
            extra_bits = ((b2 & M_EXTRA_BITS) >> 2) << 3
            n = None
            if has_size and len(hdr) >= 7:
                f.seek(p["pos"])
                full = f.read(8)
                bits = "".join(f"{b:08b}" for b in full)
                if len(bits) >= 54:
                    n = int(bits[22:54], 2)
            if not n:
                n = msf
            # 未压缩帧按 sample_size 位连续存放
            end_bit = HDR_BITS + n * ch * ss
            if end_bit + 3 > p["size"] * 8:
                continue
            # 读当前 3 位
            f.seek(p["pos"])
            data = f.read(p["size"])
            byi, bend = divmod(end_bit, 8)
            cur = 0
            for k in range(3):
                bi = bend + k
                cur = (cur << 1) | ((data[byi + bi // 8] >> (7 - bi % 8)) & 1)
            if cur != TYPE_END:
                bad.append({"index": i, "pts": p["pts"], "pos": p["pos"],
                            "size": p["size"], "n_samples": n,
                            "end_bit": end_bit, "cur_bits": cur})
    return bad, cookie


# --------------------------------------------------------------------------
# 修复
# --------------------------------------------------------------------------
def repair(src, dst, verbose=True):
    """把 src 的缺失 END 标记补上，写出到 dst。

    返回 dict：
        {"bad": 修补数, "cookie": cookie, "patched": [...]}
    只改动帧尾填充区的比特，样本数据不受影响。
    """
    bad, cookie = find_bad_frames(src)
    with open(src, "rb") as f:
        buf = bytearray(f.read())
    for b in bad:
        byi, bend = divmod(b["end_bit"], 8)
        for k in range(3):
            bi = bend + k
            idx = b["pos"] + byi + bi // 8
            buf[idx] |= (1 << (7 - bi % 8))
    with open(dst, "wb") as f:
        f.write(buf)
    if verbose:
        print(f"  [ALAC 修复] {os.path.basename(src)}: 修补 {len(bad)} 帧")
        for b in bad:
            mm, ss = divmod(b["pts"], 60)
            print(f"      {b['pts']:>9.3f}s ({int(mm)}分{ss:05.2f}秒)  "
                  f"size={b['size']:>7}  原标记={b['cur_bits']:03b} -> 111")
    return {"bad": len(bad), "cookie": cookie, "patched": bad}


def decode_samples(path):
    """解码并返回 (采样数, 错误行数)。"""
    r = subprocess.run(
        ["ffmpeg", "-hide_banner", "-nostdin", "-v", "info", "-i", path,
         "-af", "astats=metadata=1", "-f", "null", "-"],
        capture_output=True, text=True, encoding="utf-8", errors="replace")
    m = re.search(r"Number of samples:\s*(\d+)", r.stderr)
    n = int(m.group(1)) if m else None
    e = len([l for l in r.stderr.splitlines()
             if "Error submitting" in l or "invalid element" in l
             or "not implemented" in l or "Error while decoding" in l])
    return n, e


def declared_samples(path):
    """容器声明的采样数（duration_ts）。"""
    r = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "a:0",
         "-show_entries", "stream=duration_ts", "-of", "default=nw=1:nk=1",
         path], capture_output=True, text=True, encoding="utf-8",
        errors="replace")
    try:
        return int(r.stdout.strip())
    except ValueError:
        return None


def main():
    args = [a for a in sys.argv[1:]]
    check_only = "--check" in args
    args = [a for a in args if not a.startswith("--")]
    if not args:
        print(__doc__)
        return 1
    src = args[0]
    if check_only:
        bad, cookie = find_bad_frames(src)
        n, e = decode_samples(src)
        print(f"文件: {src}")
        print(f"  magic cookie: {cookie}")
        print(f"  解码采样 {n} / 声明 {declared_samples(src)} / 报错 {e} 行")
        print(f"  待修补帧: {len(bad)}")
        for b in bad:
            mm, ss = divmod(b["pts"], 60)
            print(f"    {b['pts']:>9.3f}s ({int(mm)}分{ss:05.2f}秒) "
                  f"size={b['size']} 标记={b['cur_bits']:03b}")
        return 1 if bad else 0

    dst = args[1] if len(args) > 1 else src + ".fixed.m4a"
    r = repair(src, dst)
    n0, e0 = decode_samples(src)
    n1, e1 = decode_samples(dst)
    decl = declared_samples(src)
    print(f"  修复前: 采样 {n0}  报错 {e0} 行")
    print(f"  修复后: 采样 {n1}  报错 {e1} 行  (容器声明 {decl})")
    ok = (n1 == decl and e1 == 0)
    print("  结果:", "完全修复 ✔" if ok else "仍有问题 ✗")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
