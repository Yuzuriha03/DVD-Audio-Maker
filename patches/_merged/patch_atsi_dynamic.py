#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ATSI 表改为**按曲目数动态分配**，并让扇区数按实际用量算。

## 原来的问题

`atsi2.c` 的 `create_atsi()` 里 ATSI 表是一个**固定大小的栈数组**：

    uint8_t atsi[2048 * 3];                  // 3 扇区 = 6144 字节
    ...
    if (i > 4096) *atsi_sectors = 3;         // 只有 2/3 两档
    else          *atsi_sectors = 2;

写超过 6144 字节就是**栈越界**（`docs/TROUBLESHOOTING.md` 第 3 节）。
于是 `GROUP_TRACK_LIMIT` 只能压到 64，一张盘的曲目被拆成多个「音频组」——
对 SurCode 产出的这版盘，盘1 被拆成 66+25 两组，而**两组的音频参数完全相同**
（都是 48000/24），也就是说分组**不是格式要求**，纯粹是这个上限的产物。

## 实测每轨占用

从生产构建读出 ATSI 内部的 `i` 字段（`atsi[0x804] + 0x801`）：

| 组 | 轨数 | i | 每轨 |
|---|---|---|---|
| 盘1 组1 | 65 | 5696 | 56.1 |
| 盘1 组2 | 10 | 2616 | 56.7 |
| 盘1 组3 | 14 | 2840 | 56.5 |

基线 2049 字节（ATSI_MAT 等占 0x800）⇒ **每轨 ≈ 56.5 字节**，各轨数下高度一致。

## 改动

1. **缓冲按曲目数分配在堆上**（不再占固定大小的栈）：
   `2049 + ntracks × 96`（96 是 56.5 的 ~1.7 倍余量），向上取整到扇区再多留一扇区。
   99 轨（`MAX_TRACKS`）也只分配约 12 KB。
2. **扇区数按实际用量算**（`ceil(i / 2048)`，下限 2），不再固定 2/3 两档。
3. 分配失败 / 仍是写不下时，给出**明确报错**而不是静默越界。
4. 函数出口 `free()` 掉缓冲（该函数只有一个 `return`）。

## 为什么其它地方不用改

`*atsi_sectors` 仍是唯一事实来源，下面这些**全部由它派生**，会自动跟着调整：

- 写盘长度 `create_file(..., 2048 * (*atsi_sectors), ...)`
- ATSI 内部指针：`atsi[12]`（`+ 2 * *atsi_sectors`）、
  `atsi[28]`（`*atsi_sectors - 1`）、`atsi[196]`（`*atsi_sectors`）
- 其它文件里的扇区账：`amg2.c` 的 `sectors->atsi[..] * 2`（IFO+BUP）、
  `launch_manager.c` 的 `2 * sectors.atsi[i]`、`samg2.c` 的
  `+ sectors->atsi[0]` / `sectors->atsi[g] + sectors->atsi[g+1]`

已核对：全项目没有其它地方硬编码「3 扇区」。
"""
import pathlib
import sys

PATH = pathlib.Path("/root/dvda-author-mlp8/src/atsi2.c")

if not PATH.exists():
    print("[FAIL] 找不到 %s" % PATH)
    sys.exit(1)

text = PATH.read_text(encoding="utf-8", errors="surrogateescape")
n_ok = 0
TOTAL = 4


def rep(old, new, label):
    global text, n_ok
    if old not in text:
        print("[MISS] %s" % label)
        return False
    if text.count(old) != 1:
        print("[MISS] %s（匹配到 %d 处，需唯一）" % (label, text.count(old)))
        return False
    text = text.replace(old, new, 1)
    print("[OK] %s" % label)
    n_ok += 1
    return True


# ---- 1) 固定栈数组 -> 按曲目数动态分配 ----
rep(
    """  // PATCH 09.07  ATS_ files do not need 3 sectors in most cases

  uint8_t atsi[2048 * 3];
""",
    """  // PATCH 09.07  ATS_ files do not need 3 sectors in most cases
  // PATCH 2026-09-21  size the table from the track count (was a fixed
  //                   3-sector stack array, which capped a group at ~65 tracks)

  /* ATSI 表按**曲目数**动态分配。
     实测每轨约占 56.5 字节（读出 ATSI 内部的 i 字段），另有 2049 字节基线
     （ATSI_MAT 等占 0x800）。这里按 96 字节/轨留足余量，向上取整到扇区，
     再多留一个扇区。99 轨（MAX_TRACKS）也只分配约 12 KB 的**堆**内存 ——
     不再是固定大小的栈数组，也就不会因曲目变多而写爆缓冲。 */
  size_t atsi_cap = ((2049 + (size_t) ntracks * 96 + 2047) / 2048 + 1) * 2048;
  uint8_t *atsi = (uint8_t *) calloc(atsi_cap, 1);

  if (atsi == NULL)
    {
      perror(ERR "ATSI table allocation");
      clean_exit(EXIT_FAILURE, globals);
    }
""",
    "固定栈数组 -> 按曲目数动态分配",
)

# ---- 2) memset 用实际容量（calloc 已清零，保留以缩小 diff） ----
rep(
    """  memset(atsi, 0, sizeof(atsi));
""",
    """  memset(atsi, 0, atsi_cap);
""",
    "memset 用 atsi_cap",
)

# ---- 3) 扇区数按实际用量算 + 越界自检 ----
rep(
    """  // PATCH 09.07: i > 2048*2 instead of i > 2048

  if (i > 4096)
    {
      *atsi_sectors = 3;
    }
  else
    {
      *atsi_sectors = 2;
    }
""",
    """  // PATCH 09.07: i > 2048*2 instead of i > 2048
  // PATCH 2026-09-21: sectors are computed from the actual size, not a
  //                   hardcoded 2/3 choice

  /* 自检：分配量按 96 字节/轨算，实测只占 56.5，正常不会命中；
     命中说明每轨开销比预期大，此时宁可报错也不要静默越界。 */
  if ((size_t) i > atsi_cap)
    {
      foutput("%s%d%s%zu%s", ERR "ATSI needs ", i,
              " bytes but only ", atsi_cap, " were allocated");
      FREE(atsi)
      EXIT_ON_RUNTIME_ERROR_VERBOSE("ATSI table overflow");
    }

  /* 扇区数按**实际用量**向上取整，下限 2 扇区（ATSI 规范要求）。 */
  *atsi_sectors = (uint8_t) ((i + 2047) / 2048);
  if (*atsi_sectors < 2) *atsi_sectors = 2;
""",
    "扇区数按实际用量算 + 越界自检",
)

# ---- 4) 出口释放 ----
rep(
    """  return (nb_atsi_files);
#undef files
""",
    """  FREE(atsi)

  return (nb_atsi_files);
#undef files
""",
    "出口释放缓冲",
)

if n_ok == TOTAL:
    PATH.write_text(text, encoding="utf-8", errors="surrogateescape")
    print("\nATSI 动态分配修复完成，共 %d 项" % n_ok)
else:
    print("\n只应用了 %d/%d 项，**未写入文件**" % (n_ok, TOTAL))
    sys.exit(1)
