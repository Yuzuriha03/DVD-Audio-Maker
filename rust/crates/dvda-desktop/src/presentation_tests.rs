use super::ui_test_guard;

#[test]
fn session_log_retention_keeps_three_and_preserves_unrelated_files() {
    let _ui_test_lock = ui_test_guard();
    let root = std::env::temp_dir().join(format!("dvda-log-retention-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for name in ["gui-1-1-0.log", "gui-2-1-0.log", "gui-3-1-0.log", "gui-4-1-0.log", "gui-not-ours.log", "notes.log"] {
        std::fs::write(root.join(name), name).unwrap();
    }
    let current = root.join("gui-4-1-0.log");
    super::prune_session_logs(&root, &current).unwrap();
    let count = std::fs::read_dir(&root).unwrap().count();
    assert_eq!(count, 5);
    assert!(current.exists());
    assert!(root.join("gui-not-ours.log").exists());
    assert!(root.join("notes.log").exists());
    std::fs::remove_dir_all(root).unwrap();
}

use super::*;

#[test]
fn startup_arguments_keep_json_alias_and_language_precedence() {
    let _ui_test_lock = ui_test_guard();
    let parse = |args: &[&str], environment| {
        parse_startup_args(args.iter().map(|arg| (*arg).to_owned()), environment)
    };
    for flag in ["--config", "--profile"] {
        let result = parse(
            &[flag, "日本語 profile.json", "--language", "en"],
            Some("ja"),
        )
        .unwrap();
        assert_eq!(result.profile, Some(PathBuf::from("日本語 profile.json")));
        assert_eq!(result.language.as_deref(), Some("en"));
    }
    assert_eq!(
        parse(&[], Some("ja-JP")).unwrap().language.as_deref(),
        Some("ja")
    );
    assert_eq!(parse(&[], None).unwrap().language, None);
    assert_eq!(
        parse(&["--language", "auto"], Some("ja")).unwrap().language,
        Some(Lang::from_code("auto").code().into())
    );
    for args in [
        vec!["--unknown"],
        vec!["--profile"],
        vec!["--config", "--language", "en"],
        vec!["--language"],
        vec!["--language", "invalid"],
        vec!["--language", "en", "--language", "ja"],
    ] {
        assert!(parse(&args, None).is_err(), "{args:?}");
    }
    assert!(parse(&[], Some("invalid")).is_err());
    assert!(parse(&["--language", "en"], Some("invalid")).is_ok());
}

#[test]
fn source_issue_diagnostics_include_severity_reason_source_and_details() {
    let _ui_test_lock = ui_test_guard();
    #[derive(Default)]
    struct Capture(Vec<(i32, String)>);
    impl Callbacks for Capture {
        fn emit(&mut self, stream: i32, text: &str) {
            self.0.push((stream, text.into()));
        }
        fn cancelled(&mut self) -> bool {
            false
        }
    }
    let issues = ["FAIL", "WARN"].map(|level| dvda_core::preparation::models::Issue {
        level: level.into(),
        title: "Channel mismatch".into(),
        path: "source.flac".into(),
        reason: "1 channel / 2 channels".into(),
        detail: vec!["first track".into(), "second track".into()],
    });
    let mut captured = Capture::default();
    emit_preparation_issues(&mut captured, &issues);
    assert_eq!(captured.0.len(), 2);
    assert_eq!(captured.0[0].0, 2);
    assert_eq!(captured.0[1].0, 1);
    for (_, text) in captured.0 {
        for required in [
            "Channel mismatch",
            "1 channel / 2 channels",
            "source.flac",
            "first track",
            "second track",
        ] {
            assert!(text.contains(required), "{text}");
        }
    }
}

#[test]
fn every_ui_translation_key_exists_in_all_three_languages() {
    let _ui_test_lock = ui_test_guard();
    // The translation table lives in i18n.rs.
    let source = include_str!("i18n.rs");
    let section = source
        .split("fn tr(")
        .nth(1)
        .unwrap()
        .split("#[derive(Clone, Copy)]")
        .next()
        .unwrap();
    let mut keys = std::collections::BTreeSet::new();
    for line in section.lines() {
        let line = line.trim();
        if let Some(tail) = line.strip_prefix('"')
            && let Some((key, rest)) = tail.split_once('"')
            && rest.trim_start().starts_with("=>")
        {
            keys.insert(key);
        }
    }
    assert!(keys.len() >= 100);
    for key in keys {
        for lang in [Lang::Zh, Lang::En, Lang::Ja] {
            assert!(!tr(lang, key).is_empty(), "Missing {} / {key}", lang.code());
        }
    }
}

#[test]
fn failures_never_become_successful_progress_messages() {
    let _ui_test_lock = ui_test_guard();
    let message = "[author] disc 2 failed (exit code 17)\ndecoder 0x4321: failure";
    for (lang, token) in [(Lang::Zh, "失败"), (Lang::En, "failed"), (Lang::Ja, "失敗")] {
        let output = localized_log(lang, message);
        assert!(output.contains(token), "{output}");
        assert!(
            output.contains("17") && output.contains("decoder 0x4321: failure"),
            "{output}"
        );
        assert!(log_is_problem(message));
    }
    let output = localized_detail(
        Lang::Ja,
        "[错误] No published ISO images were found [NO_ISO]",
    );
    assert!(
        output.contains("見つかりません") && output.contains("[NO_ISO]"),
        "{output}"
    );
}

#[test]
fn batch_progress_preserves_track_names_and_audio_parameters() {
    let _ui_test_lock = ui_test_guard();
    let name = "音源_日本語_{0}_{1}.flac";
    for kind in [
        "Started",
        "PcmStarted",
        "PcmProgress",
        "PcmFinished",
        "Encoded",
        "PcmError",
        "Detail",
    ] {
        let value = match kind {
            "Encoded" => {
                json!({"Frames":192000,"Rate":96000,"Bits":24,"Channels":6,"Bytes":7654321})
            }
            "PcmProgress" => json!(47),
            _ => json!("原始诊断 0xABC"),
        };
        let raw = json!({"Kind":kind,"Track":0,"Name":name,"Value":value}).to_string();
        for lang in [Lang::Zh, Lang::En, Lang::Ja] {
            let output = localized_log(lang, &raw);
            assert!(output.contains(name), "{kind}: {output}");
            assert!(!output.starts_with('{'));
            if kind == "Encoded" {
                for token in ["96000", "24", "6", "7654321"] {
                    assert!(output.contains(token));
                }
            }
            if kind == "PcmProgress" {
                assert!(output.contains("47%"));
            }
        }
    }
}

#[test]
fn menu_cover_progress_is_localized_with_album_and_position() {
    let _ui_test_lock = ui_test_guard();
    let raw = "[menu-cover] background 2/17 Album 名称";
    for (lang, label) in [
        (Lang::Zh, "菜单封面背景"),
        (Lang::En, "Menu cover background"),
        (Lang::Ja, "メニュー背景"),
    ] {
        let output = localized_log(lang, raw);
        assert!(output.contains(label), "{output}");
        assert!(
            output.contains("2/17") && output.contains("Album 名称"),
            "{output}"
        );
        assert!(!output.contains("720 576"), "{output}");
    }
}

#[test]
fn oversized_playback_cover_warning_is_localized_as_nonfatal() {
    let _ui_test_lock = ui_test_guard();
    let raw = "[menu-cover] oversized 17";
    for (lang, expected) in [
        (Lang::Zh, "超过 2 MB，继续制作"),
        (Lang::En, "disc creation will continue"),
        (Lang::Ja, "作成を続行します"),
    ] {
        let output = localized_log(lang, raw);
        assert!(output.contains(expected), "{output}");
        assert!(output.contains("17"), "{output}");
    }
}

#[test]
fn full_log_includes_records_beyond_display_limit_and_raw_diagnostics() {
    let _ui_test_lock = ui_test_guard();
    let mut lines = (0..4500)
        .map(|n| LogLine {
            raw: format!("record {n}"),
            problem: false,
            timestamp: "12:00:00".into(),
        })
        .collect::<Vec<_>>();
    lines.push(LogLine {
        raw: "[错误] No published ISO images were found [NO_ISO]".into(),
        problem: true,
        timestamp: "12:00:00".into(),
    });
    let output = full_log(Lang::Ja, &lines);
    assert!(output.contains("record 0\r\n") && output.contains("record 4499\r\n"));
    assert!(output.contains("[raw] [错误] No published ISO images were found [NO_ISO]"));
}

#[test]
fn file_filters_keep_correct_extensions_and_localized_labels() {
    let _ui_test_lock = ui_test_guard();
    for lang in [Lang::Zh, Lang::En, Lang::Ja] {
            for (kind, extension) in [
                (FileKind::Profile, "json"),
                (FileKind::Log, "txt"),
                ] {
                let (filter, actual) = dialog_filter(lang, kind);
                assert_eq!(actual, extension);
                assert!(filter.ends_with("\0\0"));
                assert!(filter.contains(tr(lang, "all_filter")));
            }
        }
    }

    #[test]
    fn import_and_custom_capacity_are_not_replaced_with_defaults() {
        let _ui_test_lock = ui_test_guard();
        assert_eq!(mode_value(mode_index("external")), "external");
        assert_eq!(mode_value(mode_index("lpcm")), "lpcm");
        assert_eq!(capacity_index(4_000_000_000), 2);
        assert_eq!(capacity_index(4_707_319_808), 0);
        assert_eq!(capacity_index(8_540_123_136), 1);
        assert_eq!(capacity_index(8_543_666_176), 2);
        assert!(valid_integer("0", 0, 10_000_000_000));
        assert!(!valid_integer("10000000001", 0, 10_000_000_000));
    }

    #[test]
    fn audio_mode_note_matches_the_selected_workflow() {
        let _ui_test_lock = ui_test_guard();
        assert_eq!(audio_note_key(0), "mlp_note");
        assert_eq!(audio_note_key(1), "lpcm_note");
        assert_eq!(audio_note_key(2), "import_note");
        assert_eq!(audio_note_key(99), "mlp_note");
        for lang in [Lang::Zh, Lang::En, Lang::Ja] {
            for mode in [0, 1, 2] {
                assert!(!tr(lang, audio_note_key(mode)).is_empty());
            }
        }
    }

    #[test]
    fn detailed_view_ignores_summary_filter_and_preserves_chatter() {
        let _ui_test_lock = ui_test_guard();
        let lines = vec![
            LogLine {
                raw: "frame=42".into(),
                problem: false,
                timestamp: "12:34:56".into(),
            },
            LogLine {
                raw: "[WAR] warning".into(),
                problem: true,
                timestamp: "12:34:57".into(),
            },
        ];
        let detail = display_log(Lang::En, &lines, true, true);
        assert!(detail.contains("12:34:56  frame=42") && detail.contains("[WAR] warning"));
        let summary = display_log(Lang::En, &lines, false, false);
        assert!(!summary.contains("frame=42") && summary.contains("[WAR] warning"));
    }

    #[test]
    fn overall_progress_stays_monotonic_and_reserves_completion_for_success() {
        let _ui_test_lock = ui_test_guard();
        let (logs, events) = sync_channel(32);
        let mut callbacks = WorkerCallbacks {
            logs,
            cancel: Arc::new(AtomicBool::new(false)),
            range: (0, 100),
            last_progress: 0,
        };
        callbacks.set_progress_range(0, 15);
        callbacks.progress(50, 100);
        callbacks.progress(20, 100);
        callbacks.progress(0, 0);
        callbacks.progress(100, 100);
        callbacks.set_progress_range(15, 65);
        callbacks.progress(40, 100);
        callbacks.set_progress_range(65, 100);
        callbacks.progress(100, 100);
        let values: Vec<_> = events
            .try_iter()
            .map(|event| match event {
                WorkerEvent::Progress(value) => value,
                _ => panic!("unexpected log event"),
            })
            .collect();
        assert_eq!(values, [7, 15, 35, 65, 99]);
    }

    #[test]
    fn task_start_is_visible_in_summary_before_progress_events() {
        let _ui_test_lock = ui_test_guard();
        for (lang, key) in [
            (Lang::Zh, "started_check"),
            (Lang::En, "started_build"),
            (Lang::Ja, "started_verify"),
        ] {
            let lines = [LogLine {
                raw: tr(Lang::Zh, key).into(),
                problem: false,
                timestamp: "12:34:56".into(),
            }];
            assert!(
                !display_log(lang, &lines, false, false).trim().is_empty(),
                "{key} should remain visible in the summary"
            );
        }
    }

    #[test]
    fn combo_measurement_preserves_original_item_height() {
        let _ui_test_lock = ui_test_guard();
        let mut item: MeasureItemStruct = unsafe { std::mem::zeroed() };
        unsafe {
            window_proc(
                ptr::null_mut(),
                WM_MEASUREITEM,
                0,
                &mut item as *mut MeasureItemStruct as Lparam,
            );
        }
        assert_eq!(item.item_height, scale(26).max(scale(20)) as u32);
    }


/// Every text the UI shows must fit the rectangle it is drawn into, in
/// every language, at every supported scale and width. Returns one line
/// per (kind, text) with the first failing condition.
fn text_clipping_findings(parent: Hwnd, ui: &mut UiState) -> Vec<String> {
    const DT_WORDBREAK: u32 = 0x10;
    const CB_GETCOUNT: u32 = 0x0146;
    const CB_GETDROPPEDWIDTH: u32 = 0x015f;
    const WM_GETFONT: u32 = 0x0031;
    let measure = |text: &str, font: Handle, width: i32, flags: u32| -> (i32, i32) {
        unsafe {
            let dc = CreateCompatibleDC(ptr::null_mut());
            let old = SelectObject(dc, font);
            let wide_text = wide(text);
            let mut rect = Rect { left: 0, top: 0, right: width.max(1), bottom: 0 };
            DrawTextW(dc, wide_text.as_ptr(), -1, &mut rect, flags | DT_CALCRECT | DT_NOPREFIX);
            SelectObject(dc, old);
            DeleteDC(dc);
            (rect.right, rect.bottom)
        }
    };
    let lift = |font: Handle, needed: i32, available: i32|  {
        let dc = CreateCompatibleDC(ptr::null_mut());
        let old = SelectObject(dc, font);
        let lift = text_lift(dc, needed, available);
        SelectObject(dc, old);
        DeleteDC(dc);
        lift
    };
    let style = |hwnd: Hwnd| GetWindowLongPtrW(hwnd, -16) as u32;
    let shown = |hwnd: Hwnd| {
        let mut current = hwnd;
        while !current.is_null() && current != parent {
            if style(current) & WS_VISIBLE == 0 {
                return false;
            }
            current = GetParent(current);
        }
        true
    };
    let client = |hwnd: Hwnd| {
        let mut rect: Rect = Rect::default();
        GetClientRect(hwnd, &mut rect);
        (rect.right, rect.bottom)
    };
    unsafe extern "system" fn collect(hwnd: Hwnd, list: Lparam) -> i32 {
        unsafe { (*(list as *mut Vec<Hwnd>)).push(hwnd) };
        1
    }
    let mut findings: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut note = |key: String, detail: String| {
        findings.entry(key).or_insert(detail);
    };
    let saved_dpi = UI_DPI.load(Ordering::Relaxed);
    let saved_lang = ui.lang();
    for dpi in [96u32, 120, 144, 168] {
        UI_DPI.store(dpi, Ordering::Relaxed);
        rescale_ui(parent, ui);
        for lang in [Lang::Zh, Lang::En, Lang::Ja] {
            set_language(ui, lang);
            for advanced in [false, true] {
                check(ui.controls.advanced, advanced);
                update_advanced(ui);
                for width in [640, 800, 1000, 1400] {
                    MoveWindow(parent, 0, 0, scale(width), scale(820), 0);
                    layout(parent, ui);
                    for tab in 0..ui.pages.len() {
                        select_tab(ui, tab);
                        let at = format!("{} {dpi}dpi {width}w adv={advanced} tab{tab}", lang.code());
                        let mut children: Vec<Hwnd> = Vec::new();
                        unsafe { EnumChildWindows(parent, collect, &mut children as *mut _ as Lparam) };
                        for hwnd in children.into_iter().filter(|hwnd| shown(*hwnd)) {
                            let class = window_class(hwnd);
                            let text = get_text(hwnd);
                            let (cw, ch) = client(hwnd);
                            let id = GetDlgCtrlID(hwnd) as usize;
                            let font = win::send_int(hwnd, WM_GETFONT, 0, 0) as Handle;
                            let font = if font.is_null() { ui_font() } else { font };
                            match class.as_str() {
                                // Status lines end in "..." by design; their texts
                                // are checked below.
                                "Static" if !text.is_empty() && style(hwnd) & 0x4000 == 0 => {
                                    // Labels wrap at word boundaries; draw_label lifts
                                    // text into the internal leading when needed.
                                    let (w, h) = measure(&text, font, cw, DT_WORDBREAK);
                                    if w > cw || h - lift(font, h, ch) > ch {
                                        note(format!("label {text:?}"), format!("needs {}x{} has {cw}x{ch} at {at}", unscale(w), unscale(h)));
                                    }
                                }
                                "Button" if style(hwnd) & 0xf == BS_OWNERDRAW && !text.is_empty() => {
                                    let (font, top, bottom, pad) = if (ID_TAB..ID_TAB + 3).contains(&id) {
                                        (semibold_font(), -2, 6, 8)
                                    } else if id == ID_BUILD {
                                        (semibold_font(), 0, 0, 8)
                                    } else {
                                        (ui_font(), 0, 0, 8)
                                    };
                                    let (w, h) = measure(&text, font, cw, DT_SINGLELINE);
                                    if w > cw - scale(pad) * 2 || h > ch - scale(top) - scale(bottom) {
                                        note(format!("button {text:?}"), format!("needs {}x{} has {}x{} at {at}", unscale(w), unscale(h), unscale(cw) - pad * 2, unscale(ch) - top - bottom));
                                    }
                                }
                                "Button" if matches!(style(hwnd) & 0xf, 2 | 3) && !text.is_empty() => {
                                    let available = cw - scale(25);
                                    let (w, h) = measure(&text, ui_font(), cw, DT_SINGLELINE);
                                    if w > available || h > ch {
                                        note(format!("checkbox {text:?}"), format!("needs {}x{} has {}x{} at {at}", unscale(w), unscale(h), unscale(available), unscale(ch)));
                                    }
                                }
                                "ComboBox" => {
                                    let count = win::send_int(hwnd, CB_GETCOUNT, 0, 0) as i32;
                                    let closed = cw - scale(36);
                                    let dropped = (win::send_int(hwnd, CB_GETDROPPEDWIDTH, 0, 0) as i32).max(cw) - scale(12);
                                    for index in 0..count {
                                        let item = unsafe { combo_item_text(hwnd, index) };
                                        let (w, _) = measure(&item, ui_font(), cw, DT_SINGLELINE);
                                        if w > closed {
                                            note(format!("combo {id} closed {item:?}"), format!("needs {} has {} at {at}", unscale(w), unscale(closed)));
                                        }
                                        if w > dropped {
                                            note(format!("combo {id} list {item:?}"), format!("needs {} has {} at {at}", unscale(w), unscale(dropped)));
                                        }
                                    }
                                }
                                "Edit" => {
                                    let (_, h) = measure("Agy国", font, cw, DT_SINGLELINE);
                                    if h > ch {
                                        note(format!("edit {id} height"), format!("needs {} has {} at {at}", unscale(h), unscale(ch)));
                                    }
                                }
                                _ => {}
                            }
                        }
                        let advanced_rect = {
                            let mut rect: Rect = Rect::default();
                            GetWindowRect(ui.controls.advanced, &mut rect);
                            rect
                        };
                        let mut last_tab: Rect = Rect::default();
                        GetWindowRect(*ui.tabs.last().unwrap(), &mut last_tab);
                        if last_tab.right > advanced_rect.left {
                            note("tabs overlap the advanced switch".into(), format!("at {at}"));
                        }
                        // Status texts the task area cycles through.
                        if tab == 0 && !advanced {
                            let (cw, ch) = client(ui.controls.status);
                            let status_font = win::send_int(ui.controls.status, WM_GETFONT, 0, 0) as Handle;
                            for key in ["ready", "started_check", "started_build", "started_verify", "done", "cancelled", "failed", "stopping"] {
                                let (w, h) = measure(tr(lang, key), status_font, cw, DT_SINGLELINE);
                                if w > cw || h > ch {
                                    note(format!("status {:?}", tr(lang, key)), format!("needs {}x{} has {}x{} at {at}", unscale(w), unscale(h), unscale(cw), unscale(ch)));
                                }
                            }
                            let stats = tr(lang, "statistics").replace("{0}", "99").replace("{1}", "59").replace("{2}", "59").replace("{3}", "9999");
                            let (cw, ch) = client(ui.controls.statistics);
                            let stats_font = win::send_int(ui.controls.statistics, WM_GETFONT, 0, 0) as Handle;
                            let (w, h) = measure(&stats, stats_font, cw, DT_SINGLELINE);
                            if w > cw || h > ch {
                                note(format!("statistics {stats:?}"), format!("needs {}x{} has {}x{} at {at}", unscale(w), unscale(h), unscale(cw), unscale(ch)));
                            }
                            // Header title and subtitle, as paint_window draws them.
                            let right = (scale(width) - scale(308)).max(scale(240));
                            let (w, h) = measure("DVD-Audio Maker", title_font(), right, DT_SINGLELINE);
                            if w > right - scale(72) || h > scale(34) {
                                note("header title".into(), format!("needs {}x{} at {at}", unscale(w), unscale(h)));
                            }
                            // Narrow windows shorten the subtitle with "..." by design.
                            let (w, h) = measure(tr(lang, "app_subtitle"), subtitle_font(), right, DT_SINGLELINE);
                            if (width >= 1000 && w > right - scale(73)) || h > scale(22) {
                                note(format!("header subtitle {:?}", tr(lang, "app_subtitle")), format!("needs {}x{} has {} at {at}", unscale(w), unscale(h), unscale(right) - 73));
                            }
                        }
                    }
                }
            }
        }
    }
    UI_DPI.store(saved_dpi, Ordering::Relaxed);
    rescale_ui(parent, ui);
    set_language(ui, saved_lang);
    check(ui.controls.advanced, false);
    update_advanced(ui);
    select_tab(ui, 0);
    findings.into_iter().map(|(key, detail)| format!("{key}: {detail}")).collect()
}

    #[test]
    fn real_controls_busy_cancel_bounded_archive_and_paused_completion() {
        let _ui_test_lock = ui_test_guard();
        #[link(name = "user32")]
        unsafe extern "system" {
            safe fn IsWindowEnabled(hwnd: Hwnd) -> i32;
        }
        use std::sync::atomic::AtomicU64;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "dvda-gui-controls-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let profile = root.join("settings.json");
        fs::write(
            &profile,
            b"{\"Version\":1,\"Language\":\"en\",\"Values\":{}}",
        )
        .unwrap();
        PROFILE_OVERRIDE.set(Some(profile)).unwrap();
        let page_class = wide("DVD_AUDIO_PAGE_RUST");
        let registration = WndClass {
            style: 0,
            procedure: page_proc,
            extra_class: 0,
            extra_window: 0,
            instance: unsafe { GetModuleHandleW(ptr::null()) },
            icon: ptr::null_mut(),
            cursor: ptr::null_mut(),
            background: ptr::null_mut(),
            menu: ptr::null(),
            class_name: page_class.as_ptr(),
        };
        unsafe {
            RegisterClassW(&registration);
            InitCommonControlsEx(&InitCommonControlsEx {
                size: std::mem::size_of::<InitCommonControlsEx>() as u32,
                classes: 0xffff,
            });
        }
        // The main window class and procedure, as in run(): state() only
        // reads user data from windows of this class.
        let main_class = wide("DVD_AUDIO_MAKER_RUST");
        let main_registration = WndClass {
            style: 0,
            procedure: window_proc,
            extra_class: 0,
            extra_window: 0,
            instance: unsafe { GetModuleHandleW(ptr::null()) },
            icon: ptr::null_mut(),
            cursor: ptr::null_mut(),
            background: ptr::null_mut(),
            menu: ptr::null(),
            class_name: main_class.as_ptr(),
        };
        unsafe { RegisterClassW(&main_registration) };
        let parent = create!(ptr::null_mut(), "DVD_AUDIO_MAKER_RUST", "", 0, 0, 0, 1180, 820, 0);
        assert!(!parent.is_null());
        let mut ui = create_controls(parent);
        layout(parent, &mut ui);
        for (combo, width) in [
            (ui.controls.capacity, 220),
            (ui.controls.norm, 260),
            (ui.controls.mode, 300),
            (ui.controls.rate, 210),
            (ui.controls.bits, 160),
            (ui.controls.language, 104),
            (ui.controls.log_view, 132),
        ] {
            let rect = window_rect(combo);
            assert_eq!(rect.right - rect.left, scale(width));
        }
        let target_rect = window_rect(ui.controls.source);
        assert_eq!(target_rect.bottom - target_rect.top, scale(24));
        let page = ui.pages[0];
        let max_scroll = page_bar_metrics(&ui.paint).max;
        scroll_page(page, scale(100), true);
        assert_eq!(GetWindowLongPtrW(page, GWLP_USERDATA), scale(100).min(max_scroll) as isize);
        layout_page_contents(&ui);
        scroll_page(page, 0, true);
        for height in [500, 600, 820] {
            MoveWindow(parent, 0, 0, scale(800), scale(height), 0);
            layout(parent, &mut ui);
            let mut log_rect: Rect = Rect::default();
            let mut action_rect: Rect = Rect::default();
             {
                GetWindowRect(ui.controls.log, &mut log_rect);
                GetWindowRect(ui.controls.build, &mut action_rect);
            }
            assert!(log_rect.bottom <= action_rect.top, "log overlaps actions at height {height}");
        }
        // Layout measures translated labels, so sweep every language.
        let original_lang = ui.lang();
        for lang in [Lang::Zh, Lang::En, Lang::Ja] {
        ui.set_lang(lang);
        for width in [640, 800, 854, 960, 1053, 1054, 1180, 1400, 960, 640] {
            for height in [360, 500, 820] {
                MoveWindow(parent, 0, 0, scale(width), scale(height), 0);
                layout(parent, &mut ui);
                // The log toolbar sits between the progress bar and the log,
                // wrapping rather than hiding controls in narrow windows.
                // Short windows hide the log first and toolbar controls
                // only where they would reach the action buttons.
                let rect_of = |hwnd: Hwnd| {
                    window_rect(hwnd)
                };
                let shown = |hwnd: Hwnd| {
                    (GetWindowLongPtrW(hwnd, -16) as u32) & WS_VISIBLE != 0
                };
                let client = window_rect(parent);
                let progress = rect_of(ui.controls.progress);
                let log_buttons: Vec<Hwnd> = ui.buttons.iter()
                    .filter(|(_, key)| matches!(*key, "copy" | "export"))
                    .map(|(handle, _)| *handle)
                    .collect();
                assert_eq!(log_buttons.len(), 2);
                let log = rect_of(ui.controls.log);
                let actions = rect_of(ui.controls.build);
                let toolbar = [ui.controls.log_view, ui.controls.only_issues, ui.controls.live]
                    .into_iter()
                    .chain(log_buttons.iter().copied())
                    .chain([label_handle(&ui, "log_section")]);
                // The layout decides with hysteresis around 500 DIPs; check what it decided.
                let compact = ui.compact_layout == Some(true);
                assert!(compact || shown(ui.controls.log), "log hidden at {width}x{height}");
                for control in toolbar {
                    assert!(shown(control) || (!shown(ui.controls.log) && compact),
                        "{} hidden at {width}x{height}", get_text(control));
                }
                let row: Vec<(Hwnd, Rect)> = [ui.controls.log_view, ui.controls.only_issues, ui.controls.live]
                    .into_iter()
                    .chain(log_buttons.iter().copied())
                    .filter(|hwnd| shown(*hwnd))
                    .map(|hwnd| (hwnd, rect_of(hwnd)))
                    .collect();
                for (index, (hwnd, rect)) in row.iter().enumerate() {
                    let text = get_text(*hwnd);
                    assert!(rect.top >= progress.bottom,
                        "{text} overlaps the progress bar at {width}x{height}");
                    assert!(rect.right <= client.right && rect.bottom <= client.bottom,
                        "{text} leaves the window at {width}x{height}");
                    assert!((!shown(ui.controls.log) || rect.bottom <= log.top) && rect.bottom <= actions.top,
                        "{text} overlaps the log or actions at {width}x{height}");
                    for (other, other_rect) in &row[index + 1..] {
                        assert!(rect.right <= other_rect.left || other_rect.right <= rect.left
                            || rect.bottom <= other_rect.top || other_rect.bottom <= rect.top,
                            "{text} overlaps {} at {width}x{height}", get_text(*other));
                    }
                }
                // Short windows show only the settings while idle and only
                // the task status and log while a task runs.
                let page = ui.pages[ui.tab_index()];
                let task_area = [ui.controls.status, ui.controls.progress,
                    label_handle(&ui, "operation_section")];
                if compact {
                    // The fixture does not register the scrollbar class.
                    let bar_shown = ui.page_scrollbar.is_null() || shown(ui.page_scrollbar);
                    assert!(shown(page) && bar_shown && ui.tabs.iter().all(|tab| shown(*tab)),
                        "settings hidden while idle at {width}x{height}");
                    assert!(task_area.iter().chain([&ui.controls.log]).all(|control| !shown(*control)),
                        "task area shown while idle at {width}x{height}");
                    let page_rect = rect_of(page);
                    assert!(page_rect.bottom <= actions.top
                        && page_rect.bottom >= actions.top - scale(12) - 1,
                        "settings do not fill the short window at {width}x{height}");
                    ui.busy = true;
                    layout(parent, &mut ui);
                    assert!(ui.pages.iter().chain(&ui.tabs).chain([&ui.page_scrollbar, &ui.controls.advanced])
                        .all(|control| !shown(*control)),
                        "settings shown during a task at {width}x{height}");
                    assert!(task_area.iter().all(|control| shown(*control)),
                        "task area hidden during a task at {width}x{height}");
                    let status = rect_of(label_handle(&ui, "operation_section"));
                    let tab = rect_of(ui.tabs[0]);
                    assert!(status.top <= tab.top + scale(2),
                        "task area does not move up during a task at {width}x{height}");
                    for control in [ui.controls.log, ui.controls.log_view, ui.controls.progress]
                        .into_iter()
                        .chain(log_buttons.iter().copied())
                        .filter(|control| shown(*control))
                    {
                        assert!(rect_of(control).bottom <= actions.top,
                            "{} reaches the actions during a task at {width}x{height}", get_text(control));
                    }
                    // Selecting a tab must not reveal a page while hidden.
                    let next = (ui.tab_index() + 1) % ui.pages.len();
                    select_tab(&mut ui, next);
                    assert!(ui.pages.iter().all(|page| !shown(*page)));
                    select_tab(&mut ui, 0);
                    ui.busy = false;
                    layout(parent, &mut ui);
                    assert!(shown(ui.pages[0]) && !shown(ui.controls.status));
                } else {
                    assert!(shown(page) && task_area.iter().all(|control| shown(*control)),
                        "tall windows hide an area at {width}x{height}");
                }
                for (page_index, ids) in [
                    (0, &[ID_SOURCE, ID_FINAL, ID_TITLE, ID_CAPACITY, ID_CUSTOM_BYTES,
                        ID_WORK_DIR, ID_BROWSE_WORK, ID_ISO_PREFIX, ID_GROUP_LIMIT][..]),
                    (1, &[ID_MODE, ID_RATE, ID_BITS, ID_IMPORT_FOLDER, ID_BROWSE_IMPORT][..]),
                    (2, &[ID_TRACKS, ID_COVER, ID_FONT_SC, ID_FONT_JP, ID_FONT_KR,
                        1007, 1008, 1009][..]),
                ] {
                    let page = ui.pages[page_index];
                    let bounds = window_rect(page);
                    for id in ids {
                        let child = GetDlgItem(page, *id as i32);
                        assert!(!child.is_null());
                        let child_rect = window_rect(child);
                        assert!(child_rect.left >= bounds.left && child_rect.right <= bounds.right,
                            "control {id} overflows page {page_index} at {width}x{height}");
                    }
                }
            }
        }
    }
    ui.set_lang(original_lang);
    let clipped = text_clipping_findings(parent, &mut ui);
    assert!(clipped.is_empty(), "clipped texts:\n{}", clipped.join("\n"));
    // Widening pixel by pixel must move every control edge in one
    // direction, changing course at most once at a layout breakpoint;
    // back-and-forth steps read as jitter while dragging the window edge.
    {
        unsafe extern "system" fn collect(hwnd: Hwnd, list: Lparam) -> i32 {
            unsafe { (*(list as *mut Vec<Hwnd>)).push(hwnd) };
            1
        }
        let saved_dpi = UI_DPI.load(Ordering::Relaxed);
        for dpi_value in [96u32, 144] {
            UI_DPI.store(dpi_value, Ordering::Relaxed);
            let mut children: Vec<Hwnd> = Vec::new();
            unsafe { EnumChildWindows(parent, collect, &mut children as *mut _ as Lparam); }
            let position = |hwnd: Hwnd| {
                let rect = window_rect(hwnd);
                let mut origin = Point { x: 0, y: 0 };
                ClientToScreen(parent, &mut origin);
                (rect.left - origin.x, rect.right - origin.x)
            };
            let mut history: Vec<Vec<(i32, i32)>> = vec![Vec::new(); children.len()];
            let mut frame: Rect = Rect::default();
            MoveWindow(parent, 0, 0, scale(700), scale(820), 0); GetWindowRect(parent, &mut frame);
            let client = client_rect(parent);
            let border = (frame.right - frame.left) - client.right;
            let widths: Vec<i32> = (scale(640)..scale(1400)).collect();
            for px in &widths {
                MoveWindow(parent, 0, 0, px + border, scale(820), 0);
                layout(parent, &mut ui);
                for (index, child) in children.iter().enumerate() {
                    history[index].push(position(*child));
                }
            }
            for (index, child) in children.iter().enumerate() {
                for (side, pick) in [("left", 0usize), ("right", 1)] {
                    let values: Vec<i32> = history[index].iter().map(|p| if pick == 0 { p.0 } else { p.1 }).collect();
                    let mut reversals = Vec::new();
                    let mut direction = 0;
                    for step in 1..values.len() {
                        let delta = values[step] - values[step - 1];
                        if delta == 0 { continue; }
                        let sign = delta.signum();
                        if direction != 0 && sign != direction {
                            reversals.push((unscale(widths[step]), values[step - 1], values[step]));
                        }
                        direction = sign;
                    }
                    assert!(reversals.len() <= 1,
                        "{} {:?} {side} edge jitters at {dpi_value} DPI: {:?}",
                        window_class(*child), get_text(*child), &reversals[..reversals.len().min(4)]);
                }
            }
        }
        UI_DPI.store(saved_dpi, Ordering::Relaxed);
        MoveWindow(parent, 0, 0, scale(1180), scale(820), 0);
        layout(parent, &mut ui);
    }
    // Growing the window pixel by pixel must move every control edge in
    // one direction, changing course at most once on each side of the
    // short-window switch. Above the default layout's height the task
    // area must stay put: only the log's lower edge and the actions row
    // may follow a dragged bottom edge.
    {
        unsafe extern "system" fn collect(hwnd: Hwnd, list: Lparam) -> i32 {
            unsafe { (*(list as *mut Vec<Hwnd>)).push(hwnd) };
            1
        }
        let saved_dpi = UI_DPI.load(Ordering::Relaxed);
        for dpi_value in [96u32, 144] {
            UI_DPI.store(dpi_value, Ordering::Relaxed);
            let mut children: Vec<Hwnd> = Vec::new();
            unsafe { EnumChildWindows(parent, collect, &mut children as *mut _ as Lparam); }
            let position = |hwnd: Hwnd| {
                let rect = window_rect(hwnd);
                let mut origin = Point { x: 0, y: 0 };
                ClientToScreen(parent, &mut origin);
                (rect.top - origin.y, rect.bottom - origin.y)
            };
            let mut frame: Rect = Rect::default();
            MoveWindow(parent, 0, 0, scale(1000), scale(820), 0); GetWindowRect(parent, &mut frame);
            let client = client_rect(parent);
            let border = (frame.bottom - frame.top) - client.bottom;
            // Growing switches out of the short layout at 504 DIPs.
            for (low, high) in [(360, 503), (504, 1200)] {
                let heights: Vec<i32> = (scale(low)..=scale(high)).collect();
                let mut history: Vec<Vec<(i32, i32)>> = vec![Vec::new(); children.len()];
                let mut client_heights: Vec<i32> = Vec::new();
                for px in &heights {
                    MoveWindow(parent, 0, 0, scale(1000), px + border, 0);
                    layout(parent, &mut ui);
                    let inner = client_rect(parent);
                    client_heights.push(inner.bottom);
                    for (index, child) in children.iter().enumerate() {
                        history[index].push(position(*child));
                    }
                }
                for (index, child) in children.iter().enumerate() {
                    for (side, pick) in [("top", 0usize), ("bottom", 1)] {
                        let values: Vec<i32> = history[index].iter().map(|p| if pick == 0 { p.0 } else { p.1 }).collect();
                        let mut reversals = Vec::new();
                        let mut direction = 0;
                        for step in 1..values.len() {
                            let delta = values[step] - values[step - 1];
                            if delta == 0 { continue; }
                            let sign = delta.signum();
                            if direction != 0 && sign != direction {
                                reversals.push((unscale(heights[step]), values[step - 1], values[step]));
                            }
                            direction = sign;
                        }
                        assert!(reversals.len() <= 1,
                            "{} {:?} {side} edge jitters at {dpi_value} DPI: {:?}",
                            window_class(*child), get_text(*child), &reversals[..reversals.len().min(4)]);
                    }
                }
                // Rows tied to the bottom edge keep an exact pixel gap to it.
                for control in [ui.controls.build, ui.controls.statistics] {
                    let index = children.iter().position(|child| *child == control).unwrap();
                    let gaps: std::collections::BTreeSet<i32> = client_heights.iter().zip(&history[index])
                        .map(|(client_height, p)| client_height - p.0)
                        .collect();
                    assert_eq!(gaps.len(), 1, "{} gap to the bottom edge wobbles at {dpi_value} DPI: {gaps:?}",
                        get_text(control));
                }
                if low == 504 {
                    // Once the default split leaves the log its floor (tabs at
                    // 126, task rows 110, actions 60), only the log grows.
                    let fixed_from = ui.default_tab_height.unwrap() + 126 + 110 + 60 + DEFAULT_LOG_FLOOR;
                    // The test window cannot exceed the screen; judge the
                    // heights it really reached.
                    let reached = *client_heights.iter().max().unwrap();
                    assert!(reached >= scale(fixed_from + 20), "sweep reached only {} DIPs at {dpi_value} DPI", unscale(reached));
                    let status = children.iter().position(|child| *child == ui.controls.status).unwrap();
                    let log = children.iter().position(|child| *child == ui.controls.log).unwrap();
                    for control in [ui.controls.status, ui.controls.progress, ui.controls.log_view, ui.controls.log] {
                        let index = children.iter().position(|child| *child == control).unwrap();
                        let tops: std::collections::BTreeSet<i32> = client_heights.iter().zip(&history[index])
                            .filter(|(client_height, _)| **client_height >= scale(fixed_from))
                            .map(|(_, p)| p.0)
                            .collect();
                        assert_eq!(tops.len(), 1, "{} moves with the height at {dpi_value} DPI", window_class(control));
                    }
                    // Below that the task area may only follow the edge while
                    // the log is at its floor.
                    for step in 1..history[status].len() {
                        if history[status][step].0 != history[status][step - 1].0 {
                            let (top, bottom) = history[log][step];
                            assert!((bottom - top - scale(DEFAULT_LOG_FLOOR)).abs() <= 1,
                                "task area moves while the log has room at {dpi_value} DPI: log {} px", bottom - top);
                        }
                    }
                }
            }
        }
        UI_DPI.store(saved_dpi, Ordering::Relaxed);
        MoveWindow(parent, 0, 0, scale(1180), scale(820), 0);
        layout(parent, &mut ui);
    }
    // Live resizing must not resize, and so re-wrap, a long log: the box
    // keeps its size (clipped to the new log area) until the drag ends.
    {
        let line = "2026-10-24 12:00:00  Encoding track 07 of 12 (MLP, 96 kHz / 24 bit, 5.1)";
        let text: String = (0..2500).map(|i| format!("{i:04} {line}{}{}", 13 as char, 10 as char)).collect();
        set_text(ui.controls.log, &text);
        MoveWindow(parent, 0, 0, scale(1180), scale(820), 0);
        layout(parent, &mut ui);
        let log_size = |hwnd: Hwnd| {
            let mut rect: Rect = Rect::default();
            GetClientRect(hwnd, &mut rect);
            (rect.right, rect.bottom)
        };
        let before = log_size(ui.controls.log);
        ui.sizing = true;
        for step in 1..=20 {
            MoveWindow(parent, 0, 0, scale(1180 - step * 3), scale(820 - step * 3), 0);
            layout(parent, &mut ui);
            assert_eq!(log_size(ui.controls.log), before, "log resized during live sizing");
        }
        assert!(ui.log_frozen);
        // The frame follows the layout while the EDIT keeps its size.
        let mut frame: Rect = Rect::default();
        GetClientRect(ui.log_frame, &mut frame);
        let target = ui.log_box.unwrap();
        assert_eq!((frame.right, frame.bottom), (target.right - target.left, target.bottom - target.top));
        ui.sizing = false;
        layout(parent, &mut ui);
        assert!(!ui.log_frozen);
        let target = ui.log_box.unwrap();
        assert_eq!(log_size(ui.controls.log), (target.right - target.left, target.bottom - target.top));
        // The real sizing messages: a resize is not a user scroll, so live
        // follow is not paused, and the log gets its size at the end.
        win::send_int(ui.controls.log, EM_LINESCROLL, 0, 2500);
        observe_log_scroll(&mut ui);
        ui.log_hold_line = None;
        ui.log_hold_until = None;
        // The window owns its state through the cell, as in WM_CREATE.
        let cell = RefCell::new(ui);
        unsafe { SetWindowLongPtrW(parent, GWLP_USERDATA, &cell as *const RefCell<UiState> as isize) };
        unsafe { window_proc(parent, 0x0231, 0, 0) };
        // Growing a log scrolled to its end reveals earlier lines, which
        // changes its first visible line.
        for step in 1..=10 {
            MoveWindow(parent, 0, 0, scale(1180), scale(820 + step * 4), 0);
            layout(parent, &mut cell.borrow_mut());
        }
        unsafe { window_proc(parent, 0x0232, 0, 0) };
        unsafe { SetWindowLongPtrW(parent, GWLP_USERDATA, 0) };
        ui = cell.into_inner();
        assert!(!ui.sizing && !ui.log_frozen);
        // The log timer observes the scroll position next.
        observe_log_scroll(&mut ui);
        assert!(ui.log_hold_until.is_none(), "resizing paused live follow");
        let target = ui.log_box.unwrap();
        assert_eq!(log_size(ui.controls.log), (target.right - target.left, target.bottom - target.top));
        // Esc in the log posts WM_CLOSE to its parent frame; the log survives.
        {
            win::send_int(ui.log_frame, WM_CLOSE, 0, 0);
            assert!(IsWindow(ui.log_frame) != 0 && IsWindow(ui.controls.log) != 0);
        }
        set_text(ui.controls.log, "");
        MoveWindow(parent, 0, 0, scale(1180), scale(820), 0);
        layout(parent, &mut ui);
    }
    // Exercise the actual controls rather than passing handcrafted blank
    // overrides to the option evaluator: population must retain auto values.
    assert_eq!(get_text(ui.controls.iso_prefix), "");
    assert_eq!(get_text(ui.controls.import_folder), "");
    set_text(ui.controls.title, "Changed title");
    let changed_work = root.join("changed-work");
    set_text(ui.controls.work_dir, &changed_work.to_string_lossy());
    let edited = edited_profile_values(&ui);
    assert_eq!(edited["DVDA_ISO_PREFIX"], "");
    assert_eq!(edited["DVDA_MLP_EXTERNAL_DIR"], "");
    let evaluated = ui
        .base
        .as_ref()
        .unwrap()
        .with_profile_values(edited)
        .unwrap();
    assert_eq!(evaluated.text("IsoPrefix"), "Changed_title");
    assert_eq!(
        PathBuf::from(evaluated.text("MlpExternalDirectory")),
        changed_work.join("mlp")
    );
    // The video standard defaults to PAL and round-trips through the profile,
    // a reload and a language switch.
    assert_eq!(combo_index(ui.controls.norm), 0);
    assert_eq!(evaluated.text("MenuNorm"), "pal");
    combo_select(ui.controls.norm, 2);
    let edited = edited_profile_values(&ui);
    assert_eq!(edited["DVDA_MENU_NORM"], "ntsc");
    let evaluated = ui.base.as_ref().unwrap().with_profile_values(edited).unwrap();
    assert_eq!(evaluated.text("MenuNorm"), "ntsc");
    combo_select(ui.controls.norm, 0);
    populate(&mut ui, &evaluated);
    assert_eq!(combo_index(ui.controls.norm), 2);
    let original_lang = ui.lang();
    set_language(&mut ui, Lang::En);
    assert_eq!(combo_index(ui.controls.norm), 2);
    assert_eq!(get_text(ui.controls.norm), "NTSC (US, Japan)");
    set_language(&mut ui, original_lang);
    populate_numeric_choice(ui.controls.rate, SAMPLE_RATES, SAMPLE_RATE_LABELS, 12345);
    populate_numeric_choice(ui.controls.bits, SAMPLE_BITS, &["16", "20", "24"], 17);
    assert_eq!(get_text(ui.controls.rate), "12345");
    assert_eq!(get_text(ui.controls.bits), "17");
    assert_eq!(edited_profile_values(&ui)["DVDA_MLP_SAMPLE_RATE"], "12345");
    assert!(validate_controls(&mut ui, false));
    assert!(!validate_controls(&mut ui, true));
    combo_select(ui.controls.rate, 1);
    assert!(!validate_controls(&mut ui, true));
    combo_select(ui.controls.bits, 2);
    assert!(validate_controls(&mut ui, true));
    if let Some((file, path)) = ui.log_file.take() {
        drop(file);
        fs::remove_file(path).unwrap();
    }
    let source = PathBuf::from(&root).join("complete.log");
    ui.log_file = Some((fs::File::create(&source).unwrap(), source.clone()));
    ui.logs.clear();
    ui.problem_count = 0;
    check(ui.controls.live, false);
    for index in 0..2700 {
        add_log(&mut ui, &format!("record {index}"), false);
    }
    assert!(ui.logs.len() <= 2500);
    let exported = root.join("export.log");
    ui.log_sender
        .try_send(WorkerEvent::Log("queued-before-export".into()))
        .unwrap();
    export_log_to(&mut ui, &exported).unwrap();
    let archived = fs::read_to_string(&exported).unwrap();
    assert!(archived.contains("record 0\n") && archived.contains("record 2699\n"));
    assert!(archived.contains("queued-before-export\n"));
    assert!(export_log_to(&mut ui, &source).is_err());
    assert_eq!(fs::read_to_string(&source).unwrap(), archived);
    let page = ui.pages[0];
    let initially_disabled = create!(page, "EDIT", "Locked", WS_CHILD, 0, 0, 100, 30, 0);
    let initially_enabled = create!(page, "EDIT", "Editable", WS_CHILD, 0, 40, 100, 30, 0);
    EnableWindow(initially_disabled, 0);
    ui.busy = true;
    set_busy_controls(&mut ui, true);
    assert_eq!(IsWindowEnabled(initially_enabled), 0);
    assert_eq!(IsWindowEnabled(initially_disabled), 0);
    for _ in 0..20 {
        scroll_page(page, i32::MAX, true);
        assert!(GetWindowLongPtrW(page, GWLP_USERDATA) > 0);
        scroll_page(page, 0, true);
        assert_eq!(GetWindowLongPtrW(page, GWLP_USERDATA), 0);
    }
    assert_eq!(IsWindowEnabled(ui.controls.language), 0);
    assert_eq!(IsWindowEnabled(ui.controls.profile), 0);
    for page in &ui.pages {
        assert_ne!(IsWindowEnabled(*page), 0);
    }
    let page = ui.pages[0];
    let label = create!(page, "STATIC", "Busy label", WS_CHILD, 0, 0, 100, 30, 0);
    let edit = create!(page, "EDIT", "Busy input", WS_CHILD, 0, 40, 100, 30, 0);
    unsafe {
        EnableWindow(page, 0);
        let dc = GetDC(page);
        assert!(!dc.is_null());
        assert_eq!(
            page_proc(page, WM_CTLCOLORSTATIC, dc as usize, label as isize),
            panel_brush() as Lresult,
        );
        assert_eq!(
            page_proc(page, WM_CTLCOLOREDIT, dc as usize, edit as isize),
            brush(c_disabled_fill()) as Lresult,
        );
        EnableWindow(edit, 0);
        assert_eq!(
            page_proc(page, WM_CTLCOLORSTATIC, dc as usize, edit as isize),
            brush(c_disabled_fill()) as Lresult,
        );
        EnableWindow(page, 1);
        assert_eq!(
            page_proc(page, WM_CTLCOLORSTATIC, dc as usize, label as isize),
            panel_brush() as Lresult,
        );
        EnableWindow(edit, 1);
        assert_eq!(
            page_proc(page, WM_CTLCOLOREDIT, dc as usize, edit as isize),
            card_brush() as Lresult,
        );
        ReleaseDC(page, dc);
        DestroyWindow(label);
        DestroyWindow(edit);
    }
    let previous_lang = ui.lang().code();
    combo_select(ui.controls.language, 2);
    handle_command(ptr::null_mut(), &mut ui, ID_LANGUAGE, CBN_SELCHANGE);
    assert_eq!(ui.lang().code(), previous_lang);
    let cancel = Arc::new(AtomicBool::new(false));
    ui.cancel = Some(Arc::clone(&cancel));
    let cell = RefCell::new(ui);
    unsafe {
        SetWindowLongPtrW(parent, GWLP_USERDATA, &cell as *const RefCell<UiState> as isize);
        window_proc(parent, WM_CLOSE, 0, 0);
        SetWindowLongPtrW(parent, GWLP_USERDATA, 0);
    }
    let mut ui = cell.into_inner();
    assert!(ui.close_when_done);
    assert!(cancel.load(Ordering::Relaxed));
    assert_eq!(IsWindowEnabled(ui.controls.cancel), 0);
    finish_operation(&mut ui, 2, "operation returned interrupted");
    assert!(get_text(ui.controls.log).contains(tr(ui.lang(), "cancelled")));
    assert!(!get_text(ui.controls.log).contains("operation returned interrupted"));
    assert_ne!(IsWindowEnabled(ui.controls.language), 0);
    assert_ne!(IsWindowEnabled(initially_enabled), 0);
    assert_eq!(IsWindowEnabled(initially_disabled), 0);
     {
        DestroyWindow(initially_enabled);
        DestroyWindow(initially_disabled);
    }
    assert!(!ui.busy);
    // More records than a Win32 message queue can hold must all reach the
    // archive before completion. Exercise the actual controls while live
    // viewing is paused, resumed and then completed with warnings.
    ui.close_when_done = false;
    ui.logs.clear();
    ui.problem_count = 0;
    ui.busy = true;
    ui.started = Some(Instant::now() - Duration::from_secs(2));
    set_busy_controls(&mut ui, true);
    let stress_path = root.join("stress.log");
    ui.log_file = Some((fs::File::create(&stress_path).unwrap(), stress_path.clone()));
    let producer_logs = ui.log_sender.clone();
    let producer = thread::spawn(move || {
        let mut callbacks = WorkerCallbacks {
            logs: producer_logs,
            cancel: Arc::new(AtomicBool::new(false)),
            range: (0, 100),
            last_progress: 0,
        };
        for index in 0..12050 {
            callbacks.emit(1, &format!("out_time_us={index}"));
        }
        callbacks.emit(2, "[警告] last warning\n    日本語.flac");
        callbacks
            .logs
            .send(WorkerEvent::Finished(UiEvent {
                kind: 1,
                text: "Completed".into(),
            }))
            .unwrap();
    });
    let paused_text = get_text(ui.controls.log);
    drain_worker_logs(ptr::null_mut(), &mut ui);
    assert_eq!(get_text(ui.controls.log), paused_text);
    check(ui.controls.live, true);
    handle_command(ptr::null_mut(), &mut ui, ID_LIVE, BN_CLICKED);
    let deadline = Instant::now() + Duration::from_secs(10);
    while ui.busy && Instant::now() < deadline {
        drain_worker_logs(ptr::null_mut(), &mut ui);
        update_statistics(&mut ui);
        thread::yield_now();
    }
    assert!(!ui.busy, "completion was lost after a progress burst");
    assert_eq!(ui.progress_value, 100);
    producer.join().unwrap();
    assert!(ui.logs.len() <= 2500);
    assert!(get_text(ui.controls.status).contains("Review the warnings"));
    assert!(get_text(ui.controls.log).contains("last warning"));
    assert!(get_text(ui.controls.log).contains("日本語.flac"));
    assert!(ui.elapsed_seconds >= 2);
    assert!(
        get_text(ui.controls.statistics).contains(&format!("0:00:{:02}", ui.elapsed_seconds))
    );
    let archived = fs::read_to_string(&stress_path).unwrap();
    let progress = archived
        .lines()
        .filter_map(|line| {
            line.split_once("out_time_us=")
                .map(|(_, index)| index.parse::<usize>().unwrap())
        })
        .collect::<Vec<_>>();
    assert_eq!(progress, (0..12050).collect::<Vec<_>>());
    assert!(archived.ends_with("Completed\n"));
    // Decode chatter must not evict task history from the summary view.
    ui.log_file = None;
    ui.logs.clear();
    ui.summary_logs.clear();
    ui.log_render_language = None;
    ui.log_hold_line = None;
    ui.log_hold_until = None;
    combo_select(ui.controls.log_view, 0);
    check(ui.controls.only_issues, false);
     {
        MoveWindow(ui.controls.log, 0, 0, 600, 80, 0);
    }
    for track in 0..40 {
        add_log(
            &mut ui,
            &format!(
                "[TASK] {}",
                json!({
                    "Action": "source_decode", "Current": track, "Total": 40,
                    "Name": format!("track-{track:02}"), "Detail": ""
                })
            ),
            false,
        );
        for index in 0..100 {
            add_log(&mut ui, &format!("out_time_us={track}{index:02}"), false);
        }
        render_log(&mut ui);
    }
    assert!(ui.logs.len() <= 2500);
    assert_eq!(ui.summary_logs.len(), 40);
    let summary = get_text(ui.controls.log);
    assert!(summary.contains("track-00") && summary.contains("track-39"));
    assert_eq!(summary.lines().count(), 40);
    assert!(
        first_visible_log_line(ui.controls.log) > 30,
        "tail-following did not reveal the newest summary row: first={}",
        first_visible_log_line(ui.controls.log)
    );
    // Preserve exactly the existing hold interval and the user's selection
    // when new rows arrive during upward review.
     {
        win::send_int(ui.controls.log, EM_SETSEL, 5, 12);
        win::send_int(ui.controls.log, EM_LINESCROLL, 0, -1000);
        win::send_int(ui.controls.log, EM_LINESCROLL, 0, 8);
    }
    observe_log_scroll(&mut ui);
    let held_line = first_visible_log_line(ui.controls.log);
    let held_until = ui.log_hold_until.unwrap();
    assert!(held_line > 0 && held_line < 15);
    add_log(&mut ui, "[build] summary-after-review", false);
    render_log(&mut ui);
    assert_eq!(first_visible_log_line(ui.controls.log), held_line);
    assert_eq!(ui.log_hold_until, Some(held_until));
    let mut start = 0u32;
    let mut end = 0u32;
    unsafe {
        SendMessageW(
            ui.controls.log,
            EM_GETSEL,
            &mut start as *mut u32 as usize,
            &mut end as *mut u32 as isize,
        );
    }
    assert_eq!((start, end), (5, 12));
    ui.log_hold_until = Some(Instant::now() - Duration::from_secs(1));
    add_log(&mut ui, "[build] summary-after-hold", false);
    render_log(&mut ui);
    assert!(first_visible_log_line(ui.controls.log) > 30);
    assert!(ui.log_hold_until.is_none());
    // Detail-buffer rotation invalidates its render cursor even if its
    // length has grown past the old count again before the next frame.
    combo_select(ui.controls.log_view, 1);
    render_log(&mut ui);
    for index in 0..3000 {
        add_log(&mut ui, &format!("detail-rotation-{index}"), false);
    }
    render_log(&mut ui);
    assert_eq!(
        get_text(ui.controls.log),
        display_log(ui.lang(), &ui.logs, true, false)
    );
    combo_select(ui.controls.log_view, 0);
    render_log(&mut ui);
    assert!(get_text(ui.controls.log).contains("track-00"));
    assert!(get_text(ui.controls.log).contains("summary-after-hold"));
    // Close the per-test archive before deleting its own temporary folder.
    ui.log_file = None;
     {
        DestroyWindow(ui.tooltip_window);
        DestroyWindow(parent);
    }
    drop(ui);
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn log_queue_applies_backpressure_and_keeps_completion_after_diagnostics() {
    let _ui_test_lock = ui_test_guard();
    use std::sync::mpsc::TrySendError;
    let (sender, receiver) = sync_channel(LOG_QUEUE_CAPACITY);
    for index in 0..LOG_QUEUE_CAPACITY {
        sender
            .try_send(WorkerEvent::Log(index.to_string()))
            .unwrap();
    }
    assert!(matches!(
        sender.try_send(WorkerEvent::Log("overflow".into())),
        Err(TrySendError::Full(_))
    ));
    let worker = thread::spawn(move || {
        sender
            .send(WorkerEvent::Finished(UiEvent {
                kind: 1,
                text: "done".into(),
            }))
            .unwrap();
    });
    for index in 0..LOG_QUEUE_CAPACITY {
        match receiver.recv().unwrap() {
            WorkerEvent::Log(text) => assert_eq!(text, index.to_string()),
            WorkerEvent::Progress(_) => panic!("progress overtook a diagnostic"),
            WorkerEvent::Retention(_) => panic!("unexpected retention request"),
            WorkerEvent::Finished(_) => panic!("completion overtook a diagnostic"),
        }
    }
    assert!(matches!(receiver.recv().unwrap(), WorkerEvent::Finished(_)));
    worker.join().unwrap();
}

#[test]
fn settings_help_and_new_runtime_messages_cover_all_languages() {
    let _ui_test_lock = ui_test_guard();
    let help: HashMap<String, String> =
        serde_json::from_str(include_str!("../locales/help.json")).unwrap();
    // DVDA_PREPARE_CACHE was retired together with its setting (one entry
    // fewer than the original baseline); DVDA_MENU_NORM added one.
    assert_eq!(help.len(), 28);
    for (key, original) in help {
        assert_eq!(setting_help(Lang::Zh, &key), original);
        for lang in [Lang::En, Lang::Ja] {
            let translated = setting_help(lang, &key);
            assert!(
                !translated.is_empty() && translated != original,
                "{key}: {translated}"
            );
        }
    }
    for source in [
        "LPCM 准备已取消。",
        "内置组件缺失: dvda-media.dll",
        "Menu data directory is missing: C:/data",
        "MLP 编码结果无效: C:/music.flac",
    ] {
        for lang in [Lang::En, Lang::Ja] {
            assert_ne!(localized_detail(lang, source), source);
        }
    }
}

#[test]
fn unicode_paths_are_opaque_in_errors_and_problem_classification() {
    let _ui_test_lock = ui_test_guard();
    let path = "D:/失败/[NO_ISO]/{0}/日本語.flac";
    let error = format!("[错误] 源文件不存在: {path} [SOURCE_MISSING]");
    for lang in [Lang::Zh, Lang::En, Lang::Ja] {
        let translated = localized_detail(lang, &error);
        assert!(translated.contains(path) && translated.ends_with("[SOURCE_MISSING]"));
    }
    let progress = json!({"Kind":"PcmProgress","Name":path,"Track":0,"Value":50}).to_string();
    assert!(!log_is_problem(&progress));
}
