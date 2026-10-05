# 独立复核问题的修复与验证

本记录对应 [复核报告](RUST-MIGRATION-RECHECK-2026-10-05.md) 的 R01–R16，以及最终单 EXE 验证期间补查的 R17，不改变用户明确排除的迁移范围。旧行为基线仍为 `fb3cbb84788a807758fa6a8f472a9ffdad3115e8`。旧 C# 应用和兼容性测试仅用于源码对照，没有恢复或重新运行。开发期另用隔离的 .NET 字符比较器核对 Unicode 数据；临时参考程序不进入源码、构建或发布包。

状态：**R01–R17 全部完成本地验收**。关联的原行为条目与 A03/A04 已重新关闭；机器可读证据见 [验收记录](rust-migration-recheck-fixes-2026-10-05.json)。结论限定于报告与清单列明的行为及已执行案例，不是任意未知输入的等价保证。

| 问题 | 修复行为 | 针对性验证入口 |
|---|---|---|
| R01 | 正式 timeline/all 在同一次 AOB 扫描中收集并检查 PTS 数量、是否推进、异常步长比例，保留合法标题重置规则。 | `developer_recheck::formal_timeline_and_all_check_pts_statistics_and_legal_title_resets`：真实双标题 LPCM ISO；正常输入、常量 PTS、有效 PTS 过少、异常步长。 |
| R02 | 空盘/空组/空音轨索引不能零轨成功；显式 ISO 未匹配到可用音轨时也返回材料不足。 | `developer_recheck::index_material_failures_do_not_skip_all_independent_checks`。 |
| R03 | 缺失、损坏及 dry-run 索引有明确诊断与 Unavailable；单独材料不足返回 2，真实损坏返回 1；All 继续执行不依赖该索引的检查。 | 同上；同时构造异常 PTS、manifest 不匹配和坏 ISO，核对各诊断实际出现及退出码优先级。 |
| R04 | 显式指定不存在的 ISO 目录、manifest、日志时，CLI 在检查前拒绝并返回 2。 | `developer_recheck::explicit_verification_material_paths_must_exist`。 |
| R05 | --iso-dir 只覆盖目录，保留配置前缀和默认 manifest/log。 | `developer_recheck::iso_directory_override_preserves_prefix_and_configured_evidence`：加入无关坏 ISO、错误总轨数及日志补齐错误。 |
| R06 | GUI 保留前缀/MLP 导入目录的原始空值，让它们随名称和工作目录重新计算；非空显式环境覆盖仍有效。 | `test-rust-gui-recheck.py`：打开、编辑、保存、关闭后重启，以及显式值/环境覆盖。 |
| R07 | 下拉框保留未知采样率和位深；保存、另存及自动保存不静默修改它们，开始任务时按旧规则拒绝非法值。 | 桌面单测与真实 GUI：12345/17 保存后不变，任务被拒绝，用户选择合法值后才改变。 |
| R08 | 恢复 JSON --config、--language 和 DVDA_LANGUAGE；命令行语言优先，未知/缺值参数拒绝。 | 桌面参数单测与真实 GUI 启动/错误对话框测试；不恢复 config.env。 |
| R09 | 准备任务逐条输出结构化问题，摘要、详细、只看提醒及完整归档均可见；复合声道统计逐项完整翻译。 | 真实 GUI：同组单/双声道 FLAC，核对中英日标题、完整原因和 Unicode 路径；窄格式匹配单测避免拆分用户路径。 |
| R10 | 按路径组件比较根目录，不按 UTF-8 字节截断；源文件位于根外时安全回退到文件名。 | `acquisition_parity`：实际 CLI、中文根外路径、空根、Unicode 镜像目录及输出规则。 |
| R11 | 使用冻结的 .NET 序数大小写映射实现匹配、歧义排除及排序，补齐 Windows API 不支持的希腊变体和补充平面字母。199 个区间存入 Rust，不增加运行依赖。 | `acquisition_parity`：重复变体、非 ASCII 名称回退、补充平面镜像/歧义、希腊变体及排序；`Straße` 与 `STRASSE` 不误合并。原生库测试遍历 1,112,064 个有效 Unicode 字符，核对冻结映射摘要。 |
| R12 | 关闭续跑或保留中间产物时不加载、计算或写入续跑凭据，使用独占暂存目录。 | `application_native::disabled_resume_and_intermediate_mode_ignore_locked_old_record`：锁住旧记录仍可制作并通过完整验证，旧记录字节不变。 |
| R13 | 续跑记录写失败保留警告与正式日志，继续发布已验证 ISO；pending 清理守卫覆盖失败/取消早退。 | `application_native::resume_write_failure_warns_and_publishes_verified_iso`：author 开始后真实锁记录，检查成功发布、警告、build.log、无损验证和 pending 清理。 |
| R14 | 结束清理失败返回目录/ISO 警告；开始前重置失败停止，不能使用未清理目录。 | `application_native::cleanup_failure_warns_and_next_build_rejects_dirty_workspace` 与 `build::tests::locked_iso_cleanup_warns_and_preserves_file_until_unlocked`：真实文件锁、旧成品保护、解锁恢复及三语言消息。 |
| R15 | 发布前把备份放入独立且唯一的恢复目录；回滚失败保留该目录并报告路径，不再依赖失败后搬迁，也不受候选目录析构影响。 | `publication::tests`：真实 EXE/ZIP 文件锁、已有恢复目录冲突、候选清理后备份仍存在且能够恢复。此项是新增事务逻辑修复，不声称旧 C# 有整组回滚。 |
| R16 | 续跑记录校验已知字段和盘号键类型；损坏时警告且不复用，null 保持空记录语义，未知字段保留。 | `resume::tests::typed_resume_record_failures_warn_and_null_remains_empty`：公开读取入口覆盖多种嵌套字段错误、null、未知字段。 |
| R17 | 日志和完成事件共用容量 4096 的有界队列；每 100 毫秒限时批量处理，全部归档后结束；保留暂停/恢复、导出已有排队日志、取消/关闭和最终刷新。关联 G13.03/A04。 | `real_controls_busy_cancel_bounded_archive_and_paused_completion`：12050 条记录逐条有序归档、暂停/恢复、尾部多行警告、完成及有界显示；`log_queue_applies_backpressure_and_keeps_completion_after_diagnostics`；最终 EXE 同一音源 MLP 完整验证 1.522 秒，LPCM 0.654 秒。 |

原有测试修正了前置条件，没有删除断言或放宽验证规则：开发 CLI 的制盘成功样本原来只有 2 个 PTS，换成生成的 1 秒 PCM→FLAC，原 ALAC 转换/修复测试仍保留；损坏续跑记录测试显式关闭 KeepIntermediate，以满足旧版启用续跑的条件。原 quick/audit 测试补入同一 JSON profile，避免依赖默认项目名前缀。单 EXE 两轨样本从每轨约 0.5 秒延长到 4 秒，仍保留非整 AU 和两标题：旧版统计会把合法标题重置计入异常步长，原短样本 1/23 超过 1% 阈值，不能作为成功样本；生产校验阈值保持不变。

验证使用自建音频、短 ISO、临时 JSON 和真实 Windows 文件锁。冻结 MLP 基准及编码核心保持不变；未重跑用户音乐库全盘，未提交或更新 GitHub tag/release。

Unicode 映射来自本机 .NET 10.0.12 的实际序数规则，开发参考执行了 6,672,384 次公开比较器核对；固定全部有效字符映射的 SHA-256 为 `985D1B540474D50B01401A5D5277F8CC225041A0C8019B24C5A2A5C0877DB6FE`。数据与生成器在 `tools/win-build/devdata/ordinal-case` 和 `generate-ordinal-case.py`，从冻结数据重新生成仅需要 Python；应用和常规构建不需要 .NET。此证据覆盖有效 Unicode 字符，不能扩展为任意无效 UTF-16 文件名的行为保证。

R17 的真实触发证据是同一双轨样本在旧队列实现中超过 180 秒仍未完成 GUI 验证，归档和完成通知受阻；对应 ISO 经正式 CLI `verify all` 0.601 秒通过，原生编解码和音频数据均正常。恢复批量队列后最终 EXE 完整验证通过。旧 C# `MainForm.cs` 使用 100 毫秒定时批量日志处理；本次修复保留这一交互行为，不减少底层验证、不丢弃进度记录，也不修改 MLP 字节输出。

并行回归曾暴露测试临时目录仅依赖时钟时可能重名。相关测试目录加进程内原子序号，避免不同测试相互覆盖；断言与生产路径不变。

## 最终验收

| 检查 | 结果 |
|---|---|
| Windows x64 workspace，包含原生及实际 D: → C: 跨卷案例 | 160 项通过，0 失败、0 忽略。 |
| 冻结 MLP 基准 | 116 个样本的完整文件长度、SHA-256 与解码 PCM 全部通过；未更新基准。 |
| 格式与静态检查 | `cargo fmt --all -- --check`、workspace/all-targets Clippy `-D warnings` 通过。 |
| 最终 EXE 真实 GUI | 18 个案例通过，覆盖中英日、派生值、非法值、启动参数、准备问题和完整归档。 |
| 只复制单 EXE 到隔离目录 | 生成双轨音源，MLP/LPCM 带菜单静图制作与完整成品验证通过；破坏编码 DLL 后自动恢复通过。 |
| 发布包 | 9 个 ZIP 根文件、27 个内嵌文件、10 条外部散列记录核验通过；无残留候选目录。 |
| R17 高密度日志 | 12050 条逐条归档、队列背压、完成顺序、暂停恢复及取消/关闭测试通过。 |
| Unicode 参考 | 1112064 个有效字符、6672384 次公开比较器对照，零差异；冻结数据可仅用 Python 再生成。 |

最终 EXE SHA-256：`33fa0db7e502bf2f06092bbb87d41cabda570c34b331aedf9def802310e759c0`。本地候选 ZIP 位于 `build/rust-recheck-fixes-package/DVD-Audio-Maker-v1.0-win-x64.zip`；本轮没有提交、推送或覆盖 GitHub 发布。
