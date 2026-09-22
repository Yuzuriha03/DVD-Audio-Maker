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

## 三处「对齐商业盘」的修复（2026-09-22）

这三处都是拿三张商业盘当基准逐字节比出来的（Enigma《15 Years After》、
李娜精选集、巴赫布兰登堡协奏曲），三者**完全一致**：

| 项目 | 商业盘 | 修前 |
|---|---|---|
| ASVS `0x19` | **0** | 1（dvda-author 硬编码，注释自称「or 0」）|
| 每静图一个 `00 00 01 B9` | 每图一个，**独占扇区** | 挤在段末扇区最后 4 字节 |
| ASVS 每图偏移表 `0x378` | 按 title 分段，`base_sect` = 该 title 全局起点、`off_sect` 段内相对 | 全局平铺、从不归零 |

---

## 相关脚本

| 脚本 | 作用 |
|---|---|
| `build_dvda_author_mlp.sh` | 本清单的执行者（还原 + 打补丁 + configure + 编译 + 建 `menu-bin`）|
| `build.sh` | 出盘流水线（`01_prepare.py` → `02_build.py`）|
| `verify.sh` | 校验产物（轨道表 / 扇区连续 / PTS / 无损 / 菜单）|
