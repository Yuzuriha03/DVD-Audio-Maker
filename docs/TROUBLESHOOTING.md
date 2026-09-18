# 故障排查记录

本文档汇总开发过程中遇到的实际问题、诊断方法与修复方案。
每个案例都包含可复现的验证命令。

---

## 目录

1. [播放加速 / 进度条无法拖动](#1-播放加速--进度条无法拖动)
2. [编译 `dvda-author` 的各类错误](#2-编译-dvda-author-的各类错误)
3. [`stack smashing detected`（轨数过多）](#3-stack-smashing-detected轨数过多)
4. [`--aob-extract` 段错误](#4---aob-extract-段错误)
5. [源文件损坏被静默放行](#5-源文件损坏被静默放行)
6. [末轨 AOB 少几字节](#6-末轨-aob-少几字节)
7. [诊断手法速查](#7-诊断手法速查)

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

### 2.5 编码结果与源不一致（WAV 头被当成音频）

编码后校验发现 PCM 不匹配，源文件与 MLP 解码结果比对：

```
源 PCM 前 6 样本: [159, -145, 3, -42, -129, 164]
MLP 解码前 6 样本: [4606290, 6471750, 5702417, ...]
```

把解码结果按字节看，开头是 `R I F F` —— **编码器把 WAV 文件头当成了音频样本**。

**根因**：MLP 编码器按裸 PCM 读取输入，而流水线交给它的是带 44/100 字节头的 WAV。

**修复**：新增 `wav_data_offset()` 定位 `data` 块起始偏移并 `fseek` 跳过。

```c
static long wav_data_offset(FILE *fp);   // 解析 RIFF/WAVE 与 data 块
...
long data_off = wav_data_offset(in_fp);
fseek(in_fp, data_off, SEEK_SET);
```

修复后编码结果与 `ffmpeg` CLI 输出**逐字节一致**：

```
39831398  dvda-author 输出
39831398  ffmpeg CLI 输出
```

### 2.6 链接错误

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
[FAIL] dvda-author 生成 盘1 失败
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

## 5. 源文件损坏被静默放行

### 症状

流水线"全部成功"，但听感异常（某处跳音/加速）。

### 根因

FFmpeg 解码出错时会**跳过损坏数据但退出码仍为 0**：

```bash
ffmpeg -v error -i broken_.m4a -f null -
# [alac] invalid element channel count
# [alac] Error submitting packet to decoder: Invalid data found
# echo $?  →  0        ← 仍然返回成功！
```

单次丢 4096 采样。若不加校验，WAV → MLP → ISO 一路"成功"，实际缺音频。

### 诊断

对比**源声明时长**与**实际解码时长**：

```
源声明采样数: 10,266,144   (213.878 秒)
实际解出采样数: 10,253,856  (213.622 秒)
缺口: 12,288 采样 = 0.256 秒
```

用 `-debug_ts` 定位失败位置：

```
pkt_pts_time:116.394667   (1:56.4)
pkt_pts_time:148.394667   (2:28.4)
pkt_pts_time:180.480000   (3:00.5)   ← 听感异常处
```

### 验证流水线本身无罪

对比源解码结果与流水线 WAV：

```
M4A解码 vs WAV 差异字节: 0 / 61523136
WAV vs MLP 差异字节:     0 / 61523136
```

**流水线是忠实的**，它只是如实转录了损坏的解码结果。

### 修复

**无法修复源文件**（下载损坏或本身不完整），只能拦下。
`01_prepare.py` 加入解码完整性校验：

| 条件 | 判定 |
|------|------|
| stderr 出现解码错误关键字 | FAIL |
| 输出时长比源声明短 > 50 ms | FAIL |
| 输出时长差异 > 5 ms | WARN |

失败时：打印清单 → 写 `decode_report.txt` → **删除旧 manifest 且不生成新的**
→ 非零码退出 → `build.sh` 的 `set -e` 中止。

实测拦截效果：

```
[FAIL] 日文版   解码报错  6 处; 时长缺失 +256 ms
[FAIL] 英文版   解码报错  2 处; 时长缺失  +85 ms
[FAIL] 韩文版   解码报错 13 处; 时长缺失 +650 ms
已校验 147 首；失败 3 首，警告 0 首
```

同批正常 FLAC 零误报。

> 多处下载来源的 ALAC 文件会带 `ccea` / `hlsf` brand 标记（Apple Music 相关）。
> 这只说明来源，不代表仍有加密 —— 本例中 `alac` 明文可见、可解出大部分音频，
> 属**数据本身有缺口**。

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

## 7. 诊断手法速查

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

### 判断是数据损坏还是工具问题

关键思路：**比对流水线各阶段的中间产物**。

```bash
# 源 → 裸 PCM
ffmpeg -v quiet -i src.m4a  -f s24le -acodec pcm_s24le a.raw
# 流水线 WAV → 裸 PCM
ffmpeg -v quiet -i pipe.wav -f s24le -acodec pcm_s24le b.raw
# MLP → 裸 PCM
ffmpeg -v quiet -i pipe.mlp -f s24le -acodec pcm_s24le c.raw

cmp a.raw b.raw    # 若一致 → 流水线忠实，问题在源
cmp b.raw c.raw    # 若一致 → MLP 无损
```

若 `a.raw` 与 `b.raw` 一致但两者都短于源声明时长，则**源文件本身损坏**。
