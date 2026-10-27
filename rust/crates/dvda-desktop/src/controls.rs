//! Control creation (the settings pages and toolbars), owner-drawn buttons, checkboxes,
//! combo arrows, tooltips, and small text/combo helpers.

use super::*;

pub(crate) struct CreateSpec<'a> {
    pub(crate) parent: Hwnd,
    pub(crate) class: &'a str,
    pub(crate) title: &'a str,
    pub(crate) style: u32,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) id: usize,
}

pub(crate) fn create_controls(parent: Hwnd) -> UiState {
    // Only entries of destroyed windows are dropped; other windows' controls stay.
    if let Some(registry) = CONTROL_RECTS.get()
        && let Ok(mut registry) = registry.lock()
    {
        registry.retain(|(handle, _, _, _, _)| IsWindow(*handle as Hwnd) != 0);
    }
    if let Some(overlays) = COMBO_OVERLAYS.get()
        && let Ok(mut overlays) = overlays.lock()
    {
        overlays.retain(|(combo, _)| IsWindow(*combo as Hwnd) != 0);
    }

    let profile_path = PROFILE_OVERRIDE
        .get()
        .and_then(Clone::clone)
        .unwrap_or_else(dvda_core::app::default_profile_path);
    let lang = startup_language(&profile_language(&profile_path).unwrap_or_else(|| "auto".into()));
    let mut labels = Vec::new();
    let mut buttons = Vec::new();
    let mut browse = HashMap::new();
    let mut browse_buttons = Vec::new();
    let profile_label = label!(
        parent,
        tr(lang, "profile"),
        "profile",
        18,
        14,
        100,
        22,
        &mut labels,
    );
    let profile = create!(
        parent,
        "EDIT",
        "",
        WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL | WS_TABSTOP,
        120,
        11,
        620,
        26,
        ID_PROFILE,
    );
    let open_profile = button(parent, tr(lang, "open"), ID_OPEN_PROFILE, 750, 10, 110, 28);
    let save_profile_button = button(parent, tr(lang, "save"), ID_SAVE_PROFILE, 870, 10, 110, 28);
    let save_as_button = button(parent, tr(lang, "save_as"), ID_SAVE_AS, 870, 10, 110, 28);
    let language_label = label!(
        parent,
        tr(lang, "language"),
        "language",
        995,
        14,
        60,
        22,
        &mut labels,
    );
    let language = combo(parent, ID_LANGUAGE, 1055, 10, 100, 120);
    combo_add(language, tr(lang, "language_zh"));
    combo_add(language, tr(lang, "language_en"));
    combo_add(language, tr(lang, "language_ja"));
    combo_select(
        language,
        match lang {
            Lang::Zh => 0,
            Lang::En => 1,
            Lang::Ja => 2,
        },
    );
    let origin = label!(
        parent,
        &profile_path.to_string_lossy(),
        "",
        390,
        42,
        1030,
        20,
        &mut Vec::new(),
    );
    let mut tabs = Vec::new();
    for (index, key) in ["start", "audio", "menu"].iter().enumerate() {
        tabs.push(tab_button(
            parent,
            tr(lang, key),
            ID_TAB + index,
            18 + index as i32 * 138,
            126,
            132,
            34,
        ));
    }
    let mut pages = Vec::new();
    let mut c = Controls {
        statistics: ptr::null_mut(),
        advanced: ptr::null_mut(),
        profile,
        language,
        source: ptr::null_mut(),
        final_dir: ptr::null_mut(),
        title: ptr::null_mut(),
        capacity: ptr::null_mut(),
        custom_bytes: ptr::null_mut(),
        work_dir: ptr::null_mut(),
        iso_prefix: ptr::null_mut(),
        group_limit: ptr::null_mut(),
        keep_files: ptr::null_mut(),
        mode: ptr::null_mut(),
        rate: ptr::null_mut(),
        bits: ptr::null_mut(),
        import_folder: ptr::null_mut(),
        menu: ptr::null_mut(),
        stills: ptr::null_mut(),
        tracks: ptr::null_mut(),
        cover: ptr::null_mut(),
        font_sc: ptr::null_mut(),
        font_jp: ptr::null_mut(),
        font_kr: ptr::null_mut(),
        status: ptr::null_mut(),
        activity: ptr::null_mut(),
        audio_note: ptr::null_mut(),
        progress: ptr::null_mut(),
        log_view: ptr::null_mut(),
        only_issues: ptr::null_mut(),
        live: ptr::null_mut(),
        log: ptr::null_mut(),
        prepare: ptr::null_mut(),
        build: ptr::null_mut(),
        verify: ptr::null_mut(),
        cancel: ptr::null_mut(),
        open_output: ptr::null_mut(),
        profile_origin: origin,
    };
    let page_start = create_page(parent);
    pages.push(page_start);
    c.source = browse_edit!(
        page_start,
        tr(lang, "source"),
        "source",
        ID_SOURCE,
        ID_BROWSE_SOURCE,
        14,
        18,
        &mut labels,
        &mut browse,
        &mut browse_buttons,
        lang,
    );
    c.final_dir = browse_edit!(
        page_start,
        tr(lang, "final"),
        "final",
        ID_FINAL,
        ID_BROWSE_FINAL,
        14,
        66,
        &mut labels,
        &mut browse,
        &mut browse_buttons,
        lang,
    );
    c.title = text_field!(
        page_start,
        tr(lang, "title"),
        "title",
        ID_TITLE,
        14,
        106,
        470,
        &mut labels,
    );
    c.capacity = combo_field!(
        page_start,
        tr(lang, "capacity"),
        "capacity",
        ID_CAPACITY,
        520,
        106,
        220,
        &mut labels,
    );
    combo_add(c.capacity, tr(lang, "dvd5"));
    combo_add(c.capacity, tr(lang, "dvd9"));
    combo_add(c.capacity, tr(lang, "custom"));
    combo_select(c.capacity, 0);
    c.work_dir = browse_edit!(
        page_start,
        tr(lang, "work_dir"),
        "work_dir",
        ID_WORK_DIR,
        ID_BROWSE_WORK,
        14,
        150,
        &mut labels,
        &mut browse,
        &mut browse_buttons,
        lang,
    );
    c.iso_prefix = text_field!(
        page_start,
        tr(lang, "iso_prefix"),
        "iso_prefix",
        ID_ISO_PREFIX,
        14,
        194,
        460,
        &mut labels,
    );
    c.group_limit = text_field!(
        page_start,
        tr(lang, "group_limit"),
        "group_limit",
        ID_GROUP_LIMIT,
        520,
        194,
        220,
        &mut labels,
    );
    c.keep_files = check_field(
        page_start,
        tr(lang, "keep_files"),
        "keep_files",
        ID_KEEP_FILES,
        760,
        194,
        &mut labels,
    );    c.custom_bytes = text_field!(
        page_start,
        tr(lang, "custom_bytes"),
        "custom_bytes",
        ID_CUSTOM_BYTES,
        760,
        106,
        360,
        &mut labels,
    );
    let custom_bytes_label = labels
        .last()
        .map(|(handle, _)| *handle)
        .unwrap_or(ptr::null_mut());
     {
        ShowWindow(custom_bytes_label, SW_HIDE);
        ShowWindow(c.custom_bytes, SW_HIDE);
    }
    let advanced_hint = label!(
        page_start,
        tr(lang, "advanced_hint"),
        "advanced_hint",
        14,
        166,
        780,
        48,
        &mut labels,
    );
     {
        win::send_int(advanced_hint, WM_SETFONT, small_font() as usize, 1);
    }
    pages.push(create_page(parent));
    let page_audio = *pages.last().unwrap();
    c.mode = combo_field!(
        page_audio,
        tr(lang, "audio_mode"),
        "audio_mode",
        ID_MODE,
        14,
        18,
        300,
        &mut labels,
    );
    combo_add(c.mode, tr(lang, "mlp"));
    combo_add(c.mode, tr(lang, "lpcm"));
    combo_add(c.mode, tr(lang, "import"));
    combo_select(c.mode, 0);
    c.rate = combo_field!(
        page_audio,
        tr(lang, "sample_rate"),
        "sample_rate",
        ID_RATE,
        340,
        18,
        210,
        &mut labels,
    );
    for item in SAMPLE_RATE_LABELS {
        combo_add(c.rate, item);
    }
    c.bits = combo_field!(
        page_audio,
        tr(lang, "bits"),
        "bits",
        ID_BITS,
        575,
        18,
        160,
        &mut labels,
    );
    for item in ["16", "20", "24"] {
        combo_add(c.bits, item);
    }
    c.import_folder = browse_edit!(
        page_audio,
        tr(lang, "import_folder"),
        "import_folder",
        ID_IMPORT_FOLDER,
        ID_BROWSE_IMPORT,
        14,
        80,
        &mut labels,
        &mut browse,
        &mut browse_buttons,
        lang,
    );
    let import_folder_label = labels
        .last()
        .map(|(handle, _)| *handle)
        .unwrap_or(ptr::null_mut());
    let import_browse = browse_buttons.last().copied().unwrap_or(ptr::null_mut());
     {
        ShowWindow(import_folder_label, SW_HIDE);
        ShowWindow(c.import_folder, SW_HIDE);
        ShowWindow(import_browse, SW_HIDE);
    }    c.audio_note = label!(
        page_audio,
        tr(lang, "mlp_note"),
        "audio_note",
        14,
        136,
        900,
        48,
        &mut labels,
    );
    pages.push(create_page(parent));
    let page_menu = *pages.last().unwrap();
    c.menu = check_field(
        page_menu,
        tr(lang, "menu_enable"),
        "menu_enable",
        ID_MENU,
        14,
        18,
        &mut labels,
    );
    c.stills = check_field(
        page_menu,
        tr(lang, "stills"),
        "stills",
        ID_STILLS,
        300,
        18,
        &mut labels,
    );
    c.tracks = text_field!(
        page_menu,
        tr(lang, "tracks"),
        "tracks",
        ID_TRACKS,
        14,
        66,
        220,
        &mut labels,
    );
    c.cover = text_field!(
        page_menu,
        tr(lang, "cover"),
        "cover",
        ID_COVER,
        260,
        66,
        220,
        &mut labels,
    );
    c.font_sc = text_field!(
        page_menu,
        tr(lang, "font_sc"),
        "font_sc",
        ID_FONT_SC,
        14,
        120,
        300,
        &mut labels,
    );
    c.font_jp = text_field!(
        page_menu,
        tr(lang, "font_jp"),
        "font_jp",
        ID_FONT_JP,
        340,
        120,
        300,
        &mut labels,
    );
    c.font_kr = text_field!(
        page_menu,
        tr(lang, "font_kr"),
        "font_kr",
        ID_FONT_KR,
        666,
        120,
        300,
        &mut labels,
    );
    for (field, id, x) in [
        (c.font_sc, 1007, 14),
        (c.font_jp, 1008, 340),
        (c.font_kr, 1009, 666),
    ] {
        let handle = button(page_menu, tr(lang, "browse"), id, x, 174, 150, 28);
        browse.insert(id, (field, BrowseKind::Font));
        browse_buttons.push(handle);
    }
    let _operation_section = label!(
        parent,
        tr(lang, "operation_section"),
        "operation_section",
        18,
        426,
        220,
        18,
        &mut labels,
    );
    let status = label!(
        parent,
        tr(lang, "ready"),
        "ready",
        18,
        448,
        220,
        24,
        &mut labels,
    );
    let activity = label!(
        parent,
        tr(lang, "ready_hint"),
        "ready_hint",
        250,
        448,
        650,
        24,
        &mut labels,
    );
    let progress = create!(
        parent,
        "msctls_progress32",
        "",
        WS_CHILD | WS_VISIBLE,
        18,
        476,
        1140,
        8,
        0,
    );
    let _log_section = label!(
        parent,
        tr(lang, "log_section"),
        "log_section",
        18,
        488,
        220,
        18,
        &mut labels,
    );
    let log_view = combo(parent, ID_LOG_VIEW, 18, 508, 140, 120);
    combo_add(log_view, tr(lang, "summary"));
    combo_add(log_view, tr(lang, "detail"));
    combo_select(log_view, 0);
    let only_issues = checkbox(parent, tr(lang, "issues"), ID_ONLY_ISSUES, (170, 508, 150, 24), false);
    let live = checkbox(parent, tr(lang, "live"), ID_LIVE, (330, 508, 150, 24), false);
    check(live, true);
    let copy = button(parent, tr(lang, "copy"), ID_COPY, 720, 478, 125, 28);
    let export = button(parent, tr(lang, "export"), ID_EXPORT, 855, 478, 130, 28);
    let log_frame = create_log_frame(parent);
    let log = create!(
        log_frame,
        "EDIT",
        "",
        WS_CHILD
            | WS_VISIBLE
            | ES_MULTILINE
            | ES_AUTOVSCROLL
            | ES_READONLY
            | ES_WANTRETURN,
        0,
        0,
        1140,
        215,
        ID_LOG,
    );
    let page_scrollbar = create!(
        parent,
        "DVD_AUDIO_SCROLLBAR_RUST",
        "",
        WS_CHILD | WS_VISIBLE,
        0,
        0,
        12,
        200,
        0,
    );
    let log_scrollbar = create!(
        parent,
        "DVD_AUDIO_SCROLLBAR_RUST",
        "",
        WS_CHILD | WS_VISIBLE,
        0,
        0,
        12,
        200,
        0,
    );
    unsafe {
        install_scrollbar(page_scrollbar, parent);
        install_scrollbar(log_scrollbar, parent);
        touch::install(log, true);
    }
     {
        win::send_int(log, EM_SETLIMITTEXT, 1_000_000, 0);
    }
    // Keep the legacy command IDs for automation/tests, but expose the
    // complete prepare-build-verify workflow through the single build action.
    let prepare = create!(parent, "BUTTON", "", WS_CHILD, 18, 740, 145, 34, ID_PREPARE);
    let build = primary_button(parent, tr(lang, "build"), ID_BUILD, 175, 740, 145, 34);
    let verify = create!(parent, "BUTTON", "", WS_CHILD, 332, 740, 145, 34, ID_VERIFY);
    let cancel = danger_button(parent, tr(lang, "cancel"), ID_CANCEL, 490, 740, 145, 34);
    let open_output = button(
        parent,
        tr(lang, "open_output"),
        ID_OPEN_OUTPUT,
        650,
        740,
        145,
        34,
    );
    c.status = status;
    c.activity = activity;
    c.progress = progress;
    c.log_view = log_view;
    c.only_issues = only_issues;
    c.live = live;
    c.log = log;
    c.prepare = prepare;
    c.build = build;
    c.verify = verify;
    c.cancel = cancel;
    c.open_output = open_output;
    labels.extend([(only_issues, "issues"), (live, "live")]);
    buttons.extend([
        (open_profile, "open"),
        (save_profile_button, "save"),
        (save_as_button, "save_as"),
        (prepare, "check"),
        (build, "build"),
        (verify, "verify"),
        (cancel, "cancel"),
        (open_output, "open_output"),
        (copy, "copy"),
        (export, "export"),
    ]);
    c.advanced = checkbox(parent, tr(lang, "advanced"), ID_ADVANCED, (440, 42, 215, 22), false);
    labels.push((c.advanced, "advanced"));
    let caption_minimize = caption_button(parent, ID_MINIMIZE);
    let caption_maximize = caption_button(parent, ID_MAXIMIZE);
    let caption_close = caption_button(parent, ID_CLOSE);
    let theme_button = caption_button(parent, ID_THEME);
    // Preserve the old distinction between everyday and advanced settings.
    let mut advanced_controls = vec![
        c.work_dir,
        c.iso_prefix,
        c.group_limit,
        c.keep_files,
        c.font_sc,
        c.font_jp,
        c.font_kr,
    ];
    let advanced_keys = [
        "work_dir",
        "iso_prefix",
        "group_limit",
        "keep_files",
        "font_sc",
        "font_jp",
        "font_kr",
    ];
    advanced_controls.extend(
        labels
            .iter()
            .filter(|(_, key)| advanced_keys.contains(key))
            .map(|(h, _)| *h),
    );
    for (id, (field, _)) in &browse {
        if advanced_controls.contains(field)
            && let Some(button) = browse_buttons
                .iter()
                .find(|h| GetDlgCtrlID(**h) as usize == *id)
        {
            advanced_controls.push(*button);
        }
    }
    c.statistics = label!(parent, "", "", 810, 700, 330, 26, &mut Vec::new());
    let loaded = if profile_path.exists() || PROFILE_OVERRIDE.get().is_some_and(Option::is_some) {
        AppOptions::load(Some(profile_path.as_path()))
    } else {
        AppOptions::load(None)
    };
    let startup_error = loaded.as_ref().err().cloned();
    let base = loaded.ok();
    let session_log = open_session_log();
    let session_log_error = session_log.as_ref().err().map(ToString::to_string);
    let (log_sender, log_receiver) = sync_channel(LOG_QUEUE_CAPACITY);
    for line in [status, activity, c.statistics] {
        single_line_label(line);
    }
    let muted = vec![
        profile_label,
        origin,
        _operation_section,
        _log_section,
        activity,
        c.statistics,
        advanced_hint,
    ];
    let paint = Rc::new(PaintFields {
        lang: Cell::new(lang),
        tab_index: Cell::new(0),
        splitter_position: Cell::new(420),
        pages: pages.clone(),
        page_scrollbar,
        log: c.log,
        log_scrollbar,
        muted,
    });
    PAINT_FIELDS.with(|fields| fields.borrow_mut().insert(parent as usize, Rc::clone(&paint)));
    let mut state = UiState {
        paint,
        main_window: parent,
        controls: c,
        custom_bytes_label,
        import_folder_label,
        import_browse,
        pages,
        wheel_remainders: HashMap::new(),
        tabs,
        labels,
        buttons,
        base,
        profile_path,
        browse,
        browse_buttons,
        tooltip_window: ptr::null_mut(),
        tooltips: Vec::new(),
        logs: Vec::new(),
        summary_logs: Vec::new(),
        log_file: session_log.ok(),
        log_sender,
        log_receiver,
        log_dirty: false,
        log_rendered_count: 0,
        log_render_language: None,
        log_render_detail: false,
        log_render_issues_only: false,
        log_first_visible_line: 0,
        log_hold_line: None,
        log_hold_until: None,
        activity_dirty: false,
        pending_finish: None,
        status_key: "ready",
        activity_raw: tr(Lang::Zh, "ready_hint").into(),
        busy: false,
        settings_hidden: false,
        busy_disabled_controls: Vec::new(),
        close_when_done: false,
        cancel: None,
        started: None,
        elapsed_seconds: 0,
        progress_value: 0,
        problem_count: 0,
        statistics_text: String::new(),
        splitter_height: None,
        default_tab_height: None,
        splitter_tabs_y: HEADER_HEIGHT + 56,
        splitter_max: 350,
        compact_layout: None,
        dragging_splitter: false,
        sizing: false,
        log_box: None,
        log_frame,
        log_frozen: false,
        splitter_preview: None,
        splitter_preview_window: ptr::null_mut(),
        advanced_controls,
        advanced_hint,
        caption_minimize,
        caption_maximize,
        caption_close,
        theme_button,
        theme_override: None,
        page_scrollbar,
        log_scrollbar,
    };
    unsafe {
        EnableWindow(state.controls.cancel, 0);
        SetWindowTheme(
            state.controls.progress,
            wide("").as_ptr(),
            wide("").as_ptr(),
        );
        SendMessageW(
            state.controls.progress,
            PBM_SETBKCOLOR,
            0,
            c_bar_track() as isize,
        );
        SendMessageW(
            state.controls.progress,
            PBM_SETBARCOLOR,
            0,
            c_accent() as isize,
        );
        win::send_int(
            state.controls.status,
            WM_SETFONT,
            semibold_font() as usize,
            1,
        );
        win::send_int(
            state.controls.statistics,
            WM_SETFONT,
            small_font() as usize,
            1,
        );
        win::send_int(state.controls.activity, WM_SETFONT, ui_font() as usize, 1);
    }
    set_text(
        state.controls.profile,
        &state.profile_path.to_string_lossy(),
    );
    if let Some(base) = state.base.clone() {
        populate(&mut state, &base);
    }
    initialize_tooltips(parent, &mut state);
    set_language(&mut state, lang);
    if let Some(error) = startup_error {
        add_log(&mut state, &format!("[错误] {error}"), true);
    }
    if let Some(error) = session_log_error {
        let message = presentation::phrase(
            lang,
            "无法保存完整日志，请检查磁盘空间或目录权限。",
            "Cannot save the full log; check disk space and folder permissions.",
            "完全なログを保存できません。空き容量とフォルダー権限を確認してください。",
        );
        add_log(&mut state, &format!("[警告] {message} {error}"), true);
    }
    show_page(&state, 0);
    update_advanced(&state);
    update_statistics(&mut state);
    let _ = profile_label;
    let _ = language_label;
    let _ = copy;
    let _ = export;
    state
}

pub(crate) fn create_page(parent: Hwnd) -> Hwnd {
    let hwnd = create!(
        parent,
        "DVD_AUDIO_PAGE_RUST",
        "",
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
        22,
        170,
        1130,
        310,
        0,
    );
    touch::install(hwnd, false);
    hwnd
}

pub(crate) fn check_field(
    parent: Hwnd,
    caption: &str,
    key: &'static str,
    id: usize,
    x: i32,
    y: i32,
    labels: &mut Vec<(Hwnd, &'static str)>,
) -> Hwnd {
    let hwnd = checkbox(parent, caption, id, (x, y, 230, 28), true);
    labels.push((hwnd, key));
    hwnd
}

pub(crate) fn checkbox(parent: Hwnd, text: &str, id: usize, (x, y, w, h): (i32, i32, i32, i32), panel: bool) -> Hwnd {
    let hwnd = create!(
        parent,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX,
        x,
        y,
        w,
        h,
        id,
    );
    if !hwnd.is_null() {
        install_check_visual(hwnd, panel);
    }
    hwnd
}
pub(crate) fn combo(parent: Hwnd, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {

    let hwnd = create!(
        parent,
        "COMBOBOX",
        "",
        WS_CHILD
            | WS_VISIBLE
            | WS_CLIPCHILDREN // The closed-field overlay child owns these pixels.
            | WS_TABSTOP
            | CBS_DROPDOWNLIST
            | CBS_OWNERDRAWFIXED
            | CBS_HASSTRINGS,
        x,
        y,
        w,
        h,
        id,
    );
    if hwnd.is_null() {
        return hwnd;
    }

    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
     {
        GetClientRect(hwnd, &mut rect);
    }
    touch::install_combo(hwnd);
    // The overlay owns the whole closed combo so the themed border and text
    // replace the system-drawn field completely. As a child of the combo it
    // moves, hides and clips together with it; a sibling overlay lagged one
    // step behind during live resizing and exposed the white native field.
    let overlay = create_from_spec(CreateSpec {
        parent: hwnd,
        class: "BUTTON",
        title: "",
        style: WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        x: 0,
        y: 0,
        width: unscale(rect.right).max(20),
        height: unscale(rect.bottom).max(20),
        id: 0,
    });
    if !overlay.is_null() {
        install_combo_arrow(overlay, hwnd);
        if window_class(parent) == "DVD_AUDIO_PAGE_RUST" {
            touch::install(overlay, false);
        }
        let registry = COMBO_OVERLAYS.get_or_init(|| Mutex::new(Vec::new()));
        if let Ok(mut registry) = registry.lock() {
            registry.push((hwnd as isize, overlay as isize));
        }
    }
    hwnd
}

pub(crate) fn button(parent: Hwnd, text: &str, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {
    owner_draw_button(
        parent,
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        id,
        (x, y, w, h),
    )
}

pub(crate) fn primary_button(parent: Hwnd, text: &str, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {
    owner_draw_button(
        parent,
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        id,
        (x, y, w, h),
    )
}

pub(crate) fn danger_button(parent: Hwnd, text: &str, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {
    owner_draw_button(
        parent,
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        id,
        (x, y, w, h),
    )
}

pub(crate) fn tab_button(parent: Hwnd, text: &str, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {
    owner_draw_button(
        parent,
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        id,
        (x, y, w, h),
    )
}

pub(crate) fn caption_button(parent: Hwnd, id: usize) -> Hwnd {
    owner_draw_button(
        parent,
        "",
        WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        id,
        (0, 0, 40, 30),
    )
}

pub(crate) fn owner_draw_button(
    parent: Hwnd,
    text: &str,
    style: u32,
    id: usize,
    (x, y, w, h): (i32, i32, i32, i32),
) -> Hwnd {
    let hwnd = create!(parent, "BUTTON", text, style, x, y, w, h, id);
    if !hwnd.is_null() {
        install_button_visual(hwnd);
    }
    hwnd
}

pub(crate) fn install_button_visual(hwnd: Hwnd) {
    unsafe {
        let original: WndProc = std::mem::transmute(GetWindowLongPtrW(hwnd, GWLP_WNDPROC));
        let visual = Box::new(ButtonVisual {
            hover: false,
            original,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(visual) as isize);
        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, button_proc as *const () as isize);
    }
}

/// # Safety
/// Window procedure called by Windows. `hwnd` is a valid window handle, and for messages
/// that pass pointers in `lparam` (such as WM_DRAWITEM or WM_NCCALCSIZE) it points to the
/// structure the system defines for that message, valid for the duration of the call.
pub(crate) unsafe extern "system" fn button_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ButtonVisual;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    match message {
        // WM_DRAWITEM paints the whole button off-screen; the native erase
        // would flash the bare WM_CTLCOLORBTN brush between the two passes.
        WM_ERASEBKGND => return 1,
        WM_MOUSEMOVE => {
            if !unsafe { (*pointer).hover } {
                unsafe {
                    (*pointer).hover = true;
                    let mut track = TrackMouseEventData {
                        size: std::mem::size_of::<TrackMouseEventData>() as u32,
                        flags: TME_LEAVE,
                        track: hwnd,
                        hover_time: 0,
                    };
                    TrackMouseEvent(&mut track);
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
        }
        WM_MOUSELEAVE => {
            if unsafe { (*pointer).hover } {
                unsafe {
                    (*pointer).hover = false;
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
        }
        WM_NCDESTROY => {
            let original = unsafe { Box::from_raw(pointer) }.original;
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                SetWindowLongPtrW(hwnd, GWLP_WNDPROC, original as *const () as isize);
            }
            return unsafe { original(hwnd, message, wparam, lparam) };
        }
        _ => {}
    }
    let original = unsafe { (*pointer).original };
    unsafe { original(hwnd, message, wparam, lparam) }
}

/// The visual state stored in a control's user data, if the control runs
/// `owner`. Each `install_*` function sets the procedure and the user data
/// together, so a window running one of these procedures stores that visual
/// type. Any other window yields `None`.
pub(crate) fn owned_visual<T>(hwnd: Hwnd, owner: WndProc) -> Option<*mut T> {
    if GetWindowLongPtrW(hwnd, GWLP_WNDPROC) != owner as *const () as isize {
        return None;
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut T;
    (!pointer.is_null()).then_some(pointer)
}

pub(crate) fn button_hovered(hwnd: Hwnd) -> bool {
    owned_visual::<ButtonVisual>(hwnd, button_proc)
        // SAFETY: `owned_visual` returned the visual installed with `button_proc`,
        // which stays alive until WM_NCDESTROY.
        .is_some_and(|visual| unsafe { (*visual).hover })
}

pub(crate) fn install_check_visual(hwnd: Hwnd, panel: bool) {
    unsafe {
        let original: WndProc = std::mem::transmute(GetWindowLongPtrW(hwnd, GWLP_WNDPROC));
        let visual = Box::new(CheckVisual {
            hover: false,
            panel,
            original,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(visual) as isize);
        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, checkbox_proc as *const () as isize);
    }
}

/// # Safety
/// Window procedure called by Windows. `hwnd` is a valid window handle, and for messages
/// that pass pointers in `lparam` (such as WM_DRAWITEM or WM_NCCALCSIZE) it points to the
/// structure the system defines for that message, valid for the duration of the call.
pub(crate) unsafe extern "system" fn checkbox_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CheckVisual;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    match message {
        WM_PAINT => {
            unsafe { draw_checkbox(hwnd) };
            return 0;
        }
        WM_ERASEBKGND => return 1,
        WM_MOUSEMOVE => {
            if !unsafe { (*pointer).hover } {
                unsafe {
                    (*pointer).hover = true;
                    let mut track = TrackMouseEventData {
                        size: std::mem::size_of::<TrackMouseEventData>() as u32,
                        flags: TME_LEAVE,
                        track: hwnd,
                        hover_time: 0,
                    };
                    TrackMouseEvent(&mut track);
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
        }
        WM_MOUSELEAVE => {
            if unsafe { (*pointer).hover } {
                unsafe {
                    (*pointer).hover = false;
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            let original = unsafe { (*pointer).original };
            let result = unsafe { original(hwnd, message, wparam, lparam) };
            unsafe { InvalidateRect(hwnd, ptr::null(), 0) };
            return result;
        }
        WM_NCDESTROY => {
            let original = unsafe { Box::from_raw(pointer) }.original;
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                SetWindowLongPtrW(hwnd, GWLP_WNDPROC, original as *const () as isize);
            }
            return unsafe { original(hwnd, message, wparam, lparam) };
        }
        _ => {}
    }
    let original = unsafe { (*pointer).original };
    unsafe { original(hwnd, message, wparam, lparam) }
}

/// # Safety
/// `hwnd` must be a checkbox created by `checkbox`, and this must run inside its
/// WM_PAINT (it calls BeginPaint and EndPaint).
pub(crate) unsafe fn draw_checkbox(hwnd: Hwnd) {
    let mut paint: PaintStruct = unsafe { std::mem::zeroed() };
    let dc = unsafe { BeginPaint(hwnd, &mut paint) };
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        GetClientRect(hwnd, &mut rect);
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const CheckVisual;
        let panel = !pointer.is_null() && (*pointer).panel;
        let hover = !pointer.is_null() && (*pointer).hover;
        FillRect(dc, &rect, if panel { panel_brush() } else { body_brush() });
        let checked = win::send_int(hwnd, BM_GETCHECK, 0, 0) == 1;
        let enabled = IsWindowEnabled(hwnd) != 0;
        let box_size = scale(16);
        let top = (rect.bottom - box_size) / 2;
        let left = scale(2);
        let fill = if !enabled {
            c_disabled_fill()
        } else if checked {
            if hover {
                c_accent_hover()
            } else {
                c_accent()
            }
        } else {
            c_card()
        };
        let border = if !enabled {
            c_disabled_text()
        } else if checked || hover {
            c_accent()
        } else {
            c_border()
        };
        fill_round_rect(
            dc,
            &Rect {
                left,
                top,
                right: left + box_size,
                bottom: top + box_size,
            },
            scale(3),
            fill,
            border,
        );
        if checked {
            let pen = CreatePen(0, scale(2).max(1), c_header_top());
            if !pen.is_null() {
                let old = SelectObject(dc, pen);
                let cx = left + box_size / 2;
                let cy = top + box_size / 2;
                MoveToEx(dc, left + scale(4), cy, ptr::null_mut());
                LineTo(dc, cx - scale(1), top + box_size - scale(4));
                LineTo(dc, left + box_size - scale(3), top + scale(4));
                SelectObject(dc, old);
                DeleteObject(pen);
            }
        }
        let label = get_text(hwnd);
        let mut text_rect = Rect {
            left: left + box_size + scale(7),
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        draw_text_run(
            &label,
            dc,
            &mut text_rect,
            ui_font(),
            if enabled { c_ink() } else { c_disabled_text() },
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        if enabled && GetFocus() == hwnd {
            let focus = Rect {
                left: scale(1),
                top: scale(1),
                right: rect.right - scale(1),
                bottom: rect.bottom - scale(1),
            };
            DrawFocusRect(dc, &focus);
        }
        EndPaint(hwnd, &paint);
    }
}

pub(crate) fn install_combo_arrow(hwnd: Hwnd, combo: Hwnd) {
    unsafe {
        let original: WndProc = std::mem::transmute(GetWindowLongPtrW(hwnd, GWLP_WNDPROC));
        let visual = Box::new(ComboArrowVisual {
            hover: false,
            combo,
            original,
            last_index: -2,
            last_enabled: false,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(visual) as isize);
        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, combo_arrow_proc as *const () as isize);
    }
}

/// # Safety
/// Window procedure called by Windows. `hwnd` is a valid window handle, and for messages
/// that pass pointers in `lparam` (such as WM_DRAWITEM or WM_NCCALCSIZE) it points to the
/// structure the system defines for that message, valid for the duration of the call.
pub(crate) unsafe extern "system" fn combo_arrow_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ComboArrowVisual;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    match message {
        WM_PAINT => {
            unsafe { draw_combo_arrow(hwnd) };
            return 0;
        }
        WM_ERASEBKGND => return 1,
        WM_MOUSEMOVE => {
            if !unsafe { (*pointer).hover } {
                unsafe {
                    (*pointer).hover = true;
                    let mut track = TrackMouseEventData {
                        size: std::mem::size_of::<TrackMouseEventData>() as u32,
                        flags: TME_LEAVE,
                        track: hwnd,
                        hover_time: 0,
                    };
                    TrackMouseEvent(&mut track);
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
        }
        WM_MOUSELEAVE => {
            if unsafe { (*pointer).hover } {
                unsafe {
                    (*pointer).hover = false;
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
        }
        0x020a => {
            // WM_MOUSEWHEEL over a closed drop-down scrolls its page instead of
            // stopping there; an open list still scrolls itself.
            let combo = unsafe { (*pointer).combo };
            if win::send_int(combo, CB_GETDROPPEDSTATE, 0, 0) == 0 {
                let page = GetParent(combo);
                if window_class(page) == "DVD_AUDIO_PAGE_RUST" {
                    return unsafe { SendMessageW(page, message, wparam, lparam) };
                }
            }
        }
        WM_LBUTTONDOWN => return 0,
        WM_LBUTTONUP => {
            let combo = unsafe { (*pointer).combo };
             {
                if IsWindowEnabled(combo) != 0 {
                    SetFocus(combo);
                    win::send_int(combo, CB_SHOWDROPDOWN, 1, 0);
                }
            }
            return 0;
        }
        WM_NCDESTROY => {
            let original = unsafe { Box::from_raw(pointer) }.original;
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                SetWindowLongPtrW(hwnd, GWLP_WNDPROC, original as *const () as isize);
            }
            return unsafe { original(hwnd, message, wparam, lparam) };
        }
        _ => {}
    }
    let original = unsafe { (*pointer).original };
    unsafe { original(hwnd, message, wparam, lparam) }
}

/// # Safety
/// `hwnd` must be a combo arrow overlay installed by `install_combo_arrow`, and this
/// must run inside its WM_PAINT.
pub(crate) unsafe fn draw_combo_arrow(hwnd: Hwnd) {
    let mut paint: PaintStruct = unsafe { std::mem::zeroed() };
    let buffer = unsafe { begin_buffered_paint(hwnd, &mut paint, c_card()) };
    let dc = buffer.memory;
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        GetClientRect(hwnd, &mut rect);
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ComboArrowVisual;
        let combo = if pointer.is_null() {
            ptr::null_mut()
        } else {
            (*pointer).combo
        };
        let hover = !pointer.is_null() && (*pointer).hover;
        let enabled = !combo.is_null() && IsWindowEnabled(combo) != 0;
        let fill = if !enabled {
            c_disabled_fill()
        } else if hover {
            c_secondary_hover()
        } else {
            c_card()
        };
        FillRect(dc, &rect, brush(fill));
        let border = if enabled && GetFocus() == combo {
            c_accent()
        } else {
            c_border()
        };
        let border_width = scale(1).max(1);
        for edge in [
            Rect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.top + border_width,
            },
            Rect {
                left: rect.left,
                top: rect.bottom - border_width,
                right: rect.right,
                bottom: rect.bottom,
            },
            Rect {
                left: rect.left,
                top: rect.top,
                right: rect.left + border_width,
                bottom: rect.bottom,
            },
            Rect {
                left: rect.right - border_width,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            },
        ] {
            FillRect(dc, &edge, brush(border));
        }
        if !combo.is_null() {
            let index = win::send_int(combo, CB_GETCURSEL, 0, 0) as i32;
            let text = combo_item_text(combo, index);
            let mut text_rect = Rect {
                left: rect.left + scale(8),
                top: rect.top,
                right: rect.right - scale(28),
                bottom: rect.bottom,
            };
            draw_text_run(
                &text,
                dc,
                &mut text_rect,
                ui_font(),
                if enabled { c_ink() } else { c_disabled_text() },
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
            );
            let color = if !enabled {
                c_disabled_text()
            } else if hover || GetFocus() == combo {
                c_accent()
            } else {
                c_ink()
            };
            let pen = CreatePen(0, scale(1).max(1), color);
            if !pen.is_null() {
                let old = SelectObject(dc, pen);
                let center_x = rect.right - scale(13);
                let center_y = (rect.top + rect.bottom) / 2;
                let half = scale(4);
                let drop = scale(2);
                MoveToEx(dc, center_x - half, center_y - drop, ptr::null_mut());
                LineTo(dc, center_x, center_y + drop);
                LineTo(dc, center_x + half, center_y - drop);
                SelectObject(dc, old);
                DeleteObject(pen);
            }
        }
        finish_buffered_paint(hwnd, &paint, &buffer);
    }
}
pub(crate) fn sync_combo_overlays() {
    let Some(overlays) = COMBO_OVERLAYS.get() else {
        return;
    };
    let Ok(overlays) = overlays.lock() else {
        return;
    };
    for (combo, overlay) in overlays.iter() {
        let combo = *combo as Hwnd;
        let overlay = *overlay as Hwnd;
        if combo.is_null() || overlay.is_null() {
            continue;
        }
        // The overlay is a child of its combo, so only the closed-field size
        // can drift (width changes or a DPI/font change of the item height).
        let mut combo_rect: Rect = Rect::default();
        let mut overlay_rect: Rect = Rect::default();
         {
            GetClientRect(combo, &mut combo_rect);
            GetClientRect(overlay, &mut overlay_rect);
        }
        if overlay_rect.right == combo_rect.right && overlay_rect.bottom == combo_rect.bottom {
            continue;
        }
        unsafe {
            SetWindowPos(
                overlay,
                ptr::null_mut(),
                0,
                0,
                combo_rect.right.max(1),
                combo_rect.bottom.max(1),
                0x0114, // NOZORDER | NOACTIVATE | NOCOPYBITS
            );
            InvalidateRect(overlay, ptr::null(), 0);
        }
    }
}

pub(crate) fn create_from_spec(spec: CreateSpec<'_>) -> Hwnd {    let CreateSpec {
        parent,
        class,
        title,
        style,
        x,
        y,
        width,
        height,
        id,
    } = spec;
    let design = (x, y, width, height);
    let class = wide(class);
    let title = wide(title);
    let is_edit = class.as_slice() == wide("EDIT").as_slice();
    let hwnd = unsafe {
        CreateWindowExW(
            if is_edit {
                // Filled field style: the white edit surface sits on the tinted
                // panel, so no 3D client edge is needed.
                0
            } else if class.as_slice() == wide("DVD_AUDIO_PAGE_RUST").as_slice()
                || class.as_slice() == wide("DVD_AUDIO_LOG_FRAME_RUST").as_slice()
            {
                0x00010000 // WS_EX_CONTROLPARENT
            } else {
                0
            },
            class.as_ptr(),
            title.as_ptr(),
            if class.as_slice() == wide("DVD_AUDIO_PAGE_RUST").as_slice() {
                style | WS_CLIPCHILDREN
            } else {
                style
            },
            scale(x),
            scale(y),
            scale(width),
            scale(height),
            parent,
            id as Hmenu,
            GetModuleHandleW(ptr::null()),
            ptr::null_mut(),
        )
    };
    if !hwnd.is_null() {
        if window_class(hwnd) == "Static" {
            #[link(name = "comctl32")]
            unsafe extern "system" {
                fn SetWindowSubclass(hwnd: Hwnd, proc: unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam, usize, usize) -> Lresult, id: usize, data: usize) -> i32;
            }
            unsafe { SetWindowSubclass(hwnd, buffered_label_proc, 2, 0); }
        }
        if window_class(parent) == "DVD_AUDIO_PAGE_RUST" {
            touch::install(hwnd, false);
        }
        let registry = CONTROL_RECTS.get_or_init(|| Mutex::new(Vec::new()));
        if let Ok(mut registry) = registry.lock() {
            registry.push((hwnd as isize, design.0, design.1, design.2, design.3));
        }
         {
            win::send_int(hwnd, WM_SETFONT, ui_font() as usize, 1);
            if is_edit {
                // Breathing room inside the borderless field.
                let margin = scale(6);
                win::send_int(hwnd, EM_SETMARGINS, 3, (margin | (margin << 16)) as isize);
            }
        }
    }
    hwnd
}

pub(crate) fn set_text(hwnd: Hwnd, text: &str) {
    let text = wide(text);
    unsafe {
        SetWindowTextW(hwnd, text.as_ptr());
    }
}
pub(crate) fn get_text(hwnd: Hwnd) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
    let mut value = vec![0u16; len + 1];
    let count =
        unsafe { GetWindowTextW(hwnd, value.as_mut_ptr(), value.len() as i32) }.max(0) as usize;
    String::from_utf16_lossy(&value[..count])
}
pub(crate) fn window_class(hwnd: Hwnd) -> String {
    let mut buffer = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
    if len > 0 {
        String::from_utf16_lossy(&buffer[..len as usize])
    } else {
        String::new()
    }
}
pub(crate) fn check(hwnd: Hwnd, value: bool) {
     {
        win::send_int(hwnd, BM_SETCHECK, usize::from(value), 0);
    }
}
pub(crate) fn checked(hwnd: Hwnd) -> bool {
    win::send_int(hwnd, BM_GETCHECK, 0, 0) == 1
}
pub(crate) fn combo_add(hwnd: Hwnd, text: &str) {
    let text = wide(text);
    unsafe {
        SendMessageW(hwnd, CB_ADDSTRING, 0, text.as_ptr() as isize);
    }
}
pub(crate) fn combo_select(hwnd: Hwnd, index: i32) {
     {
        win::send_int(hwnd, CB_SETCURSEL, index.max(0) as usize, 0);
    }
}
pub(crate) fn combo_items(hwnd: Hwnd, items: &[&str], selected: i32) {
     {
        win::send_int(hwnd, CB_RESETCONTENT, 0, 0);
    }
    for item in items {
        combo_add(hwnd, item);
    }
    combo_select(hwnd, selected.min(items.len().saturating_sub(1) as i32));
}
pub(crate) fn combo_index(hwnd: Hwnd) -> i32 {
    win::send_int(hwnd, CB_GETCURSEL, 0, 0) as i32
}
pub(crate) fn populate_numeric_choice(hwnd: Hwnd, values: &[i64], labels: &[&str], value: i64) {
    combo_items(hwnd, labels, 0);
    let index = values.iter().position(|v| *v == value).unwrap_or_else(|| {
        // Retain unknown profile values until the user explicitly corrects them.
        combo_add(hwnd, &value.to_string());
        values.len()
    });
    combo_select(hwnd, index as i32);
}
pub(crate) fn numeric_choice(hwnd: Hwnd, values: &[i64]) -> i64 {
    usize::try_from(combo_index(hwnd))
        .ok()
        .and_then(|index| values.get(index).copied())
        .unwrap_or_else(|| parse_i64(hwnd, 0))
}

/// Status lines show one line and end in "..." instead of wrapping into
/// an invisible second line.
pub(crate) fn single_line_label(hwnd: Hwnd) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, -16) as u32;
        // SS_LEFTNOWORDWRAP | SS_NOPREFIX | SS_ENDELLIPSIS
        SetWindowLongPtrW(hwnd, -16, ((style & !0x1f) | 0x0c | 0x0080 | 0x4000) as isize);
    }
}

pub(crate) fn tooltip_info(tooltip: &Tooltip) -> ToolInfo {
    ToolInfo {
        size: std::mem::size_of::<ToolInfo>() as u32,
        flags: 0x11,
        owner: GetParent(tooltip.control),
        id: tooltip.control as usize,
        rect: Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        instance: ptr::null_mut(),
        text: tooltip.text.as_ptr() as *mut u16,
        param: 0,
        reserved: ptr::null_mut(),
    }
}

pub(crate) fn initialize_tooltips(parent: Hwnd, state: &mut UiState) {
    let tooltip_window = unsafe {
        CreateWindowExW(
            0x8,
            wide("tooltips_class32").as_ptr(),
            ptr::null(),
            0x80000003,
            0,
            0,
            0,
            0,
            parent,
            ptr::null_mut(),
            GetModuleHandleW(ptr::null()),
            ptr::null_mut(),
        )
    };
    if tooltip_window.is_null() {
        return;
    }
    state.tooltip_window = tooltip_window;
     {
        win::send_int(tooltip_window, 0x0418, 0, 460);
    }
    let c = &state.controls;
    for (control, key) in [
        (c.source, "DVDA_SRC"),
        (c.final_dir, "DVDA_FINAL_DIR"),
        (c.title, "DVDA_TITLE"),
        (c.capacity, "DVDA_DISC_BYTES"),
        (c.custom_bytes, "DVDA_DISC_BYTES"),
        (c.work_dir, "DVDA_BUILD_DIR"),
        (c.iso_prefix, "DVDA_ISO_PREFIX"),
        (c.group_limit, "DVDA_GROUP_TRACK_LIMIT"),
        (c.mode, "DVDA_MLP_SOURCE"),
        (c.rate, "DVDA_MLP_SAMPLE_RATE"),
        (c.bits, "DVDA_MLP_BITS"),
        (c.import_folder, "DVDA_MLP_EXTERNAL_DIR"),
        (c.menu, "DVDA_MENU"),
        (c.tracks, "DVDA_MENU_TRACKS_PER_PAGE"),
        (c.stills, "DVDA_MENU_STILLPICS"),
        (c.cover, "DVDA_MENU_COVER_DIM"),
        (c.font_sc, "DVDA_MENU_FONT"),
        (c.font_jp, "DVDA_MENU_FONT_JP"),
        (c.font_kr, "DVDA_MENU_FONT_KR"),
        (c.keep_files, "DVDA_KEEP_TMP"),
    ] {
        let tooltip = Tooltip {
            control,
            key,
            text: wide(&setting_help(state.lang(), key)),
        };
        let info = tooltip_info(&tooltip);
        unsafe {
            SendMessageW(tooltip_window, 0x0432, 0, &info as *const ToolInfo as isize);
        }
        state.tooltips.push(tooltip);
    }
}
