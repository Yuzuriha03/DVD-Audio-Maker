# Rust 迁移与三语言行为审计（2026-10-05）

## 结论与范围

**历史记录：本报告初次“全部关闭”结论曾被独立复核撤回；所发现 R01–R16 现已修复，最终验证另补齐 R17。** 当前验收见 [修复验收报告](RUST-MIGRATION-RECHECK-FIXES-2026-10-05.md) 和 [完整清单](RUST-MIGRATION-REMAINING.md)。下文保留 G01–G14 的迁移过程和当时证据，141 项计数仍指当时执行，不改写为本轮结果。90 份旧源码和 115 个旧测试入口仍是审计索引。应用层为独立 Rust，格式和编码沿用 C/C17；没有 C# 运行回退。

本报告只描述本地源码和候选包，未提交、推送或更新 GitHub tag/release。旧 85 项及历史 C# 记录保留原值；此前证据见 [阶段验证记录](rust-migration-checklist-validation-2026-10-05.json)，当前状态见 [复核记录](rust-migration-recheck-2026-10-05.json)。固定测试不能证明任意未知音源与任意播放器都一致。

## 最初发现的八组遗漏（均已关闭）

下表保留实施前的发现背景。“当前”指初次复查时；修复后入口、正常/异常测试与验收证据逐条列在完成清单。随后还沿旧私有调用链发现并完成 G09–G14，见表后说明。

| 编号 | 旧行为与当前遗漏 | 当前证据与影响 |
|---|---|---|
| G01 字体工具 | `OpenTypeFontTool` 的三份 OTF 合并、共享表去重、TTC 校验，以及提取/检查字体的开发入口未迁移。旧打包入口调用 `PackMenuFonts`。 | `rust/crates/dvda-toolchain/src/main.rs` 仅复制现成 OTF/TTC，随后要求 `DvdaNotoCJK-Regular.ttc` 存在；不能仅凭原始 OTF 重建字体集合，也没有核对集合的字体顺序、名称和字形覆盖。现有候选包复用了既有 TTC。 |
| G02 原生打包输入校验 | 旧 `ValidateMediaRuntime`、`ValidateFormatsRuntime` 和图像打包校验会检查构建清单、长度/哈希、PE x64 架构和 DLL 导入依赖。Rust 打包入口尚无等价的完整输入校验。 | 当前 `dvda-toolchain` 按文件清单复制并拒绝同名内容冲突；`runtime.rs` 校验内嵌归档及释放后的内容。这些不能代替打包前核对原生构建记录、架构和依赖闭包。原生源码构建脚本仍在，此项不要求恢复已删除的外部命令兼容层。 |
| G03 准备清单与成品总轨数 | 旧 `DiscVerifier.QuickCheck` 将全部 ISO 的 IFO 总轨数与准备 manifest 总轨数比较；Rust 验证入口未接入 manifest。 | `verify.rs::Job/run` 仅使用编码索引与成品，已有 IFO/索引轨数对照；若编码索引与成品同时漏掉准备清单中的曲目，就缺少这项独立核对。 |
| G04 构建日志与成品交叉核对 | 旧 GUI 调用 `DiscVerifier.Audit`，读取本次/回退日志，检测 `PES_PADDING_FAILED`，核对制盘命令、曲目表和 ISO 的映射；当前正式验证未接回这些分支。 | `verification.rs::parse_audit_log` 只接到开发操作分发和单测，`app.rs::run_verify` → `verify.rs::run` 不读构建日志。直接读取 IFO 已覆盖部分结构检查，但没有补上上述日志诊断。 |
| G05 轨道扇区连续与总量 | 旧 `DiscVerifier.Audit` 检查相邻轨道 `first == previous.last + 1`（`TRACK_GAP`）和 AOB 总扇区数等于轨道末扇区加一（`AOB_SECTOR_MISMATCH`）。 | 当前 `ifo.rs` 检查单条范围，`verify.rs::timeline` 检查是否越过 AOB 边界和 PTS；`aob.rs::audit_diagnostics` 未补上这两项关系检查。音频无损验证仍存在，不能据此声称所有异常盘都会漏检。 |
| G06 静图异常分支 | 缺少制作阶段静图 VOB 警告、ASVS 固定头最小长度检查，以及无菜单预期时要求静图文件的分支。 | `menu_check.rs::verify`、`menu_verify.rs::stills/verify` 与旧入口不等价。单页 Next 检查经核对旧函数提前返回条件，确认不是迁移遗漏；初次局部判断已撤回。 |
| G07 开发打包入口 | `--formats-runtime`、构建环境路径覆盖、正常源码/产物目录默认值和发布版本格式校验未完整保留。 | 当前 `dvda-toolchain::run/package` 使用固定格式库目录及旧 release 字体/素材回退；这些是开发打包能力，不是被用户删除的 config.env。 |
| G08 发布提交与记录 | 最终 EXE/ZIP 的临时文件完成后替换，以及工具自动生成发布散列清单/EXE 哈希输出未完整迁移。 | 当前打包器提前清理上次 stage，删除旧 ZIP 后直接压缩；运行时缓存已有原子修复，不能代替发布产物的失败保留。 |

八组原始子项均已接回实际入口并通过反例；A01–A04 的继续核对又补齐以下行为：

| 补迁 | 最终行为与证据 |
|---|---|
| G09 开发入口 | 配置/计划/音频/格式/验证命令与 JSON 参数恢复；子模式材料要求、卷标、菜单关闭跳过、退出码正确；实际 CLI 及九组启动脚本回归。 |
| G10 独立 M4A 转换 | ALAC 修复、标签、封面和全精度 PCM 验证、有界并发、失败隔离、显式删除规则；实际文件占用/取消/混合批次通过。 |
| G11 FLAC 边界 | 截断、越界、非法编码和尾随数据返回错误；封面元数据原子修改保持音频字节，失败不留临时文件。 |
| G12 菜单字体/素材 | 真实文字覆盖、SC/JP/KR 回退、缺素材/封面/静图警告与错误语义；真实图像/ISO 反例和三语言诊断。 |
| G13 GUI | 34 项设置、精确 DVD9、日志摘要/详细/完整归档、筛选、提示、时间戳、任务忙状态、取消关闭、可拖拽分隔条；[GUI 专项审计](RUST-GUI-BEHAVIOR-AUDIT-2026-10-05.md)。 |
| G14 缓存发布 | 大小写无关缓存、坏记录警告、原子缓存写入、保留旧编码目标、实际 D: → C: 暂存替换；缺源/锁定目标/Unicode 路径回归。 |

完整字体 CLI 的 15 个案例、TTC/三份 OTF 两条真实打包路径均通过。重建后的三个区域字体与原始 OTF 逐字节相同，渲染结果相同；TTC 容器校验和字段与旧容器有差异，未宣称容器本身逐字节相同。开发散列记录在输出根目录，用户包仍不包含构建 JSON。

## 工作流修复

| 原缺口 | 当前实现 | 验证证据 |
|---|---|---|
| 源到成品 PCM 校验缺失 | 按编码时同样的转换和归一化策略重建目标 PCM，与解码文件全量比较；另核对缓存编码结果与盘内音频 | MLP/LPCM 应用测试、隔离 GUI 制作并验证；换成另一条有效音源会失败 |
| LPCM 误传 FLAC 到 WAVE 验证器 | 传入正确的规范化缓存 WAVE 与标题结束标志，并核对原音源转换链 | 多标题 LPCM ISO 及故意损坏盘内音频反例 |
| 缓存身份不足 | 准备、MLP/LPCM 缓存纳入媒体/编码 DLL 及依赖内容、转换策略、参数和元数据上下文 | 同长度、同时间戳替换组件及上下文仍失效；固定输出不改写 |
| 制作前检查未接入 | GUI/CLI 制作入口自动检查或复用有效快照；失败阻断制作 | 冷启动、重复制作、损坏音源及旧索引保留；ALAC 修复副本删除后重新生成 |
| 光盘断点未接入 | 暂存已完成 ISO，保存身份与签名，取消后可恢复；输入、参数、组件、封面、字体或菜单素材变化后重建 | 暂存后取消、继续复用且不再运行 author；换封面后重新 author |
| 空间与发布前检查不足 | 工作/缓存/输出所在卷进行合并空间估算；检查实际 ISO 容量、必要 AUDIO_TS 文件和菜单按钮/高亮 | 分卷估算单测、实际菜单制作；空间不足保持旧版的警告语义 |
| IFO/时间线检查缺失 | 从盘内 ATS IFO 解析标题、曲目、扇区、PTS 和静图引用，核对索引与 AOB 边界 | 多标题短曲、损坏 IFO、空组/重复组/重复盘索引反例 |
| 菜单验证未接回 | 发布前核对按钮 XML、返回索引与高亮；成品验证解析 AMG/ASVS 并解码页面检查画面、封面和索引文字区域 | 含索引、曲目分页、静态封面的实际 ISO；按钮错号、菜单导航错链、截断表反例 |
| 取消与失败路径 | 遍历、解码和原生数据回调传播取消；工作线程异常总会发回完成状态 | 流式转换/批量编码取消、异常回调、暂存取消、验证预取消 |

原生制盘程序与菜单/校验 DLL 已使用现有 MSYS2 GCC 从当前源码重建。修复 author 的图像库定位，使平铺 onefile 缓存与开发目录布局都可加载正确的 dvda-image.dll。

导入已有 MLP 保持只读。测试既覆盖采样数严格相同的成功导入，也覆盖导入带额外尾部补零文件时不能通过严格 PCM 验证。旧版只对内置编码的已知不足 1 ms 尾部零填充允许有限容差；这不是码流逐字节对照的容差，不能用于掩盖编码字节差异或任意外部文件差异。

## 界面、配置与日志

- 中文、英文、日文四页设置、状态、帮助、文件过滤器和已知诊断使用完整资源。准备报告与 GUI 共享翻译层；路径、标题、编码参数和未知原生诊断保持原值。
- JSON 数值/布尔值正常归一化，不再静默忽略。换工作目录重新求值派生路径，未知扩展键保留。config.env 不读取。
- 语言切换、任务开始和退出自动保存日常配置。打开别的方案不会自动覆盖它；提供保存、另存、覆盖提示与取消处理。
- 恢复高级设置折叠、小窗口滚动、字体文件选择、精确容量、标题分组、专辑数量限制、PCM 临时目录和 MLP 暂存目录。非法数值指出位置并阻止保存/执行。
- 恢复用时、提醒计数和运行指示；停止期间保持停止状态，结束后再关闭。
- 自动写入完整任务日志，制作全过程另写 build.log；每次任务重置显示，超过 2,500 条后内存缩到最近 2,000 条，导出从完整归档读取；暂停显示不丢记录，任务完成强制刷新，拒绝覆盖正在写入的日志。
- 源码启动脚本指定 x64 GNU 目标并设置已验证原生库位置，VS Code 使用 CodeLLDB。settings.local.json 指向当前重建的 author；个人路径继续被 Git 忽略。

界面采用 Rust 原生 Win32 控件，按操作和结果验证等价，不以 WinForms 像素或控件句柄相同为标准。开发 CLI 的参数/工具链诊断和未知 Windows/第三方诊断允许保留原文；面向用户的已知消息与报告支持三语言。

## 单 EXE 发布

候选包：`build/rust-migration-checklist-package/DVD-Audio-Maker-v1.0-win-x64.zip`，21,082,048 字节；EXE 为 23,045,130 字节。最终哈希记录见验收 JSON。

- ZIP 根目录恰有一个 GUI EXE、三个用户 README、LICENSE、两份 THIRD-PARTY 及两个 NOTICE，共九个文件。
- 必要的 27 个运行文件内嵌：原生库、项目 author、六个菜单素材文件和一份字体集合。FFmpeg 共享 DLL 去重；冲突内容报错。
- Windows 系统压缩 API 压缩归档；首次释放到用户缓存，完整 SHA-256 核验、并发锁、越界路径拒绝和原子修复。
- 文档、许可、开发 CLI、源码、PDB、配置/构建 JSON、.NET 运行时不嵌入。用户只运行顶层 EXE，内部使用释放的原生组件。
- 隔离测试只复制 EXE，并清除开发 DLL 环境覆盖；自动生成 FLAC 和渐变封面，从真实 GUI 完成 MLP/LPCM 带索引、分页、静图的制作及成品验证。英文准备报告已检查；损坏 mlp_encoder.dll 后重启恢复原完整哈希。

## 验证与复现

最终 Windows x64 workspace **141 项通过，0 失败、0 忽略**。其中包含 116 个冻结 MLP 样本的输入哈希、完整输出长度/SHA-256 与解码 PCM 检查，以及批量并发矩阵、168 项音频元数据场景、PCM 位深/声道/宽存储、ALAC 修复、图像、原生 ABI、事务发布与真实应用入口回归。116 是样本数，不等于 cargo 测试数；旧 C# 115 项仅映射，没有恢复执行。另通过九组实际启动脚本分支与准备失败阻断。

原生固定基准使用 build/media-native；生产共享库 build/media-native-shared 另通过应用入口和隔离 onefile 测试。冻结版本差异不能通过改写基准来掩盖。cargo fmt、workspace 全目标 Clippy（-D warnings）和 x64 release 构建通过。

真实 GUI 检查三语言、四页切换、Unicode/BOM JSON、扩展键、已有 MLP、精确容量、自动保存无参数重启恢复、非法数值、滚动、另存实际写入与文件对话框取消。截图和详细运行记录位于 Git 忽略的 build 目录。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/win-build/test-rust-workflow.ps1 -OtherVolume D:\
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --all-targets --offline -- -D warnings
python -X utf8 tools/win-build/test-rust-gui.py --exe build/rust-migration-checklist-package/DVD-Audio-Maker/DVD-Audio-Maker.exe
python -X utf8 tools/win-build/test-rust-onefile.py --exe build/rust-migration-checklist-package/DVD-Audio-Maker/DVD-Audio-Maker.exe
python -X utf8 tools/win-build/test-rust-package.py build/rust-migration-checklist-package
python -X utf8 tools/win-build/test-rust-build-entrypoints.py
python -X utf8 tools/win-build/audit-rust-migration.py
```

本轮使用自生成短音源，未重新制作用户的整套专辑，也未重新调用原版 SurCode 生成新对照。MLP 的本次证据是迁移前冻结的 116 个完整文件基准，不扩展为所有未知输入都已验证。C17 MLP 核心和固定 DLL 内容未改变，没有事后修补编码文件。

明确排除仍遵循用户要求：不恢复 config.env、C# 兼容层、GUI dry-run、外部 FFmpeg MLP 编码分支和原版 SurCode 进程；MLP 反编译来源替换问题不在本次范围内。GitHub 发布是后续独立操作。
