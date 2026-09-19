# 故障排查记录

本文档汇总开发过程中遇到的实际问题、诊断方法与修复方案。
每个案例都包含可复现的验证命令。

---

## 目录

1. [播放加速 / 进度条无法拖动](#1-播放加速--进度条无法拖动)
2. [编译 `dvda-author` 的各类错误](#2-编译-dvda-author-的各类错误)
3. [`stack smashing detected`（轨数过多）](#3-stack-smashing-detected轨数过多)
4. [`--aob-extract` 段错误](#4---aob-extract-段错误)
5. [ffmpeg 解码丢帧（Apple ALAC 未压缩帧缺 END 标记）](#5-ffmpeg-解码丢帧apple-alac-未压缩帧缺-end-标记)
6. [末轨 AOB 少几字节](#6-末轨-aob-少几字节)
7. [位深丢失（产生非法音频组）](#7-位深丢失产生非法音频组)
8. [MLP 不记录时长（校验失效风险）](#8-mlp-不记录时长校验失效风险)
9. [声道数不一致（预防性检查）](#9-声道数不一致预防性检查)
10. [审计误报：取到了上一次构建的日志](#10-审计误报取到了上一次构建的日志)
11. [校验脚本的组匹配错误（ATS_01_1.AOB 是组1）](#11-校验脚本的组匹配错误ats_01_1aob-是组1)
12. [诊断手法速查](#12-诊断手法速查)

---

## 1. 播放加速 / 进度条无法拖动

### 症状

- foobar2000 播放时**进度条无法拖动**
- 某些区段听起来**被加速**
- 时间显示异常

### 诊断

直接解析 AOB 内每个扇区的 PES 头时间戳，检查是否随播放推进：

```bash
python3 check_aob_pts.py /path/to/ATS_01_1.AOB
```

**异常时的输出**：

```
文件: ATS_01_1.AOB
总扇区: 524288  含PTS扇区: 524288
首个 PTS: 98  末个 PTS: 98          ← 首末相同！
时间跨度: 0.000 秒
步长: 最小 0 最大 0  负步长 0 个  零步长 524287 个
异常步长 数量: 524287 占比 100.000%
```

或抽查若干位置的扇区：

```
扇区        0    1    2    3   10  100  1000  10000  100000  524287
PTS 值     98   98   98   98   98   98    98     98      98      98
```

**正常时**：

```
首个 PTS: 98  末个 PTS: 16084673
时间跨度: 178.718 秒
步长: 最小 599 最大 4125  负步长 0 个  零步长 0 个
异常步长 数量: 0 占比 0.000%
```

### 根因

MLP 光盘的 PES 头时间戳来自 `info->pts[]` 表，该表由 `calc_PTS_DTS_MLP()`
依据 `mlp_layout[].nb_samples`（逐扇区累积采样数）填充。

而在 `decode_mlp_file()` 中，布局累积读取的是 `frame->nb_samples`：

```c
cumbytes_written = decode(context, codec, codecpar, packet, frame, ...);
...
totnbsamples += frame->nb_samples;      // ← 恒为 0
```

问题在于 FFmpeg 的 `avcodec_receive_frame()` 在返回 `EAGAIN` / `EOF` 前，
**会先调用 `av_frame_unref(frame)`**，把帧信息清零。而 `decode()` 内部会一直循环
到收到 EAGAIN 才返回，于是循环外再读 `frame->nb_samples` 必然是 0。

插桩验证：

```
[DIAG_ACC] #0     frame->nb_samples=0  totnbsamples(前)=0
[DIAG_ACC] #5000  frame->nb_samples=0  totnbsamples(前)=0
[DIAG_ACC] #15000 frame->nb_samples=0  totnbsamples(前)=0
```

但 `decode()` 内部明明是正常的：

```
[DIAG] 收到帧 #1: nb_samples=40 format=2 ch=2     ← 有值
```

链路后果：

```
nb_samples 恒为 0
  → info->numsamples = 0
  → info->PTS_length = 0
  → calc_PTS_DTS_MLP() 算出的 pts[] 全为同一个值
  → 每个扇区 PTS 都是 98
  → 播放器无法定位进度
```

### 修复

在**成功收到帧的当下**立即保存采样数（`patches/patch_read.py`）：

```c
static int g_last_nb_samples = 0;      // 文件头部新增

ret = avcodec_receive_frame(context, frame);
if (ret == AVERROR(EAGAIN) || ret == AVERROR(EOF))
  break;

/* 必须在 EAGAIN 之前取走 nb_samples */
g_last_nb_samples = frame->nb_samples;
```

布局累积改用该变量：

```c
totnbsamples += g_last_nb_samples;
```

> **注意**：插入点必须落在 `if/else` 链**之外**，否则会孤立 `else` 分支导致
> `error: 'else' without a previous 'if'`。

### 修复前后对照

| 指标 | 修复前 | 修复后 |
|------|--------|--------|
| `PTS_length` | 0 | 16,084,725 |
| 扇区 PTS | 恒定 98 | 98 → 16,084,673 递增 |
| 时间跨度 | 0 秒 | 178.718 秒（与源完全一致） |
| 异常步长占比 | 100% | 0.000% |

### 验证

```bash
# 单曲验证
bash verify.sh timeline

# 全部 AOB：扇区数、轨间连续、PTS 完整性、下降点归位
bash verify.sh audit
```

---

## 2. 编译 `dvda-author` 的各类错误

按遇到顺序排列。运行 `build_dvda_author_mlp.sh` 会自动处理全部。

### 2.1 SoX API 不兼容

```
libsoxconvert.c:150: error: assignment to 'int *' from 'int'
libsoxconvert.c:152: error: request for member 'signal' in something
                        not a structure or union
```

系统 SoX 头文件与源码期望的 14.4.2 版本不匹配。

**修复**：编译时关闭 SoX 并加桩函数（`patches/patch_base.py`）。

```bash
CFLAGS="-DWITHOUT_sox"     # 必须放 CFLAGS
```

> 该工程的 Makefile 用 `$(if test ...)` 注入 `-DWITHOUT_sox`，这段写法是坏的，
> 因此 `CPPFLAGS` 不生效，**只能从 CFLAGS 传入**。

### 2.2 `close_handles` 命名冲突

```
winport.c:327: error: conflicting types for 'close_handles';
               have 'void(int *, int *, int *, int *, int *)'
```

`winport.h` 里有一个 4 参数的内联版本与 `winport.c` 的 5 参数实现同名。

**修复**：把内联版本改名为 `close_file_descriptors`，并补上 Linux 下的正确声明；
实现改为**按值传参**（调用点传的是 `FILE_DESCRIPTOR` 即 `int` 值）。

### 2.3 FFmpeg 版本错配

```
mlp.c:86:  error: 'AVFrame' has no member named 'channels'
mlp.c:123: error: 'AVFrame' has no member named 'pkt_pos'
mlp.c:553: error: implicit declaration of function 'avcodec_close'
```

`mlp.c` 按 FFmpeg 4.x API 编写，却链接到了 FFmpeg 8。

**修复**：指向随包自带的 FFmpeg 4.2.4（`local.ubuntu.20.10`）可解决此错误，
但**那样只能支持 16-bit**。要支持 24-bit 必须迁移到 FFmpeg 8，见下一节。

### 2.4 FFmpeg 8 API 迁移要点

| FFmpeg 4.x | FFmpeg 8.x |
|------------|------------|
| `ctx->channels` | `ctx->ch_layout.nb_channels` |
| `ctx->channel_layout` | `ctx->ch_layout` |
| `frame->pkt_pos / pkt_duration / pkt_size` | **已移除**，需自行记录 |
| `av_parser_parse2()` | `av_read_frame()` |
| `avcodec_close()` | `avcodec_free_context()` |
| `AV_SAMPLE_FMT_S16 / S32` | `AV_SAMPLE_FMT_S16P / S32P`（planer） |

其中两个是**功能性**问题，不只是改名：

**① `pkt_pos` 的替代**：MLP 扇区布局依赖它。改为自行记录输入包位置：

```c
static int64_t g_last_pkt_pos = 0;
...
g_last_pkt_pos = packet->pos;      // 读包时保存
```

**② planer 格式填充**：原代码按 packed 写入 `frame->data[0]`，
planer 格式需按通道分别写入 plane：

```c
const int nch = c->ch_layout.nb_channels;
for (int ch = 0; ch < nch; ++ch)
  memset(frame->data[ch], 0, nframe * 4);          // s32p: 每样本 4 字节

for (int s = 0; s < nframe; ++s)
  for (int ch = 0; ch < nch; ++ch) {
    uint8_t *dst = (uint8_t *)frame->data[ch] + (size_t)s * 4 + 1;
    fread(dst, 1, 3, in_fp);                        // 24-bit 存入高 3 字节
  }
```

### 2.5 链接错误

```
cannot find /root/dvda-author-mlp8/local/lib/libswresample.a
```

Makefile 硬编码了 `local/lib/*.a` 路径。

**修复**：建立 `local/lib` 并链接系统库；系统没有 `libavfilter.so` 时从链接行移除。

```bash
for l in avcodec avformat avutil swresample; do
  ln -sfn /usr/lib/x86_64-linux-gnu/lib$l.so $SRC/local/lib/lib$l.a
done
```

> **坑**：清理对象文件时**不能用** `find -name '*.a' -delete` —— 会把刚建立的
> 库符号链接一并删掉。应只清理 `src/`、`libutils/`、`libfixwav/` 下的 `.o`。

---

## 3. `stack smashing detected`（轨数过多）

### 症状

```
*** stack smashing detected ***: terminated
[FAIL] dvda-author 生成第 1 盘失败
```

### 定位

用二分法测试不同轨数：

```
n=5   OK      n=30  OK      n=60  OK
n=10  OK      n=40  OK      n=70  OK
n=20  OK      n=50  OK      n=77  失败 rc=134  堆栈破坏=1
```

阈值落在 70 与 77 之间。

### 根因

`atsi2.c`：

```c
uint8_t atsi[2048 * 3];        // 固定 3 扇区 = 6144 字节
int ntitletracks[99];
```

ATSI 表缓冲固定 3 扇区，而每轨约需 52 字节 → **约 70 轨即写超出栈缓冲区**。
且下游只支持 `atsi_sectors` 取 2 或 3，无法自动扩容。

### 修复

**在流水线层面限制**（不改上游代码）：`02_build.py`

```python
GROUP_TRACK_LIMIT = 64      # 每组最多 64 轨，留出安全余量
```

超出时按**专辑边界**再拆一组（专辑仍不拆散）。实测 93 轨拆成 3 组后零堆栈破坏。

---

## 4. `--aob-extract` 段错误

### 症状

```bash
dvda-author-dev --aob-extract ATS_01_1.AOB -o out -W -P0 -n
# → Segmentation fault
```

但**提取出的文件是完整正确的**：

```
39831398  track_01_title_01.mlp     ← 与源 MLP 大小相同
efbf4eb0430047e1e6f5e46a0131acc3    ← MD5 完全一致
```

### 说明

这是上游在收尾阶段的已知行为，**不影响提取数据的正确性，也不影响光盘播放**。

校验脚本据此忽略其返回码，只比对提取结果：

```bash
"$NEW" --aob-extract "$AOB" -o "$EXT" -W -P0 -n >/dev/null 2>&1 || true
cmp -s "$SRC_MLP" "$EXTRACTED_MLP" && echo "一致 ✔"
```

---

## 5. ffmpeg 解码丢帧（Apple ALAC 未压缩帧缺 END 标记）

> 这一节是本文档最有价值的案例之一：**最初被误判为「源文件损坏」，
> 实际是 ffmpeg 的解码缺陷，且可精确修复。**

### 症状

- 流水线"全部成功"，但**听感异常**（3 分钟处有跳音/断续）
- 同一文件用 **foobar2000 / Apple 播放器播放完全正常**
- ffmpeg 报错但退出码为 0：

```bash
ffmpeg -v error -i x.m4a -f null -
# [alac] invalid element channel count
# [alac] Error submitting packet to decoder: Invalid data found
# echo $?  →  0        ← 仍然返回成功！
```

丢帧数（每次 4096 采样）：

| 文件 | 报错行数 | 丢失采样 | 缺失时长 |
|------|----------|----------|----------|
| 日文版 | 6 | 12,288 | 256 ms |
| 英文版 | 2 | 4,096 | 85 ms |
| 韩文版 | 14 | 31,208 | 650 ms |

### 关键判据：不是源损坏

| 检验 | 结果 | 结论 |
|------|------|------|
| 包位置是否连续覆盖 `mdat` | 2507/2507，100.00% | 容器索引正常 |
| 失败帧占总帧数 | 3/2507 = **0.12%** | 不是"格式不支持" |
| `-ignore_editlist` / `-advanced_editlist 0` / `-threads 1` | 报错数不变 | 与 edit list、线程无关 |
| 干净重封装后 | 报错数不变 | 不是容器层面的问题 |
| 用户实际播放 | **完全正常** | 源数据没问题 |

### 根因

Apple 的 ALAC 编码器会**周期性插入「未压缩帧」**（raw PCM，用于随机访问定位），
特征：

- `is_compressed = 0`（帧头第 3 字节的 `0x02` 位）
- `extra_bits = 0`
- 包大小 = `4 + n_samples × channels × sample_size/8`

实测出现在 `115.712s / 147.712s / 179.712s`，间隔**恰好 32.000 秒**（每 375 帧）。

这类帧的比特数恰好是：

$$\underbrace{23}_{\text{帧头}} + \underbrace{4096 \times 2 \times 24}_{196608} = 196631\ \text{位}$$

包共 $24580 \times 8 = 196640$ 位，**剩 9 位**，应按规范写入 END 元素（3 位 `111`）。
但 Apple 写的是 `000`。

于是 ffmpeg 把它读成一个 SCE 元素：

```c
element = get_bits(&alac->gb, 3);          // 读到 000 = TYPE_SCE
channels = (element == TYPE_CPE) ? 2 : 1;  // → 1
if (ch + channels > alac->channels ||
    ff_alac_channel_layout_offsets[...] + channels > alac->channels) {
    av_log(avctx, AV_LOG_ERROR, "invalid element channel count\n");
    return AVERROR_INVALIDDATA;
}
```

第一次循环不报错（$ch=0$，$0+1 \le 2$ 通过），**第二次循环 $ch$ 已为 1，
$1+1 > 2$ 才报错** —— 这正好解释了「报错条数与异常帧数一一对应」。

> 这也是为什么**播放器能正常播**：Apple 的 CoreAudio 不需要靠 END 标记
> 定位帧边界（它按包长切分），而 ffmpeg 依赖解码器的元素循环自然结束。

### 诊断

**① 帧头比特分析** —— 找出所有 `is_compressed=0` 的帧，检查其 END 标记：

```python
HDR_BITS = 3 + 4 + 12 + 1 + 2 + 1        # = 23
end_bit = HDR_BITS + n_samples * channels * sample_size
# 读 end_bit 起的 3 位，若 != 0b111 即为问题帧
```

实测输出：

```
eb_raw 分布: 0=3, 1=2504
  #1356  115.712s  size=24580  byte2=0x03 hex=200003cc598a6a34
  #1731  147.712s  size=24580  byte2=0x02 hex=2000021460ea7236
  #2106  179.712s  size=24580  byte2=0x03 hex=2000039c728a081a
相邻正常包:      byte2=0x04 hex=2000040404130809   ← is_compressed=0, extra_bits=8
```

注意相邻正常包的 `extra_bits=8` 而异常包 `extra_bits=0`，配合包头大小
即可确认是「未压缩帧」而非损坏数据。

**② 包大小验证**

```
24580 = 4(帧头) + 4096×2×3     ← 24-bit 未压缩帧的精确字节数
16388 = 4(帧头) + 4096×2×2     ← 16-bit 版本（韩文版）
```

字节数**分毫不差**，说明数据是完整的、有意义的。

### 修复

把 END 标记（`111`）写回正确位置。**不动任何样本数据**，只改帧尾填充位：

```
原始:  200003cc 598a6a34 ... [样本数据] ... 000
修复:  200003cc 598a6a34 ... [样本数据] ... 111
```

工具：`dvda_scripts/alac_endfix.py`

```bash
python3 alac_endfix.py --check x.m4a          # 只检测
python3 alac_endfix.py x.m4a x.fixed.m4a      # 修复到新文件
```

修复效果（**采样数分毫不差**）：

| 文件 | 修复前 | 修复后 | 容器声明 | 报错 |
|------|--------|--------|----------|------|
| 日文版 | 10,253,856 | 10,266,144 | 10,266,144 ✔ | 6 → 0 |
| 英文版 | 10,262,048 | 10,266,144 | 10,266,144 ✔ | 2 → 0 |
| 韩文版 | 9,364,628 | 9,393,300 | 9,393,300 ✔ | 14 → 0 |

### 两层无损性验证

**V1. 包内原始 PCM == ffmpeg 解码结果**

未压缩帧的样本数据就是裸 PCM，可直接从包里提取，与 ffmpeg 修复后的
解码输出逐字节比对 —— 证明补 END 后读出的正是包里原有的数据。

**V2. 挖掉补回的帧后 == 原始解码结果**

从修复后的解码 PCM 中，按已知位置挖掉补回的帧，应当与**原始解码结果
完全一致** —— 证明除了补回的帧，其它音频一个字节都没变。

### 接入流水线

`01_prepare.py` 现在会在解码校验失败时**自动尝试修复**：

1. 检测到解码报错 → 调 `alac_endfix.find_bad_frames()`
2. 有可修帧 → 修复到 `$DVDA_ALAC_FIX_DIR`（默认 `/root/dvda-build/alacfix`）
3. **原文件绝不修改**，manifest 指向修复后的副本
4. 修复后重新解码校验，采样数必须精确达标，否则仍判 FAIL

修复记录会写入 `decode_report.txt`，并记在 manifest 的 `repaired` /
`orig_src` 字段中，便于追溯。

### 教训

> **「播放正常但转码报错」是解码器问题的强信号。**
>
> 最初仅凭「ffmpeg 报错 + 采样数缺失」就判定源文件损坏，是错的。
> 真正推翻该结论的是用户的一句反馈：**原文件播放没问题**。
>
> 判定源文件是否损坏，必须交叉验证：
> 1. 容器索引是否完整（包位置是否连续覆盖 mdat）
> 2. 失败比例（0.12% 说明是局部特征，不是"不支持该格式"）
> 3. **同文件在其它播放器/解码器下是否正常**
> 4. 数据是否"有意义的完整"（如未压缩帧字节数与理论值分毫不差）

---

## 6. 末轨 AOB 少几字节

### 症状

审计报告扇区数比轨道表少 1：

```
组3: A. 扇区数 AOB=334525 轨道表=334526  不一致 ✗
```

检查文件大小：

```
685109244 字节  =  334525.998 扇区     ← 不是 2048 整数倍，余 2044
```

### 说明

`dvda-author` 写末轨最后一个 pack 时少写了 4 字节填充，
而 IFO 已按整扇区（334526）声明。

**验证音频完整性**（提取末轨并与源 MLP 比对）：

```
51819200  track_14_title_14.mlp
一致 ✔
```

**音频数据是完整的**，缺的仅是填充。

### 修复

`02_build.py` 在打包前把 AOB 补零至扇区边界：

```python
for aob in sorted(glob.glob(os.path.join(out, "AUDIO_TS", "*.AOB"))):
    size = os.path.getsize(aob)
    rem = size % 2048
    if rem:
        with open(aob, "ab") as fp:
            fp.write(b"\x00" * (2048 - rem))
```

---

## 7. 位深丢失（产生非法音频组）

### 症状

`dvda-author` 输出的轨道表里，某一轨位深与同组其他轨不同：

```
第 1 次（错误）：
    1     01   48000   24   2 L-R     11612440
    1     02   48000   24   2 L-R     11588000
    1     03   48000   24   2 L-R     11636800
    1     04   48000   16   2 L-R     11544000   ← 整组是 24-bit，这一轨却是 16-bit
    1     05   48000   24   2 L-R     11612440
```

对应日志：

```
Group   Title  Track  First_Sect   Last_Sect  First_PTS  PTS_length cga
    1   04/05     4       92380      111901          98    21645000   1
```

**注意工具链全程没有报错**，ISO 也照常生成、容量也正常。
这类问题最危险的地方就是：**结果看起来完全正常，实际已经错了。**

### 根因

- 源文件是 44.1kHz / **16-bit**
- 归一化决定「重采样到 48kHz / 24-bit」
- 但 `aresample` **只改采样率，不改位深**
- MLP 编码器**沿用输入的位深**，于是这一轨被编成 16-bit

### 诊断

让 `dvda-author` 报告每轨参数：

```bash
/root/dvda-author-mlp8/src/dvda-author-dev -g a.mlp b.mlp \
  -o out -D tmp -W -P0 -n 2>&1 | grep -E 'Found MLP audio|MTabLayout|Track'
```

或直接用 `ffprobe`（MLP 不记录时长，但采样率/位深可读）：

```bash
ffprobe -v error -select_streams a:0 \
  -show_entries stream=sample_rate,bits_per_raw_sample \
  -of default=nw=1:nk=1 x.mlp
# 48000
# 24
```

### 修复：显式指定 `-sample_fmt`

MLP 编码器只接受 planer 格式 `s16p` / `s32p`。
注意**不能用 `s32`**，会报：

```
[mlp @ ...] Specified sample format s32 is not supported by the mlp encoder
[mlp @ ...] Supported sample formats:
[mlp @ ...]   s16p
[mlp @ ...]   s32p
```

正确写法：

```python
cmd += ["-sample_fmt", "s16p" if bits == 16 else "s32p",
        "-c:a", "mlp", "-strict", "-2", mlp]
```

### 验证：指定位深前后必须不同

以 44.1k/16 的源为例（重采样到 48k/24），对比「指定位深」与「不指定」：

```bash
SRC=x.flac

# A：显式指定 s32p
ffmpeg -i "$SRC" -af aresample=48000:resampler=soxr \
       -sample_fmt s32p -c:a mlp -strict -2 A.mlp

# E：不指定位深
ffmpeg -i "$SRC" -af aresample=48000:resampler=soxr \
       -c:a mlp -strict -2 E.mlp

# 用 dvda-author 报告实际位深
for f in A E; do
  echo -n "$f: "
  dvda-author-dev -g $f.mlp -o o -D t -W -P0 -n 2>&1 \
    | grep -m1 'Found MLP audio'
  rm -rf o t
done
```

实测结果：

```
A: Found MLP audio: 2 channels, 24 bits per sample, 48000 Hz   ✔
E: Found MLP audio: 2 channels, 16 bits per sample, 48000 Hz   ← 错
A.mlp 61018364 B
E.mlp 39140014 B        ← 体积差一半，因为位深浅了一半
```

也可以用 `aformat` 达到同样效果（`aformat=sample_fmts=s32` 或 `s32p` 均可）：

```
A_sfmt_s32p        61018364 B
C_aformat_s32      61018364 B
D_aformat_s32p     61018364 B   三者输出逐字节一致
```

### 加固

`02_build.py` 编码后用 `ffprobe` 复核，不一致即删除文件并抛错：

```python
got = ffprobe_mlp(mlp)           # (sample_rate, bits_per_raw_sample)
exp = (track["sr"], track["bits"])
if got != exp:
    os.remove(mlp)
    raise RuntimeError(f"MLP 参数不符: 期望 {exp}, 实际 {got}")
```

---

## 8. MLP 不记录时长（校验失效风险）

### 症状

MLP 容器**不记录时长**：

```bash
ffprobe -v error -show_entries format=duration -of default=nw=1 x.mlp
# duration=N/A
```

因此无法靠「回读输出时长」来核验完整性。若不另想办法，损坏的源文件会
**静默通过**（每次丢 4096 采样）。

一个有价值的旁证：源文件损坏时，编码产物会被**提前截断** ——
例如某个损坏的 M4A，其 MLP 只有 33,454,262 B，而正常情况下应有 53,084,192 B。
截断本身也是信号，但它依赖「事先知道正常体积」，不够可靠。

### 修复：改用 astats 采样数

在同一次解码中读取采样数（`-f null -` 不落盘）：

```bash
ffmpeg -hide_banner -v info -i x.flac -af astats=metadata=1 -f null - 2>&1 \
  | grep 'Number of samples'
# [Parsed_astats_0 @ ...] Number of samples: 10267032
```

与「源声明时长 × 目标采样率」比对。实测精度极高（正常文件差 **+0** 采样）：

```
[PASS] 正常 FLAC 48k/24        期望 10267032 / 实解 10267032  差 +0      报错 0
[PASS] 正常 FLAC 44.1k/24→48k  期望 11636770 / 实解 11636770  差 +0      报错 0
[FAIL] 损坏 M4A (韩文版)       期望 10224000 / 实解 10192792  少 650ms  报错 14
[FAIL] 损坏 M4A (英文版)       期望 10266144 / 实解 10262048  少 85ms   报错 2
```

**并且必须处理「取不到采样数」的情况** —— 否则校验手段失效时又会静默通过：

```python
if samples is None:
    level = "FAIL"
    reasons.append("未能读到解码采样数(astats 无输出)")
```

### 副作用：下游脚本需要另一套数据来源

`verify.sh` 与 `verify_pts_length.py` 需要每轨的**源音频时长**，
而 MLP 里读不到。为此 `02_build.py` 输出 `mlp_index.json`：

```json
{
  "/root/dvda-build/mlp/group_48000_24__0001__xxx.mlp": {
    "src": "/mnt/c/.../01. xxx.flac",
    "dur": 241.925667,
    "sr": 48000,
    "bits": 24,
    "resample_to": null,
    "title": "xxx"
  }
}
```

校验时的比对基准是「源音源 + 同一条重采样滤镜链」：

```bash
ffmpeg -i "$src" -af "aresample=${rto}:resampler=soxr" -f s24le src.raw
ffmpeg -i "$mlp" -f s24le dec.raw
```

---

## 9. 声道数不一致（预防性检查）

DVD-Audio 同一音频组内所有曲目须同声道数。单声道与立体声**无法无损互转**，
所以本工具链不做声道转换，而是直接检查并在不一致时失败。

### 实测：一整批音源全部为立体声

```bash
ffprobe -v error -select_streams a:0 \
  -show_entries stream=channels,channel_layout \
  -of default=nw=1 src.flac
```

147 个文件的统计结果：

```
  2 声道: 147 个
全部为 2ch / stereo ✔
```

参数组合（同一批次里采样率与位深可以混用，声道数不行）：

| 数量 | 声道 | 布局 | 采样率 | 位深 |
|------|------|------|--------|------|
| 128 | 2 | stereo | 48000 | 24 |
| 13 | 2 | stereo | 44100 | 24 |
| 4 | 2 | stereo | 44100 | 16 |
| 1 | 2 | stereo | 48000 | 16 |
| 1 | 2 | stereo | 96000 | 24 |

`stereo` 布局即 FL（前左）+ FR（前右），对应 `dvda-author` 报告中的 `L-R`。
归一化后最终只有两个音频组：`48000/24`（131 首）与 `44100/24`（16 首）。

> 注意：`ffprobe -of default=nw=1` 会**去掉键名**，只输出值。
> 解析时要用 `default=nw=1:nk=1` 并按行取值，或保留键名。
> 否则会误判「标签不存在」。

---

## 10. 审计误报：取到了上一次构建的日志

### 症状

重新出盘后审计报「扇区数不一致」，且 PTS 下降点也不落在轨边界：

```
第 1 盘 组1    65 轨
  A. 扇区数 AOB=1588763 轨道表=1588632  不一致 ✗
  D. PTS 下降点 64 个；落在轨边界 异常 ✗
    非轨边界的下降点: [834886, 861611, 888172, ...]
    应下降但未下降的轨起点: [1442050, 1072775, ...]
```

**但 AOB / IFO / ISO 本身完全正确。**

### 根因

`audit_disc.py` 原先按固定顺序取「第一个存在」的日志：

```python
LOG_CANDIDATES = [
    pathlib.Path(os.path.join(BUILD_DIR, "rebuild-final.log")),
    pathlib.Path(os.path.join(BUILD_DIR, "finalrebuild.log")),
    pathlib.Path(os.path.join(BUILD_DIR, "build.log")),
]
LOG = next((p for p in LOG_CANDIDATES if p and p.exists()), None)
```

而 `build.sh` 每次运行写的是 `build.log`，**旧日志不会自动删除**。
于是：新构建的 AOB 与旧构建的轨道表比对 → 必然不一致。

### 诊断

列出各日志的时间与其中的关键数值：

```
日志                     时间              disc1组1 max last_sector
rebuild-final.log        09-18 18:57        1588631  -> +1 = 1588632   ← 被误用
finalrebuild.log         09-18 18:27        1588631  -> +1 = 1588632
build.log（本次构建）    09-19 08:36        1588762  -> +1 = 1588763   ✔
```

AOB 实际总扇区 = 1,588,763 —— 与**本次** build.log 的轨道表一致。

用正确的日志复核：

```
A. 扇区数 AOB=1588763 轨道表=1588763  一致 ✔
D. PTS 下降点 64 个；落在轨边界 全部命中 ✔
```

### 修复

改为在候选列表中取 **mtime 最新**者，并打印所用日志路径与时间：

```python
_existing = [p for p in _LOGS if p and p.exists()]
LOG = max(_existing, key=lambda p: p.stat().st_mtime) if _existing else None
if LOG is not None:
    print(f"构建日志: {LOG}  (mtime {time.strftime(...)})")
```

并在发现另有日志时间相近（< 60 秒）时给出提示：

```
⚠ 另有 1 个日志时间相近，请确认所用日志对应本次构建
```

`verify_pts_length.py` 与 `verify.sh` 同样修正，后者还支持
`DVDA_BUILD_LOG` 环境变量显式指定。

> **教训**：只要「比对 A 与 B」，就要确认 A 和 B 来自**同一次**构建。
> 用固定的文件名顺序去猜哪个是当前产物，在存在历史残留时必然出错。

---

## 11. 校验脚本的组匹配错误（ATS_01_1.AOB 是组1）

### 症状

`verify.sh lossless` 报告：

```
[2] 成品 ISO 内音轨校验失败  ✗
```

### 根因

第 2 项校验从 `mlp_index.json` 取「排序第一个」的 MLP，去与
`ATS_01_1.AOB` 里提取出的音轨比对。但两者**未必同组**：

- `ATS_01_1.AOB` 存的是**组1**（最先传入 `-g` 的那组）
- `mlp_index.json` 按路径排序，第一个可能是 `group_44100_24__0001__*`
  （属于组2），于是拿组2 的 MLP 去比组1 的 AOB → 必然失败

### 修复

从构建日志解析第 1 张盘的 `dvda-author` 命令行，取**组1 的第 1 轨**对应的 MLP：

```python
for line in t.splitlines():
    if not line.startswith("+ ") or " -g " not in line:
        continue
    if "/disc1 " not in line and "/disc1\t" not in line:
        continue
    seg = line.split(" -g ", 1)[1].split(" -o ", 1)[0]
    ...
```

> **注意**：MLP 文件名含空格，**不能用 `split()` 切分**。
> 须按 MLP 目录前缀逐个取到 `.mlp` 结尾（与 `verify_pts_length.py` 一致）：
>
> ```python
> for chunk in seg.split(pfx)[1:]:
>     e = chunk.find(".mlp")
>     if e >= 0:
>         names.append(pfx + chunk[:e + 4])
> ```

修正后：

```
[1] MLP 解码 PCM 与源音源逐字节一致  ✔
[2] 成品 ISO 内音轨与源 MLP 一致  ✔
```

---

## 12. 诊断手法速查

### 解析 AOB 的 PES 时间戳

```python
def parse_pts(b):
    """PTS/DTS 为 5 字节：4bit 标记 + 3×(1bit 标记 + 15bit 值)"""
    return ((((b[0] >> 1) & 0x07) << 30)
            | ((((b[1] << 8) | b[2]) >> 1) << 15)
            | (((b[3] << 8) | b[4]) >> 1))

sec = data[s * 2048:(s + 1) * 2048]
idx = sec.find(b"\x00\x00\x01\xBD", 4, 64)     # PES start code
if idx >= 0 and (sec[idx + 7] & 0x80):         # PTS present
    pts = parse_pts(sec[idx + 9:idx + 14])
```

### 日志中解析轨道表时注意 ANSI 转义

轨道表行带颜色码，正则匹配前必须剥离：

```python
text = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", text)
```

轨道行 9 个字段（**容易数错**）：

```
   组  标题号/总数  轨号  首扇区  末扇区  First_PTS  PTS_length  cga
    3      02/14       2   23635   46942      105     17935755     1
```

### 用 gdb 定位崩溃

```bash
gdb -batch -ex run -ex bt --args ./dvda-author-dev -g file.mlp -o out -D tmp -W -P0 -n
```

若符号被 strip，需重编时去掉链接期的 `-s`：

```bash
sed -i 's/ -s  dvda-author.o/ dvda-author.o/' src/Makefile
```

### 追踪 `strlen` 的空指针

```gdb
break strlen
commands
silent
printf "strlen arg=%p caller=%p\n", $rdi, *(void**)$rsp
continue
end
run
```

输出中最后一个 `arg=(nil)` 即崩溃点。

### 判断是源的问题还是工具的问题

关键思路：**比对「源解码」与「MLP 解码」的裸 PCM**。

```bash
# 源 → 裸 PCM（需施加与编码时相同的重采样）
ffmpeg -v quiet -i src.m4a -f s24le a.raw
# MLP → 裸 PCM
ffmpeg -v quiet -i pipe.mlp -f s24le c.raw

cmp a.raw c.raw    # 一致 → MLP 无损，编码链路忠实
```

若两者一致但都短于源声明时长，说明 `ffmpeg` 解码源时就丢了数据 ——
这时**不要急着判定源损坏**，先用另一条判据核对（见第 5 节）：
同一文件在其它播放器里是否正常。
