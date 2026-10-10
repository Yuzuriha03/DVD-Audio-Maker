# 第三方组件与许可证

发布包包含本项目的 Rust 实现及所需开源库。许可证和来源声明位于 ZIP
根目录的 `LICENSE`、`NOTICE-Image.txt` 和 `NOTICE-Menu.txt`。

| 组件 | 用途 | 许可证/来源 |
|---|---|---|
| 内置 Rust author 与 ISO writer | DVD-Audio IFO/AOB、菜单编排及 ISO9660/UDF 镜像 | 项目 GPL v3；迁移自 `dvda-author` 的规则保留上游归属 |
| 内置 Rust 菜单会话与资源管理 | 菜单生命周期、失败恢复和状态重置 | 项目 GPL v3 |
| 静态链接的 dvdauthor/spumux 菜单库 | DVD-Video 导航和子图像 | GPL v2 或更高版本，来自 `tools/menu-native/vendor`；保留少量 C 错误处理与可变参数边界 |
| 内置 Rust 媒体适配层与 FFmpeg 库 | 解码、重采样、PCM 校验和菜单 MPEG 生成 | 项目 GPL v3；FFmpeg 及其依赖遵循各自上游许可证 |
| 内置 Rust 图像适配层与 ImageMagick/FreeType | 图像处理、菜单绘制和字体渲染 | 项目 GPL v3 及 ImageMagick、FreeType 等上游许可证 |
| 内置 Rust 格式解析与成品校验 | IFO/AOB、MLP/LPCM 和镜像验证 | 项目 GPL v3；LPCM 规则保留 `dvda-author` 归属 |
| 内置 Rust MLP 编码器 | MLP 编码 | 项目 GPL v3 |
| `DvdaNotoCJK-Regular.ttc` | 中日韩菜单字体 | SIL Open Font License 1.1 |

用户使用单文件 Rust GUI。author、项目适配层和 MLP 编码器静态集成在程序中，
所需第三方 DLL、字体和菜单数据随程序内嵌。旧项目 C author、适配 DLL 和 C17
编码器已移除。FFmpeg、ImageMagick 及其他库只启用工作流所需功能；发布包不含
`ffmpeg.exe`、`ffprobe.exe`、ImageMagick 命令行程序、eac3to 或 SurCode。

构建脚本、源码哈希和完整来源记录保留在仓库与本地 `build` 目录。退役 author
源码的版权与哈希记录位于
`rust/crates/dvda-author/tests/fixtures/legacy-source-provenance.json`，原源码可从 Git
历史恢复。发布或修改软件时，请保留许可证及 NOTICE 文件，并按对应上游许可证
提供源码或补丁。构建边界见 [Windows 构建说明](../README.md) 和
[原生运行时迁移](../../../docs/NO-EXTERNAL-RUNTIME-MIGRATION.md)。
