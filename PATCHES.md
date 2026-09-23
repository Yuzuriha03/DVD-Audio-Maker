# 源码补丁清单

`build_dvda_author_mlp.sh` 是**幂等**的：每次运行都会先把下面这些源文件从
`DVDA_AUTHOR_ORIG`（原始源码）还原，再按本清单**按顺序**重新打一遍补丁。
所以只要本清单是完整的，工具链就能从零复现。

源文件：`DVDA_AUTHOR_SRC`（默认 `tools/dvda-author-mlp8`）
原始源码：`DVDA_AUTHOR_ORIG`（默认 `tools/dvda-author`）

---

## 顺序总表

`[1b]` 还原 → `[2]` 基础 → `[3]` configure → `[4]` Makefile → `[5]` 全部源码补丁

| # | 补丁 | 目标 | 目的 |
|---|---|---|---|
| — | `patch_base.py` | 多个 | 与 FFmpeg 版本无关的基础修复 |
| — | `patch_fix_fn_strtok.py` | `auxiliary.c` | `fn_strtok()` 越界写：`--stillpics` 的空项（空串）就触发 |
| 1 | `patch_read.py` | `mlp.c` | FFmpeg 8：`channels`/`ch_layout`、`pkt_pos` |
| 2 | `patch_read2.py` | `mlp.c` | FFmpeg 8：提取分支的读取循环 |
| 3 | `patch_encode.py` | `mlp.c` | FFmpeg 8：planer 采样格式、放开 24-bit |
| 4 | `patch_ats_pack.py` | `ats.c` | pack 补到 2048 边界（否则每盘丢 1 轨）|
| 5 | `patch_atsi_dynamic.py` | `atsi2.c` | ATSI 按曲目数动态分配（一组可达 99 轨）|
| 6 | `patch_stillpics_atsi_record.py` | `atsi2.c` | 静图记录不能跳过「沿用上一张」的轨 |
| 7 | `patch_mlp_one_title.py` | `amg2.c` `ats.c` `structures.h` | 去掉「MLP → 每轨自成 title」 |
| 8 | `patch_asvs_no_buttons.py` | `asvs.c` | `0x19`「activates buttons」→ 0 |
| 9 | `patch_still_end_code.py` | `menu.c` | 静图程序结束码独占扇区 + 补 `0xFF` |
| 10 | `patch_asvs_image_sectors.py` | `asvs.c` | ASVS 每图偏移 = `base_sect` + `off_sect` |
| 11 | `patch_menu_paging.py` | `menu.c` `xml.c` | 菜单分页 |
| 12 | `patch_menu_backgrounds.py` | `menu.c` | 每页背景图 |
| 13 | `patch_menu_screentext.py` | `menu.c` | `--screentext` 解析 |
| 14 | `patch_menu_layout.py` | `menu.c` `xml.c` | 按钮矩形按页分行 |
| 15 | `patch_menu_arrows.py` | `menu.c` `xml.c` | 上/下/左/右箭头按钮 |
| 16 | `patch_menu_stillpics.py` | `menu.c` | `create_mpg` 的 `pict` 重新分配判据 |
| 17 | `patch_menu_stillpics_list.py` | `menu.c` | 文件列表模式指向 tempdir |
| 18 | `patch_menu_amg_size.py` | `amg2.c` | `sectors.amg` 按菜单页数撑大 |
| 19 | `patch_menu_amg_cells.py` | `amg2.c` | 菜单 cell 结束地址用当前页大小 |
| 20 | `patch_menu_one_album_per_page.py` | `menu.c` `xml.c` `amg2.c` `structures.h` 等 | 一页一个专辑（**必须最后**）|

### 顺序约束（改动清单前务必确认）

- `patch_menu_paging` 必须早于 `patch_menu_layout`
  —— layout 会把 `command->maxntracks` 换成 `img->maxbuttons`，
  而 `maxbuttons` 由 paging 决定。
- `patch_menu_one_album_per_page` 必须最后
  —— 它改的是 `menu.c` 的排版循环，要求前面几个补丁已就位。

---

## 有意**不**接入的补丁

放在 `patches/_experiments_18xx/`，仅供查阅：

| 补丁 | 为什么不用 |
|---|---|
| `patch_asvs_per_track.py` | 把 ASVS 记录从「按 title」改成「按轨」。三张商业盘都是**按 title**（Enigma 8 title → 8 条记录，每条「图数 = 该 title 的轨数」），改成按轨与商业盘不同构，实测也修不好上一曲/下一曲 |
| `patch_ats_ptt_srpt.py` | PTT 表结构是按 DVD-Video 猜的（参考盘上 `ATS_PTT_SRPT = 0`），实装后 **PowerDVD 直接崩溃** |
| `patch_asvs_per_image.py`<br>`patch_ats_album_rank.py`<br>`patch_ats_still_pertrack.py`<br>`patch_ats_still_manual.py` | 2026-09-22 18:04~18:32 的实验性改动（把 ASVS 改成「每图一条」等），未产出可用结果 |

---

## 「G 版本」形态（2026-09-23 回滚基准）

盘2 的 `测试_G_单记录.iso` 曾在真机（PowerDVD 8）上正常显示静图，
其形态作为**回滚基准**记录如下，重建时应能逐项复现：

| 配置项 | 取值 |
|---|---|
| `DVDA_TITLE_MODE` | **`one`**（整盘 1 个 title） |
| ATS `nr_of_titles` | **1** |
| ASVS `nr_of_asvs_records` | **1**（记录内 56 图，`base_sect=0`） |
| ATS 静图表 `f2` / `f3` | **上游常数** `6×轨数` / `(轨数-1)×6 + 15 + (图数-1)×10` |
| 静图表 `byte1` | **`0x00`**（不开幻灯片） |
| ASVS `0x0E..0x0F` | **`0x0012`**（上游原值） |
| ASVS `0x18` | `0x53`（PAL，与本工程静图制式一致） |
| ASVS `0x19` | `0x00`（`patch_asvs_no_buttons.py`，**保留**） |
| ASVS 调色板 | **上游菜单配色**（`--active*-palette` 取值） |
| 静图程序结束码 | 独占扇区 + 补 `0xFF`（`patch_still_end_code.py`，**保留**） |
| 静图 MPEG 头部 | **上游原样**：`progressive_sequence=0`、声明码率 = 编码器实际值 |
| 静图导航扇区 | **mplex 原生**（PCI 980B + DSI 1018B，含空指针） |
| 四个 IFO 规范版本号 | **`0x12`**（上游原值） |

对应的构建参数：
```
DVDA_TITLE_MODE=one bash local-bin/dvda.sh one 2
```

### 2026-09-23 回滚时停用的补丁

以下补丁都**保留在 `patches/` 目录**（有完整依据记录），但**已从
`build_dvda_author_mlp.sh` 的 `PATCH_SEQ` 里注释掉**，恢复上游原始行为。
停用依据是「回滚到 G 版本状态」—— G 版本（`DVDA_TITLE_MODE=one` +
单 ASVS 记录 + 上游静图表字段）是当时能正常显示静图的形态。

| 补丁 | 改了什么 | 为什么停用 |
|---|---|---|
| `patch_stills_per_track_rank.py` | 静图表后两字段改「按轨递进」、`byte1=0x04` | `byte1=0x04` 是**可翻页幻灯片**（`--stilloptions manual`），不是本项目要的形态；且实测未能修好静图 |
| `patch_asvs_header_mode.py` | ASVS `0x0E` → `0x0000`（照单记录商业盘） | 实测改成 `0x0000` 会**坏静图**；上游原值 `0x0012` 才是 G 版本用的 |
| `patch_asvs_palette.py` | ASVS 调色板 → `00101010 × 16` | 实测会把静图弄坏；G 版本用的是上游的菜单配色 |
| `patch_still_bitrate.py` | 静图 mpeg2enc 加 `-b 3200`（降码率） | 用户要求**高质量静图**；码率限制是为「ASVS 总量 2048 扇区上限」加的，而那个结论是在硬件加速崩溃期间测的，不可靠 |
| `patch_still_headers.py` | 静图头部 `progressive_sequence` 0→1、码率→9000 | 对齐商业盘的理论依据成立，但实测未能修好静图，一并回滚以便对照 |

> **注意**：这些补丁的「依据」章节里记录的对照数据仍然有效（都是从三张
> 商业盘上逐字节量出来的），只是**结论未获实测支持**。重新启用前请先用
> `DVDA_TITLE_MODE=one` + 上游字段做一个基线对照。

### 已回滚的构建后处理（`02_build.py`）

`build_disc()` 里有两个后处理函数，**代码保留但调用被 `if False and ...`
停用**（回滚到 G 状态）：

| 函数 | 作用 | 状态 |
|---|---|---|
| `fix_asvs_nav_sectors()` | 把 `AUDIO_SV.VOB` 每张静图的导航扇区从 mplex 的「空壳」改成商业盘形态（去掉空 DSI） | **已停用** |
| `fix_spec_versions()` | 把四个 IFO 的规范版本号对齐 Bach/李娜 的 1.1 形态（`0x11`/`0x00`） | **已停用** |

两个函数都能**幂等重跑**，需要时把 `if False and ` 去掉即可。

---

## 三处「对齐商业盘」的修复（2026-09-22）

这三处都是拿三张商业盘当基准逐字节比出来的（Enigma《15 Years After》、
李娜精选集、巴赫布兰登堡协奏曲），三者**完全一致**：

| 项目 | 商业盘 | 修前 |
|---|---|---|
| ASVS `0x19` | **0** | 1（dvda-author 硬编码，注释自称「or 0」）|
| 每静图一个 `00 00 01 B9` | 每图一个，**独占扇区** | 挤在段末扇区最后 4 字节 |
| ASVS 每图偏移表 `0x378` | 按 title 分段，`base_sect` = 该 title 全局起点、`off_sect` 段内相对 | 全局平铺、从不归零 |

### 第四处：ATS 静图记录的「按轨」字段 —— ⚠️ **已回滚，未接入**

> 2026-09-23 回滚到 G 版本状慁时停用。参见下方「有意不接入」里的记录。

巴赫《布兰登堡协奏曲》是**1 title / 18 轨 / ASVS 单记录**——与「单 title +
每轨一张图」的形态相同。其静图表（6 字节/轨）：

```
轨1: 01 04 00 6c 00 75     → 第2字段 108、第3字段 117
轨2: 01 04 00 76 00 7f     → 118、127
轨3: 01 04 00 80 00 89     → 128、137
```

当时据此推出的规律：**byte1 = 0x04**；**第 2 字段 = 6×轨数 + 10×轨序**；
**第 3 字段 = 第 2 字段 + 9**。补丁 `patch_stills_per_track_rank.py`。

**但 byte1 = 0x04 是错的** —— 它对应源码里的 `--stilloptions manual`
（"Enable browsable (manual advance) pictures"），即**可手动翻页的幻灯片**，
不是本项目要的形态。补丁已停用，后续实测也未能证明它能修好静图。

---

## 相关脚本

| 脚本 | 作用 |
|---|---|
| `build_dvda_author_mlp.sh` | 本清单的执行者（还原 + 打补丁 + configure + 编译 + 建 `menu-bin`）|
| `build.sh` | 出盘流水线（`01_prepare.py` → `02_build.py`）|
| `verify.sh` | 校验产物（轨道表 / 扇区连续 / PTS / 无损 / 菜单）|
