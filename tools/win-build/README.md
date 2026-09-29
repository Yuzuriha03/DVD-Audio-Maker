# Windows 原生工具链构建

本目录用于在 **Windows + MSYS2/MinGW-w64** 环境中编译 DVD-Audio Maker 的第三方工具链，并生成可复制到其他 Windows 机器使用的自包含发布包。

运行本构建流程时不需要 WSL，也不允许误用 WSL 的 `bash.exe`。推荐始终从 `build-all.bat` 启动。

## 产物

完整构建会生成：

```text
tools/win-build/
├── logs/                              configure 与 make 日志
├── publish/win-x64/                  C# CLI 发布中间目录
└── release/
    ├── DVD-Audio-Maker/              可直接使用的发布目录
    └── DVD-Audio-Maker.tar.gz        压缩包（默认生成）

<dvda-author 源码树的父目录>/
└── menu-bin/                          编译、组装后的本地工具目录
```

发布包包含：

- `.NET 10` 自包含的 `dvda.exe`
- `dvda-author-dev.exe`
- 带 AMGM 补丁的 `dvdauthor.exe`、`spumux.exe`、`spuunmux.exe`
- `mkisofs.exe`、mjpegtools 和 ImageMagick
- SC、JP、KR 三个独立的 Noto Sans CJK 字体 face
- 菜单运行期素材、配置、许可文件和启动器 `dvda.cmd`

目标机器不需要安装 Python、MSYS2、WSL 或 .NET Runtime。FFmpeg、FFprobe 和 Metaflac 仍需位于 `PATH`，或在发布包的 `config.sh` 中填写完整路径。

## 前置条件

### .NET 10 SDK

用于发布 C# CLI，并在需要时运行仓库内的字体提取工具：

```bat
dotnet --version
```

版本应为 `10.x`。

### MSYS2 MINGW64 工具链

建议安装或解压到以下任一位置：

```text
D:\dev\msys64
C:\msys64
D:\msys64
```

也可通过 `MSYS2_ROOT` 指定其他位置。

进入 MSYS2 后安装依赖：

```bash
pacman -Syu
pacman -S --needed mingw-w64-x86_64-gcc \
                   mingw-w64-x86_64-ffmpeg \
                   mingw-w64-x86_64-pkgconf \
                   mingw-w64-x86_64-freetype \
                   mingw-w64-x86_64-fontconfig \
                   mingw-w64-x86_64-libpng \
                   mingw-w64-x86_64-imagemagick \
                   make
```

构建脚本还会使用 `g++`、`windres`、`objdump` 以及 MSYS2 自带的 autotools 辅助文件。

## 准备 dvda-author 源码树

仓库中的 [`../dvda-author-mlp8`](../dvda-author-mlp8/README.md) 只是便于审阅差异的局部源码镜像，不能直接拿来构建。

构建需要完整的上游源码树，至少包括：

```text
configure
configure.ac
Makefile.in
src/
libutils/
libfixwav/
menu/
m4.extra.dvdauthor/
dvdauthor-0.7.1/
local.w10/bin/
```

其中：

- `menu/silence.wav`、`menu/activeheader` 等是运行期素材。
- `dvdauthor-0.7.1` 必须包含 AMGM 和 `jump group ... track ...` 支持。
- `local.w10/bin` 提供 MSYS2 仓库中没有的 `mkisofs`、mjpegtools 和已验证的 ImageMagick 构建。

从固定上游版本准备源码树的示例：

```bash
git clone https://github.com/fabnicol/dvda-author tools/win-build/src
cd tools/win-build/src
git checkout 8fca43a
git apply ../../../docs/dvda-author-changes.patch
```

源码树默认放在 `tools/win-build/src`。也可以设置：

```bat
set DVDA_SRC_TREE=D:\src\dvda-author
```

## 字体

菜单需要三个独立字体：

```text
NotoSansCJKsc-Regular.otf
NotoSansCJKjp-Regular.otf
NotoSansCJKkr-Regular.otf
```

构建脚本会优先复用已有单 face 字体；若没有，则从静态版 `NotoSansCJK-Regular.ttc` 中提取。

可以把 TTC 放在源码树根目录，或显式指定：

```bat
set DVDA_FONT_SRC=D:\fonts\NotoSansCJK-Regular.ttc
```

不要使用 `NotoSansSC-VF.ttf`、`NotoSansJP-VF.ttf` 等单语可变字体。也不要直接把 TTC 作为菜单字体：ImageMagick 按文件路径加载 TTC 时通常只取 face 0，即 JP face，中文会显示为日文字形且不会报错。

字体提取由仓库中的 `.NET 10` 字体工具完成，不再依赖 Python/fontTools。

## 一键构建

在仓库根目录运行：

```bat
tools\win-build\build-all.bat
```

MSYS2 不在默认位置时：

```bat
set MSYS2_ROOT=D:\dev\msys64
tools\win-build\build-all.bat
```

其他可选变量：

```bat
set DVDA_SRC_TREE=D:\src\dvda-author
set DVDA_FONT_SRC=D:\fonts\NotoSansCJK-Regular.ttc
set JOBS=8
set DVDA_TARBALL=0
```

`build-all.bat` 会用 MSYS2 自己的 `/usr/bin/bash` 启动 `build-all.sh`，依次执行：

1. `check-src.sh`：检查源码树、工具链、FFmpeg 开发库、字体和 .NET SDK。
2. `build-author.sh`：编译支持 24-bit MLP 和多语言字体的 `dvda-author-dev.exe`。
3. `build-dvdauthor.sh`：编译带 AMGM 补丁的菜单工具。
4. `assemble-menu-bin.sh`：组装 EXE、DLL、ImageMagick 和字体。
5. `make-release.sh`：发布自包含 C# CLI 并生成最终目录。

## 单独运行某一步

调试时可以在 MSYS2 MINGW64 shell 中运行：

```bash
cd /path/to/DVD-Audio-Maker/tools/win-build
bash check-src.sh
bash build-author.sh
bash build-dvdauthor.sh
bash assemble-menu-bin.sh
bash make-release.sh
```

一般情况下仍建议重新运行完整的 `build-all.bat`。`build-author.sh` 会重新执行 `configure`，不复用可能含有旧绝对路径的缓存。

## 发布包使用

构建成功后：

```bat
cd tools\win-build\release\DVD-Audio-Maker

dvda.cmd config --check
dvda.cmd prepare
dvda.cmd build --dry-run
dvda.cmd build
dvda.cmd verify all
```

发布包内的详细用户说明见 [`docs/README.md`](docs/README.md)。

## 常见问题

### `MSYS2 not found`

设置 `MSYS2_ROOT`，并确认以下文件存在：

```text
<MSYS2_ROOT>\usr\bin\bash.exe
```

### 检测到 WSL 或 `uname -s` 返回 Linux

说明启动了 WindowsApps 中的 WSL 启动器，而不是 MSYS2 Bash。退出后直接运行 `build-all.bat`，不要从 PATH 调用裸 `bash`。

### FFmpeg 开发库缺失

确认命令能返回版本：

```bash
pkg-config --modversion libavcodec
pkg-config --modversion libavformat
pkg-config --modversion libavutil
pkg-config --modversion libswresample
```

不能只检查 `ffmpeg.exe`；编译 `dvda-author` 还需要头文件和导入库。

### ImageMagick 无法读取 JPEG

组装阶段优先使用上游 `local.w10/bin` 中已验证的非模块化 ImageMagick。MSYS2 的模块化 ImageMagick 若缺少 coder 模块，会报 `NoDecodeDelegateForThisImageFormat`。`assemble-menu-bin.sh` 会用真实 JPEG 做提前自检。

### `configure` 极慢

大量短进程可能被实时杀毒软件的文件系统过滤驱动显著拖慢。检查安全软件，而不是先假定 MSYS2 本身必然很慢。

### 修改头文件后行为异常

上游 Makefile 不完整跟踪头文件依赖。`build-author.sh` 会主动清理旧对象；不要手动跳过该步骤。

### 中文、日文或韩文路径变成 `?`

Windows MinGW 程序默认 argv 受系统代码页限制。构建脚本会向 `dvda-author-dev.exe` 嵌入 UTF-8 active code page manifest，并检查 `activeCodePage` 是否存在。

## 相关文档

- [`../../README.md`](../../README.md)：项目总览与使用说明
- [`../../docs/DVDA-AUTHOR-CHANGES.md`](../../docs/DVDA-AUTHOR-CHANGES.md)：上游改动说明
- [`../../docs/TROUBLESHOOTING.md`](../../docs/TROUBLESHOOTING.md)：历史问题与诊断记录
- [`../../docs/LICENSING.md`](../../docs/LICENSING.md)：第三方许可
