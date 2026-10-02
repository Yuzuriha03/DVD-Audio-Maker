# Windows 原生发布包组装

[简体中文](README.md) | [English](README.en.md)

本目录使用纯 Windows CMD 和 C# 组装 DVD-Audio Maker 自包含发布包。

组装过程不调用 PowerShell、WSL、Bash、MSYS2、Autotools 或 Make。

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
magick.exe
fonts\NotoSansCJKsc-Regular.otf
fonts\NotoSansCJKjp-Regular.otf
fonts\NotoSansCJKkr-Regular.otf
```

同时放入这些程序所需的 DLL、ImageMagick XML 和其他运行期文件。打包器会原样复制整个目录，不会猜测或下载依赖。

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

`build-all.cmd` 启动 `src/DvdaMaker.Toolchain`，默认只发布 x64 GUI，复制第三方工具和菜单素材、生成 SHA-256 清单并创建 ZIP。标准包不构建或复制 CLI，也没有 `dvda.cmd`。

添加 `--framework-dependent` 可生成无 .NET 运行时的精简包；未指定 --output 时输出到 `tools/win-build/release-framework-dependent`。不加该参数仍生成自包含包。两种模式均在 README 开头生成对应运行要求，并附 RUNTIME.md。

显式传入 `--include-cli` 才会附加 CLI、`dvda.cmd` 和开发说明；该模式仍与 GUI 共享运行库，逐字节核对同名程序集，不允许不同内容互相覆盖。不裁剪 WinForms、字体或第三方动态库；ZIP 使用最小体积压缩等级。

## 产物

```text
tools\win-build\release\
├── DVD-Audio-Maker\
│   ├── DVD-Audio-Maker.exe
│   ├── *.dll / *.json / 语言资源目录
│   ├── menu-bin\
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

FFmpeg、FFprobe 和 Metaflac 仍需放在包内 `menu-bin`、位于 `PATH`，或在 GUI / `config.env` 中配置完整路径。

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
