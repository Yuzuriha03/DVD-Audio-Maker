# 旧 C 实现清理（2026-10-11）

Rust author、ISO writer、媒体/图像适配层、MLP 编码器和菜单会话完成迁移后，
本次移除旧实现及其构建产物，并使默认构建和回归测试脱离冻结 C DLL。

- 移除 `tools/dvda-author-mlp8` 源码镜像、旧 author patch、C oracle harness、
  项目 C 媒体/图像 helper，以及已迁移的 C17 MLP 源码、DLL 和构建脚本。
- 清理工作区内 2,230 个旧产物文件，合计 588,901,307 字节（约 562 MiB）。
  包括旧构建目录中的文件、缓存 DLL，以及已验证内嵌旧 C 组件的 GUI/ZIP。
  删除前逐项核对工作区绝对路径、文件长度和 SHA256；保留空目录及审计摘要。
- 保留 28 个 IFO/CSV 黄金数据及其来源记录，保留当前 Rust 产物和第三方依赖。
  66 个退役 author 源文件的 Git blob、SHA256 与版权记录见
  `rust/crates/dvda-author/tests/fixtures/legacy-source-provenance.json`。
  完整旧源码及 patch 可从提交 `6c5086127590001c544373783653fe991f0ebaeb` 恢复。

默认 MLP 测试为 17 项 Rust 测试；四项历史 C 差分测试仅通过
`external-oracle` feature 和显式外部 DLL 启用。格式/成品校验的可选 oracle
同样要求显式路径。素材默认目录为 `tools/win-build/prebuilt`，缺失素材明确报错。

仍保留第三方 dvdauthor/spumux 的 C 实现、少量 `setjmp`/可变参数兼容边界，
以及编译时 FFmpeg/ImageMagick ABI 布局探针。它们不属于退役的自有业务实现。

实际 LPCM 回归矩阵发现 Windows 临时目录发布偶发错误。Rust author 现仅对
Windows 错误 5/32/33 重试原子目录重命名，等待上限一秒，每次检查取消状态；
失败保留原有回滚语义。真实目录句柄测试覆盖锁释放后的恢复与等待中取消，
完整制盘测试继续检查已有 ISO 保留、回滚和重试。

本地清理清单保存在 `build/legacy-artifact-cleanup.json` 与
`build/legacy-binary-residue-cleanup.json`；构建、完整制盘、菜单会话、工作区测试
及单文件产品流程的终验记录同样保留在 `build`。构建和验收命令见
[开发说明](DEVELOPMENT.md)、[Rust author](RUST-AUTHOR-MIGRATION.md) 和
[单文件发布](ONEFILE-PUBLISH.md)。
