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


## SHA-256 后端回归与基准

生产 SHA-256 固定使用 `sha2 = 0.11.0`，x86/x64 在运行时检测到 `sha`、`sse2`、`ssse3`、`sse4.1` 时使用 SHA-NI，否则回退可移植实现；构建不强制 `target-cpu=native`。不再保留手写 SHA-256 实现。回归测试保留标准向量，并将封装层的一次性、分块和 reader 摘要与上游 `sha2` API 比较。

```powershell
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --release --offline -p dvda-core --lib hash:: -- --skip release_files_benchmark
```

只读 Release 文件基准通过 `DVDA_HASH_BENCH_INPUTS` 接收 JSON 文件数组，并将新 JSON 写入 `DVDA_HASH_BENCH_REPORT`。它对真实 DLL、MLP 缓存样本和发布 ZIP 测量 sha2 内存及 warm-file 摘要，采用七轮中位数；新报告仅包含 sha2 耗时，已有前后对比报告作为历史记录保留；结果只表示摘要路径，不代表冷盘或完整制盘耗时。报告保存在被 Git 忽略的 `build` 目录。`sha2` 及其传递依赖沿用上游 MIT 或 Apache-2.0 许可证。


缓存复用仍须通过源/输出指纹、编码组件身份、目标参数与完整 MLP CRC/奇偶校验；
参数缓存未知或缺失时重新读取，不跳过完整扫描。自动检查线程数为
`min(可用核心数, 16, 曲目数)`，取消或异常时不保存本轮参数缓存。

```powershell
python tools\win-build\build-formats-runtime.py --msys-root C:\msys64 --output build\formats-native
python tools\win-build\test-formats-optimization.py --before C:\previous\dvda-formats.dll --after build\formats-native\dvda-formats.dll --samples C:\samples\mlp --report build\formats-report.json
$env:DVDA_FORMATS_NATIVE_DIR = (Resolve-Path build\formats-native).Path
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-native -- --include-ignored
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-core --lib mlp_workflow -- --include-ignored --skip real_cached --skip another_volume
```

对照脚本需要旧 DLL 与真实 MLP 样本，比较完整扫描 ABI 结果、CRC 边界与对齐输出。
只读真实缓存计时测试 `real_cached_mlp_preflight_benchmark_is_read_only` 需要设置
`DVDA_MLP_PREFLIGHT_MANIFEST`、`ROOT`、`OUTPUT`、`MEDIA`、`ENCODER` 和 `REPORT`
（后五项均带相同的 `DVDA_MLP_PREFLIGHT_` 前缀）。媒体/编码 DLL 必须与缓存身份一致；
任一未命中都会失败，不会编码或覆盖样本。测试计时不能替代完整制盘与播放验证。
