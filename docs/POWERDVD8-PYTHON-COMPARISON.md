# PowerDVD 8 黑屏：Python 版本与当前实现对照

检查日期：2026-10-06。源码基线：`39ddb6a9ab0d8b0a949b6c00499313ec1aaf6780`（删除 Python 业务脚本的 `f5c9f58` 的父提交）。当前提交：`935b844`，另有尚未提交的 GUI、自动分盘及自动线程改动。

**已确认两类实质回归：ISO 写入器没有保留旧版 DVD-Audio 的文件系统和物理布局；菜单/静图编码没有保留旧版单帧序列的结束方式。** 修复前成品确实含有这些问题，不能仅凭 FFmpeg 解码成功判定 PowerDVD 兼容。

**范围说明：第 1–5 节记录的是修复前 ISO 和旧实现的基线，不代表当前源码仍有这些结构缺陷。** 本次完成源码追溯、四张现有镜像的结构比较、视频包解析和单帧解析实验；没有改写待比较的参考盘或已交付镜像。修复后另行构建了 LPCM/MLP 短样本，并用本机 PowerDVD 8 播放，结果见“修复后复核”。完整专辑和完整制盘仍未重跑。

## 比较对象

| 对象 | 镜像 | 字节数 | 菜单页 / 标题 / 播放静图 |
| --- | --- | ---: | --- |
| E 盘参考 1 | `E:\DISCs\DVD-Audio\Wuthering Waves Singles EPs\Wuthering Waves Singles EPs 1.iso` | 4,659,234,816 | 31 / 28 / 91 |
| E 盘参考 2 | 同目录 `Wuthering Waves Singles EPs 2.iso` | 3,134,130,176 | 19 / 17 / 56 |
| 当前成品 1 | `D:\鸣潮DVD_Audio_ext\Wuthering_Waves_Singles_EPs_1.iso` | 3,784,384,512 | 25 / 23 / 74 |
| 当前成品 2 | 同目录 `Wuthering_Waves_Singles_EPs_2.iso` | 4,016,273,408 | 24 / 22 / 73 |

当前两张成品的修改时间分别为当天 16:58 和 17:01。它们已经包含此前 `preload=120000` 的时间戳调整。两组镜像的分盘边界不同，因此按各自实际页数、标题数和静图数核对结构，不把文件大小或整体哈希不同直接当作故障。

原始检查结果与提取材料保存在 `D:\dvda-black-screen-analysis\audit-current\`：

- `layout.json`：各文件实际 LBA、大小。
- `comparison.json`：导航指针、文件系统字段、视频序列和解析器结果；包含所提取 VOB 的 SHA-256。
- `decode-check.json`：四张盘菜单及静图首帧解码结果。
- `end-code-control.json`：只改变序列结束码的对照实验。
- `compare.py`、`parser-check.c`：检查程序。
- `iso9660.py`、`02_build.py`、`menu_assets.py`、`verify_menu.py`：通过 `git show 39ddb6a:<文件>` 提取的历史源码。

## 1. ISO 按文件名排序，破坏了 IFO 中的相对扇区地址

Python 的 `02_build.py::build_disc()` 最后调用：

```text
mkisofs -dvd-audio -V <卷标> -o <镜像> <输出目录>
```

`-dvd-audio` 为音频文件指定物理排序权重。旧版实际布局为：

```text
AUDIO_PP.IFO
AUDIO_TS.IFO → AUDIO_TS.VOB → AUDIO_TS.BUP
AUDIO_SV.IFO → AUDIO_SV.VOB → AUDIO_SV.BUP
ATS_01_0.IFO → ATS_01_1.AOB … ATS_01_n.AOB → ATS_01_0.BUP
后续标题集
```

当前 [iso_writer.c](../tools/dvda-author-mlp8/src/iso_writer.c) 的 `compare_nodes()` 使用 `_stricmp()` 按文件名排序，`assign_files()` 和 `write_files()` 按此顺序放置数据。成品的物理顺序变成：

```text
ATS_01_0.BUP → ATS_01_0.IFO → ATS_01_1.AOB …
AUDIO_PP.IFO
AUDIO_SV.BUP → AUDIO_SV.IFO → AUDIO_SV.VOB
AUDIO_TS.BUP → AUDIO_TS.IFO → AUDIO_TS.VOB
```

但是 [amg2.c](../tools/dvda-author-mlp8/src/amg2.c) 仍按照旧版布局计算地址：

- `AMG + 0x30`：`2 * amg_sectors + top_vob_sectors`，目标实际是 `AUDIO_SV.IFO`。
- 标题表中各条目的 ATSI 地址：`2 * (amg_sectors + asvs_sectors) + still_vob_sectors + top_vob_sectors`，再累加前面标题集的大小。
- AMG 末扇区字段同样假定 BUP 位于菜单 VOB 后面。

这里有源码注释把 `0x30` 称为 `AUDIO_SV.VOB`；实测参考盘的地址落在 **`AUDIO_SV.IFO`**，不能照注释再额外移过 IFO。

### 成品证据

所有 LBA 从 0 起，以 2048 字节为一个扇区。下表“声明位置”由 AMG 所在 LBA 加其相对地址计算。

| 镜像 | 目标 | 声明位置 | 实际位置 | 结果 |
| --- | --- | ---: | ---: | --- |
| 参考 1 | ASVS IFO | 2,001 | 2,001 | 一致 |
| 参考 1 | ATSI IFO | 5,311 | 5,311 | 一致 |
| 参考 2 | ASVS IFO | 1,315 | 1,315 | 一致 |
| 参考 2 | ATSI IFO | 3,279 | 3,279 | 一致 |
| 当前 1 | AMG BUP | 1,847,844 | 1,846,320 | 越过文件末尾 |
| 当前 1 | ASVS IFO | 1,847,851 | 1,843,035 | 越过文件末尾 |
| 当前 1 | ATSI IFO | 1,851,138 | 27 | 越过文件末尾 |
| 当前 2 | AMG BUP | 1,961,071 | 1,959,209 | 越过文件末尾 |
| 当前 2 | ASVS IFO | 1,961,078 | 1,955,172 | 越过文件末尾 |
| 当前 2 | ATSI IFO | 1,965,117 | 26 | 越过文件末尾 |

当前盘 1 只有 1,847,844 个扇区，盘 2 只有 1,961,071 个扇区。按表中错误地址实际读取，得到空数据。

盘 1 的 23 条标题指针、静图入口及备份入口共 **25 项越界**；盘 2 对应 **24 项越界**。参考盘相同检查全部通过。

菜单 VOB 的直接入口 `AMG + 0xC0` 在当前盘上恰好仍正确，因为 `AUDIO_TS.IFO` 后面仍紧接 `AUDIO_TS.VOB`。所以不能仅凭静图地址错误断言已经单独解释了全部菜单黑屏；菜单视频还有第 3 节所述独立回归。

这是确定的成盘结构错误，不能由“音频仍然能播放”排除。按文件名打开内容与按 IFO 指针寻址是两种不同访问路径。

## 2. UDF 丢失，ISO9660 卷描述符也存在错误

旧版 `mkisofs -dvd-audio` 会启用 UDF。已在本机旧工具链源码 `D:\dev\winbuild\src\cdrtools-3.02\mkisofs\mkisofs.c` 的 `dvd_aud_vid_flag → rationalize_udf → use_udf` 路径和 `udf.c` 的 DVD-Audio 排序表中确认。

参考镜像具有：

- 卷识别序列 `BEA01`、`NSR02`、`TEA01`。
- LBA 256 的 UDF Anchor Volume Descriptor。

当前 `iso_writer.c` 仅写 ISO9660 PVD、终止符、路径表和目录；两个当前成品均没有上述 UDF 结构。

另外，`write_volume_descriptors()` 对根目录记录没有复用正确的 `write_directory_record()`，造成以下字段与旧版不等价：

| PVD 字段 | 参考盘 | 当前盘 |
| --- | ---: | ---: |
| 根目录标志，绝对偏移 181 | `0x02`：目录 | `0x01`：隐藏标志，目录位未置位 |
| 根目录 Volume Sequence Number，小端 / 大端 | 1 / 1 | 0 / 0 |
| File Structure Version，偏移 881 | 1 | 0 |

四张镜像逐字节读取确认了上述值。它们都是现有成品中可复现的文件系统差异；各字段对 PowerDVD 8 的独立影响尚未做播放器 A/B 实验。

ISO 写入器及其替换入口首次出现在 **`becc1bb`（2026-10-03）**。因此这一回归在 C# 阶段迁移原生依赖时就已经引入，并不是 Rust GUI 绘制造成的。

## 3. 新的视频链路遗漏单帧 MPEG 序列结束码及静图程序结束扇区

Python 时期，`menu.c::create_mpg()` 使用：

```text
jpeg2yuv → mpeg2enc -f 8 -n p -a 2 -q 1 -b 9800 -H → mplex -f 8
```

随后静图路径依次执行：

1. `dvda_pad_program_end()`：把 `mplex` 写在末尾的 `00 00 01 B9` 改成单独的结束扇区。
2. `dvda_rewrite_nav_sector()`：生成适用于静图的导航扇区。
3. `dvda_fix_sequence_progressive()`：按实际图像修正序列逐行标志。
4. 统计最终扇区数，生成 ASVS 表。

迁移后的 [dvda-menu-media.c](../tools/win-build/native/dvda-menu-media.c) 用 FFmpeg MPEG-2 编码器和 DVD muxer 替代前面的程序链。`encode_video()` 发送一帧并 flush 编码器，随后调用 `av_write_trailer()`；实际产物没有写出旧版单帧序列的 `00 00 01 B7`，也没有静图的 `B9` 程序结束码。

旧的 `dvda_pad_program_end()` 仍然存在，但它仅处理“文件最后四字节已经是 B9”的情况。新 muxer 没有生成 B9，于是函数直接返回成功，结束扇区实际没有生成。

### 实际码流统计

先按 PES 包长度提取视频 elementary stream，再统计 B7，避免把音频、字幕或填充中的相同字节误认为视频结束码。B9 按独立扇区开头统计。

| 镜像 | 菜单页数 / 菜单 B7 数 | 静图数 / 静图 B7 数 | 静图 B9 结束扇区 |
| --- | --- | --- | ---: |
| 参考 1 | 31 / 31 | 91 / 91 | 91 |
| 参考 2 | 19 / 19 | 56 / 56 | 56 |
| 当前 1 | 25 / **0** | 74 / **0** | **0** |
| 当前 2 | 24 / **0** | 73 / **0** | **0** |

### 单帧解析对照实验

对四张镜像分别提取首个菜单和首个静图的视频序列，用本机 FFmpeg `av_parser_parse2()` 读取；先记录没有 EOF 时是否交出完整帧，再显式发送 EOF。

| 输入 | EOF 之前交出帧 | 仅在 EOF 才交出帧 |
| --- | ---: | ---: |
| 两张参考盘的首个菜单 / 静图 | 1 | 0 |
| 两张当前盘的首个菜单 / 静图 | 0 | 1 |

进一步只在独立实验副本中改变 B7：

- 从参考盘 1 的首个菜单、静图序列删除 B7，结果变为“仅在 EOF 交出帧”。
- 给当前盘 1 的对应序列添加 B7，结果变为“EOF 前交出帧”。

没有改动任何 ISO、音频或发布文件。这个实验确认结束码缺失改变了单帧解析行为；它提供了播放器等待完整画面而持续黑屏的具体机制，但不是 PowerDVD 8 内部执行过程的直接观测，最终仍需修复后实测。

视频链路替换首次出现在 **`facfee0`（2026-10-04）**，同样早于 Rust 迁移收尾。

## 4. 此前时间戳修正覆盖的范围有限

`935b844` 把 FFmpeg DVD muxer 的 `preload` 设置为 120000 微秒，解决此前默认 500 ms 与老短菜单时间线的差异。

当前镜像实测：

| 首个视频 PES | 参考盘 | 当前盘 |
| --- | --- | --- |
| 菜单 PTS / DTS，单位 90 kHz | 10800 / 7200 | 11702 / 未显式写 DTS |
| 静图 PTS / DTS | 10800 / 7200 | 10800 / 未显式写 DTS |

这说明当前成品已不是此前 500 ms 延迟的版本；继续只调整 preload 无法补回失效的 LBA、UDF、B7 或 B9。

其他编码差异也已记录：菜单 `progressive_sequence` 从 0 变为 1；旧版序列头带自定义 intra 量化矩阵，新版使用默认矩阵；编码码率控制方式也发生变化。这些可能影响码流大小、画质或兼容性，不能不经实验就与黑屏直接画等号。

## 5. 已核对保留的实现，以及为何旧校验会通过

以下行为没有发现能解释全面黑屏的变化：

- Python 的菜单参数和 Rust `menu.rs::build_assets()` 都传递 topmenu、页面背景、screentext、index-pages、index-covers、字体和 stillpics；Windows 静图列表仍使用分号。
- 静图依然每轨引用一张；同专辑复用 JPG 路径，未改回“空项沿用上一张”的旧缺项做法。
- 封面仍按 `93.75%x100%!` 补偿像素宽高比，fit 到 720×576，再居中补黑边，JPEG 质量仍为 92。
- `asvs.c`、`atsi2.c`、`xml.c`、`include/menu.h` 在 Python 基线到当前提交之间没有 Git 差异。源代码中的正确局部表结构不能弥补最终 ISO 物理顺序被改变。
- 四张盘视频均为 720×576、25 fps、4:3、Main Profile/Main Level、4:2:0；序列声明码率均为 9.8 Mb/s，VBV 为 229376 字节。AMG 和 ASVS 的 PAL 属性仍为 `0x53`，静图按钮属性仍为 0。
- 本次实际解码四张盘的菜单与静图首帧全部成功。当前盘 1 的首张菜单和播放封面也已查看，均有正常图像内容。

修复前校验的盲区是：

1. `menu_verify.rs::verify()` 和 `verify.rs::read_iso_file()` 根据 ISO9660 **文件名**提取 IFO、VOB，再验证局部表格和图像，没有把 AMG 指针换算成实际 ISO LBA 去读取目标。错误的盘级地址因而被绕过。
2. 验证输入被切成独立文件交给 FFmpeg，在 EOF 时会 flush；第 3 节实验说明，这会掩盖缺少序列结束码的单帧行为差异。
3. `test-menu-media.py` 检查尺寸、帧率、宽高比、时间戳、2048 字节包边界和确定性，没有检查 B7/B9。
4. 修复前的 ISO 读取检查不验证 UDF，也不拒绝上述错误的 PVD 根记录和版本字段。

Python 的抽帧检查也具有部分相同盲区；过去正常是因为被调用的 `mkisofs` 和 `mpeg2enc/mplex` 提供了这些行为。替换这些程序后，沿用既有抽帧检查不足以证明行为等价。修复后的验证器已覆盖 PVD/UDF 描述符及菜单导航实际 ISO extent；UDF 目录树另由 `pycdlib` 遍历/提取、Windows 挂载和短样本播放器读取交叉验证。Rust 校验器本身目前不遍历 UDF 目录树并比对全部文件 extent。

## 修复项与验收状态

1. **已完成：**原生 ISO 写入器恢复 DVD-Audio 数据物理排序、UDF 和正确的 ISO9660 卷字段；目录项按名称排序，文件数据按 DVD-Audio 顺序排布。
2. **已完成：**视频编码/封装阶段生成单帧序列结束码和静图结束扇区，再据最终大小生成导航表；没有对完成的 ISO 作字节补丁。
3. **已完成：**成品校验确认 IFO 盘级地址指向真实 ISO extent 且不越界，检查 UDF/PVD 描述符，并检查菜单与静图结束结构以及 EOF 前单帧解析。
4. **短样本已通过：**LPCM 和 MLP 均在 PowerDVD 8 中完成进菜单、选曲、两首曲目播放、显示封面和返回索引。**完整原始专辑/整盘尚未复测。**结构检查和播放器实测仍作为独立结论记录。

另有一项非黑屏根因的输出差异尚未对齐：视频时间戳以及 `progressive_sequence`、intra 量化矩阵和码率控制。当前短样本能正常播放，尚无证据证明这些差异导致黑屏；也没有做逐项 A/B，因此不应把它们描述为已与 Python 输出等价。

上述修复不需要把完整流程退回 Python；需要补齐的是替代原生组件时遗漏的输出语义。

## 修复后复核（2026-10-06）

以下是按上文问题完成的源码修复与验证；前文的镜像数据仍是修复前现场，原始参考镜像和已交付 ISO 均未被改写。

- `iso_writer.c` 现在把 ISO 目录项继续按文件名排序，同时按 cdrtools 的 DVD-Audio 权重分配文件数据扇区；恢复 PVD 根目录标志、卷序号和文件结构版本，并生成 UDF 1.02 的识别序列、主/备用 VDS、完整性序列、首尾 Anchor、文件集、目录和文件条目。UDF 与 ISO9660 指向同一份文件数据。
- ISO 写入器 14 文件夹具通过 MinGW GCC `-Wall -Wextra -Werror` 编译；验证了目录项名称序、DVD-Audio 数据扇区序、PVD 字段、UDF tag checksum/CRC、双 VDS、FSD/目录项和共享 extent。`pycdlib` 可分别遍历 ISO9660/UDF 并从 UDF 提取文件；Windows `Mount-DiskImage` 将镜像识别为 UDF 并列出文件。
- 另将当前成品 1 的 ISO 文件提取到 D 盘隔离目录，再用新写入器从原文件内容重建镜像。AMG 的 ASVS、菜单 VOB、BUP 和两份标题表共 49 条地址均命中目标文件真实 LBA，越界/错位数为 0；UDF 可读取 13 个 `AUDIO_TS` 文件。该步骤只重建容器，没有重新编码音频或改动原成品 ISO。
- `dvda-menu-media.c` 在交给 DVD muxer 前给最终 MPEG 视频包补齐 B7；静图在 mux trailer 后写入独立的 B9/`0xFF` 扇区。测试覆盖 PAL/NTSC、4:3/16:9、带/不带菜单音频，检查完整包扇区、静图 B9、B7 和 EOF 前完整帧解析，并保留失败输入与确定性回归。
- Rust 成品校验现检查 PVD 字段、UDF VRS、Anchor、主/备用描述符标签及 CRC、分区、完整性序列、FSD/终止符和结束 Anchor；菜单校验按 ISO extent 核对 AMG 的菜单 VOB、BUP、ASVS 指针及两份标题表内全部 ATSI/VTSI 指针。制盘后的 ISO 容量检查也会先拒绝文件系统结构错误。UDF 文件树/extent 的外部验证由 `pycdlib` 遍历与提取、Windows 挂载和 PowerDVD 实际读取补足；当前 Rust 校验器不单独遍历整个 UDF 文件树。
- 新作者程序使用 MSYS2 GCC 从已配置源码树构建成功，位于 `D:\dvda-release-build-20261006\black-screen-fix\author-native\dvda-author-dev.exe`。Rust 核心单测 82 项通过、2 项按原有条件忽略；桌面端 17 项通过；`dvda-core` 与 `dvda-desktop` Clippy `-D warnings` 通过，原生菜单和 ISO 夹具通过，`git diff --check` 通过。

### PowerDVD 8 短样本播放

本机安装了 CyberLink PowerDVD 8.0.1531。使用原生作者程序生成两轨、48 kHz / 24 位 / 双声道、每轨 10 秒的测试盘；菜单和静图走修复后的 MPEG/ISO 流程。LPCM 与 MLP 模式各自执行应用集成测试、挂载生成的 UDF 镜像，并在 PowerDVD 8 中完成以下操作：进入专辑索引，打开曲目页，播放第 1/2 与第 2/2 曲目，观察播放计时及封面画面，曲目结束后返回索引页。

- LPCM：播放器 OSD 显示 `MPEG-2` 画面和 `LPCM 2.0` 音频；播放时封面可见，计时从约 1 秒推进到 6 秒，第二曲可播放并返回索引。
- MLP：播放器 OSD 显示 `MPEG-2` 画面和 `MLP 2.0` 音频；播放时封面可见，计时从约 1 秒推进到 6 秒，第二曲可播放并返回索引。
- 应用端的 `application_menu_and_title_boundaries` 集成测试在 LPCM 与 MLP 两种模式下均通过；PowerDVD 样本截图保存在 `D:\dvda-release-build-20261006\black-screen-fix\playback-sample\`。

这证明当前短样本的菜单、两首曲目的视频封面、音频播放和返回索引路径可用；它没有覆盖用户的完整专辑、长时播放、所有 DVD-Audio 采样率/声道组合或实体光盘，因此不能据此宣称用户原盘已完整修复。

### 仍有差异

新样本的菜单首个视频 PES 为 PTS `11702`、未显式写 DTS；静图为 PTS `10800`、未显式写 DTS。参考盘两者均为 `10800 / 7200`。PowerDVD 8 在上述短样本中正常显示并播放，因此这项差异**尚未复现黑屏**，但仍属于未完成的输出行为对齐。`progressive_sequence`、intra 量化矩阵和码率控制也尚未做单变量播放器 A/B；现有证据不足以把它们认定为黑屏原因或兼容性无影响。

**状态：报告确认的 ISO 布局/UDF/PVD、导航地址和 MPEG 单帧结束结构回归已修复；LPCM 与 MLP 短样本通过 PowerDVD 8 实测。完整专辑/全盘仍未复测，时间戳与编码参数差异仍待单变量验证。**
