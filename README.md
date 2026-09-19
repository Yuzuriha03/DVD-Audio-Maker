# 鸣潮 DVD-Audio 制作脚本

把「鸣潮先约电台」FLAC 音源制作成 DVD-Audio 光盘 ISO。

## 环境要求

- WSL Ubuntu 26.04(`resolute`),已换阿里云源
- WSL 内已安装:`python3`、`ffmpeg`(需支持 `mlp` 编码器)、`gdb`(可选)
- patched `mkisofs`(`/opt/dvda-author/local.ubuntu.20.10/bin/mkisofs`)
- **自编译的 `dvda-author`**:`/root/dvda-author-mlp8/src/dvda-author-dev`
  (链接系统 FFmpeg 8、已迁移 API、支持 24-bit 无损 MLP;见下文「MLP 支持」)

## 目录约定(全部在 WSL 内部 ext4,提速)

| 用途 | 路径 |
|------|------|
| 音源(只读) | `/mnt/c/Users/yyz57/Music/鸣潮先约电台` |
| MLP 缓存目录 | `/root/dvda-build/mlp/` |
| MLP 索引 | `/root/dvda-build/mlp_index.json` |
| ALAC 修复产物 | `/root/dvda-build/alacfix/` |
| AUDIO_TS 输出 | `/root/dvda-build/out/` |
| dvda-author 临时目录 | `/root/dvda-build/tmp/` |
| ISO 输出 | `/root/dvda-build/iso/` |
| 最终产物 | `/mnt/d/鸣潮DVD_Audio/` |

## 使用

```bash
# 进入 WSL(root),运行完整流水线
wsl -d Ubuntu-26.04 -u root -e bash -lc "cd /mnt/c/Users/yyz57/Music/鸣潮先约电台/dvda_scripts && bash build.sh"
```

或分步:

```bash
python3 01_prepare.py   # 步骤1:扫描 + 专辑归一化 + 解码完整性校验,生成 manifest.json
python3 02_build.py     # 步骤2:直接对源文件编码 MLP + dvda-author 出盘 + mkisofs 打包
```

## 脚本说明

- `01_prepare.py` — 扫描 FLAC/M4A，按专辑归一化采样率/位深（以多数曲目参数为准），组内按发布日+曲序号排序，并做**解码完整性校验**（见下）。**不生成 WAV。**
- `02_build.py` — 读取 manifest,直接以**源文件**为输入逐曲做无损 MLP 编码(缓存,需重采样的曲目在同一命令内完成),按专辑发布顺序分盘(盘1填满、专辑不拆),dvda-author 生成 AUDIO_TS,mkisofs 打包,复制到 D 盘;输出 `mlp_index.json` 供校验脚本使用
- `build.sh` — 一键编排上面两步

## 关键决策(记录)

1. **专辑归一化**:同一张专辑若曲目采样率/位深不一致,以「多数采样率 + 该采样率下多数位深」为目标重采样少数曲目,保证整张专辑在同一组内连续播放。最终 2 组:48kHz/24bit(131首)+ 44.1kHz/24bit(16首)。
2. **分组轨数上限**:DVD-Audio 协议每组最多 99 轨;但 dvda-author 的 ATSI 表缓冲固定 3 扇区(6144 字节),每轨约 52 字节,**超过约 70 轨会栈溢出**。故每组限制为 64 轨以内(GROUP_TRACK_LIMIT),超出按专辑边界再拆一组。
3. **分盘**:按专辑发布顺序,盘1 填满,盘2 装剩余;两张均为单层 DVD-5,专辑不拆散。
4. **纯 DVD-Audio**:只含 AUDIO_TS,不放空的 VIDEO_TS 占位目录(空目录无兼容性价值)。
5. **MLP 无损压缩**:已启用。24-bit 音源压缩率约 18%,总 AOB 约 7.22 GiB,可放入两张 DVD-5(上限 8.77 GiB)。
   已验证:MLP 解码后 PCM 与源**逐字节一致**;AOB 中提取的 MLP 与输入 MLP **MD5 一致**。
6. **不生成 WAV**:实测「源 --[重采样]--> MLP」与「源 --[重采样]--> WAV --> MLP」输出**逐字节一致**
   (48k 直通与 44.1k→48k soxr 重采样均已验证),WAV 只是中转。去掉后省约 9 GB 落盘与一轮读写 I/O。
   代价:MLP 容器不记录时长(`ffprobe` 返回 N/A),故完整性校验改用 `astats` 采样数比对,
   并由 `02_build.py` 输出 `mlp_index.json`(MLP → 源文件/声明时长/重采样目标)供热时长脚本使用。
7. **必须显式指定位深**:WAV 中转阶段原本用 `-c:a pcm_s24le` 隐式把位深强制成 24-bit。
   直连后 MLP 编码器会**沿用源位深**,于是 44.1k/16 的源重采样到 48k 后仍是 **16-bit**,
   混进 24-bit 音频组 —— 而 dvda-author **不会报错**,照样出盘。
   故 `ensure_mlp()` 现在显式传 `-sample_fmt s32p`(MLP 只接受 planer 名,`s32` 会报错),
   并在编码后用 `ffprobe` 复核采样率/位深,不一致即删除并报错。
   已验证 `-sample_fmt s32p` 的输出与旧「WAV + pcm_s24le」路径**逐字节一致**(MLP 与解码 PCM)。
8. **声道数**:DVD-Audio 同组内须同声道数,单声道与立体声无法无损互转,故不做转换而是检查。
   实测本项目 147 个源文件**全部为 2ch/stereo**(FL+FR,即 dvda-author 报的 `L-R`)。

## MLP 支持(方案 C)

原版 `dvda-author`(`/opt/dvda-author/src/dvda-author`)是 core 构建,不含 MLP;随包 FFmpeg 4.2.4 的
MLP 编码器只支持 16-bit,且其 MLP 读写路径存在缺陷(自产 MLP 无法通过自身 lossless 校验)。

解决方式:把 `mlp.c` 迁移到系统 FFmpeg 8 API 并重新编译到 `/root/dvda-author-mlp8`:

| 问题 | 处理 |
|------|------|
| `AVCodecParameters/AVCodecContext/AVFrame.channels` 等已移除 | 改用 `ch_layout.nb_channels` |
| `AVFrame.pkt_pos/pkt_duration/pkt_size` 已移除 | 自行记录输入包位置 `g_last_pkt_pos` |
| **`avcodec_receive_frame` 在 EAGAIN 前会 `av_frame_unref`** | **收到帧时立即保存 `g_last_nb_samples`**（见下） |
| `av_parser_*` 读取路径 | 改为 `av_read_frame`（位置信息可靠） |
| `avcodec_close` 已移除 | 改用 `avcodec_free_context` |
| MLP 编码器只接受 planer 采样格式 | 改写 `encode_fmt_s16/s32` 为按 plane 填充 |
| 24-bit 被直接跳过 | 放开限制，改用 `AV_SAMPLE_FMT_S32P` |
| 编码器按裸 PCM 读取 | 新增 `wav_data_offset()` 跳过 WAV 头 |
| SoX 14.4.2 API 不兼容 | `-DWITHOUT_sox` + `libsoxconvert.c` 桩函数 |
| `close_handles` 同名冲突/参数不一致 | 改名内联版本、统一为 4 参按值 |
| Makefile 注入 `-DWITHOUT_sox` 的写法失效 | 改为从 `CFLAGS` 传入 |

### 关键修复：整个时间轴曾经完全失效

**症状**：进度条无法拖动、部分段落加速播放。

**根因**：MLP 光盘的 PES 头 PTS 依赖 `mlp_layout[].nb_samples` 逐扇区累积。
而 `avcodec_receive_frame()` 在返回 `EAGAIN`/`EOF` 前会 **先 `av_frame_unref(frame)`**，
导致循环外再读 `frame->nb_samples` 恒为 0：

```
[DIAG_ACC] #0      frame->nb_samples=0  totnbsamples(前)=0
[DIAG_ACC] #5000   frame->nb_samples=0  totnbsamples(前)=0
```

于是 `numsamples=0` → `PTS_length=0` → 每个扇区的 PTS 都是常量 98：

```
扇区        0    1    2    3   10  100  1000  10000  100000  524287
PTS 值     98   98   98   98   98   98    98     98      98      98
```

**修复**：在成功收到帧的当下立即保存采样数（`g_last_nb_samples`），
布局累积改用该值；写入点位于 `if/else` 链之外以免孤立 `else`。

修复后同一首曲子的对照：

| 指标 | 修复前 | 修复后 |
|------|--------|--------|
| `PTS_length` | 0 | 16,084,725 |
| 扇区 PTS | 恒定 98 | 98 → 16,084,673 递增 |
| 时间跨度 | 0 秒 | 178.718 秒（与源一致） |
| 异常步长 | 100% | 0.000% |

### 另一个修复：末轨 AOB 少 4 字节

`dvda-author` 写末轨最后一个 pack 时可能少写几字节填充，使 AOB 不是 2048 的整数倍
（实测组3 为 685,109,244 字节，少 4 字节），而 IFO 已按整扇区声明。
`02_build.py` 现在会在打包前把 AOB 补零至扇区边界，使两者严格一致。
（已验证这 4 字节仅为填充，音频数据完整。）

编译：执行 `build_dvda_author_mlp.sh` 即可（幂等，可重复运行），
它按顺序应用 `patches/` 下的 5 个补丁脚本并链接系统 FFmpeg 8。

> 注意：非 core 构建下不能传 `-9`/`-X`（会因 `make_absolute` 返回 NULL 而崩溃）；
> `--encode` 由本脚本的 FFmpeg 预编码替代，直接把 `.mlp` 作为 `-g` 输入交给 dvda-author。

## 原版 dvda-author 编译补丁

`/opt/dvda-author` 已打上 `fixes/` 下的 3 个补丁,修复 4 处上游 bug:

1. `fix_merged_and_audio_close.py` — 删除半成品 `interleave_*_merged` 函数 + `audio_close_merged` 签名/前置声明
2. `fix_close_handles.py` — `winport.h` 补 `close_handles()` 实现
3. `fix_secure_mkdir.py` — `secure_mkdir` 空路径死循环

另需:`local` 目录软链接到 `local.ubuntu.20.10`(内含 ffmpeg 4.x 预编译静态库,项目 API 匹配)。

---

## 成品

| 盘 | 曲目 | ISO 大小 | DVD5 剩余 |
|----|------|----------|-----------|
| 盘1 | 91 首 | 4,600,489,984 B (4.28 GiB) | 106,829,824 B |
| 盘2 | 56 首 | 3,124,076,544 B (2.91 GiB) | 1,583,243,264 B |

- 输出目录:`D:\鸣潮DVD_Audio\`
- 每盘只含 `AUDIO_TS`(无 `VIDEO_TS`)
- 盘1 因轨数较多拆为 3 个音频组(65 / 12 / 14),盘2 为 2 个组

## 无损性验证记录

1. MLP 解码后 PCM 与**源音源**（施加与编码时相同的重采样链）**逐字节一致**
2. `dvda-author` 编码输出与 `ffmpeg` CLI 输出 **逐字节一致**
3. 从成品 ISO 提取的音轨 MLP 与源 MLP **MD5 一致**
   (`efbf4eb0430047e1e6f5e46a0131acc3`)

## 脚本清单

```
dvda_scripts/
├── build.sh                    # 一键流水线
├── 01_prepare.py               # 步骤1:扫描 + 归一化 + 解码校验 + manifest(不写 WAV)
├── 02_build.py                 # 步骤2:直读音源编码 MLP → 分盘 → 出盘 → 补零 → 打包 ISO
├── alac_endfix.py              # Apple ALAC「未压缩帧缺 END 标记」检测与修复
├── verify.sh                   # 成品校验(容量 / 审计 / 时间轴 / 无损)
├── audit_disc.py               # 光盘一致性审计(扇区/PTS/轨边界)
├── check_aob_pts.py            # AOB 逐扇区 PTS 检查
├── verify_pts_length.py        # 逐轨 PTS_length 与源时长比对
├── build_dvda_author_mlp.sh    # 重编带 24-bit MLP 支持的 dvda-author
├── patches/                    # 上面脚本用到的源码补丁(勿单独改名)
│   ├── patch_base.py           #   winport 冲突 / SoX 守卫
│   ├── patch_read.py           #   读取端 FFmpeg 8 迁移 + g_last_nb_samples 修复
│   ├── patch_read2.py          #   提取分支读取循环
│   ├── patch_encode.py         #   编码端 planer 格式 + 放开 24-bit
│   └── patch_wavhdr.py         #   跳过 WAV 头
├── fixes/                      # /opt/dvda-author 的上游 bug 补丁(构建前提)
└── README.md
```

## 校验

```bash
bash verify.sh            # 全部
bash verify.sh capacity   # 仅 DVD5 容量与结构
bash verify.sh audit      # 仅光盘一致性审计
bash verify.sh timeline   # 仅时间轴抽查
bash verify.sh lossless   # 仅 MLP 无损性
```

`audit_disc.py` 按音频组独立核对（扇区号在各组内从 0 起）：

| 检查 | 内容 |
|------|------|
| A | 组内 AOB 扇区总数 == 该组轨道表最大末扇区 + 1 |
| B | 组内各轨扇区首尾相接（无缝无叠） |
| C | 每个扇区都有 PTS |
| D | 每个 PTS 下降点恰好落在某轨的首个扇区 |
| E | 各轨起点的 PTS 取值 |

## 步骤1 的解码完整性校验

**为什么需要**：ffmpeg 在 ALAC 等格式解码出错时会**静默跳过帧但退出码仍为 0**。
若不校验，MLP → ISO 会一路"成功"，实际却缺失音频（每次丢 4096 采样 ≈ 85 ms @48k）。

**判定方式**（`01_prepare.py`）：

| 条件 | 判定 |
|------|------|
| stderr 出现解码错误关键字 | **FAIL** |
| 解码采样数比源声明少 > 50 ms 对应值 | **FAIL** |
| 采样数差异 > 5 ms 对应值 | WARN |
| `astats` 未输出采样数（校验手段本身失效） | **FAIL** |
| 音频组内声道数不一致 | **FAIL** |
| 其余 | 通过 |

错误关键字包括：`Error submitting packet to decoder`、`invalid element`、
`Error while decoding`、`Invalid data found`、`CRC mismatch`、`corrupt`、
`not implemented` 等。

**自动修复**：检测到解码异常时，先尝试 `alac_endfix.py`。
修复成功则重新校验，采样数必须**精确等于**容器声明值。
**原文件绝不修改**，产物写入 `$DVDA_ALAC_FIX_DIR`。

**失败时的行为**：

1. 打印每个失败文件的曲名、原因、缺失毫秒数与 ffmpeg 原始报错
2. 写入完整报告到 `/root/dvda-build/decode_report.txt`
3. **不生成 `manifest.json`**（并删除旧的），以非零码退出
4. `build.sh` 的 `set -e` 会立即中止，不会进入步骤2

**实测效果**：

```
[PASS] 正常 FLAC 48k/24        期望 10267032 / 实解 10267032  差 +0      报错 0
[PASS] 正常 FLAC 44.1k/24→48k  期望 11636770 / 实解 11636770  差 +0      报错 0
[修复] Apple ALAC 日文版       补 3 帧 → 10253856 变 10266144  报错 6 → 0
[修复] Apple ALAC 英文版       补 1 帧 → 10262048 变 10266144  报错 2 → 0
[修复] Apple ALAC 韩文版       补 7 帧 →  9364628 变  9393300  报错 14 → 0
```

同批的正常 FLAC 零误报（差 +0 采样、报错 0 处）。

## Apple ALAC「未压缩帧缺 END 标记」（重要）

**症状**：ffmpeg 报 `invalid element channel count` 并静默丢帧，退出码仍为 0，
但 **foobar2000 / Apple 播放器播放完全正常**。

**根因**：Apple 的 ALAC 编码器会周期性插入「未压缩帧」（raw PCM，用于随机访问
定位），间隔恰好 **32.000 秒**（每 375 帧）。这类帧的位数为

```
帧头 23 位 + n_samples × channels × sample_size
```

其后应按规范写 END 元素（3 位 `111`），但 Apple 写的是 `000`。ffmpeg 于是
读成 SCE（单声道）元素，第二次循环时声道数溢出而丢掉整帧。

**判据**（足以排除"源损坏"）：

| 检验 | 结果 |
|------|------|
| 包位置连续覆盖 `mdat` | 2507/2507，100.00% |
| 失败帧占比 | 3/2507 = **0.12%** |
| 包大小 | `24580 = 4 + 4096×2×3`（24-bit）/ `16388 = 4 + 4096×2×2`（16-bit），**分毫不差** |
| `-ignore_editlist` / `-advanced_editlist 0` / `-threads 1` | 报错数不变 |

**修复**：`alac_endfix.py` 把 END 写回（**只改帧尾填充的 3 位，不动任何样本数据**）：

```
原始:  200003cc 598a6a34 ... [样本数据] ... 000
修复:  200003cc 598a6a34 ... [样本数据] ... 111
```

| 文件 | 修复前 | 修复后 | 容器声明 | 报错 |
|------|--------|--------|----------|------|
| 日文版 | 10,253,856 | 10,266,144 | 10,266,144 ✔ | 6 → 0 |
| 英文版 | 10,262,048 | 10,266,144 | 10,266,144 ✔ | 2 → 0 |
| 韩文版 | 9,364,628 | 9,393,300 | 9,393,300 ✔ | 14 → 0 |

**无损性两层验证**（11 个坏帧全部通过）：

- **V1** 每个未压缩帧的「包内原始 PCM」与解码结果**逐样本吻合**
- **V2** 按包序并行推进，正常段逐字节一致，修复侧增量恰好等于补回的帧数

```bash
python3 alac_endfix.py --check x.m4a       # 只检测
python3 alac_endfix.py x.m4a x.fixed.m4a   # 修复到新文件
```

> **教训**：「播放正常但转码报错」是解码器问题的强信号。
> 最初仅凭 ffmpeg 报错就判定源文件损坏，是错的。

## 已知限制

- `dvda-author` 的 ATSI 表缓冲固定 3 扇区，单组超过约 70 轨会栈溢出，故 `GROUP_TRACK_LIMIT=64`
- 非 core 构建下不能传 `-9`/`-X`（会因 `make_absolute` 返回 NULL 崩溃）
- MLP 只支持 `s16p`/`s32p`，即 16-bit 与 24-bit；其他位深需先转换
- `--aob-extract` 提取音频时会在收尾阶段段错误退出（上游已知行为），
  但提取出的音轨数据完整（MD5 与源一致），不影响光盘播放
- 源文件若**真正损坏**（数据缺失），本流水线无法修复，只能在校验阶段拦下。
  但 Apple ALAC 的「缺 END 标记」问题**可以自动修复**（见上文，原文件不改动）

