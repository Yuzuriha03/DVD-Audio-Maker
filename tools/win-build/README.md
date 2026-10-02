# Windows 原生发布包组装

[简体中文](README.md) | [English](README.en.md)

本目录使用纯 Windows CMD 和 C# 组装 DVD-Audio Maker 自包含发布包。

组装过程不调用 PowerShell、WSL、Bash、MSYS2 shell、Autotools 或 Make。图像和媒体原生库维护构建使用 Python 及 Windows 上的 MSYS2/MinGW-w64；日常 C# 开发复用已验证的产物。

## 架构边界

仓库中的 C# 项目可由 .NET 10 SDK 直接构建。第三方 C 工具当前没有可维护的 CMake 或 Visual Studio 工程，因此打包器不会尝试翻译上游 Autotools 构建。

必须事先准备完整的 Windows x64 工具目录。默认位置为：

```text
tools\win-build\prebuilt\
```

也可设置：

```bat
set DVDA_PREBUILT_DIR=D:\dev\winbuild\menu-bin
```

## 必需第三方文件

预编译目录至少需要：

```text
dvda-author-dev.exe
mkisofs.exe
dvdauthor.exe
spumux.exe
spuunmux.exe
jpeg2yuv.exe
mpeg2enc.exe
mplex.exe
mp2enc.exe
fonts\NotoSansCJKsc-Regular.otf
fonts\NotoSansCJKjp-Regular.otf
fonts\NotoSansCJKkr-Regular.otf
```

同时放入原生工具所需的 DLL 和运行期文件；无需提供 ImageMagick EXE/XML，image-native 目录拥有独立配置。打包器不下载依赖；仅按版本指纹完全匹配的已验证配置清理闲置 DLL，未知版本保留原样。原版工具配置清理六个 ImageMagick DLL；采用经过验证的 MLP 专用 FFmpeg 库后，再清理 74 个不再被使用的 DLL。

打包时将三个 OTF 的相同字体表合并到一个标准 TTC 集合，保留 SC、JP、KR 的全部字形和区域映射，并生成按 face 索引选择字体的 ImageMagick 配置。预编译目录也可直接提供已生成的 fonts/DvdaNotoCJK-Regular.ttc；打包器会检查三个区域 face。没有删除字符，也没有改变 ZIP 压缩参数。

`dvda-author-dev.exe` 必须包含本项目所需的 24-bit MLP、菜单、多语言字体和 UTF-8 argv 修复；菜单工具必须包含 AMGM 与 `jump group ... track ...` 支持。

## 运行期素材树

打包还需要完整素材树中的：

```text
menu\silence.wav
menu\activeheader
```

默认尝试使用 `tools\dvda-author-mlp8\`。如果局部镜像不含素材，请设置 `DVDA_SRC_TREE` 或使用 `--source`。

## 生成发布包

```bat
tools\win-build\build-all.cmd ^
  --source "D:\dev\winbuild\src" ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --output "D:\DVD-Audio-Maker-Release"
```

可用参数：

| 参数 | 说明 |
|---|---|
| `--repo` | 仓库根目录；通常自动发现 |
| `--source` | 完整运行期素材树 |
| `--prebuilt` | Windows 第三方工具目录 |
| `--output` | 发布输出根目录 |
| `--include-cli` | 可选：生成包含 CLI 的开发诊断包；默认只发布 GUI |
| `--framework-dependent` | 精简模式：不包含 .NET 运行时；用户需安装 .NET 10 Desktop Runtime x64 |
| `--image-author` | 重编译的制盘程序及 author-build.json 所在目录，默认 build/image-author |
| `--ffmpeg-libraries` | 已验证的 MLP 专用 x64 FFmpeg DLL 目录；替换三个库并按指纹清理闲置依赖 |

`build-all.cmd` 启动 `src/DvdaMaker.Toolchain`，默认只发布 x64 GUI，复制第三方工具和菜单素材、生成 SHA-256 清单并创建 ZIP。标准包不构建或复制 CLI，也没有 `dvda.cmd`。

添加 `--framework-dependent` 可生成无 .NET 运行时的精简包；未指定 --output 时输出到 `tools/win-build/release-framework-dependent`。不加该参数仍生成自包含包。两种模式均在 README 开头生成对应运行要求，并附 RUNTIME.md。

显式传入 `--include-cli` 才会附加 CLI、`dvda.cmd` 和开发说明；该模式仍与 GUI 共享运行库，逐字节核对同名程序集，不允许不同内容互相覆盖。不裁剪 WinForms 或字形；ZIP 的最小体积压缩等级保持不变。

## 原生图像库

GUI 图片处理与底层制盘程序共用 x64 `image-native/dvda-image.dll`。打包时从暂存目录移除 ImageMagick EXE 及旧 XML 配置。首次打包前依次构建下列产物，也可复用经过验证的 image-native 和 image-author 目录：

```bat
python tools\win-build\build-image-runtime.py --msys-root "D:\dev\msys64"
python tools\win-build\build-image-bridge.py --msys-root "D:\dev\msys64"
python tools\win-build\build-image-author.py --source "D:\dev\winbuild\src" --msys-root "D:\dev\msys64"
tools\win-build\build-all.cmd --source "D:\dev\winbuild\src" --prebuilt "D:\dev\winbuild\menu-bin" --framework-dependent
```

脚本固定 ImageMagick/FreeType 源码归档 SHA-256，核对解压后的上游文件，再生成本项目补丁。维护构建需要 Python 3.12+、MSYS2/MinGW-w64 及 JPEG/PNG/WebP/zlib 静态开发库。制盘程序在独立源码快照中编译，不修改原生工作树或编译器安装。旧转发 EXE 仅保留为历史测试工具。

日常 C# 开发可将已验证包的完整 `image-native` 复制到 `build/image-native`，或设置 `DVDA_IMAGE_NATIVE_DIR`。保留 DLL 配置、许可说明和来源清单。发布配置按相对路径读取相邻 menu-bin/fonts，调试时也须保留这一目录结构。详见 [内置图像处理](../../docs/INPROCESS-IMAGES.md)。

已精简的 `menu-bin` 可以直接作为下次的 `--prebuilt`，无需重复编译或传入替换参数。`NativeOptimizationProfile.json` 锁定原始的 118 个原生文件；`MinimalFfmpegProfile.json` 另行锁定三个已验证的新库和 74 个额外闲置 DLL。打包时检查普通导入、延迟导入及二进制/XML 名称引用；未知或被引用的文件保留，原始输入目录不修改。

FFmpeg 重编译是独立维护步骤，需要 Windows x64 的 Python 3.12+、MSYS2 Make/GPG 和 MinGW-w64 GCC。脚本固定 FFmpeg 9.0.2 源码及签名，固定 NASM 下载哈希，产物仍为 Windows x64 DLL。

```bat
python tools\win-build\build-minimal-ffmpeg.py --msys-root "D:\dev\msys64"
tools\win-build\build-all.cmd --source "D:\dev\winbuild\src" --prebuilt "D:\dev\winbuild\menu-bin" --framework-dependent --ffmpeg-libraries "build\ffmpeg-minimal\install\bin"
```

更换编译器、参数或版本会改变哈希，打包器拒绝未验证的替换库。按 [原生依赖重编译记录](../../docs/MINIMAL-FFMPEG.md) 完成接口、音频和制盘验证后再更新指纹。这个默认 mlp 配置只用于 menu-bin 的制盘工具；MLP 编码核心是独立组件。

## GUI 内置媒体库

GUI 的音源处理不再启动外部 FFmpeg / FFprobe。日常 C# 开发可以直接把已验证发布包的 media-native 目录复制到 build/media-native，无需每次编译原生库。MSBuild 也支持 NativeMediaDirectory 属性；运行时调试可用 DVDA_MEDIA_NATIVE_DIR 指定目录。打包前必须备齐 DLL 与 media-build.json，打包器验证文件哈希、x64 架构和导入依赖。

维护原生实现时，使用独立 media 配置（MSYS2 还需 libsoxr、zlib 开发库）：

```bat
python tools\win-build\build-minimal-ffmpeg.py --msys-root "D:\dev\msys64" --work-directory build\ffmpeg-media --profile media
python tools\win-build\build-media-bridge.py --msys-root "D:\dev\msys64"
```

该目录包括音频解码、SWR/SOXR、MPEG-2/PNG 及小型 C 接口，不包含 FFmpeg 命令行 EXE；与 menu-bin 的 MLP 专用库分离。验收与能力范围见 [内置媒体处理](../../docs/INPROCESS-MEDIA.md)。


## 产物

```text
tools\win-build\release\
├── DVD-Audio-Maker\
│   ├── DVD-Audio-Maker.exe
│   ├── *.dll / *.json / 语言资源目录
│   ├── menu-bin\
│   ├── image-native\
│   ├── media-native\
│   ├── data\menu\
│   ├── config.env
│   ├── MANIFEST.txt
│   ├── README.md
│   ├── THIRD-PARTY.md
│   └── LICENSE
└── DVD-Audio-Maker.zip
```

自包含包不需要另装 .NET；精简包要求目标机器安装 .NET 10 Desktop Runtime（Windows x64）。双击 `DVD-Audio-Maker.exe` 打开 GUI，可导入旧 `config.env` 并保存 JSON 方案。MLP 编码使用内嵌原生 x64 DLL，不运行独立编码 EXE。

请完整解压并分发整个目录；EXE 依赖同目录的运行库和应用程序集，不能单独拷贝。

GUI 无需安装 FFmpeg 或 FFprobe。可选 M4A/ALAC 转 FLAC 整理功能仍需 Metaflac，可放在 menu-bin、PATH 或在 GUI 中设置路径。开发 CLI 保留显式外部 FFmpeg 路径作为对照后端。

## 使用发布包

```bat
cd tools\win-build\release\DVD-Audio-Maker
DVD-Audio-Maker.exe
```

在界面中完成检查、预演、制作和验证。开发目录保留 `cli.cmd`、`gui-debug.cmd` 和 VS Code F5 配置，详见 [开发调试](../../docs/DEVELOPMENT.md)。

## 失败诊断

- 找不到预编译目录时会明确失败，不会回退到 Bash 或 WSL。
- 目录不完整时会逐项列出缺少的 EXE 或字体。
- DLL 不在固定列表中，需由提供预编译目录的人保证完整。
- 找不到素材时，使用 `--source` 指向包含 `menu` 目录的完整素材树。

## 相关文档

- [`../../README.md`](../../README.md)：项目总览
- [`../../docs/DVDA-AUTHOR-CHANGES.md`](../../docs/DVDA-AUTHOR-CHANGES.md)：第三方源码改动
- [`../../docs/LICENSING.md`](../../docs/LICENSING.md)：第三方许可
