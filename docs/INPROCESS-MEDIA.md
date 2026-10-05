# 进程内媒体处理

Rust GUI 通过项目构建的 x64 `dvda-media.dll` 完成音源探测、解码、PCM/FLAC 输出、ALAC 检查、封面处理和成品校验。运行时不会启动 `ffmpeg.exe` 或 `ffprobe.exe`，用户也不需要另行安装 FFmpeg。

媒体 DLL 由所需的 FFmpeg 库源码构建，随发布包放在 GUI 旁边并由打包工具校验。缺少 DLL 时直接报告错误，不搜索 PATH，也不静默回退到外部程序。ImageMagick、Metaflac、eac3to 和原版 SurCode 采用相同规则。

## 配置和运行时

程序只使用版本化 JSON 配置方案。默认文件为 `%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`，可用 `--profile` 指定其他 JSON 文件。`config.env` 和旧的可执行文件路径配置不再读取。开发测试可以通过明确的环境变量选择已经验证的原生组件目录；发布包始终使用旁边的 DLL。

媒体运行时只包含本流程需要的 `avcodec`、`avformat`、`avutil`、`swresample`、`swscale`、`libsoxr`、zlib 及必要的 x64 运行库。MLP 编码器和 DVD 菜单组件独立放置。媒体库身份参与缓存键，替换原生构建后旧准备缓存会自动失效。

ALAC 转 FLAC 的元数据整理使用进程内编辑器。临时输出先写在目标旁边，验证成功后才原子发布；失败或取消会删除临时文件。MLP 编码结果不会在序列化后打补丁。

## 构建和验证

原生构建输入见 [Windows 构建说明](../tools/win-build/README.md)。设置已经验证的原生目录后运行 Rust 测试：

```powershell
$env:DVDA_MEDIA_NATIVE_DIR = (Resolve-Path build/media-native).Path
cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- --include-ignored
```

集成样本覆盖 Unicode 路径和标签、ALAC、FLAC 封面逐字节保留、PCM 哈希、无效输入、取消、并行解码和菜单帧提取。开发阶段可以使用参考 FFmpeg 工具生成样本，但发布包不依赖这些外部程序。
