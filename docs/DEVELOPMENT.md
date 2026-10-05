# 开发和调试

当前应用是 Windows x64 Rust workspace。用户发布包只有 GUI；开发 CLI 保留在源码中。媒体、图像、格式、校验和 MLP 组件均通过项目构建的原生 DLL 调用。

## 源码入口

在仓库根目录运行：

```bat
gui-debug.cmd
cli.cmd abi.version
cli.cmd prepare --profile "C:\work\settings.json"
cli.cmd build --dry-run --profile "C:\work\settings.json"
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline
```

`settings.json` 是唯一的文件配置格式。`settings.local.json` 适合保存本机路径，已被 Git 忽略；也可以通过 `--profile` 使用隔离方案。`config.env`、`DVDA_CONFIG` 和 `--config` 不受支持。

GUI 只提供检查音源、制作光盘和验证成品。dry-run 仅属于开发 CLI 诊断入口，不显示在 GUI 中。

## 原生组件

按 [Windows 构建说明](../tools/win-build/README.md) 准备源码构建的组件。源码启动脚本通过 rust-dev-env.cmd 设置 build 下的原生库默认位置，已有环境覆盖优先；正式用户包的组件内嵌在 EXE 中。开发测试可以用 `DVDA_MEDIA_NATIVE_DIR`、`DVDA_IMAGE_NATIVE_DIR`、`DVDA_FORMATS_NATIVE_DIR` 和 `DVDA_ENCODER_LIBRARY` 明确指定组件；缺少组件时程序直接报错。

MLP C17 核心位于 `native/mlp-encoder`，格式 C17 DLL 源码位于 `tools/formats-native`。原生调试应使用对应的 MinGW 符号和独立调试器，不要为了调试改变正式编码器的浮点选项。

## VS Code

使用 Rust Analyzer 和 CodeLLDB（或 WinDbg）调试 `dvda-desktop`、`dvda-cli`。`.vscode/tasks.json` 提供 workspace 构建、测试、release 构建和打包任务；`.vscode/launch.json` 提供 GUI/CLI x64 调试入口。

## 发布

```powershell
cargo build --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --release --workspace --offline
dvda-toolchain.exe package --repo . --output build/release --media-runtime build/media-native-shared --image-runtime build/image-native --image-author build/rust-author-current --prebuilt build/release-menu-final --version v1.0
```

发布包 `DVD-Audio-Maker-v1.0-win-x64.zip` 不包含 .NET runtime、开发 CLI、PDB、构建来源 JSON、`config.env` 或个人 profile。构建产物留在被 Git 忽略的 `build` 目录。

## 验收

```powershell
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File tools/win-build/test-rust-workflow.ps1
```

菜单 fixture 会生成菜单、索引页、静图、AUDIO_TS 和 ISO，并调用项目构建的 author。MLP 的逐字节样本对照、PCM 比较和 C17 ABI 验证记录在各组件文档中。
