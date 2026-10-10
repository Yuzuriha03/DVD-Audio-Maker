# 开发和调试

当前应用是 Windows x64 Rust workspace。用户发布包只有 GUI；开发 CLI 保留在源码中。CLI 和 GUI 默认启用 `direct-bridges` 与 `rust-mlp`：媒体、图像、菜单适配器和 MLP 编码静态并入主程序，第三方 FFmpeg 等库仍为原生依赖。当前静态 one-file 候选包已完成产品回归、Rust MLP 菜单/标题/ISO/verify 回归和无开发环境变量的隔离启动验证。

旧 DLL 路径仅用于冻结参考与兼容回归，可用 `--no-default-features` 构建 CLI/GUI。默认直链构建不读取 `DVDA_ENCODER_LIBRARY`。

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

按 [Windows 构建说明](../tools/win-build/README.md) 准备第三方库与 dvd-author。直链构建需要设置 `DVDA_FFMPEG_PREFIX`、`DVDA_MAGICK_WORK` 和 `DVDA_MSYS_ROOT`。FFmpeg 使用延迟导入；正式包先解压内嵌 runtime 并注册 DLL 搜索目录，再调用媒体接口。隔离启动使用全新 `LOCALAPPDATA` 和仅系统目录的 PATH，已确认窗口创建以及仅 9 个第三方 DLL 的解压清单。开发测试直接调用接口时仍需显式提供这些第三方 DLL 的搜索路径。

`--no-default-features` 兼容构建和冻结差分测试才使用 `DVDA_MEDIA_NATIVE_DIR`、`DVDA_IMAGE_NATIVE_DIR` 与 `DVDA_ENCODER_LIBRARY`；默认直链构建不通过这些变量选择已迁移的后端。

MLP C17 核心位于 `native/mlp-encoder`，保留为冻结差分参考；默认生产编码入口使用静态链接的 `dvda-mlp`。格式解析、MLP CRC/奇偶校验、PCM 比较和 AOB 成品校验由 `dvda-native` 中的 Rust 实现完成，不依赖格式或成品校验 DLL。

剩余自有原生组件正在迁移到 workspace 中的 `dvda-mlp`、`dvda-menu` 和 `dvda-bridges`。crate 注册或默认 feature 编译通过不代表生产替换完成；媒体/图像 feature 的真实 DLL、菜单集成及编码器全部布局、元数据、错误与取消行为必须分别验证。旧 C 与冻结 DLL 在迁移验收前保留，冻结 DLL 仅用作差分 oracle；不要将尚未通过完整验收的 Rust 编码器加入发布包。dvd-author 及第三方库不属于本轮迁移范围。

## VS Code

使用 Rust Analyzer 和 CodeLLDB（或 WinDbg）调试 `dvda-desktop`、`dvda-cli`。`.vscode/tasks.json` 提供 workspace 构建、测试、release 构建和打包任务；`.vscode/launch.json` 提供 GUI/CLI x64 调试入口。

## 发布

```powershell
cargo build --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --release --workspace --offline
dvda-toolchain.exe package --repo . --output build/release --media-runtime build/media-native-shared --image-runtime build/image-native --encoder-runtime build/mlp-encoder --image-author build/rust-author-current --prebuilt build/release-menu-final --version v1.0
```

打包器强制验证编码器目录中的 `encoder-build.json`：Rust 实现、ABI v1、唯一的 `mlp_encoder.dll`、文件摘要与大小、x64 PE 及完整依赖闭包。此清单必须由验收通过的源码构建产生；没有清单或仍使用旧 C 编码器时拒绝打包，不再从其他路径静默回退。当前迁移完成前，上述发布命令仅作为目标流程，不代表已有可发布的编码器。

打包器还要求 author 输出附带 `menu-build.json`，其摘要必须与 `author-build.json` 的源码输入记录一致；菜单适配器必须为 Rust，SPU/导航 DLL 的大小与摘要必须同时匹配两个清单。缺失、旧 C、清单篡改或 DLL 不一致都在创建发布候选目录前拒绝，保留已有发行文件。

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
`min(2 × 可用逻辑处理器, 曲目数)` workers；取消或异常时不保存本轮参数缓存。

```powershell
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-native
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-core --lib mlp_workflow -- --include-ignored --skip real_cached --skip another_volume
```

Rust 测试覆盖完整扫描、CRC 边界、对齐输出和 PCM 比较；可选旧 DLL 差分测试见[格式处理说明](C17-FORMATS.md)。

迁移回归已包含固定编码器生成的九种真实 MLP profile（44.1–192 kHz、16/20/24 位、单声道/立体声/六声道），检查与对齐结果均与旧 DLL 对照。设置 `DVDA_FORMATS_ORACLE=1` 启用；旧 DLL 仅用于测试，不作为生产回退。
设置 `DVDA_TEST_AUTHOR` 为项目构建的 author EXE，执行 `cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-native --test authored_lpcm` 可运行完整 LPCM 制盘矩阵；不要设置 `DVDA_TEST_AUTHOR_QUICK`，以覆盖 gapless/分 title、短音轨、奇数采样、音频/头部损坏与截断拒绝。
菜单集成测试的 `DVDA_TEST_MENU_DATA` 应指向包含 `menu` 的素材目录；通过 `DVDA_MENU_NATIVE_DIR` 显式指定已验收的菜单 DLL 目录（验收脚本的 `-MenuRuntime` 参数），不再要求素材同级放置旧 `menu-bin`。未显式指定时仍使用内嵌运行时或同级 `menu-bin`。迁移后应用 8 项、转换 1 项、编码 5 项集成测试通过（不含显式性能基准）。本地单文件打包通过，实际 EXE 内嵌索引不含旧 formats/verifier DLL；这些验证不代表硬件播放认证。
只读真实缓存计时测试 `real_cached_mlp_preflight_benchmark_is_read_only` 需要设置
`DVDA_MLP_PREFLIGHT_MANIFEST`、`ROOT`、`OUTPUT`、`MEDIA`、`ENCODER` 和 `REPORT`
（后五项均带相同的 `DVDA_MLP_PREFLIGHT_` 前缀）。媒体/编码 DLL 必须与缓存身份一致；
任一未命中都会失败，不会编码或覆盖样本。测试计时不能替代完整制盘与播放验证。

## PCM 准备优化与并行度评估

所有生产 worker 池使用检测到的逻辑处理器数量的两倍（饱和计算、至少一个），只受工作项数量限制。旧 job 字段被忽略；CLI `--jobs` 与求值选项 `MlpJobs` 已移除。Windows 媒体编解码上下文也请求同一自动策略，实际线程由编解码器支持情况决定；菜单静图解码同样使用该策略，但单帧 PNG 编码必须保持单线程（帧线程编码器只在 flush 后才交回输出，一次性编码会返回 EAGAIN）。构建脚本共用 `worker_policy.py`，移除固定构建上限和 FFmpeg 构建 `--jobs` 选项。GUI 编排与子进程跟踪监听是单任务协调，不属于并行工作池。

批量编码在解码 WAV 的采样率、存储/有效位深、声道掩码、RIFF 结构与 PCM 精度均满足目标时直接复用该临时文件；否则仍执行规范化。20 位数据复用前检查低四位，保留取消检查和临时文件清理。规范化对 16→16、24→24 使用批量复制，24→20 校验后复制，32 位存储→24 位使用专用打包路径，不放宽精度校验。

真实缓存预检基准保留 1/2/4/8/16 标签作为旧配置兼容性探针：请求值被忽略，报告同时记录 `requested_workers` 和自动策略的实际 worker 数。它仅测量缓存校验，不代表完整解码/编码吞吐；应使用真实缓存、匹配的媒体/编码 DLL，多轮运行比较中位数，并另外监测内存与 CPU。缺少真实数据时不根据策略单元测试调整生产调度，也不声称已获得性能收益。

合成端到端基准可显式运行 `encoder_native.rs` 中默认忽略的 `synthetic_batch_worker_benchmark`。设置 `DVDA_BATCH_BENCH=1`、`DVDA_BATCH_BENCH_ROOT`（build 下不存在的短绝对目录）、`DVDA_BATCH_BENCH_EXE`（发布单文件 EXE）和 `DVDA_ENCODER_LIBRARY`，使用 Release GNU target 执行该测试。基准从 EXE 提取并校验媒体组件，为 16/20/24 位分别生成 16 条约半秒的立体声/六声道音轨，交替使用旧请求值标签 1/2/4/8/16，对自动策略各测三轮。720 个输出均与首次自动策略运行的 MLP 摘要及独立生成的 PCM 对照（包含末尾 AU 零填充）；已验证的输出删除，报告写入自有目录中的 worker-benchmark.json。计时不包含样本生成和输出验证，也不代表优化前后收益或长专辑性能；不据此提高默认并发数。
