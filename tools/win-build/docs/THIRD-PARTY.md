# 组件与许可（THIRD-PARTY）

本包是若干开源程序的再分发（含针对本用途的修改）。清单如下。

## 主程序

| 组件 | 版本 | 许可 | 说明 |
|---|---|---|---|
| **dvda-author** (`menu-bin/dvda-author-dev.exe`) | 上游 `8fca43a` + 本工程改动 | **GPL v3** | 见 `LICENSE`。改动：24-bit 无损 MLP 支持、FFmpeg 9 适配、菜单/静图多项修复、Windows 移植 |
| **dvdauthor** (`menu-bin/dvdauthor.exe`) | 0.7.1 | **GPL v2** | 含 AMGM 菜单与 `jump group` 补丁（上游版本不认菜单跳转语法） |
| **spumux** / **spuunmux** | 0.7.1 | **GPL v2** | 与 dvdauthor 同一份源码 |

主程序均以 GPL 发布；本包随附完整许可证文本（`LICENSE`）。
源码可从上游 `github.com/fabnicol/dvda-author` 获取，本工程的改动集
为一份可 `git apply` 的补丁。

## 随包工具（二进制再分发）

| 组件 | 版本 | 许可 |
|---|---|---|
| **cdrtools / mkisofs** (`mkisofs.exe`) | 3.02a | **CDDL**（mkisofs 部分） |
| **mjpegtools** (`jpeg2yuv` / `mpeg2enc` / `mplex` / `mp2enc`) | 2.1.0 | **GPL v2** |
| **ImageMagick** (`magick` / `convert` / `mogrify` + 11 个 `.xml`) | 7.0.8-47 Q16 | **ImageMagick License**（Apache 2.0 风格） |
| 其余 DLL | 各自的 MSYS2/MinGW-w64 构建 | 见各项目 |

## 字体

| 文件 | 许可 |
|---|---|
| `menu-bin/fonts/NotoSansCJKsc-Regular.otf` | **SIL Open Font License 1.1**（Google Noto Sans CJK） |
| `menu-bin/fonts/NotoSansCJKjp-Regular.otf` | 同上 |
| `menu-bin/fonts/NotoSansCJKkr-Regular.otf` | 同上 |

这三个文件是从 `NotoSansCJK-Regular.ttc` 中**抽出的单 face**（SC / JP / KR），
供 ImageMagick 按文件路径加载（见 `make-menu-font.sh`）。

**为什么要三个**：不是「缺字」问题，而是**字形选择**问题 —— 各 face 都含
中文/日文/韩文/拉丁，但**同一批汉字**有区域性变体字形（直/骨/令/次/别 …）。
菜单按每条文字所属语言自动选 face（中文→SC、日文→JP、韩文→KR）。

OFL 允许随软件再分发与嵌入。**不要**把字体单独作为商品出售。

## 本工程自身的脚本

`scripts/` 下的 Python 脚本：**GPL v3**（与 dvda-author 一致）。

---

## 关于 GPL 合规

本包分发的是可执行文件。若你要**再分发**本包或其修改版：

- 保留 `LICENSE` 与本文档；
- 同时提供对应源码（上游 + 本工程改动补丁）。GPL v3 第 6 节允许以
  「提供书面要约」方式替代随附源码，但最省事的做法是同时给出源码地址。
