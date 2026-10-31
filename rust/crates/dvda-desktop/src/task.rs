//! Background tasks: start, progress, completion, cancellation and busy state.

use super::*;

pub(crate) fn emit_preparation_issues(
    callbacks: &mut dyn Callbacks,
    issues: &[dvda_core::preparation::models::Issue],
) {
    for issue in issues {
        let mut text = format!("[{}] {}\n    {}", issue.level, issue.title, issue.reason);
        if !issue.path.is_empty() {
            text.push_str(&format!("\n    {}", issue.path));
        }
        for detail in &issue.detail {
            text.push_str(&format!("\n    {detail}"));
        }
        callbacks.emit(if issue.level == "FAIL" { 2 } else { 1 }, &text);
    }
}

pub(crate) fn recovery_choice(lang: Lang) -> Result<bool, String> {
    task_choice(lang, false)
}

pub(crate) fn retention_choice(lang: Lang) -> Result<bool, String> {
    task_choice(lang, true)
}

pub(crate) fn task_choice(lang: Lang, retention: bool) -> Result<bool, String> {
    #[repr(C, packed)]
    struct Button {
        id: i32,
        text: *const u16,
    }
    #[repr(C, packed)]
    struct Config {
        size: u32,
        parent: Hwnd,
        instance: *mut std::ffi::c_void,
        flags: u32,
        common: u32,
        title: *const u16,
        icon: *const u16,
        instruction: *const u16,
        content: *const u16,
        count: u32,
        buttons: *const Button,
        default_button: i32,
        radio_count: u32,
        radios: *const Button,
        default_radio: i32,
        verification: *const u16,
        expanded: *const u16,
        expanded_control: *const u16,
        collapsed_control: *const u16,
        footer_icon: *const u16,
        footer: *const u16,
        callback: usize,
        callback_data: isize,
        width: u32,
    }
    #[link(name = "comctl32")]
    unsafe extern "system" {
        fn TaskDialogIndirect(
            config: *const Config,
            button: *mut i32,
            radio: *mut i32,
            verified: *mut i32,
        ) -> i32;
    }
    let (title, content, continue_text, restart_text) = if retention {
        match lang {
            Lang::Zh => (
                "制作未完成",
                "是否保留本次进度，以便下次继续制作？删除进度只会删除此任务的中间文件，不会删除音源或已输出的 ISO。",
                "保留进度",
                "删除进度",
            ),
            Lang::ZhHant => (
                "製作未完成",
                "是否保留本次進度，以便下次繼續製作？刪除進度只會刪除此任務的中間檔案，不會刪除音源或已輸出的 ISO。",
                "保留進度",
                "刪除進度",
            ),
            Lang::Ja => (
                "作成が完了していません",
                "次回再開するため進捗を保存しますか？削除するのはタスクの一時ファイルのみです。",
                "進捗を保存",
                "進捗を削除",
            ),
            Lang::Ko => (
                "제작 미완료",
                "다음에 계속할 수 있도록 진행 상황을 보관할까요? 삭제 시 이 작업의 중간 파일만 삭제하며 음원과 출력된 ISO는 삭제하지 않습니다.",
                "진행 상황 보관",
                "진행 상황 삭제",
            ),
            _ => (
                "Production unfinished",
                "Keep progress to continue next time? Deletion removes only this task's intermediate files, not sources or published ISOs.",
                "Keep progress",
                "Delete progress",
            ),
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
            _ => (
                "Unfinished production found",
                "Continue validates saved progress. Restart deletes this task's intermediate files and starts over. Sources and published ISOs are preserved.",
                "Continue production",
                "Restart production",
            ),
        }
    };
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let title = wide(title);
    let content = wide(content);
    let continue_text = wide(continue_text);
    let restart_text = wide(restart_text);
    let buttons = [
        Button {
            id: 1001,
            text: continue_text.as_ptr(),
        },
        Button {
            id: 1002,
            text: restart_text.as_ptr(),
        },
    ];
    let mut config: Config = unsafe { std::mem::zeroed() };
    config.size = std::mem::size_of::<Config>() as u32;
    config.title = title.as_ptr();
    config.instruction = title.as_ptr();
    config.content = content.as_ptr();
    config.count = 2;
    config.buttons = buttons.as_ptr();
    config.default_button = 1001;
    let mut button = 0;
    let result = unsafe {
        TaskDialogIndirect(
            &config,
            &mut button,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result < 0 || !matches!(button, 1001 | 1002) {
        return Err(
            "Cannot display unfinished-task recovery choice; task files have been preserved."
                .into(),
        );
    }
    Ok(button == 1002)
}

pub(crate) fn start_operation(state: &mut UiState, command: usize) {
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
    let options = match if command == ID_VERIFY {
        Ok(options)
    } else if command == ID_PREPARE {
        options.scratch_options()
    } else if options.has_unfinished_task() {
        recovery_choice(state.lang()).and_then(|restart| options.recover_task(restart))
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
    for hwnd in [
        state.controls.prepare,
        state.controls.build,
        state.controls.verify,
    ] {
        {
            EnableWindow(hwnd, 0);
        }
    }
    {
        EnableWindow(state.controls.cancel, 1);
    }
    let start_key = match command {
        ID_PREPARE => "started_check",
        ID_BUILD => "started_build",
        _ => "started_verify",
    };
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
        let (ok, text) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match command {
            ID_PREPARE => {
                callbacks.set_progress_range(0, 100);
                let outcome = dvda_core::app::run_prepare(&options, false, &mut callbacks);
                if let Some(data) = &outcome.data {
                    emit_preparation_issues(&mut callbacks, &data.issues);
                }
                (
                    outcome.succeeded(),
                    outcome.failure.map(|f| f.message).unwrap_or_else(|| {
                        tr(
                            lang,
                            if outcome
                                .data
                                .as_ref()
                                .is_some_and(|data| data.failure_count == 0)
                            {
                                "done"
                            } else {
                                "failed"
                            },
                        )
                        .into()
                    }),
                )
            }
            ID_BUILD => {
                callbacks.set_progress_range(0, 15);
                let prepared = dvda_core::app::run_prepare(&options, false, &mut callbacks);
                if let Some(data) = &prepared.data {
                    emit_preparation_issues(&mut callbacks, &data.issues);
                }
                if !prepared.succeeded() {
                    (
                        false,
                        prepared
                            .failure
                            .map(|failure| failure.message)
                            .unwrap_or_else(|| tr(lang, "failed").into()),
                    )
                } else {
                    callbacks.set_progress_range(15, 65);
                    match dvda_core::app::run_build(&options, false, &mut callbacks) {
                        Ok(result) => {
                            emit_diagnostics(&mut callbacks, &result.diagnostics);
                            if !result.succeeded {
                                (false, tr(lang, "failed").into())
                            } else {
                                callbacks.set_progress_range(65, 100);
                                match dvda_core::app::run_verify(&options, &mut callbacks) {
                                    Ok(verification) => {
                                        emit_diagnostics(&mut callbacks, &verification.diagnostics);
                                        (
                                            verification.succeeded,
                                            tr(
                                                lang,
                                                if verification.succeeded {
                                                    "done"
                                                } else {
                                                    "failed"
                                                },
                                            )
                                            .into(),
                                        )
                                    }
                                    Err(error) => (false, error),
                                }
                            }
                        }
                        Err(error) => (false, error),
                    }
                }
            }
            _ => {
                callbacks.set_progress_range(0, 100);
                match dvda_core::app::run_verify(&options, &mut callbacks) {
                    Ok(result) => {
                        emit_diagnostics(&mut callbacks, &result.diagnostics);
                        (
                            result.succeeded,
                            tr(lang, if result.succeeded { "done" } else { "failed" }).into(),
                        )
                    }
                    Err(error) => (false, error),
                }
            }
        }))
        .unwrap_or_else(|_| (false, "任务异常终止，请查看详细日志。".into()));
        if ok && command == ID_BUILD {
            options.finish_task();
        }
        if !ok && command == ID_BUILD {
            let _ = callbacks
                .logs
                .send(WorkerEvent::Retention(Box::new(options)));
        } else {
            drop(options);
        }
        let kind = if cancel.load(Ordering::Relaxed) {
            2
        } else if ok {
            1
        } else {
            0
        };
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
        tr(state.lang(), "cancelled")
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
    for hwnd in [
        state.controls.prepare,
        state.controls.build,
        state.controls.verify,
    ] {
        {
            EnableWindow(hwnd, 1);
        }
    }
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
/// that pass pointers in `lparam` (such as WM_DRAWITEM or WM_NCCALCSIZE) it points to the
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
                    &mut state.busy_disabled_controls as *mut Vec<Hwnd> as Lparam,
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
