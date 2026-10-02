# dvda-author 修改源码镜像

[简体中文](README.md) | [English](README.en.md)

本目录保存 DVD-Audio Maker 对 `dvda-author` 修改过的核心 C/C++ 源文件，主要用于：

- 在仓库中审阅和搜索修改后的实现。
- 记录 MLP、时间轴、菜单、字体和 Windows 兼容性改动。
- 与完整工作树或补丁文件进行差异核对。
- 避免把完整上游源码、第三方依赖和编译产物提交到主仓库。

## 重要说明

**这里不是完整、可独立构建的 dvda-author 源码树。**

本目录通常只包含：

```text
src/          修改过的 dvda-author 核心源码
libutils/     修改过的公共工具源码
MIRROR-NOTES.md
README.md
```

它不包含完整构建所需的内容，例如：

```text
configure
configure.ac
Makefile.in
libfixwav/
menu/
m4.extra.dvdauthor/
dvdauthor-0.7.1/
local.w10/
```

因此不要在本目录运行 `configure`、`make` 或 Windows 工具链脚本。

## 权威来源

构建时应使用完整的 `dvda-author` 工作树，并应用本项目改动。推荐固定到上游提交 `8fca43a`：

```bash
git clone https://github.com/fabnicol/dvda-author /path/to/dvda-author
cd /path/to/dvda-author
git checkout 8fca43a
git apply /path/to/DVD-Audio-Maker/docs/dvda-author-changes.patch
```

完整改动集：

- [`../../docs/dvda-author-changes.patch`](../../docs/dvda-author-changes.patch)

改动说明及依据：

- [`../../docs/DVDA-AUTHOR-CHANGES.md`](../../docs/DVDA-AUTHOR-CHANGES.md)

试验过但未启用的改动：

- [`../../docs/DVDA-AUTHOR-DISABLED.md`](../../docs/DVDA-AUTHOR-DISABLED.md)

实际构建入口：

- Linux/WSL：[`../../build_dvda_author_mlp.sh`](../../build_dvda_author_mlp.sh)
- Windows/MSYS2：[`../win-build/README.md`](../win-build/README.md)

## 主要改动范围

镜像中的代码可能包括以下类别的修改：

- 24-bit MLP 输入和 FFmpeg 新版 API 适配。
- MLP 帧、轨道边界和字节对齐处理。
- ATSI、AOB、PTS 和标题时间轴修复。
- 菜单生成、AMG/ASVS 关联和播放静图处理。
- SC、JP、KR 字体按文本语言分派。
- Windows/MinGW 路径、进程、管道和 UTF-8 兼容性。
- 上游崩溃、缓冲区限制和资源清理问题的修复。

具体内容以补丁和完整工作树为准，不应仅根据此目录推断全部变更。

## 同步规则

当完整工作树中的修改发生变化时：

1. 更新完整工作树并确认能编译、测试。
2. 重新生成 `docs/dvda-author-changes.patch`。
3. 将修改过的 `src/`、`libutils/` 文件同步到本镜像。
4. 比较镜像、补丁应用结果与完整工作树，确保对应文件逐字节一致。
5. 不要同步对象文件、可执行文件、生成的 Makefile、第三方库或大型素材。

镜像是审阅副本，不应反向覆盖未经核对的完整工作树。

## 许可

本目录中的文件来源于 `dvda-author`，并包含本项目的修改。其许可和第三方说明见：

- [`../../LICENSE`](../../LICENSE)
- [`../../docs/LICENSING.md`](../../docs/LICENSING.md)

使用、修改或重新分发时，需要同时遵守上游项目及其依赖的许可条款。
