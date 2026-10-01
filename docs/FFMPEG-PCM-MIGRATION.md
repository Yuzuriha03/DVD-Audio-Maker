# FFmpeg 音源预处理迁移（2026-10-02）

## 目标与范围

将 surcode-batch 的音源解码、重采样和目标位深准备统一交给 DVDA_FFMPEG。MLP 仍由现有 Windows x64 进程内MLP 编码核心 DLL 编码；不修改原生编码算法，不恢复 FFmpeg 的 MLP 编码分支。

## 实施顺序

1. 接入经过 PCM 验证的 FFmpeg 参数，保留声道布局，显式准备 16/20/24 位整数 PCM；无重采样和降精度时要求 PCM 逐样本一致。
2. 更新配置、GUI、日志和缓存身份；旧 config.env 可以读取，旧 eac3to 路径不再参与执行。FFmpeg 支持绝对路径及 PATH 查找。
3. 增加真实批量入口回归，覆盖 DVD-Audio 全格式、声道顺序、20 位精度、转换、失败保护、取消和缓存。
4. 对比原版成功样本的整文件字节；相同 PCM 与元数据必须产生相同 MLP。重采样或降位深不承诺复现 eac3to 的转换结果，要求新链路的目标 PCM 与 MLP 解码精确一致。
5. 运行兼容性测试，更新 Windows x64 发布包，通过发布版编码、缓存和制盘验证。

## PCM 策略

只选择首个音频流，不混音、不强制改变声道布局。使用固定的 swresample 参数与禁用抖动的可重复转换。20 位 PCM 在 FFmpeg 滤镜中显式量化，并以 24 位容器存储；现有 WAVE 规范化负责验证有效位和 DVD 环绕标签，不修补 MLP 输出。

参数语义通过本机 FFmpeg 的 filter=aresample 和 filter=aeval 帮助核对。已有高噪声与普通信号基准扩展为真实批量入口测试。

## 已完成验收

- 兼容性测试 104/104 通过，覆盖失败、取消、输出保护、PATH、旧 env 和 GUI 日志。
- 真实批量入口 202/202 通过：84 个原生格式 × 普通/高噪声两类信号，15 个转换场景 × 两类信号，另有 3 个侧环绕布局及 1 个六声道 ALAC 场景。
- 每个用例均验证目标 PCM 与独立 FFmpeg 解码一致、末尾只有既有 AU 零填充，并与直接核心编码比较整文件字节。原生格式额外验证输入 PCM 不变；20 位降精度验证舍入和限幅。
- 原版对照 78/78 通过，共 95,138,694 字节。链路为源 WAVE → FLAC → 新批量入口的 FFmpeg → 固定 DLL，使用相同辅助元数据。
- 原生 MLP 编码源码和固定 DLL 未修改；DLL SHA256 保持 ece6d0a8033a26e2528042a7b74c66c249ea3c8d7378c06809fb94c8f6bd79b8。
- 发布版 GUI 的超限历史样本转码、缓存复用、旧 eac3to 身份缓存重建、ISO 制作和成品校验全部通过。测试特意设置不存在的 eac3to 路径。
- GUI、CLI、MLP DLL 和本机 FFmpeg 均为 AMD64。发布清单 146 个文件的 SHA256 验证通过，发布 ZIP 已更新。

完整验收摘要及用例见 [ffmpeg-pcm-migration-validation.json](ffmpeg-pcm-migration-validation.json)。

## 使用

在 GUI 的“音频编码”页设置“音源转换与校验工具”，对应 DVDA_FFMPEG。默认值 ffmpeg 从 PATH 查找；也可选择 ffmpeg.exe 的完整路径。旧 config.env/JSON 中的 DVDA_MLP_EAC3TO_EXE 会保留读取，但不再参与验证、执行和缓存身份。DVDA_MLP_SOURCE 继续使用 surcode-batch；MLP 编码仍为进程内 DLL。

首次迁移会按新的转换器身份重建旧 MLP 缓存。转换失败或取消不会覆盖已有 MLP。重采样、降位深的目标 PCM 可能与 eac3to 不同；这一差异发生在音源转换阶段，不通过改变编码文件来掩盖。

复现命令：

    dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release
    dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release -- --ffmpeg-pcm-integration <空输出目录>
    dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release -- --ffmpeg-original-corpus <原版矩阵.json> <空输出目录>

发布入口：build/gui-x64-release/DVD-Audio-Maker/DVD-Audio-Maker.exe。真实音源测试材料保存在 build/ffmpeg-migration-20261002；测试不改动用户音乐目录。
