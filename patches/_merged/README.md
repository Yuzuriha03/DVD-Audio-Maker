# 已固化进源码树的改动

**这 24 个补丁脚本不再被构建流程执行。** 它们记录的改动已经**直接写进**
`tools/dvda-author-mlp8/` 的源码里，保留在此仅供对照、回溯与重新应用。

## 为什么改掉「每次重放补丁」

原先是每次 `dvda.sh author` 都：

1. 从 `tools/dvda-author`（上游 + core 配置）**还原** 17 个将被修改的源文件
2. 依次**重放** 23 个补丁脚本
3. 再编译

这套机制踩过的坑：

- **补丁静默失效**：`build_dvda_author_mlp.sh` 只看退出码，而补丁脚本打印
  `[MISS]` 时未必返回非零；曾有一个补丁「写在脚本里但从未编译进去」，
  连续两轮排查都以为它生效了。
- **还原会抹掉工作**：还原是 `cp -f` 无条件覆盖。凡是不在补丁链里的源码改动
  （哪怕是刚手工修好的），一次 `author` 就无声消失。
- **无法用 grep 判断现状**：要回答「这行代码现在长什么样」得把补丁在脑子里
  跑一遍，而不是直接看源码。

固化之后：源码树就是**唯一事实来源**，`grep` 到的就是编译进去的。

## 校验

`SOURCE-MANIFEST.txt` 记录 20 个改动文件的 md5。构建脚本第 `[1/6]` 步比对：

- 文件缺失 → **告警**（改动可能被覆盖）
- md5 不同 → **提示**（手工改过源码），不中止

两者都只是告警，因为手工改源码是允许的；要挡的是「改动凭空消失」。

改完源码后重新生成基线：

```bash
cd /root/dvda-author-mlp8
for f in $(git diff --name-only -- src libutils mk); do
  printf "%s  %s\n" "$(md5sum "$f" | cut -d' ' -f1)" "$f"
done
# 把输出替换进 SOURCE-MANIFEST.txt（保留开头的 # 注释行）
```

## 清单

顺序即原先的 `PATCH_SEQ` 顺序（有依赖关系时已在说明里注明）。

### FFmpeg 8 迁移（`src/mlp.c` 等）

| 补丁 | 改动 |
|---|---|
| `patch_read.py` | `channels`/`ch_layout` 字段、`pkt_pos` |
| `patch_read2.py` | 提取分支的读取循环 |
| `patch_encode.py` | planer 采样格式（`s16p`/`s32p`）、放开 24-bit |

### 通用上游缺陷（与本工程无关，但会踩到）

| 补丁 | 改动 |
|---|---|
| `patch_base.py` | 基础修复（`close_handles` 等） |
| `patch_fix_fn_strtok.py` | `fn_strtok()` 越界写 —— `--stillpics` 的空项（空串）就触发 |
| `patch_ats_pack.py` | `write_pes_padding()` 在 length 为 1~6 时一个字节都不写 → 每盘丢 1 轨 |
| `patch_atsi_dynamic.py` | ATSI 表由固定 3 扇区改为按曲目数动态分配（一组可达 99 轨） |

### 音频 / title 结构

| 补丁 | 改动 |
|---|---|
| `patch_mlp_one_title.py` | 去掉「MLP → 每轨自成 title」；`pts_shift` 遇新 title 归零 |
| `patch_stillpics_atsi_record.py` | 静图记录不能跳过「沿用上一张」的轨 |

### ASVS / 静图

| 补丁 | 改动 |
|---|---|
| `patch_asvs_no_buttons.py` | `asvs[0x19]`「activates buttons」1 → 0（三张商业盘全 0） |
| `patch_asvs_image_sectors.py` | 每图偏移 = `base_sect` + 本 title 内相对 `off_sect` |
| `patch_still_end_code.py` | 静图程序结束码 `00 00 01 B9` 独占扇区 + 补 0xFF |
| `patch_mpeg2_autodetect.py` | 制式（NTSC/PAL）与 `progressive_sequence` 改为**按实际码流自检**后如实填写 |
| `patch_asvs_nav_sectors.py` | 静图导航扇区去掉 mplex 的空 DSI —— **现已改由 C 实现**（见下） |

### 菜单

| 补丁 | 改动 |
|---|---|
| `patch_menu_paging.py` | 菜单分页（决定 `img->maxbuttons`，必须早于 `layout`） |
| `patch_menu_backgrounds.py` | `blankscreen` 不再覆盖 `--background` |
| `patch_menu_screentext.py` | 重写 `screentext` 解析（手工切分、补齐条数、size 不复用） |
| `patch_menu_layout.py` | `command->maxntracks` → `img->maxbuttons`（10 处） |
| `patch_menu_arrows.py` | 箭头页码的括号增量 |
| `patch_menu_stillpics.py` | `create_mpg` 的 `pict` 重新分配判据补 `pict == NULL` |
| `patch_menu_stillpics_list.py` | 文件列表模式指向 `tempdir` |
| `patch_menu_amg_size.py` | `sectors.amg` 按菜单页数撑大 |
| `patch_menu_amg_cells.py` | 菜单 cell 结束地址用当前页大小 |
| `patch_menu_one_album_per_page.py` | 「一页一个专辑」排版（**必须最后**） |

## `patch_asvs_nav_sectors.py` 的特殊情况

它原本打的是 `scripts/02_build.py`（注入一个 Python 后处理函数），
**不是** dvda-author 的 C 源码。2026-09-24 整理时改由 C 实现：

```c
/* src/menu.c */
static int dvda_rewrite_nav_sector(const char *path)
```

在 `generate_background_mpg()` 的静图循环里、`dvda_pad_program_end()` 之后调用
（同一循环还有 `dvda_fix_sequence_progressive()`）。等长改写（恒 2048 字节），
故 `stillpicvobsize`、ASVS 偏移表、其余任何表都不用动。

放进 C 的好处：每张静图的 mpg 自己就带着那个导航扇区，
**不必再回头读 `AUDIO_SV.IFO` 去反算扇区号**，少一层耦合。

## 从上游重新开始

```bash
git clone https://github.com/fabnicol/dvda-author /path/to/dvda-author-mlp8
cd /path/to/dvda-author-mlp8
# 按上表顺序逐个执行本目录下的补丁脚本（它们都是幂等的：
# 检测到已应用会打印 [SKIP]，匹配不到会打印 [FAIL]）
for p in patch_read.py patch_read2.py ... ; do python3 /path/patches/_merged/$p; done
# patch_asvs_nav_sectors.py 除外：它的效果现在在 menu.c 里，需手工补
```

⚠️ 补丁脚本里的路径是**写死的绝对路径**（`/root/dvda-author-mlp8/src`）。
换机器要先把这些路径改掉。

上游基线 commit：`8fca43a`（`Update tag-and-release`）。
