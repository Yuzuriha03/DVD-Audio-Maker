# Windows 原生构建（不需要 WSL）

从源码构建出 `dvda-author`（含 24-bit 无损 MLP）、`dvdauthor` / `spumux`
（含 AMGM 补丁），并组装成**可分发**的发布目录。

**全部在 MSYS2 / MinGW-w64 下完成** —— MSYS2 是 Windows 原生环境，
所以这套流程：**不调用 WSL、不使用 `\\wsl.localhost`、不依赖 `/mnt/*`**。

---

## 1. 一键构建

```
build-all.bat
```

它会依次做 5 件事，任一失败即中止（这样不会在坏的基础上继续）：

| 步骤 | 做什么 | 单独跑 |
|---|---|---|
| 1 | 体检：源码树、工具链、FFmpeg 开发库、字体、字体工具 | `check-src.sh` |
| 2 | 编 `dvda-author-dev.exe`（含 MLP + 按语言分派字体） | `build-author.sh` |
| 3 | 编 `dvdauthor` / `spumux` / `spuunmux`（AMGM 补丁） | `build-dvdauthor.sh` |
| 4 | 组装工具目录（exe + DLL + ImageMagick + 字体） | `assemble-menu-bin.sh` |
| 5 | 打出可分发目录 `release/DVD-Audio-Maker/` | `make-release.sh` |

产出：

```
<源码树>/../menu-bin/                   中间产物（工具目录）
<工具包>/release/DVD-Audio-Maker/       可分发（自带 exe/DLL/字体/配置）
<工具包>/logs/                          各步骤日志
```

## 2. 前置条件

### 2.1 MSYS2

免安装版最简单：

1. 下载 `msys2-base-x86_64-*.tar.xz`
2. 用 Windows 自带的 `tar.exe` 解压到某目录（如 `D:\msys64`）
3. 跑一次 `<root>\usr\bin\bash.exe`，然后：

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

MSYS2 不在默认位置时设 `MSYS2_ROOT`（本工具包会自动探测
`<工具包>\..\msys64`、`D:\dev\msys64`、`C:\msys64`、`D:\msys64`）。

### 2.2 源码树

需要一棵**完整的** dvda-author 源码树，包含：

```
configure  configure.ac  Makefile.in      autotools 入口
src/  libutils/  libfixwav/               源码
menu/                                     运行期素材（C 代码直接引用）
  silence.wav   静音轨（菜单用）
  activeheader  菜单激活头
m4.extra.dvdauthor/                       dvdauthor 的 autotools 辅助 m4
dvdauthor-0.7.1/                          **含 AMGM 补丁**（菜单必需）
local.w10/bin/                            mkisofs / mjpegtools 等预编译二进制
NotoSansCJK-Regular.ttc                   Noto Sans CJK 静态版（抽字体用）
```

放在 `<工具包>\src` 即可，或用 `DVDA_SRC_TREE` 指定。

> **为什么 `dvdauthor-0.7.1` 和 `local.w10/bin` 必需且不能从 MSYS2 拿**：
> - 菜单按钮写的是 `<button>jump group G track K</button>`，发行版里的
>   dvdauthor 不认这个语法（需要 AMGM 补丁）
> - **MSYS2 没有 mjpegtools 包、也没有 mkisofs/cdrkit/xorriso 包**
>   （实测 `pacman -Ss` 查不到），所以静图编码与 ISO 打包只能靠上游那套
>   预编译二进制

### 2.3 字体

需要一个 **Noto Sans CJK 的静态 `.ttc`**（`NotoSansCJK-Regular.ttc`）。
工具包会在以下位置找：

1. `$DVDA_FONT_SRC`
2. `<源码树>/NotoSansCJK-Regular.ttc`
3. `<源码树>/fonts/NotoSansCJK-Regular.ttc`
4. `<工具包>/NotoSansCJK-Regular.ttc`
5. `C:\Windows\Fonts\NotoSansCJK-Regular.ttc`

从 TTC 按 family 名提取 SC、JP、KR 三个单 face 字体的工作由仓库内
`src/DvdaMaker.FontTool` 完成。该工具使用纯 C# 重建 standalone OpenType，
修复表偏移和 `head.checkSumAdjustment`，并检查 family、PostScript name 及
汉字、假名、谚文、拉丁字符覆盖；不需要 Python/fontTools。

> ⚠️ **不要**用 Windows 自带的 `NotoSansSC-VF.ttf` / `NotoSansJP-VF.ttf` ——
> 那些是**单语**字体（SC 版没有谚文），会给别的语言开出空白。

Linux 上装 `fonts-noto-cjk` 后，文件在
`/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc`，拷过来即可。

## 3. 可选环境变量

| 变量 | 默认 | 说明 |
|---|---|---|
| `MSYS2_ROOT` | 自动探测 | MSYS2 安装根 |
| `DVDA_SRC_TREE` | `<工具包>\src` | 源码树 |
| `DVDA_FONT_SRC` | 自动查找 | Noto CJK 静态 ttc |
| `JOBS` | CPU 核数 | make 并行数 |

Windows 形式（`D:\x\y`）与 MSYS 形式（`/d/x/y`）都接受 ——
bash 侧会用 `cygpath` 转换。

## 4. 装到哪里

把本工具包放在**仓库的 `tools/win-build/`** 下最省事：

```
DVD-Audio-Maker\                    <- 仓库根
  src\DvdaMaker.Cli\               <- C# CLI 项目
  docs\README.md  THIRD-PARTY.md    <- 发布包的文档
  tools\win-build\                  <- 本工具包
    build-all.bat ...
```

然后 `tools\win-build\build-all.bat` 一把跑完。

## 5. 目标机器需要什么

发布目录是自包含的（C# CLI + 工具链 exe/DLL + 字体 + ImageMagick 配置），
目标机器**只需要**：

- Windows 10 1903+ / 11（依赖 UTF-8 代码页支持）
- **FFmpeg**（在 PATH 里，或用 `config.sh` 给全路径）

**不需要** MSYS2、不需要 WSL、不需要装字体。

## 6. 常见问题

**`MSYS2 not found`**
设 `MSYS2_ROOT`，或装到 `C:\msys64` / `D:\msys64`。

**`找不到 MSYS2 的 mingw 前缀`**
MSYS2 装了但缺 `mingw64/lib/pkgconfig` —— 说明还没装工具链，
按 2.1 的 `pacman -S` 装上。

**体检报 FFmpeg 缺失但 `pacman` 说装了**
先确认真装了：`pkg-config --modversion libavcodec`。
如果这个能出结果而体检仍报缺，那是本工具包的 bug（历史上出过一次：
把 `libavcodec` 又拼了一次 `lib` 前缀，成了 `liblibavcodec.dll.a`）。

**`configure: error: C compiler cannot create executables`**
PATH 里混进了别的工具链（例如 WSL 的 `bash.exe`）。
本工具包会把 MSYS2 的 `mingw64\bin` 与 `usr\bin` 放到 PATH **最前面**；
如果仍报错，检查是否有 `MSYS2_ROOT` 指向了错误的目录。

**`make` 报 `没有规则可制作目标".../da-utf8.c"`**
说明构建脚本被改动过。`da-utf8.o` **不能**加进 `OBJECTS`（Makefile 里那条
静态模式规则会因此多出一个不存在的 `da-utf8.c` 前提）；它只能作为
`dvda-author:` 的前提。见 `build-author.sh` 的注释。

**菜单文字是空白**
看 `<源码树>/../menu-bin/fonts/` 下三个 `.otf` 是否都在。
若只有 `.ttc`，说明抽 face 那步没跑成功。

**中文菜单显示成日文字形**
`DVDA_MENU_FONT` 指向了 `.ttc`。ImageMagick 按文件路径加载 `.ttc` 时
**只取 face 0**，而 Noto Sans CJK 的 face 0 是 JP。用抽好的单 face `.otf`。

## 7. 这套工具包与 Linux 构建的关系

同一份 C# 业务实现和 dvda-author 源码，两个平台各自编排：

| | Linux | Windows |
|---|---|---|
| 编排 | `scripts/build_dvda_author_mlp.sh` + `build.sh` | 本工具包 |
| 编译器 | 系统 gcc + 系统 FFmpeg | MinGW-w64 gcc + MSYS2 FFmpeg |
| 菜单辅助 | apt 的 mjpegtools / ImageMagick + 自编 dvdauthor | 上游预编译二进制 + MSYS2 ImageMagick + 自编 dvdauthor |
| 字体 | fontconfig 家族名（`Noto-Sans-CJK-SC`） | 单 face 文件路径（`.otf`） |
| 特有处理 | —— | UTF-8 argv manifest、反斜杠路径归一化、MSVCRT 兼容垫片 |

两边的产物已实测**音频本体（AOB）与 ATSI 表逐字节相同**；
仅静图/菜单的 VOB 因两版 ImageMagick 的 JPEG 舍入略有差异
（静态图解码 PSNR 50.26 dB，肉眼不可分）。
