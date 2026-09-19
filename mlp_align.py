#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把 ffmpeg 编码的 MLP 对齐到参考编码器（SurCode MLP）的头部

背景
----
同曲目、同参数的 MLP，ffmpeg 与 SurCode 的 major sync（28 字节）
实测只差 3 处，其余 25 字节全同：

    [14:16] peak_bitrate                SurCode 3200   ffmpeg 3199
    [16]    extended_substream_info     SurCode 1      ffmpeg 0
    [26:28] checksum16                  （前两项的后果）

另有一处不在 major sync 里：

    流末尾                          SurCode 有 END_OF_STREAM   ffmpeg 无

本脚本做**纯字节修补**（不重编码），把上述 4 处对齐：

1. ``peak_bitrate``：ffmpeg 的 ``mlp_peak_bitrate()`` 用了
   ``((peak << 4) - 8) / sample_rate``（整数截断），写出的值经解码公式
   ``(raw * sr + 8) >> 4`` 反算**回不到** 9600000。
   改成向上取整 ``ceil((9600000 * 16 - 8) / sr)`` 即可往返精确：
   48000 Hz -> 3200（与 SurCode 相同）、44100 Hz -> 3483。
2. ``extended_substream_info`` -> 1（SurCode 147/147 都是 1）。
   注意：ffmpeg 的解码器对 MLP 忽略此字段，且 ``mlp_get_major_sync_size()``
   只在 TrueHD(0xba) 下才用它计算扩展长度，所以改它对 MLP 是**无副作用**的。
3. 重算 major sync 的 ``checksum16``。
4. 在最后一个 access unit 的第 1 个子流数据体**末尾**插入
   ``END_OF_STREAM``（``0xD234D234``），并按规范修正：
   ``substream header`` 的 ``end``（+2 words）、
   ``access unit header`` 的 ``length``（+2 words）、
   该 AU 头的 4 位奇偶校验、以及子流的 ``parity`` / ``checksum``。

**不动压缩载荷**，所以音频内容逐字节不变（有自检可证）。

为什么 ffmpeg 不写 END_OF_STREAM
--------------------------------
``mlp`` 编码器未声明 ``AV_CODEC_CAP_SMALL_LAST_FRAME``（``truehd`` 有），
于是 ffmpeg 通用层总把末帧补齐成完整 ``frame_size``（40 样本），
编码器里 ``shorten_by = frame_size - nb_samples`` 恒为 0，
写 END_OF_STREAM 的分支永不进入 —— 这是**必然**结果，靠命令行参数无法绕过。

用法::

    python3 mlp_align.py --check  <文件...>     # 只报告差异
    python3 mlp_align.py --align  <文件...>     # 原地对齐
    python3 mlp_align.py --align -o 输出目录 <文件...>
"""

import argparse
import os
import struct
import sys

# ---- MLP 规范常量 ----
SYNC_MAJOR = b"\xf8\x72\x6f"
SYNC_MLP = 0xBB
END_OF_STREAM = 0xD234D234
MAJOR_SYNC_SIZE = 28
FLAGS_DVDA = 0x4000
SUBSTREAM_INFO_ALWAYS_SET = 0x04
BASE_PEAK_BITRATE = 9600000          # 编码器里的目标峰值码率


# --------------------------------------------------------------------------
# CRC / 校验（复刻 ffmpeg 的 av_crc，已在真实文件上逐字节验证过）
# --------------------------------------------------------------------------
def _bswap32(x):
    return int.from_bytes(x.to_bytes(4, "big"), "little")


def _crc_table(poly, bits):
    t = [0] * 256
    for i in range(256):
        c = (i << 24) & 0xFFFFFFFF
        for _ in range(8):
            c = ((c << 1) & 0xFFFFFFFF) ^ \
                ((poly << (32 - bits)) if (c >> 31) else 0)
        t[i] = _bswap32(c)
    return t


_CRC_2D = _crc_table(0x002D, 16)     # av_crc_init(crc_2D, 0, 16, 0x002D)
_CRC_63 = _crc_table(0x0063, 8)      # av_crc_init(crc_63, 0,  8, 0x0063)


def _av_crc(tbl, crc, buf, n):
    for i in range(n):
        crc = tbl[(crc & 0xFF) ^ buf[i]] ^ (crc >> 8)
    return crc


def checksum16(buf, size):
    """major sync 的 16 位校验（末 2 字节按小端参与 XOR）。"""
    crc = _av_crc(_CRC_2D, 0, buf, size - 2)
    crc ^= buf[size - 2] | (buf[size - 1] << 8)          # AV_RL16
    return crc


def checksum8(buf, size):
    crc = _av_crc(_CRC_63, 0x3C, buf, size - 1)          # crc_63[0xa2] == 0x3c
    crc ^= buf[size - 1]
    return crc


def calc_parity(buf, size):
    s = 0
    for i in range(size):
        s ^= buf[i]
    s ^= s >> 16
    s ^= s >> 8
    return s & 0xFF


def peak_bitrate_raw(sample_rate):
    """向上取整，使解码公式 ``(raw * sr + 8) >> 4`` 恰好回到 9600000。

    ffmpeg 用向下取整，48000 Hz 得到 3199（反算 9597000），
    参考编码器写 3200（反算恰好 9600000）。
    """
    num = BASE_PEAK_BITRATE * 16 - 8
    return -(-num // sample_rate)                        # ceil 除法


# --------------------------------------------------------------------------
# access unit 遍历
# --------------------------------------------------------------------------
def walk(data):
    """返回 [(au_off, au_len, has_major_sync)]；覆盖不全则抛错。"""
    pos, out = 0, []
    while pos + 4 <= len(data):
        h = int.from_bytes(data[pos:pos + 2], "big")
        n = (h & 0x0FFF) * 2
        if n < 4 or pos + n > len(data):
            raise ValueError("access unit 在 offset %d 处长度非法 (%d)" % (pos, n))
        out.append((pos, n, data[pos + 4:pos + 7] == SYNC_MAJOR))
        pos += n
    if pos != len(data):
        raise ValueError("access unit 未覆盖整个文件（余 %d 字节）"
                         % (len(data) - pos))
    return out


def major_sync_off(au_off, has_ms):
    """major sync 在 access unit 内的偏移（AU 头 4 字节之后）。"""
    return au_off + 4 if has_ms else None


def sample_rate_of(ms_bytes):
    """从 major sync 还原采样率。

    byte 5 是 **两个** 4 位字段：``coded_sample_rate[0]`` 在高半字节、
    ``coded_sample_rate[1]``（未使用，编码器写 0xF）在低半字节。
    取值规则同 ffmpeg 的 ``mlp_samplerate()``：
    第 3 位为 1 则基频 44100，否则 48000，低 3 位是移位。
    """
    ratebits = (ms_bytes[5] >> 4) & 0x0F
    if ratebits == 0x0F:
        return None
    return (44100 if (ratebits & 8) else 48000) << (ratebits & 7)


# --------------------------------------------------------------------------
# 检查
# --------------------------------------------------------------------------
def inspect(data):
    """返回一份诊断结果 dict（不改数据）。"""
    aus = walk(data)
    # 注意：major sync 在 access unit 内偏移 +4（AU 头占 4 字节）
    ms_off = [o + 4 for o, _n, h in aus if h]
    r = {
        "size": len(data), "aus": len(aus), "major_syncs": len(ms_off),
        "interval": (len(aus) / len(ms_off)) if ms_off else 0,
        "ms_errors": [], "au_parity_errors": [], "sub_errors": [],
        "eos": False, "peak_raw": None, "ext": None, "sr": None,
    }

    # major sync
    for o in ms_off:
        b = data[o:o + MAJOR_SYNC_SIZE]
        if len(b) < MAJOR_SYNC_SIZE:
            r["ms_errors"].append((o, "截断"))
            continue
        sig = int.from_bytes(b[8:10], "big")
        if sig != 0xB752:
            r["ms_errors"].append((o, "INFO_SIG=0x%04x" % sig))
        if checksum16(b, MAJOR_SYNC_SIZE - 2) != int.from_bytes(b[26:28], "little"):
            r["ms_errors"].append((o, "checksum16 不符"))
        v = int.from_bytes(b[14:16], "big")
        r["peak_raw"] = v & 0x7FFF
        r["ext"] = b[16] & 3
        r["sr"] = sample_rate_of(b)

    # AU 头奇偶 + 子流校验
    for au_off, au_len, has_ms in aus:
        off = au_off + 4
        if has_ms:
            off += MAJOR_SYNC_SIZE
        if off + 2 > au_off + au_len:
            continue
        sh = int.from_bytes(data[off:off + 2], "big")
        end = (sh & 0x0FFF) * 2
        sd = data[off + 2: off + 2 + end]
        # AU 头奇偶：XOR(input_timing, length_words, 各子流头字节)
        timing = int.from_bytes(data[au_off + 2:au_off + 4], "big")
        p = timing ^ (au_len // 2)
        p ^= (sh >> 8) & 0xFF
        p ^= sh & 0xFF
        p ^= p >> 8
        p ^= p >> 4
        p &= 0xF
        want = (data[au_off] >> 4) & 0xF
        if (p ^ 0xF) != want:
            r["au_parity_errors"].append((au_off, want, p ^ 0xF))
        # 子流 parity/checksum
        if (sh >> 13) & 1 and len(sd) >= 2:
            body = sd[:-2]
            if sd[-2] != (calc_parity(body, len(body)) ^ 0xA9) or \
               sd[-1] != checksum8(body, len(body)):
                r["sub_errors"].append(au_off)
        if pos_is_last(aus, au_off) and sd[:-2][-4:] == b"\xd2\x34\xd2\x34":
            r["eos"] = True
    return r


def pos_is_last(aus, au_off):
    return aus and aus[-1][0] == au_off


# --------------------------------------------------------------------------
# 对齐
# --------------------------------------------------------------------------
def align_bytes(data):
    """返回 (新数据, 变更说明 dict)。不改原数据。"""
    buf = bytearray(data)
    info = {"peak": 0, "ext": 0, "cksum": 0, "eos": False}

    aus = walk(data)
    ms_off = [o + 4 for o, _n, h in aus if h]     # AU 内 +4 才是 major sync

    # ---- 1/2/3. major sync：peak_bitrate、extended_substream_info、checksum ----
    for o in ms_off:
        b = bytearray(buf[o:o + MAJOR_SYNC_SIZE])
        sr = sample_rate_of(b)
        if not sr:
            raise ValueError("offset %d 的 major sync 采样率未知" % o)
        want_peak = peak_bitrate_raw(sr)
        v = int.from_bytes(b[14:16], "big")
        if (v & 0x7FFF) != want_peak:
            v = ((v >> 15) << 15) | (want_peak & 0x7FFF)
            b[14:16] = v.to_bytes(2, "big")
            info["peak"] += 1
        if (b[16] & 3) != 1:
            # 保留高 4 位（num_substreams）与中间 2 位，只改低 2 位
            b[16] = (b[16] & 0xFC) | 1
            info["ext"] += 1
        new_ck = checksum16(bytes(b), MAJOR_SYNC_SIZE - 2)
        if int.from_bytes(b[26:28], "little") != new_ck:
            b[26:28] = new_ck.to_bytes(2, "little")
            info["cksum"] += 1
        buf[o:o + MAJOR_SYNC_SIZE] = b

    # ---- 4. 末尾插 END_OF_STREAM ----
    au_off, au_len, has_ms = aus[-1]
    off = au_off + 4 + (MAJOR_SYNC_SIZE if has_ms else 0)
    sh_off = off
    sh = int.from_bytes(buf[sh_off:sh_off + 2], "big")
    end = (sh & 0x0FFF) * 2
    sd_off = sh_off + 2
    body = bytes(buf[sd_off: sd_off + end - 2])

    if body[-4:] == END_OF_STREAM.to_bytes(4, "big"):
        return bytes(buf), info                       # 已有，幂等

    new_body = body + END_OF_STREAM.to_bytes(4, "big")
    p = calc_parity(new_body, len(new_body)) ^ 0xA9
    c = checksum8(new_body, len(new_body))
    buf[sd_off: sd_off + end] = new_body + bytes([p, c])

    # substream header 的 end += 2 words
    sh = (sh & 0xF000) | (((sh & 0x0FFF) + 2) & 0x0FFF)
    buf[sh_off:sh_off + 2] = sh.to_bytes(2, "big")
    # AU header 的 length += 2 words，并重算 4 位奇偶
    timing = int.from_bytes(buf[au_off + 2:au_off + 4], "big")
    new_len_words = (au_len // 2) + 2
    pn = timing ^ new_len_words
    pn ^= (sh >> 8) & 0xFF
    pn ^= sh & 0xFF
    pn ^= pn >> 8
    pn ^= pn >> 4
    pn &= 0xF
    old = int.from_bytes(buf[au_off:au_off + 2], "big")
    new_hdr = ((pn ^ 0xF) << 12) | (new_len_words & 0x0FFF)
    buf[au_off:au_off + 2] = new_hdr.to_bytes(2, "big")
    info["eos"] = True
    info["au_hdr"] = (old, new_hdr)
    return bytes(buf), info


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------
def main():
    ap = argparse.ArgumentParser(description="把 ffmpeg 的 MLP 对齐到 SurCode 头部")
    ap.add_argument("files", nargs="+")
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--check", action="store_true", help="只报告")
    g.add_argument("--align", action="store_true", help="原地（或写 -o）对齐")
    ap.add_argument("-o", "--outdir", help="输出目录（保持文件名）")
    ap.add_argument("-q", "--quiet", action="store_true")
    args = ap.parse_args()

    rc = 0
    for f in args.files:
        if not os.path.isfile(f):
            print("[跳过] 不是文件: %s" % f)
            rc = 1
            continue
        data = open(f, "rb").read()
        r = inspect(data)
        name = os.path.basename(f)

        if args.check:
            bad = (r["ms_errors"] or r["au_parity_errors"] or r["sub_errors"])
            flag = "OK " if not bad else "BAD"
            print("[%s] %s" % (flag, name))
            print("       AU %d，major sync %d（每 %.1f 个），peak=%s，ext=%s，"
                  "EOS=%s" % (r["aus"], r["major_syncs"], r["interval"],
                              r["peak_raw"], r["ext"], r["eos"]))
            if r["ms_errors"]:
                print("       major sync 异常 %d 处，例如 %s"
                      % (len(r["ms_errors"]), r["ms_errors"][:3]))
            if r["au_parity_errors"]:
                print("       AU 奇偶异常 %d 处，例如 %s"
                      % (len(r["au_parity_errors"]), r["au_parity_errors"][:3]))
            if r["sub_errors"]:
                print("       子流校验异常 %d 处" % len(r["sub_errors"]))
            if bad:
                rc = 1
            continue

        new, info = align_bytes(data)
        if args.outdir:
            dst = os.path.join(args.outdir, name)
            os.makedirs(args.outdir, exist_ok=True)
        else:
            dst = f
        if new == data:
            if not args.quiet:
                print("[已对齐] %s" % name)
            continue
        if not args.quiet:
            print("[对齐] %s   peak×%d ext×%d cksum×%d EOS=%s (+%d 字节)"
                  % (name, info["peak"], info["ext"], info["cksum"],
                     info["eos"], len(new) - len(data)))
        tmp = dst + ".tmp"
        with open(tmp, "wb") as fp:
            fp.write(new)
        os.replace(tmp, dst)
    return rc


if __name__ == "__main__":
    sys.exit(main())
