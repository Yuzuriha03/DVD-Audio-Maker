# MLP 编码核心 MLP 编码（2026-10-02）

[简体中文](MLP-ENCODER.md) | [English](MLP-ENCODER.en.md)

## 当前调用链

`surcode-batch`（也接受 `batch-surcode`）现在调用项目内嵌的MLP 编码核心：

`音源 → FFmpeg → 整数 PCM WAVE → 进程内MLP 编码核心 DLL → 只读校验 → MLP 缓存 → 出盘`

原版 `surcodemlp.exe`、GUI 自动化和 SSF 会话文件已经从运行链中移除。FFmpeg 的 MLP 编码提供器也已删除；FFmpeg/FFprobe 继续用于音源处理、解码、探测和成品校验。已有外部 MLP 使用 external 导入；旧配置值 surcode 仅兼容映射到 external，不再是独立模式，外部文件统一严格校验。配置 `DVDA_MLP_SOURCE=ffmpeg` 会明确报错。

## 配置

```ini
DVDA_MLP_SOURCE=surcode-batch
DVDA_FFMPEG=C:/tools/ffmpeg/bin/ffmpeg.exe
DVDA_MLP_SURCODE_SAMPLE_RATE=48000
DVDA_MLP_SURCODE_BITS=24
DVDA_MLP_JOBS=1
DVDA_MLP_EXTERNAL_DIR=
DVDA_MLP_METADATA_CONTEXT=
```

旧采样率和位深配置名保留兼容性；它们设置本批次的目标格式。采样率支持 44100、48000、88200、96000、176400、192000 Hz，位深支持 16、20、24 bit。声道沿用输入布局，低四档采样率支持最多六声道，高两档最多双声道。空的 MLP 输出目录使用构建目录下的 `mlp`。不再需要 `DVDA_MLP_SURCODE_EXE`。

转换时保留声道布局与 PCM 顺序，由 FFmpeg 完成采样率和位深准备；现有 WAVE 规范化检查有效位并处理侧环绕标签，仍兼容旧 PCM 的末尾 RIFF 对齐字节省略。20 位使用显式量化和 24 位存储容器；无重采样/降精度时 PCM 不变，不会用输出补丁修正编码结果。当前批量界面使用单一采样率、单一位深；MLP 编码核心的混合声道组接口尚未暴露为项目配置。

## 逐字节一致的条件

必须同时匹配输入 PCM、编码参数和辅助元数据上下文。默认使用确定性的空辅助 TLV 上下文，不注入当前时钟。若要复现历史原版文件，指定该次编码对应的 `MSCTX001` 上下文文件：`DVDA_MLP_METADATA_CONTEXT=.../metadata.stampctx`。上下文在编码前作为输入传入；编码器不读取对照 MLP，应用也不在输出后替换字节。

显式上下文包含 AU 总数及区间，必须与输入轨道匹配；验证历史文件时使用单轨任务，不能把一条轨道的上下文盲目复用于整张专辑。默认空上下文的输出不能宣称与任意带时间等辅助数据的历史文件完整相同。

## 发布与缓存

GUI、CLI 与内嵌 DLL 均为 Windows x64。主程序直接通过流式 C ABI 调用MLP 编码核心，显式保持 x87 PC53 算术及回调前后的浮点控制状态；没有 MLP 编码子进程。核心和编码器源码位于 `src/DvdaMaker.SurcodeTool/Native`；不依赖外部开发工作区或原版程序安装目录。

单文件发布包含该资源，运行时解压到 `%LOCALAPPDATA%/DVD-Audio-Maker/native/<SHA256>/mlp_encoder.dll` 并核验 SHA256。用户不需要安装 C 编译器。升级缓存依据源文件内容、核心、实际 FFmpeg 二进制、PCM 转换策略、元数据上下文、格式参数及输出内容；缺少来源记录的旧缓存会重建。准备和编码失败时保留已发布的 MLP。

当前 WAVE 输入使用 RIFF 32 位长度，单轨超过约 4 GiB 的 PCM 需另行支持 RF64/流式输入；遇到不支持的布局、浮点 PCM、有效位精度丢失或损坏输入会报错。

## 验证范围与复跑

迁移前的 Windows x64 自包含单文件发布包已通过 78 组完整原版对照：从合成 FLAC 经真实 eac3to、项目批量入口和MLP 编码核心编码，共 **95,138,694 字节**与对应原版文件逐字节一致；每组同时验证独立核心解压及第二次缓存命中。对照使用相同 PCM 和显式辅助元数据上下文。

本项目已经通过 84 组默认声道布局的原生编码、独立 FFmpeg 解码及 PCM 精确比较；迁移前另外 7 组真实 FLAC → eac3to → MLP 编码核心链路通过 PCM 比较及与直接核心编码的完整字节比较。涵盖中文目录、末尾 AU 零填充、16/20/24 bit 与所有六档采样率。缓存/并发/失败保护纳入兼容性测试，97/97 项通过。

```powershell
dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release
dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release -- --mlpencoder-integration
dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release -- --mlpencoder-batch-integration C:/tools/ffmpeg/bin/ffmpeg.exe
```

后两项需要真实 FFmpeg；批量入口已无需 eac3to。测试只生成合成音频，并打印保留测试材料的临时目录。

编码器验证工作区此前的 720 组核心参数检查、78 组原版文件比较属于上游证据，不等同于本项目已经暴露所有混合组配置。源码和固定二进制指纹、上游范围见 `Native/mlpencoder-validation.json`。历史 EXE 接入的复测结果见 `mlpencoder-integration.json`。

## GUI / x64 DLL 接入

GUI 的目录和参数控件替代手工编辑配置；旧 env 可以导入，CLI 仍可读取。GUI 直接调用同一业务流水线。DLL 接入后的独立回归结果记录在 gui-dll-validation.json；它与较早的 EXE 接入记录分别保存。

## 超大 AU 无损回退（2026-10-02）

MLP 编码核心现在在提交每个重启区间前检查 AU 大小。正常编码计划没有超过既有 1536 字节上限时，完整输出保持原样；只有超限区间才改用不带预测滤波器的无损编码，保留实际 PCM、可逆矩阵、声道顺序、AU 数量和显式元数据。回退仍须通过原有大小与 FIFO 检查，没有降位深、降采样或输出补丁。

本次 88.2 kHz / 24 位 / 六声道高噪声样本的超限块从 1582 字节降到 1526 字节，FFmpeg、原版 VFY 与编码器 VFY 都确认 PCM 完整还原。198 个 PCM 测试全部通过，197 个原来成功的输出字节不变；78 组原版对照共 95,138,694 字节仍逐字节一致。具体实现、边界与验证记录见 [修复说明](MLP-OVERSIZE-FIX.md)。

新增回归入口（需要 FFmpeg）：

    dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release -- --oversized-au-integration

## FFmpeg 音源预处理迁移（2026-10-02）

批量入口现在读取 DVDA_FFMPEG，支持绝对路径和 PATH。旧 eac3to 配置保留读取但不再执行；转换器或策略改变会使旧 MLP 缓存重建。原生编码源码和固定 DLL 均未改动。详见 [迁移方案和验收](FFMPEG-PCM-MIGRATION.md)。

迁移后验收：202/202 FFmpeg 批量 PCM 用例、78/78 原版整文件字节对照、104/104 兼容性测试，以及 x64 GUI 发布版转码、旧缓存迁移、ISO 制作和成品验证均通过。
