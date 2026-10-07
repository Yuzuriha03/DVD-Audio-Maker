# C# 应用层迁移至 Rust

本文件记录最终采用的迁移方案和当前验收边界。目标是形成 **Rust 应用层 + 已验证的 C/C17 原生组件** 的 Windows x64 程序，并保持音频、MLP、LPCM、菜单、制盘和成品校验行为。

## 当前验收状态（2026-10-05）

**报告与清单已确认的迁移范围完成本地验收。** 补迁及独立复核发现的问题均已修复，涵盖正式 PTS/无损验证、GUI 设置/诊断/日志队列、MLP 导入路径及续跑/发布失败处理。逐项历史验收报告可从 Git 历史追溯；旧源文件和测试入口数量仅为审计背景，不把本次通过结论扩展为任意未知输入保证。当前为独立 Rust 应用层与 C/C17 组件，没有恢复 C# 或 config.env。

Windows x64 workspace 最终 **160 项通过，0 失败、0 忽略**；116 个冻结 MLP 样本的整文件长度/SHA-256 和解码 PCM 均通过，基准及编码核心未改写。最终单 EXE 的 18 个 GUI 案例、MLP/LPCM 带菜单静图制作/完整成品验证、缓存 DLL 修复及发布布局/哈希核验均通过。验证使用自建小样本，未重跑用户全盘。

- GUI/CLI 制作入口自动检查音源，恢复有效准备快照，核对编码身份、磁盘空间与光盘断点，再事务式发布。
- 成品验证包含源到目标 PCM、编码结果到盘内音频、IFO 曲目和时间线，以及菜单导航、按钮、高亮和画面。
- `MlpStreamAligner`、`PcmComparer` 与格式边界继续由 C17 DLL 提供。MLP 固定样本按输入、参数和上下文核对完整输出，不对编码结果打补丁。
- 三语言 GUI 及准备报告共享翻译资源，路径、音轨标题和元数据保持原样，未知原生诊断保留原文。
- JSON 配置自动保存，支持打开、保存和另存；旧 config.env、C# 兼容层、GUI dry-run 和外部编码进程已按用户要求移除。
- 发布包是必要组件内嵌的 x64 GUI EXE，用户说明和许可旁置，同放进一个标准命名 ZIP。

## 目录边界

| 目录 | 用途 |
|---|---|
| `rust/crates/dvda-core` | Rust 工作流、配置、缓存、媒体准备、制盘、校验和业务规则 |
| `rust/crates/dvda-desktop` | 原生 Win32 GUI |
| `rust/crates/dvda-cli` | 开发 CLI、样本和验证入口 |
| `rust/crates/dvda-native` | 原生 DLL 的 Rust ABI 加载与调用 |
| `rust/crates/dvda-toolchain` | Rust 发布打包与包内容审计 |
| `native/mlp-encoder` | MLP C17 编码核心、源码、构建脚本和固定 x64 DLL |
| `tools/formats-native` | 格式解析、对齐和 PCM 比较的 C17 DLL 源码 |
| `tools/win-build` | 原生组件构建、author 组装和发布包辅助脚本 |

## 配置规则

JSON profile 的 `Values` 保存用户设置。GUI 可以编辑并保存 profile，CLI 和 GUI 使用同一读取器。环境变量仅作为开发自动化的显式覆盖，不能触发 `config.env` 搜索或导入；发布包使用经过完整性检查的内嵌组件缓存。

MLP 编码设置使用 `DVDA_MLP_SOURCE=surcode-batch`，LPCM 使用 `lpcm`，已有原版 SurCode MLP 文件通过 `DVDA_MLP_EXTERNAL_DIR` 只读导入。旧的 FFmpeg MLP 编码分支、原版 SurCode 启动、eac3to 调用和事后输出修补均不存在。

## 验收命令

```powershell
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File tools/win-build/test-rust-workflow.ps1 -OtherVolume D:\
cargo build --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --release --workspace --offline
```

需要使用本机已验证的原生 DLL 目录设置 `DVDA_ENCODER_LIBRARY`、`DVDA_MEDIA_NATIVE_DIR`、`DVDA_IMAGE_NATIVE_DIR`、`DVDA_DISC_VERIFY_LIBRARY` 和 `DVDA_FORMATS_NATIVE_DIR`，然后运行 workspace 测试。菜单 fixture 会在进程内生成菜单、索引页、静图、AUDIO_TS 和 ISO，并调用项目构建的 author 验证。

发布包由以下命令生成：

```powershell
dvda-toolchain.exe package --repo . --output build/rust-migration-checklist-package --media-runtime build/media-native-shared --image-runtime build/image-native --image-author build/rust-author-current --source build/source-release-20261004 --prebuilt build/prebuilt-release-20261004 --formats-runtime build/formats-native --version v1.0
```

标准包名为 `DVD-Audio-Maker-v1.0-win-x64.zip`。必要组件内嵌，首次运行释放到用户缓存并验证完整性；三个用户 README 与许可放在 EXE 旁边。详细布局和独立 EXE 验证脚本见 [单文件发布](ONEFILE-PUBLISH.md)。

## 完成判定

迁移只有同时满足以下条件才算完成：

1. Rust workspace 可以在 x64 目标完成格式检查、Clippy、测试和 release 构建。
2. 原生 ABI 测试、PCM/格式样本对照、MLP 固定样本和菜单 ISO fixture 全部通过。
3. 发布包在没有 .NET、外部 FFmpeg、ImageMagick、eac3to 或 SurCode 的环境中只依靠包内组件运行。
4. `rg --files -g '*.cs' -g '*.csproj' -g '*.sln'` 无结果，源码和发布流程不存在 C# 回退入口。
5. JSON profile 能完成 GUI/CLI 的加载、编辑和保存，非 JSON profile 明确拒绝。
6. MLP 结果来自编码行为本身；验证只比较输入、参数、上下文和完整输出，不修改输出字节。
7. 工作流、GUI/CLI 和打包入口的迁移验收均应闭环；底层函数单独通过不能替代入口验证。历史逐项记录从 Git 历史追溯，后续问题以对应修复及回归测试记录。
8. 恢复必要组件内嵌的 onefile 发布、运行时完整性检查和修复；用户说明及许可放在 EXE 旁边，同装入标准命名 ZIP。

本文件不把历史 C# 测试命令当作当前构建步骤；历史覆盖通过 Rust 测试、固定样本和 Git 历史追溯。
