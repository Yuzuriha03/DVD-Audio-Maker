# C# → Rust 迁移剩余行为清单

更新时间：2026-10-05。状态：**报告与清单确认的迁移项目已完成本地验收**。G01–G14 共 53 个行为子项、A01–A04 四项审计，以及独立复核 R01–R16 和最终验证补查的 R17 均已闭环。R 编号是具体问题，不与关联的 G/A 编号重复计数。逐项实现与实际入口验证见 [修复验收报告](RUST-MIGRATION-RECHECK-FIXES-2026-10-05.md)、[机器可读验收记录](rust-migration-recheck-fixes-2026-10-05.json)。此结论限定于已列明并验证的范围，不作为任意输入的等价证明。

本文件逐项记录本轮确认的遗漏，供后续持续迁移和验收使用。旧实现基线为 Git `fb3cbb84788a807758fa6a8f472a9ffdad3115e8`，当前实现为本地未提交的 Rust/C 工作树。旧 C# 文件已从工作树删除，下面的 `src/...` 路径指 Git 历史，可通过 `git show fb3cbb8:src/...` 查看，不需要恢复 C# 运行层。

初次清单包含 G01–G08；逐入口复核又补迁 G09–G14。独立反例曾撤回初次“全部关闭”的结论；本轮已按关闭标准修复并重新验收 R01–R16，另补齐 R17。验收依据仍是固定旧源码的调用分支、真实入口和针对性反例，不把删除 `.cs` 或测试总数作为覆盖证明。

G01–G08 的背景段落保留初次发现时的状态。最终 Windows x64 workspace **160 项通过，0 失败、0 忽略**；116 个冻结 MLP 样本的完整文件长度/SHA-256 与解码 PCM 检查通过；最终单 EXE 的 18 个 GUI 案例、MLP/LPCM 双轨带菜单静图制作/校验及缓存修复通过。当前证据见 [机器可读验收记录](rust-migration-recheck-fixes-2026-10-05.json)；旧 85/141 项记录及原反例保留为历史。[90 份旧源文件 / 115 个旧测试入口映射](RUST-MIGRATION-ENTRYPOINTS.md) 仍只作为审计索引。旧 C# 应用/测试没有重新运行；开发期 Unicode 比较器参考与产品依赖的区别见修复报告。

## 独立复核与最终验证的修复项目

以下 R01–R16 按 [历史复核报告](RUST-MIGRATION-RECHECK-2026-10-05.md) 的关闭标准修复；R17 是最终单 EXE 验证期间发现并补齐的日志批处理遗漏。各项反例、修复入口和最终证据见 [修复验收报告](RUST-MIGRATION-RECHECK-FIXES-2026-10-05.md)。

- [x] **R01 — P1：正式 timeline/all 接回 PTS 统计。** 常量零时间戳不能通过；覆盖有效 PTS 过少和异常步长比例。关联 G09.03/A03。
- [x] **R02 — P1：无损验证拒绝空编码索引。** 不得零轨未比较就成功。关联 G09.03/A03。
- [x] **R03 — P2：缺索引恢复材料不足语义。** 明确诊断、退出码 2，其余独立检查继续执行。关联 G09.03。
- [x] **R04 — P2：CLI 拒绝显式缺失的 manifest/log。** 区分默认可选材料与用户明确指定输入。关联 G09.03/G09.04。
- [x] **R05 — P2：--iso-dir 保留配置筛选与默认材料。** 同目录参数不能跳过 manifest/log 或扩大前缀范围。关联 G09.03/G09.04。
- [x] **R06 — P2：GUI 保留派生设置的空值。** 修改名称/工作目录后，默认 ISO 前缀/MLP 目录继续随之计算。关联 G13.01。
- [x] **R07 — P2：GUI 不静默改写非法采样率/位深。** 保留输入并按旧语义校验。关联 G13.01。
- [x] **R08 — P2：GUI JSON 启动参数和语言覆盖。** 恢复 --config、--language、DVDA_LANGUAGE；不恢复 config.env。关联 G13.01/A04。
- [x] **R09 — P2：GUI 显示音源检查的结构化问题。** 声道不一致等错误必须在日志中说明。关联 G13.02/A04。
- [x] **R10 — P2：MLP 导入路径安全处理 Unicode。** 根目录外中文路径不得 panic；保留文件名回退。
- [x] **R11 — P2：MLP 导入保留 Unicode 大小写语义。** 非 ASCII 文件名回退及歧义检查按旧规则工作。
- [x] **R12 — P2：关闭续跑后不依赖续跑记录。** 锁住 resume.json 不得影响 --no-resume。关联 A03。
- [x] **R13 — P2：续跑记账失败恢复为警告。** 不阻断有效成品发布，正确处理 pending。关联 A03。
- [x] **R14 — P2：恢复清理失败诊断。** 结束清理警告；开始前目录重置失败停止。关联 A03。
- [x] **R15 — P2：发布回滚失败仍保留恢复备份。** 恢复目录冲突/搬迁失败不得被析构清理掉唯一备份。关联 G08.01/G08.02/A03；属于迁移中新事务逻辑缺陷。
- [x] **R16 — P3：续跑记录嵌套字段类型诊断。** 类型损坏警告及 null 空记录语义保持一致。关联 G14.01。
- [x] **R17 — P2：恢复 GUI 批量日志处理。** 最终单 EXE 验证发现逐条投递/重绘不适用于上万条进度记录；恢复有界队列与定时批量刷新，完整归档、暂停显示、结束刷新和取消关闭保持正确。关联 G13.03/A04。见 [修复记录](RUST-MIGRATION-RECHECK-FIXES-2026-10-05.md)。

## G01：字体生成、检查和开发入口

旧入口：`src/DvdaMaker.FontTool/OpenTypeFontTool.cs`、`src/DvdaMaker.FontTool/Program.cs`，以及 `src/DvdaMaker.Toolchain/Program.cs::PackMenuFonts`。

当前入口：[dvda-toolchain/src/main.rs](../rust/crates/dvda-toolchain/src/main.rs) 的 `copy_font_directory`、`copy_first_font_directory` 和 `package`。它们只复制已有 OTF/TTC，要求目标 TTC 存在，并直接生成 SC/JP/KR 字体索引。目前候选包使用既有 TTC，未证明可以从原始字体重新生成。

- [x] **G01.01 — 字体结构解析。** 恢复 SFNT/TTC face、表目录、name、cmap format 4/12 的读取和范围检查，为以下功能提供共同实现。验收：正常 OTF/TTC 可读；截断头、越界表、非法 face 数量和损坏映射被拒绝。
- [x] **G01.02 — 单 face 检查。** 恢复 `InspectFace` / `VerifyFace`：family、PostScript 名称、汉字/假名/谚文/拉丁代表字符覆盖、全字体校验和；单 face 入口拒绝 TTC。验收：错误 family、缺失各代表字符和错误 checksum 分别有反例；不把四个代表字符测试描述成全部 Unicode 字形验证。
- [x] **G01.03 — 从集合提取三个区域字体。** 恢复 `ExtractNotoCjkFaces`：按 family 唯一选择 SC/JP/KR，重写表偏移、校验和与 `checkSumAdjustment`，原子写入 OTF，返回原 face 编号和名称。验收：正常提取后可重新解析/渲染；缺少或重复 family 被拒绝；失败不留下半写入的目标文件。
- [x] **G01.04 — 合并及共享表去重。** 恢复 `PackNotoCjkFaces`：按 SC/JP/KR 顺序合并，按表名/内容识别相同表，哈希相同时继续比较实际内容，保持四字节对齐。验收：合并前后每个 face 重建后的内容逐字节相同；重复表确实复用；不同区域字形不被替换；输出原子提交。
- [x] **G01.05 — 集合校验。** 恢复 `VerifyNotoCjkCollection`：恰有三个 face、顺序与 family 正确、代表字符覆盖完整，各 face 可重建。验收：错序、缺 face、多 face、名称错误、覆盖不足和表损坏分别被拒绝。
- [x] **G01.06 — 接回发布打包。** 提供三份原始 OTF 时生成 TTC，提供已有 TTC 时先校验；将 type.xml 指向已核验的 face。打包目录只保留需要的集合，不能把 OTF 副本也全部嵌入；只清理本次暂存副本，保留输入目录。验收：从原始 OTF 和预制 TTC 两条路径均可打包，不依靠旧 release 目录；包内只有一份必要字体集合。
- [x] **G01.07 — 恢复开发用字体命令。** 旧 `dvda-font extract` / `verify` 的功能接入 Rust 开发工具，提供参数、输出和非零失败状态。无需另发用户 EXE。验收：通过命令入口运行 G01.02/G01.03 的成功和失败样本，不能只测私有函数。

实现可使用 Rust 或已有 C17 边界，不引入 C#、外部字体命令行程序或新的用户运行依赖。

## G02：打包前核验原生输入

旧入口：`src/DvdaMaker.Toolchain/Program.cs::ValidateMediaRuntime/ValidateFormatsRuntime/ConsolidateSharedMedia`、`NativeImagePackager.cs::ValidateRuntime/ValidateAuthor/ValidateBinary`、`NativeToolOptimizer.cs::ReadImports`。

当前 `dvda-toolchain` 按固定名称复制文件并比较同名内容；[runtime.rs](../rust/crates/dvda-core/src/runtime.rs) 负责归档和释放后的完整性。二者没有替代旧打包入口的来源清单、PE 架构和导入关系检查。现有 Python 原生构建脚本已有部分 PE/依赖检查，不能写成“仓库完全没有”；缺口是打包已有产物时没有再次核对。

- [x] **G02.01 — 媒体输入清单。** 核对 `media-build.json`、必要 DLL、实际 DLL 集合、合法文件名、长度和 SHA-256。验收：清单缺项、文件缺失、同长度内容替换、额外未登记 DLL 和不合法路径均被打包入口发现。
- [x] **G02.02 — C17 格式库输入。** 核对 `formats-build.json` 与 `dvda-formats.dll` 的长度/哈希、Windows x64 架构和允许的系统依赖。验收：错误 DLL、旧记录、32 位 DLL和非允许依赖均失败。
- [x] **G02.03 — 图像库输入。** 核对 `image-build.json`，保持一个自包含 `dvda-image.dll` 的约束、文件身份、原生 x64 属性和系统依赖限制。验收：多出 DLL、托管 PE、32 位 PE、缺记录和非系统依赖分别失败。
- [x] **G02.04 — author 与菜单/校验库输入。** 核对 `author-build.json` 中的 EXE、`runtime_files`、实际 DLL 集合、名称、长度和哈希；确认已从源码构建且使用进程内菜单库。验收：缺 `dvda-menu-spu.dll`、`dvda-menu-nav.dll`、`dvda-disc-verify.dll`，记录与文件不一致或错误 linkage 均失败。
- [x] **G02.05 — PE 导入与完整依赖集合。** 覆盖普通导入和延迟导入、合法字符串/RVA 边界、x64 原生属性；将显式加载的菜单/校验模块也纳入根集合，按组件原有规则区分系统 DLL/API set 与包内 DLL。验收：普通/延迟导入各有缺依赖反例，损坏 PE 被拒绝，合法 Windows 系统依赖通过；通过实际打包入口验证。
- [x] **G02.06 — 共享 FFmpeg 构建配置。** 接回媒体端 `profile=shared` 与 author 端对应源码共享 profile/linkage 的核对，并保留当前同名 DLL 内容一致检查。验收：文件名相同但内容不同失败；内容碰巧一致但构建记录不是共享 profile 也不能静默通过；两端同源构建成功且包内不重复。

构建记录用于开发期核验，不因此重新塞进用户 EXE 或发布 ZIP。无需恢复旧 ImageMagick 转发器、便携包瘦身哈希表或外部 FFmpeg 编码兼容分支。

## G03：准备清单与成品的独立总轨数核对

旧入口：`src/DvdaMaker.Building/DiscVerification.cs::QuickCheck/ReadManifestTrackCount`；GUI `DesktopWorkflow` 向审计入口传入存在的 `ManifestPath`。

当前 [app.rs](../rust/crates/dvda-core/src/app.rs) 的 `verify_job/run_verify` → [verify.rs](../rust/crates/dvda-core/src/verify.rs) 的 `Job/run/timeline`，有编码索引与 IFO 的比较，没有读取准备 manifest。

- [x] **G03.01 — manifest → 全部 ISO 总轨数。** 从准备清单各专辑的 `files` 数量得到独立预期，与当前筛选的所有成品 IFO 总轨数比较，接回 GUI/CLI 正式验证。验收：索引和 ISO 同时少一轨、同时多一轨时仍被发现；多盘合计正确；清单存在但损坏不能静默略过；清单不存在时按旧可选参数语义处理，不把它当作零轨。正常索引/音频验证继续执行。

## G04：构建日志审计接回正式验证

旧入口：`src/DvdaMaker.Building/DiscVerification.cs::Audit/SelectLatestBuildLog/ParseAuditLog/ReadPaddingIssues/MarkUnavailable`，由旧 GUI `DesktopWorkflow` 实际调用。

当前 [verification.rs](../rust/crates/dvda-core/src/verification.rs) 有日志解析器、ANSI 去除和最近正式段落选择，但仅开发分发/单测调用；`verify.rs::Job/run` 没有构建日志输入。读取实际 IFO 已覆盖一些结构检查，不等于恢复以下诊断。

- [x] **G04.01 — 日志来源选择。** 接入配置的构建日志路径，恢复旧候选路径 `build.log`、`rebuild-final.log`、`finalrebuild.log`，存在文件去重后按修改时间选择；保留禁止回退时只用指定路径的语义。验收：指定日志、仅回退日志、多个新旧日志和全部缺失。
- [x] **G04.02 — 正式入口使用已有解析器。** 将 ANSI 清理、最近正式构建段落、author 命令参数/引号路径、每盘曲目表解析接到验证流程。验收：混合多次构建、开发 dry-run、中文/空格路径及多组多盘日志；使用当前真实 `build.log` 检查，不能只用手写字符串。
- [x] **G04.03 — 缺日志的不可完成状态。** 恢复 `BUILD_LOG_MISSING`；材料缺失不能显示该审计已通过。验收：删去测试副本的所有候选日志后，GUI/CLI 正确报告不能完成这部分检查。
- [x] **G04.04 — 缺曲目表。** 恢复 `TRACK_TABLE_MISSING`。验收：有日志但没有有效轨道行时报告材料不足，不能得到空表通过。
- [x] **G04.05 — 缺 author 命令。** 恢复 `DVDA_COMMAND_MISSING` 分支，并核对与 G04.04 的检查顺序和解析器可达性。验收：使用旧语义相同的输入/解析结果检查该分支，不能为了覆盖率改变旧解析含义。
- [x] **G04.06 — 日志与 ISO 映射。** 根据 ISO 盘号匹配 author 输出盘标识，保留旧位置回退语义；无法关联时报告 `DISC_LOG_MAPPING_MISSING`。验收：盘号排序、输出标签、多盘缺一条命令和无法映射的 ISO。
- [x] **G04.07 — 日志/IFO 每盘轨数。** 恢复 `DISC_TRACK_MAPPING_MISMATCH` 和材料不足状态。验收：日志少轨、多轨、映射到错误盘的反例，且不会仅因编码索引与 IFO 自洽而通过。
- [x] **G04.08 — PES 补齐失败诊断。** 恢复 ANSI 清理后的不区分大小写 `pes_padding length must be higher` 检测，并报告 `PES_PADDING_FAILED`。验收：原始、变换大小写和带 ANSI 的失败日志，以及无该错误的日志；退出码成功也不能掩盖该审计诊断。

以上各项在真实 GUI/CLI 中可见，已知用户诊断接入中、英、日资源。日志缺失/材料不足与已发现成品损坏可以使用当前结果类型表达，但不能都被当作成功。恢复审计时继续复用已有 AOB 扫描结果，避免为相同检查重新全盘读取。

## G05：轨道扇区关系

旧入口：`src/DvdaMaker.Building/DiscVerification.cs::Audit` 中按组排序的轨道表检查。

当前 [ifo.rs](../rust/crates/dvda-core/src/ifo.rs) 检查单条范围；`verify.rs::timeline` 检查越界和 PTS；[aob.rs](../rust/crates/dvda-core/src/aob.rs) 的 `audit_diagnostics` 检查 PTS 回退位置。这些没有包含以下两条关系。

- [x] **G05.01 — 相邻轨道连续。** 恢复相邻轨道 `first == previous.last + 1`，异常报告 `TRACK_GAP`。验收：中间空一扇区、重叠一扇区及正常相接；覆盖多标题同组。
- [x] **G05.02 — 整组 AOB 总量。** 恢复组内 AOB 实际扇区数等于轨道最大 `last + 1`，异常报告 `AOB_SECTOR_MISMATCH`。验收：多余尾扇区、少尾扇区、跨多个 AOB 文件的正确总量；不以“所有轨道地址都未越界”代替相等比较。

旧数据源是日志轨道表，现有 Rust 又能读 IFO；恢复原检查，并核对两种来源的关系。音频无损校验继续保留，不能宣称上述缺口意味着所有相关坏盘都会逃过其余检查。

## G06：静图/菜单异常分支

旧入口：`src/DvdaMaker.Building/MenuBuildVerifier.cs::VerifyAsync`、`MenuDiscVerifier.cs::Verify/ValidateAsvs`。

当前入口：[menu_check.rs](../rust/crates/dvda-core/src/menu_check.rs) 的 `verify`、[menu_verify.rs](../rust/crates/dvda-core/src/menu_verify.rs) 的 `stills/verify`。

- [x] **G06.01 — 制作阶段静图文件警告。** 配置了有效播放封面而 `AUDIO_SV.VOB` 缺失或为零字节时，恢复 `MENU_STILL_VOB_MISSING` 警告。验收：缺文件、空文件、有内容三种情况；在发布前菜单检查中可见，严重程度维持旧警告语义，不能以稍后用户手动验证替代。
- [x] **G06.02 — ASVS 固定头最小长度。** 恢复 `AUDIO_SV.IFO` 至少为 `0x60` 字节的检查。当前 `stills` 在零记录时只读到偏移 20 的字段。验收：构造 24–95 字节、记录数为零、扇区字段恰好自洽的输入，仍应报 `ASVS_TOO_SHORT`；不能只测试记录表越界。
- [x] **G06.03 — 无菜单预期时的必要静图文件。** 旧 `expectation == null` 时要求 `AUDIO_SV.IFO` 和 `AUDIO_SV.VOB`；当前没有预期且没有 VOB 时直接得到零静图。验收：菜单预期缺失时分别缺 IFO、缺 VOB、二者都缺；预期明确静图数为零时仍允许无静图，正数时要求文件和数量匹配。

**已排除的误判：** 单页菜单 Next 检查不是 C# 迁移遗漏。旧 `Verify` 仅在 `menuPages > 1` 时调用 `ValidateMenuCellChain`，后者开头还有 `if (menuPages <= 1) return;`。当前 Rust 也只对多页检查 Next。初次只看局部分支得出的相反判断已撤回；不将其计入未迁移项。若未来强化单页结构校验，应另记为新改进。

## G07：开发打包参数和输入选择

旧入口：`src/DvdaMaker.Toolchain/Program.cs::Parse/PackageAsync`。当前 `dvda-toolchain/src/main.rs::run/package` 未覆盖以下仍有用途的接口。

- [x] **G07.01 — 格式库目录参数。** 恢复 `--formats-runtime <path>`，允许打包指定的 C17 构建产物，不能只能从固定 `build` 目录二选一。验收：两个不同构建目录可显式选择；缺值/缺文件失败；选中产物通过 G02.02 校验。
- [x] **G07.02 — 开发环境路径覆盖。** 恢复 `DVDA_SRC_TREE`、`DVDA_PREBUILT_DIR` 的读取，命令行明确参数优先。它们是构建工具环境变量，不是被删除的 `config.env`。验收：仅环境变量、命令行覆盖环境变量、路径带空格/中文。
- [x] **G07.03 — 正常源码/预构建目录默认选择。** 旧默认使用仓库内 `tools/dvda-author-mlp8` 与 `tools/win-build/prebuilt`。当前字体/素材在未传参时回退到旧 release 目录；恢复可从明确源码素材和当前产物组装的路径。验收：无旧 release 缓存、只有合规原始输入和当前原生产物的环境也能打包；错误信息指向真实缺失输入。源字体缺失时明确要求提供字体，不能凭空生成。
- [x] **G07.04 — 发布版本参数校验。** 恢复 `v1.0`、`v1.1.0` 及旧允许预发布后缀的约束；当前任意字符串直接进入 ZIP 文件名。验收：合法版本正常命名；缺值、空值、分隔符或其他非法版本在改写输出前被拒绝。

不恢复 .NET 自包含/框架依赖选项，也不把开发 CLI 放进精简用户包。无意义的历史别名无需伪造底层功能。

## G08：发布产物提交与哈希记录

旧入口：`src/DvdaMaker.Toolchain/Program.cs::PublishOneFileAsync/WriteManifest`。旧 onefile EXE 与 ZIP 使用临时文件完成后替换，输出 SHA-256 及 `MANIFEST.txt`。

当前 `dvda-toolchain::package` 先删除上次 stage，`compress_zip` 对正式 ZIP 直接写入，打包成功只输出路径。运行时释放缓存的原子修复是另一条路径，不能视为本项已迁移。

- [x] **G08.01 — 最终 EXE 提交。** 恢复先构建/校验候选，再原子替换正式 EXE；失败不丢失已有可用产物。验收：构建失败、复制失败和成功替换；旧最终 EXE 在失败时保持原哈希。
- [x] **G08.02 — 最终 ZIP 提交。** 在目标目录内写临时 ZIP，成功完成后替换；失败清理自己的临时文件，保留旧 ZIP。验收：压缩失败/中断后旧包完整，成功包含预期根文件且可解压。不能通过更换压缩格式来代替该行为。
- [x] **G08.03 — 发布散列记录。** 恢复打包工具自动生成可追溯的最终文件 SHA-256 清单和 EXE 哈希输出。当前手工验证 JSON 只证明一次候选包，不是打包工具能力。验收：记录与实际文件逐项一致，文件变化后记录变化。开发构建/组件 JSON 仍不进入用户包；清单保留位置须在发布布局文档中明确，不因本项把无关资料内嵌。

## 全面核对中发现并完成的补迁

### G09：开发 CLI 和验证子模式

- [x] **G09.01 — 配置、计划、制作。** 恢复来源诊断、缺值退出码、派生值 shell 导出、`plan`、`prepare --force`、`build --dry-run/--no-resume`。启动脚本固定 x64 并定位当前原生组件。
- [x] **G09.02 — 格式开发命令。** 恢复 `convert/m4a2flac`、`alac check/repair`、`mlp --check/--align`、`iso list/extract`、`aob-pts`。MLP 对齐仅限开发者显式操作，正式编码不调用它修补输出。
- [x] **G09.03 — 验证子模式。** 恢复 `all/quick/capacity/audit/menu/timeline/lossless/config`、单独 `quick-check/audit`、卷标/根目录核对、菜单关闭跳过、显式 ISO；timeline 不依赖日志；损坏返回 1，仅材料不足返回 2。
- [x] **G09.04 — 实际入口。** JSON 方案、重复/缺失参数、非法语言、实际短音频制作与验证、dry-run 无 ISO；三份开发启动脚本九组分支与失败阻断通过。

### G10：独立 M4A → FLAC

- [x] **G10.01 — 输入、标签和封面。** 递归去重、只接受 ALAC、原值/改名规则、END 修复副本、封面字节及 PICTURE 描述规范化。
- [x] **G10.02 — 转换和验证。** 有界并发、压缩参数、dry-run 不写文件；旧 S16 MD5 加完整精度 PCM 比较后原子发布。
- [x] **G10.03 — 失败、取消和删除。** 单文件失败继续批次；保留源和旧目标，清理临时目录；显式 `--in-place` 仅在全部成功后删除源，单个删除失败继续其它文件。

### G11：FLAC 元数据异常边界

- [x] **G11.01 — 有界解析。** comment 数目/长度、PICTURE 字段、UTF-8、尾随数据和每个截断点返回错误；缺少可选块维持旧语义。
- [x] **G11.02 — 写入保护。** 自有 `create_new` 临时文件、同步、原子替换；占用目标失败保留原文件且清理临时文件，音频与其它块不变。

### G12：运行期菜单字体与素材

- [x] **G12.01 — 字体覆盖和回退。** 所需脚本、真实渲染墨迹、候选回退、JP/KR 区域字体、缺字诊断接回实际构建。
- [x] **G12.02 — 素材异常。** 必要素材缺失报错；辅助素材缺失警告；无封面黑背景和 warning；索引缺缩略图报错；播放静图生成失败维持 warning，索引按实际生成数量记录。
- [x] **G12.03 — 验证可达性。** 构建、续跑和成品验证使用一致预期；已知诊断中英日齐全，实际图像和 ISO 反例通过。

### G13：GUI 行为

- [x] **G13.01 — 设置与容量。** 旧 34 个有效设置逐项映射；DVD9 恢复 `8540123136` 字节，自定义范围及原值保留。
- [x] **G13.02 — 日志展示。** 摘要过滤工具进度、保留 fatal/告警/路径错误及建议；详细日志不受“只看提醒”过滤；原始路径和参数保留。
- [x] **G13.03 — 完整日志。** 每任务重置、时间戳、有界内存、完整归档导出；拒绝覆盖正在写的日志，暂停不丢记录，结束强制刷新；设置/日志分隔条可拖动。
- [x] **G13.04 — 生命周期。** 忙时锁定设置/方案/语言，取消只触发一次，关闭时取消并等任务结束；恢复控件。实际 Win32 控件和三语言 GUI 通过，见 [专项审计](RUST-GUI-BEHAVIOR-AUDIT-2026-10-05.md)。

### G14：缓存与 MLP 发布

- [x] **G14.01 — 缓存读写。** 损坏 MLP/续跑缓存警告、类型校验、Windows 大小写无关路径查找/更新；原子写入失败保留旧缓存。
- [x] **G14.02 — 编码目标替换。** 不预删旧 MLP，现有 Win32 覆盖移动支持跨卷暂存；失败保留旧目标；编码目标路径匹配已修复 UTF-8 切割；导入路径另由 R10/R11 补齐并验收。
- [x] **G14.03 — 反例和实际跨卷。** 缺源/占用、坏缓存/重复键、实际采集 warning、各身份字段失效、D: → C: 替换和字节检查通过。

## 不属于未迁移项的内容

| 功能 | 处理依据 |
|---|---|
| `config.env` 读取/导入与旧 env 配置编辑 | 用户明确要求删除，改为 JSON；G07 的构建环境变量不恢复该配置格式。 |
| C# host/FFI 回退、.NET 运行时和 WinForms 类本身 | 用户要求最终删去 C#；要保持操作行为，不要求类名、句柄或像素逐一相同。 |
| GUI dry-run | 用户要求仅开发调试使用；开发入口和日志识别仍需覆盖。 |
| 外部 FFmpeg MLP 分支、原版 SurCode/eac3to 进程、旧 ImageMagick 命令转发兼容层 | 按用户要求移除，不为“类清单一致”重新加入。 |
| 旧便携 ImageMagick/FFmpeg 包的硬编码哈希瘦身方案 | 当前采用源码构建、必要文件清单与共享库；需迁移有效的输入验证，不恢复过时包的删除规则。 |
| 已迁入 C17 的 `MlpStreamAligner`、`PcmComparer`、格式原生功能 | 继续使用现有 C 实现，不再翻译成 Rust 或重新引入外部兼容实现。 |
| MLP 核心反编译来源替换 | 用户明确排除；本清单不改算法来源，也不允许补丁编码文件来伪造字节一致。 |
| 用户发布包中的开发 CLI、源码、构建 JSON、运行时清单文件 | 保持精简 onefile + 用户文档/许可布局。开发工具保留在源码开发流程。 |

## 入口归档与全面核对

核对结果已归档：90 份旧 C# 源文件及成员索引、115 个旧测试入口均有当前实现/证据或明确排除依据。G09–G14 来自私有调用链复查；R01–R16 及补查 R17 已补齐正式入口、异常分支与 GUI 回归，A03/A04 在本次列明范围内重新验收，见 [修复验收报告](RUST-MIGRATION-RECHECK-FIXES-2026-10-05.md)。

- [x] **A01 — 旧功能入口逐项归档。** 从固定 Git 基线列出 C# GUI/CLI、准备、构建、成品验证、字体工具和打包工具入口；每项注明当前 Rust/C 对应入口、已有证据、上述缺口编号或用户排除依据。对旧调用栈中的私有辅助分支继续追踪，不能只按文件名或公开函数统计。
- [x] **A02 — 历史测试与当前测试映射。** 逐项核对旧兼容性入口、当前 Rust 测试和冻结样本覆盖。原始 C# 结果、此次静态发现、新执行的测试分别记录；不能用 85 项测试总数或 116 个 MLP 样本数抵消未覆盖行为。
- [x] **A03 — 失败与取消路径。** 在上述映射中核对缓存失效、输入缺失/损坏、临时文件、只读/访问失败、取消和恢复、诊断严重级别及 GUI/CLI 返回结果。已有专项证据可以复用；未实际验证的路径标为待验证，不写成通过。
- [x] **A04 — 三语言与 GUI 可达性。** 对新增/恢复的用户诊断验证中英日资源，检查功能能从 GUI/CLI 正式入口到达。底层函数存在、资源键存在或历史对照通过均不能单独替代实际入口证据。

## 后续执行与完成标准

以下表格保留前轮各领域的实现和通过证据（其中测试计数是当时记录）。本轮新增 R01–R17 的最终证据另见 [修复验收报告](RUST-MIGRATION-RECHECK-FIXES-2026-10-05.md)，不把新结果写进旧快照。

| 条目 | 实现与验证 |
|---|---|
| G01 | `dvda-toolchain/src/fonts.rs` 八项测试；15 个实际 CLI 案例；SC/JP/KR 提取字节与原 OTF 一致，渲染 RGBA 一致。TTC 容器的 `checkSumAdjustment` 差异不冒充容器逐字节相同。 |
| G02/G07/G08 | 工具链 18 项测试；真实 PE/清单/版本/输入失败保留；TTC 与原始 OTF 两条打包路径；最终 ZIP/内嵌组件/外部散列逐项核验。 |
| G03–G06/G12 | `application_native.rs` 五项实际入口回归及 `verification/menu_fonts/menu_verify` 单测；真实 IFO/日志/AOB/静图/字体/缺素材反例。G04.05 是旧解析器顺序下的防御分支，直接合法结构边界测试，没有篡改解析器。 |
| G09/G10 | `developer_commands.rs`、`conversion_native.rs`、配置/菜单/事务发布回归、`test-rust-build-entrypoints.py` 九组实际脚本案例。 |
| G11/G14 | FLAC 四项、原生格式五项、缓存/MLP/续跑测试；真实文件占用失败和 D: → C: 替换。 |
| G13/A04 | 桌面 12 项测试、三语言真实 GUI；仅复制 EXE 后执行 MLP/LPCM 带菜单静图的制作和成品验证，并验证缓存 DLL 修复。 |
| A01/A02/A03 | [入口映射](RUST-MIGRATION-ENTRYPOINTS.md) 与机器可读成员清单、历史测试逐项对应；失败/取消/损坏/回滚/缓存/恢复均在当前回归中执行。 |

保持 Windows x64、原有 MSYS2 GCC 构建链、Rust 应用层 + C/C17 原生组件。现有 Python 原生源码构建脚本可以继续使用，不把“已有工具不叫 Rust”当作新迁移目标。需要新格式底层代码时优先评估已有 C17 组件，避免增加重复实现和原生运行依赖。

完成后运行与改动匹配的测试；冻结 MLP 完整文件对照不得更新成新输出掩盖差异。纯打包变更无需重跑用户全盘制盘，使用原生输入反例、打包失败恢复和独立 EXE 冒烟验证即可；验证流程变更须有相应 ISO/日志反例。全部清单与审计任务闭环后才可更新迁移完成状态。GitHub 提交、tag/release 状态单独记录，不把本地文档更新写成已发布。
