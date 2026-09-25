# dvda-author 工具链改动说明

本工程的改动**固化在源码树** `tools/dvda-author-mlp8` 里，由 git 提交保存。

本文档汇总每项改动的**依据**（为什么这样改、反例、实测数据），内容从原先 25 个补丁脚本的 docstring 搬运而来 —— 补丁脚本已删除，这里是它们的等价物。

| 项 | 值 |
|---|---|
| 上游基线 | `github.com/fabnicol/dvda-author` @ `8fca43a` |
| 改动集 | 源码树 `dvda-maker` 分支，或导出的 `dvda-author-changes.patch` |
| **源码** | 另在仓库镜像一份：`tools/dvda-author-mlp8/`（约 1.2 MB） |
| 改动规模 | 20 个文件，1251 插入 / 354 删除（此后又有多轮改动，见下） |

> **源码在哪读**：不想先 `git apply` 补丁就能看代码的话，直接看
> `tools/dvda-author-mlp8/` —— 那是改动后源码的**只读镜像**
> （`src/` 与 `libutils/` 下的 `.c`/`.h`，逐字节同步），
> 说明见该目录下的 `README.md`。修改请改工作机的源码树，
> 再跑 `python3 local-bin/sync_repo.py` 刷新镜像。


---

## 构建基础


### 与 FFmpeg 版本无关的基础修复（winport / libsoxconvert）


为 mlp8 构建树应用与 FFmpeg 版本无关的基础修复（幂等）。

修复项：
1. winport.h：4 参内联 close_handles 与 5 参实现同名冲突 → 改名；
   并补上 Linux 下按值传参的 close_handles 声明。
2. winport.c：实现改为按值传参的 4 参版本。
3. libsoxconvert.c：加 WITHOUT_sox 守卫与桩函数。


### fn_strtok() 空串时越界写栈


修 `fn_strtok()` 的越界写：空串时写零长度 VLA，并踩坏调用方的 `globals`。

`auxiliary.c` 的 `fn_strtok()`：

    char *s = strdup(chain);
    ...
    uint32_t j = 1, k = 0;
    int32_t cut[strlen(s) / 2];        // ← 长度为 0 的 VLA（当 chain 是空串）
    cut[0] = -1;                       // ← 越界写栈
    do  if (s[j] == delim) { cut[++k] = j; }
    while (s[j++] != '\0');            // ← 空串时从 s[1] 起，已越过 1 字节的分配
    cut[k + 1] = j - 1;                // ← 再次越界

两处缺陷：

1. **空串**（`chain == ""`）时 `strdup` 只分配 1 字节，而扫描从 `s[1]` 开始 ——
   已越过分配边界，会一路读到堆里第一个 `\0` 才停；`cut` 又是 0 长度的 VLA，
   `cut[0] = -1` 与 `cut[k+1]` 都是**越界写栈**。
   实测后果：栈上调用方保存的 `globals` 指针被踩成 `0x7ffd00000006`，
   随后 `create_stillpic_directory()` 里
   `change_directory(globals->settings.stillpicdir, globals)` 段错误。
   **崩点与原因隔了好几层，且完全取决于栈布局** —— 同一个二进制，
   gdb 下（布局不同）不崩、正常运行崩，换成别的曲目数也可能只是"偶尔"崩。

2. **`cut` 的容量不够**：条目数 = 分隔符个数 + 2。原式按 `strlen(s) / 2`
   开，对 `",,,"` 这种每字符都是分隔符的串只需 1 项却要写 5 项。
   （`--stillpics` 里 `:::` 就是这种串。）

什么时候会走到空串：`--stillpics` 用**空项表示沿用上一张图**
（`--stillpics A.jpg::C.jpg`），解析时会对每个空项调用
`fn_strtok("", ',', ...)` —— 也就是「每专辑只存一张封面」这个省 ASVS 预算的
标准做法，必然触发。实测本项目盘2（56 轨 / 39 个空项）稳定段错误。

修法：
  · 扫描前先 `strlen(s)`；空串直接跳过扫描
  · `cut` 按 `slen + 2` 开（最坏情况每字符都是分隔符，正好够）


---

## FFmpeg 8 迁移


### mlp.c 读包路径迁移到 FFmpeg 8（ch_layout / pkt_pos）


把 dvda-author 的 mlp.c 从 FFmpeg 4.x API 迁移到 FFmpeg 8.x API。

要点：
- channels / channel_layout → ch_layout
- AVFrame 的 pkt_pos / pkt_duration / pkt_size 已移除：
  改为自行记录输入包位置（MLP 扇区布局依赖 pkt_pos）
- 读包方式：av_parser_* → av_read_frame（位置信息更可靠）
- avcodec_close → avcodec_free_context
- 编码端改用 planer 格式（s16p/s32p），并允许 24-bit


### decode_mlp_file() 提取分支改用 av_read_frame


迁移 decode_mlp_file 中「提取分支」的第二个解析循环到 av_read_frame。


### 编码端迁移到 FFmpeg 8 并放开 24-bit


把 mlp.c 的编码端迁移到 FFmpeg 8 API，并启用 24-bit。

- MLP 编码器在 FFmpeg 8 使用 planer 采样格式(s16p/s32p)，
  原实现按 packed 填充 frame->data[0]，需按 plane 分别填充。
- 24-bit 原被直接拒绝；FFmpeg 8 支持 s32p，故放开并做 <<8 对齐。


---

## 音频与 title 结构


### 一个音频组只生成一个 title


让一个音频组只生成**一个 title**，从而支持「下一首 / 上一首」逐轨切歌。

## 不要按专辑切 title（实测负面结论，2026-09-22）

曾尝试在专辑边界往命令行插 `-z`，让 title 粒度跟商业盘一致
（Enigma《15 Years After》99 首 = 8 个 title，李娜精选 24 首 = 2 个 title）。
**实测结果：既无收益又有副作用，已回滚。**

  · **跨专辑连播失效** —— 一张盘播完一个专辑就停，不会继续下一个专辑；
  · **上一曲/下一曲照样坏** —— 对齐 title 粒度**并没有**修好这个问题。

所以本工程的布局固定为「**一个音频组 = 一个 title**，title 内每首歌一个 track」。
`02_build.py` 的 `build_disc()` 里注明了「不要插 `-z`」，改动前请先看那段说明。

## 症状

PowerDVD / 硬碟机里点「下一段」不能切歌：
第一首时按能跳到第二首，从第二首起再按就**回到该首开头**，永远走不到第三首。
手动选到第三、四首后按「下一段」还会跳回第二首。

## 根因

`amg2.c` 里决定「一个新 title 从哪里开始」的判断：

    if (samplerate != 前一轨.samplerate || bitspersample != ... || channels != ...
        || cga != ...
        || files[group][track].type     == AFMT_MLP     // ← 元凶
        || files[group][track - 1].type == AFMT_MLP)    // ← 元凶
      files[group][track].newtitle = 1;

紧挨着的注释写明了作者的态度：

> apparently MLP does not allow "gapless" same-audio characteristics titles, which
> means that 3 following tracks with same audio specs will create 3 titles instead
> of 1 for gapless PCM. TODO: check if this is software-dependent

也就是**上游出于「MLP 不能像 PCM 那样无缝接轨」的猜测**，让「前一轨或当前轨是
MLP」时一律另起 title。本项目全部音源都是 MLP，于是**每一首歌都自成一个
title**（实测：盘1 91 个 title、盘2 56 个 title，每个 title 恰好 1 轨；
旧的无菜单构建也完全一样，所以这不是菜单功能引入的）。

而 DVD 里「下一段 / 上一段」（下一章 / 上一章）的语义是
**在同一个 title 内前进到下一轨**。每个 title 只有一轨，播放器就无处可去，
只能退化成它自己的兜底行为 —— 表现为「按了没反应 / 跳回本首开头」。

## 修法

去掉 MLP 那两行条件，保留音频属性比较。于是一个音频组 = 一个 title，
title 内每首歌是一个 track（正是商业 DVD-Audio 盘的常规布局）：

    修前：title 1[轨1] title 2[轨2] title 3[轨3] ...   （盘2：56 title / 56 轨）
    修后：title 1[轨1 轨2 轨3 ... 轨56]               （盘2：1 title / 56 轨）

一次播放内会连续播完全组，播放器的「下一段」也就有了落点；
连带的收益是 AOB 内的间隙被消除，一张盘可以**一次连续播放到底**。

保留的属性比较仍然有意义：一个组内若中途中采样率/位深/声道数变化，
还是会正确地切成多个 title（本项目不会，分组已按属性做过）。

## 连带影响与验证

- **菜单不受影响**：菜单按钮是 `jump group G track K`，dvdauthor 侧
  `dvdvmy.y` 里 `TRACK_TOK NUM_TOK` 与 `TITLE_TOK NUM_TOK` 是同一个产生式
  （都 `|128`），VM 指令 `0x0A` 直接取 `i2-128` 当**曲目号**使用，
  与 title 粒度无关。（`dvdauthor-0.7.1/src/dvdcompile.c:994`）
- **播放封面（ASVS）预期保留**：静图机制是**按轨**工作的 ——
  ATSI 的静图记录是 `(图号, 轨号, onset)` 三元组，onset 相对各轨
  （每轨第一张图 → 0），且**只为有图的轨写记录**，没有记录的轨
  「沿用上一张」。ASVS 侧 `ntitlepics[组][title]` 会把 title 内各轨的图数
  累加，所以改成单 title 后只是把「28 条各 1 张」变成「1 条 28 张」，
  逐图扇区表（0x378）内容不变。
  但**这是推断，必须实盘验证**（PowerDVD 里逐首确认封面是否跟着变）。
- 上游那句「MLP 不允许无缝」若在**某些播放器**上成立，症状会是播放到
  轨边界出现卡顿/断音。实盘要留意。

## 连带改动：`ats.c` 必须「无条件按轨刷 pack」

只去掉 `amg2.c` 的分 title 条件会导致**段错误**（实测 `write_lpcm_header`
读 `info->mlp_layout[880418]` 越界）。因为 `ats.c` 里那个刷 pack 的块
**同时承担两件事**，而它原本挂在 `if (files[i].newtitle)` 上：

```c
if (i < ntracks)
  {
    if (files[i].newtitle)          // ← 原版每轨都成立（MLP 每轨自成 title）
      {
        write_pes_packet(fpout, &files[i-1], audio_buf, bytesinbuf, ...);
        ++pack;
        bytesinbuf = 0;
        pack_in_title = 0;          // ← 关键：每轨归零
        totpayload = 0;
      }
    files[i].first_sector = files[i-1].last_sector + 1;
```

两件事是：

1. **按轨对齐扇区**。`files[i].first_sector = files[i-1].last_sector + 1`
   只在「轨从 pack 边界开始」时才成立。不刷就会让下一轨从中途开始，
   读盘端 `get_ps1()` 取不到 pack 头 → **整首曲子被丢弃**
   （就是之前修过的那一类缺陷）。
2. **把 `pack_in_title` 归零**。MLP 的 `info->mlp_layout[]` 由
   `allocate_mlp_tracktable()` **按轨**分配，而
   `write_lpcm_header()` 用它算 PTS 偏移：

       frame_offset = info->mlp_layout[pack_in_title].pkt_pos - ...

   跨轨累加就会读到数组外面（实测 880418）→ 段错误。

所以改成**无条件按轨刷**。对 MLP 盘来说这与改动前的行为**完全一致**
（原版每轨都刷），只是标题结构从「每轨一个 title」变成「一组一个 title」，
因此 AOB 的字节内容不变，变的只有 ATSI / AMG 的标题表。

## 还原方法

本补丁改动多处（`amg2.c` 去掉 MLP 分标题条件；`ats.c` 改为无条件按轨刷 +
title 内 PTS 连续；`atsi2.c` 每轨都标记为曲目起点；`structures.h` 加字段），
把各处改回原样即可；`build_dvda_author_mlp.sh` 的 `[1b]` 会把这些文件从原始
源码还原，所以停用本脚本即可回到旧行为。

## 各处的验证状态

| 改动 | 状态 |
|---|---|
| `amg2.c` 一个 title | ✅ 已验证（`numtitles=1`、`tracks=N`） |
| `ats.c` 按轨刷 pack | ✅ 已验证（不刷会段错误） |
| `ats.c` title 内 PTS 连续 | ✅ 已验证（cell `first_pts` 递增，AOB 无回落） |

> 曾试过另一个改动（把 ATSI 时间戳记录的「轨类型」`0xC000` 补到每一轨），
> 想验证它是不是「曲目起点」位 —— **真机测试无效，已撤掉**。


---

## 上游缺陷修复


### write_pes_padding() 在 length 1~6 时一个字节都不写


修复 ats.c 的 pack 边界错位：MLP 每轨最后一个 pack 可能短 1~6 字节。

write_pes_padding() 在 length 为 1~6 时只打印一句错误就 return，**一个字节都不写**。
调用方（MLP 最后一包）传的是「补到 2048 边界还需的字节数」，该值落到 1~6 时：

  · 该轨最后一个 pack 短 1~6 字节 → 文件不再按 2048 对齐
  · 下一轨的 pack 头因此落在扇区中间（实测偏移 2042 / 2043）
  · 读盘端按 IFO 给的扇区号取该轨时，该扇区不以 pack 头开头 →
    拿不到 stream id → 整首被判为无效而丢弃
    （实测 foo_input_dvda 每张盘少 1 轨，且少的正是紧随短包头之后的那首）

原先的判据 `length > 6` 还顺带把 length == 6 也判为错误 —— 而 6 字节恰好
就是一个空 PES 填充包（3 字节起始码 + 1 字节 stream id + 2 字节长度），
本可以正常写出。故一并放开。

改动
  1. length < 6：补零到边界（与 length == 0 分支同样的处理）
  2. length >= 6：正常写 PES 填充包
  3. ff_buf 改为 length + 1 大小，避免 length == 0 时的零长数组


### ATSI 表按曲目数动态分配


ATSI 表改为**按曲目数动态分配**，并让扇区数按实际用量算。

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


---

## 静图记录


### 静图记录不能跳过「沿用上一张」的轨


候选修补：给 ATSI 的静图记录补上「沿用上一张」的引用。

> ✅ **状态：已接入 `build_dvda_author_mlp.sh` 的 `[5/8]` 步，且实盘封面正常。**
>
> 2026-09-21 用成品盘验证：播放时**每首曲子都能显示所属专辑的封面**
> （盘1 28/28、盘2 17/17 个专辑全部正确）。该构建**已包含本补丁**。
>
> ⚠️ 早先有过一段“本脚本未接入”的注释，是误写：它一直在构建流程里。
> 若日后要判断它是否**必要**，必须做对照实验（停用本补丁重建一张盘
> 再测封面），不能只看“封面能用”就下结论。

## 结构（两侧配合的关系）

- `asvs.c` 生成 `AUDIO_SV.IFO`，里面有一张**紧凑**的 title 表：每个
  「有独立图片的 title」占一条（8 字节）。本项目盘1 有 28 个专辑 →
  28 条，`totnumtitles = 28`。
- `atsi2.c` 生成 `ATS_xx_0.IFO`，其中的 stills 记录里写
  **「用第几个 title 条目」的序号**（1-based）：

      if (ntitlepics[j]) ++pictitlecount;        // 有图片的 title 累加
      ...
      atsi[i++] = pictitlecount;                // title-with-pics rank

  也就是说 title → ASVS 条目的映射由 ATSI 承担，ASVS 侧保持紧凑是对的。

## 缺陷

同一段里还有一句：

    for (r = 0; r < ntitletracks[j]; ++r)      // ← 一轨一条记录的设计
      {
        ++trackcount;
        //  This might be taken off in some unclear cases.
        if ((ntitlepics[j] == 0) && (img->npics[trackcount - 1] == 0))
          continue;                            // ← 跳过
        atsi[i++] = pictitlecount;
        ...
      }

`--stillpics` 用**空项**表示「沿用上一张图」时，那些轨的 `img->npics` 是 0
（MLP 每首歌自成 title，所以 `ntitlepics[title]` 也是 0）→ 这些轨被整个跳过。
两个后果：

1. 本 title 写出的记录数 < `ntitletracks[j]`，破坏「一轨一条记录」的格式约定；
2. 这些轨在 ATSI 里**没有任何静图引用** → 播放时不显示封面。

实测盘1：91 个 title 里只有 28 个（专辑首曲）有记录，**63 条轨没有封面**。
（`asvs.c` 那边是 28 条紧凑条目，与这 28 个 title 对应，本身没问题。）

## 修法

去掉这个跳过：只要前面已经出现过带图片的 title，就照常写入
`pictitlecount` —— 它此刻的值正是**最近一个有图片 title 的序号**，
于是这些轨复用同一张封面。`asvs.c` 里那张图本来就已存在，**不额外占扇区**。

只有当「至今没有任何 title 带图片」时才跳过（保持“完全没有封面”时的原行为）。

ATSI 因此每个 title 多写约 6 字节 × 缺失轨数（本项目盘1 约 +378 字节），
由 `patch_atsi_dynamic.py` 的动态分配吸收。


---

## ASVS / 静图


### ASVS 0x19（activates buttons）1 → 0


把 ASVS 的 `0x19`（activates buttons）从硬编码的 1 改成 0。

## 依据（2026-09-22 实测三张商业盘）

| 盘 | `0x18` | `0x19` | 结果 |
|---|---|---|---|
| Enigma《15 Years After》 | 0x53 | **0** | 正常 |
| 李娜精选集 | 0x43 | **0** | 商业盘 |
| 巴赫布兰登堡协奏曲 | 0x43 | **0** | 商业盘 |
| 本工程 | 0x53 | **1** | 上一曲/下一曲失效 |

三张商业盘**无一例外都是 0**，只有 dvda-author 写 1。

源码原文（`asvs.c`）：

    asvs[0x19] = 0x1; // activates buttons // number of menus ? // or 0

注释自己就写了「**or 0**」，说明作者也不确定；此处选商业盘一致的值 0。

## 为什么怀疑它

字段名是「**activates buttons**」。我们的静图（`--stillpics` 生成的
720×576 画面）**没有任何按钮**，声明「按钮已激活」与实物不符。
播放器若据此进入「按钮导航」模式，`上一段/下一段` 会被按钮逻辑吞掉
—— 与实测现象（曲目显示 0、上一段无反应、下一段跳回第 1 首）吻合。

## 说明

只改这 1 字节，不动其它任何字段；`0x18`（Enigma 0x53 / 李娜巴赫 0x43）
两者都有商业盘在用，故保留 dvda-author 的 0x53。


### ASVS 每图偏移 = base_sect + 本 title 内相对偏移


修 ASVS 的「每图偏移表」：必须是**相对本 title 起点**、每个 title 归零。

## 实测三张商业盘（2026-09-22）

`AUDIO_SV.IFO` 偏移 `0x378` 起是 2 字节/图的偏移表，**按 title 分段**，
每段第一项恒为 0，其后是该图**相对本 title 起点**的扇区偏移：

    Enigma:
      title1（15 图，每图 23 扇区）: 0  23  46  69 ... 322
      title2（12 图，每图 20 扇区）: 0  20  40  60 ... 220     ← 重新从 0 起
      title3（12 图，每图 19 扇区）: 0  19  38  57 ... 209     ← 又归零

各段长度恰好 = 该 title 的图数（= 轨数）。

## 我们的错在哪

`asvs.c` 里 `totpicsectors` 是**全局累加**的（ASVS 记录里的「起始扇区」
必须是全局绝对值，所以它不能改）。但它被**同一个变量**拿去写这张表：

    if (j)                                  /* 还跳过了 j==0 */
      uint16_copy(&asvs[t], totpicsectors); /* 写的是全局累计值 */

于是写出「全局绝对扇区、且从不归零」的一条平铺表：

    我们: 0 26 52 78 104 130 156 181 206 ... 1398

后果：只有 **title1**（起点恰好是 0）的取值与「相对偏移」碰巧一致；
title2 之后全部偏大 → 播放器按「图号 + 偏移」去 VOB 取图时越界/取错，
表现为**只有 title1 的封面能显示，后面的全都没有**（实测现象）。

## 修法

新增**每 title 归零**的计数器 `titlesectors`：

  · 表里写 `titlesectors`（先写后加）→ 首项自然为 0，其后为相对偏移；
  · `totpicsectors` 保持全局累加，继续供 ASVS 记录的「起始扇区」与
    文件末尾的「总扇区」使用（这两处**必须**是全局值）。


### 静图导航扇区去掉 mplex 的空 DSI


把静图的导航扇区从 mplex 的「空壳」改成商业盘形态（去掉空 DSI）。

> ## 🚫 本脚本已作废 —— 不要运行
>
> 它的效果**已经改由 C 实现**：`src/menu.c` 的
> `dvda_rewrite_nav_sector()`，在 `generate_background_mpg()` 的静图循环里
> 紧跟 `dvda_pad_program_end()` 调用。
>
> 本脚本要修改的目标（`02_build.py` 的 `build_disc()` 里的 AOB 补零段）
> **已被删除**，所以它现在必定报 `[FAIL] 找不到 AOB 补零段`。
> 下面的实现细节只是历史记录；要重新应用请照它写 C 代码，别跑这个脚本。

## 依据（2026-09-23 实测，五张盘）

`AUDIO_SV.VOB` 里每张静图的**第 1 个扇区是导航扇区**：

| 盘 | 起始码 | PCI 长度 | DSI |
|---|---|---|---|
| 巴赫 | `ba, bb@0x0e, bf@0x23, be@0x312` | 745 | ❌ 无 |
| 李娜 | 同上**逐字节相同** | 745 | ❌ 无 |
| Enigma | 同上**逐字节相同** | 745 | ❌ 无 |
| 本工程 | `ba, bb@0x0e, bf@0x26, **bf@0x400**` | **980** | ✅ **有** |

`mplex -f 8` 的手册自己就写了它写的是空壳：

> 8 - DVD (with NAV sectors). Don't get too excited. This is really a very
> minimal mux format. It includes **empty versions** of the peculiar VOBU
> start sectorsDVD VOB's include.

DSI（Data Search Information）位于扇区内**固定偏移 0x400**，装的是「前后 VOBU
的扇区指针」。mplex 写的是**全零** → 等于声明「下一个 VOBU 在扇区 0」。
实测从菜单跳到第 50 曲时播放器直接崩溃。

三张商业盘的导航扇区**各自盘内恒定**（第 1 张与第 2 张逐字节相同），
且 PCI 里只有 11 个非零字节（Enigma 只剩 1 个）→ **播放器不看 PCI 内容**，
只要导航扇区存在且不含误导性的 DSI。

## 改法

在 `02_build.py` 的 `build_disc()` 里、**打包 ISO 之前**调用
`fix_asvs_nav_sectors()`：按 `AUDIO_SV.IFO` 的 ASVU 记录（0x60 起，每条固定
8 字节）+ 每图偏移表（0x378 起，每条 2×图数，**两个独立游标**）定位每张静图的
导航扇区，整块改写成商业盘形态：

    pack_header(14B, 与本工程原本逐字节相同)
    + 系统头(15B) + PCI(745B) + 0xBE 填充包 + 0xFF 补满 2048

扇区数不变，故 ASVS 偏移表、每图大小、其它任何表都不用动。**幂等**。

⚠️ xorriso 解出来的文件是只读的（`-r-xr-xr-x`），测试时要先 `chmod u+w`。


### 静图程序结束码独占扇区


把静图 VOB 的程序结束码从「扇区末 4 字节」改成「独占扇区 + 补 0xFF」。

## 实测依据（2026-09-22）

`AUDIO_SV.VOB` 里每张静图末尾都应有一个 MPEG 程序结束码 `00 00 01 B9`。
三张商业盘都有，且**位置一致**——独占一个扇区，其后用 `0xFF` 填满扇区：

    Enigma《15 Years After》99 张 → 99 个，全部落在扇区开头（偏移 % 2048 == 0）
    李娜精选集            12 张 → 12 个
    巴赫布兰登堡          18 张 → 18 个

本工程（dvda-author + mplex）把结束码写在**段末扇区的最后 4 字节**
（偏移 % 2048 == 2044），紧随其后就是下一段静图的 pack 头：

    商业盘:  ... pack 数据 ... | B9 FF FF ... FF |   ← 结束码独占扇区
    我们:    ... pack 数据 ... FF FF FF FF B9 | BA ...   ← 挤在扇区尾

DVD 规范要求程序结束码**之后**填充到扇区边界；我们的填充在结束码之前，
等于「结束码后面直接是新的 pack」。严格解码器可能据此把两段静图当成
一个程序，从而算错「当前在第几轨」。

## 修法

对每张静图的 mpg（`generate_background_mpg` 里 `create_mpg` 之后、
`stat_file_size` 之前）后处理：

  1. 若末扇区最后 4 字节是 `00 00 01 B9`，把它们改成 `FF FF FF FF`；
  2. 在文件末尾追加一个扇区：`B9` + 2044 个 `FF`。

这样每段静图末尾就是「结束码 + 填充」，与三张商业盘一致。

⚠️ 必须在 `stat_file_size()` **之前**做，因为它决定
`img->stillpicvobsize[]`，而 ASVS 表里的每张图起始扇区就是由它累加出来的；
改完自动一致，不必另改 ASVS。


### 静图记录的两个偏移字段按轨递进


ATS 静图记录：两个偏移字段改为**按轨递进**（byte1 保持 0x00）。

## 症状

**同一张专辑内只有第一首能显示出封面，切到同专辑的其它曲目画面不刷新**
（换专辑时才刷新）。用户 2026-09-24 报告。

## 根因（2026-09-24 实测三张盘）

ATS 的静图表布局是「**先 6 字节/轨的记录，紧跟 10 字节/图的清单**」，
两种记录都相对静图表起点（描述符 `+14`）定位。每轨那 6 字节：

    [图号/最高 title 序号:1][byte1:1][本轨清单起始偏移:2][本轨清单结束偏移:2]

上游把后两项写成**与轨无关的常量**：

    uint16_copy(&atsi[i], 0x06 * ntitletracks[j]);                 /* 起 */
    uint16_copy(&atsi[i], (ntitletracks[j]-1)*0x6
                          + 0x0F + (ntitlepics[j]-1)*0xA);         /* 止 */

- `起 = 0x06×轨数` —— 只有**第 1 轨**碰巧正确
- `止 = (轨数-1)×6 + 0x0F + (图数-1)×0xA = 6×轨数 + 10×图数 - 1`
  —— 只有**末轨**碰巧正确

于是同一 title 内所有轨都声称「我的图在 [6n, 6n+10p-1]」→ 播放器逐轨查表
时取到的都是**第 1 张图**，曲目内自然不换图。

### 正常工作的商业盘是按轨递进的

| 盘 | 轨 | a | b |
|---|---|---|---|
| 李娜（2 title / 24 轨 / 12 图） | 轨1 | 72 = 6×12 + 10×0 | 81 = a+9 |
| | 轨2 | 82 = 6×12 + 10×1 | 91 = a+9 |
| 巴赫（1 title / 18 轨 / 18 图） | 轨1 | 108 = 6×18 + 10×0 | 117 = a+9 |
| | 轨2 | 118 = 6×18 + 10×1 | 127 = a+9 |
| **本工程（修前）** | 轨1..n | **36（常量）** | **95（常量）** |

即 `a(r) = 6×轨数 + 10×(前 r 轨的图数之和)`，`b(r) = a(r) + 10×本轨图数 - 1`。
每轨恰好 1 张图时退化为 `a = 6n + 10r`、`b = a + 9`。

> 第三方实现 foo_input_dvda 也按「每轨各自的起止」解读这两个字段。

## byte1 保持 0x00

源码里 `0x04` 对应 `--stilloptions manual`
（"Enable browsable (manual advance) pictures"，可手动翻页的幻灯片）。
本工程要的是「画面随曲切换、不提供翻页」，故不写 0x04。

**上游原本就是这样写的**（只在传了 `--stilloptions` 时才置 0x04），
本补丁**不改动这段逻辑** —— 实测本工程从不传 `--stilloptions`，
所以 byte1 一直是 0x00。

## 两个字段都不改表长

每轨仍是 6 字节 → ATSI 总长度不变，扇区数、`0x0804` 处的指针等一律不动。

## 验证（重建后实测）

- 17 个 title 全部变成 `a = [6n, 6n+10, 6n+20, …]`、`b = a+9`
- 与修复前对比：**78 个差异字节**（= 2×(56 轨 − 17 title)，
  即除首轨外的 a 低字节 + 除末轨外的 b 低字节），**全部落在静图表内**
- 其余 6 个系统文件（AUDIO_PP.IFO / AUDIO_SV.IFO/VOB / AUDIO_TS.IFO/BUP/VOB）
  与修复前**逐字节相同**


### 制式与逐行/隔行改为按码流自检


让 dvda-author 自己探测「制式」与「逐行/隔行」，并如实填写参数。

## 为什么要有这个补丁

有两处参数此前是**写死**的，与实际码流无关：

| 位置 | 上游写法 | 问题 |
|---|---|---|
| `asvs.c` 的 `asvs[0x18]` | `0x53`（= PAL） | 传 `-4 ntsc` 时仍写 PAL →「声明 NTSC 实播 PAL」 |
| `amg2.c` 的 `amg[0x100]` / `unknown2` | `0x53000000` | 同上（菜单区视频属性） |
| 序列扩展 `progressive_sequence` | 照抄 `mpeg2enc` 的 **0** | 静图实际是逐行单帧，头里却声明隔行 |

前两处靠「照抄商业盘」的字节补丁修过（`patch_asvs_video_attr.py`，已停用）；
第三处靠 `patch_still_headers.py` **无条件**把该位改成 1（也已停用）。
无条件改和写死一样不可靠 —— 一旦输入换成隔行素材就会写出假值。

## 本补丁的做法：探测后如实填写

`menu.c` 里新增一段码流自检，直接从**实际产出的 MPEG-2** 里读：

    序列头       00 00 01 B3   尺寸 / aspect_ratio_information /
                               frame_rate_code / bit_rate_value
    序列扩展     00 00 01 B5 + 0x1?   progressive_sequence
    图像编码扩展 00 00 01 B5 + 0x8?   picture_structure /
                               frame_pred_frame_dct / progressive_frame

由此得到两个结论：

1. **制式**：`frame_rate_code`（3/6 = 625/50，其余 = 525/60）与序列头里的
   画面高度（576/480）互相印证，冲突时以画面高度为准 —— 显示制式由它决定。
   据此算出 `video_attr` 字节：`0x43` = 525/60，`0x53` = 625/50。
   写入 ASVS `0x18` 与 AMG `0x100`（菜单区）。
   探测不到序列头时退回 `--norm` 的设置，并打 `[WAR]`。
2. **逐行/隔行**：仅当图像编码扩展表明画面确实是「整帧 + frame_pred_frame_dct
   + progressive_frame」而序列扩展却写着 0 时，才把该位写回 1。
   内容真的是隔行就一个字节都不动。

`progressive_sequence` 的改动与 `patch_still_end_code.py` 同理，**必须在
`stat_file_size()` 之前**执行，否则 ASVS 的扇区指针与实物不符。

## 只改这两个参数

序列头声明的码率**不改**：那是编码器自己的设置，本身自洽（本工程 7.5 Mbps、
商业盘 9.0 Mbps 只是两套编码参数，不是真假问题）。自检报告里会打印出来供核对。


---

## 选曲菜单


### 菜单分页：每页按钮数真的是每页容量


修复菜单分页：让「每页按钮数」真的是每页容量，而不是把总数除以页数。

原式（menu.c 两处）：
    img->maxbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) / img->nmenus;
    img->resbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) % img->nmenus;

语义应当是：maxbuttons = 每页能画几个按钮，resbuttons = 末页额外多出的几个。
但原式先把总数**截到 32** 再**除以页数**，于是无论 nmenus 设多大，
所有页加起来最多只有 32 个按钮 —— 89 首的盘只有前 32 首能进菜单，
其余全部点不到（且不报错）。

对照：xml.c 分层分支里 `maxbuttons = Min(..., ntracks[groupcount])` 才是
正确用法（一页一个组的容量）—— 可见原意就是「每页容量」，
非分层分支写错了。

改法：
    maxbuttons = Min(32, ceil(totntracks / nmenus))   // 每页容量
    resbuttons = 0                                     // 末页由循环自然截断

配合调用方传 `-6 --nmenus=ceil(总轨数/每页目标数)`，即可覆盖全部曲目。
（menu_characteristics_coherence_test 会由 nmenus 反推 ncolumns，
 故 nmenus=组数时 ncolumns 恰为 1 —— 单列长列表。）


### --background 的「每页一张」真正生效


让 `-b/--background` 的「每页一张背景图」真正生效（自动菜单路径）。

`-b` 接受逗号分隔的背景 jpg 列表（一页一张），解析后存进
`img->backgroundpic[]`。但选项解析收尾处的复制循环把**每一页**都从
`backgroundpic[0]` 复制：

    copy_file2dir_rename(img->backgroundpic[0], tempdir, "bgpic0.jpg", ...);
    for (u = 1; u < img->nmenus; u++)
        copy_file2dir_rename(img->backgroundpic[0], tempdir,
                             "bgpic<u>.jpg", ...);          // <- 又是 [0]

于是 tempdir 下 bgpic0..N.jpg 全是同一张，`-b` 形同虚设，
菜单所有页只能共用一个背景。

改法：第 u 页取 `backgroundpic[u]`；列表不够长或该项为空时回退到 [0]
（与原行为一致，不影响只用单张背景的旧用法）。

改完即可用 `-b` 给每页配不同背景 —— 例如「一页一张专辑封面」。


### 重写 --screentext 解析（原版一用就崩）


修复 `-O/--screentext`（菜单文字链）—— 当前版本一用就崩。

调用点（menu.c 的 generate_menu_pics，screentext 分支）有三处缺陷，
合起来使 `--screentext` **任何输入都段错误**：

1) `basemotif = fn_strtok(chain, '=', ..., 1, cutloop, remainder)`

   `count` 是传给 `cutloop` 的「消费几个子串」计数器，而 cutloop 是
   `++loop; if (count > loop) return 1; else { loop=0; return 0; }`
   —— 传 1 时第 1 个子串就 return 0 而 break。
   而 fn_strtok 在 break 时的语义是：`array[0..k-1]` 有效、`array[k]` 被置哨兵
   NULL、`remainder` = 第 k 个子串。
   于是 k=0：`basemotif[0]` 变成 NULL（专辑标题丢失），`remainder` 退回**整串**。

   传 2 才正确：第 1 个子串进 array[0]，break 于 k=1，
   `remainder` = 第一个 '=' 之后的部分。

2) `dim` 是 fn_strtok 输出的**槽位数**（元素数 + 1 个哨兵），
   但遍历写成 `for (k = 0; k < dim; k++)`，最后一次读到
   `grouparray[dim-1] == NULL` → `strlen(NULL)` 段错误。
   上界必须用实际元素个数。

3) `size` 被复用：既作为列宽传入 `&size` 当 fn_strtok 的输出参数，
   之后又当**字符串截断长度**用于
   `if (strlen(text) > size) text[size] = '\0'`。
   fn_strtok 会把 `*size` 覆写成子串个数（例如 3），
   于是**每个曲名被截成 3 字节**（中文只剩 1 个字）。

另外两处 `remainder`/`rem` 是不初始化 VLA，而 fn_strtok 只在提前 break 时
才写它们（子串数刚好用尽时**不写**）→ 读到未初始化栈内存。
统一先置 `'\0'`。

修好后 `--screentext "专辑=组标题=曲1,曲2:组标题2=曲3,曲4"` 才可用。


### 排版改用每页容量（修 >34 轨的栈溢出）


菜单排版改用「每页容量」，修掉 >34 轨的组必崩的栈溢出。

`command->maxntracks` 被赋成**整组最大轨数**（amg2.c:165
`command->maxntracks = MAX(track, command->maxntracks)`），
而菜单排版把它当成「一页有几行」用：

  xml.c  compute_coordinates()：
      uint16_t y0[MAX_BUTTON_NUMBER], y1[MAX_BUTTON_NUMBER];   // 36
      for (j = 1; j < command->maxntracks + 2; ++j)
          y1[j] = ...; y0[j] = ...;
      随后还用到 y0[command->maxntracks]、y0[command->maxntracks + 1]

`MAX_BUTTON_NUMBER` 是 36，所以 maxntracks >= 35 时就越界写栈数组 ——
实测 40 轨的组直接 `*** stack smashing detected ***` 中止。
（盘1 的组1 有 65 轨，必然中招；没有菜单时不走这段代码，所以以前没暴露。）

正确的「行数」应当是每页的按钮数 `img->maxbuttons`
（= Min(32, ceil(总轨数 / 页数))，见 patch_menu_paging.py），
它既受屏幕上限约束（<=32 < 34，不会越界），
又是排版真正需要的行数 —— 而且必须与 spumux 按钮坐标一致，
否则文字与按钮会错位（menu.c 画文字、xml.c 画按钮，两边共用这个值）。

本补丁把 menu.c / xml.c 里**全部** `command->maxntracks` 换成
`img->maxbuttons`。两处已核实 `img` 与 `command->img` 是同一对象：
  · amg2.c:82  `#define img command->img`
  · 两个函数都由 amg2.c 以 `img == command->img` 调用
所有这些出现位置都在菜单排版路径内，没有别的语义。


### 翻页箭头：文字错位 + 末页重复画 5 次


修菜单翻页箭头：文字错位（第 2 页起跑到页面顶部）+ 末页重复画 5 次。

## 缺陷一：箭头文字从第 2 页起全部错位

`menu.c` 画箭头文字时把 `offset` 传进了 `mogrify_img()`：

    mogrify_img(arrowstring, img->ncolumns - 1, img->maxbuttons,
                img, img->maxbuttons, command1, command2, offset, img->arrowcolor);
                                        ^^^^^^ track            ^^^^^^ offset

而 `mogrify_img()` 里是

    y0 = EVEN(y(track + 1 - offset, maxnumtracks + 4));

`offset` 的语义是「**本页首轨的全局序号**」，它是给曲名用的（让每页第 1 首
都画在第 1 行）。但箭头用的是**固定绝对行号** `img->maxbuttons` /
`img->maxbuttons + 1`（页面底部的箭头槽，与 `xml.c` 里输出的按钮坐标同源），
不该再减 offset。

于是：

| 页 | offset | `y(track+1-offset)` | 结果 |
|---|---|---|---|
| 第 1 页 | 0（初始值） | y(13) | 底部 ✔ |
| 第 2 页起 | >0 | `12+1-offset` | **顶部第 1、2 行** ✗ |
| 末页 | 0（走完整个组时被复位） | y(13) | 底部 ✔ |

实测（56 轨 5 页，逐页提取文字墨迹的行区间）：

```
页 0  墨迹 … 408-430, 441-455      ← 441-455 是底部的 Next
页 1  墨迹 … 378-400, 408-429      ← 没有 441-455！箭头被画到了第 1、2 行
页 4  墨迹 … 288-309, 441-455      ← 底部 Previous
```

也就是说**第 2 页到倒数第 2 页**，箭头文字压在最先两首曲名上，
而底部的按钮位置没有文字 —— 看起来就是「翻页后不显示 Previous/Next」。

修法：箭头调用一律传 `offset = 0`（用绝对行号定位）。

## 缺陷二：末页把同一个箭头重复画 5 次

原来是 `do { ... } while (buttons < menubuttons + arrowbuttons);`。
`buttons` 进入这个循环前是**本页已画的按钮数**，而 `menubuttons` 仍是
「满页容量」：曲目正好填满时两者正好接上，但**末页只有 8 首**（56 - 4×12）
时 `buttons = 8`、目标是 `12 + 1 = 13` → 循环 5 轮，把同一个 Previous
重复画在同一位置。

`xml.c` 里同构的循环会输出 5 个**完全重叠**的按钮，实测：

```
页 4   button09..button13 全部 y0=440..470   ← 5 个重叠按钮
```

修法：改成单次判断 —— 不该有重复。

## 附带清理

去掉 `char arrowstring[9]` + `strcpy`（`DEFAULT_PREVIOUS` 是 8 字符 + NUL = 9，
刚好塞满这个缓冲，本来就贴着边界）；直接把字面量传给 `mogrify_img()`
（该函数已有直接传字面量的用法）。

`menu.c` 与 `xml.c` 这两段必须**保持一致**（一个画文字、一个输出按钮），
所以本补丁同时改两处。


### --stillpics 与菜单共存时的段错误


修 `--stillpics`（每轨静图）与菜单共存时的段错误。

`pict` 是 menu.c 的**文件级** static 缓冲，在 `create_mpg()` 里按需分配，
分配判据是它自己的另一个 static（函数内）计数器 `s`：

    static unsigned long s;                 // 函数内 static
    ...
    if (s == 0)
      {
        s = MAX(strlen(globals->settings.stillpicdir) + 26,
                strlen(img->backgroundpic[rank]) + 1);
        pict  = calloc(s, sizeof(char *));
      }
    ...
    sprintf(pict, "%s" SEPARATOR "pic_%03u.jpg", globals->settings.stillpicdir, rank);

而 `generate_background_mpg()` 结束时会把 `pict` 置空：

    FREE(pict)        // { free(pict); pict = NULL; }

于是「先做菜单背景（ANIMATEDVIDEO）、再做静图背景（STILLPICS）」这条路径上，
第二次进入 create_mpg 时 `s != 0` 但 `pict == NULL`，
直接 `sprintf(NULL, ...)` → 段错误（实测）。

修法：把「pict 为空」也作为重新分配的判据。


### --stillpics 文件列表模式必崩


修 `--stillpics` 的**文件列表模式**（`--stillpics a.jpg:b.jpg:...`）必崩。

`--stillpics` 有两种用法：
  · 目录模式：`--stillpics <目录>`，目录里放 pic_000.jpg、pic_001.jpg …
    → 该分支会把 `stillpicdir` 设成这个目录
  · 文件列表模式：`--stillpics a.jpg:b.jpg:...`（`:` 分轨、`,` 分该轨多张）
    → 该分支**不设** `stillpicdir`

而 `create_stillpic_directory()` 一进来就无条件
`change_directory(globals->settings.stillpicdir, globals)`，
`change_directory` 在路径不存在时 `clean_exit(-1)`。

`stillpicdir` 的默认值是在 main() 里早于命令行解析就填好的
（`dvda-author.c`: `if (!stillpicdir) stillpicdir = strdup(tempdir)`），
那时 `-D/--tempdir` 还没生效，所以它是**编译期默认 tempdir**
（<cwd>/.dvda-author/temp，通常不存在）→
文件列表模式必然以
`[ERR] Impossible to cd to <...>/.dvda-author/temp.` + 退出码 255 结束。

修法：文件列表分支里把 `stillpicdir` 指到**当前生效的 tempdir**。
这同时也让两边路径一致 —— `create_stillpic_directory()` 把图片复制到
`<tempdir>/pic_%03u.jpg`（k 只在非空项递增），而 `create_mpg()` 读的是
`<stillpicdir>/pic_%03u.jpg`；两者必须同一个目录。

修好之后就能用空项表示「本轨复用上一张图」，即
`--stillpics cover0.jpg::cover1.jpg`（第 2 轨沿用第 1 轨的图）——
这是「每专辑只编一张封面」省静图预算的关键（ASVS 上限 1024 扇区/盘）。


### 按菜单页数撑大 AMG 缓冲


按菜单页数撑大 AMG 缓冲 —— 页数多时 `uint8_t amg[sectors->amg * 2048]` 会溢出。

`launch_manager.c` 里

    sectors.amg = SIZE_AMG + (globals->text ? 8 : 0)
                + (globals->topmenu <= TS_VOB_TYPE);

`SIZE_AMG` 是常量 3，于是 `sectors.amg` 恒为 4（开菜单时）。
而 `amg2.c` 的 `create_amg()` 里

    uint8_t amg[sectors->amg * 2048];        // 4 * 2048 = 8192 字节

要写**随页数增长**的菜单表（`menusector` 分支，从 `amg[0x1820]` 起）：

    页索引   : 8 * (nmenus - 1)
    每页条目 : 0x13A 字节，共 nmenus 份
    → 需要 0x1820 + 8*(nmenus-1) + nmenus*0x13A 字节

| 页数 | 需要 | 缓冲 | 结果 |
|---|---|---|---|
| 1 | ~6490 | 8192 | ✔ |
| 4 | ~7456 | 8192 | ✔（40 轨测试盘通过的页数） |
| **9** | **~9066** | 8192 | **✗ 越界约 900 字节** |

越界写的是**栈**（VLA），所以崩点离真正原因很远：实测一路跑到
`amg2.c` 的 `create_amg()` 深处才 `.data` 段错误，而前面的菜单编码、
spumux、dvdauthor 全都正常完成 —— 极难定位。

本补丁按页数把 `sectors.amg` 撑到够用。`sectors.amg` 同时决定
`sizeofamg = sizeof(amg)` 与写盘长度、以及各处扇区指针
（`2*sectors->amg + ...`、`sectors->amg - 1`、`menusector * sectors->amg`），
所以它会自动跟着调整，盘上 IFO 也跟着变大，是自洽的。


### AMG 菜单 cell 结束地址用错大小


修 AMG 菜单 table 里每个菜单的 cell 结束地址用错大小（翻页跳转失效）。

## 症状

菜单里「Next / Previous」翻页按钮按了无效、回不到上一页（**且不止一页**），
而**选曲按钮正常**。

这个「只有翻页坏、选曲不坏」的不对称正是线索：
- 选曲按钮走 `jump group G track K` → 经 ATSI 定位到音频区，与菜单表无关
- 翻页按钮走 `jump menu N` → 由播放器查 **AMG IFO 的菜单 PGC 表**，而那张表是
  `amg2.c` **手写**的（逆向出来的结构）

## 根因

`amg2.c` 的菜单表里，每个菜单的 cell 结束地址这么算：

    if (j > 1) menuvobsize_sum += img->menuvobsize[j - 2] - 1;   // 累加「前面」各页
    uint32_check(&amg[i], menuvobsize_sum + img->menuvobsize[img->nmenus - 1] - 1 - 1);
                                            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ 恒为**最后一页**的大小

`menuvobsize_sum` 是前面各页的累计，但加上去的是 `menuvobsize[nmenus-1]` ——
**最后一页的大小**，而不是当前这一页。于是：

- `nmenus == 1` 时它恰好就是当前页 → 正确（这是它一直没被发现的原因）
- `nmenus > 1` 时，第 1..n-1 页的结束地址全错

而且**错误会被各页大小相近掩盖**：本项目 8 页的 VOB 分别是
37/40/37/43/39/39/41/39 扇区，算出

    正确： [35, 74, 110, 152, 190, 228, 268, 306]
    实际： [37, 73, 112, 148, 190, 228, 266, 306]
                                  ^^^  ^^^            ^^^
                                  巧合相同

⇒ **第 5、6、8 页恰好正确，其余 5 页错误**，与「不止一页有问题」完全吻合。

在成品 IFO 里核对（`AUDIO_TS.IFO` 的菜单表，8 个地址等间距出现）：

    代码写出的值 37/73/112/148/190/228/266/306   ← 全部在 IFO 里找到
    正确值       35/74/110/152/268               ← 一个都不存在

## 修复

把 `img->menuvobsize[img->nmenus - 1]` 换成 `img->menuvobsize[j - 1]`
（当前页），其余算式不动 —— `nmenus == 1` 时行为完全不变。

修正后的算式等价于「累计到当前页为止的有效扇区数 - 1」：

    期望 = Σ_{i<j} (sizes[i] - 1) - 1

（减 1 是 dvdauthor 处理 topmenu 时会丢一个数据扇区，原注释已说明）

起始地址一侧不需要改：cell j 的 start 用的就是 `menuvobsize_sum`，
而 cell j-1 的 end + 1 恰好等于它 —— 两边自洽。

顺带加两条自检（`globals->debugging` 时打印），以及 `verify_menu.py` 里的
**cell 连续性校验**：`start_j == end_{j-1} + 1` 且 `start <= end`。


### 「一页一个专辑」排版


把选曲菜单改成「一页一个专辑」：大标题 = 光盘标题，小标题 = 专辑名。

## 原来的行为

菜单文字链 `--screentext` 的每一段对应一个**音频组**，而分页是
dvda-author 自己按「逐页填满 R 行」算的（R = ceil(总轨数/页数)）。
于是一页里会混进多张专辑的曲子，只能给每首曲子加「专辑名 | 」前缀来区分。

## 目标

    大标题（每页都有） = 光盘标题
    小标题（该页顶部） = 专辑名
    一页只放一个专辑；专辑换了就自动换页
    每页背景 = 该专辑的封面（素材侧改，见 menu_assets.py）

## 为什么能这么做

`menu.c` 在 `ncolumns == 1` 时的画页循环是

    do {
      mogrify_img(grouptext[groupcount][0], ...);            // 小标题
      offset = track;
      do { mogrify_img(tracktext[groupcount][track], ...); track++; }
      while ((buttons < menubuttons) && (track < ntracks[groupcount]));
      if (track == ntracks[groupcount]) { group++; groupcount++; track = 0; }
      else break;                                            // 本段没画完 → 换页
    } while ((group < img->ncolumns) && (groupcount < ngroups));

也就是**一次只画一个文字段，段内曲子画完才换页**。所以只要让
`--screentext` 每段 = 一个专辑，页数就等于专辑数，新专辑自动换页。
`albumtext`（第一个 `=` 之前）由 `prepare_overlay_img()` 画在**每一页**顶部，
正好当大标题。

## 需要改的三件事

1. **页与音频组解耦**。按钮是 `jump group G track K`，G/K 是
   「音频组 + 组内曲目号」，与「页」无关。一页一个专辑后页号 != 组号，
   必须把每页换算回 (组, 组内首轨)。新增 `compute_menu_pages()` 做这件事：
   解析 `--screentext` 得到每页曲目数，再按音频组的轨数把它切成
   「页 → (组, 组内首轨)」。**任何一步对不上就整体放弃**（返回 0），
   调用方退回旧行为 —— 宁可保持可用的旧排版，也不要画出错位的按钮。

2. **每页行数要按该页实际曲目数取**。原式 `maxbuttons` 是
   `ceil(总轨数/页数)` 一个全局值，各专辑曲目数不同就必然有页画不下或
   留空。行数同时决定文字行距（`mogrify_img` 的 `maxnumtracks`）与
   按钮矩形（`compute_coordinates` 用 `img->maxbuttons`），所以两边都用
   同一个按页取值即可保持一致。

3. **`ntracks[]` 的语义改为「每页曲目数」**。`menu.c`/`xml.c` 里的
   `ntracks[k]`（k 是文字段号）本来就该是「第 k 段的曲目数」，
   只是原先段 == 音频组才恰好一致。现在由 `amg2.c` 传入按页的数组。

## 为什么必须校验而不是「大致对上」

按钮错位**不会报错**：只是点一首放成另一首。所以所有前提
（页数 == `--nmenus`、页曲目数之和 == 音频组轨数之和、每页不跨组、
每页不超过屏幕行数）都显式检查，任一不满足就整体回退。

## 回退

停用本脚本（并从 build 脚本的补丁列表里去掉）即可回到旧排版；
`[1b]` 会把 menu.c / xml.c / amg2.c / structures.h 从原始源码还原。

---

# 附：原先的补丁机制（已停用）

改动固化进源码之前，本工程用 25 个 Python 补丁脚本「重放」到上游源码上。该机制已于 2026-09-24 停用，**改为直接提交源码树的 git**（见文首）。
下面几节解释了当时踩过的坑，以及**为什么改成 git**。

## ⚠️ 对已固化的源码树跑这些补丁，**大部分会报 MISS —— 这是正常的**

这些补丁是**按顺序**应用的，后一个补丁往往会改写前一个补丁的落点。
所以拿它们逐个去跑已经全部应用完的源码树时，前几个的 `old` 早就被后面的
补丁改掉了 → 报 `[MISS]`。这不代表改动丢了。

**判据不看这些输出，看源码本身**。实测（2026-09-24）25 个补丁里
18 个报 `[SKIP] 已应用`、7 个报 `[MISS]`，但逐个在源码里查证，
**7 个的改动全部都在**：

| 报 MISS 的补丁 | 源码里的证据 |
|---|---|
| `patch_asvs_image_sectors.py` | `asvs.c:162` `uint32_copy(&asvs[k], totpicsectors)`（base_sect 全局）；`:167` `titlesectors`（off_sect 相对） |
| `patch_ats_pack.py` | `ats.c:300` `if (length < 6)` 补零分支；`:323` `uint8_t ff_buf[length + 1]` |
| `patch_atsi_dynamic.py` | `atsi2.c:193` `atsi_cap = ((2049 + ntracks*96 + 2047)/2048 + 1) * 2048`；`:194` `calloc(atsi_cap,1)` |
| `patch_menu_paging.py` | `menu.c` 里 `compute_menu_pages`（2 处） |
| `patch_menu_backgrounds.py` | `command_line_parsing.c:199` `cli_background_list`；`:2517` 置位；`:2704` 用它 |
| `patch_menu_screentext.py` | 同上那片已改写 |
| `patch_asvs_nav_sectors.py` | `menu.c` 的 `dvda_rewrite_nav_sector()`（**它的效果现在在 C 里**，脚本本身已作废 —— 它要改的 `02_build.py` 代码段已被删除） |

**最强判据还是 `.o` oracle**（下一节）：18 个 `.o` 与参考逐一比对，
指令序列 **18/18 零差异** —— 说明当前源码与产生可用 ISO 的那份源码功能一致。

---

## ⚠️ 补丁必须幂等 —— 这里有三个曾经不是

**插入式**替换的 `old` 在插入之后**依然存在**（新内容插在它之前），
所以不能用 `old not in text` 判断「是否已应用」—— 那样每次运行都会**再插一份**。

实测（2026-09-24）三个补丁犯过这个错：

| 补丁 | 表现 |
|---|---|
| `patch_read.py` | `g_last_pkt_pos` 声明与 `g_last_nb_samples` 语句各叠了 3 份 |
| `patch_encode.py` | 同类插入重复 |
| `patch_menu_amg_size.py` | `NEW = OLD + 增长块`，OLD 永远匹配 → AMG 增长块叠了 3 份 |

**判据**：`if new in text: SKIP`，而不是 `if old not in text: MISS`。
三处都已修正，并用「快照 → 跑一遍 → 比对 md5 → 还原」的方法复测：
**25 个补丁全部幂等**（对已固化的源码树运行不产生任何字节变化）。

```bash
# 复测方法（可复用）
cd tools/dvda-author-mlp8
find src libutils -type f \( -name '*.c' -o -name '*.h' \) | while read -r f; do
  mkdir -p "/tmp/snap/$(dirname "$f")"; cp -p "$f" "/tmp/snap/$f"; done
for p in scripts/patches/_merged/*.py; do
  before=$(find src libutils -name '*.c' -o -name '*.h' | xargs md5sum | sort | md5sum)
  python3 "$p" >/dev/null 2>&1
  after=$(find src libutils -name '*.c' -o -name '*.h' | xargs md5sum | sort | md5sum)
  [ "$before" != "$after" ] && echo "不幂等: $p"
  # 不等就还原，再继续
done
```

> 这些补丁**不参与构建**，只在「从上游重建」时手工执行。
> 但即便是手工执行，不幂等也足以毁掉源码树 —— 所以必须修。

---

## ⚠️ `SOURCE-MANIFEST.txt` 用 `.o` 作 oracle 的验证法

源码树改动后，怎么确认「改对了」？除了比对 `SOURCE-MANIFEST.txt` 的 md5，
更强的手段是**用编译产物反查**：

```bash
# 1) 改源码前先备份全部 .o
mkdir -p /tmp/ref_o && cp src/*.o /tmp/ref_o/

# 2) 改完源码后重编，逐个比对**指令序列**（忽略 DWARF 行号）
objdump -d --no-show-raw-insn /tmp/ref_o/foo.o | tail -n +3 > /tmp/a.dis
objdump -d --no-show-raw-insn src/foo.o          | tail -n +3 > /tmp/b.dis
diff /tmp/a.dis /tmp/b.dis      # 无输出 = 功能等价
```

要点：

- **必须用一样的编译命令**。手工在 `src/` 里 `make` 与
  `build_dvda_author_mlp.sh` 的 flags 不同（少了
  `-Wno-error=incompatible-pointer-types` 等），会产生假差异 ——
  实测 `ats.o` 因此报了 9990 行差异，**重跑真实构建后为 0**。
- `.o` 的 md5 **不可复现**（DWARF 含行号），所以要比**指令序列**而不是 md5。
- 也可反过来用：源码丢了但 `.o` 还在时，靠它把源码**精确重建**出来
  （改注释不影响指令序列，只影响行号）。

---

## 一级菜单（专辑索引页）：缩略图网格 + `--index-pages`

**需求**：选曲前先来一屏专辑封面缩略图，点哪张进哪个专辑的选曲页。

### 为什么不用上游的 `--menustyle hierarchical`

上游确实有分层菜单（`img->hierarchical`，`nmenus = ngroups + 1`），
但它**按音频组分层**。本工程每张盘只有 **1 个音频组**（曲目参数统一后
自然合并），所以那个机制只会给出 2 屏，没有意义。故按**专辑**自己做一层。

### 页序与映射（纯位置，不必传表）

```
页序： [索引页 0..I-1] [专辑页 0..A-1]

索引页 p 的第 k 格 → 专辑号 a = p * INDEX_PER_PAGE + k
                  → 菜单号（1-based）= index_pages + p * INDEX_PER_PAGE + k + 1
```

C 侧只需要一个数字 `--index-pages I` 就能算出全部跳转目标。
`INDEX_PER_PAGE = INDEX_COLS * INDEX_ROWS = 4 * 3 = 12`。

### 关键实现点

| 位置 | 做法 |
|---|---|
| `command_line_parsing.c` | 新增 `--index-pages N`（选项号 43）→ `img->index_pages` |
| `command_line_parsing.c` | 新增 `--index-covers`（选项号 44）→ `img->indexcovers`（扁平列表，页序 × 格子序） |
| `structures.h` | `pic` 末尾追加 `index_pages`、`indexcovers`、`indexcoverssize`（`pic` 按位置初始化，必须放最后） |
| `menu.c` `compute_menu_pages()` | 前 N 段是索引页：格子数**不计入** `total`（`total` 要与音频总轨数相等）；这几页 `page_group/t0` 置 0 |
| `menu.c` `dvda_make_index_pages()` | **画面在这里现画**（见下节）：背景渐变 + 网格 + 暗角、4x3 封面拼贴、专辑名、缩略图描边 |
| `amg2.c` `create_topmenu()` | 在 `compute_menu_pages()` 之后、`generate_background_mpg()` 之前调用上者 |
| `command_line_parsing.c` | 背景拷贝**跳过**索引页（那几页的画面由上面现画）；`--background` 的语义变成「只覆盖非索引页」 |
| `menu.c` `generate_menu_pics()` | 索引页**不画文字**，只逐格画按钮描边 + 底部箭头 |
| `xml.c` | 索引用 `jump menu T`；spumux 用**网格矩形**，不能走 `compute_coordinates()` |
| `menu_assets.py` | 只提供**素材**：`MenuPlan.index_covers` 铺平封面路径驱动 `--index-covers`，专辑名走 `--screentext` |

### 画面由 C 现画（`dvda_make_index_pages()`）

**为什么搬进 C**：以前画面是外部脚本用 ImageMagick 拼好、经 `--background`
传进来的。那样几何常量存在**两份**（`menu.h` 与脚本各一份），改一处忘另一处
就会「点到的不是想选的那张」。搬进 C 之后几何的**唯一来源**是 `menu.h`。

素材分工：
- **封面路径** ← `--index-covers`（逗号分隔的扁平列表，顺序 = 页序 × 格子序）
- **专辑名**   ← `--screentext`（索引页那几段的 `标签=名字1,名字2,...`）

画面 = 三层背景 + 每格 [封面 + 专辑名] + 缩略图描边：

```
背景   对角渐变（左上青蓝 → 右下深靛）+ 与格子对齐的细网格 + 径向暗角
格子   封面缩到 INDEX_THUMB 见方居中；下方 INDEX_LABEL_H 放专辑名
名称   白字 + `caption:` 自动换行；字号按 index_label_units() 估
描边   每个格子描一圈深灰，把封面从背景里「托」出来
```

专辑名的字号在 C 里估：`index_label_units()` 按字符数（全角 10、ASCII 5，
UTF-8 首字节判断），再 `size = 10 * INDEX_LABEL_W / units` 夹到
`INDEX_LABEL_FONT_MIN..MAX`。`caption:` 自己也会换行，所以估偏只是行数变多、
字被缩小居中，**不会溢出格子**。

#### ⚠️ 四个踩过的坑

1. **`-stroke` / `-fill` / `-compose` 都是「粘住」的**。它们是**设置**而不是
   算子，会一直生效到被改掉。`-compose multiply` 用完不复位时，后面的
   `-flatten` 也按 multiply 合成 —— 整页被压暗、封面与背景相乘
   （实测整页均值从 ~60 掉到 44、封面从 ~116 掉到 ~40）。所以画完暗角
   必须 `-compose over`；画文字前必须 `-stroke none`。
2. **`-repage` 必须写在 `( )` 内部**。它是**算子**，不加括号会对列表里每一张
   生效（包括开头那张底色），结果底色被挪到最后一格、画布露出 `-flatten`
   的默认白底 —— 整页只剩右下角一张图。
3. **描边要单独一遍画在 `-flatten` 之后**。`-draw` 会作用到列表里的每一张，
   拼在一起画时边框会被后续叠加顺序盖掉、或落到某张小图自己的坐标系里。
4. **封面的下标是全局连续的**：`--index-covers` 是扁平列表，每页从 0 重来的话
   第 2 页会重复第 1 页的封面（实测两页画面完全相同）。

#### ⚠️ 生成命令必须检查**退出码**

上游到处都是 `if (system(...) == -1)` —— 那只在 **fork 失败**时为真。命令
本身失败（参数错、文件读不到）时返回的是 `状态 << 8`，会被当成成功。
所以我们出过「convert 静默失败、背景图不存在，只看到后面一句
『背景图读不到』」的情况。`run_convert()` 现在检查
`WIFEXITED && WEXITSTATUS == 0`，并把失败的命令原样打出来。

配错参数时最典型的一条：`xc:rgb(62,107,138)` 里的括号是 shell 的语法字符，
**不加引号就是 `syntax error near unexpected token '('`** —— 所以所有
`rgb(...)` 都要走 `cs_arg()` 加引号。

### 几何（唯一的定义在 `menu.h`）

```
屏幕 720x576
   0 .. 59     大标题带（dvda-author 的 prepare_overlay_img() 画，墨迹 y=28..54）
  60 .. 480    4x3 网格：行高 140（60 + 3*140 = 480）
  65 .. 195    格 0：缩略图 100x100 @(40,65)，专辑名 170x28 @(5,167)
 496 .. 552    翻页箭头带（INDEX_ARROW_Y0/Y1）

格子(col,row) 内容区 = (col*180+5, 60 + row*140+5)，尺寸 170x130
```

- **顶部 60 px 是大标题带**：标题由 `prepare_overlay_img()` 在**每一页**
  画在 y≈28..54。网格从 0 开始就会被标题压住（实测过：标题墨迹
  362x26 在 (47,28)，而缩略图从 y=5 开始）。
- 这些常量**只在 `menu.h` 定义一份**。`menu_assets.py` 里还有一份，但
  只用于两件事：算「一页放几张专辑」（`INDEX_PER_PAGE`）和**校验**
  （`verify_menu.py` 按它独立采样像素，核对 C 画出来的位置）。
- 网格底 = 480，箭头在 496..552，两者不相交。

### 文字样式与选中指示

**所有文字一套样式**：白字身 + 在 (+2,+2) 偏移处再画一遍黑字
（= 描边/阴影），与索引页专辑名一模一样（那边是 Python 用 `caption:`
画两遍，这边是 C 用 `-draw "text"` 画两遍）。偏移量 `TEXT_SHADOW_DX/DY`。

**两种状态下文字完全相同** —— 选中与否只靠**行左侧的小三角箭头**表示。

这不是偷懒，是 DVD 子画面的硬约束逼出来的：

| | |
|---|---|
| 整幅子画面只有 **4 个调色板项**（第 4 个要留给透明） | `subgen.c` 里 `s->masterpal[]` 上限 4 |
| spumux 每个**按钮**的调色板也只有 **4 项** | `subgen-image.c` 的 `checkcolor()`：`if (p->numpal == 4) return false;` |
| 颜色取自**图像层各像素自己的颜色** | 同层内没法用第二种颜色 |

所以「未选中黑描边、选中变红描边」是不可能的：描边是同一段文字偏移 2px，
与字身必然重叠，重叠处会多出 `白字身 + 红描边` 这种组合。实测每个按钮的
三色组合涨到 **5 种** → `pickbuttongroups()` 全部失败 →
`ERR: Cannot pick button masks` → `assert(useimg)` 中止 → 菜单整页丢失。

改法：文字在两层的颜色**完全一致**（黑描边 + 白字身），红只用在
**与文字不相交**的图形上：

| 动画层 | 内容 | 颜色 |
|---|---|---|
| `impic` | 全部文字（白字身 + 黑描边） | 白 / 黑 |
| `hlpic` | 同上 + 行左侧的**红箭头** + 索引页格子描边框 + 翻页箭头文字 | 白 / 黑 / 红 |
| `slpic` | 同 `impic`（`-pixel` 换成等价写的白，见下） | 白 / 黑 |

三色组合恰好 4 种：`透明` / `白字` / `黑描边` / `红箭头`。实测 19 页
每个按钮都是 4（≤ 4 通过）。

箭头几何（`menu.h`）：`TEXT_ARROW_W/H/GAP`，左顶点在
`x0 - GAP - W`；`ncolumns=1` 时 `x0=45` → 左顶点 34，按钮矩形从 33 开始，
正好在里面且与文字（从 45 起）不相交。

#### ⚠️ 两个踩过的坑

**1. `-stroke` 在 ImageMagick 里是「粘住」的**

`mogrify_thumb()` 用 `-stroke "rgb(红色)"` 画描边格子，而这个设置**会一直
生效到被改掉为止**。`-draw "text"` 是**同时用 `-fill` 和 `-stroke`** 画的，
于是后面的文字全被 6px 宽的红色描边糊住 —— 图上看到的「红字」其实是
「黑字 + 红描边」。修法：每条文字/多边形绘制前显式写 `-stroke none`。

**2. `snprintf` 格式串与参数对不上 → 段错误**

给几处 `snprintf` 插入 `-stroke none` 时忘了同步补 `%s`，参数表整体错位，
`(int) floor(...)` 落到了 `%s` 上（int 当指针解引用）→ `SIGSEGV`
（`dvda-author` 退出码 -11），日志里只有半截 mplex 输出、没有任何报错。
改这种格式串后必须逐个数 `%` 与参数（或看 `-Wformat` 警告）。

#### 标题（光盘标题）的描边要追加到 `command1`

标题烘在 `svpic.png` 里、被复制到三层，所以要在它旁边加描边只能额外往
高亮层画一遍。**不能**在 `prepare_overlay_img()` 里直接改
`img->highlightpic[menu]` 文件 —— `generate_menu_pics()` 紧接着会
`copy_file(impic, hlpic)` 把它整个覆盖掉。而且 `command1` 在那句之前被
`snprintf(command, ...)` 重置过一次，追加必须在重置之后。

### 索引页的 Next 只在自己几页之间翻

索引页的 Next **不能**用它跳到专辑内容页（去专辑靠点缩略图，用 Next 会
猜错用户想听哪张）。所以 `DVDA_HAS_NEXT()`（`menu.h`，`menu.c` 与两个
XML 共用）把它限定为 `menu + 1 < index_pages` —— **最后一个索引页没有
Next**。Previous 仍是「不是第一页就有」。
槽 1 在「没有 Next 但有 Previous」时放 Previous，不会留下空槽。

### 二级页的「返回专辑索引」按钮（Menu）

二级（选曲）页底部多一个槽位，内容固定是 `jump menu 1`（第一个菜单页
就是索引页 1）。它**只在存在索引页**（`--index-pages > 0`）时出现 ——
没有索引页就无处可回；索引页自身也不画（那一页就是索引）。

槽位顺序（与其后三个位置的编号顺序必须一致）：

| 槽 | 文案 | 跳转 | 出现条件 |
|---|---|---|---|
| 1 | `Next` | `jump menu menu+2` | 非末页 |
| 2 | `Previous` | `jump menu menu` | 非首页 |
| 3 | `Menu` | `jump menu 1` | 二级页且 `index_pages > 0` |

行号 = `MENU_BUTTON_ROW(img)` = `maxbuttons + 2`。**三处必须一致**：

* `menu.c` `generate_menu_pics()` —— 画文字（`mogrify_img` 的 track）
* `xml.c` dvdauthor XML —— 出 `jump menu 1`
* `xml.c` spumux XML —— 出按钮矩形

因此 `xml.c` 的 `compute_coordinates()` 要把 y 填到 `maxbuttons + 2`；
少填一行就会读到未初始化的坐标（历史上正是这种坐标让 spumux 报
"Button coordinates out of range" 并输出 0 字节）。

⚠️ 二级页的箭头文字是竖着堆在**左下角同一列**（`x0[0]=33`）：
`y()` 给每行 48 px，所以 `Next`/`Previous`/`Menu` 分别在 y≈384/432/480，
互不重叠。想换布局就改 `x0[img->ncolumns - 1]` 的取值 ——
但必须与 spumux 的 `x0[]` 同步。
- 改动这些常量要**同时改三处**：`menu.h`（定义）、`menu.c`
  （`mogrify_thumb()` 画描边框）、`xml.c`（spumux 按钮矩形）、
  以及 `menu_assets.py`（背景图）。不一致就会出现「点到的不是想选的那张」。

### ⚠️ 踩过的三个坑（都很难查）

**1. 索引页的翻页箭头拿到了未初始化的坐标**

翻页箭头的代码在 `if/else` 链**之后无条件执行**，用的是
`x0[]/y0[]/x1[]/y1[]` —— 那几张数组由 `compute_coordinates()` 填，
而索引页根本不调它（网格不用「按行分行」）。结果是垃圾值：

```
ERR: Button coordinates out of range (720,576): (33,10944)-(708,35056)
spumux: subgen-image.c:901: imgfix: Assertion `useimg' failed.
```

spumux 直接失败、`topmenu0/topmenu1` **输出 0 字节**，于是
`menuvobsize[0..1] = 0` → `create_amg()` 里
`cell_end = sum + 0 - 2` **下溢成 0xFFFFFFFE** → IFO 的 cell 地址链坏掉。

**修法**：网格只占上面 3 行（432 px），**底部 144 px 留给箭头带**；
箭头用**绝对坐标**（`INDEX_ARROW_Y0/Y1`、`INDEX_PREV_X0/INDEX_NEXT_X0`），
`menu.c` 与 `xml.c` 两侧都用同一套常量。

**2. 格子尺寸不能写 `norm_x / COLS`**

`norm_y / INDEX_ROWS = 576/3 = 192` 会把整幅画面均分，而网格只占 432 px。
必须用**显式常量** `INDEX_CELL_W=180` / `INDEX_CELL_H=144`，
否则按钮区与 Python 生成的缩略图错开（实测 y1 差 48 px）。

**3. 别把 `12` 硬编码进跳转公式**

改每页格数时容易漏掉某处。实测 `menu * 16` 没跟着改：
索引页 2 的第 2 格算出 `tgt = 20 > nmenus(19)` → `break`，
只输出 1 个按钮，而 spumux 侧输出 5 个 →
自检 `按钮数一致（跳转 == 位置）` **抓住了它**：

```
[菜单][FAIL] 按钮数不一致（跳转 vs 位置）：第 2 页: 3 vs 7
```

### 格子里的内容（缩略图 + 专辑名）

专辑名**不交给 C 侧画**，而是由 Python 连同缩略图一起烘进背景 jpg。
理由：名称是静态的，而且需要「按实际排版换行 + 不够宽就缩字号」这种
逐格排版；C 侧的 `mogrify_img()` 是按**行**布点的，套不到网格上。

格子（`col*180+5, 60 + row*140+5` 起，170x130）内部：

```
┌─ 170 ──────────────────┐
│   缩略图 100x100（水平居中，左缩进 40）  │  100
│                （2 px 间隙）                │    2
│   专辑名 170x28，居中，自动换行 + 缩字号    │   28
└────────────────────────┘  合计 130
```

- 封面实测全是 **3000x3000**，所以缩略图取**正方形且不裁切** ——
  若铺满 170x130（1.3:1），正方形封面会被裁掉上下约 23% 的高度。
- **按钮区 = 整个格子内容区**（名称也在里面），由 C 侧输出，所以
  缩略图/名称的尺寸随便改，**不会**影响点击区。
- 换行/缩字号用 ImageMagick 自己的 `caption:` 排版来量（`-size Wx` 高度
  自适应），再逐级降字号，取第一个「换行后高度 <= 28」的。
  按字符数估宽度对中英混排（`Lulala! Lululala!`）必错。

实测本项目 45 张专辑：44 个在 **17 pt** 单行放下，只有
`星炬不熄 [毕业合唱 Version]` 降到 13 pt，没有截断。名称条只有 28 px 高，
**装不下两行**（两行至少 30 px），所以换行只在名称特别长时才发生；
连 9 pt 都塞不下时会截断加省略号（不让 `caption:` 把下半行裁掉）。

### ⚠️ `mogrify` 命令串末尾少一个空格 → 静默什么都不画

索引页的描边框（`mogrify_thumb()`）和翻页箭头（`mogrify_arrow_abs()`）
是把片段 `strcat` 到 `command1`/`command2` 上，最后再接输出文件路径。
格式串末尾忘了空格，两者就粘成一个参数：

```
… -draw "rectangle 545,293 715,427""/path/hlpic0.png"
```

mogrify 拿不到输出文件 → 把结果写到 stdout → **退出码 0**、文件不变。
症状：**按钮框和箭头全不出现，而日志一切正常**（按钮数自检只查 XML
编号，查不到画面）。

专辑页没事，是因为 `mogrify_img()` 的格式串末尾本来就有空格。
修法：两个函数末尾都加空格，并向 `mogrify_img()` 看齐。
`02_build.py` 的 `check_menu_overlay()` 现在把它做成构建期自检：
**每页 `hlpic<N>.png` 的墨迹必须多于 `impic<N>.png`**，
索引页的箭头带还必须有墨迹。见 `docs/TROUBLESHOOTING.md` 第 22 节。

### ⚠️ `-repage` 是**算子**，不在括号内就会毁掉整页
生成网格用的是「逐层 `-repage +x+y` 定位，最后 `-flatten`」。
关键：**`-repage` 必须写在 `( )` 里面**。

它是 IM 的**算子**（operator）而不是设置项，所以不加括号时会对
「当前图像列表里的**每一张**」生效 —— 包括开头那张 720x576 的黑底色。
结果黑底也被挪到最后一格的位置，画布露出 `-flatten` 的默认**白底**：

```
症状：整页只有右下角一张封面，其余全白（那道白不是封面，是 flatten 的默认底）
     整页均值 = 238（正常 ~86）
```

踩了**两次**：第一次是缩略图层，第二次是加专辑名时的 `caption` 层
（写成了 `... caption:名字 ) -repage +5+111`，括号提前闭合）。
`magick` 对这两种写法**都不报错**，只能靠量像素发现。

### 验证

```bash
source local-bin/env.sh && python3 scripts/verify_menu.py
```
disc2（17 专辑 / 56 轨）实测：

```
[OK] 菜单页数 = 19（期望 19）          ← 2 索引页 + 17 专辑页
[OK] 叠加图自检通过（19 页：高亮层都有按钮框/下划线，2 个索引页有翻页箭头）
[菜单] 按钮数一致（19 页，跳转 == 位置，17 个专辑页都带「返回索引」）✔
[OK] 翻页链路：19 页 cell 地址连续（跨度 27/28/41/41/43/...,  末页 end=746）
     地址链 0→746 与 AUDIO_TS.VOB 扇区边界逐页吻合
[OK] 菜单画面非纯色（均值 231, 标准差 71）—— 背景图生效
```

索引页本身的内容校验（`verify_menu.py` 不管这块）—— 逐格量封面均值
与名称的最亮像素：

```python
# 每格都要 封面均值 > 3 且 名称最亮 > 200
```

> 画面均值从 100 升到 231 —— 二级菜单背景的压暗从 70% 降到 35%
> （`DVDA_MENU_COVER_DIM`）。

---

## 为什么改用 git 提交

| | 补丁脚本（旧） | git 提交（现） |
|---|---|---|
| 体量 | 25 个脚本 4589 行 | 一份 diff，20 个文件 1251 插入 |
| 精确性 | **位置匹配**重放，落点被后续补丁改写就失配 | 逐字符 diff，精确 |
| 幂等 | 需逐个保证（曾有 3 个不幂等，把源码叠坏） | 天然幂等 |
| 回滚 | 手工 `cp` 备份 | `git checkout` |
| 移植 | 路径写死在脚本里 | `git format-patch` |

## 从上游重建

```bash
git clone https://github.com/fabnicol/dvda-author tools/dvda-author-mlp8
cd tools/dvda-author-mlp8
git checkout 8fca43a                       # 基线（浅克隆需先 fetch --unshallow）
git apply /path/to/dvda-author-changes.patch
```

导出改动集：

```bash
cd tools/dvda-author-mlp8
git diff 8fca43a -- src libutils > dvda-author-changes.patch
```

> 若源码树的 git 里已提交了这些改动，也可直接
> `git format-patch 8fca43a -- src libutils`。
