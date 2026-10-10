# 许可与法律说明

[简体中文](LICENSING.md) | [English](LICENSING.en.md)

> 本文包含项目早期架构与分发说明。当前仓库和发布包组成以[项目说明](../README.md)及[第三方声明](../tools/win-build/docs/THIRD-PARTY.md)为准；历史流程描述不代表当前运行依赖。

本文档说明本仓库的许可状况、第三方组件归属，以及使用 MLP 编码时需注意的法律限制。

---

## 1. 本仓库的许可：GPL-3.0

**整个仓库以 [GPL-3.0](https://www.gnu.org/licenses/gpl-3.0.html) 发布**，全文见根目录 `LICENSE`。

### 为什么必须是 GPL-3.0

本工程的 author 与 LPCM 打包规则由修改过的
[dvda-author](https://github.com/fabnicol/dvda-author) 实现迁移到 Rust，
继续保留上游归属和 GPL 许可。当前仓库包含：

- `rust/crates/dvda-author`：Rust author、IFO/AOB 与 ISO writer
- `docs/RUST-AUTHOR-MIGRATION.md`：实现范围与验收记录
- `rust/crates/dvda-author/tests/fixtures/legacy-source-provenance.json`：退役源码的
  Git 版本、文件哈希、版权声明和黄金数据来源
- `docs/DVDA-AUTHOR-CHANGES.md` / `DVDA-AUTHOR-DISABLED.md`：历史改动说明

旧 C 镜像和 `docs/dvda-author-changes.patch` 已移除，可以从本仓库提交
`6c5086127590001c544373783653fe991f0ebaeb` 恢复。源码清理不改变许可和归属要求。

而 dvda-author 的许可状况：

| 项目 | 值 |
|------|-----|
| 仓库 `COPYING` 文件 | **GNU GPL version 3**（2007-06-29） |
| 多数源码文件头部 | "GNU General Public License ... either **version 2** of the License, or (at your option) any later version" |
| 程序内声明文本 | "either **version 3** of the License, or (at your option) any later version" |

源码头部写的是 "v2 or later"，因此**升级到 GPL-3.0 是被允许的**；
加之项目本身就分发 GPL-3.0 的 `COPYING`，所以 **GPL-3.0 是准确且安全的选择**。

### 本仓库自身程序的说明

当前 GUI、CLI、author、媒体和图像适配层、MLP 编码器均由 Rust 实现，
仓库统一采用 GPL-3.0。旧 C# 和 Python 入口保留在 Git 历史中；当前发布包
使用进程内 Rust 实现及所需第三方库，组成见第三方声明。

---

## 2. 第三方组件归属

### dvda-author

```
Copyright Dave Chapman 2005
Copyright Fabrice Nicol 2007-2019
Copyright Lee and Tim Feldkamp 2008-2009
License: GPL-3.0
```

上游代码的版权归原作者所有。本仓库的 Rust 移植保留对应归属、黄金数据和源码来源记录。

### FFmpeg

本节早期流程使用系统 FFmpeg（实测 8.0.1）完成解码与 MLP 编码。
当前流程使用 FFmpeg 库解码，由项目 Rust 编码器生成 MLP：

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

---

## 3. MLP 编码的法律提示

**这一点需要特别注意。**

MLP（Meridian Lossless Packing）是 **Dolby** 拥有的技术。FFmpeg 的 MLP 编码器
被标记为 **experimental（实验性）** —— 这正是必须加 `-strict -2` 才能使用的原因：

```bash
ffmpeg -i input.flac -c:a mlp -strict -2 output.mlp
#                          ^^^^^^^^^^^^ 不加会报 "Experimental feature"
```

dvda-author 自身的帮助文本也明确写有：

> `--encode` Use this option to encode to MLP audio.
> This option is based on the ffmpeg encoder and **subject to the same legal
> restrictions as those applying to the MLP ffmpeg encoder**.

上述说明针对历史 FFmpeg CLI 预编码流程；当前项目使用 Rust MLP 编码器。
实现语言的变化不替代使用者对适用规则的判断。

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
