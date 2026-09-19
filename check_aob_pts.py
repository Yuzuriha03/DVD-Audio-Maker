#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""解析 AOB 扇区的 PES 头 PTS/DTS，检验时间轴是否连续正确。

DVD-Audio 每个扇区(2048B)为一个 pack：
  pack header : 00 00 01 BA ...
  PES header  : 00 00 01 BD ... 其中含 PTS(5字节, offset 约 23) 与 DTS(约 28)
"""
import pathlib
import sys

def parse_pts(b):
    # 5 字节 PTS/DTS 编码：4bit 标记 + 3x(1bit 标记 + 15bit 值)
    v = ((b[0] >> 1) & 0x07) << 30
    v |= ((b[1] << 8 | b[2]) >> 1) << 15
    v |= (b[3] << 8 | b[4]) >> 1
    return v

def analyze(path, max_sectors=None):
    data = pathlib.Path(path).read_bytes()
    n = len(data) // 2048
    if max_sectors:
        n = min(n, max_sectors)

    pts_list = []
    for s in range(n):
        sec = data[s * 2048:(s + 1) * 2048]
        if sec[0:4] != b"\x00\x00\x01\xBA":
            pts_list.append(None)
            continue
        # 找 PES start code 00 00 01 BD
        idx = sec.find(b"\x00\x00\x01\xBD", 4, 64)
        if idx < 0:
            pts_list.append(None)
            continue
        flags = sec[idx + 7]
        if flags & 0x80:            # PTS present
            pts_list.append(parse_pts(sec[idx + 9:idx + 14]))
        else:
            pts_list.append(None)

    valid = [v for v in pts_list if v is not None]
    print("文件:", path)
    print("总扇区:", len(pts_list), " 含PTS扇区:", len(valid))

    if len(valid) < 3:
        print("!! 有效 PTS 太少，时间轴缺失")
        return

    print("首个 PTS:", valid[0], " 末个 PTS:", valid[-1])
    print("时间跨度: %.3f 秒 (%.3f 分钟)" % ((valid[-1] - valid[0]) / 90000,
                                            (valid[-1] - valid[0]) / 90000 / 60))

    # 步长统计
    steps = [valid[i + 1] - valid[i] for i in range(len(valid) - 1)]
    neg = sum(1 for x in steps if x < 0)
    zero = sum(1 for x in steps if x == 0)
    print("步长: 最小 %d 最大 %d  负步长 %d 个  零步长 %d 个"
          % (min(steps), max(steps), neg, zero))

    # 异常占比
    import statistics
    med = statistics.median(steps)
    print("步长中位数: %d" % med)
    weird = [x for x in steps if x <= 0 or x > med * 20]
    ratio = 100.0 * len(weird) / len(steps)
    print("异常步长(<=0 或 >20倍中位数) 数量: %d 占比 %.3f%%" % (len(weird), ratio))

    print("前 8 个步长:", steps[:8])
    print("中段 8 个步长:", steps[len(steps)//2:len(steps)//2 + 8])
    print("末段 8 个步长:", steps[-8:])

    print()
    if valid[-1] == valid[0]:
        print("结论: 失败 — 所有扇区 PTS 相同，时间轴缺失，进度无法定位")
        return False
    if ratio > 1.0:
        print("结论: 失败 — 异常步长占比过高")
        return False
    print("结论: 通过 — PTS 随播放单调递增，时间轴正常")
    return True


if __name__ == "__main__":
    results = [analyze(p) for p in sys.argv[1:]]
    sys.exit(0 if all(results) else 1)
