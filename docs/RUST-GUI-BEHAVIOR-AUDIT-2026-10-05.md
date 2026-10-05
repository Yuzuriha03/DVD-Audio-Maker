# GUI 行为补充审计与迁移记录

日期：2026-10-05；旧基线：`fb3cbb84788a807758fa6a8f472a9ffdad3115e8`。本记录对应剩余清单的 **G13**，核对五个旧桌面文件的入口和辅助分支；不以控件总数代替工作流验证。

后续独立单 EXE 复核曾确认四组遗漏（R06–R09）：派生设置空值、非法音频参数保留、JSON 启动参数/语言覆盖、音源检查结构化问题。它们已修复并通过最终 EXE 的 18 个真实 GUI 案例；最终验证另补齐 R17 批量日志队列、完整归档与完成顺序。G13/A04 的当前验收见 [修复验收报告](RUST-MIGRATION-RECHECK-FIXES-2026-10-05.md)。下文保留此前测试的历史结果，不把旧计数改写为新结果。

## 本次恢复的行为

1. DVD9 预设恢复为 **8,540,123,136 字节**；其他自定义容量不误认成 DVD9。自定义输入范围恢复为 0–10,000,000,000。
2. 详细日志不受“只看提醒”过滤，详细模式禁用该筛选控件；关闭实时更新保留当前显示，任务结束强制刷新。
3. 保留 `[WAR]`、`[FAIL]`、`[FATAL]`、`fatal error`、`could not`、`no such file` 等问题分支；后台 stderr 不再仅凭来源被判为错误。结构化 PCM 进度中的文件名保持原样。
4. 恢复摘要与详细记录区分：PCM 高频进度仅更新活动说明，临时路径和普通工具进度保留在详细记录。卷标、目录、扇区提示使用三语言说明，未知失败仍可见。
5. 每次任务重置摘要/问题计数/进度，另建完整 UTF-8 日志；窗口最多保留近期 2,500 行，超过后缩到 2,000 行；导出从完整归档读取，不丢弃早期记录。
6. 日志加入本地时间戳；写入或刷新失败明确提示；拒绝把完整日志导出到正在写入的源日志。
7. 执行时锁定设置、方案和语言；取消按钮在取消后禁用；关闭窗口自动请求取消并等待工作线程完成，再关闭。成品浏览仍可用。
8. 恢复失败处理建议和成功但有提醒的状态；停止任务不会显示底层中断文本作为成功消息。
9. 恢复设置区与日志区之间可拖动的分隔条；保留缩小窗口后的设置滚动。
10. 保存方案同时保存默认方案，重新启动仍恢复当前设置；未知配置键继续保留。

## 原入口映射

| 旧入口 | 当前入口 |
|---|---|
| `MainForm.BuildEditors/BuildFields` | `create_controls / create_page / update_advanced / layout / scroll_page` |
| `MainForm.Browse` | `browse_target / folder_dialog / file_dialog` |
| `MainForm.Populate/CaptureSettings` | `populate / edited_profile_values / overrides` |
| `MainForm.OpenProfile/SaveProfile/OnClosing` | `choose_and_load_profile / save_profile / save_profile_as / autosave / window_proc` |
| `MainForm.RunAsync/SetBusy/CancelTask` | `start_operation / finish_operation / set_busy_controls / cancel_operation` |
| `MainForm.ResetLog/DrainLog/FlushArchive/ArchiveFailed` | `reset_task_log / add_log / open_session_log` |
| `MainForm.RenderLog/CopyArchive/SaveLog` | `display_log / render_log / export_log_to / export_log / full_log` |
| `MainForm.Guard/TaskLogPresentation.ErrorAdvice` | `localized_detail / presentation.error_advice / message_box` |
| `MainForm.FormatDuration` | `update_statistics` |
| `MainForm.Snapshot/CheckLogControls/RunSmoke` | `tools/win-build/test-rust-gui.py / test-rust-onefile.py / real_controls_busy_cancel_bounded_archive_and_paused_completion` |
| `TaskLogPresentation.Present` | `presentation.problem / presentation.summary / presentation.activity_only / localized_log / batch_message` |
| `TaskLogWriter.Write/WriteLine/Flush/Append/Emit` | `WorkerCallbacks.emit / add_log (callback API emits records; CRLF fragments split; full raw UTF-8 archive)` |
| `CapacityEditor.Value` | `capacity_index / overrides / validate_controls` |
| `DesktopWorkflow.RunAsync` | `start_operation -> app.run_prepare/run_build/run_verify -> emit_diagnostics` |

## 34 项设置对应

旧 `SettingDefinition.All` 的 34 个设置均有当前控件与读取/保存路径。排除的是用户要求删除的 env 导入格式，而不是这些 JSON 设置。

| 设置键 | Win32 控件 ID |
|---|---|
| `DVDA_SRC` | `200` |
| `DVDA_FINAL_DIR` | `201` |
| `DVDA_TITLE` | `202` |
| `DVDA_DISC_BYTES` | `203` |
| `DVDA_MAX_DISCS` | `204` |
| `DVDA_BUILD_DIR` | `205` |
| `DVDA_ISO_PREFIX` | `206` |
| `DVDA_GROUP_TRACK_LIMIT` | `207` |
| `DVDA_PREPARE_CACHE` | `208` |
| `DVDA_RESUME` | `209` |
| `DVDA_MLP_SOURCE` | `300` |
| `DVDA_MLP_SURCODE_SAMPLE_RATE` | `301` |
| `DVDA_MLP_SURCODE_BITS` | `302` |
| `DVDA_MLP_EXTERNAL_DIR` | `305` |
| `DVDA_MLP_JOBS` | `303` |
| `DVDA_MLP_METADATA_CONTEXT` | `304` |
| `DVDA_MLP_BATCH_TEMP_DIR` | `306` |
| `DVDA_MLP_BATCH_OUTPUT_DIR` | `307` |
| `DVDA_MENU` | `400` |
| `DVDA_MENU_TRACKS_PER_PAGE` | `402` |
| `DVDA_MENU_STILLPICS` | `401` |
| `DVDA_MENU_COVER_DIM` | `403` |
| `DVDA_MENU_INDEX_MIN_ALBUMS` | `404` |
| `DVDA_MENU_FONT` | `405` |
| `DVDA_MENU_FONT_JP` | `406` |
| `DVDA_MENU_FONT_KR` | `407` |
| `DVDA_AUTHOR` | `500` |
| `DVDA_AUTHOR_SRC` | `501` |
| `DVDA_KEEP_TMP` | `502` |
| `DVDA_KEEP_INTERMEDIATE` | `503` |
| `DVDA_LOSS_WARN_S` | `504` |
| `DVDA_LOSS_ERROR_S` | `505` |
| `DVDA_ALBUM_LIMIT` | `212` |
| `DVDA_TITLE_MODE` | `211` |

## 本次执行证据

- `cargo test -p dvda-desktop --target x86_64-pc-windows-gnu --offline`：12 通过，0 失败，0 跳过。
- `cargo clippy -p dvda-desktop -p dvda-toolchain --all-targets --target x86_64-pc-windows-gnu --offline -- -D warnings`：通过。
- `test-rust-gui.py` 实际窗口回归通过中、英、日三语言、4 个页面、方案重载/另存/取消、滚动、DVD9 保存、详细筛选和拖动分隔条；控件快照与截图在 `build/gui-audit-checklist/`。
- `real_controls_busy_cancel_bounded_archive_and_paused_completion` 创建真实 Win32 控件，检查忙状态、关闭取消、停止按钮、暂停显示后结束刷新；写入 2,700 行并确认首尾记录完整导出，同时拒绝覆写活动日志。
- 机器可读映射：[rust-gui-behavior-audit-2026-10-05.json](rust-gui-behavior-audit-2026-10-05.json)。

## 范围与排除

旧 WinForms 的类名、句柄、颜色和逐像素布局不作为行为等价要求；旧 env 导入、eac3to 流程和 C# 专属烟测启动变量按既定用户范围排除。原烟测功能通过 Rust 自动化脚本及真实控件测试保留。

本记录不声称穷举了所有操作系统权限、DPI 或设备环境；已执行的失败类别和实际控件证据如上。DesktopWorkflow 各阶段的算法和验证正确性由核心工作流证据另行覆盖。
