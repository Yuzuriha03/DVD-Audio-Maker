# 组件与许可（THIRD-PARTY）

[简体中文](THIRD-PARTY.md) | [English](THIRD-PARTY.en.md)

本包是若干开源程序的再分发（含针对本用途的修改）。清单如下。

单文件发布中，下文的二进制路径相对于程序自动生成的运行缓存。组件许可文本以 `NOTICE-Image.txt` 和 `NOTICE-Menu.txt` 放在 ZIP 根目录，与 EXE 同级。构建来源 JSON 仅用于本地打包校验，不随用户发布包分发。

## 主程序

| 组件 | 版本 | 许可 | 说明 |
|---|---|---|---|
| **dvda-author** (`menu-bin/dvda-author-dev.exe`) | 上游 `8fca43a` + 本工程改动 | **GPL v3** | 见 `LICENSE`。改动：24-bit 无损 MLP 支持、FFmpeg 9 适配、菜单/静图多项修复、Windows 移植 |
| **dvdauthor/spumux C subset** (dvda-menu-nav.dll / dvda-menu-spu.dll) | 0.7.1 + AMGM/project changes | **GPL v2 or later** (source headers) | tools/menu-native/vendor; ORIGIN.json and COPYING; NOTICE-Menu.txt |

主程序均以 GPL 发布；本包随附完整许可证文本（`LICENSE`）。
源码可从上游 `github.com/fabnicol/dvda-author` 获取，本工程的改动集
为一份可 `git apply` 的补丁。

## 随包工具（二进制再分发）

| 组件 | 版本 | 许可 |
|---|---|---|
| **ImageMagick**（`image-native/dvda-image.dll`） | 7.0.8-47 Q16 HDRI | **ImageMagick License**（Apache 2.0 风格） |
| **FFmpeg 动态库**（menu-bin） | 9.0.2，媒体和菜单共用的 Windows x64 源码构建 | **GPL v3 或更高版本**（本构建配置） |
| 其余 DLL | 各自的 MSYS2/MinGW-w64 构建 | 见各项目 |

## FFmpeg 构建

FFmpeg 库来自未经修改的 `https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz`。源码 SHA-256：`8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e`。shared 配置一次构建 MLP 制盘与 MPEG-2/MP2 菜单编码、DVD MPEG-PS 复用，以及 FLAC/ALAC/AAC/PCM/MLP 解码、FLAC/PCM 输出、SWR/SOXR 重采样和 MPEG-2/PNG/JPEG 处理。GUI 的媒体 C 接口和 author 都链接同一安装前缀；发布版将所需 DLL 统一放在 menu-bin，每个文件只保留一份。MLP 编码核心仍是独立组件。发布包不含 FFmpeg 命令行程序。

构建脚本为本仓库 `tools/win-build/build-minimal-ffmpeg.py`（默认发布使用 shared 配置，保留 mlp / menu / media 对照配置）与 `build-media-bridge.py`；本地构建目录中的 media-build.json 记录配置、源文件与所有 DLL 的哈希及导入依赖。随包还有 libsoxr、zlib 及其运行库；不包含 FFmpeg / FFprobe 命令行程序。本项目的 dvda-media.dll 接口源码位于 tools/win-build/native/dvda-media.c，遵循项目 GPL v3 许可。配置及验证见 docs/MINIMAL-FFMPEG.md 和 docs/INPROCESS-MEDIA.md。


## 字体

| 文件 | 许可 |
|---|---|
| `menu-bin/fonts/DvdaNotoCJK-Regular.ttc`（SC / JP / KR 三个 face） | **SIL Open Font License 1.1**（Google Noto Sans CJK） |

SC / JP / KR 三个 face 的相同 OpenType 表共享存储于一个字体集合中，全部字形、字符映射和区域差异均保留。`image-native/type.xml` 用明确的 face 索引分别注册三个字体名称；ImageMagick 按名称加载，避免将所有语言都渲染为 face 0。

**为什么保留三个 face**：不是「缺字」问题，而是**字形选择**问题 —— 各 face 都含
中文/日文/韩文/拉丁，但**同一批汉字**有区域性变体字形（直/骨/令/次/别 …）。
菜单按每条文字所属语言自动选 face（中文→SC、日文→JP、韩文→KR）。

OFL 允许随软件再分发与嵌入。**不要**把字体单独作为商品出售。

## 本工程自身程序

`DVD-Audio-Maker.exe`、可选开发包中的 `dvda.exe` 及其 C# 源码：**GPL v3**（与 dvda-author 一致）。
发布包不再包含或调用 Python 业务脚本。

---

## 关于 GPL 合规

本包分发的是可执行文件。若你要**再分发**本包或其修改版：

- 保留 `LICENSE` 与本文档；
- 同时提供对应源码（上游 + 本工程改动补丁）。GPL v3 第 6 节允许以
  「提供书面要约」方式替代随附源码，但最省事的做法是同时给出源码地址。

## 按需构建的图像库

图像 DLL 静态包含所需的 ImageMagick 核心、FreeType 轮廓字体、JPEG/PNG 编解码、WebP 解码与 zlib。JPEG/PNG 支持读写；WebP 仅用于读取封面。外部委托和动态 coder 模块已禁用。桥接源码为 tools/win-build/native/dvda-image.c、author-image-loader.c；构建脚本为 build-image-runtime.py、build-image-bridge.py、build-image-author.py。本地构建目录中的 image-build.json 和 author-build.json 记录来源及源码补丁哈希；具体组件许可文本随 NOTICE-Image.txt 提供。
