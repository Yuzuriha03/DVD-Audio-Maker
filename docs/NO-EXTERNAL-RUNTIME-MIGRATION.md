# 原生运行时迁移

Windows x64 工作流现在由 Rust 应用和项目构建的原生组件完成。GUI 不会调用 FFmpeg、FFprobe、ImageMagick、Metaflac、eac3to、SurCode、dvdauthor 或 mkisofs 的命令行程序。

## 已完成的边界

| 区域 | 原生实现 |
|---|---|
| 音源媒体 | `dvda-media.dll`，负责探测、解码、PCM/FLAC 输出和 ALAC 检查 |
| 图像 | `dvda-image.dll`，负责所需图像转换 |
| MLP | 通过流式 C ABI 调用 `mlp_encoder.dll` |
| DVD 菜单 | `dvda-menu-spu.dll`、`dvda-menu-nav.dll` 和项目构建的 author |
| ISO 写入 | author 内置 ISO 写入器 |
| 成品校验 | Rust ISO 读取、AOB 流式 MLP/LPCM 比较和 PCM 校验 |
| 配置 | 仅使用版本化 JSON 方案 |

发布 ZIP 是 Windows x64 原生 GUI 包。必要 DLL、菜单资源、字体、许可证和用户文档随包提供。组件缺失时报告错误，不替换为外部程序。

## MLP 逐字节规则

MLP 编码器在序列化前接收目标 PCM、格式参数和辅助元数据，序列化后不修改编码字节。因此，只有 PCM、参数、上下文和编码行为都相同，参考文件才可能逐字节一致。已有 MLP 可以只读导入；该入口用于导入原版 SurCode MLP 编码生成的文件。

## 明确排除

仓库仍保留用于生成原生 DLL 的第三方库源码、构建脚本和来源记录。Python、MSYS2/MinGW-w64 GCC 等是开发构建前置条件，不是应用运行时依赖。通用 DVD-Video、字幕制作和未使用的命令行工具不属于本工作流。

旧 C# 项目、Rust 到 C# 的兼容桥和 `config.env` 解析器已删除。程序不会执行旧的 `DVDA_MKISOFS`、`DVDA_METAFLAC`、eac3to 或媒体可执行文件路径设置。

## 验收命令

在仓库根目录运行：

```powershell
cargo fmt --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- --include-ignored
```

菜单样本和原生 ABI 测试覆盖 author、菜单资源、ISO 写入、PCM 比较及失败处理。实体播放器播放仍需人工确认，不由自动化测试代替。
