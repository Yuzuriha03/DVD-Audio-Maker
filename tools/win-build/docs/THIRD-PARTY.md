# 第三方组件与许可证

发布包包含若干开源项目的必要运行时组件及本项目修改。用户包的许可证和来源说明位于 ZIP 根目录的 `LICENSE`、`NOTICE-Image.txt` 和 `NOTICE-Menu.txt`。

| 组件 | 用途 | 许可证/来源 |
|---|---|---|
| `dvda-author-dev.exe` | DVD-Audio 菜单、导航和 ISO 写入 | GPL v3，基于上游 `dvda-author` 及项目补丁 |
| `dvda-menu-nav.dll`、`dvda-menu-spu.dll` | 菜单导航和子图像 | GPL v2 或更高版本，来自 `tools/menu-native/vendor` |
| `dvda-media.dll` | 进程内媒体处理 | 项目 GPL v3 C ABI，链接所需 FFmpeg 库 |
| `dvda-image.dll` | 进程内图像处理 | 项目桥接代码及 ImageMagick/FreeType 等上游许可证 |
| `mlp_encoder.dll` | MLP 编码 | 项目 C17 核心，GPL v3 |
| `DvdaNotoCJK-Regular.ttc` | 中日韩菜单字体 | SIL Open Font License 1.1 |

FFmpeg、ImageMagick 和其他库均由项目构建并只保留工作流需要的功能。发布包不含 `ffmpeg.exe`、`ffprobe.exe`、ImageMagick 命令行程序、eac3to 或 SurCode。构建脚本、源码哈希和完整来源记录留在仓库与本地 `build` 目录，不复制到用户包。

发布或修改软件时，请保留许可证和 NOTICE 文件，并按对应上游许可证提供源码或补丁。详细构建边界见 [Windows 构建说明](../README.md) 和 [原生运行时迁移](../../../docs/NO-EXTERNAL-RUNTIME-MIGRATION.md)。
