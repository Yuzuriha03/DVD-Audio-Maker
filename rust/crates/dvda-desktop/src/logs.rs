//! Session and task logs: queueing, rendering, localization, copy and export.

use super::*;

pub(crate) fn open_session_log() -> std::io::Result<(fs::File, PathBuf)> {
    let root = dvda_core::app::default_profile_path()
        .parent()
        .ok_or_else(|| std::io::Error::other("Missing application directory"))?
        .to_owned();
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = root.join(format!(
        "gui-{timestamp}-{}-{}.log",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let file = fs::OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("Cannot create session log {}: {error}", path.display()),
            )
        })?;
    // Pruning is housekeeping: an old log that cannot be removed (another program
    // holding it, or a security tool blocking the delete) must not stop the app.
    let _ = prune_session_logs(&root, &path);
    Ok((file, path))
}

pub(crate) fn prune_session_logs(
    root: &std::path::Path,
    current: &std::path::Path,
) -> std::io::Result<()> {
    let mut previous = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(identifier) = name
            .strip_prefix("gui-")
            .and_then(|s| s.strip_suffix(".log"))
        else {
            continue;
        };
        let parts: Vec<_> = identifier.split('-').collect();
        if parts.len() != 3
            || parts
                .iter()
                .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
        {
            continue;
        }
        if entry.path() != current && entry.file_type()?.is_file() {
            previous.push((entry.metadata()?.modified()?, entry.path()));
        }
    }
    previous.sort_by(|a, b| b.cmp(a));
    for (_, path) in previous.into_iter().skip(2) {
        fs::remove_file(&path).map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("Cannot remove old session log {}: {error}", path.display()),
            )
        })?;
    }
    Ok(())
}

pub(crate) fn log_timestamp() -> String {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(value: *mut [u16; 8]);
    }
    let mut value = [0u16; 8];
    unsafe {
        GetLocalTime(&mut value);
    }
    format!("{:02}:{:02}:{:02}", value[4], value[5], value[6])
}

pub(crate) fn reset_task_log(state: &mut UiState) {
    state.logs.clear();
    state.summary_logs.clear();
    state.log_dirty = false;
    state.log_rendered_count = 0;
    state.log_render_language = None;
    state.log_first_visible_line = 0;
    state.log_hold_line = None;
    state.log_hold_until = None;
    state.activity_dirty = false;
    state.problem_count = 0;
    if state.log_file.is_none() {
        match open_session_log() {
            Ok(file) => state.log_file = Some(file),
            Err(error) => add_log(state, &format!("Cannot save the full log: {error}"), true),
        }
    }
    set_text(state.controls.log, "");
}

pub(crate) fn add_log(state: &mut UiState, text: &str, problem: bool) {
    let timestamp = log_timestamp();
    // Keep the source path and continuation lines of a multiline diagnostic in
    // the problem view together with its severity-bearing first line.
    let problem = problem || log_is_problem(text);
    for text in text.split(['\r', '\n']).filter(|line| !line.is_empty()) {
        if let Some((file, _)) = &mut state.log_file
            && let Err(error) = writeln!(file, "[{timestamp}] {text}").and_then(|_| file.flush())
        {
            state.log_file = None;
            let message = presentation::phrase(
                state.lang(),
                "无法继续保存完整日志，窗口仅保留最近记录。请检查磁盘空间或目录权限。",
                "無法繼續儲存完整日志，視窗僅保留最近記錄。請檢查磁盤空間或目錄權限。",
                "Cannot continue saving the full log. Only recent records remain; check disk space and folder permissions.",
                "完全なログを保存できません。最近の記録のみ表示されます。空き容量とフォルダー権限を確認してください。",
                "전체 로그를 계속 저장할 수 없습니다. 최근 기록만 유지됩니다. 디스크 공간과 폴더 권한을 확인하세요.",
            );
            push_log_line(
                state,
                LogLine {
                    raw: format!("[警告] {message} {error}"),
                    problem: true,
                    timestamp: timestamp.clone(),
                },
            );
            state.problem_count += 1;
        }
        let problem = problem || log_is_problem(text);
        // Repetitive category warnings are represented by their explicit
        // stage summary, not hundreds of identical entries in the counter.
        state.problem_count += usize::from(
            problem
                && !text
                    .trim()
                    .starts_with("WARN: Button y coordinates are odd for button "),
        );
        push_log_line(
            state,
            LogLine {
                raw: text.to_owned(),
                problem,
                timestamp: timestamp.clone(),
            },
        );
    }
    if presentation::activity(state.lang(), text).is_some() {
        state.activity_raw = text.to_owned();
        state.activity_dirty = true;
    }
    state.log_dirty = true;
}

pub(crate) fn push_log_line(state: &mut UiState, line: LogLine) {
    // Decoder progress can fill the detail buffer before the next track ends.
    // Keep task history separately so that chatter cannot evict summary rows.
    if presentation::summary(state.lang(), &line.raw, line.problem).is_some() {
        state.summary_logs.push(line.clone());
        if state.summary_logs.len() > 2500 {
            state.summary_logs.drain(..state.summary_logs.len() - 2000);
            if !state.log_render_detail {
                state.log_render_language = None;
            }
        }
    }
    state.logs.push(line);
    if state.logs.len() > 2500 {
        state.logs.drain(..state.logs.len() - 2000);
        if state.log_render_detail {
            state.log_render_language = None;
        }
    }
}

pub(crate) fn drain_worker_logs(hwnd: Hwnd, state: &mut UiState) {
    observe_log_scroll(state);
    let started = Instant::now();
    let mut processed = 0;
    let mut drained = false;
    while processed < LOG_BATCH_LIMIT {
        let batch: Vec<_> = state.log_receiver.try_iter().take(64).collect();
        if batch.is_empty() {
            drained = true;
            break;
        }
        processed += batch.len();
        for event in batch {
            match event {
                WorkerEvent::Log(text) => add_log(state, &text, false),
                WorkerEvent::Progress(value) => update_progress(state, value),
                WorkerEvent::Finished(event) => state.pending_finish = Some(event),
                WorkerEvent::Retention(options) => {
                    if retention_choice(state.lang()).unwrap_or(false) {
                        options.discard_task();
                    }
                }
            }
        }
        // Return to the message pump regularly so cancel, close, resizing and
        // statistics remain responsive even during continuous native output.
        if started.elapsed() >= LOG_BATCH_BUDGET {
            break;
        }
    }
    if state.activity_dirty
        && state.busy
        && !state
            .cancel
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
    {
        set_text(
            state.controls.activity,
            &localized_log(state.lang(), &state.activity_raw),
        );
    }
    state.activity_dirty = false;
    if drained && let Some(event) = state.pending_finish.take() {
        // The worker queues completion only after its last log. Finish and
        // close must wait until every queued diagnostic has been archived.
        finish_operation(state, event.kind, &event.text);
        if state.close_when_done {
            unsafe {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
        }
    } else if state.log_dirty && checked(state.controls.live) && !state.log_frozen {
        // A frozen log (live sizing) is rendered once the drag ends.
        render_log(state);
    }
}
pub(crate) fn display_log(lang: Lang, lines: &[LogLine], detail: bool, only: bool) -> String {
    let mut text = String::new();
    for line in lines {
        let message = if detail {
            Some(localized_detail(lang, &line.raw))
        } else if only && !line.problem {
            None
        } else {
            presentation::summary(lang, &line.raw, line.problem)
        };
        if let Some(message) = message {
            text.push_str(&format!("{}  {}\r\n", line.timestamp, message));
        }
    }
    text
}
pub(crate) fn render_log(state: &mut UiState) {
    let detail = combo_index(state.controls.log_view) == 1;
    let issues_only = checked(state.controls.only_issues);
    {
        EnableWindow(state.controls.only_issues, i32::from(!detail));
    }
    let lines = if detail {
        &state.logs
    } else {
        &state.summary_logs
    };
    let can_append = state.log_render_language == Some(state.lang())
        && state.log_render_detail == detail
        && state.log_render_issues_only == issues_only
        && state.log_rendered_count <= lines.len();
    let text = display_log(
        state.lang(),
        if can_append {
            &lines[state.log_rendered_count..]
        } else {
            lines
        },
        detail,
        issues_only,
    );
    let hold_line = state
        .log_hold_until
        .filter(|deadline| Instant::now() < *deadline)
        .and(state.log_hold_line);
    update_log_text(state.controls.log, &text, can_append, hold_line);
    if hold_line.is_none() {
        state.log_hold_line = None;
        state.log_hold_until = None;
    }
    state.log_rendered_count = lines.len();
    state.log_render_language = Some(state.lang());
    state.log_render_detail = detail;
    state.log_render_issues_only = issues_only;
    state.log_first_visible_line = first_visible_log_line(state.controls.log);
    state.log_dirty = false;
}

pub(crate) fn update_log_text(log: Hwnd, text: &str, append: bool, hold_line: Option<i32>) {
    if append && text.is_empty() {
        return;
    }
    let text = wide(text);
    unsafe {
        let mut selection_start = 0u32;
        let mut selection_end = 0u32;
        SendMessageW(
            log,
            EM_GETSEL,
            &mut selection_start as *mut u32 as usize,
            &mut selection_end as *mut u32 as isize,
        );
        // Keep text changes, selection and viewport positioning in one redraw.
        // Following the tail must not restore an old caret at the first line.
        win::send_int(log, WM_SETREDRAW, 0, 0);
        if append {
            let end = GetWindowTextLengthW(log).max(0);
            win::send_int(log, EM_SETREADONLY, 0, 0);
            win::send_int(log, EM_SETSEL, end as usize, end as isize);
            SendMessageW(log, EM_REPLACESEL, 0, text.as_ptr() as isize);
            win::send_int(log, EM_SETREADONLY, 1, 0);
        } else {
            SetWindowTextW(log, text.as_ptr());
        }
        if let Some(line) = hold_line {
            win::send_int(
                log,
                EM_SETSEL,
                selection_start as usize,
                selection_end as isize,
            );
            let current = first_visible_log_line(log);
            win::send_int(log, EM_LINESCROLL, 0, (line - current) as isize);
        } else {
            let end = GetWindowTextLengthW(log).max(0);
            win::send_int(log, EM_SETSEL, end as usize, end as isize);
            win::send_int(log, WM_VSCROLL, SB_BOTTOM, 0);
        }
        win::send_int(log, WM_SETREDRAW, 1, 0);
        RedrawWindow(log, ptr::null(), ptr::null_mut(), RDW_INVALIDATE);
    }
}

pub(crate) fn first_visible_log_line(log: Hwnd) -> i32 {
    (win::send_int(log, EM_GETFIRSTVISIBLELINE, 0, 0) as i32).max(0)
}

pub(crate) fn observe_log_scroll(state: &mut UiState) {
    let current = first_visible_log_line(state.controls.log);
    if current != state.log_first_visible_line {
        state.log_first_visible_line = current;
        state.log_hold_line = Some(current);
        state.log_hold_until = Some(Instant::now() + Duration::from_secs(10));
    }
}
pub(crate) fn log_is_problem(text: &str) -> bool {
    presentation::problem(text)
}

pub(crate) fn numbers(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(crate) fn localized_log(lang: Lang, raw: &str) -> String {
    let text = raw.trim();
    if text.is_empty() {
        return String::new();
    }
    if let Some(message) = presentation::task_message(lang, text) {
        return message;
    }
    if let Some(message) = batch_message(lang, text) {
        return message;
    }
    if let Some(value) = text.strip_prefix("[MLP-CHECK] start ")
        && !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => format!("正在检查已有 MLP：0/{value} 首"),
            Lang::ZhHant => format!("正在檢查已有 MLP：0/{value} 首"),
            Lang::En => format!("Checking existing MLP files: 0/{value} tracks"),
            Lang::Ja => format!("既存 MLP を確認中：0/{value} 曲"),
            Lang::Ko => format!("기존 MLP 확인 중: 0/{value}곡"),
        };
    }
    if let Some(value) = text.strip_prefix("[MLP-CHECK] progress ")
        && let Some((position, title)) = value.split_once(' ')
        && let Some((current, total)) = position.split_once('/')
        && !current.is_empty()
        && !total.is_empty()
        && current.bytes().all(|byte| byte.is_ascii_digit())
        && total.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => format!("正在检查已有 MLP：{current}/{total} 首；{title}"),
            Lang::ZhHant => format!("正在檢查已有 MLP：{current}/{total} 首；{title}"),
            Lang::En => format!("Checking existing MLP files: {current}/{total} tracks; {title}"),
            Lang::Ja => format!("既存 MLP を確認中：{current}/{total} 曲；{title}"),
            Lang::Ko => format!("기존 MLP 확인 중: {current}/{total}곡; {title}"),
        };
    }
    if let Some(value) = text.strip_prefix("[MLP-CHECK] complete ")
        && let Some((reused, pending)) = value.split_once(' ')
        && !reused.is_empty()
        && !pending.is_empty()
        && reused.bytes().all(|byte| byte.is_ascii_digit())
        && pending.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => {
                format!("MLP 检查完成：可复用 {reused} 首，需编码 {pending} 首。")
            }
            Lang::ZhHant => {
                format!("MLP 檢查完成：可重用 {reused} 首，需編碼 {pending} 首。")
            }
            Lang::En => format!("MLP checks complete: {reused} reusable, {pending} to encode."),
            Lang::Ja => {
                format!("MLP の確認完了：{reused} 曲を再利用、{pending} 曲をエンコードします。")
            }
            Lang::Ko => format!("MLP 검사 완료: 재사용 {reused}곡, 인코딩 필요 {pending}곡."),
        };
    }
    if let Some(value) = text.strip_prefix("[MLP-PROGRESS] ")
        && let Some((current, total)) = value.split_once('/')
        && current.bytes().all(|byte| byte.is_ascii_digit())
        && total.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => format!("MLP 编码进度：{current}/{total} 首"),
            Lang::ZhHant => format!("MLP 編碼進度：{current}/{total} 首"),
            Lang::En => format!("MLP encoding progress: {current}/{total} tracks"),
            Lang::Ja => format!("MLP エンコード進捗：{current}/{total} 曲"),
            Lang::Ko => format!("MLP 인코딩 진행률: {current}/{total}곡"),
        };
    }
    if let Some(value) = text.strip_prefix("[menu-cover] ")
        && let Some((kind, details)) = value.split_once(' ')
        && let Some((position, album)) = details.split_once(' ')
        && let Some((current, total)) = position.split_once('/')
        && current.bytes().all(|byte| byte.is_ascii_digit())
        && total.bytes().all(|byte| byte.is_ascii_digit())
    {
        let (zh, en, ja, ko) = match kind {
            "background" => (
                "菜单封面背景",
                "Menu cover background",
                "メニュー背景",
                "메뉴 표지 배경",
            ),
            "still" => (
                "播放封面静图",
                "Playback cover image",
                "再生用カバー画像",
                "재생 표지 이미지",
            ),
            _ => return text.to_owned(),
        };
        return match lang {
            Lang::Zh => format!("{zh} {current}/{total}：{album}"),
            Lang::ZhHant => format!(
                "{} {current}/{total}：{album}",
                if kind == "background" {
                    "正在建立選單背景"
                } else {
                    "正在建立播放封面"
                }
            ),
            Lang::En => format!("{en} {current}/{total}: {album}"),
            Lang::Ja => format!("{ja} {current}/{total}：{album}"),
            Lang::Ko => format!("{ko} {current}/{total}: {album}"),
        };
    }
    if let Some(number) = text.strip_prefix("[menu-cover] oversized ")
        && number.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => format!("播放封面图片 {number} 超过 2 MB，继续制作。"),
            Lang::ZhHant => format!("播放封面圖片 {number} 超過 2 MB，繼續製作。"),
            Lang::En => {
                format!("Playback cover image {number} exceeds 2 MB; disc creation will continue.")
            }
            Lang::Ja => {
                format!("再生用カバー画像 {number} は 2 MB を超えています。作成を続行します。")
            }
            Lang::Ko => {
                format!("재생 표지 이미지 {number}이(가) 2 MB를 초과합니다. 제작을 계속합니다.")
            }
        };
    }
    if let Some(value) = text.strip_prefix("[verify] pcm ") {
        let mut fields = value.splitn(3, ' ');
        if let (Some(position), Some(result), Some(path)) =
            (fields.next(), fields.next(), fields.next())
            && let Some((current, total)) = position.split_once('/')
        {
            let result = result.strip_suffix(':').unwrap_or(result);
            return match (lang, result) {
                (Lang::Zh, "ok") => format!("成品音频校验：{current}/{total} 首一致：{path}"),
                (Lang::ZhHant, "ok") => format!("成品音訊校驗：{current}/{total} 首一致：{path}"),
                (Lang::En, "ok") => {
                    format!("Output audio check: {current}/{total} tracks match: {path}")
                }
                (Lang::Ko, "ok") => format!("출력 오디오 검사: {current}/{total}곡 일치: {path}"),
                (Lang::Ja, "ok") => format!("出力音声検証：{current}/{total} 曲一致：{path}"),
                (Lang::Zh, "failed") => format!("成品音频校验：{current}/{total} 首不一致：{path}"),
                (Lang::ZhHant, "failed") => {
                    format!("成品音訊校驗：{current}/{total} 首不一致：{path}")
                }
                (Lang::En, "failed") => {
                    format!("Output audio check: {current}/{total} tracks differ: {path}")
                }
                (Lang::Ko, "failed") => {
                    format!("출력 오디오 검사: {current}/{total}곡 불일치: {path}")
                }
                (Lang::Ja, "failed") => format!("出力音声検証：{current}/{total} 曲不一致：{path}"),
                _ => text.to_owned(),
            };
        }
    }
    if let Some(value) = text.strip_prefix("[verify] complete ") {
        let mut fields = value.split_whitespace();
        if let (Some(discs), Some("discs"), Some(tracks), Some("tracks"), Some(status)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) {
            let succeeded = status == "ok";
            return match (lang, succeeded) {
                (Lang::Zh, true) => {
                    format!("成品校验完成：检查 {discs} 张光盘、{tracks} 首音轨，结果通过。")
                }
                (Lang::ZhHant, true) => {
                    format!("成品校驗完成：檢查 {discs} 張光碟、{tracks} 首音軌，結果通過。")
                }
                (Lang::En, true) => format!(
                    "Output verification completed: {discs} discs and {tracks} tracks checked successfully."
                ),
                (Lang::Ko, true) => {
                    format!("출력 검증 완료: 디스크 {discs}개, {tracks}곡 검사 통과.")
                }
                (Lang::Ja, true) => format!(
                    "出力検証完了：ディスク {discs} 枚、{tracks} 曲を確認し、問題ありません。"
                ),
                (Lang::Zh, false) => {
                    format!("成品校验结束：检查 {discs} 张光盘、{tracks} 首音轨，发现问题。")
                }
                (Lang::ZhHant, false) => {
                    format!("成品校驗結束：檢查 {discs} 張光碟、{tracks} 首音軌，發現問題。")
                }
                (Lang::En, false) => format!(
                    "Output verification finished: {discs} discs and {tracks} tracks checked; problems were found."
                ),
                (Lang::Ko, false) => format!(
                    "출력 검증 종료: 디스크 {discs}개, {tracks}곡 검사 중 문제가 발견되었습니다."
                ),
                (Lang::Ja, false) => format!(
                    "出力検証終了：ディスク {discs} 枚、{tracks} 曲を確認し、問題が見つかりました。"
                ),
            };
        }
    }
    if let Some(value) = text.strip_prefix("发现 ")
        && let Some(count) = numbers(value).first()
    {
        return tr(lang, "log_found_audio").replace("{0}", count);
    }
    if let Some(value) = text.strip_prefix("共需重采样 ")
        && let Some(count) = numbers(value).first()
    {
        return if count == "0" {
            tr(lang, "log_resample_none").into()
        } else {
            tr(lang, "log_resample_count").replace("{0}", count)
        };
    }
    if text.starts_with("[缓存] 复用元数据 ") || text.starts_with("[缓存] 复用已校验结果 ")
    {
        let values = numbers(text);
        if values.len() >= 2 {
            let key = if text.contains("复用元数据") {
                "log_cache_metadata"
            } else {
                "log_cache_verified"
            };
            return tr(lang, key)
                .replace("{0}", &values[0])
                .replace("{1}", &values[1]);
        }
    }
    if let Some(value) = text.strip_prefix("[MLP] ")
        && !value.starts_with("临时目录:")
        && !value.starts_with("MLP 输出目录:")
    {
        return tr(lang, "log_encoding")
            .replace("{0}", &localization::translate(lang.code(), value));
    }
    if let Some(value) = text.strip_prefix("[MLP-FINALIZE] start ")
        && value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => format!("开始整理 MLP 编码结果，共 {value} 首。"),
            Lang::ZhHant => format!("開始整理 MLP 編碼結果，共 {value} 首。"),
            Lang::En => format!("Finalizing {value} MLP encoding results."),
            Lang::Ja => format!("MLP エンコード結果を整理中（{value} 曲）。"),
            Lang::Ko => format!("MLP 인코딩 결과 {value}개를 정리합니다."),
        };
    }
    if let Some(value) = text.strip_prefix("[MLP-FINALIZE] progress ") {
        let mut fields = value.splitn(2, ' ');
        if let (Some(position), Some(title)) = (fields.next(), fields.next())
            && let Some((current, total)) = position.split_once('/')
        {
            return match lang {
                Lang::Zh => {
                    format!("正在整理 MLP 编码结果：{current}/{total}，{title}")
                }
                Lang::ZhHant => {
                    format!("正在整理 MLP 編碼結果：{current}/{total}，{title}")
                }
                Lang::En => {
                    format!("Finalizing MLP encoding result {current}/{total}: {title}")
                }
                Lang::Ja => format!("MLP エンコード結果を整理中：{current}/{total}、{title}"),
                Lang::Ko => format!("MLP 인코딩 결과 정리 중: {current}/{total}, {title}"),
            };
        }
    }
    if let Some(value) = text.strip_prefix("[MLP-FINALIZE] complete ")
        && value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => format!("MLP 编码结果整理完成，共 {value} 首。"),
            Lang::ZhHant => format!("MLP 編碼結果整理完成，共 {value} 首。"),
            Lang::En => format!("MLP encoding results finalized: {value} tracks."),
            Lang::Ja => format!("MLP エンコード結果の整理が完了しました（{value} 曲）。"),
            Lang::Ko => format!("MLP 인코딩 결과 정리 완료: {value}곡."),
        };
    }
    if let Some(value) = text.strip_prefix("[LPCM] ") {
        return tr(lang, "log_lpcm").replace("{0}", &localization::translate(lang.code(), value));
    }
    if let Some(value) = text.strip_prefix("[author] disc ")
        && value.chars().all(|c| c.is_ascii_digit())
        && let Some(number) = numbers(value).first()
    {
        return tr(lang, "log_author_disc").replace("{0}", number);
    }
    if let Some(value) = text.strip_prefix("[build] ")
        && value.ends_with(" tracks loaded")
        && let Some(number) = numbers(value).first()
    {
        return tr(lang, "log_loaded_tracks").replace("{0}", number);
    }
    if let Some(value) = text.strip_prefix("总计 ")
        && let Some(number) = numbers(value).first()
    {
        return tr(lang, "log_total_tracks").replace("{0}", number);
    }
    if let Some(value) = text.strip_prefix("报告已写入: ") {
        return tr(lang, "log_report_written").replace("{0}", value);
    }
    if let Some(value) = text.strip_prefix("manifest.json 已生成 -> ") {
        return tr(lang, "log_manifest_written").replace("{0}", value);
    }
    localized_detail(lang, text)
}

pub(crate) fn localized_detail(lang: Lang, text: &str) -> String {
    let text = text.trim();
    if let Some(event) = dvda_core::task_log::parse(text)
        && let Some(message) = presentation::task_message(lang, text)
    {
        return if event.detail.is_empty() {
            message
        } else {
            format!("{message} · {}", event.detail)
        };
    }
    if let Some(message) = batch_message(lang, text) {
        return message;
    }
    for prefix in ["[FAIL] ", "[WARN] ", "[WAR] "] {
        if let Some(message) = text.strip_prefix(prefix) {
            return format!("{prefix}{}", localization::translate(lang.code(), message));
        }
    }
    for (prefix, key) in [
        ("[警告] ", "log_warning_prefix"),
        ("[错误] ", "log_error_prefix"),
        ("[信息] ", ""),
    ] {
        if let Some(tail) = text.strip_prefix(prefix) {
            let (message, code) = tail
                .rsplit_once(" [")
                .filter(|(_, code)| code.ends_with(']'))
                .map(|(message, code)| (message, format!(" [{code}")))
                .unwrap_or((tail, String::new()));
            return format!(
                "{}{}{}",
                tr(lang, key),
                localization::translate(lang.code(), message),
                code
            );
        }
    }
    for key in [
        "ready",
        "ready_hint",
        "loaded",
        "done",
        "failed",
        "cancelled",
        "stopping",
        "started_check",
        "started_build",
        "started_verify",
    ] {
        if [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja]
            .iter()
            .any(|&source| tr(source, key) == text)
        {
            return tr(lang, key).into();
        }
    }
    for key in ["saved", "profile_error"] {
        for source in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja] {
            if let Some(tail) = text.strip_prefix(tr(source, key)) {
                return format!(
                    "{}{}",
                    tr(lang, key),
                    if key == "saved" {
                        tail.into()
                    } else {
                        localization::translate(lang.code(), tail)
                    }
                );
            }
        }
    }
    localization::translate(lang.code(), text)
}

pub(crate) fn batch_message(lang: Lang, raw: &str) -> Option<String> {
    if !raw.starts_with('{') {
        return None;
    }
    let value: Value = serde_json::from_str(raw).ok()?;
    let name = value.get("Name")?.as_str()?;
    let detail = value.get("Value").unwrap_or(&Value::Null);
    let key = match value.get("Kind")?.as_str()? {
        "Started" => "log_started",
        "PcmStarted" => "log_pcm_started",
        "PcmProgress" => "log_pcm_progress",
        "PcmFinished" => "log_pcm_finished",
        "Encoded" => "log_encoded",
        "PcmError" => "log_pcm_error",
        "Detail" => {
            return Some(format!(
                "{name}: {}",
                localized_detail(lang, detail.as_str().unwrap_or_default())
            ));
        }
        _ => return None,
    };
    let mut result = tr(lang, key).to_owned();
    if key == "log_encoded" {
        for (i, field) in ["Rate", "Bits", "Channels", "Bytes"].iter().enumerate() {
            result = result.replace(
                &format!("{{{}}}", i + 1),
                &detail
                    .get(field)
                    .map(Value::to_string)
                    .unwrap_or_else(|| "?".into()),
            );
        }
    } else if key == "log_pcm_progress" {
        result = result.replace(
            "{1}",
            &detail
                .as_i64()
                .map(|v| v.clamp(0, 100).to_string())
                .unwrap_or_else(|| "?".into()),
        );
    } else if key == "log_pcm_error" {
        result = result.replace(
            "{1}",
            &localized_detail(lang, detail.as_str().unwrap_or_default()),
        );
    }
    // Substitute track names last so user data containing braces stays verbatim.
    Some(result.replace("{0}", name))
}
pub(crate) fn copy_log(state: &UiState) {
    let text = get_text(state.controls.log);
    let mut data = text
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        if OpenClipboard(ptr::null_mut()) != 0 {
            EmptyClipboard();
            let handle = GlobalAlloc(0x42, data.len() * 2);
            if !handle.is_null() {
                let destination = GlobalLock(handle) as *mut u16;
                if !destination.is_null() {
                    std::ptr::copy_nonoverlapping(data.as_mut_ptr(), destination, data.len());
                    GlobalUnlock(handle);
                    SetClipboardData(13, handle);
                }
            }
            CloseClipboard();
        }
    }
}
pub(crate) fn export_log(hwnd: Hwnd, state: &mut UiState) {
    let Some(path) = file_dialog(
        hwnd,
        true,
        "DVD-Audio-Maker.log.txt",
        tr(state.lang(), "select_log"),
        state.lang(),
        FileKind::Log,
    ) else {
        return;
    };
    let result = export_log_to(state, Path::new(&path));
    if let Err(error) = result {
        message_box(
            hwnd,
            &format!("{}{}", tr(state.lang(), "export_failed"), error),
            "DVD-Audio Maker",
            0x10,
        );
    } else {
        add_log(
            state,
            &format!(
                "[信息] {}",
                presentation::phrase(
                    state.lang(),
                    "详细日志已导出，可用于排查问题。",
                    "詳細日志已匯出，可用于排查問題。",
                    "The full log was exported for troubleshooting.",
                    "詳細ログを保存しました。問題の調査に利用できます。",
                    "문제 해결을 위해 전체 로그를 내보냈습니다."
                )
            ),
            false,
        );
    }
}

pub(crate) fn export_log_to(state: &mut UiState, destination: &Path) -> Result<(), String> {
    // Include all records already waiting when export was requested. Limit
    // the snapshot to the queue capacity so an active producer cannot keep
    // the export operation in an unbounded drain loop.
    let queued: Vec<_> = state
        .log_receiver
        .try_iter()
        .take(LOG_QUEUE_CAPACITY)
        .collect();
    for event in queued {
        match event {
            WorkerEvent::Log(text) => add_log(state, &text, false),
            WorkerEvent::Progress(value) => update_progress(state, value),
            WorkerEvent::Finished(event) => state.pending_finish = Some(event),
            WorkerEvent::Retention(options) => {
                if retention_choice(state.lang()).unwrap_or(false) {
                    options.discard_task();
                }
            }
        }
    }
    if let Some((_, source)) = &state.log_file {
        if presentation::same_path(source, destination) {
            return Err(presentation::phrase(
                state.lang(),
                "请选择其他文件名，不能覆盖正在记录的日志。",
                "請選擇其他檔案名，不能覆蓋正在記錄的日志。",
                "Choose another file name; the active log cannot be overwritten.",
                "別のファイル名を選んでください。記録中のログは上書きできません。",
                "다른 파일 이름을 선택하세요. 현재 로그는 덮어쓸 수 없습니다.",
            )
            .into());
        }
        fs::copy(source, destination).map_err(|e| e.to_string())?;
    } else {
        fs::write(destination, full_log(state.lang(), &state.logs)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn full_log(lang: Lang, lines: &[LogLine]) -> String {
    let mut text = String::new();
    for line in lines {
        let translated = localized_detail(lang, &line.raw);
        text.push_str(&format!("[{}] ", line.timestamp));
        text.push_str(&translated);
        text.push_str("\r\n");
        if translated != line.raw.trim() {
            text.push_str("[raw] ");
            text.push_str(&line.raw);
            text.push_str("\r\n");
        }
    }
    text
}
