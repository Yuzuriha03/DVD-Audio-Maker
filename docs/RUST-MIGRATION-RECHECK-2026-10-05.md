# 功能迁移独立复核（2026-10-05）

**本文件是修复前的独立复核快照，保留原反例、代码位置和当时 141 项测试结果。R01–R16 现已修复；最终验证补查的 R17 也已完成，当前状态见 [修复验收报告](RUST-MIGRATION-RECHECK-FIXES-2026-10-05.md)、[机器可读验收记录](rust-migration-recheck-fixes-2026-10-05.json)。** 下文“当前”“本轮”均指发现问题时。

发现问题时的结论：**主要流程已迁入 Rust/C，但尚未达到与旧 C# 行为等价的完成标准。** 本轮确认 16 项待补行为，其中 2 项会让实际没有通过必要检查的成品显示校验成功。此前“全部关闭”的结论撤回；原来的通过记录保留为已执行测试的结果，不能用作全部分支完成的证明。

对照基线为 `fb3cbb84788a807758fa6a8f472a9ffdad3115e8`，被测实现为当前本地工作树和同一工作树生成的 x64 单 EXE。旧行为通过 `git show` 核对，没有重新运行 C#。本轮只修改审计文档；反例、音频、ISO、配置和锁文件均使用隔离测试目录。未运行用户音乐库全盘制作。

## 已重新验证的部分

- Windows x64 workspace：**141 项通过，0 失败、0 忽略**，包括真实原生调用、跨卷文件替换，以及 116 个冻结 MLP 样本的整文件长度/SHA-256 与解码 PCM 对照；没有修改冻结基准或编码核心。
- `cargo fmt --check` 与全 workspace、全部 target 的 `clippy -D warnings` 通过。
- 候选 ZIP 检查通过：9 个根文件、27 个内嵌组件，开发输出中的 10 条散列记录与文件一致。单 EXE SHA-256 为 `e2446c4ddeb570f48455e8262d6e3e6d6e78862013b24d961ed921bcf5f6ef6c`。
- SC/JP/KR 字体集合再次通过真实工具入口检查；此次没有重新打包。此前 onefile 制作证据仍对应相同 EXE，但本轮 GUI 复核另外发现以下反例。
- 当前应用层没有 `.cs`、`.csproj`、`.sln`。这说明实现语言已迁移，不等于旧行为全部迁移。

完整检查、证据散列及反例摘要见 [机器可读复核记录](rust-migration-recheck-2026-10-05.json)。修复待办同步进入 [迁移清单](RUST-MIGRATION-REMAINING.md)。

## 已确认的 16 项问题

P1 表示应优先修复的校验误报成功；P2 表示明确的行为差异或失败保护缺口；P3 表示较低影响的诊断差异。所有条目当前均未修复。

| 编号 | 优先级 | 实际问题与影响 | 当前入口 / 旧依据 |
|---|---|---|---|
| R01 | P1 | **正式时间线和完整验证漏掉 PTS 统计。** 把测试盘的全部时间戳改成 0，`verify timeline` 仍成功；另一个真实 LPCM 小盘的 19 个时间戳改为 0 后，`verify all` 同样成功，PCM 对照仍一致。已有独立 `aob-pts` 能发现异常，但正式入口未调用对应统计。 | [verify.rs](../rust/crates/dvda-core/src/verify.rs) `timeline` / 行 683、707、745；旧 `VerificationPipeline.cs:288`、`AobPtsAnalyzer.cs:155` 检查 `PTS_TOO_FEW`、`PTS_NOT_ADVANCING`、`PTS_ABNORMAL_RATIO`。 |
| R02 | P1 | **空编码索引使无损验证零轨成功。** ISO 存在、索引为 `{"__discs__":[]}` 时，`verify lossless` 返回 0、`Succeeded:true`、`TrackCount:0`，实际没有比较音频。 | [verify.rs](../rust/crates/dvda-core/src/verify.rs) 行 237、795、802；旧 `VerificationPipeline.cs:342` 明确拒绝空盘列表，报告材料不足。 |
| R03 | P2 | **缺索引的结果类型和独立检查丢失。** 当前返回笼统 `VERIFY_FAILED` / 退出码 1；旧实现返回 `MLP_INDEX_MISSING`、不可完成状态 / 退出码 2。当前 All 在运行其余独立检查前便退出。 | [verify.rs](../rust/crates/dvda-core/src/verify.rs) 行 100、224；旧 `VerificationPipeline.cs:574`、CLI `Program.cs:639、645、982`。缺索引退出码已实测，其余检查被提前跳过由调用顺序确认。 |
| R04 | P2 | **CLI 显式指定不存在的清单或日志仍成功。** `quick-check --manifest missing.json`、`--log missing.log` 均返回 0，没有提示指定材料未检查。默认可选材料缺失与用户明确指定缺失被混为一谈。 | [developer.rs](../rust/crates/dvda-cli/src/developer.rs) 行 443；[verification.rs](../rust/crates/dvda-core/src/verification.rs) 行 284；旧 CLI `Program.cs:692–705` 预检后返回 2。 |
| R05 | P2 | **指定 ISO 目录时改变了检查范围。** `--iso-dir` 清空配置前缀，并丢弃默认 manifest/log。正常方式能发现“IFO 2 轨、manifest 1 轨”；仅追加同目录 `--iso-dir` 后变成成功。目录里不属于配置前缀的坏 ISO 也被额外纳入。 | [developer.rs](../rust/crates/dvda-cli/src/developer.rs) 行 447–466；旧 CLI `Program.cs:656–660、710–718` 保留前缀及配置材料。两种差异均经真实 CLI 复现。 |
| R06 | P2 | **GUI 把自动派生设置固化。** 原来留空的 ISO 前缀、导入 MLP 目录被计算值填满；修改光盘名称或工作目录后，仍保存旧前缀、旧目录。 | [desktop/main.rs](../rust/crates/dvda-desktop/src/main.rs) 行 2769、2777、3273；旧 `MainForm.cs:245、266` 保留原始空值。 |
| R07 | P2 | **GUI 静默改写非法采样率/位深。** 加载 12345 Hz / 17 bit 后，保存即变成 48000 Hz / 24 bit，仅提示成功。 | [desktop/main.rs](../rust/crates/dvda-desktop/src/main.rs) 行 2787、2792、2920；旧 `MainForm.cs:256、320` 保留未知选项，`ProjectSettings.cs:96、98` 在执行前拒绝非法值。 |
| R08 | P2 | **GUI 启动参数和语言覆盖遗漏。** JSON `--config` 被忽略，转而读取默认方案；`--language en` 和 `DVDA_LANGUAGE=en` 无法覆盖日语方案。这里不涉及已删除的 config.env。 | [desktop/main.rs](../rust/crates/dvda-desktop/src/main.rs) 行 948、1300；旧桌面 `Program.cs:15、25、27、40`、`L.cs:29`。 |
| R09 | P2 | **“检查音源”漏显示失败原因。** 同组混入单声道和双声道 FLAC，报告文件有声道错误，但 GUI 摘要和详细日志都只有任务失败，没有原因。 | [desktop/main.rs](../rust/crates/dvda-desktop/src/main.rs) 行 3446；[pipeline.rs](../rust/crates/dvda-core/src/preparation/pipeline.rs) 行 235；旧 `DesktopWorkflow.cs:29` 逐条输出准备问题。 |
| R10 | P2 | **导入 MLP 的部分 Unicode 路径触发 panic。** 根为 `C:\abcd`、音源为 `C:\中中\song.flac` 时，按字节截断 UTF-8 导致 CLI 退出 101。修复后的音源可以位于原根目录外，此路径回退是正式流程需要的能力。 | [acquisition.rs](../rust/crates/dvda-core/src/acquisition.rs) 行 102，经 `build.rs:542 → mlp_import.rs:66` 调用；旧 `ExternalMlpProvider.cs:154–157` 安全回退到文件名。 |
| R11 | P2 | **导入 MLP 的 Unicode 大小写匹配不等价。** `a/Été.mlp` 与 `b/été.mlp` 未报告歧义；`Été.flac` 查找小写索引失败。 | [acquisition.rs](../rust/crates/dvda-core/src/acquisition.rs) 行 30、78 仅处理 ASCII；旧 `ExternalMlpProvider.cs:172–180` 使用 `OrdinalIgnoreCase`。 |
| R12 | P2 | **关闭续跑仍依赖续跑文件。** 配置关闭续跑且传入 `--no-resume`，锁住旧 `resume.json` 仍导致 `BUILD_FAILED`；解锁后同输入成功。 | [build.rs](../rust/crates/dvda-core/src/build.rs) 行 203、237 无条件加载/写入；旧 `BuildPipeline.cs:102` 在禁用时不访问续跑记录。 |
| R13 | P2 | **续跑记账失败阻断已完成的制作。** author 启动后锁住记录，ISO 已生成并暂存，仍因记录写失败中止发布，留下 `.pending`。旧行为是报告警告，继续发布有效成品。 | [build.rs](../rust/crates/dvda-core/src/build.rs) 行 396、[resume.rs](../rust/crates/dvda-core/src/resume.rs) 行 65、69；旧 `BuildPipeline.cs:149–155、224`。 |
| R14 | P2 | **清理失败被静默吞掉。** 锁住测试临时文件后构建成功、文件残留，却没有旧 `TEMP_CLEANUP_FAILED` 等警告。开始前的目录重置也复用忽略删除错误的函数；这部分由静态调用确认，未宣称已复现错盘。 | [build.rs](../rust/crates/dvda-core/src/build.rs) 行 650、654、656、661；旧 `DiscBuildExecutor.cs:159、177、201、220` 区分开始前失败和结束后警告。 |
| R15 | P2 | **发布回滚再次失败会删除唯一恢复备份。** 第一文件替换后被锁、第二文件提交失败、恢复目录又已存在时，备份搬迁失败，随后候选目录析构仍把备份删除。实测新 EXE/旧 ZIP 混合且备份消失。 | [publication.rs](../rust/crates/dvda-toolchain/src/publication.rs) 行 31、101–107。独立 harness 使用与当前模块相同的生产逻辑。这是迁移中新事务逻辑的缺陷；旧 C# 只有单文件原子替换，不能说旧版已提供整组回滚。 |
| R16 | P3 | **续跑记录字段类型损坏时缺警告。** 数字 signature、错误类型的身份字段、非整数盘号键均被安静忽略；顶层 null 则额外警告。实际均未错误复用缓存，影响是诊断不等价。 | [resume.rs](../rust/crates/dvda-core/src/resume.rs) 行 16–35；旧 `DiscResumeStore.cs:59–68` 类型化读取并报告反序列化失败，允许 null 作为空记录。已实际调用当前公开 `Store::load`。 |

GUI 四项通过候选单 EXE 和真实 Win32 控件复现；测试使用临时 `LOCALAPPDATA`/JSON，没有改用户日常方案，测试进程已退出。R15 使用真实 Windows 文件锁及第二次提交故障注入，生产源码未修改。所有旧行为结论来自固定基线源码，不能当作重新执行旧版所得。

## 修复验收要求

| 条目 | 关闭前必须新增的验证 |
|---|---|
| R01 | 从正式 timeline/all 入口验证常量 PTS、有效时间戳过少、异常步长比例；正常盘和合法标题重置继续通过。统计函数单测不能代替正式入口。 |
| R02–R05 | 用真实 ISO 覆盖空/缺索引、显式缺材料、同目录覆盖参数、前缀过滤及默认 manifest/log 保留；分别核对诊断、检查是否实际执行、退出码。 |
| R06–R09 | 真实 GUI 覆盖留空派生值、修改父设置、非法参数保留与拒绝、JSON 启动参数/语言优先级，以及准备错误在摘要和详细日志中的显示；中英日均能到达。 |
| R10–R11 | 从导入入口覆盖根目录外中文路径、非 ASCII 大小写匹配及重复名称歧义；正常镜像目录优先规则不变。 |
| R12–R13 | 关闭续跑时锁住记录仍可制作；启用时记录写入失败仅按旧语义警告，已验证成品正常发布，并正确处理 pending 文件。 |
| R14 | 结束清理失败保留成品并返回具体警告；开始前重置失败应停止，不能无提示复用旧目录。 |
| R15 | 注入提交失败、回滚失败、恢复目录冲突/搬迁失败组合；退出后至少保留唯一可恢复备份及准确位置，不能由析构清理掉。 |
| R16 | 覆盖嵌套字段和键类型损坏、顶层 null；核对警告和不复用结果，不只断言重新构建。 |

完成标准仍是实际入口行为等价、必要失败分支有反例、冻结编码样本保持逐字节一致。已明确排除的 C# 兼容层、config.env、GUI dry-run、外部工具回退等不恢复。此次发现说明 90 份源文件/115 个旧测试入口的映射只是审计索引，不能替代每条调用分支的验证，也不能据此给出可靠的“迁移百分比”。
