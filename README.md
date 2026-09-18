# DVD-Audio Maker

把高解析度音频（FLAC / ALAC-M4A）制作成 **DVD-Audio** 光盘 ISO 的工具链。

针对 **24-bit / 48 kHz、44.1 kHz** 音源，使用 **MLP（Meridian Lossless Packing）无损压缩**，
把约 9 GB 的 WAV 压到约 7 GB，从而装入两张单层 DVD-5。

> 本仓库**不包含任何音频文件**。你需要自备音源。

---

## 它解决了什么

DVD-Audio 制作工具链（[dvda-author](https://github.com/fabnicol/dvda-author)）停留在 2020 年，
直接使用会遇到若干硬障碍。本仓库的主要内容就是这些障碍的**修复补丁与验证脚本**。

### 1. 启用 24-bit 无损 MLP 压缩

| 问题 | 处理 |
|------|------|
| 原版 `dvda-author` 是 core 构建，**不含 MLP** | 重新编译，链接系统 FFmpeg |
| 随包 FFmpeg 4.2.4 的 MLP 编码器**只支持 16-bit** | 改用系统 FFmpeg 8 |
| `mlp.c` 是按 FFmpeg 4.x API 写的 | 迁移到 8.x（`ch_layout`、`AVPacket`、`av_read_frame` 等） |
| 24-bit 被硬编码跳过 | 放开，改用 `AV_SAMPLE_FMT_S32P` |
| MLP 编码器只接受 planer 格式 | 重写填充逻辑为按 plane 写入 |
| 编码器按裸 PCM 读取 | 新增 `wav_data_offset()` 跳过 WAV 头 |
| SoX 14.4.2 API 不兼容 | `-DWITHOUT_sox` + 桩函数 |

**结果**：24-bit 音源压缩率约 **18%**，且**解码后与源 PCM 逐字节一致**。

### 2. 修复时间轴（播放加速 / 进度条不可拖）

这是最隐蔽的一个。MLP 光盘的 PES 头时间戳依赖逐扇区的采样数累积，而
`avcodec_receive_frame()` 在返回 `EAGAIN` 前会**先 `av_frame_unref(frame)`**，
导致循环外读 `frame->nb_samples` 恒为 0：

```
[DIAG] rank=19867  layout_size=19868  totnbsamples=0
[DIAG] nb_samples: [0]=0 [1]=0 [2]=0 [last]=0
```

→ `numsamples=0` → `PTS_length=0` → **每个扇区的 PTS 都是常量 98**：

```
扇区        0    1    2   10  100  1000  10000  100000  524287
PTS 值     98   98   98   98   98    98     98      98      98
```

播放器拿不到推进的时间戳，表现就是**进度条无法拖动**，并可能**加速播放**。

**修复**：在成功收到帧的当下立即保存采样数（`g_last_nb_samples`）。

修复前后对照（同一首曲子）：

| 指标 | 修复前 | 修复后 |
|------|--------|--------|
| `PTS_length` | 0 | 16,084,725 |
| 扇区 PTS | 恒定 98 | 98 → 16,084,673 递增 |
| 时间跨度 | 0 秒 | 178.718 秒（与源一致） |
| 异常步长 | 100% | 0.000% |

### 3. 拦截源文件损坏

FFmpeg 解码出错时（如损坏的 ALAC 帧）会**跳过损坏数据但退出码仍为 0**。
不加校验的话，MLP → ISO 会一路"成功"，实际却缺失音频。

`01_prepare.py` 现在会校验并在失败时**拒绝生成 manifest 并以非零码退出**。

### 4. 拦截位深与声道不一致

- 直连编码时 MLP 编码器会沿用源位深，16-bit 源会被编成 16-bit MLP 混入 24-bit 组
- 同组声道数不一致（如单声道混入立体声）会产出非法音频组

现在两者都会在编码后被 `ffprobe` 复核，不一致即失败。

### 5. 其他修复

- `dvda-author` 的 ATSI 表缓冲固定 3 扇区，**单组超过约 70 轨会栈溢出**
- 末轨 AOB 可能少写几字节填充，导致文件不是 2048 的整数倍（已自动补齐）

---

## 目录结构

```
DVD-Audio-Maker/
├── README.md
├── LICENSE                      # GPL-3.0 全文
├── .gitignore
├── build.sh                     # 一键流水线
├── 01_prepare.py                # 步骤1：扫描音源 + 专辑归一化 + 解码校验
├── 02_build.py                  # 步骤2：MLP 编码 → 分盘 → 出盘 → 打包 ISO
├── verify.sh                    # 成品校验入口
├── audit_disc.py                # 光盘一致性审计
├── check_aob_pts.py             # AOB 逐扇区 PTS 检查
├── verify_pts_length.py         # 逐轨 PTS_length 与源时长比对
├── build_dvda_author_mlp.sh     # 重编支持 24-bit MLP 的 dvda-author
├── patches/                     # 重编所需的源码补丁（5 个）
├── fixes/                       # /opt/dvda-author 的上游 bug 补丁（3 个）
└── docs/
    ├── TROUBLESHOOTING.md       # 问题诊断记录与修复细节
    └── LICENSING.md             # 许可状况、第三方归属与法律说明
```

---

## 环境要求

- **WSL2 + Ubuntu 24.04 / 26.04**（脚本按 Linux 路径编写）
- `python3`、`ffmpeg`（需含 `mlp` 编码器）、`ffprobe`
- `make`、`gcc`、`autoconf`（重编 `dvda-author` 用）
- `xorriso`（校验工具）
- FFmpeg 8 开发库：`libavcodec-dev`、`libavformat-dev`、`libavutil-dev`、`libswresample-dev`

检查 FFmpeg 是否支持 MLP：

```bash
ffmpeg -hide_banner -encoders | grep mlp
# 应输出:  A..X.D mlp   MLP (Meridian Lossless Packing)
```

---

## 快速开始

### 1. 准备 dvda-author 源码

```bash
git clone https://github.com/fabnicol/dvda-author /opt/dvda-author
```

### 2. 打上游 bug 补丁

```bash
cd /opt/dvda-author
REPO=/path/to/DVD-Audio-Maker
python3 $REPO/fixes/fix_merged_and_audio_close.py
python3 $REPO/fixes/fix_close_handles.py
python3 $REPO/fixes/fix_secure_mkdir.py
./configure --enable-core-build && make -j2
```

### 3. 重编带 MLP 支持的版本

```bash
bash $REPO/build_dvda_author_mlp.sh
# 产物: /root/dvda-author-mlp8/src/dvda-author-dev
```

该脚本是**幂等**的，可重复运行。

### 4. 路径配置

所有路径都可用**环境变量**覆盖，脚本内保留一份默认值（作者的原始环境）。
换环境时**无需改代码**，设好变量即可：

```bash
export DVDA_SRC="/path/to/音源"          # 音源目录（只读）
export DVDA_BUILD_DIR="/root/dvda-build" # 构建根目录
export DVDA_FINAL_DIR="/mnt/d/输出"      # ISO 最终输出目录
export DVDA_AUTHOR="/root/dvda-author-mlp8/src/dvda-author-dev"
export DVDA_MKISOFS="/opt/dvda-author/local.ubuntu.20.10/bin/mkisofs"
```

完整清单：

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `DVDA_SRC` | `/mnt/c/Users/yyz57/Music/鸣潮先约电台` | 音源目录（只读） |
| `DVDA_MANIFEST` | `/root/dvda-build/manifest.json` | 清单输出 |
| `DVDA_REPORT` | `/root/dvda-build/decode_report.txt` | 解码完整性报告 |
| `DVDA_BUILD_DIR` | `/root/dvda-build` | 构建根目录（日志/审计/临时） |
| `DVDA_OUT_ROOT` | `/root/dvda-build/out` | 出盘工作目录 |
| `DVDA_TMP_ROOT` | `/root/dvda-build/tmp` | 临时目录 |
| `DVDA_ISO_DIR` | `/root/dvda-build/iso` | ISO 暂存目录 |
| `DVDA_MLP_DIR` | `/root/dvda-build/mlp` | MLP 缓存目录 |
| `DVDA_MLP_INDEX` | `/root/dvda-build/mlp_index.json` | MLP → 源文件/时长/重采样索引 |
| `DVDA_FINAL_DIR` | `/mnt/d/鸣潮DVD_Audio` | ISO 最终输出目录 |
| `DVDA_AUTHOR` | `/root/dvda-author-mlp8/src/dvda-author-dev` | 自编译 dvda-author |
| `DVDA_MKISOFS` | `/opt/dvda-author/local.ubuntu.20.10/bin/mkisofs` | patched mkisofs |
| `DVDA_FFMPEG` | `ffmpeg` | ffmpeg 可执行文件 |
| `DVDA_AUTHOR_SRC` | `/root/dvda-author-mlp8` | 编译目录 |
| `DVDA_AUTHOR_ORIG` | `/opt/dvda-author` | 原始源码目录 |
| `DVDA_ISO_PREFIX` | `Wuthering_Waves_Singles_EPs` | ISO 文件名前缀 |

> 缓存文件名由 `manifest.json` 中的 `name` 字段决定，两步脚本之间通过该字段对齐。

### 5. 运行

```bash
bash build.sh
```

或分步：

```bash
python3 01_prepare.py   # 完成后检查解码完整性报告
python3 02_build.py     # 出盘并打包 ISO
```

---

## 处理逻辑

1. **扫描** FLAC / M4A，用 `ffprobe` 读取参数与标签
2. **专辑归一化**：同一专辑若采样率/位深不一致，以「多数采样率 + 该采样率下多数位深」
   为目标重采样少数曲目，保证整张专辑在同一音频组内连续播放
3. **分组排序**：按 (采样率, 位深) 分组，组内按发布日期 + 曲序排序
4. **解码完整性校验**：每首跑一次「只解码不落盘」的 ffmpeg（`-f null -` + `astats`），
   比对解码采样数与源声明采样数
5. **MLP 编码**：直接以**源文件**为输入（无损，结果缓存复用），
   需归一化的曲目在同一命令内完成 soxr 重采样
6. **分盘**：按专辑发布顺序，填满盘1 再装盘2；**专辑绝不拆散**
7. **出盘**：`dvda-author` 生成 `AUDIO_TS`，补齐 AOB 扇区边界
8. **打包**：`mkisofs -dvd-audio` 生成 ISO，复制到输出目录

### 为什么不再生成 WAV（以及它带来的两个坑）

传统做法是「源 → WAV → MLP」。实测两条路径的输出**逐字节一致**：

| 场景 | 源 → MLP | 源 → WAV → MLP | 结果 |
|------|----------|----------------|------|
| 48k/24 FLAC（无需重采样） | 52,946,982 B | 52,946,982 B | 一致 ✔ |
| 44.1k/24 FLAC（soxr → 48k） | 60,950,716 B | 60,950,716 B | 一致 ✔ |
| 44.1k/**16** FLAC（soxr → 48k） | 61,018,364 B | 61,018,364 B | 一致 ✔ |
| ↑ 解码后 PCM | 69,820,800 B | 69,820,800 B | 一致 ✔ |

WAV 只是中转，去掉后可省下约 **9 GB**（147 首 × 平均 60 MB）落盘与一轮读写 I/O。

但 WAV 中转阶段**隐式承担了两个约束**，去掉后必须显式补回，否则会静默产出错误结果：

#### 坑 1：位深不再被强制（会导致非法音频组）

旧流程用 `-c:a pcm_s24le` 把 WAV 统一成 24-bit，MLP 编码器读这个 WAV 自然就是 24-bit。
直连后 MLP 编码器会**沿用源的位深**，于是 44.1k/16 的源重采样到 48k 后**仍是 16-bit**，
混进 24-bit 的音频组 —— dvda-author 会把这种参数不一致的光盘照样做出来：

```
1  04  48000  16  2 L-R  ...   ← 错：整组是 24-bit，这一轨却是 16-bit
```

**修复**：显式指定 `-sample_fmt s32p`（MLP 只接受 planer 名，`s32` 会报
`Specified sample format s32 is not supported`），并在编码后用 `ffprobe` 复核
采样率与位深，不一致就删除文件并报错。

#### 坑 2：时长不再可回读（导致校验失效）

MLP 容器不记录 duration（`ffprobe` 返回 `N/A`），无法像 WAV 那样回读时长做校验。

**修复**：改用 `astats` 采样数比对（见下文「解码完整性校验」），并由 `02_build.py`
输出 `mlp_index.json` 记录「MLP → 源文件 / 声明时长 / 重采样目标」，供热时长校验脚本使用。

### 声道数约束

DVD-Audio 同一音频组内所有曲目须同声道数。本工具链**不做声道转换**
（单声道与立体声无法无损互转），因此 `01_prepare.py` 会检查组内声道是否一致，
不一致直接失败。

实测本项目的全部 147 个音源均为 **2ch / stereo**（FL 前左 + FR 前右，即 dvda-author
报的 `L-R`），无单声道、无多声道、无非标准布局：

| 数量 | 声道 | 布局 | 采样率 | 位深 |
|------|------|------|--------|------|
| 128 | 2 | stereo | 48000 | 24 |
| 13 | 2 | stereo | 44100 | 24 |
| 4 | 2 | stereo | 44100 | 16 |
| 1 | 2 | stereo | 48000 | 16 |
| 1 | 2 | stereo | 96000 | 24 |

归一化后最终只有两个音频组：**48000/24（131 首）** 与 **44100/24（16 首）**。

---

## 校验

```bash
bash verify.sh            # 全部
bash verify.sh capacity   # DVD5 容量与结构
bash verify.sh audit      # 光盘一致性审计
bash verify.sh timeline   # AOB 时间轴抽查
bash verify.sh lossless   # MLP 无损性
```

`audit_disc.py` 按音频组独立核对（扇区号在各组内从 0 起）：

| 检查 | 内容 |
|------|------|
| A | 组内 AOB 扇区总数 == 该组轨道表最大末扇区 + 1 |
| B | 组内各轨扇区首尾相接（无缝无叠） |
| C | 每个扇区都有 PTS |
| D | 每个 PTS 下降点恰好落在某轨的首个扇区 |

实测结果（147 首 / 2 张盘）：

```
盘/组   轨数   AOB扇区   轨道表扇区  PTS下降  轨边界
盘1 组1    65    1588632    1588632       64      64
盘1 组2    12     322656     322656       11      11
盘1 组3    14     334526     334526       13      13
盘2 组1    54    1467081    1467081       53      53
盘2 组2     2      57836      57836        1       1

审计结论: 全部通过 ✔
```

---

## 解码完整性校验

**判定规则**（`01_prepare.py`）：

| 条件 | 判定 |
|------|------|
| stderr 出现解码错误关键字 | **FAIL** |
| 解码采样数比源声明少 > 50 ms 对应值 | **FAIL** |
| 采样数差异 > 5 ms 对应值 | WARN |
| `astats` 未输出采样数（校验手段本身失效） | **FAIL** |
| 音频组内声道数不一致 | **FAIL** |
| 其余 | 通过 |

基准值 = `源声明时长 × 目标采样率`（重采样后按目标采样率计算）。

关键字涵盖 `Error submitting packet to decoder`、`invalid element`、
`Error while decoding`、`Invalid data found`、`CRC mismatch`、`corrupt`、
`not implemented` 等。

**失败时**：打印问题清单 → 写入 `decode_report.txt` → **不生成 `manifest.json`**
→ 以非零码退出 → `build.sh` 的 `set -e` 立即中止。

实测（同一批音源）：

```
[PASS] 正常 FLAC 48k/24        源声明 213.897s × 48000 = 10267032 / 实解 10267032  差 +0      报错 0
[PASS] 正常 FLAC 44.1k/24→48k  源声明 242.433s × 48000 = 11636770 / 实解 11636770  差 +0      报错 0
[FAIL] 损坏 M4A (韩文版)       源声明 213.000s × 48000 = 10224000 / 实解 10192792  差 -31208  报错 14
[FAIL] 损坏 M4A (英文版)       源声明 213.878s × 48000 = 10266144 / 实解 10262048  差 -4096   报错 2
```

正常 FLAC **零误报**（差 +0 采样、报错 0 处）。

---

## 已知限制

- `dvda-author` 的 ATSI 表缓冲固定 3 扇区，**单组超过约 70 轨会栈溢出**，
  故 `GROUP_TRACK_LIMIT = 64`，超出时按专辑边界再拆一组
- 非 core 构建下**不能传 `-9`/`-X`**（会因 `make_absolute` 返回 NULL 崩溃）
- MLP 只支持 `s16p` / `s32p`，即 16-bit 与 24-bit；其他位深需先转换
- `--aob-extract` 提取音频时会在收尾阶段段错误退出（上游已知行为），
  但提取出的音轨数据完整（MD5 与源一致），不影响光盘播放
- 源文件若本身损坏（如不完整的 ALAC），本工具链**无法修复**，只会在校验阶段拦下
- 光盘仅含 `AUDIO_TS`（纯 DVD-Audio），不含 `VIDEO_TS` 与菜单

---

## 许可与法律

**本仓库以 [GPL-3.0](LICENSE) 发布。**

原因：`patches/` 与 `fixes/` 是对 [dvda-author](https://github.com/fabnicol/dvda-author)
（GPL-3.0）源码的**修改**，属衍生作品，需与其许可保持一致。

| 组件 | 许可 |
|------|------|
| dvda-author | GPL-3.0（源码头部为 "GPL v2 or later"） |
| 本仓库补丁与脚本 | GPL-3.0 |
| FFmpeg | LGPL-2.1+；Ubuntu 构建含 GPL-2+ 部分 |
| MLP 编解码器 | LGPL-2.1+ |
| mkisofs (cdrtools) | CDDL（本仓库不分发） |

### ⚠️ MLP 编码的法律提示

MLP（Meridian Lossless Packing）是 **Dolby** 的技术。FFmpeg 的 MLP 编码器被标记为
**experimental**（这正是必须加 `-strict -2` 的原因），dvda-author 的帮助文本也注明其
"subject to the same legal restrictions as those applying to the MLP ffmpeg encoder"。

本工具链虽改用 ffmpeg CLI 预编码 MLP，但**用的是同一个编码器**，限制同样适用。
请自行确认所在司法辖区的相关规定。

### 关于音频内容

**本仓库不含任何音频、音乐或封面文件**，也不包含任何解密或绕过版权保护的功能。
若源文件本身损坏，流水线会在解码校验阶段如实报告并中止。

详见 [`docs/LICENSING.md`](docs/LICENSING.md)。

---

## 文档

- [`docs/TROUBLESHOOTING.md`](docs/TROUBLESHOOTING.md) — 全部问题的诊断过程与修复细节
- [`docs/LICENSING.md`](docs/LICENSING.md) — 许可状况、第三方归属与法律说明
