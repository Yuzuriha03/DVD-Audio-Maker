//! Background tasks: start, progress, completion, cancellation and busy state.

use super::{
    AppOptions, Arc, AtomicBool, Callbacks, EnableWindow, EnumChildWindows, GetClientRect, Hwnd,
    Instant, IsWindowEnabled, Lang, Lparam, Ordering, PBM_SETPOS, PBM_SETRANGE32, PathBuf,
    RDW_ALLCHILDREN, RDW_INVALIDATE, Rect, RedrawWindow, UiEvent, UiState, Value, WorkerCallbacks,
    WorkerEvent, add_log, autosave, checked, edited_profile_values, get_text, layout,
    localized_log, presentation, ptr, render_log, reset_task_log, set_text, thread, tr,
    update_statistics, validate_controls, win, window_class,
};

pub(crate) fn emit_preparation_issues(
    callbacks: &mut dyn Callbacks,
    issues: &[dvda_core::preparation::models::Issue],
) {
    for issue in issues {
        let mut text = format!("[{}] {}\n    {}", issue.level, issue.title, issue.reason);
        if !issue.path.is_empty() {
            text.push_str("\n    ");
            text.push_str(&issue.path);
        }
        for detail in &issue.detail {
            text.push_str("\n    ");
            text.push_str(detail);
        }
        callbacks.emit(if issue.level == "FAIL" { 2 } else { 1 }, &text);
    }
}

pub(crate) fn recovery_choice(owner: Hwnd, lang: Lang) -> Result<bool, String> {
    task_choice(owner, lang, false, false, None)
}

pub(crate) fn retention_choice(
    owner: Hwnd,
    lang: Lang,
    cancelled: bool,
    error: Option<&str>,
) -> Result<bool, String> {
    task_choice(owner, lang, true, cancelled, error)
}

#[allow(
    clippy::too_many_lines,
    reason = "Five-language recovery and retention copy stays in one ordered table"
)]
fn choice_text(
    lang: Lang,
    retention: bool,
    cancelled: bool,
) -> (&'static str, &'static str, &'static str, &'static str) {
    if retention {
        match lang {
            Lang::Zh => {
                if cancelled {
                    (
                        "已取消",
                        "你主动停止了本次任务。是否保留进度以便下次继续制作？删除进度只会删除此任务的中间文件，不会删除音源或已输出的 ISO。",
                        "保留进度",
                        "删除进度",
                    )
                } else {
                    (
                        "制作失败",
                        "本次制作失败。是否保留本次进度，以便下次继续制作？删除进度只会删除此任务的中间文件，不会删除音源或已输出的 ISO。",
                        "保留进度",
                        "删除进度",
                    )
                }
            }
            Lang::ZhHant => {
                if cancelled {
                    (
                        "已取消",
                        "你主動停止了本次任務。是否保留進度以便下次繼續製作？刪除進度只會刪除此任務的中間檔案，不會刪除音源或已輸出的 ISO。",
                        "保留進度",
                        "刪除進度",
                    )
                } else {
                    (
                        "製作失敗",
                        "本次製作失敗。是否保留本次進度，以便下次繼續製作？刪除進度只會刪除此任務的中間檔案，不會刪除音源或已輸出的 ISO。",
                        "保留進度",
                        "刪除進度",
                    )
                }
            }
            Lang::Ja => {
                if cancelled {
                    (
                        "キャンセルしました",
                        "ユーザーが今回のタスクを停止しました。次回再開するため進捗を保存しますか？削除するのはタスクの一時ファイルのみで、音源や出力済み ISO は削除しません。",
                        "進捗を保存",
                        "進捗を削除",
                    )
                } else {
                    (
                        "作成に失敗しました",
                        "今回の作成に失敗しました。次回再開するため進捗を保存しますか？削除するのはタスクの一時ファイルのみで、音源や出力済み ISO は削除しません。",
                        "進捗を保存",
                        "進捗を削除",
                    )
                }
            }
            Lang::Ko => {
                if cancelled {
                    (
                        "취소됨",
                        "사용자가 이번 작업을 중지했습니다. 다음에 계속할 수 있도록 진행 상황을 보관할까요? 삭제 시 이 작업의 중간 파일만 삭제하며 음원과 출력된 ISO는 삭제하지 않습니다.",
                        "진행 상황 보관",
                        "진행 상황 삭제",
                    )
                } else {
                    (
                        "제작 실패",
                        "이번 제작에 실패했습니다. 다음에 계속할 수 있도록 진행 상황을 보관할까요? 삭제 시 이 작업의 중간 파일만 삭제하며 음원과 출력된 ISO는 삭제하지 않습니다.",
                        "진행 상황 보관",
                        "진행 상황 삭제",
                    )
                }
            }
            Lang::En => {
                if cancelled {
                    (
                        "Cancelled",
                        "You stopped this task. Keep progress to continue next time? Deletion removes only this task's intermediate files, not sources or published ISOs.",
                        "Keep progress",
                        "Delete progress",
                    )
                } else {
                    (
                        "Production failed",
                        "Production failed. Keep progress to continue next time? Deletion removes only this task's intermediate files, not sources or published ISOs.",
                        "Keep progress",
                        "Delete progress",
                    )
                }
            }
        }
    } else {
        match lang {
            Lang::Zh => (
                "发现未完成的制作",
                "继续制作会校验并使用已保存的进度；重新制作会删除该任务的中间文件，从头开始。音源和已输出的 ISO 不会删除。",
                "继续制作",
                "重新制作",
            ),
            Lang::ZhHant => (
                "發現未完成的製作",
                "繼續製作會校驗并使用已儲存的進度；重新製作會刪除該任務的中間檔案，從頭開始。音源和已輸出的 ISO 不會刪除。",
                "繼續製作",
                "重新製作",
            ),
            Lang::Ja => (
                "未完了の作業があります",
                "保存済みの進捗を確認して再開するか、このタスクの一時ファイルを削除して最初から作成します。音源と出力済み ISO は削除しません。",
                "作成を続ける",
                "最初から作成",
            ),
            Lang::Ko => (
                "미완료 제작 발견",
                "계속하면 저장된 진행 상황을 검증하여 사용합니다. 다시 시작하면 이 작업의 중간 파일을 삭제하고 처음부터 제작합니다. 음원과 출력된 ISO는 보존됩니다.",
                "제작 계속",
                "처음부터 제작",
            ),
            Lang::En => (
                "Unfinished production found",
                "Continue validates saved progress. Restart deletes this task's intermediate files and starts over. Sources and published ISOs are preserved.",
                "Continue production",
                "Restart production",
            ),
        }
    }
}

pub(crate) fn task_choice(
    owner: Hwnd,
    lang: Lang,
    retention: bool,
    cancelled: bool,
    error: Option<&str>,
) -> Result<bool, String> {
    let (title, content, continue_text, restart_text) = choice_text(lang, retention, cancelled);
    let content = match error {
        Some(error) if !error.trim().is_empty() => {
            let detail = localized_log(lang, error);
            format!(
                "{content}

{detail}"
            )
        }
        _ => content.to_owned(),
    };
    let primary = super::dialog_ui::show_choice(
        owner,
        lang,
        super::dialog_ui::DialogKind::Warning,
        title,
        &content,
        continue_text,
        restart_text,
    )
    .ok_or_else(|| {
        "Cannot display unfinished-task progress choice; task files have been preserved.".to_owned()
    })?;
    Ok(!primary)
}

#[allow(
    clippy::too_many_lines,
    reason = "Keeps preparation, build, verification, retention and completion ordering in one worker lifecycle"
)]
pub(crate) fn start_operation(state: &mut UiState) {
    if !validate_controls(state, true) {
        return;
    }
    let path = PathBuf::from(get_text(state.controls.profile).trim());
    let base = if path.is_file() {
        AppOptions::load(Some(&path))
    } else {
        AppOptions::load(None)
    };
    let mut options =
        match base.and_then(|base| base.with_profile_values(edited_profile_values(state))) {
            Ok(options) => options,
            Err(error) => {
                add_log(
                    state,
                    &format!("{}{}", tr(state.lang(), "profile_error"), error),
                    true,
                );
                return;
            }
        };
    options.language = state.lang().code().into();
    let options = match if options.has_unfinished_task() {
        recovery_choice(state.main_window, state.lang())
            .and_then(|restart| options.recover_task(restart))
    } else {
        options.task_options()
    } {
        Ok(options) => options,
        Err(error) => {
            add_log(state, &error, true);
            return;
        }
    };
    let cancel = Arc::new(AtomicBool::new(false));
    autosave(state);
    state.cancel = Some(Arc::clone(&cancel));
    state.busy = true;
    state.started = Some(Instant::now());
    state.elapsed_seconds = 0;
    state.progress_value = 0;
    reset_task_log(state);
    set_busy_controls(state, true);
    update_statistics(state);
    relayout_for_task(state);
    EnableWindow(state.controls.build, 0);
    {
        EnableWindow(state.controls.cancel, 1);
    }
    let start_key = "started_build";
    state.status_key = start_key;
    state.activity_raw = tr(Lang::Zh, start_key).into();
    set_text(state.controls.status, tr(state.lang(), start_key));
    set_text(state.controls.activity, tr(state.lang(), start_key));
    // Verification can spend time in detailed-only checks, so keep a useful
    // summary row visible even when its low-level progress is filtered out.
    add_log(state, tr(Lang::Zh, start_key), false);
    if checked(state.controls.live) {
        render_log(state);
    }
    {
        let progress = state.controls.progress;
        win::send_int(progress, PBM_SETRANGE32, 0, 100);
        win::send_int(progress, PBM_SETPOS, 0, 0);
    }
    let lang = state.lang();
    let logs = state.log_sender.clone();
    thread::spawn(move || {
        let mut callbacks = WorkerCallbacks {
            logs,
            cancel: Arc::clone(&cancel),
            range: (0, 100),
            last_progress: 0,
        };
        let (ok, cancelled, text, failure_text) =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let outcome = dvda_core::gui::run_command(&options, &mut callbacks);
                emit_preparation_issues(&mut callbacks, &outcome.preparation_issues);
                emit_diagnostics(&mut callbacks, &outcome.diagnostics);
                let failure_text = outcome.failure.clone().unwrap_or_default();
                let message = if outcome.cancelled {
                    tr(lang, "cancelled_detail").to_owned()
                } else if let Some(failure) = &outcome.failure {
                    let label = tr(lang, "failed_build");
                    format!("{label}: {failure}")
                } else {
                    let done = outcome.succeeded;
                    tr(lang, if done { "done" } else { "failed" }).into()
                };
                (outcome.succeeded, outcome.cancelled, message, failure_text)
            }))
            .unwrap_or_else(|_| {
                (
                    false,
                    false,
                    "任务异常终止，请查看详细日志。".into(),
                    String::new(),
                )
            });
        if ok {
            drop(options);
        } else {
            let _ = callbacks.logs.send(WorkerEvent::Retention {
                options: Box::new(options),
                cancelled,
                error: failure_text,
            });
        }
        let kind = if cancelled { 2 } else { u32::from(ok) };
        let _ = callbacks
            .logs
            .send(WorkerEvent::Finished(UiEvent { kind, text }));
    });
}

pub(crate) fn emit_diagnostics(callbacks: &mut dyn Callbacks, diagnostics: &[Value]) {
    for diagnostic in diagnostics {
        if let Some(message) = diagnostic.get("Message").and_then(Value::as_str) {
            let severity = diagnostic
                .get("Severity")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let code = diagnostic.get("Code").and_then(Value::as_str).unwrap_or("");
            let prefix = match severity {
                2 => "[错误]",
                1 => "[警告]",
                _ => "[信息]",
            };
            callbacks.emit(
                if severity == 2 { 2 } else { 1 },
                &format!("{prefix} {message} [{code}]"),
            );
        }
    }
}

pub(crate) fn update_progress(state: &mut UiState, value: u16) {
    state.progress_value = state.progress_value.max(value.min(99));
    {
        win::send_int(
            state.controls.progress,
            PBM_SETPOS,
            state.progress_value as usize,
            0,
        );
    }
    if state.busy {
        set_text(
            state.controls.status,
            &format!(
                "{} \u{00b7} {}%",
                tr(state.lang(), state.status_key),
                state.progress_value
            ),
        );
    }
}

pub(crate) fn finish_operation(state: &mut UiState, kind: u32, text: &str) {
    state.cancel = None;
    state.elapsed_seconds = state.started.map_or(0, |s| s.elapsed().as_secs());
    state.started = None;
    let key = match kind {
        1 => "done",
        2 => "cancelled",
        _ => "failed",
    };
    state.status_key = key;
    let text = if kind == 2 {
        tr(state.lang(), "cancelled_detail")
    } else {
        text
    };
    set_text(state.controls.status, tr(state.lang(), key));
    set_text(state.controls.activity, &localized_log(state.lang(), text));
    add_log(state, text, kind == 0);
    if kind == 1 && state.problem_count > 0 {
        set_text(
            state.controls.status,
            &format!(
                "{} · {}",
                tr(state.lang(), key),
                presentation::phrase(
                    state.lang(),
                    "有提示待查看",
                    "有提示待查看",
                    "Review the warnings",
                    "警告を確認してください",
                    "경고 확인"
                )
            ),
        );
    }
    if kind == 0 {
        let advice = presentation::error_advice(state.lang(), text);
        add_log(state, &format!("[信息] {advice}"), false);
        set_text(state.controls.activity, &advice);
    }
    render_log(state);
    if kind == 1 {
        state.progress_value = 100;
        {
            win::send_int(state.controls.progress, PBM_SETPOS, 100, 0);
        }
    }
    // Publish idle only after the final diagnostics, progress and status are
    // complete. A new task must not be accepted inside an unfinished callback.
    {
        EnableWindow(state.controls.cancel, 0);
    }
    set_busy_controls(state, false);
    EnableWindow(state.controls.build, 1);
    state.busy = false;
    update_statistics(state);
    relayout_for_task(state);
}

/// Short windows switch between the settings and the task area when a task
/// starts or ends.
pub(crate) fn relayout_for_task(state: &mut UiState) {
    let mut rect: Rect = Rect::default();
    GetClientRect(state.main_window, &mut rect);
    // Minimized windows lay out again when restored.
    if rect.bottom > 0 {
        layout(state.main_window, state);
    }
}
pub(crate) fn cancel_operation(state: &mut UiState) {
    if let Some(cancel) = &state.cancel {
        cancel.store(true, Ordering::Relaxed);
        {
            EnableWindow(state.controls.cancel, 0);
        }
        state.status_key = "stopping";
        set_text(state.controls.status, tr(state.lang(), "stopping"));
        add_log(state, tr(Lang::Zh, "stopping"), false);
    }
}

/// # Safety
/// Window procedure called by Windows. `hwnd` is a valid window handle, and for messages
/// that pass pointers in `lparam` (such as `WM_DRAWITEM` or `WM_NCCALCSIZE`) it points to the
/// structure the system defines for that message, valid for the duration of the call.
pub(crate) unsafe extern "system" fn disable_busy_child(hwnd: Hwnd, parameter: Lparam) -> i32 {
    let disabled = unsafe { &mut *(parameter as *mut Vec<Hwnd>) };
    if window_class(hwnd) != "Static" && IsWindowEnabled(hwnd) != 0 {
        disabled.push(hwnd);
        EnableWindow(hwnd, 0);
    }
    1
}

pub(crate) fn set_busy_controls(state: &mut UiState, busy: bool) {
    let enabled = i32::from(!busy);
    if busy {
        for page in &state.pages {
            unsafe {
                EnumChildWindows(
                    *page,
                    disable_busy_child,
                    &raw mut state.busy_disabled_controls as Lparam,
                );
            }
        }
    } else {
        for hwnd in state.busy_disabled_controls.drain(..) {
            EnableWindow(hwnd, 1);
        }
    }
    for page in &state.pages {
        unsafe {
            RedrawWindow(
                *page,
                ptr::null(),
                ptr::null_mut(),
                RDW_INVALIDATE | RDW_ALLCHILDREN,
            );
        }
    }
    for hwnd in state
        .tabs
        .iter()
        .copied()
        .chain([state.controls.profile, state.controls.language])
    {
        unsafe {
            EnableWindow(hwnd, enabled);
            RedrawWindow(
                hwnd,
                ptr::null(),
                ptr::null_mut(),
                RDW_INVALIDATE | RDW_ALLCHILDREN,
            );
        }
    }
    for (hwnd, key) in &state.buttons {
        if matches!(*key, "open" | "save" | "save_as") {
            {
                EnableWindow(*hwnd, enabled);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Lang;

    #[test]
    fn cancelled_diagnostic_is_localized_for_the_dialog_without_changing_the_raw_text() {
        let raw = "Source preparation cancelled";
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja, Lang::Ko] {
            let displayed = localized_log(lang, raw);
            assert_eq!(displayed, tr(lang, "source_preparation_cancelled"));
            assert_eq!(raw, "Source preparation cancelled");
        }
    }

    #[test]
    fn cancelled_and_failed_retention_are_distinct_in_all_five_languages() {
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja, Lang::Ko] {
            let cancelled = choice_text(lang, true, true);
            let failed = choice_text(lang, true, false);
            assert_ne!(cancelled.0, failed.0, "{}", lang.code());
            assert_ne!(cancelled.1, failed.1, "{}", lang.code());
            assert_eq!(cancelled.2, failed.2);
            assert_eq!(cancelled.3, failed.3);
            let recovery = choice_text(lang, false, false);
            assert_ne!(recovery.0, "");
            assert_ne!(recovery.2, "");
            assert_ne!(recovery.3, "");
        }
    }
}
