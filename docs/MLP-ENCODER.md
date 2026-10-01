# MLP 编码核心 MLP 编码（2026-10-01）

## 当前调用链

`surcode-batch`（也接受 `batch-surcode`）现在调用项目内嵌的MLP 编码核心：

`音源 → eac3to → 整数 PCM WAVE → MLP 编码核心 mlp_encode.exe → 只读校验 → MLP 缓存 → 出盘`

原版 `surcodemlp.exe`、GUI 自动化和 SSF 会话文件已经从运行链中移除。FFmpeg 的 MLP 编码提供器也已删除；FFmpeg/FFprobe 继续用于音源处理、解码、探测和成品校验。已有外部 MLP 的 `external` / `surcode` 导入模式仍可用。配置 `DVDA_MLP_SOURCE=ffmpeg` 会明确报错。

## 配置

```ini
DVDA_MLP_SOURCE=surcode-batch
DVDA_MLP_EAC3TO_EXE=C:/tools/eac3to/eac3to.exe
DVDA_MLP_SURCODE_SAMPLE_RATE=48000
DVDA_MLP_SURCODE_BITS=24
DVDA_MLP_JOBS=1
DVDA_MLP_EXTERNAL_DIR=
DVDA_MLP_METADATA_CONTEXT=
```

旧采样率和位深配置名保留兼容性；它们设置本批次的目标格式。采样率支持 44100、48000、88200、96000、176400、192000 Hz，位深支持 16、20、24 bit。声道沿用输入布局，低四档采样率支持最多六声道，高两档最多双声道。空的 MLP 输出目录使用构建目录下的 `mlp`。不再需要 `DVDA_MLP_SURCODE_EXE`。

转换时保留 PCM 声道顺序，处理 eac3to 的侧环绕声道标签和末尾 data 块省略的 RIFF 对齐字节；不会用输出补丁修正编码结果。采样率转换或降位深仍由 eac3to 完成。当前批量界面使用单一采样率、单一位深；MLP 编码核心的混合声道组接口尚未暴露为项目配置。

## 逐字节一致的条件

必须同时匹配输入 PCM、编码参数和辅助元数据上下文。默认使用确定性的空辅助 TLV 上下文，不注入当前时钟。若要复现历史原版文件，指定该次编码对应的 `MSCTX001` 上下文文件：`DVDA_MLP_METADATA_CONTEXT=.../metadata.stampctx`。上下文在编码前作为输入传入；编码器不读取对照 MLP，应用也不在输出后替换字节。

显式上下文包含 AU 总数及区间，必须与输入轨道匹配；验证历史文件时使用单轨任务，不能把一条轨道的上下文盲目复用于整张专辑。默认空上下文的输出不能宣称与任意带时间等辅助数据的历史文件完整相同。

## 发布与缓存

内嵌的 Windows x86 核心保留经过验证的 x87 精度行为，64 位 .NET 主程序以独立子进程调用。核心和编码源码位于 `src/DvdaMaker.SurcodeTool/Native`；不依赖旧外部工作区或原版程序安装目录。

单文件发布包含该资源，运行时解压到 `%LOCALAPPDATA%/DVD-Audio-Maker/native/<SHA256>/mlp_encode.exe` 并核验 SHA256。用户不需要安装 C 编译器。升级缓存依据源文件内容、核心、eac3to、元数据上下文、格式参数及输出内容；缺少来源记录的旧缓存会重建。准备和编码失败时保留已发布的 MLP。

当前 WAVE 输入使用 RIFF 32 位长度，单轨超过约 4 GiB 的 PCM 需另行支持 RF64/流式输入；遇到不支持的布局、浮点 PCM、有效位精度丢失或损坏输入会报错。

## 验证范围与复跑

最终 Windows x64 自包含单文件发布包通过 78 组完整原版对照：从合成 FLAC 经真实 eac3to、项目批量入口和MLP 编码核心编码，共 **95,138,694 字节**与对应原版文件逐字节一致；每组同时验证独立核心解压及第二次缓存命中。对照使用相同 PCM 和显式辅助元数据上下文。

本项目已经通过 84 组默认声道布局的原生编码、独立 FFmpeg 解码及 PCM 精确比较；另外 7 组真实 FLAC → eac3to → MLP 编码核心链路通过 PCM 比较及与直接核心编码的完整字节比较。涵盖中文目录、末尾 AU 零填充、16/20/24 bit 与所有六档采样率。缓存/并发/失败保护纳入兼容性测试，93/93 项通过。

```powershell
dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release
dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release -- --mlpencoder-integration
dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release -- --mlpencoder-batch-integration C:/tools/eac3to/eac3to.exe
```

后两项需要真实 FFmpeg；批量测试还需要 eac3to。测试只生成合成音频，并打印保留测试材料的临时目录。

编码器验证工作区此前的 720 组核心参数检查、78 组原版文件比较属于上游证据，不等同于本项目已经暴露所有混合组配置。源码和固定二进制指纹、上游范围见 `Native/mlpencoder-validation.json`。当前项目的复测结果见 `mlpencoder-integration.json`。
