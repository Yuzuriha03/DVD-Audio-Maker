# 开发与调试

[简体中文](DEVELOPMENT.md) | [English](DEVELOPMENT.en.md)

标准用户包只包含 GUI；CLI 源码、解决方案项目和测试继续保留。GUI 直接调用公共制作模块，不需要 CLI 子进程。开发使用 .NET 10 SDK、Windows x64。

媒体处理现在使用进程内 DLL。源码运行前，把已验证发布包的 media-native 目录复制到 build/media-native，或按 [Windows 构建说明](../tools/win-build/README.md) 构建。日常修改 C# 后无需重新编译原生库；MSBuild 的 NativeMediaDirectory 和运行时的 DVDA_MEDIA_NATIVE_DIR 可指定其他目录。GUI 不再要求安装 FFmpeg/FFprobe，对照集成测试仍需要参考程序。

GUI 只提供检查音源、开始制作、验证成品；预演仅通过开发 CLI 的 cli.cmd build --dry-run 使用，会写缓存和独立索引。GUI smoke action 仅接受 Prepare、Build、Verify，旧 Preview 请求明确失败。

## 快捷入口

在源码根目录执行：

```bat
gui-debug.cmd
cli.cmd config
cli.cmd config --config "C:\work\test settings.env" --shell
cli.cmd prepare --config "C:\work\test settings.env"
cli.cmd build --dry-run --config "C:\work\test settings.env"
dotnet run --project tests/DvdaMaker.CompatibilityTests -c Debug
```

两个新增脚本自动构建 Debug / win-x64，转发全部参数并返回真实退出码，不需要先生成发布包。Debug 构建保留 PDB 供断点调试；发布包仍不带开发符号。现有 gui.cmd、build.cmd、verify.cmd 继续可用。准备和编码会读写所选音源及工作目录，复现问题时请使用独立测试配置。

打包使用独立的 `tools/win-build/publish/build-artifacts-win-x64` 编译目录，每次清理后重新构建；不复用开发测试留下的 bin/obj，避免增量缓存把调试记录带入发布程序集。

## VS Code 断点调试

安装工作区推荐的 C# 扩展和 .NET 10 SDK，在“运行和调试”中选择：

- GUI (Debug x64)：F5 构建并启动界面。
- CLI (Debug x64)：默认只打印配置；在 .vscode/launch.json 的 args 中填入要复现的子命令及 --config。
- Compatibility tests (Debug x64)：运行兼容性测试，可在失败路径设断点。

preLaunchTask 自动构建对应项目，工作目录固定在仓库根目录。可在 DesktopWorkflow、BuildPipeline、SurcodeMlpProvider 和 MlpEncoder 设置托管断点。MLP 的 C 核心需要单独的原生调试器与匹配符号；这些 C# 配置不会自动提供 C 内部单步调试，不要为调试改变正式编码核心的浮点编译选项。

## 发布配置

默认生成 framework-dependent Windows x64 GUI 单 EXE；运行端需要 .NET 10 Desktop Runtime x64。EXE 只内嵌运行组件，说明、许可、配置示例及组件授权声明作为旁文件，一起打包进一个发布 ZIP。只在发布阶段嵌入压缩资源；Debug 的 bin/obj、PDB、CLI 和 F5 不变。

```bat
tools\win-build\build-all.cmd ^
  --source "D:\dev\winbuild\src" ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --media-runtime "build\media-native-shared" ^
  --image-author "build\image-author-shared"

rem Optional diagnostic folder, using the same native builds
tools\win-build\build-all.cmd ^
  --source "D:\dev\winbuild\src" ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --media-runtime "build\media-native-shared" ^
  --image-author "build\image-author-shared" --directory --include-cli --output "D:\release-diagnostics"
```

媒体与菜单从同一个 shared FFmpeg 前缀构建，目录/缓存中的单套 DLL 位于 menu-bin。可用 DVDA_MEDIA_NATIVE_DIR 指向该目录，DVDA_IMAGE_NATIVE_DIR 指向相邻 image-native。缓存默认 %LOCALAPPDATA%/DVD-Audio-Maker/runtime；开发测试可设置 DVDA_BUNDLE_CACHE_ROOT 隔离缓存。完整步骤见 [Windows 构建说明](../tools/win-build/README.md)，验收设计见 [单文件发布](ONEFILE-PUBLISH.md)。

下面保留历史验收记录，其体积及自包含行为不代表当前默认发布。

## 2026-10-02 验收

- 默认包：build/gui-only-x64-release/DVD-Audio-Maker；423 个文件，357,490,318 字节，ZIP 为 172,946,471 字节。CLI 的五个运行文件全部不存在。
- 可选诊断包：build/gui-cli-diagnostics-x64-release/DVD-Audio-Maker；CLI 入口和独立说明存在，转发正常。
- 39/39 专项检查和 Debug / x64 的 107/107 兼容性测试通过。
- 实际执行三条 VS Code preLaunchTask 构建命令，确认三个调试目标及 portable PDB；两个开发脚本启动成功，包含空格的参数和 CLI 失败退出码正确传递。未执行 VS Code 界面内的交互式断点操作。
- 发布与调试编译目录已隔离；发布应用程序集无 CodeView/内嵌 PDB 调试记录。
- 中英文 GUI 在隔离全局运行库搜索路径后均从包内加载 CoreCLR；仅 GUI 包直接完成单、双、六声道编码，三组完整 MLP 与之前发布包逐字节相同。
- 原生 MLP 核心指纹、工具、字体和菜单素材保持不变；两个包的清单及 ZIP 内容均已校验。

详见 [gui-only-validation.json](gui-only-validation.json)。

## 图像组件调试

当前菜单开发还需要与源码匹配的 author、menu-bin DLL、image-native 和 data/menu。将测试配置的 DVDA_AUTHOR 指向完整新包的 menu-bin/dvda-author-dev.exe，DVDA_AUTHOR_SRC 指向同包 data；不要只换 EXE 或继续配合旧菜单工具。可运行 gui-debug.cmd --config <测试配置>；DVDA_IMAGE_NATIVE_DIR 可指向新包 image-native，DVDA_MEDIA_NATIVE_DIR 指向 media-native。三份新增 C 模块用 tools/win-build/build-menu-runtime.py 重建，author 用 build-image-author.py --menu-runtime <模块目录> --ffmpeg-runtime <menu配置安装目录> 接入。

GUI 与原生制盘共用 image-native/dvda-image.dll。从已验证发布包复制 image-native 和相邻 menu-bin/fonts，或设置 DVDA_IMAGE_NATIVE_DIR 指向完整图像目录。普通 C# 修改不需要重新编译 ImageMagick。构建及错误/取消回归方法见 [内置图像处理](INPROCESS-IMAGES.md) 与 [原生构建](../tools/win-build/README.md)。
