# DVD-Audio-Maker Android 移植：代码级对齐审计

结论：**当前 Android shim 只覆盖了 Windows 版的一部分行为**（约能完成"选目录→出 ISO→基本验证"），
按代码核对，Windows 版还有大量行为未对齐。下面每条都给出源码位置（/root/workspace/dvda-port/DVD-Audio-Maker/rust/crates/...）。

## 一、已确认对齐/已实现
- FLAC/M4A 解码、重采样、抖动、MLP 编码、LPCM 直通（dvda-mlp / dvda-bridges media）
- 选曲菜单、索引页、封面、CJK/日/韩字体参数、PAL/NTSC、宽高比、下混、CGA、背景视频、topvob/stillvob
  （dvda-author 能力 + shim 组参数）
- 逐轨 MLP 载荷验证（dvda-native disc_verify）、容量/菜单结构/时间线(单调)检查
- 断点续制、日志、设置、历史、队列、方案、5 语言消息（部分）
- 构建前检查音源（但只 probe，见下）

## 二、代码确认未对齐（按影响排序）

### 1) 音源标签/分轨/排序：完全未对齐
- Windows 工程按 **Album 标签** 分组：dvda-core/src/planner.rs:32 albums() 用 track["Album"]。
- 音源元数据来自媒体 probe 的标签（preparation/pipeline.rs:298 audio::read_metadata），
  曲目名用 Title 标签进菜单（menu.rs 100/178 取 track["Title"]）。
- 曲号排序按文件名开头的数字，解析失败算 9999：planner.rs:197 track_number()。
- 我们的实现：按文件夹分组、菜单标题用文件名 stem、按字节序排序。标签全丢。

### 2) "检查音源"语义不同：Windows 是 prepare（真解码+编码），我们只 probe
- app.rs:466 run_prepare -> preparation/pipeline.rs：枚举音源、读元数据、**全部解码为 PCM**、
  重采样、ALAC 修复、校验、编码 MLP、写 manifest/snapshot 与缓存。
- 我们 check_library 只 probe 时长/声道（shim/src/planner.rs check_library）。
  → 音源损坏/解码失败只在真正制作时才暴露；Windows 在检查阶段就失败。
- 构建前自动检查同理（我们调用的是 probe 检查）。

### 3) 规划算法不同（两阶段 vs 单阶段）
- Windows：先把所有曲目编码成 MLP，再用真实 MLP 字节数规划：
  estimate = MlpSize × 1.025（planner.rs:24），容量上限 = DiscBytes − 8 MiB（planner.rs:135），
  默认 DiscBytes = 4_707_319_808（options.rs DiscBytes）。
- 分组按 (采样率, 位深, mlp/lpcm, 声道, 通道掩码) 分桶（planner.rs groups_for_albums 78-133）；
  组轨上限 99(DVDA_GROUP_TRACK_LIMIT)，超限只报警告 ALBUM_EXCEEDS_GROUP_LIMIT；
  专辑超容量报警告 ALBUM_EXCEEDS_DISC；MLP 缺失是错误 MLP_MISSING。
- 我们：单阶段，按 PCM 时长估 0.72×PCM+1MB，一组=一个专辑，没有 8MiB 余量、没有 99 轨上限报警、
  没有诊断码/严重级模型。

### 4) 标题结构 / TitleMode / AlbumLimit 未实现
- DVDA_TITLE_MODE 默认 "album"（options.rs:396-407）：album=按专辑边界分 title（author 的 -z），
  one=单 title，数字=每 N 轨一个 title。author.rs title_ends()/build_arguments() 生成 -z。
- DVDA_ALBUM_LIMIT：每盘专辑数超限给诊断（disc.rs:352，build.rs:180）。
- 我们：每个专辑一个 -g，从不生成 -z；没有这两项设置。

### 5) ISO 命名与卷标格式不同
- ISO 名 = "{IsoPrefix}_{盘号}.iso"（build.rs:989），IsoPrefix 缺省由标题生成（options.rs:284-291）；
  卷标 = "{Title} {盘号}"（verify 期望值 verify.rs VOLUME_ID_MISMATCH 分支）。
- 我们：固定 "DVD-Audio-NN.iso"，卷标 "ANDROIDn"。

### 6) 验证只有 5 项里的 3 项，且强度低
- Windows 5 模式：All / Capacity / Timeline / Menu / Lossless（verify.rs:67）。
- Capacity：ISO 文件总大小 vs DiscBytes、AUDIO_TS 存在、AOB/IFO 计数、卷标匹配……（verify.rs:644）。
  我们：只把 AUDIO_TS/VIDEO_TS 文件大小求和对比容量。
- Timeline：源时长与 ATSI 时间线的丢失阈值 DVDA_LOSS_ERROR_S(0.05)/WARN_S(0.005)（options.rs、app.rs），
  还要核 manifest 与 build log（verification::manifest_issues / log_issues，verify.rs:261-273）。
  我们：只检查 PES PTS 单调。
- Menu：结构性 AMG/ASVS/VIDEO_TS 指针、cell 链、备份指针等 ~37 个检查码（menu_verify.rs），
  外加画面检查 MENU_FRAME_NEAR_SOLID / MENU_INDEX_*。
  我们：只看 AUDIO_SV.VOB 是否存在。
- 诊断码/严重级模型（verification.rs 的 TRACK_GAP、PTS_INVALID、AOB_SECTOR_MISMATCH…）完全没有。

### 7) 断点续制的匹配依据完全不同
- Windows 每盘签名 = 盘号/卷标/ISO 名/标题/TitleMode/组上限/菜单全部设置 +
  每个源文件的 identity（Size + LastWriteUtcTicks + HeadHash + TailHash）+ 可执行文件哈希 +
  菜单字体/封面哈希（resume.rs disc_signature、identity.rs:21-54）；ISO 也做 identity 与
  audit_log 哈希（resume.rs:58-72）。签名不匹配就必须重做。
- 我们：只比对源文件大小 + MLP 大小（shim planner）。源文件改动但大小不变会被错误复用。

### 8) 缺少 MLP 导入/外部 MLP 工作流
- DVDA_MLP_SOURCE = mlp-batch | lpcm | external（options.rs）；external 时用 DVDA_MLP_EXTERNAL_DIR，
  acquisition.rs 做 ordinal 大小写不敏感 basename 匹配、歧义名报告。
- 我们：只支持现场编码（mlp）/WAV 直通（lpcm），没有 external 复用已有 .mlp。

### 9) 菜单可配置项大量缺失
- 每页曲目数默认 12（DVDA_MENU_TRACKS_PER_PAGE）、索引页最少专辑数 4（DVDA_MENU_INDEX_MIN_ALBUMS）、
  静止图开关、封面缩放 35%、菜单语言=界面语言、nmenus 总页数、菜单数据目录/字体解析（menu.rs:70-110、430）。
- 我们：固定每页 30/29 轨、索引条件 >1 个专辑、不传 --nmenus、封面比例固定、没有这些设置项。

### 10) 构建流水线差异
- Windows 先 menu preflight（数据目录）→ disk_space 检查（代码确实有，build.rs:204/240；
  但你说 Windows 界面没有这个行为，这里只作为代码事实记录）→ acquisition → plan →
  逐盘 menu build_assets → author → ISO 校验 validate_iso → 导航有效性 →
  menu_check::verify → 写 resume → 清理 → staging → 原子发布到最终目录（build.rs:593-716）。
- 我们：直接写最终 ISO，验证是自研三项，没有 staging/原子发布、ISO 结构校验、导航校验。

### 11) 准备阶段缓存/并行度
- Windows preparation cache 带签名与快照失效提示（"[快照] 修复音源已改变，将重新检查。"），
  MLP 编码按 worker 预算并行（options.rs worker_count/WorkBudget、batch.rs 有界队列）。
- 我们：顺序执行，无准备缓存（仅 resume 复用 MLP 文件）。

### 12) 日志协议与本地化规模
- Windows 日志含结构化事件 "[TASK] {json}"（task_log.rs），GUI 解析它显示进度/告警；
  locales 约 978 条 × 5 语言（locales/messages.*.json）。
- 我们：自定义文本日志（约 20 条消息），App 界面文字仍只有中文。

### 13) 设置项与环境变量
- Windows 设置是完整 profile：约 40+ 键（ISO_PREFIX/DISC_BYTES/GROUP_TRACK_LIMIT/标题模式/ALBUM_LIMIT/
  MLP_* 各项/菜单各项/字体/损失阈值/AUTHOR_SRC/BUILD_DIR/…），并支持 DVDA_* 环境变量覆盖；
  新建 profile 写全量默认值，已有 profile 保持稀疏（app.rs load/save_profile）。
- 我们：settings.json 约 20 个键，无环境变量覆盖、无稀疏/全量语义。

### 14) 其它细节
- Dither16 只对 16bit（DVDA_DITHER_16 默认 true，options.rs:332）；我们始终按开关传 dither。
- LPCM 目标速率白名单与 9.6Mb/s 码率、20bit 低 4 位检查在编码前做（lpcm.rs:22-29、pcm.rs:46-68）。
  我们没有前置校验，靠 author 报错。
- 通道掩码/CGA 校验在 prepare rules::validate_channels（pipeline.rs:326），我们无。
- 多平台路径/环境：DVDA_EXE 目录便携设置、LOCALAPPDATA 默认 build 目录（app.rs），Android 无对应。

## 三、仅 Windows 平台/开发用途，不必移植
- 运行库解包（runtime.rs）、kernel32/shell32、PowerShell 脚本、作者/图像 DLL 目录、oracle 差分入口、
  开发者 CLI dry-run、DVDA_AUTHOR_SRC 等。
- 成品播放/磁盘空间检查：按你的说明不算 Windows 行为（代码里有 disk_space 调用，但作为非目标处理）。

## 四、建议实现顺序（真要对齐行为）
1. 标签管线：probe 出 Album/Title/Track/Date，按 Album 分组、按曲号排序；菜单/ATSI 用标签。
2. "检查音源"改成 prepare：解码校验 + 重采样 + 编码 MLP（可只写 manifest 不发布），构建前复用。
3. 两阶段规划：先编码再用真实 MLP 大小 ×1.025、DiscBytes−8MiB 分盘、格式分桶、组上限/诊断码。
4. 验证补齐：Capacity 按 ISO 总大小+卷标+AOB/IFO 计数；Timeline 按源时长与阈值；Menu 结构检查；
   All 模式串起来；诊断码/严重级。
5. resume 签名（源 identity/可执行文件/字体封面哈希）、ISO 名/卷标、staging 原子发布。
6. 菜单设置项（每页曲目/索引阈值/静止图/封面比例/nmenus/TitleMode/AlbumLimit）。
7. 日志改 [TASK] 事件协议、补齐本地化文案（界面语言资源化）。
