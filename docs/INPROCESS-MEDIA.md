# 内置媒体处理

[简体中文](INPROCESS-MEDIA.md) | [English](INPROCESS-MEDIA.en.md)

2026-10-02。正常 GUI 的音源转换、信息读取、解码与成品校验现在直接调用随包 Windows x64 DLL，不启动 ffmpeg.exe 或 ffprobe.exe。仍使用 FFmpeg 的库实现，不需要用户另外安装 FFmpeg。

## 使用和配置

- 发布包保留完整 media-native 目录。标准包为 x64 GUI-only，不含 .NET；运行需要 .NET 10 Desktop Runtime x64。
- GUI 继续读取旧 config.env 和 JSON 方案，自动改用内置媒体组件，界面不再提供 FFmpeg / FFprobe 路径设置。
- 默认配置为 DVDA_FFMPEG=builtin:media、DVDA_FFPROBE=builtin:probe。开发 CLI 仍接受显式外部路径，用于参考对照；GUI 不使用旧路径，也不在组件缺失时自动回退到外部程序。
- 可选 ALAC 转 FLAC 的封面/标签整理仍需 Metaflac。ImageMagick、dvda-author、mkisofs 等其他工具沿用既有流程。

## 实现范围

BuiltinMedia 把项目已有的媒体请求映射到小型 C ABI，通过 P/Invoke 调用 dvda-media.dll。它仅支持本项目使用的请求，不是通用 FFmpeg 命令行实现。

原生代码负责探测、首音频流解码、ALAC 包枚举、PCM/FLAC 输出、MD5、封面复制及 DVD 菜单首帧提取。保留进度回调、超时和取消；文件先写入同目录临时文件，成功才替换目标，失败/取消清理临时产物。DLL 从 media-native 及 Windows 系统目录加载，不借用 PATH 中的库。

media 配置使用 FFmpeg 9.0.2 的 avcodec、avformat、avutil、swresample、swscale，并包含 libsoxr、zlib 及必要运行库。与 menu-bin 中供原生制盘工具使用的 MLP 专用库分开。打包器检查 DLL 清单、SHA-256、AMD64 架构与普通/延迟导入依赖。

MLP 准备沿用 SWR、禁用抖动、既有 20 位舍入和限幅，以及 WAVE 规范化。MLP 编码核心 SHA-256：ece6d0a8033a26e2528042a7b74c66c249ea3c8d7378c06809fb94c8f6bd79b8。媒体转换器的 DLL 哈希进入缓存身份，升级后重建旧身份缓存。编码输出不进行字节修补。

## 验收

- 108/108 兼容性测试通过。
- 202/202 PCM/MLP 对照通过：84 个原生格式各测试普通/噪声信号，另测重采样、16/20/24 位转换、侧环绕布局和六声道 ALAC。目标 PCM 与参考流程精确一致；完整 MLP 与直接编码参考逐字节相同。
- 35 项媒体集成断言通过，覆盖 Unicode 标签/路径、ALAC 包检查、FLAC 封面逐字节保留、音频 MD5、SOXR 解码计数、菜单帧像素、无效输入、取消及并行解码。
- 94 项发布回归断言通过。在 PATH 仅包含 Windows 与 .NET、旧 GUI 配置的 FFmpeg/FFprobe 路径故意指向不存在文件的条件下，完成中英文 GUI 启动、3 组编码、完整多语言菜单 ISO 制作及成品校验。3 组编码加 6 条制盘轨道的完整 MLP 均与旧包相同；42 个菜单/静图的解码像素一致。
- 同时修复 Metaflac 文本解析的既有问题：元数据块 type=6 不再被误当封面类型，标签读取固定 UTF-8 并排除 Windows 行尾 CR。
- 最终复制的发布目录及 ZIP 与测试包逐文件、逐哈希一致；发布包共 89 个文件，除清单自身外均通过清单哈希校验。所有媒体 DLL 为 x64，不含 FFmpeg/FFprobe EXE、CLI 或 .NET 运行时。

范围说明：SOXR 仅用于诊断重采样后的采样数，不进入 MLP 编码准备。该计数完全相同；额外探测的两份 24 位 SOXR 原始 PCM 与外部 libsoxr 构建存在最多 1 LSB 的舍入差异，因此不宣称不同构建的任意 SOXR 输出逐字节相同。实际编码链的 SWR/量化/MLP 对照全部要求并实现逐字节相同。

ISO 无损校验保留既有首轨抽样策略，六个 MLP 文件另行整文件对照。制盘回归沿用基准的工具短路径和 MENU_INDEX_MIN_ALBUMS=99，未把已有的单页索引菜单问题计为已修复。

## 发布包与体积

本机产物：build/inprocess-media-x64-release/DVD-Audio-Maker；同级 DVD-Audio-Maker.zip。

| 项目 | 上一版 MLP 专用库包 | 本版内置媒体包 |
|---|---:|---:|
| ZIP 字节 | 33,669,495 | 35,333,326 |
| 解压字节 | 54,524,247 | 59,780,290 |

ZIP 约 33.70 MiB，增加约 1.59 MiB；解压增加约 5.01 MiB。新增的是原先依赖用户安装的媒体处理能力，ZIP 格式和压缩参数不变。ZIP SHA-256：cc5ce39ebbe684469b19dcb4a428908e6ed3fbae6b05b0cb27c028b34d03ef16。

## 开发与复现

原生构建/预编译运行库复用见 [Windows 构建说明](../tools/win-build/README.md)。日常 C# 调试只需已有 DLL，不需要每次重编译原生库。

```text
dotnet run --project tests/DvdaMaker.CompatibilityTests
dotnet run --project tests/DvdaMaker.CompatibilityTests -- --builtin-pcm-integration <空目录>
dotnet run --project tests/DvdaMaker.CompatibilityTests -- --builtin-media-integration <空目录>
```

集成对照需要参考 FFmpeg/FFprobe 生成和检查测试材料；这不属于 GUI 运行要求。媒体操作测试另需 Metaflac，可用 DVDA_TEST_METAFLAC 指定完整路径。完整测试记录、构建参数、DLL 指纹与限制见 [inprocess-media-validation.json](inprocess-media-validation.json)。
