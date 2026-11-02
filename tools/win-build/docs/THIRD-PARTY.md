# 第三方组件与许可证

发布包包含本项目的 Rust 实现及所需开源库。许可证和来源声明位于 ZIP
根目录的 `LICENSE`、`NOTICE-Image.txt` 和 `NOTICE-Menu.txt`。

| 组件 | 用途 | 许可证/来源 |
|---|---|---|
| 内置 Rust author 与 ISO writer | DVD-Audio IFO/AOB、菜单编排及 ISO9660/UDF 镜像 | 项目 GPL v3；迁移自 `dvda-author` 的规则保留上游归属 |
| 内置 Rust 菜单实现 | DVD-Video 导航、子图像及菜单会话/资源管理；行为与规则衍生自 dvdauthor/spumux，归属保留于 `NOTICE-Menu.txt` | 项目 GPL v3；dvdauthor/spumux 归属 |
| 内置 Rust 媒体适配层与 FFmpeg 库 | 解码、重采样、PCM 校验和菜单 MPEG 生成 | 项目 GPL v3；FFmpeg 及其依赖遵循各自上游许可证 |
| 内置 Rust 图像链路（`image`、`rustybuzz`、`swash`） | 图像处理、菜单绘制和字体渲染 | 项目 GPL v3；上游 MIT/Apache-2.0 可选许可及 BSD、Zlib、Unicode 条款，完整声明见 `NOTICE-Image.txt` |
| 内置 Rust 格式解析与成品校验 | IFO/AOB、MLP/LPCM 和镜像验证 | 项目 GPL v3；LPCM 规则保留 `dvda-author` 归属 |
| 内置 Rust MLP 编码器 | MLP 编码 | 项目 GPL v3 |
| `DvdaNotoCJK-Regular.ttc` | 中日韩菜单字体 | SIL Open Font License 1.1 |

用户使用单文件 Rust GUI。author、项目适配层和 MLP 编码器静态集成在程序中，
Rust 图像库、FFmpeg 及其依赖静态链接；字体和菜单数据随程序内嵌。旧项目 C author、适配 DLL 和 C17
编码器已移除。ImageMagick 和 FreeType 不再是图像构建或运行时依赖；显式提供的历史图像 oracle
仅用于可选测试，不随发布包分发。发布包不含
`ffmpeg.exe`、`ffprobe.exe`、ImageMagick 命令行程序、eac3to 或 参考编码器。

构建脚本、源码哈希和完整来源记录保留在仓库与本地 `build` 目录。退役 author
源码的版权与哈希记录位于
`rust/crates/dvda-author/tests/fixtures/legacy-source-provenance.json`，原源码可从 Git
历史恢复。发布或修改软件时，请保留许可证及 NOTICE 文件，并按对应上游许可证
提供源码或补丁。构建边界见 [Windows 构建说明](../README.md)；原生运行时迁移记录只保存在本地源码工作副本中。
