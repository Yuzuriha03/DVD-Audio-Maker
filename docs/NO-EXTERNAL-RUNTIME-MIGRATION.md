# 脱离外部运行时依赖迁移清单

[English](NO-EXTERNAL-RUNTIME-MIGRATION.en.md)

范围为 Windows x64 GUI 完整 DVD-Audio 制作流程。七步迁移均已实现，保留文末明确排除的既有组件和开发用途。新增媒体接口、菜单模块、XML 适配、资源管理和 AOB 字节比较器使用 C。

后续 C# 应用层迁移另见 [Rust 迁移方案](RUST-MIGRATION.md)。本文的“明确排除”限定于当时的原生依赖迁移任务；已完成的 C/C17 组件继续复用，Rust 应用层计划不改变本文的完成记录。

## 完成状态

| 工作 | 当前实现 | 状态 |
|---|---|---|
| ISO9660 写入及移除 mkisofs | author 内置 dvda_iso_write，流式写入 2048 字节扇区 | 完成 |
| FLAC 标签、封面 | FlacMetadataEditor 原子更新元数据，保留音频帧 | 完成 |
| 图像转 YUV4MPEG2 | dvda-image.dll 内置转换 | 完成 |
| MPEG-2、MP2、DVD MPEG-PS | C 接口 dvda_menu_create_mpg，动态链接按需源码构建的 FFmpeg | 完成 |
| 子图像及按钮覆盖 | dvda-menu-spu.dll | 完成 |
| AMGM 菜单与导航接入 | dvda-menu-nav.dll，由 author 进程内加载 | 完成 |
| 全盘逐轨验证 | C dvda-disc-verify.dll、现有 ISO reader、媒体解码和 PCM 比较 | 完成 |

## 当前运行边界

正常 GUI 保留项目构建的 dvda-author-dev.exe 作为原生制盘进程。其内部完成图像转换、菜单编码/复用、子图像、导航和 ISO 写入，不再启动 jpeg2yuv、mpeg2enc、mp2enc、mplex、spumux、dvdauthor 或 mkisofs 独立程序。GUI 媒体和图像处理也不启动 FFmpeg、FFprobe、ImageMagick、Metaflac、eac3to 或原版 SurCode。

不依赖独立程序不等于不使用其开源算法。FFmpeg 和 ImageMagick 源码构建的必要功能仍随包提供；Windows 系统 DLL 和 .NET 10 Desktop Runtime x64 仍是运行环境。发布包为 GUI-only、framework-dependent，开发 CLI 保留在源码中。

### 菜单媒体

tools/win-build/native/dvda-menu-media.c 将一帧 YUV420 编成 MPEG-2 I-frame；可选输入为 48 kHz、16 位、双声道 WAV，编码成 MP2，再复用为 2048 字节对齐的 DVD MPEG-PS。支持 PAL 720×576 / 25 fps、NTSC 720×480 / 30000÷1001 fps 及 4:3、16:9。静图不要求音频。无效尺寸、截断图像/WAV、错误采样率和空音频返回失败，清理已经打开的失败输出。

FFmpeg menu 配置只保留 author 所需 MLP/PCM、MPEG-2、MP2、解析及封装；动态链接 avcodec、avformat、avutil，不生成命令行程序，不启用网络、滤镜、SWR/SWS。GUI media 配置单独维护，MLP 编码仍由MLP 编码核心完成。

### 子图像、按钮与导航

tools/menu-native/vendor 迁入本项目使用的 dvdauthor 0.7.1 C 子集及 AMGM/跳转补丁；ORIGIN.json 记录来源和导入前哈希，版权与 COPYING 保留。模块仅暴露项目所需 C ABI。PNG 使用既有图像库；XML 使用 Windows XmlLite，禁止 DTD 和外部实体。

每次调用独立加载模块，专用堆和资源登记管理文件、目录及 COM 对象。退出和断言失败转为错误返回，调用后释放资源并卸载模块，不终止宿主进程。错误传回制盘流程，不接受零长度菜单作为成功输出。原生 DLL 内容进入续跑签名，升级后不复用旧算法生成的暂存 ISO。

本次覆盖 GUI 实际生成的 AMGM 菜单、子图像和导航。通用 DVD-Video 标题制作、文本字幕、SVCD、反向拆解 spuunmux 等未使用功能不在需求内，不增加对应工具或 API。

### 全盘验证

verify lossless 和 GUI“验证成品”遍历正式索引的每张盘、每个组和每条轨道：

1. ISO reader 按顺序读取所有 ATS_NN_n.AOB 分段。
2. 只读 C 比较器解析 PES，按轨序比较全部 MLP 文件字节，包含头部和结束标记；只跳过封装自身的 padding。
3. batch-surcode 使用相同 SWR、位深转换和 WAV 规范化重建目标 PCM，与 MLP 解码 PCM 比较。
4. 默认 PCM 等长；既有 SurCode 规则仅容许不足 1 ms 的完整零采样帧尾部填充。非零尾部、截短、内容差异均失败。旧外部 MLP 转换策略未知时，不用相近采样数冒充逐字节验证。

验证器不改写 MLP 或 AOB。缺失后续轨道、末轨损坏、截断扇区、错误 PES 长度和取消均有回归。MLP 核心和验收要求不变：相同目标 PCM、设置及元数据上下文必须得到整文件一致的 MLP，不能事后修补输出来制造一致。

## 明确排除

以下既有组件和开发用途不需要重新迁移或删除：

- C# GUI、CLI、共享业务 DLL 和测试项目；
- mlp_encoder.dll 及 Native/source；
- dvda-media.dll、dvda-image.dll 及其源码构建链；
- dvda-author-dev.exe 和项目补丁；局部镜像仍需已配置的完整 author 源码树；
- MLP 解析、缓存、分盘、菜单规划和日志；
- 历史 FFmpeg/FFprobe、ImageMagick、magick-shim 对照路径；
- eac3to、原版 SurCode、旧 surcode.exe 和独立回归基准；
- Python、MSYS2 GCC、Make、GPG、NASM 等开发构建工具。

这些不属于未完成迁移项。旧 DVDA_MKISOFS、DVDA_METAFLAC、eac3to 和媒体路径配置可继续导入以兼容 config.env，不因此恢复 GUI 旧程序调用。

## 构建与验收

参数见 [Windows 构建说明](../tools/win-build/README.md)，结果、哈希及限制见 [菜单迁移验证记录](menu-migration-validation.json)。先用 build-minimal-ffmpeg.py --profile menu 构建共享库，再用 build-menu-runtime.py 构建三个 C 模块，最后运行 build-image-author.py 接入现有 author。兼容性测试入口为 dotnet run --project tests/DvdaMaker.CompatibilityTests。

test-menu-media.py 覆盖 PAL/NTSC 与错误输入；test-menu-native.py 对照旧算法整文件输出；test-disc-verify.py 注入后续轨道和封装损坏；test-image-release.py、test-menu-workflows.py 跟踪实际 GUI 进程树，验证普通/索引菜单、多盘、多组及采样率/位深转换。产物放在忽略目录。硬件播放器实播不由自动测试代替。
