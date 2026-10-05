# GUI 与原生 DLL

当前用户入口是 Windows x64 原生 Rust GUI。GUI、CLI 和工作流库共用同一个 JSON profile 读取器；`config.env`、临时 env 文件和隐藏的旧配置导入均已删除。

## 当前结构

- `rust/crates/dvda-desktop` 提供 Win32 GUI、JSON profile 打开/保存、后台任务和用户友好的日志。
- `rust/crates/dvda-cli` 提供开发调试、样本检查和自动化入口；GUI 不显示 dry-run。
- `rust/crates/dvda-core` 提供准备、MLP/LPCM 编码调度、缓存、菜单、制盘和成品校验。
- `rust/crates/dvda-native` 负责加载并调用已经验证的媒体、图像、格式、校验和 MLP C ABI。
- `native/mlp-encoder` 保存 MLP C17 核心和固定 x64 DLL。编码结果由算法直接生成，绝不在输出后打补丁。

## 运行边界

GUI 不启动 FFmpeg、FFprobe、ImageMagick、Metaflac、eac3to、原版 SurCode 或独立 MLP 编码器。所需库由源码构建后随发布包提供。组件缺失会立即报错，不会静默回退到 PATH 中的程序。

MLP 编码通过流式 C ABI 完成，保留 x87 PC53 运算和回调前后的浮点控制状态。相同 PCM、参数和辅助元数据上下文必须得到相同完整 MLP；比较程序只读验证，不修改文件。

## JSON profile

默认配置保存到 `%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`。开发脚本可在仓库根目录发现被 Git 忽略的 `settings.local.json`，也可以显式传入 `--profile PATH`。示例 settings.example.json 仅保留在源码中，发布包不包含任何 JSON 配置。

## 验收

```powershell
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File tools/win-build/test-rust-workflow.ps1
```

发布包审计确认只有一个内嵌必要 DLL、菜单资源及字体的 GUI EXE，以及三语用户 README、许可证和 NOTICE；不包含 .NET runtime、CLI、PDB、构建 JSON 或 `config.env`。
