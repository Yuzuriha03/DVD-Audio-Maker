# Rust 开发 CLI

正式发布包只包含 GUI。源代码工作区提供 `cli.cmd`，用于开发时执行可重复的 Rust 诊断任务。

## JSON 方案

CLI 通过 `--profile` 读取 JSON 方案。不指定时，`cli.cmd` 会优先使用被 Git 忽略的 `settings.local.json`；程序默认方案位置为 `%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`。

```bat
cli.cmd abi.version
cli.cmd prepare --profile "C:\work\settings.json"
cli.cmd build --dry-run --profile "C:\work\settings.json"
cli.cmd verify --profile "C:\work\settings.json"
```

不支持 `config.env` 和 `DVDA_CONFIG`。开发 CLI 的 `--config` 仅作为 JSON `--profile` 别名，不读取 env 文件。旧方案中的外部工具路径不会被读取或执行。媒体、图像、格式和编码 DLL 必须使用项目构建并验证过的 x64 组件。

## 已恢复的开发命令

`config --check/--shell/--shell-all` 检查必填项或导出求值后的路径和设置；`plan` 查看分盘；`prepare --force` 强制重验；`build --no-resume` 禁用本次续跑。三份启动脚本均固定 x64 GNU 目标并设置原生组件路径。

`verify` 支持 `all/quick/capacity/audit/menu/timeline/lossless/config`。独立 `quick-check`、`audit` 支持 `--iso-dir`、`--manifest`、`--log`。成功返回 0，发现损坏返回 1，仅材料不足或用法错误返回 2。`--language` 支持中英日。

`dvda-cli convert PATH... [--in-place] [--dry-run] [--level 0..8]` converts ALAC to FLAC; worker count is automatic and uses two workers per detected logical processor, capped only by the number of files.

格式诊断入口包括 `iso list ISO [INNER]`、`iso extract ISO INNER OUTPUT`、`aob-pts FILE...` 和 `mlp --check FILE...`。`mlp --align` 仅为显式开发修复命令，正式 MLP 编码不调用它改写输出。

字体工具为 `dvda-toolchain fonts extract/verify/pack/verify-collection`，例如 `fonts extract INPUT.ttc OUTPUT_DIR` 或 `fonts verify FONT.otf "Noto Sans CJK SC"`。这些开发入口不进入用户发布包。

## 构建和测试

```bat
cargo build --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline
cargo fmt --manifest-path rust\Cargo.toml --all -- --check
cargo clippy --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File tools\win-build\test-rust-workflow.ps1 -OtherVolume D:\
```

`build.cmd`、`verify.cmd`、`gui.cmd` 和 `gui-debug.cmd` 都是 Rust 薄封装。`build --dry-run` 仅用于开发诊断，可能生成准备和编码缓存，但不会创建 ISO。
