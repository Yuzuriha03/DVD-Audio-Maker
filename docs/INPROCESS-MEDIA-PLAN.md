# 内置媒体处理方案

2026-10-02。目标：正常 GUI 工作流不再启动外部 ffmpeg.exe / ffprobe.exe，也不要求用户安装它们。

1. 基于已验证的 FFmpeg 9.0.2 编译 Windows x64 媒体库，补齐 FLAC、ALAC、AAC、PCM、MLP、DVD 菜单 MPEG-2/PNG，以及原有 SWR/SOXR 重采样能力。
2. 增加小型、可取消的原生 C 接口，由 C# 在进程内调用。迁移探测、PCM 准备、解码计数、PCM/MD5 校验、封面和菜单帧提取。
3. GUI 使用内置处理器，旧 config.env 仍可导入；移除用户必须填写 FFmpeg / FFprobe 路径的设置。开发测试保留显式外部参考路径以便做对照。
4. 保持MLP 编码核心不变。原始 PCM、重采样/位深转换、MLP、错误/取消、多声道、标签封面和完整制盘均与原流程对照验证。
5. 重新打包 x64 GUI-only / framework-dependent 包，检查依赖、清单和实际体积；不提交、推送或更新公开 Release。

边界：这里移除外部 FFmpeg 进程依赖，仍使用随包 FFmpeg 原生库。ImageMagick、dvda-author 等其他原生制盘工具不属于本次替换范围。

以上五项均已完成。结果、实际体积及测试边界见 [内置媒体处理](INPROCESS-MEDIA.md) 与 [验收记录](inprocess-media-validation.json)。
