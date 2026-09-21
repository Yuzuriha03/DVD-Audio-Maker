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
12. [审计/校验误报：dry-run 冲掉了构建日志](#12-审计校验误报dry-run-冲掉了构建日志)
13. [标签键名大小写导致专辑被拆散、归一化静默跳过](#13-标签键名大小写导致专辑被拆散归一化静默跳过)
14. [ffmpeg 的 MLP 编码器不写 END_OF_STREAM](#14-ffmpeg-的-mlp-编码器不写-end_of_stream)
15. [每张盘少一首：pack 未补齐到扇区边界](#15-每张盘少一首pack-未补齐到扇区边界)
16. [选曲菜单（AMG / ASVS）的坑（34 个小节）](#16-选曲菜单amg--asvs的坑)
17. [诊断手法速查](#17-诊断手法速查)

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

### 修复（两阶段）

#### 第一阶段：在流水线层面限制（当时不改上游代码）

`02_build.py` 里设 `GROUP_TRACK_LIMIT = 64`（每组最多 64 轨，留安全余量），
超出时按**专辑边界**再拆一组（专辑仍不拆散）。实测 93 轨拆成 3 组后零堆栈破坏。

这解决了崩溃，但**代价是分组**：一张盘的曲目被拆成多个「音频组」。
对 SurCode 产出的这版盘，盘1 被拆成 66+25 两组，而**两组的音频参数完全相同**
（都是 48000/24）—— 也就是说分组**不是格式要求**，纯粹是这个上限的产物。

#### 第二阶段：把 ATSI 表改成按曲目数动态分配（`patch_atsi_dynamic.py`）

先测准每轨占用：从生产构建的 ATSI 里读出 `i` 字段（`atsi[0x804] + 0x801`）

| 组 | 轨数 | `i` | 每轨 |
|---|---|---|---|
| 盘1 组1 | 65 | 5696 | 56.1 |
| 盘1 组2 | 10 | 2616 | 56.7 |
| 盘1 组3 | 14 | 2840 | 56.5 |

基线 2049 字节（ATSI_MAT 等占 0x800）⇒ **每轨 ≈ 56.5 字节**，各轨数下高度一致。

于是把固定栈数组换成**按曲目数分配的堆缓冲**：

```c
  /* 2049 基线 + 96 字节/轨（实测 56.5 的 ~1.7 倍余量），向上取整到扇区再多留一扇区 */
  size_t atsi_cap = ((2049 + (size_t) ntracks * 96 + 2047) / 2048 + 1) * 2048;
  uint8_t *atsi = (uint8_t *) calloc(atsi_cap, 1);
  ...
  /* 扇区数按实际用量算，不再固定 2/3 两档 */
  *atsi_sectors = (uint8_t) ((i + 2047) / 2048);
  if (*atsi_sectors < 2) *atsi_sectors = 2;
  ...
  FREE(atsi)
```

99 轨（`MAX_TRACKS`）也只分配约 12 KB 的堆内存。实测扇区数随轨数正确增长：

| 轨数 | `i` | 扇区 |
|---|---|---|
| 1 | 2112 | 2 |
| 10 | 2616 | 2 |
| 40 | 4296 | 3 |
| 70 | 5976 | 3 |
| 91 | 7152 | 4 |

小组合仍是 2 扇区（不浪费盘），大组合按需增长。

于是 `DVDA_GROUP_TRACK_LIMIT` 可以放到 **99**（= `MAX_TRACKS` = 单组轨数上限），
本项目盘1 的 91 首从此在**一个组**里。这样还有两个附带好处：

1. **菜单页数只由曲目总数决定**，不再受组划分影响（`dim == 1`）；
2. 跨组那一类缺陷（分页不自洽、`tracktext` 条数不足、`cutloop` 静态变量泄漏）
   **全部消失** —— 这一轮撞到的段错误全在跨组路径上。

> **教训**：遇到「上游用固定大小缓冲截断了容量」时，先量准**每单位实际占用**
> （读它自己写的长度字段最可靠），再决定扩多少 —— 这比拍一个「放宽到 N」
> 的数字稳得多，也能顺手把扇区数从「固定分档」改成「按实际用量」，
> 小输入不浪费、大输入不溢出。

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

```
帧头 23 位 + 采样数据 4096 × 2 × 24 = 196608 位  →  共 196631 位
```

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

**工具链全程没有报错**，ISO 也照常生成、容量也正常。

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
  "__meta__":    { "discs": 2, "tracks": 147, "dry_run": false },
  "__discs__":   [ { "disc": 1, "volid": "… 1",
                     "groups": [ { "group": 1, "aob": "ATS_01_1.AOB",
                                   "sr": 48000, "bits": 24,
                                   "tracks": [ { "mlp": "…/group_48000_24__0001__01. xxx.mlp",
                                                 "src": "…/01. xxx.flac" } ] } ] } ],
  "…/group_48000_24__0001__01. xxx.mlp": {
    "src": "/mnt/c/.../01. xxx.flac",
    "dur": 241.925667,
    "sr": 48000,
    "bits": 24,
    "resample_to": null,
    "title": "xxx"
  }
}
```

其中逐曲条目以 **MLP 路径为键**；`__meta__` / `__discs__` 是元数据段，
遍历索引统计曲目数时需跳过 `__` 开头的键
（`verify_pts_length.py` 就是这么做的）。

`__discs__` 记录「第 N 组 → `AUDIO_TS/ATS_01_N.AOB`」的对应关系，
是 `verify.sh` 定位「第 1 盘 组1 第1轨」的依据，详见第 11 节。

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
>
> 另一个更隐蔽的变体：`--dry-run` 把真出盘的日志**截断覆盖**了。
> 见[第 12 节](#12-审计校验误报dry-run-冲掉了构建日志)。

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

### 后续加固：改用分盘计划定位，拿不到就报错

上面只靠构建日志定位，仍有两个缺口：

- 日志被 `--dry-run` 覆盖或轮替后，就再也解析不出 `-g` 命令行
- 老代码在这种情况下**回退到 `sorted(keys)[0]`** —— 而 `group_44100_*`
  按字典序排在 `group_48000_*` 之前，于是又会拿错组的 MLP，
  把一个**完全正确**的 ISO 判为失败（同一个坑换了个入口）

现在改成三级处理，**绝不再猜**：

1. 先查构建日志里的 `dvda-author` 命令行
2. 再查 `mlp_index.json` 的 `__discs__` 分盘计划
   （`02_build.py` 直接写出「第 N 组 → `ATS_01_N.AOB`」与每轨的源 MLP，
   不依赖日志是否还在）
3. 两条路都拿不到 → 打印 `[FAIL]` 与原因，返回 1

```
定位依据: mlp_index.json 分盘计划（第1盘 组1 第1轨）
```

---

## 12. 审计/校验误报：dry-run 冲掉了构建日志

### 症状

跑完一遍 `bash build.sh --dry-run`（一切正常），再 `bash verify.sh` 却报错：

```
=================== 光盘一致性审计 ===================
[跳过] 构建日志中未解析到轨道表

=================== MLP 无损验证 ===================
[2] 成品 ISO 内音轨校验失败  ✗

=================== 存在问题，见上文 ✗ ===================
```

但 ISO 本身是对的 —— 手工比对可证：

```bash
got=$(dvda-author --aob-extract a.AOB -o ext -W -P0 -n 2>/dev/null; \
      find ext -name 'track_01_title_01.mlp')
cmp "$got" "$BUILD_DIR/mlp/group_48000_24__0001__01. xxx.mlp"   # 逐字节一致
```

### 根因

`--dry-run` 不执行 dvda-author，所以它写出的日志里**根本没有轨道表**
（`First_Sect` / `Last_Sect` / `PTS_length`）。而日志名沿用了 `build.log`，
于是把上次真出盘积累的轨道表**截断覆盖**了。

两处写入点都会截断，**都得改**：

| 位置 | 行为 |
|------|------|
| `build.sh` | `python3 01_prepare.py 2>&1 \| tee "$LOG"` —— `tee` **不带 `-a` 就是截断** |
| `02_build.py` | `run()` 与 `main()` 里的 `open(CFG.build_log, "a")`（追加，但前面已被 tee 清空） |

审计需要轨道表才能比对 AOB 扇区号；无损校验在没有 `__discs__` 计划的
旧索引下退化成 `sorted()[0]`，于是两个校验同时误报。

### 修复

1. `build.sh`：`--dry-run` 时 `LOG="$DVDA_BUILD_DIR/build-dryrun.log"`
2. `02_build.py`：模块级 `BUILD_LOG`，`main()` 里按 `--dry-run` 重定向；
   日志头加 `[DRY-RUN]` 标记与「本文件不含轨道表」说明
3. `mlp_index.json` 增写 `__meta__`（含 `dry_run` 标记）与 `__discs__` 分盘计划
4. `audit_disc.py`：跳过时调 `dryrun_hint()`，点明「这是 dry-run 日志」
   与下一步该跑什么

验证（dry-run 前后指纹不变）：

```bash
md5sum "$DVDA_BUILD_DIR/build.log" > /tmp/before.md5
bash build.sh --dry-run
md5sum -c /tmp/before.md5        # build.log: OK
```

> 教训：**同一份日志被两种运行模式共享**，就会互相破坏。
> 让不同模式写不同文件，比在读取端猜“这份日志是否可用”可靠得多。

---

## 13. 标签键名大小写导致专辑被拆散、归一化静默跳过

### 症状

把一批 Apple Music 的 m4a 转成 FLAC 后重新出盘，日志里出现异常：

```
共需重采样 9 首          ← 之前是 10 首
group_44100_16: 1 首     ← 多出一个孤零零的组
```

原本应被归一化到 48000/24 的那首 `Lulala! Lululala! (韩文版)`（44100/16）
不再出现在重采样列表里，而是自成一组。它所在的专辑有 5 首，其余 4 首都是
48000/24，按「多数采样率」应当被归一化。

**盘上会出现一个只有 1 轨的额外音频组，且该曲的采样率/位深与同专辑其他曲不同。**

### 根因

`01_prepare.py` 的标签读取是按**原样大小写**精确匹配的：

```python
d["album"] = tags.get("album", "")
```

而不同来源的标签习惯不同：

| 来源 | 键名 |
|------|------|
| MP4 / m4a（Apple） | 小写 `album` / `title` / `date` |
| FLAC（按 Vorbis 惯例，多数打标工具） | 大写 `ALBUM` / `TITLE` / `DATE` |

于是同名专辑里：

```
01. 中文版.flac    album = 'Lulala! Lululala!(...) - EP'    ← 原有 FLAC，小写键
02. 日文版.flac    album = ''                                ← 转出来的，大写 ALBUM
03. 英文版.flac    album = ''
04. 韩文版.flac    album = ''
05. Instrumental  album = 'Lulala! Lululala!(...) - EP'      ← 原有 FLAC，小写键
```

专辑键变成 `''` 与真实名两种，分组被拆成 `{01, 05}` 和 `{02, 03, 04}`；
後者里 44100/16 那一首与其余两首不同组内无法归一化，就形成独立组。

> **规范依据**：Vorbis comment 的字段名**大小写不敏感**（
> [vorbis-spec-ref](https://xiph.org/vorbis/doc/v-comment.html)：
> "the field name ... is case-insensitive"）。
> MP4 与 FLAC 两边都合法，**是读的一方有 bug**。

### 诊断

把 5 个文件的标签直接打出来：

```bash
for f in *.flac; do echo "--- $f"; \
  metaflac --list --block-type=VORBIS_COMMENT "$f" | grep -i 'album'; done
```

会看到大写/小写两种键名。用 ffprobe 交叉验证更直接：

```bash
ffprobe -v error -show_format -of json "$f" | python3 -c \
  "import json,sys; print(json.load(sys.stdin)['format']['tags'])"
```

ffprobe 对 FLAC 会**原样保留键名大小写**，所以能看出差异。

### 修复

读取时统一转小写：

```python
tags[k.lower()] = v
...
d["date"] = tags.get("date") or tags.get("releasetime") or ""
d["title"] = tags.get("title", os.path.splitext(os.path.basename(path))[0])
d["album"] = tags.get("album", "")
```

修复后重跑步骤1：重量采样数从 9 恢复到 10，多余的组消失。

> **教训**：“新增一类音源”时不要把注意力全放在音频参数上。
> 元数据也是接口，且**大小写、归一化、编码**都可能不一致。
> 本次的音频流完全正确（无损、参数无误），出问题的只是字符串键名。

---

## 14. ffmpeg 的 MLP 编码器不写 END_OF_STREAM

### 症状

用 ffmpeg 编出的 MLP 拿去出盘，**ffmpeg 自己能正常解码**，但无法确认在
硬件 DVD-Audio 播放器上是否正常 —— 与参考实现 SurCode MLP 的产出逐字节
对比后，发现流末尾缺一个标记。

### 根因

MLP 规范要求流末尾写 `END_OF_STREAM`（`0xD234D234`）。ffmpeg 的编码器：

```c
if (ctx->last_frames == 0 && ctx->shorten_by) {
    put_bits32(&pb, END_OF_STREAM);
}
```

`shorten_by = frame_size - frame->nb_samples`。而 `mlp` 编码器**未声明**
`AV_CODEC_CAP_SMALL_LAST_FRAME`（`truehd` 有）：

```
$ ffmpeg -h encoder=mlp     → dr1 delay exp            ← 没有 small
$ ffmpeg -h encoder=truehd  → dr1 delay small exp      ← 有 small
```

于是 ffmpeg 通用层总把末帧补齐成完整 `frame_size`（40 样本），
`shorten_by` 恒为 0，写结束标记的分支**永不进入**。
这是必然结果，**靠命令行参数无法绕过**。

### 排查

用 access unit 解析器判定，**不要用子串搜索**：

```python
# ✗ 会误判：压缩数据里可能偶然出现 d234d234 这 4 个字节
EOS in data

# ✔ 只在「最后一个 AU 的第 1 个子流数据体末尾」找
```

实测 147 个文件中，有 1 个在压缩数据里偶然撞上该字节序列，
按子串搜索会被误判为「已有结束标记」。

### 修复

`mlp_align.py` 做纯字节修补（详见该文件头部注释）。要点：

1. 在最后一个 AU 的第 1 子流数据体**末尾**插入 4 字节 `d2 34 d2 34`
2. 重算 `substream header` 的 `end`（+2 words）
3. 重算 `access unit header` 的 `length`（+2 words）**及其 4 位奇偶校验**
4. 重算该子流的 `parity` 与 `checksum`

第 3 步容易漏 —— AU 头的 4 位奇偶是
`XOR(input_timing, length_words, 各子流头字节)` 的折叠结果，length 变了它就得跟着变。

### 验证手段

要改一个带校验和的二进制流，前提是能**独立验证改动**。
`mlp_align.py` 复刻了 ffmpeg 的 `av_crc`：

```python
crc_2D = 建表(poly=0x002D, bits=16, le=0)   # 建表后每个值 bswap32
crc_63 = 建表(poly=0x0063, bits=8,  le=0)

checksum16 = av_crc(crc_2D, 0, buf[:n-2]) ^ AV_RL16(buf[n-2:])
checksum8  = av_crc(crc_63, 0x3c, buf[:n-1]) ^ buf[n-1]
```

判据：**能否重算出文件里已有的校验值**。在 ffmpeg 与 SurCode 两种真实产出上
都验证通过（147 个文件、21 万+ 个 access unit 的 parity/checksum 全部对上）。

修复后应满足：

```
[1] 对齐自检通过（校验和/奇偶/子流/结束标记）
[2] ffmpeg 解码时出现 "End of stream indicated."
[3] 解码 PCM 与修复前逐字节相同（音频未被改动）
[4] 与 SurCode 的 major sync 逐字节相同
```

### 附带发现

同曲目、同参数下，两套编码器的 major sync（28 字节）只差 3 处：

| 偏移 | 字段 | SurCode | ffmpeg |
|------|------|---------|--------|
| `[14:16]` | `peak_bitrate` | 3200 | 3199 |
| `[16]` | `extended_substream_info` | 1 | 0 |
| `[26:28]` | `checksum16` | — | — |

`peak_bitrate` 的差来自 ffmpeg 用向下取整 `((peak<<4)-8)/rate`，
而解码公式是 `(raw*rate+8)>>4` —— 48000 Hz 下写 3199 会反算成 9597000。
改向上取整即往返精确。

> **教训**：「软件解码器能播」与「符合规范」是两件事。
> 本项目的软件端几乎都用 libavcodec 的 `mlp` 解码器，与编码器同源，
> 自洽性容易满足；要判断规范性必须找一个**独立实现**做参照
> （这里是 SurCode），并逐字段比对。

---

## 15. 每张盘少一首：pack 未补齐到扇区边界

### 症状

刻出的盘在 foo_input_dvda（foobar2000 的 DVD-Audio 插件）里**每张盘少一首**：

| 盘 | IFO 声明 | foobar 实际显示 |
|---|---|---|
| 外部 MLP 版 盘1 | 91 | 90 |
| ffmpeg 版 盘1 | 89 | 88 |
| 两张盘2 | 正常 | 正常 |

**而所有常规检查都通过**：

- IFO 里 `nr_of_titles` 与各 title 的 `tracks` 合计正确
- IFO 里所有 title 的 `len_in_pts` 求和 == 音源总时长（538.1 分，精确相同）
- 逐轨 sector 表正确：首轨从 0 起、首尾相接、无重叠无空洞、末轨末扇区 == AOB 总扇区 − 1
- AOB 里逐扇区解析 PES 头，PTS 下降点个数正确（65 + 24 + 55）
- 逐曲时长与源文件比对 147/147 全吻合

也就是说：**盘上什么都不缺，但读盘端看不见其中一首。**

### 诊断路径

关键是先确认「少的是同一首，还是两版各少不同的曲目」，答案决定了方向：

```bash
# 1. 构建日志里有没有补齐失败的记录（最快的一步）
grep -c 'pes_padding length must be higher' <BUILD_DIR>/build.log

# 2. 找「连续多个扇区都不以 pack 头开头」的区段
#    正常情况每个扇区开头都是 00 00 01 BA
```

两版少的是**不同的**曲目（ffmpeg 版第 48 轨、外部版第 16 轨），所以不是某一首
音频内容的问题，而是与轨道边界计算有关。

### 根因

`ats.c` 的 `write_pes_padding()` 在 `length` 为 **1~6** 时只打印一句错误就 `return`，
**一个字节都不写**：

```c
if (length > 6)
  {
    length -= 6; // We have 6 bytes of PES header.
    ...
  }
else
  {
    foutput("%s
", ERR "pes_padding length must be higher than 6;");
    return;      // ← 什么都不写
  }
```

调用方（MLP 的最后一包）传的是「补到 2048 边界还需的字节数」，该值落在 1~6 时：

1. 该轨最后一个 pack 短 1~6 字节 → 文件不再按 2048 对齐
2. **下一轨的 pack 头因此落进扇区中间**（实测偏移 2042 / 2043）
3. `foo_input_dvda` 的 `get_ps1()` 要求扇区以 `00 00 01 BA` 开头才能取到 stream id；
   取不到 → `get_audio_stream_info()` 返回 false → **该轨被整首丢弃**
4. 该轨末尾填充成功、把差额补回 → **错位只延续一轨就自愈**

于是表现就是「每张盘恰好少一首」，而且少的总是**紧跟在短包头之后的那一首**。

实证（ffmpeg 版）：

```
扇区 1175803 末尾 32 字节: ... d2 34 d2 34 e9 3d 00 00 01 ba 44 00
                                            ↑ EOS  ↑ 2 字节 ↑ pack 头（在扇区外了）
扇区 1175804 开头 32 字节: 04 00 04 01 01 89 c3 f8 00 00 01 bb ...
```

`d2 34 d2 34` 是我们补写的 `END_OF_STREAM`，其后本该是「2 字节 + pack 头」收在
2048 边界内，但 pack 头被挤到了下一个扇区的偏移 2042。

> 顺带一提：原先的判据 `length > 6` 还**把 `length == 6` 也判为错误**。
> 而 6 字节恰好就是一个空 PES 填充包（3 字节起始码 + 1 字节 stream id +
> 2 字节长度），本可以正常写出。这个 off-by-one 让触发窗口比必要的大了一倍。

### 修复

`patches/patch_ats_pack.py`（已挂在 `build_dvda_author_mlp.sh` 的第 [5/7] 步）：

- `length < 6` → 补零到边界（与 `length == 0` 分支同样的处理）
- 放开 `length == 6` → 正常写 PES 填充包
- `ff_buf` 改为 `length + 1`，避免 `length == 0` 时的零长数组

补丁精确匹配 `write_pes_padding`（其错误消息**不带**前导空格、且有 `maxverbose`
那一行），不会误改 `read_pes_padding`（它带两个前导空格）。

改完需**重建 dvda-author**：

```bash
bash build_dvda_author_mlp.sh
```

### 验证

```bash
# 1. 构建日志里该错误应恒为 0 次
grep -c 'pes_padding length must be higher' <BUILD_DIR>/build.log

# 2. 直接读原先坏掉的那几个扇区（不必解整盘）
xorriso -indev <ISO> -find /AUDIO_TS -name ATS_01_3.AOB -exec report_lba
#   输出形如：File data lba:  0 , <LBA> , <Blocks> , <Size> , '<path>'
#   注意是逗号分隔，别用 \s+ 正则去切
dd if=<ISO> bs=2048 skip=$((LBA + rel - 1)) count=3 status=none | xxd | head
#   前一扇区末尾应是「... 00 00 01 ba」，本扇区开头应是「00 00 01 ba」

# 3. 快速结构校验（会逐轨检查首扇区，约 3 秒）
bash verify.sh quick
```

实测结果：两版构建日志里该错误均为 0 次；原先坏掉的两个边界
（ffmpeg 盘1 扇区 1175804、外部版 盘1 扇区 355313）现在都以 pack 头开头。

### 教训

> **音频数据全对，不代表容器结构对。**
>
> 这个缺陷下「内容层」的每一项检查都是绿的 —— 时长对、轨数对、逐轨扇区表对、
> 解码后的 PCM 也对。唯一的异常是**字节对齐**：某个 pack 跨过了扇区边界。
>
> 所以审计里补上了检查 F（每轨首扇区必须以 pack 头开头）。它是唯一能拦住这类
> 问题的检查，而它此前一直是缺的 —— 缺陷因此躲过了很多轮完整校验。
>
> 另外：**必须用一个独立的读盘实现来验证**。这里用的是用户机器上 foobar2000 的
> `foo_input_dvda` 插件 —— 它的源码（以及它的行为）与我们的写入端完全无关，
> 所以「它显示 90 首」这条外部信息才是发现问题的起点。
>
> （它的曲目表按路径缓存，重建 ISO 后需在 foobar 里移除该专辑再重新添加，
> 否则可能显示的是上一次构建的结果 —— 这一点也要先排除。）

## 16. 选曲菜单（AMG / ASVS）的坑

给盘加「选曲菜单 + 播放封面」时踩到的一串问题。上有的 dvda-author 菜单功能
**基本只在 1 组 + 单页 + 几十轨的规模下被测试过**，本项目是 3 组、91 曲、
27 张封面，几乎每个规模假设都会被打破。

修复全部落在 `scripts/patches/patch_menu_*.py` 与 `scripts/menu_assets.py`。

### 16.1 分页公式写错：所有页加起来恒 ≤ 32 个按钮

`menu.c` 两处（`menu_characteristics_coherence_test` 与 `generate_menu_pics`）：

```c
img->maxbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) / img->nmenus;
img->resbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) % img->nmenus;
```

`maxbuttons` 应当是「**每页容量**」，但原式先截到 32 再除以页数 ——
于是无论 `--nmenus` 设多大，**所有页合计最多只有 32 个按钮**。
91 首的盘只有前 32 首能点，其余点不到，**且不报错**。

对照：`xml.c` 的分层分支写的是 `Min(..., ntracks[groupcount])`（组的轨数），
可见原意确实是「每页容量」，非分层分支写错了。

**判断依据**：统计 `spu_xmltemp_*.xml` 里的 `<button>` 总数，
应当 ≥ 曲目总数（多出的是翻页箭头）。

**修复**：`patch_menu_paging.py` 改为
`maxbuttons = Min(32, ceil(totntracks / nmenus))`，`resbuttons = 0`。

### 16.2 页数护栏把非分层菜单也压小了

同一函数里还有一段（原本只对分层菜单成立）：

```c
if ((img->ncolumns) * ngroups < img->nmenus - 1) img->nmenus = ngroups * ncolumns + 1;
```

非分层菜单的页数只受按钮总数约束，套用这个式子会把用户给的 `--nmenus`
静默压掉（实测 3 组时 8 → 4）。**加 `img->hierarchical &&` 限定**。

### 16.3 每页背景图全是同一张（`--background` 形同虚设）

`-b/--background` 接受逗号分隔的**每页一张**背景 jpg，解析没错，但选项收尾
的复制循环固定用 `backgroundpic[0]`：

```c
copy_file2dir_rename(img->backgroundpic[0], tempdir, "bgpic0.jpg", ...);
for (u = 1; u < img->nmenus; u++)
    copy_file2dir_rename(img->backgroundpic[0], tempdir, "bgpic<u>.jpg", ...);  // 又是 [0]
```

**验证手法**：跑完后逐页比对 `tmp/discN/bgpic<u>.jpg` 与源图的 md5。

### 16.4 `--blankscreen` 反过来覆盖 `--background`

选项收尾处：只要给了 `--blankscreen`，就把这个 png 转成 jpg **写进
`backgroundpic[0]`**（写之前先 `unlink`）。于是
`--background /tmp/bg0.jpg` 之后，`/tmp/bg0.jpg` 被就地改写成
「blankscreen 转出的 jpg」（实测与随包的 `menu/black_PAL_720x576.jpg` 逐字节相同），
用户给的封面图被删掉。

两者语义并不冲突：`--blankscreen` 是**文字浮层底图**（`prepare_overlay_img()`
把它拷成 `svpic.png` 再往上写字），`--background` 是**每页的背景视频图**。
用 `cli_background_list` 标记记住「用户给了列表」，给了就不覆盖。

### 16.5 `--blankscreen` 必须是全透明，且 `mogrify` 画不上 ASCII

- `--blankscreen` png 是文字浮层底图：**全透明**才能让背景视频透出来
  （`xc:none`，不要加 `-alpha set -depth 8 PNG32:` —— 那会变成叠加态）。
- ⚠️ **这台机器上没有任何字体能同时画中英文**：

| 字体 | ASCII | 汉字 | 假名 | 韩文 | CJK标点 |
|---|---|---|---|---|---|
| `fonts-droid-fallback`（系统自带） | **✗** | ✓ | ✓ | **✗** | ✓ |
| `fonts-wqy-microhei` | ✓ | ✓ | ✓ | **✗** | ✓ |
| `fonts-noto-cjk` | ✓ | ✓ | ✓ | ✓ | ✓ |

  Ubuntu 的 `fonts-droid-fallback` 是 **CJK-only 精简版**：
  `fc-list ':charset=0041'` 都查不到它 —— 连 `A` 都没有。
  而本项目曲名同时含**中文 + 日文假名 + 韩文**，缺任何一项那首曲子就是空白。

  **判据必须用功能性检测**，不能查字体表：

  ```bash
  # 真的渲一小块看有没有墨迹（>0 才算能画）
  convert -size 160x48 xc:none -font "$F" -pointsize 20 -fill white \
          -annotate +2+32 "Ag" -format "%[fx:mean.a]" info:
  ```

  查 `fc-query` 的字符集会被骗过 —— Droid 的字体信息里写着覆盖 Basic Latin，
  实际一个像素都不出，且 ImageMagick **不报错**。

- ⚠️ **`-font` 名字里的空格会切断命令**：`menu.c` 拼的是
  `snprintf(..., "-font", img->textfont, ...)`，**没有引号**。
  传 `"Droid Sans Fallback"` 会被 shell 切成两个参数 →
  `mogrify: no decode delegate for this image format 'Sans'` → 完全不画文字。
  必须用 ImageMagick 的字体名（空格换成连字符）：`Droid-Sans-Fallback`。
- ⚠️ **未知字体名会被静默替换**：ImageMagick 找不到名字时不报错、改用默认字体，
  于是「拉丁可画」的检测结果来自别的字体，而汉字仍然画不出。
  所以要先 `convert -list font` 确认名字存在，再做功能性检测
  （`menu_assets.font_exists()` + `font_coverage()`）。

### 16.6 字号过大 → 菜单直接缺失，而 dvda-author 返回 0

`compute_pointsize(img, 10, img->maxbuttons, globals)` 把字号算到上限 35，
而每页 11 行时行距只有约 34 px（`y()` 的 `labelheight + 12`）→
文字与下划线矩形重叠 → `spumux` 报

```
spumux: src/subgen-image.c:901: imgfix: Assertion `useimg' failed.
```

**不产出 `AUDIO_TS.VOB`**，但 dvda-author 仍然 exit=0。

实测各字号能否出 VOB（40 轨 4 页）：

| `--fontsize` | 菜单 VOB |
|---|---|
| 自动（算出 35） | **无** |
| 12 / 18 / 22 / 25 | 有 |

**正确算法**（`menu_assets.compute_fontsize`）：

```
行数 R = Min(32, ceil(总轨数 / 页数))
labelheight = (576 - 56 - 40 - (R+4)*12) / (R+4)      # 整数除法
行距 S = labelheight + 12
字号 P = min(S - 10, 35)，且 >= 7                       # 下划线在基线下 4~6px
```

**必须在构建后显式核对 `AUDIO_TS.VOB` 是否真的产出** —— 这是唯一能发现的判据。

### 16.7 `> 34 轨的组` → `*** stack smashing detected ***`

`xml.c` 的 `compute_coordinates()`：

```c
uint16_t y0[MAX_BUTTON_NUMBER], y1[MAX_BUTTON_NUMBER];   // 36
for (j = 1; j < command->maxntracks + 2; ++j) { y1[j] = ...; y0[j] = ...; }
```

而 `command->maxntracks` 是 **`MAX(track, command->maxntracks)`** ——
**整组轨数**，不是每页行数（`amg2.c:165`）。组内 ≥ 35 轨就越界写栈。
盘1 的组1 有 65 轨，必然中招；没有菜单时不走这段代码，所以一直没暴露。

**修复**：`patch_menu_layout.py` 把 `menu.c` / `xml.c` 里**全部**
`command->maxntracks` 换成 `img->maxbuttons`（= 每页行数）。
· `menu.c` 里写 `img->maxbuttons`（这些函数都有 `img` 参数）
· `xml.c` 的 `compute_coordinates()` 只有 `command`，要写 `command->img->maxbuttons`
  （`amg2.c:82` 有 `#define img command->img`，两处是同一对象）

### 16.8 `--screentext` 一用就崩，而且曲名被截成 3 字节

三处缺陷叠加，使 `-O/--screentext` **任何输入都段错误**：

1. `basemotif = fn_strtok(chain, '=', ..., count=1, cutloop, remainder)`
   —— `count` 是 `cutloop` 的「消费几个子串」计数器，传 1 时**第 1 个子串
   就 return 0 而 break**；而 `fn_strtok` 在 break 时 `array[k]` 被置哨兵 NULL、
   `remainder` = 第 k 个子串。于是 `albumtext = basemotif[0]` 变成 **NULL**。
   传 **2** 才对。
2. `dim` 是 `fn_strtok` 输出的**槽位数**（元素数 + 1 个哨兵），
   但遍历写成 `for (k = 0; k < dim; k++)` → 最后一次读到
   `grouparray[dim-1] == NULL` → `strlen(NULL)` 段错误。
   上界要用 `arraylength()`。
3. `size` 被复用：既作为列宽传入 `&size` 当输出参数，之后又当**字符串截断长度**：

   ```c
   size = (norm_x - 40 - 20*(ncolumns-1)) / ncolumns;      // 例如 213
   basemotif = fn_strtok(..., &size, ...);                  // ← 被覆写成 3
   ...
   if (strlen(tracktext[g][t]) > size) tracktext[g][t][size] = '\0';   // 砍到 3 字节
   ```

   于是**每个曲名被截成 3 字节**（中文只剩 1 个字）。要用独立变量 `ncut`。

另外 `remainder` / `rem` 是未初始化 VLA，而 `fn_strtok` 只在提前 break 时
才写它们（子串数刚好用尽时**不写**）→ 读到未初始化栈内存。

**修复**：`patch_menu_screentext.py`。

### 16.9 纯菜单背景（无 `--stillpics`）会导致 `background_movie_N.mpg` 缺失

`--blankscreen` 全透明且没给 `--background` 时，背景帧全黑，
`mpeg2enc` 编不出有效 MPEG-2 → 后面报

```
[ERR] freopen (stdin)
img->backgroundmpg[0]=.../background_movie_0.mpg errno=2: No such file or directory
```

**规避**：给每页一张不透明的背景图（本项目用封面拼图，压暗 70%）。

### 16.10 `--stillpics` 文件列表模式必然 cd 失败（退出码 255）

`: ` 分轨、`,` 分该轨的多张图。但**文件列表分支不设 `stillpicdir`**，
而 `create_stillpic_directory()` 一进来就 `change_directory(stillpicdir)`；
`stillpicdir` 的默认值在 `main()` 里**早于命令行解析**就已填好
（那时 `-D/--tempdir` 还没生效），所以它是**编译期默认**的
`<cwd>/.dvda-author/temp` —— 通常不存在：

```
[ERR]  Impossible to cd to /tmp/menu-test/.dvda-author/temp.
[ERR]: No such file or directory          → 退出码 255
```

**修复**：`patch_menu_stillpics_list.py` 把 `stillpicdir` 指到当前 tempdir
（同时也让「复制图片的目的地」与「读取图片的路径」一致）。

### 16.11 `pict` 被置 NULL 后再次 sprintf → 段错误

`generate_background_mpg()` 末尾有 `FREE(pict)`（= `free(pict); pict = NULL;`），
而 `create_mpg()` 里 `pict` 是**文件级** static、分配判据 `s` 却是**函数内** static
（不复位）：

```c
if (s == 0) { s = MAX(...); pict = calloc(s, sizeof(char *)); }
sprintf(pict, "%s/pic_%03u.jpg", ...);      // ← 第二次进入时 pict == NULL
```

「先做菜单背景、再做静图背景」这条路上必然触发。

**修复**：`patch_menu_stillpics.py`，判据加 `|| (pict == NULL)`。

### 16.12 分页必须逐字复刻 dvda-author 的分配（否则背景与按钮错位）

**这是我自己引入又修掉的一个坑**，值得单独记。

dvda-author **自己**决定哪几首落在哪一页：
`R = Min(32, ceil(总轨数 / 页数))`，逐页填满 R 行、组内连续、一页不跨组
（`ncolumns=1` 时 `while ((group < img->ncolumns) && ...)` 保证不跨组）。
我们的任务只是把「每页的背景图与文字」对上**它**的分配。

我最初为了「页内专辑完整」在专辑边界**提前断页**，于是：

1. 实际页数从 9 变成 10；
2. 我把 10 传给 `--nmenus=10`；
3. dvda-author 的 `R` 跟着变成 `ceil(91/10) = 10`（原来按 9 页算是 11）；
4. 我按「11 行 + 专辑边界」排版，它按「10 行、不听专辑边界」排版
   → **两套分页错开，背景与标题跟按钮对不上**。

**判据**：解出 (页数 N, R) 后，用同一个公式反推
`sum_g ceil(n_g / R)`，必须**恰好等于** N：

```
surcode 盘1  [66,25]     → 9 页 x 11   (6+3 = 9)  ✔
surcode 盘2  [56]        → 5 页 x 12   (5   = 5)  ✔
ffmpeg  盘1  [65,10,14]  → 14 页 x 7   (10+2+2 = 14) ✔
ffmpeg  盘2  [56,2]      → 8 页 x 8    (7+1 = 8)  ✔
```

**修复**：`menu_assets.group_pages()` 只在 `p == pages` 时接受，
`build_menu()` 去掉专辑边界断页，`02_build.py` 每次构建都跑这组自检。

> **教训**：只要上游工具**自己也做一遍**我们想模拟的决策，
> 就必须把它的算法**逐字复刻**并加自检 ——
> 差一行除数，出来的是「看起来正常但内容错位」的盘。

### 16.13 开菜单后 ISO 根目录多一个空 `VIDEO_TS`

菜单最后要跑一次 `dvdauthor -o <outdir> -x <xml>` 写虚拟机命令，
而 `dvdauthor` 是 **DVD-Video 工具**，会按惯例在 `-o` 目录下建一个空
`VIDEO_TS`。它与 dvda-author 的 `-n/--no-videozone` **无关**（那个只管
dvda-author 自己要不要建）。

实测：同一套参数加 `--topmenu` 后，ISO 根目录从「只有 `AUDIO_TS`」
变成「`AUDIO_TS` + `VIDEO_TS`」。

**本项目保留它**（用户明确要求）。顺带一个反直觉的观察：
删掉 `VIDEO_TS` 后 dvda-author 反而会多打一句
`[ERR] Directory '<...>/VIDEO_TS'` —— 它记着这个目录的扇区账，
留着反而日志更干净。

### 16.14 `--aob-extract` 报 `Aborted` 是已知行为

见第 4 节，与菜单无关；菜单盘上同样会出现。

### 16.15 静图容量：1024 扇区/盘，且「同图复用」才省得下来

`asvs.c` 里

```c
totpicsectors += img->stillpicvobsize[index + j];
if (totpicsectors > 1024) foutput(ERR "Exceeding stillpic buffer limit (2 MB) ...");
```

`totpicsectors` 在**循环外**初始化 → 是**整盘累计**，上限 1024 扇区 ≈ 2 MB。

而 `generate_background_mpg()` 对 STILLPICS 是**每轨各编一次再 `cat_file`
拼接**，所以每张图都单独占额度，**同一张图重复给也照样占**。
必须用**空项复用**才省得下来：

```
--stillpics A.jpg::C.jpg     ← 第 2 轨沿用 A，只占 2 份
```

实测（3 轨）：`A,B,C` → 75 扇区；`A::C`（1 张复用）→ 52 扇区。

预算换算：约 **15 扇区/张** → 上限约 **46 张/盘**。
· 本项目盘1 有 27~28 个专辑 ✔（每轨一张则 89 张 ✗ 超限）

**注意**：`--stillpics` 的项数必须**恰好等于总轨数**，否则报
`You forgot at least one track on --stillpics` 并退出 255。

### 16.16 `[WAR] Coherence test for ISO start sector failed` 是上游既有现象

```
[WAR]  Coherence test for ISO start sector failed: start sector assessed as: 287 but should be: 289
```

它只是 `startsector == ntotalfiles + 272` 的账目核对（`launch_manager.c:518`）。
实测**无菜单的生产构建同样是 287 vs 289**（差 2 扇区），与菜单无关；
校验脚本也不扫 `[ERR]`/`[WAR]`，不影响判定。

（若删掉空 `VIDEO_TS`，`ntotalfiles` 少 1 → 变成 `287 vs 288`，仍差 1。）

### 16.17 页数一多就在「很远的地方」段错误（AMG 缓冲不随页数增长）

**症状**：MLP 审计、写 AOB、菜单背景编码、`mogrify`、`spumux`、`dvdauthor`
**全部正常完成**，日志最后一句是 `mplex` 的 `MUX STATUS: no under-runs detected.`，
然后 dvda-author 段错误（`exit=139`），**没有任何 `[ERR]`**。

`dmesg` / gdb 显示崩在 `amg2.c` 的 `create_amg()` 里 `uint32_copy()`：

```
uint32_copy (buf=0x8000000016d4 <Cannot access memory>, x=1081472) at c_utils.h:423
#1  create_amg (...) at amg2.c:1357
#2  launch_manager (...) at launch_manager.c:472
```

**根因**是 VLA 越界写栈：

```c
// launch_manager.c —— SIZE_AMG 是常量 3，所以恒为 4 扇区
sectors.amg = SIZE_AMG + (globals->text ? 8 : 0) + (globals->topmenu <= TS_VOB_TYPE);

// amg2.c —— 缓冲区大小由它决定
uint8_t amg[sectors->amg * 2048];        // 4 * 2048 = 8192 字节
```

而菜单表（`menusector` 分支，从 `amg[0x1820]` 起）**随页数增长**：

```
需要 = 0x1820 + 8 * (nmenus - 1) + nmenus * 0x13A
```

| 页数 | 需要 | 缓冲 | 结果 |
|---|---|---|---|
| 1 | ~6490 | 8192 | ✔ |
| 4 | ~7456 | 8192 | ✔ ← 40 轨测试盘就是这个页数，所以**当时没暴露** |
| **9** | **~9066** | 8192 | **✗ 越界约 900 字节** |

越界写的是**栈**（VLA），所以崩点离真正原因很远 —— 前面所有阶段都跑完了
才崩，而且栈已被破坏（gdb 里 `create_amg` 的入参都是垃圾值）。

**修复**：`patch_menu_amg_size.py` —— 在 `sectors.amg` 初始化后按
`img->nmenus` 撑够：

```c
  if (globals->topmenu <= TS_VOB_TYPE && img->nmenus > 1)
    {
      uint32_t need = 0x1820 + 8 * (img->nmenus - 1) + img->nmenus * 0x13A;
      uint32_t need_sectors = (need + 2047) / 2048;
      if (need_sectors > sectors.amg) sectors.amg = need_sectors;
    }
```

`nmenus` 在命令行解析阶段（`menu_characteristics_coherence_test`）已定型，
所以这里读到的是最终值。`sectors.amg` 同时决定 `sizeofamg = sizeof(amg)`
与写盘长度、以及各处扇区指针（`2*sectors->amg + ...`、`sectors->amg - 1`、
`menusector * sectors->amg`），会自动跟着调整 —— 撑大后
`AUDIO_TS.IFO` 从 4 扇区变成 5 扇区，**自洽**。

**判据**：`AUDIO_TS.IFO` 的字节数 = `sectors.amg * 2048`；
开 9 页菜单时应为 10240（5 扇区），若仍是 8192 就说明补丁没生效。

> **教训**：VLA 越界写栈的崩点可以与原因相距极远（跑完十几分钟的全部流程
> 才崩），而且**会破坏崩溃现场的参数**，让 gdb 显示一堆垃圾值。
> 遇到「所有阶段都成功、最后莫名段错误」时，优先怀疑
> **由外部变量决定大小的自动数组（VLA）**。

### 16.18 `fn_strtok("")` 越界写栈，把调用方的 `globals` 踩坏

**这是最隐蔽的一个**：崩点（`create_stillpic_directory` 里的 `change_directory`）
与原因（`fn_strtok` 的一个零长度 VLA）隔了好几层，而且**完全取决于栈布局** ——
同一个二进制 gdb 下不崩、直接跑崩；同一套参数换一张盘就只是「偶尔」崩。

**症状**：dvda-author 把所有产物（AOB / `AUDIO_TS.VOB` / `AUDIO_SV.VOB` /
`AUDIO_TS.IFO`）**全部生成完之后**才段错误；日志里只有启动横幅一行
（stdout 全缓冲，崩溃前的输出全丢了），没有任何 `[ERR]`。

**定位手法**（这组技巧值得复用）：

```bash
# 1. 打开 core dump（core_pattern=core，但 ulimit -c 默认是 0）
python3 -c "import resource; resource.setrlimit(resource.RLIMIT_CORE, (-1,-1))
import subprocess; subprocess.run([...])"

# 2. 分析 core —— 完全不改变内存布局，所以必现（gdb 下反而可能不崩）
gdb -batch -ex bt ./dvda-author-dev ./core.12345
```

core 里的回溯直指：

```
#0 create_stillpic_directory (string="", count=-1, globals=0x7ffd00000006)  ← globals 是垃圾
gdb> info line
Line 1363 of src/menu.c:  change_directory(globals->settings.stillpicdir, globals);
```

`globals` 的真值是 `0x7ffdd1a7ebf0`，却变成了 `0x7ffd00000006` ——
典型的高位保留、低位被改写的**栈被踩**特征。

**根因**在 `auxiliary.c` 的 `fn_strtok()`：

```c
  char *s = strdup(chain);          // chain == "" → 只分配 1 字节
  uint32_t j = 1, k = 0;
  int32_t cut[strlen(s) / 2];       // strlen("")/2 == 0 → 长度为 0 的 VLA
  cut[0] = -1;                      // ← 越界写栈
  do  if (s[j] == delim) { cut[++k] = j; }
  while (s[j++] != '\0');           // ← 从 s[1] 起扫，空串时已越过分配
  cut[k + 1] = j - 1;               // ← 再次越界
```

两个缺陷：

1. **空串**：`strdup("")` 只有 1 字节，扫描却从 `s[1]` 开始 —— 越过分配边界，
   会一路读到堆里第一个 `\0` 才停；`cut` 又是 0 长度 VLA，
   `cut[0] = -1` 与 `cut[k+1]` 都是**越界写栈**。
2. **`cut` 容量不够**：条目数 = 分隔符个数 + 2。原式按 `strlen(s)/2` 开，
   而 `",,,"`（每字符都是分隔符）需要 5 项却只给 1 项。

**什么时候会走到空串**：`--stillpics` 用**空项表示沿用上一张图**
（`--stillpics A.jpg::C.jpg`），解析时对每个空项调用 `fn_strtok("", ',', ...)`。
也就是说，**「每专辑只存一张封面」这个省 ASVS 预算的标准做法必然触发**。
实测本项目盘2（56 轨 / 39 个空项）稳定段错误，而盘1（63 个空项）
只是**侥幸**没崩 —— 两者共享同一份崩坏的栈布局，只是破坏到的位置不同。

**修复**：`patch_fix_fn_strtok.py`

```c
  size_t slen = strlen(s);
  int32_t cut[slen + 2];            // 最坏情况每字符都是分隔符
  cut[0] = -1;
  uint32_t j = 1, k = 0;
  if (slen > 0)                     // 空串跳过扫描
    do  if (s[j] == delim) { cut[++k] = j; }
    while (s[j++] != '\0');
  cut[k + 1] = j - 1;
```

修复后盘2 的 `returncode` 从 `-11` 变成 `0`，产物齐全。

> **教训**：
> 1. **「产物都在、最后才崩」= 找越界写，不要找逻辑错误**。
>    所有文件都写出来了说明主流程没问题，崩的是清理阶段或后续步骤 ——
>    也就是**前面某个越界写刚刚咬到关键变量**。
> 2. **gdb 下不崩 + 直接跑必崩 → 用 core dump**。
>    gdb 会改内存布局（默认还关 ASLR），`set disable-randomization off`
>    也未必能复现；core dump 不影响布局，是这类 bug 的正解。
> 3. **零长度 VLA（`int32_t x[strlen(s)/2]` + 空串）是经典地雷**。
>    凡是「按输入长度开的 VLA」，都要问一句「长度为 0 时怎么办」。
> 4. **别把「某张盘碰巧没崩」当成没问题**。盘1 与盘2 走的是同一条有缺陷
>    的代码路径，只是一个踩中了关键变量、一个没踩中。

### 16.19 `tracktext[组]` 条数不足 → `strlen(NULL)` 段错误

`menu.c` 的 `generate_menu_pics()` 画文字时按 **`ntracks[组]`** 索引：

```c
do {
      maxtracklength = MAX(maxtracklength, strlen(tracktext[group][track]));
      ...
      track++;
} while (track < ntracks[group]);
```

而 `tracktext[组]` 由 `fn_strtok(rem, ',', ...)` 生成，条数取决于
`--screentext` 里该组给了几个曲名。**条数少于 `ntracks[组]` 时，
最后一个槽位是 `fn_strtok` 写的 NULL 哨兵 → `strlen(NULL)` 段错误。**

⚠️ **与文字内容完全无关，只与条数有关**，所以「曲名看起来都对」也可能崩。

两种成因：
1. `--screentext` 少给了组（组定义数 < 音频组数）；
2. **组间漏了 `:`** —— 格式是
   `专辑标题=组1标题=轨1,轨2:组2标题=轨3,轨4`。
   漏掉冒号时整串只解析成 1 个组：该组的「组标题」被赋成轨名列表，
   而真正的轨文字落到下一个 `=` 之后，于是条数只剩原来的一小半。

**修复**：`patch_menu_screentext.py` 在解析后**统一按 `ntracks[k]` 补足
每一组**（不足处填空串），并对未定义的组补空组标题：

```c
      for (k = 0; k < dim; k++)
        {
          if (!grouptext[k]) { grouptext[k] = calloc(2, ...); grouptext[k][0] = strdup(""); }
          int need = (k < (int) ngroups) ? (int) ntracks[k] : 0;
          int have = tracktext[k] ? arraylength(tracktext[k]) : 0;
          if (have >= need) continue;
          tracktext[k] = realloc(tracktext[k], (need + 1) * sizeof(char *));
          for (int i = have; i < need; ++i) tracktext[k][i] = strdup("");
          tracktext[k][need] = NULL;
        }
```

**判据**：gdb 下 `p tracktext[group][track]` —— 若是 `(char *) 0x0` 就是这个坑。

### 16.21 翻页箭头：文字从第 2 页起错位到顶部，末页重复画 5 次

**症状**（用户报告）：「菜单翻页后不显示 Previous / Next」。

**实测定位**（56 轨 5 页，逐页提取菜单文字图的墨迹行区间）：

```
页 0  墨迹 … 408-430, 441-455      ← 441-455 是底部的 Next    ✔
页 1  墨迹 … 378-400, 408-429      ← 没有 441-455！箭头跑到第 1、2 行
页 4  墨迹 … 288-309, 441-455      ← 底部是 Previous          ✔
```

也就是说**第 2 页到倒数第 2 页**，箭头文字压在最先两首曲名上，
而底部箭头槽位置没有文字 —— 看起来就是「翻页后没有 Previous/Next」。

**根因**：`menu.c` 画箭头时把 `offset` 传进了 `mogrify_img()`：

```c
mogrify_img(arrowstring, img->ncolumns - 1, img->maxbuttons,
            img, img->maxbuttons, command1, command2, offset, img->arrowcolor);
                                /* track 绝对行号 */  /* offset 传了进去 */
```

而 `mogrify_img()` 里是

```c
y0 = EVEN(y(track + 1 - offset, maxnumtracks + 4));
```

`offset` 的语义是「**本页首轨的全局序号**」，它是给曲名用的（让每页第 1 首
都画在第 1 行）。但箭头用的是**固定的绝对行号** `img->maxbuttons` /
`img->maxbuttons + 1`（页面底部的两个箭头槽，与 `xml.c` 输出的按钮坐标同源），
**不该再减 offset**：

| 页 | `offset` | `y(track+1-offset)` = `y(13-offset)` | 结果 |
|---|---|---|---|
| 第 1 页 | 0（初始值） | `y(13)` | 底部 ✔ |
| 第 2 页起 | 12、24、… | `y(1)`、`y(-11)`… | **顶部第 1、2 行** ✗ |
| 末页 | 0（走完整个组时复位） | `y(13)` | 底部 ✔ |

**副作用（同一段代码）**：原来写的是
`do { ... } while (buttons < menubuttons + arrowbuttons);`。
`buttons` 进入循环前是**本页已画的按钮数**，而 `menubuttons` 仍是「满页容量」：
曲目正好填满时两者接得上，但**末页只有 8 首**（56 − 4×12）时
`buttons = 8`、目标 `12 + 1 = 13` → 循环 **5 轮**，把同一个 Previous
重复画在同一位置。`xml.c` 里同构的循环会输出 5 个**完全重叠**的按钮：

```
页 4   button09..button13 全部 y0=440..470    ← 5 个重叠按钮
```

**修复**：`patch_menu_arrows.py`（同时改 `menu.c` 与 `xml.c`，两处必须一致 ——
一个画文字、一个输出按钮）

- 箭头调用一律传 `offset = 0`（用绝对行号定位）
- 去掉 `do-while` 改成单次判断，并加 `arrows_drawn`/`arrows_emitted`
  与 `arrowbuttons` 的调试比对（不一致时告警）
- 顺手去掉 `char arrowstring[9]` + `strcpy`
  （`DEFAULT_PREVIOUS` 是 8 字符 + NUL = 9，正好塞满这个缓冲，本来就贴边界），
  直接把字面量传给 `mogrify_img()`

**修复后实测**：

```
页 0  13 个按钮  箭头区 441-455
页 1  14 个按钮  箭头区 441-455, 471-485     ← 底部两行，对上了
页 2  14 个按钮  箭头区 441-455, 471-485
页 3  14 个按钮  箭头区 441-455, 471-485
页 4   9 个按钮  箭头区 441-455              ← 从 13 降到 9（去掉 5 个重叠）
```

末页 `button09` 的 `y1=470`，与前面页的 Previous 位置一致 ✔

> **教训**：同一段代码里混用「相对行号」和「绝对行号」是这类错位的常见根源。
> `mogrify_img(track, offset)` 这套签名要求调用方明确自己是哪一种 ——
> **曲名用相对（配 offset）、箭头用绝对（offset 传 0）**。
> 判断方法：把「画文字」与「输出按钮」两处坐标做**逐页比对**，
> 再用抽帧看文字墨迹的实际行区间；只看菜单截图很容易以为「就是没画」。
>
> 另外：`do { ... } while (buttons < 目标)` 这种「按累计计数循环」的写法，
> 一旦入口的 `buttons` 基数与 `目标` 的基准不同（一个含本页已画数、
> 一个是满页容量），末页就会多转几轮 —— 改成单次判断更稳。

### 16.22 曲名里的 ASCII 逗号把一条标签切成两条

**症状**（用户报告）：`03. 繁星、新生,与你.flac` 在菜单里被拆成**两个按钮**。

**根因**：`--screentext` 用 `=`、`:`、`,` 三个字符分层，而**没有转义机制**
（`fn_strtok` 就是按分隔符简单切）。曲名 `繁星、新生,与你` 里有一个 **ASCII 逗号**
（前面那个 `、` 是顿号 U+3001，不是分隔符），于是这条标签被切成两条：

```
…,繁星、新生,与你,…      ← 期望 1 条
…,繁星、新生,与你,…      ← 实际 2 条
```

多出来的那一条让**之后所有标签整体错位一格**，最后一条真标签则没有位置
（`tracktext[组]` 的条数只会被补足、不会被截断，所以多余的那条顶掉了末尾）。
表现为「一首歌被拆成两个按钮」，而两端都不报错。

实测本项目有 **4 首**曲名含 ASCII 逗号：

```
繁星、新生,与你
誓约之枷,应许之愿 (feat. VISION SOUND & Thena A)
誓约之枷,应许之愿 (feat. Thena A & VISION SOUND) [伴奏版]
奔流,因你不息(游戏《鸣潮》原声音乐)
```

**定位手法**：把 `--screentext` 按 `:` 拆成组、再按 `,` 拆成轨，
统计每组的轨数 —— 与 `ntracks[]` 不一致就说明有分隔符冲突：

```
段0 轨数 = 95   ← 而该组只有 91 轨
```

**修复**：`menu_assets.sanitize()` —— 把这三个字符换成**视觉等价**的安全字符：

| 字符 | 中文语境 | 纯 ASCII 语境 |
|---|---|---|
| `,` | `，`（全角逗号） | `·` |
| `:` | `：` | `·` |
| `=` | `＝` | `·` |

含汉字的标题用全角形式（读起来几乎一样），纯 ASCII 标题用间隔号。
被判定的标题会记进 `MenuPlan.sanitized`，便于核对显示文字与源标签的差异。

修复后段0 的轨数从 95 回到 **91** ✔

> **教训**：任何「用分隔符拼接、没有转义」的字符串协议，都必须检查
> **数据里是否含分隔符** —— 尤其当数据来自用户标签（曲名/专辑名/歌名里
> 出现逗号、冒号、等号都很常见）。检查方式不是「看代码」，
> 而是**解析回来后数一遍条目数与期望是否相等**。

### 16.23 Previous 按了回不去：跳转指令与按钮位置数量不一致

**症状**（用户报告）：有几页按 Previous 回不到前一页。

**根因**：箭头在 dvda-author 里有**两处独立输出**，都必须与 `arrowbuttons`
数量一致，而我第一轮只修了其中一处：

| 输出 | 文件/函数 | 内容 |
|---|---|---|
| 按钮**跳转指令** | `xml.c` 的 `generate_amgm_xml()` | `jump menu N;`（交给 dvdauthor） |
| 按钮**位置矩形** | `xml.c` 的 `generate_spumux_xml()` | `<button x0=… y0=…/>`（交给 spumux） |

两处的 `do { ... } while (buttons < menubuttons + arrowbuttons);` **是同一个缺陷**：
`buttons` 进入循环前是「本页已画的按钮数」，而 `menubuttons` 是「满页容量」，
末页曲目不满时前者远小于后者 → 多转几轮、重复输出同一个箭头。

实测末页（91 轨 8 页）：

```
amgm（跳转）:  13 个按钮 = 7 轨 + 6 个重复的 Previous（button08..button13）
spumux（位置）:  8 个按钮 = 7 轨 + 1 个 Previous（button08）
```

**13 vs 8 不一致** → dvdauthor 按编号对应两者，多出来的 button09..13
**没有高亮区域** → 该页的 Previous 选不中、回不到上一页。

**修复**：`patch_menu_arrows.py` 现在处理**三块**（原只处理两块）：

1. `menu.c` 箭头**文字** —— `offset` 传 0（绝对行号）+ 去掉 do-while
2. `xml.c` 箭头**位置**（spumux）—— 去掉 do-while
3. `xml.c` 箭头**跳转**（amgm）—— 去掉 do-while ← **本轮补上**

**修复后实测**（逐页比对编号序列）：

```
页      amgm按钮  spumux按钮  一致   箭头跳转
0       13        13          ✔      [Next → menu 2]
1       14        14          ✔      [Next → menu 3, Previous → menu 1]
…       …         …           ✔      …
7       8         8           ✔      [Previous → menu 7]
```

**并且把这个不变量做成了构建自检**（`02_build.py` 的 `check_menu_buttons()`）：
dvda-author 跑完后立刻解析两份 XML，逐页比对按钮编号序列，
不一致就判该盘失败并打印页号与数量。自检已用旧构建的测试数据验证过能抓到
（旧第 5 页：13 vs 9 → FAIL）。

> **教训**：同一份「逻辑」被输出到**多个产物**时（这里文字/位置/跳转是三份），
> 必须逐个检查，不能修了一处就以为修好了 —— 「位置对了」不代表「跳转也对」。
> 最有效的手段是把「两边的数量/编号必须相等」变成**可机检的不变量**，
> 而不是靠人眼看菜单截图。这也是本轮唯一能提前发现该缺陷的办法。

---

### 16.24 Previous 回不到上一页：菜单 cell 结束地址用错了大小

**症状**（用户报告）：有几页按 Previous 回不到前一页 —— **不止一页**，
而且**选曲按钮完全正常**。

「只有翻页坏、选曲不坏」这个不对称就是线索：

| 按钮 | 跳转指令 | 由谁解析 |
|---|---|---|
| 选曲 | `jump group G track K` | 经 **ATSI** 定位到音频区（`atsi2.c`） |
| 翻页 | `jump menu N` | 播放器查 **AMG IFO 的菜单 PGC 表**（`amg2.c` **手写**） |

所以问题在 `amg2.c` 手写的菜单表，与前面修过的按钮几何/编号无关。

**根因**：每个菜单的 cell 结束地址这么算：

```c
if (j > 1)
  menuvobsize_sum += img->menuvobsize[j - 2] - 1;      // 累加「前面」各页
uint32_copy(&amg[i], menuvobsize_sum
                     + img->menuvobsize[img->nmenus - 1] - 1 - 1);
                     /* ↑ 恒为**最后一页**的大小，而不是当前这一页 */
```

`nmenus == 1` 时它恰好就是当前页 → 正确（所以一直没被发现）；
`nmenus > 1` 时，第 1..n-1 页的结束地址全错。

**而且错误会被各页 VOB 大小相近掩盖**。实测 8 页的 VOB 是
37/40/37/43/39/39/41/39 扇区，算出：

```
正确： [35, 74, 110, 152, 190, 228, 268, 306]
实际： [37, 73, 112, 148, 190, 228, 266, 306]
                              ^^^  ^^^            ^^^
                              巧合相同
```

⇒ **第 5、6、8 页恰好正确，其余 5 页错误** —— 与「不止一页有问题」完全吻合。
这也解释了为什么它看起来毫无规律。

**判据（在成品 IFO 里直接核对）**：

```python
# AUDIO_TS.IFO 的菜单表：0x1820 起，每项 0x13A 字节
# 正确的值在表里能找到，旧值也能找到 —— 两者都在，说明是旧代码
```

实测旧成品：IFO 里出现的正好是旧序列 `37/73/112/148/190/228/266/306`，
而正确值 `35/74/110/152/268` **一个都不存在**。

**修复**：`patch_menu_amg_cells.py` —— 把 `menuvobsize[img->nmenus - 1]`
换成 `menuvobsize[j - 1]`（当前页）。修正后的算式等价于
「累计到当前页的有效扇区数 − 1」：

```
期望 = Σ_{i<j} (sizes[i] - 1) - 1
```

起始地址一侧不用改：cell j 的 start = `menuvobsize_sum`
= cell j-1 的 end + 1，两边自洽。

修复后实测：正确值全部命中，旧值消失（只剩 3 处本来就相等的）。
两张成品盘（8 页 / 5 页）重新构建后按字节核对：

```
盘1: 页1..8  start=0,38,72,110,152,195,236,276  end=37,71,109,151,194,235,275,316
      start_j == end_{j-1}+1 全程成立；末页 end=316=317-1；next/prev = j+1 / j-1
盘2: 页1..5  start=0,40,81,124,166  end=39,80,123,165,204
      （旧构建的 start=[40,81,123,165] 是断链的，现在连续）
```

> ⚠️ **踩坑记录：第一次修错了地方。** `amg2.c` 里有**两份**这段代码：

> | 函数 | 作用 |
> |---|---|
> | `create_topmenu()` | 写的是 dvdauthor/spumux 跑**之前**的占位 IFO |
> | `create_amg()` | 写的是**最终** IFO（落在盘上的那份） |

> 两者只差 `uint32_check` vs `uint32_copy`，所以补丁的「唯一匹配」检查
> 只命中前者。结果是**看起来修好了、重编也通过了，但成品 IFO 里的值没变**。
> 我是靠「二进制里搜不到新加的调试字符串」才发现的 —— 也就是说：
> **光看补丁报告 [OK] 不够，必须验证产物**。
>
> 顺带发现 `src/amg2.c` 不在 `build_dvda_author_mlp.sh` 的还原列表里，
> 已补上（否则补丁的幂等性没有保障）。

### 16.25 校验：菜单 cell 地址链（`verify_menu.py`）

上面那个缺陷很难靠现象定位（页大小相近时错误会被抵消），所以做成
**可机检的不变量**。

#### 字段位置（实测，别信源码里的常量）

菜单表从 `0x1810` 起：`0x181C` 的 4 字节是**第 1 页 PGC 的相对指针**
（基准 `0x1810`），`0x1820` 起是 `nmenus-1` 项的索引表（每项 8 字节，
末 4 字节同样是相对指针）。页内相对 PGC 起始：

```
+0x09C 下一页菜单号   +0x09E 上一页菜单号   (uint16)
+0x11E cell 起始扇区  +0x126 同一值再写一遍  (uint32)
+0x12A cell 结束扇区                        (uint32)
```

⚠️ **`amg2.c` 里的 `0x13A` 不是 PGC 步长**。实测步长是 `0x132`，差的 8 字节
正好是**跟着这一页走的索引项**——所以「一行」= PGC(0x132) + 索引项(8) = 0x13A，
源码公式本身自洽，但**照 `0x13A` 去定位 PGC 会从第 2 页起整段错位**，
读出来全是无关数据（而且能凑出“看着像地址”的假值）。第一版校验就是这么
写的：它只用「出现两次」这种与偏移无关的启发式，8 页只认出 2 个 start，
只好退化成「聊胜于无」。改成按实测偏移直读后：

```
页   start  end  跨度  next prev
1       0     37   38    2    0
2      38     71   34    3    1
3      72    109   38    4    2
...
8     276    316   41    0    7
末页 end=316 = 317-1 ✔   跨度之和 317 = VOB 扇区数 ✔
```

#### 判据

1. 每页 `start` 的两份副本相等；
2. `start_j == end_{j-1} + 1`（链连续，无缝隙 / 无重叠）；
3. 各页跨度之和 == `AUDIO_TS.VOB` 扇区总数；
4. 第 1 页 `start == 0`，末页 `end == 总扇区数 - 1`；
5. `next/prev` 菜单号分别为 `j+1` / `j-1`（末页 `next = 0`）。

#### 反向验证（必须有）

正向通过说明不了什么——**要证明检查在旧盘上会报错**。做法是把成品 IFO
复制一份，按旧公式（`end` 一律用最后一页的跨度）改写每页 `+0x12A`：

```
改造前: 8 页, 0 个问题  → 通过 ✔
改造后: 8 页, 7 个问题  → 报错 ✔
  · 第 1 页 cell 结束于 40，但第 2 页从 38 开始（应为 41）—— 地址链断裂
  · 第 2 页 cell 结束于 78，但第 3 页从 72 开始（应为 79）—— 地址链断裂
  · ...（第 6 页未报，它的旧值与正确值恰好相同）
  · 各页 cell 跨度之和 328 != AUDIO_TS.VOB 扇区数 317
```

**第 6 页没报出来**这件事本身就是最好的注脚：错误被巧合抵消，
纯看现象只会得到「有几页不行」。

---

### 16.26 播放封面（ASVS）：「每专辑一张」是怎么成立的

需求是「每首曲子播放时显示所属专辑封面」。它由 ASVS 承担：
`AUDIO_SV.IFO`（表）+ `AUDIO_SV.VOB`（静图），与菜单（AMG）是两套独立机制。

#### 实现方式

`--stillpics` 的每一项对应一轨，**空项表示「沿用上一张图」**。我们只在
**每个专辑的首曲**给图片，同专辑其余轨留空：

```
专辑A 轨1 → A.jpg      专辑A 轨2 → (空)   专辑A 轨3 → (空)
专辑B 轨1 → B.jpg      专辑B 轨2 → (空)   ...
```

于是**每个专辑只占一张图**（盘1 28 张 / 91 轨，盘2 17 张 / 56 轨），
播放器在同专辑内维持上一张显示 —— 这正是想要的「对应」，
也顺带把 ASVS 预算压到最低（每张约 15~25 扇区，上限 1024）。

⚠️ 由此得到一个**隐含依赖**：专辑内「沿用上一张」是**按顺序**生效的，
所以一旦某个专辑的静图条目断链（例如该专辑没有 `cover.jpg`），
它之后的曲子会**显示上一个专辑的封面**，而不会留空。
`verify_menu.py` 现在会把这种情况报成 `[WARN]`。

#### `AUDIO_SV.IFO` 的字段（实测，对应 `asvs.c`）

```
0x0C  u16  有独立静图的曲目条数（= 有封面的专辑数）
0x14  u32  静图总扇区数 - 1
0x60  起   每条记录 8 字节：图数(u8) 起始图号(u16) 起始扇区(u32)
```

盘1 实测：28 条记录、每条 1 张图、起始图号 1..28 连续、
起始扇区 `0,25,46,69,…,645` 连续、`0x14 = 669 = 670-1`。

#### 校验（`verify_menu.py` 第 5c 项）

`check_stills()` 核对：条数 == 有封面的专辑数、每条 ≥1 张图、
起始图号从 1 起连续、`0x14 == AUDIO_SV.VOB 扇区数-1`、起始扇区不越界。

**已做反向验证**（把改过的 IFO 直接喂给 `check_stills_data()`，
绕开内部那次解包，否则会被重新解包覆盖）：

| 改动 | 结果 |
|---|---|
| 总扇区数 `0x14` 改错 | 报错 ✔ |
| 某条起始图号改错 | 报错 ✔ |
| 条数少声明 1 | 报错 ✔ |
| 某条图数置 0 | 报错 ✔ |
| 期望专辑数多于实际（缺 cover） | 报错 ✔ |

> 为什么要把取文件和校验拆成 `check_stills()` / `check_stills_data()`：
> 第一版合成一个函数，测试时想喂「改坏的数据」却发现**内部会重新解包，
> 把改动覆盖掉**，于是「报错测试」永远失败（看起来像检查无效）。

#### 关于 `patches/patch_stillpics_atsi_record.py`

它针对的是 `atsi2.c` 里那个 `continue` —— 当某轨没有独立图片时跳过写
ATSI 的静图引用，理由是「ATSI 没有引用的轨播放时不显示封面」。
**但实盘验证：封面显示正常，无需该补丁**（foobar2000 下 28/17 张
全部正确对应）。该脚本**仍未接入构建脚本**，保留档案以待
「换用其他播放器后封面不显示」的情形，接入前必须先在真机上验证。

### 16.27 「下一段」切不了歌：一个音频组被切成了每轨一个 title

**症状**：播放器里点「下一段」不能切歌 —— 第 1 首按能跳到第 2 首，
从第 2 首起再按就**回到该首开头**，永远走不到第 3 首；手动选到第 3、4 首
后按「下一段」还会跳回第 2 首。

**根因**在 `amg2.c` 决定「一个新 title 从哪里开始」的判断：

```c
if (samplerate != 前一轨.samplerate || bitspersample != ... || channels != ...
    || cga != ...
    || files[group][track].type     == AFMT_MLP      // ← 元凶
    || files[group][track - 1].type == AFMT_MLP)     // ← 元凶
  files[group][track].newtitle = 1;
```

紧挨着的注释写明了作者的态度：

> apparently MLP does not allow "gapless" same-audio characteristics titles,
> which means that 3 following tracks with same audio specs will create 3 titles
> instead of 1 for gapless PCM. TODO: check if this is software-dependent

也就是上游**出于「MLP 不能像 PCM 那样无缝接轨」的猜测**，让「前一轨或当前轨
是 MLP」时一律另起 title。本项目全部音源都是 MLP，于是**每首歌都自成一个
title**（实测盘1 = 91 个 title、盘2 = 56 个 title，每个 title 恰好 1 轨；
旧的无菜单构建完全同型，所以这不是菜单功能引入的）。

而 DVD 里「下一段 / 上一段」（下一章 / 上一章）的语义是
**在同一个 title 内前进到下一轨**。每个 title 只有一轨，播放器就无处可去。

**修法**：去掉那两个 MLP 子句（`patches/patch_mlp_one_title.py`），
一个音频组 = 一个 title、title 内每首歌一个 track —— 这是商业 DVD-Audio
盘的常规布局。连带收益是一张盘可以**连续播放到底**。

保留音频属性比较：组内若中途中采样率/位深/声道数变化，仍会正确地切成多个
title（本项目分组已按属性做过，不会触发）。

#### 但只改这一处会连带崩掉（必须同时改 `ats.c`）

`ats.c` 里那个刷 pack 的块挂在 `if (files[i].newtitle)` 上，
它**同时承担两件事**：

```c
if (files[i].newtitle)
  {
    write_pes_packet(...);      // ① 把上一轨余量刷成一个独立 pack
    ++pack;
    bytesinbuf = 0;
    pack_in_title = 0;          // ② 每轨归零
  }
files[i].first_sector = files[i - 1].last_sector + 1;
```

- ① 保证 `files[i].first_sector = files[i-1].last_sector + 1` 成立在
  **pack 边界**上；不刷则下一轨从扇区中途开始，读盘端 `get_ps1()` 取不到
  pack 头 → **整首曲子被丢弃**。
- ② MLP 的 `info->mlp_layout[]` 是**按轨**分配的
  （`allocate_mlp_tracktable()`），而 `write_lpcm_header()` 用它算偏移：

  ```c
  frame_offset = info->mlp_layout[pack_in_title].pkt_pos - ...
  ```

  跨轨累加就会读到数组外面。

实测症状：`write_lpcm_header` 收到 `pack_in_title = 880418`，
读 `mlp_layout[880418]` → **段错误**（core 回溯一目了然）。

改成**无条件按轨刷**即可。对 MLP 盘来说这与改动前行为**完全一致**
（原版每轨都刷），所以 AOB 的字节内容不变，变的只有 ATSI / AMG 的标题表
—— 校验也证实了这一点：成品 ISO 内音轨与源 MLP **逐字节一致**。

修完后实测：`ATS_01_0.IFO` 的 `numtitles = 1`、`title1: tracks = 56`，
一个 title 含全部曲目。

---

### 16.28 菜单改成「一页一个专辑」：栈数组容量与 ASVS 记录数

需求：**大标题 = 光盘标题，小标题 = 专辑名，一页一个专辑，每页背景 = 该专辑封面。**

好消息是 dvda-author 本来就有这个机制：`menu.c` 在 `ncolumns == 1` 时
的画页循环**一次只画一个文字段、段内曲目画完才换页**；而 `--screentext`
第一个 `=` 之前是 `albumtext`，由 `prepare_overlay_img()` 画在
**每一页**顶部。所以只要让 `--screentext` **每段 = 一个专辑**：

- 段标题 → **小标题**（`grouptext[段][0]`，字号 = `0.8 × --fontsize`）
- `albumtext` → **大标题**（字号硬编码 `DEFAULT_POINTSIZE = 25`，每页都有）

因此 `MAX_POINTSIZE` 收到 **30**，保证「大标题（25）> 小标题（24）」。

需要自己动手的是三件事：

1. **页与音频组解耦**。按钮是 `jump group G track K`，G/K 是
   「音频组 + 组内曲目号」，与「页」无关。新增 `compute_menu_pages()`
   解析 `--screentext` 得到每页曲目数，再按音频组的轨数把每页换算回
   `(组, 组内首轨)`。**任何一步对不上就整体放弃**并退回旧排版 ——
   按钮错位不会报错，只会点一首播成另一首。
2. **每页行数按该页实际曲目数取**。原 `maxbuttons = ceil(总轨数/页数)`
   是全局值，各专辑曲目数不同必然有页画不下。行数同时决定文字行距
   （`mogrify_img` 的 `maxnumtracks`）与按钮矩形（`compute_coordinates`
   用 `img->maxbuttons`），按页取同一个值即可保证两边对齐。
3. **`ntracks[]` 的语义改为「每页曲目数」**。`menu.c` / `xml.c` 里的
   `ntracks[k]` 本来就该是「第 k 段的曲目数」，只是原先段 == 音频组才恰好一致。

#### 坑 1：`main` 的栈数组只有 9 个元素

```c
uint32_t tab0[9] = {0};   // soundtracksize
uint32_t tab1[9] = {0};   // grouptextsize  ← globals->grouptextsize 指向它
uint32_t tab2[9] = {0};   // tracktextsize  ← globals->tracktextsize 指向它
```

`menu.c` 会**直接往这两个数组里写**（`globals->grouptextsize[k] = 2;`）。
原先「文字段 == 音频组」最多 9 个；现在文字段 == 页数（实测 17 / 28），
写 `tab1[16]` 就冲掉 `main` 的栈 canary：

```
*** stack smashing detected ***: terminated     （退出码 134）
```

core 回溯显示溢出是在 `main` **返回时**才被检出（`dvda-author.c:435`）
—— 也就是说**崩溃点离肇事点很远**。修法：容量改为 `MENU_TEXT_GROUP_MAX`
（256，因为 `--nmenus` 是 `uint8_t`），并在解析处加
`dim > MENU_TEXT_GROUP_MAX` 的报错守卫。

#### 坑 2：ASVS 的记录数是按 **title** 分的

一页一个专辑后，`AUDIO_SV.IFO` 的静图表从「28 条记录、各 1 张图」变成
「**1 条记录**、28 张图」。两种都是合法的编组：

| 标题结构 | ASVS 记录数 | 图的总数 |
|---|---|---|
| 每轨一个 title | 每专辑 1 条 = 28 条 | 28 |
| 一组一个 title | 1 条 | 28 |

所以校验的判据必须是**「图的总数 == 有封面的专辑数」**，而不是
「记录数 == 专辑数」—— 否则会把正确产物判为失败
（`verify_menu.py` 已按此修正）。播放时「哪一轨显示哪一张」由 ATSI 的静图
记录 `(图号, 轨号, onset)` 决定，与记录条数无关；那部分本来就是**按轨**的，
「同专辑后续轨无记录 → 沿用上一张」的机制也不变。

#### 坑 3：容器变紧

一页一个专辑 → 页数变多 → 每页一张整幅封面 → `AUDIO_TS.VOB` 从 317 扇区
涨到 **943 扇区**（盘1）。盘1 剩余空间从 56.2 MB 降到 **54.9 MB**，
仍然可刻，但余量更小了。

#### 怎么确认「画对了」

不想只靠肉眼看截图时，**直接分析渲染产物**：`<build>/tmp/discN/` 里有
`hlpic<页>.png`（文字层，带 alpha）与 `bgpic<页>.jpg`（背景帧）。
把 alpha 逐行统计墨迹，即可得到每一行文字的 y 范围：

```
y  28.. 76  右边界 x=408   ← 大标题(基线 48) + 小标题(基线 74) 合成一带
y  97..124  右边界 x=134   ← 曲目1（y(1, maxbuttons+4=10) = 122）✔
y 145..172                 ← 曲目2（基线 170）✔
...
y 388..410                 ← Next 箭头（基线 410）✔
```

顺带能验证「文字没超出画面」（右边界 < 720），以及**各页行数不同时行距
确实跟着变**（页1 六轨行距 48、页2 五轨行距 53）。背景则用
`compare -metric RMSE` 与该专辑的 `cover.jpg` 比对（实测页1 与「飞行雪绒」
封面 RMSE = **1.4%** → 就是它；页2 与它 12% → 不是）。

---

### 16.29 「下一曲」跳回第 1 首：一个 title 内的时间轴不连续

把「每轨一个 title」改成「一组一个 title」之后（16.27），**选曲与自动连播都正常**，
但**不管哪一首按「下一曲」都跳回曲目 1**。

#### 根因：MLP 的 PTS 是按轨的，合并后没有平移

MLP 每轨都从 `PTS0`（实测 98）起算，AOB 里每条音轨的 pack 头 PTS 都从 98 重新
开始（实测一个 AOB 内有 24 处回落，正好是该 AOB 里的轨数）。原版每轨自成一个
title，于是**每轨一条时间轴**，没问题；合并成一个 title 后，按 DVD 的结构
**一个 title（PGC）只有一根时间轴**，而 `info->first_PTS` 就是在轨道首个 pack
处取的 PTS：

```c
if (start_of_file)
  {
    if (pack_in_title == 0 || wait_for_next_pack)
      {
        info->first_PTS = PTS;   /* ← 于是 56 个 cell 的 first_pts 全是 98 */
```

于是 `ATS_01_0.IFO` 里那张 **cell 时间戳表**（PGC 的逻辑时间轴）变成 56 个 cell
全部 `first_pts = 98` —— 时间轴断成 56 段、每段都从 0 开始。播放器按
「下一曲 = 跳到下一个 Program/Cell」查这根时间轴时，寻址任何一首都会落到
PTS 98 处，也就是**第 1 首**。

这也解释了什么更早的行为：那时每轨一个 title，Next =「本 title 内下一个
PTT」，而 title 里只有 1 轨 → 绕回本首开头（正是最初看到的症状）。

#### 修法：给后续轨加累计 PTS 偏移

`patch_mlp_one_title.py`：

```c
/* create_ats：每过一个音轨边界，把上一轨的时长累加 */
pts_shift += files[i - 1].PTS_length;
files[i].pts_shift = pts_shift;

/* write_pes_packet：把本轨平移到它所属 title 的时间轴上 */
PTS += info->pts_shift;
DTS += info->pts_shift;
SCR += (uint64_t) info->pts_shift * 300;   /* 27MHz / 90kHz = 300 */
```

`first_PTS` 是同一个函数里从 PTS 取的，所以会**自动**变成累计值 ——
不需要单独去改 `atsi2.c`。偏移量用上一轨声明的时长 `PTS_length`（与 AMG 里的
title 长度同源）；实测轨间空隙 < 1000 ticks（< 11 ms），无害。

顺带给 `fileinfo_t` 加了 `pts_shift` 字段（**追加在结构末尾**，因为这个结构
也怕按位置初始化）。

#### 实测

```
修前：cell1..56 的 first_pts 全是 98
      → quick_check 报「PGC 时间轴不连续」（已用改前的成品做反向验证）

修后：cell1  = 98
      cell2  = 21016373        (= 98 + 21016275)
      cell56 = 1141013198
      末 cell 结束 1161646073  vs  len_in_pts 1161645975（差 98 = PTS0）✔

AOB 的 PTS 也连续了：98 → 392794298 → 787155098 → 1161645548（0 处回落）
```

#### 这正好对上 DVD-Audio 的「单 PGC + 多 Cell」模型

- **PGC** 提供一根**连续的逻辑时间轴**（就是上面那张 cell 时间戳表）
- **Cell** 是这根时间轴上的音频片段，一个 cell 一首歌
- cell/program 边界就是曲目分界；播放器在 PGC 内顺序播放、播完自动进下一个
- 「下一曲」= 跳到下一个 Program/Cell

`quick_check.py` 新增了这条不变量（每个 title 内所有 cell 的 `first_pts` 必须
严格递增、末 cell 结束要对得上 `len_in_pts`，容差 1 秒）。

> 教训：**时间轴是「结构」的一部分，不只是元数据**。把 N 个各自从 0 开始的
> 片段并进一个 PGC 时必须整体平移 —— 否则「能不能播」看不出任何问题，
> 但**定位**会全部塌到第一段。

---

### 16.30 不管播哪首都是第一个专辑的封面：图号按「title」计数了

**症状**：合并成一个 title 之后，播放**任何**一首曲子都显示**第一个专辑**的封面。

**根因**：ATSI 每条静图记录的**第 1 个字节是「图号」**，而 dvda-author 写的是

```c
atsi[i++] = pictitlecount;      /* 「第几个有图的 title」，按 title 计数 */
```

而 `AUDIO_SV.IFO` 那边记的是**全局**图号：

```c
uint16_copy(&asvs[k], pict + 1);   /* pict 跨 title 累计 → 全局图号 */
pict += npics;
```

原来「每轨自成一个 title」（28 个 title × 1 张图）时，两者**恰好相等**：
title k 的 `pictitlecount = k+1`，全局图号也是 `k+1` —— 又一次「按 title 计数
碰巧等于按轨编号」。

合并成一个 title 之后 `pictitlecount` 恒为 **1**。实测成品的 ATSI：

```
轨 1..8: 图号=1 flag=0x00 u16a=0x0150 u16b=0x01f9     ← 56 条全是 1
```

于是所有轨都指向第 1 张图 → 不管播哪首都是第一个专辑的封面。

> ⚠️ **我在这里的第一版「修法」是错的，而且把播放器搞崩了。**
> 我当时以为那个字节是「图号」，把它改成 1..17。实际它是「**第几条 ASVS
> 记录**」，而 ASVS 的记录是**按 title 一条**（记录内部才含图数+起始图号）
> —— 所以引用 2..17 指向了**不存在的记录**，播放器越界 → **跳到后面的曲目
> 直接闪退**。该改动已回退。
>
> 正确的修法是**把两张表的粒度一起从 title 换成 track**
> （`patch_asvs_per_track.py`），见 16.32。

---

### 16.31 `ATS_PTT_SRPT` 从来没写过

`ATSI_MAT` 里的 `ATS_PTT_SRPT`（偏移 `0xC8`）**dvda-author 恒写 0** ——
源码里只有一句 `uint32_copy(&atsi[0xCC], 1);  // Start sector of ATST_PGCI_UT`，
`0xC8` 从未被赋值。实测成品：`ats_pgcit = 1` 而 `ATS_PTT_SRPT = 0`。

这在 DVD 里的意义是：**没有任何「分曲点」**。而「下一曲 / 上一曲」的定义就是
「跳到下一个 / 上一个 **PTT（Part Of Title）**」。

已按与 DVD-Video `VTS_PTT_SRPT` 同构的布局补写（`patch_ats_ptt_srpt.py`）：

```
+0x00 u16 nr_of_srpts        本 titleset 的 title 数
+0x02 u16 zero
+0x04 u32 last_byte
+0x08 u32 ttu_offset[nr_of_srpts]     每个 title 的 TTU 偏移（相对本表）
TTU: +0x00 u16 nr_of_ptts    该 title 的曲目数
     +0x02 u16 zero
     +0x04 { u16 pgcn; u16 pgn; }[nr_of_ptts]
```

实测产出的表（盘2）：

```
ATSI 大小=8192  ATS_PTT_SRPT(0xC8)=3 扇区 → 0x1800
PTT 表: nr_of_srpts=1 last_byte=239 ttu_offset[0]=12
  TTU@0x180c: nr_of_ptts=56
  (pgcn,pgn) = (1,1) (1,2) … (1,56)
```

表写在 PGCI 之后的扇区对齐处；`i` 推进到表末尾后，
`*atsi_sectors = ceil(i/2048)` 自动把 ATSI 从 3 扇区撑到 **4 扇区**，
而 `atstt_vobs`（AOB 起始扇区）、`atsi[28]`、`atsi[12]` 全都由它派生、
自动跟着调整（实测 `atstt_vobs = 4` ✓）。

⚠️⚠️ **状态：已废弃。** 真机测试结果：**「下一曲」仍不前进，且跳转后
直接闪退** —— 说明这张表的布局/取值有错（或者它根本不是「下一曲」的解析
入口），播放器按错误指针寻址就越界了。所以本脚本已从构建流程撤下、改成
「执行即拒绝」。失败原因分析与后续前提见 16.32。

---

### 16.32 「封面按轨」方案 B，以及一次让播放器崩溃的教训

#### 教训：两次盲试的代价

16.30 里记的「图号按 title 计数」是对的，但**我据此做的第一版修复是错的**，
而且后果比原缺陷严重得多 ——

1. 我先把 ATSI 静图记录的第 1 字节当成「全局图号」，改成 1..17。
   实际它是「**第几条 ASVS 记录**」，而 ASVS 的记录是**按 title 一条**
   （记录内部才含图数+起始图号）。
   → 引用 2..17 指向**不存在的记录** → 播放器越界 →
   **跳到后面的曲目直接闪退**。
2. 同时我按「从 DVD-Video 类推」的结构补写了 `ATS_PTT_SRPT`
   （16.31）→ 「下一曲」仍不前进，**且跳转后闪退**。

两处都已回退；`patch_ats_ptt_srpt.py` 改成**执行即拒绝**，防止误接线。

**根本教训**：不要靠类推去填二进制表。这类改动「错了不报错，而是让播放器
崩」，代价极高。再动 ATSI/ASVS 的表之前必须先拿到权威结构定义，或拿到
同类型的已知良好参考盘来对比字段；两者都没有时应当**停手并如实说明**。

#### 方案 B：把粒度从 title 换成 track（两边成对改）

正确的问题描述不是「图号错了」，而是**两张表都按 title 组织，而播放器按
「当前轨」查表**：

| 文件 | 原来 | 改成 |
|---|---|---|
| `asvs.c` | 每个 **title** 写一条记录（图数+起始图号+起始扇区，加 `0x378` 起的扇区表）| 每个**有图的轨**写一条 |
| `atsi2.c` | `pictitlecount` 按 **title** 累加 | 按**有图的轨**累加 |

原来「每轨自成一个 title」，两者都等价于「按轨」；合并成一个 title 后，
前者只写出 **1 条**记录、后者恒为 **1**，于是所有轨都查第 1 条 → 全显示
第一张图。

**关键：两边必须成对改。** 只改 ATSI 的引用值（引用 1..17）而 ASVS 仍是
1 条记录，就是上面那次崩溃。

因为这是「改记录的条数与内容」而不是「猜一张未知表的布局」，风险性质不同。
用环境变量 `DVDA_ASVS_PER_TRACK=1` 启用，不设置则完全保持原行为
（不需要重新编译就能切换），失败可秒回退。

实测产出：

```
AUDIO_SV.IFO 记录数(0x0C) = 17        ← 每个专辑一条
  记录1: 图数=1 起始图号=1 起始扇区=0
  记录2: 图数=1 起始图号=2 起始扇区=25
  记录3: 图数=1 起始图号=3 起始扇区=49
ATSI 静图引用号 = [1,1,1,1,1,1, 2,2,2,2,2, 3,3,3,3,3,3, 4, 5,5, …]
  最大 = 17 ≤ 17 ✔ 不越界
```

引用序列与专辑结构完全吻合（专辑1 = 轨 1-6 → 记录 1；专辑2 = 轨 7-11 →
记录 2；…）。

> ✅ **真机已验证（PowerDVD）**：封面**随专辑正确变化**，且**不再闪退**。

#### 新增校验：静图引用号不得超过 ASVS 记录数

`quick_check.py` 增加一条（这正是本该拦住那次崩溃的检查）：

```
[OK] 静图引用号 17 <= AUDIO_SV.IFO 记录数 17 ✔
```

越界时报 FAIL 并直接点出后果与成因。**已做反向验证**：把某条引用号改成 99
（ASVS 只有 17 条）重建 ISO，检查准确报错：

```
✗ 静图引用号 99 > AUDIO_SV.IFO 记录数 17
   后果: 播放器按越界记录号取封面 → **崩溃**（跳到后面的曲目时闪退）
   成因: ATSI 与 ASVS 的「按 title / 按轨」不一致，两者必须成对改
```

---

### 16.34 「上一曲 / 下一曲」的准确现象与当前结论

（**尚未解决**。本节把现象记准，并列出已排除的方向，避免后人重复我走过的弯路。）

#### 现象（PowerDVD，盘2，56 首）

| 操作 | 观察 |
|---|---|
| 播放任意一首 | 显示 **`曲目0/56`** —— 「当前曲目」**恒为 0**，与正在播哪一首都无关 |
| 按「上一段」 | 显示仍是 `曲目0#`，**音频完全不变**（无处可退） |
| 按「下一段」 | 跳到 **`曲目1#` = 第一首**（从「0」前进一格 = 列表第 1 项） |
| 「跳转至」列表 | **曲目号正确**（1..56），选哪首就跳哪首 |

#### 由此得到的结论

- **「第 N 首 → 位置」这条正向解析是通的**：列表号正确、跳转正确 ——
  它用的是 ATSI 里那张逐轨表（`first_pts` + `first_sector/last_sector`），
  而那张表我们已确认正确且时间轴连续（16.29）。
- **「当前位置 → 第几首」这条反向追踪始终为 0**：播放器不知道当前播到
  第几首，所以「当前曲目」永远显示 0、上一段无处可去、下一段只能回到第 1 项。

也就是说，缺的是**让播放器能判定「当前落在哪个曲目区间」的信息**。
最可能的载体是 `ATS_PTT_SRPT`（16.31）—— PTT 表给出每个曲目的区间，
播放器据此反查当前位置；该字段为 0 时它只能认定「还没进入任何曲目」（0）。

#### ⚠️ 但**不要**再照 DVD-Video 的表类推着写

我按 `VTS_PTT_SRPT` 的结构补写了 `ATS_PTT_SRPT`（16.31）：
「下一曲」**没有改善**，而且**跳到后面的曲目会让 PowerDVD 直接闪退**
（错误的指针导致越界）。已撤下并把脚本改成**执行即拒绝**。

结论：**没有权威结构定义就不该动它**。这类改动的失败模式不是「不生效」，
而是「让播放器崩」，代价远高于普通的软缺陷（详见 16.32 与教训 26）。

#### 要继续攻这个问题，需要下面任一项

1. **一张同类型的已知良好参考盘**（商业 DVD-Audio）。
   把它的 `ATS_PTT_SRPT` 与我们的逐字段对比，就知道真实布局。
   这是最直接的路 —— 用现成的工具读它的 `ATS_xx_0.IFO` 即可
   （本工具链的解析代码可以直接复用）。
2. **DVD-Audio 规范的导航部分**（Part 4 的 ATS 章节），
   里面有各表的准确字段定义。

在上面的条件满足之前，建议把「上一曲 / 下一曲」当作**已知限制**接受：
- 自动连播、菜单选曲、「跳转至」列表、播放封面**都正常**；
- 唯一损失是不能用播放器的上一曲/下一曲按钮逐轨走。

（顺带记录一个已被证伪的猜测：我曾以为 ATSI 时间戳记录首字节的
`0xC000` 是「曲目起点」位 —— 补到每一轨后真机**无变化**，已撤。
说明那个位只对 `t == 0` 有意义，不是这里的答案。）

---

### 16.35 教训小结

1. **上游的「自动」功能往往只在小规模下被验证过**。加菜单前先在最小规模
   （3 首 1 页）跑通，再逐项放大到真实规模（多组 / 多页 / 多专辑），
   每一步都留一个可判定的判据。

2. **「不报错」不等于「做对了」**。菜单这一串问题里，最危险的几个
   （按钮只覆盖 32 首、菜单 VOB 根本没生成、AMG 缓冲越界）都是 exit=0 或
   崩在毫无关联的地方。必须显式核对产物：按钮总数、`AUDIO_TS.VOB` /
   `AUDIO_SV.VOB` 是否存在、`AUDIO_TS.IFO` 的扇区数。

3. **凡是「我们也算一遍」的地方，都要与上游逐字对齐并加自检**（见 16.12）。

4. **字体要按实际用到的字符集做功能性检测**（见 16.5），
   而不是查字体表或只测一个「中文」探针 —— 韩文就是这么漏掉的。

5. **`--stillpics` 的额度是「张数 × 15 扇区」，不是「图大小」**；
   省额度的唯一手段是**复用**（空项），重复给同一路径不算省。

6. **「所有阶段都成功、最后莫名段错误」→ 优先怀疑 VLA**
   （由外部变量决定大小的自动数组）。它的崩点可以离原因十几分钟远，
   并且会破坏崩溃现场的局部变量。

7. **调试手段**：`stdout` 全缓冲会吞掉崩溃前的输出（GNU `foutput` 走 stdio）。
   有效办法：`stdbuf -o0 -e0`、`dmesg`、以及**用同结构的小素材复现**
   —— 把 91 首真实文件换成 91 个 1 秒小文件，复现从 ~10 分钟压到 ~1 分钟。
   本项目可复用的复现脚本：`/tmp/m91/run.sh`（66+25 两组、9 页，
   参数与真实构建一致）。

8. **「产物都在、最后才崩」→ 找越界写，别找逻辑错误**。
   所有文件都写出来了说明主流程是对的，崩的是清理阶段 ——
   也就是**前面某个越界写刚咬到关键变量**（见 16.18）。

9. **gdb 下不崩 + 直接跑必崩 → 用 core dump**。
   gdb 会改内存布局（默认还关 ASLR），`set disable-randomization off` 也未必
   能复现。core dump 不影响布局，是这类 bug 的正解：
   `resource.setrlimit(RLIMIT_CORE, (-1,-1))` + `gdb -batch -ex bt prog core.N`

10. **凡是「按输入长度开的 VLA」，都要问一句「长度为 0 时怎么办」**。
   `int32_t cut[strlen(s) / 2]` + 空串就是经典地雷（见 16.18）。

11. **别把「某张盘碰巧没崩」当成没问题**。盘1 与盘2 走同一条有缺陷的代码路径，
   区别只是一个踩中了关键变量、一个没踩中。规模不同的两张盘都要跑。

12. **「用分隔符拼接、没有转义」的协议，必须检查数据里是否含分隔符**。
    曲名/专辑名里出现逗号、冒号、等号都很常见（本项目就有 4 首含 ASCII 逗号）。
    检查方式是**解析回来后数条目数**，不是看代码（见 16.22）。

13. **同一份逻辑输出到多个产物时，要逐个检查**。
    菜单的箭头有「文字/位置/跳转」三份输出，修了两份不等于修好
    —— 「位置对了」不代表「跳转也对」（见 16.23）。
    最有效的办法是把「两边数量/编号必须相等」变成**可机检的不变量**。

14. **自检要能在旧数据上验证「它真的会报错」**。
    `check_menu_buttons()` 写完后，我拿**旧构建**的测试数据跑了一遍，
    确认它确实报 FAIL（13 vs 9）—— 否则无法排除「自检永远通过」。

### 本项目修复的上游缺陷总览（19 个补丁脚本）

| 补丁 | 目标文件 | 修的问题 |
|---|---|---|
| `patch_base` | 多处 | 基础修复（编译/环境相关） |
| `patch_read` / `_read2` / `_encode` / `_ats_pack` | `mlp.c`、`ats.c` | FFmpeg 8 API 迁移、24-bit、pack 边界 |
| `patch_fix_fn_strtok` | `auxiliary.c` | **空串时的零长度 VLA 越界写**（见 16.18） |
| `patch_menu_paging` | `menu.c` | 分页公式（合计恒 ≤32 按钮）、页数护栏误伤 |
| `patch_menu_backgrounds` | `command_line_parsing.c` | 每页背景取错、blankscreen 覆盖、堆越界 |
| `patch_menu_screentext` | `menu.c` | `cutloop` 静态变量泄漏、条数不足、`size` 复用 |
| `patch_menu_layout` | `menu.c`、`xml.c` | `maxntracks` 当行数用 → >34 轨砸栈 |
| `patch_menu_arrows` | `menu.c`、`xml.c` | 箭头文字错位（见 16.21）、末页重复（见 16.23） |
| `patch_menu_stillpics` / `_list` | `menu.c`、`command_line_parsing.c` | `pict` 置 NULL、文件列表模式 cd 失败 |
| `patch_menu_amg_size` | `launch_manager.c` | AMG 缓冲不随页数增长（见 16.17） |
| `patch_menu_amg_cells` | `amg2.c` | 菜单 cell 结束地址用错大小 → 部分页 Previous 失效（见 16.24） |
| `patch_atsi_dynamic` | `atsi2.c` | ATSI 表固定 3 扇区 → 一组最多 ~65 轨（见 16.19） |
| `patch_mlp_one_title` | `amg2.c`、`ats.c` | MLP 每轨自成 title → 「下一段」切不了歌（见 16.27） |
| `patch_stillpics_atsi_record` | `atsi2.c` | 静图记录被跳过（封面引用） |
| `patch_menu_one_album_per_page` | `menu.c`、`xml.c`、`amg2.c`、`structures.h`、`menu.h`、`commonvars.h`、`dvda-author.c` | 一页一个专辑（大/小标题、每页封面）（见 16.28） |

---

15. **补丁报 [OK] 不等于产物变了 —— 必须验证产物**。
    补丁只改了「源码里第一处匹配」，而同一段代码在文件里有两份
    （一份写占位 IFO、一份写最终 IFO），结果重编通过、产物却没变
    （见 16.24）。判断方法是**在二进制/产物里搜新代码的特征**
    （例如新增的调试字符串），而不是只看补丁的日志。

16. **同一个文件里可能有同一段代码的多份副本**。改之前先
    `grep -c` 数一遍，别依赖「唯一匹配」检查替你发现 ——
    它只能告诉你「有多份」，不能告诉你「该改哪一份」。

17. **给「很难靠现象定位」的缺陷留一个可机检的不变量**。
    cell 地址链就是这么被固化下来的（见 16.25）：每页 VOB 大小相近时，
    错误的地址会在部分页上恰好抵消，纯靠看现象只能得出「有几页不行」。

18. **一段代码可能承担两件事**。删条件前先看清它到底在做什么 ——
    `if (files[i].newtitle)` 除了「分 title」，还在**按轨对齐 pack**
    并把 `pack_in_title` 归零。只去掉前者，后者静默失效 → 段错误。
    判据：改完之后，产物里**与改动无关的部分应当逐字节不变**
    （这次靠「成品 ISO 内音轨与源 MLP 逐字节一致」确认下来）。

19. **固定容量的栈数组遇到「数量级变化」就会炸**。`tab1[9]` 配
    「最多 9 个音频组」够用；文字段改成「页」之后是 17 / 28，直接冲掉
    `main` 的 canary，而**报错点（main 返回时）离肇事点很远**。
    凡是「数量由数据决定」的容器，先问它的容量是按什么假设定的。

20. **判据要跟着结构变**。ASVS 记录数从「每专辑一条」变成「每 title 一条」
    是**同一份数据的两种合法编组**；校验若写死「记录数 == 专辑数」，
    就会把正确产物判为失败。改成「图的总数 == 专辑数」才是它真正的不变量。

21. **看不到图也能验证排版**：文字层是带 alpha 的 PNG，逐行统计墨迹就能
    算出每行文字的 y 范围，再与 `menu.c` 的 `y(track, maxnumtracks)`
    公式对照 —— 比肉眼看更准，还能顺手查出「超出画面」。

---



---

22. **时间轴是「结构」的一部分，不只是元数据**。把 N 个各自从 0 开始的片段
    并进一个 PGC 时必须整体平移 PTS：否则「能不能播」完全正常，但**定位**会
    全部塌到第一段（见 16.29）。这类缺陷的判据要落在**结构字段**上
    （cell 的 first_pts 是否递增），而不是「听起来对不对」。
23. **同一个「下一曲不对」的背后可以是两个完全不同的结构问题**：
    先是 title 粒度（16.27），再是时间轴连续性（16.29）。每次都要重新回到
    **产物字段**去定位，不能因为「上次是那样」就套用结论。

---

24. **「按 title 计数」是 dvda-author 反复出现的假设**。合并 title 之后，
    凡是「原来每轨自成一个 title」而恰好成立的东西都要重新审一遍：
    时间轴（16.29）、封面图号（16.30）都是同一个模式 ——
    按 title 计数的值碰巧等于按轨编号，合并后塌成常数。
    排查手法：**把两个本该一一对应的量并列打印**（如「ATSI 图号序列」对
    「专辑边界」），一眼就能看出哪个塌了。
25. **缺一张表比填错一张表更容易判断**。`ATS_PTT_SRPT` 恒为 0 是「没有」，
    这种缺失可以从产物直接确认；而 `0xC000` 那种「某个位可能是这个意思」
    的猜测，试了才知道 —— 所以后者一定要**可秒回退**（单独补丁 + 备份），
    失败了就撤掉、不留半截状态。

---

26. **不要靠类推去填二进制表**。`ATS_PTT_SRPT` 我按 DVD-Video 的同名表
    类推着写，结果播放器直接崩。这类改动「错了不报错，而是让播放器崩」，
    代价远高于普通缺陷 —— 必须拿到权威定义或同类型的已知良好参考盘。
27. **一次只动一处，并且要能秒回退**。我把「改引用值」和「补一张猜的表」
    一起放出去，结果崩了却分不清是哪一个（还浪费了一轮实测）。
    后来给方案 B 加了 `DVDA_ASVS_PER_TRACK` 环境开关，不改代码就能切换。
28. **成对的约定必须成对改**。「ATSI 引用的记录号」与「ASVS 的记录条数」
    是一对：只改一边就是越界崩溃（16.32）。凡是这种「引用值 ≤ 表项数」的
    关系，都该变成一条机检不变量 —— 那条检查本来就能拦住这次崩溃。

---

## 17. 诊断手法速查

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
