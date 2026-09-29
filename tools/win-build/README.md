# Windows 原生发布包组装

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

`build-all.cmd` 启动 `src/DvdaMaker.Toolchain`，依次检查文件、发布 C# CLI、复制第三方工具和菜单素材、写入 `dvda.cmd`、生成 SHA-256 清单并创建 ZIP。

## 产物

```text
tools\win-build\release\
├── DVD-Audio-Maker\
│   ├── app\
│   ├── menu-bin\
│   ├── data\menu\
│   ├── config.env
│   ├── dvda.cmd
│   ├── MANIFEST.txt
│   ├── README.md
│   ├── THIRD-PARTY.md
│   └── LICENSE
└── DVD-Audio-Maker.zip
```

目标机器不需要安装 .NET Runtime。FFmpeg、FFprobe 和 Metaflac 仍需位于 `PATH`，或在 `config.env` 中配置完整路径。

## 使用发布包

```bat
cd tools\win-build\release\DVD-Audio-Maker
dvda.cmd config --check
dvda.cmd prepare
dvda.cmd build --dry-run
dvda.cmd build
dvda.cmd verify all
```

## 失败诊断

- 找不到预编译目录时会明确失败，不会回退到 Bash 或 WSL。
- 目录不完整时会逐项列出缺少的 EXE 或字体。
- DLL 不在固定列表中，需由提供预编译目录的人保证完整。
- 找不到素材时，使用 `--source` 指向包含 `menu` 目录的完整素材树。

## 相关文档

- [`../../README.md`](../../README.md)：项目总览
- [`../../docs/DVDA-AUTHOR-CHANGES.md`](../../docs/DVDA-AUTHOR-CHANGES.md)：第三方源码改动
- [`../../docs/LICENSING.md`](../../docs/LICENSING.md)：第三方许可
