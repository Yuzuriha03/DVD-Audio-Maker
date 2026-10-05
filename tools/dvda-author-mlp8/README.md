# dvda-author 修改源码镜像

本目录保存 DVD-Audio Maker 对 `dvda-author` 的受控 C/C++ 修改，用于审阅补丁、记录来源和构建隔离快照。它不是完整的、可单独构建的上游源码树。

## 目录用途

```text
src/          已修改的 author 核心源码
libutils/     已修改的公共工具源码
MIRROR-NOTES.md
README.md
```

完整构建必须使用上游完整工作树并应用 `docs/dvda-author-changes.patch`。该镜像不应覆盖未经验证的工作树，也不包含对象文件、可执行文件、第三方库或大型资源。

## 当前集成

Windows x64 发布使用项目构建的 author、媒体和图像 DLL。菜单媒体、子图像、导航和 ISO 写入均通过进程内 C 模块完成；GUI 入口是 `DVD-Audio-Maker.exe`，依次执行检查音源、制作光盘和验证成品。dry-run 只保留在开发 CLI。

发布包为 `DVD-Audio-Maker-v1.0-win-x64.zip`，本维护镜像不会复制进用户包。构建输入、许可证和来源记录见 [Windows 构建说明](../win-build/README.md)、[第三方组件](../win-build/docs/THIRD-PARTY.md) 和 [原生运行时迁移](../../docs/NO-EXTERNAL-RUNTIME-MIGRATION.md)。

## 同步规则

1. 在完整上游工作树中构建并测试。
2. 重新生成 `docs/dvda-author-changes.patch`。
3. 只同步修改过的 `src/` 和 `libutils/` 文件，并逐字节比较镜像、补丁和完整工作树。
4. 不同步对象、可执行文件、生成 Makefile、第三方库和大型资源。

## 许可证

文件来源于 `dvda-author`，并包含本项目修改。使用、修改和再发布必须同时遵守上游项目及其依赖的许可证，保留仓库根目录的 `LICENSE` 和发布包 NOTICE 文件。
