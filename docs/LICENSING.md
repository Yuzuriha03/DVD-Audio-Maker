# 许可与法律说明

本文档说明本仓库的许可状况、第三方组件归属，以及使用 MLP 编码时需注意的法律限制。

---

## 1. 本仓库的许可：GPL-3.0

**整个仓库以 [GPL-3.0](https://www.gnu.org/licenses/gpl-3.0.html) 发布**，全文见根目录 `LICENSE`。

### 为什么必须是 GPL-3.0

`patches/` 与 `fixes/` 是对 [dvda-author](https://github.com/fabnicol/dvda-author)
源码的**修改**，属于衍生作品，需与原项目许可保持一致。其中：

- `patches/patch_read.py`、`patch_encode.py` 等**内联引用了 dvda-author 的原始代码**（作为替换文本）
- `fixes/*.py` 直接改写 `winport.h`、`libsoxconvert.c` 等文件

而 dvda-author 的许可状况：

| 项目 | 值 |
|------|-----|
| 仓库 `COPYING` 文件 | **GNU GPL version 3**（2007-06-29） |
| 多数源码文件头部 | "GNU General Public License ... either **version 2** of the License, or (at your option) any later version" |
| 程序内声明文本 | "either **version 3** of the License, or (at your option) any later version" |

源码头部写的是 "v2 or later"，因此**升级到 GPL-3.0 是被允许的**；
加之项目本身就分发 GPL-3.0 的 `COPYING`，所以 **GPL-3.0 是准确且安全的选择**。

### 本仓库自身脚本的说明

`01_prepare.py`、`02_build.py`、`verify.sh` 等脚本通过**子进程方式调用** `dvda-author`，
不与其链接，按 GPL 的通常理解不构成衍生作品，本可单独采用其他许可。
但为保持仓库整体清晰、避免混用许可带来的困扰，**统一采用 GPL-3.0**。

---

## 2. 第三方组件归属

### dvda-author

```
Copyright Dave Chapman 2005
Copyright Fabrice Nicol 2007-2019
Copyright Lee and Tim Feldkamp 2008-2009
License: GPL-3.0
```

`patches/` 中所有补丁均基于上述代码，版权归原作者所有。本仓库仅提供修改补丁。

### FFmpeg

本工具链通过系统 FFmpeg（实测 8.0.1）完成解码与 MLP 编码：

| 情况 | 许可 |
|------|------|
| FFmpeg 默认构建 | **LGPL v2.1 or later** |
| Ubuntu/Debian 并启用 `--enable-gpl` | 部分文件为 **GPL v2 or later**，整体需按 GPL 对待 |
| **MLP 编解码器**（`mlpdec.c` / `mlpenc.c`） | **LGPL v2.1 or later** |

链接关系检查：

- dvda-author（GPL-3.0）+ FFmpeg 的 GPL-2.0-or-later 部分 → 兼容（"or later" 允许升级至 GPL-3.0）
- MLP 编解码器本身是 LGPL-2.1+ → 与 GPL-3.0 兼容

若你希望完全避开 GPL 部分，可自行编译一个 **LGPL-only** 的 FFmpeg
（不加 `--enable-gpl`、`--enable-nonfree`），本工具链只需 `libavcodec`、
`libavformat`、`libavutil`、`libswresample` 与 MLP 编解码器。

### mkisofs（cdrtools）

`mkisofs -dvd-audio` 由 Jörg Schilling 为 cdrtools 提供的补丁实现，采用 **CDDL**
（Common Development and Distribution License）。本仓库**不包含**该二进制，
仅通过命令行调用，使用前请自行获取并遵守其许可。

---

## 3. MLP 编码的法律提示

**这一点需要特别注意。**

MLP（Meridian Lossless Packing）是 **Dolby** 拥有的技术。FFmpeg 的 MLP 编码器
被标记为 **experimental（实验性）** —— 这正是必须加 `-strict -2` 才能使用的原因：

```bash
ffmpeg -i input.wav -c:a mlp -strict -2 output.mlp
#                         ^^^^^^^^^^^^ 不加会报 "Experimental feature"
```

dvda-author 自身的帮助文本也明确写有：

> `--encode` Use this option to encode to MLP audio.
> This option is based on the ffmpeg encoder and **subject to the same legal
> restrictions as those applying to the MLP ffmpeg encoder**.

本工具链虽然**不走 dvda-author 的 `--encode`**（改为先用 ffmpeg CLI 预编码 MLP 文件，
再交给 dvda-author 打包），但**用的是同一个 MLP 编码器**，因此上述限制同样适用。

**实务提示：**

- 请自行确认你所在司法辖区对 MLP 编码的使用规定
- 保留源文件（本流程不修改音源），必要时可用 LPCM 方案规避
- 本仓库仅提供技术实现，不对使用者的法律合规性作任何担保

---

## 4. 关于音频内容

**本仓库不包含任何音频、音乐或封面文件。**

工具链处理的音源由使用者自行准备。请确保你拥有相应内容的合法使用权。
本工具链不包含任何解密或绕过版权保护的功能 —— 若源文件本身损坏
（例如不完整的受保护下载），流水线会在解码校验阶段如实报告并中止，而不会尝试修复。

---

## 5. 免责声明

本软件按 GPL-3.0 的条款分发，**不提供任何担保**。作者不对使用本工具链
所产生的任何后果负责，包括但不限于光盘制作失败、数据损失或法律纠纷。
