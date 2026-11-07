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
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --workspace --offline
```

`settings.json` 是唯一的文件配置格式。`settings.local.json` 适合保存本机路径，已被 Git 忽略；也可以通过 `--profile` 使用隔离方案。`config.env`、`DVDA_CONFIG` 和 `--config` 不受支持。

GUI 只提供检查音源、制作光盘和验证成品。dry-run 仅属于开发 CLI 诊断入口，不显示在 GUI 中。

## 原生组件

按 [Windows 构建说明](../tools/win-build/README.md) 准备第三方库与 Rust author。直链构建使用 `DVDA_FFMPEG_PREFIX`、`DVDA_MAGICK_WORK` 和 `DVDA_IMAGE_DEPS_DIR`，可由 `cargo-msvc.ps1` 设置默认值；不再需要菜单 vendor。生产默认使用 MSVC static profile，将五个 FFmpeg 库及 soxr/zlib 静态链接到 EXE；内嵌运行资源只解压字体、XML 配置和菜单素材。shared profile 仍可用于开发对照测试，其 GUI 使用延迟导入。

`--no-default-features` 兼容构建和冻结差分测试才使用 `DVDA_MEDIA_NATIVE_DIR`、`DVDA_IMAGE_NATIVE_DIR` 与 `DVDA_ENCODER_LIBRARY`；默认直链构建不通过这些变量选择已迁移的后端。

默认生产 MLP 编码入口使用静态链接的 `dvda-mlp`，旧 C 编码器及冻结 DLL 已清理。独立开发 ABI 适配器可由 Rust 构建到 `build/mlp-encoder`；历史 C 对照仅接受显式外部输入。格式解析、MLP CRC/奇偶校验、PCM 比较和 AOB 成品校验由 `dvda-native` 中的 Rust 实现完成，不依赖格式或成品校验 DLL。

完整制盘现由 `dvda-author` 提供：AOB、ATSI/SAMG/AMG/ASVS、菜单/静图编排及 ISO9660/UDF writer 都是 Rust 实现，GUI/CLI 经 `dvda-core::author_runtime` 进程内调用。项目旧 C author 源码、构建入口和产物已清理；保留的第三方图像、视频库经 Rust bridge 调用。接口、构建与独立制盘验收见 [Rust author 迁移记录](RUST-AUTHOR-MIGRATION.md)。

自有菜单会话、SPU、导航和资源管理现由进程内 `dvda-menu` Rust 实现；不再构建菜单 vendor 或 C 跳转桥。保留的第三方库为图像/视频依赖，另有构建期 ABI 探针；历史阶段见 [菜单会话迁移记录](RUST-MENU-SESSION-MIGRATION.md)。

## VS Code

使用 Rust Analyzer 和 CodeLLDB（或 WinDbg）调试 `dvda-desktop`、`dvda-cli`。`.vscode/tasks.json` 提供 workspace 构建、测试、release 构建和打包任务；`.vscode/launch.json` 提供 GUI/CLI x64 调试入口。

## 发布

```powershell
cargo build --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --release --workspace --offline
dvda-toolchain.exe package --repo . --output build/release --media-runtime build/msvc-native/ffmpeg-static/install --image-author build/rust-author-production --prebuilt build/release-menu-final --version v1.0
```

默认 GUI 将 Rust MLP 编码器静态链接到主程序，不装配编码器 DLL。只有显式提供 `--encoder-runtime` 时，打包器才验证该目录的 `encoder-build.json`、Rust ABI v1、文件摘要与大小、x64 PE 及完整依赖闭包。

打包器验证 Rust author 的 `author-build.json`、源码及运行资源，不再要求 `menu-build.json` 或静态菜单 vendor。开发 author EXE 不进入 GUI 的内嵌运行时；工作流测试单独构建它。缺失、清单篡改或文件不一致都在创建发布候选目录前拒绝，保留已有发行文件。

发布包 `DVD-Audio-Maker-v1.0-win-x64.zip` 不包含 .NET runtime、开发 CLI、PDB、构建来源 JSON、`config.env` 或个人 profile。构建产物留在被 Git 忽略的 `build` 目录。

## 验收

`build/msvc-native` 中的清单记录第三方库的构建来源和哈希，应与库文件一起保留。当前静态 Rust author 需要重建 FFmpeg，不再要求 ImageMagick 或菜单 vendor。`build-rust-author.py` 使用 `--ffmpeg-runtime`、`--menu-assets` 和 `--font`；`--magick-work` / `--image-dependencies` 已退役并显式报错。清单只收录实际字体和菜单素材，不再生成或装配 ImageMagick XML 配置。静态打包验证不再要求旧图像目录。完整发布及视觉差分验收仍在进行。

```powershell
cargo +1.99.0 fmt --manifest-path rust/Cargo.toml --all -- --check
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --locked --offline --workspace --all-targets --all-features -- -D warnings"
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --locked --offline --workspace --all-targets -- -D warnings"
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --locked --offline --workspace --all-targets --no-default-features -- -D warnings"
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --locked --offline -p dvda-core --all-targets --no-default-features -- -D warnings"
powershell -NoProfile -ExecutionPolicy Bypass -File tools/win-build/test-rust-workflow.ps1
```

工作区将 `clippy::pedantic` 保持为 `warn`，严格验收入口用 `-D warnings` 提升为错误。协议数值转换和 C 对应实现只使用带理由的局部豁免；FFmpeg 生成代码在 `build.rs` 修复，不能直接编辑 `target` 产物。

审计既有豁免时，另运行 `cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --workspace --all-targets --all-features --locked --offline --message-format=json -- --force-warn clippy::pedantic`，并对 default / no-default-features 分别收集结果。这会展开被抑制的诊断，退出成功不代表零警告；比较时每条诊断只取第一个 primary span，按规则、文件和行号去重，保持相同工具链与 feature，同时检查 `unfulfilled_lint_expectations`。workspace 的 no-default-features 不会消除依赖重新启用的 Core 默认 feature，因此上面单独检查 Core 的 fallback 分支。补齐具体 Errors/Panics 契约后删除对应文档豁免，不以 `allow` 改为 `expect` 作为修复。长函数优先按独立职责拆分；共享控件注册表的 GUI 装配、平铺的配置映射、GDI 资源生命周期及顺序 WAV 解析可以保留有具体理由的函数级豁免，不为行数引入平行状态结构。MLP 的 C 对应函数边界、算术和调用关系不属于 lint 重构范围。

菜单 fixture 会生成菜单、索引页、静图、AUDIO_TS 和 ISO，并调用项目构建的 author。MLP 的逐字节样本对照、PCM 比较和 C17 ABI 验证记录在各组件文档中。

工作流默认 `-Mode Functional`：通过 `cargo-msvc.ps1` 构建当前源码的 `dvda-author-dev`，从 Cargo JSON 获取真实产物路径，将 EXE 与运行库 DLL 装配到独占的 `build/rust-workflow/author-*`，结束后清理。不再读取生产目录遗留的 author EXE。媒体/图像 runtime 和菜单素材必须提前准备，默认分别为 `build/rust-author-production` 及其 `data`；脚本不重建第三方依赖。

使用 `-Mode Benchmarks` 单独以 Release 编译、按完整测试名执行七项手动基准，不运行功能测试。缺少可选条件会逐项输出 `NOT RUN`，不计为通过：

- 跨卷：`-OtherVolume` 或 `DVDA_TEST_OTHER_VOLUME`，必须可写且与临时目录不同卷；显式传入无效目录时报错。
- 浮点无损矩阵：PATH 中需有 `ffmpeg`，该项属于功能测试而非性能基准。
- 五项外部编码器集成：显式 `-EncoderLibrary` 或 `DVDA_ENCODER_LIBRARY`，SHA256 必须匹配 MLP 文档中的冻结 C oracle；不自动查找或恢复 C 产物。
- 哈希基准：`DVDA_HASH_BENCH_INPUTS` 为现有文件路径 JSON 数组，`DVDA_HASH_BENCH_REPORT` 的父目录须存在。
- 真实缓存：完整设置 `DVDA_MLP_PREFLIGHT_{MANIFEST,ROOT,OUTPUT,MEDIA,ENCODER,REPORT}`，输入路径及报告父目录须存在；缓存身份由测试进一步验证。
- 批处理基准：oracle 加 `DVDA_BATCH_BENCH=1`、尚不存在的 `DVDA_BATCH_BENCH_ROOT`，以及指向已打包 EXE 的 `DVDA_BATCH_BENCH_EXE`。

功能测试遇到失败仍继续其他测试 EXE，最终返回非零并汇总失败程序。完整 LPCM 验收不要设置 `DVDA_TEST_AUTHOR_QUICK`。跳过项与真实失败必须分别记录，测试成功也不代表播放器验收通过。


## SHA-256 后端回归与基准

生产 SHA-256 固定使用 `sha2 = 0.11.0`，x86/x64 在运行时检测到 `sha`、`sse2`、`ssse3`、`sse4.1` 时使用 SHA-NI，否则回退可移植实现；构建不强制 `target-cpu=native`。不再保留手写 SHA-256 实现。回归测试保留标准向量，并将封装层的一次性、分块和 reader 摘要与上游 `sha2` API 比较。

```powershell
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --release --offline -p dvda-core --lib hash:: -- --skip release_files_benchmark
```

只读 Release 文件基准通过 `DVDA_HASH_BENCH_INPUTS` 接收 JSON 文件数组，并将新 JSON 写入 `DVDA_HASH_BENCH_REPORT`。它对真实 DLL、MLP 缓存样本和发布 ZIP 测量 sha2 内存及 warm-file 摘要，采用七轮中位数；新报告仅包含 sha2 耗时，已有前后对比报告作为历史记录保留；结果只表示摘要路径，不代表冷盘或完整制盘耗时。报告保存在被 Git 忽略的 `build` 目录。`sha2` 及其传递依赖沿用上游 MIT 或 Apache-2.0 许可证。


缓存复用仍须通过源/输出指纹、编码组件身份、目标参数与完整 MLP CRC/奇偶校验；
参数缓存未知或缺失时重新读取，不跳过完整扫描。自动检查线程数为
`min(2 × 可用逻辑处理器, 曲目数)` workers；取消或异常时不保存本轮参数缓存。

```powershell
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --offline -p dvda-native
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --offline -p dvda-core --lib mlp_workflow -- --include-ignored --skip real_cached --skip another_volume
```

Rust 测试覆盖完整扫描、CRC 边界、对齐输出和 PCM 比较；可选旧 DLL 差分测试见[格式处理说明](C17-FORMATS.md)。

迁移回归已包含固定编码器生成的九种真实 MLP profile（44.1–192 kHz、16/20/24 位、单声道/立体声/六声道），检查与对齐结果均与旧 DLL 对照。设置 `DVDA_FORMATS_ORACLE=1` 启用；旧 DLL 仅用于测试，不作为生产回退。
设置 `DVDA_TEST_AUTHOR` 为项目构建的 author EXE，执行 `cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --offline -p dvda-native --test authored_lpcm` 可运行完整 LPCM 制盘矩阵；不要设置 `DVDA_TEST_AUTHOR_QUICK`，以覆盖 gapless/分 title、短音轨、奇数采样、音频/头部损坏与截断拒绝。
菜单集成测试的 `DVDA_TEST_MENU_DATA` 指向包含 `menu` 的素材目录。`DVDA_MENU_NATIVE_DIR` 仅表示运行资源目录，不是静态 vendor archive；媒体/图像目录与字体配置指向已装配 runtime，不再要求旧 `menu-bin`。历史集成测试结果不代表硬件播放认证。
只读真实缓存计时测试 `real_cached_mlp_preflight_benchmark_is_read_only` 需要设置
`DVDA_MLP_PREFLIGHT_MANIFEST`、`ROOT`、`OUTPUT`、`MEDIA`、`ENCODER` 和 `REPORT`
（后五项均带相同的 `DVDA_MLP_PREFLIGHT_` 前缀）。媒体/编码 DLL 必须与缓存身份一致；
任一未命中都会失败，不会编码或覆盖样本。测试计时不能替代完整制盘与播放验证。

## PCM 准备优化与并行度评估

所有生产 worker 池使用检测到的逻辑处理器数量（至少一个），并按工作项数量限制，不再乘以二或设置固定的 16 线程上限。废弃的 job `jobs` 字段、CLI `--jobs` 与求值选项 `MlpJobs` 已移除。Windows 媒体编解码上下文按活动媒体任务数均分逻辑处理器（向上取整），避免外层并行时嵌套过度订阅；单个任务仍可使用全部核心，实际线程由编解码器支持情况决定；菜单静图解码同样使用该策略，但单帧 PNG 编码必须保持单线程（帧线程编码器只在 flush 后才交回输出，一次性编码会返回 EAGAIN）。构建脚本共用 `worker_policy.py`，移除固定构建上限和 FFmpeg 构建 `--jobs` 选项。GUI 编排与子进程跟踪监听是单任务协调，不属于并行工作池。

批量 MLP 编码与 LPCM 准备在解码 WAV 的采样率、存储/有效位深、声道掩码、RIFF 结构与 PCM 精度均满足目标时直接复用该临时文件；否则仍执行规范化。20 位数据复用前检查低四位，保留取消检查和临时文件清理。规范化对 16→16、24→24 使用批量复制，24→20 校验后复制，32 位存储→24 位使用专用打包路径，不放宽精度校验。

制作模式在需要重新完整解码时，可同步生成并规范化生产 PCM；默认 Rust MLP 直连则在该次解码中通过有界内存队列直接生成临时 MLP，不写生产 WAV。清单携带源文件/中间文件身份及目标参数、抖动和媒体组件签名，MLP/LPCM 仅在全部证据匹配时复用，否则回退原转换。显式 MLP 元数据和非直连兼容构建保留 WAV 路径；单独检查不生成 PCM。直连成品校验并行解码源和成品，经有界队列逐样本比较，不生成临时 WAV/raw，保留首个差异偏移和末尾零填充检查。ALAC cookie 扫描、包位检查与修复均使用有界文件读写，修复结果仍以临时文件原子发布。完整验证中菜单结构检查与无损内容校验并行，诊断按阶段稳定合并。

WAV 容器读写统一由 `dvda-audio` 实现；`dvda-core` 只负责 DVD-Audio 参数限制和 PCM 转换。制作入口使用整数 RIFF 读取策略（拒绝重复 fmt/data，跳过附加元数据，兼容仅末尾 data 缺少对齐字节），规范化使用共享 Writer 的紧凑 RIFF 模式，保持 68 字节头及 32 位长度限制。通用媒体入口仍使用原有 RIFF/RF64 策略及带 RF64 预留区的 104 字节头。

真实缓存预检基准比较自动调度的首次检查与参数缓存复用，只报告实际 worker 数。它仅测量缓存校验，不代表完整解码/编码吞吐；应使用真实缓存、匹配的媒体/编码 DLL，多轮运行比较中位数，并另外监测内存与 CPU。缺少真实数据时不根据策略单元测试调整生产调度，也不声称已获得性能收益。

合成端到端基准可显式运行 `encoder_native.rs` 中默认忽略的 `synthetic_batch_worker_benchmark`。设置 `DVDA_BATCH_BENCH=1`、`DVDA_BATCH_BENCH_ROOT`（build 下不存在的短绝对目录）、`DVDA_BATCH_BENCH_EXE`（发布单文件 EXE）和 `DVDA_ENCODER_LIBRARY`，使用 Release GNU target 执行该测试。基准从 EXE 提取并校验媒体组件，为 16/20/24 位分别生成 16 条约半秒的立体声/六声道音轨，自动策略测三轮，每轮重复五次。720 个输出均与首次自动策略运行的 MLP 摘要及独立生成的 PCM 对照（包含末尾 AU 零填充）；已验证的输出删除，报告写入自有目录中的 worker-benchmark.json。计时不包含样本生成和输出验证，也不代表优化前后收益或长专辑性能；不据此提高默认并发数。

配置方案使用正式的 `DVDA_*` 键；已移除将 SourceDirectory、FinalDirectory、BuildDirectory、Title、IsoPrefix、MlpSource、MlpExternalDirectory 自动展开为配置键的旧别名。求值结果中的同名字段不受影响。
