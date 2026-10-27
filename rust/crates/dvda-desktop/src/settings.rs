//! Settings values: choices, profile load/save and validation.

use super::*;

pub(crate) const SAMPLE_RATES: &[i64] = &[44100, 48000, 88200, 96000, 176400, 192000];
pub(crate) const SAMPLE_BITS: &[i64] = &[16, 20, 24];
pub(crate) const SAMPLE_RATE_LABELS: &[&str] = &[
    "44.1 kHz",
    "48 kHz",
    "88.2 kHz",
    "96 kHz",
    "176.4 kHz",
    "192 kHz",
];

/// Menu video standards in combo order; the option value passed to the author.
pub(crate) const MENU_NORMS: &[&str] = &["pal", "secam", "ntsc"];

pub(crate) fn norm_index(norm: &str) -> i32 {
    MENU_NORMS.iter().position(|value| *value == norm).unwrap_or(0) as i32
}

pub(crate) fn norm_value(index: i32) -> &'static str {
    MENU_NORMS.get(index.max(0) as usize).copied().unwrap_or("pal")
}

pub(crate) fn capacity_index(bytes: i64) -> i32 {
    match bytes {
        0 | 4_707_319_808 => 0,
        8_540_123_136 => 1,
        _ => 2,
    }
}
pub(crate) fn mode_index(mode: &str) -> i32 {
    match mode {
        "lpcm" => 1,
        "external" => 2,
        _ => 0,
    }
}
pub(crate) fn mode_value(index: i32) -> &'static str {
    match index {
        1 => "lpcm",
        2 => "external",
        _ => "mlp-batch",
    }
}
pub(crate) fn update_choices(state: &UiState) {
    let mode = combo_index(state.controls.mode);
    let imported = mode == 2;
    set_text(
        state.controls.audio_note,
        tr(state.lang(), audio_note_key(mode)),
    );
    let custom_capacity = combo_index(state.controls.capacity) == 2;
     {
        ShowWindow(
            state.custom_bytes_label,
            if custom_capacity { SW_SHOW } else { SW_HIDE },
        );
        ShowWindow(
            state.controls.custom_bytes,
            if custom_capacity { SW_SHOW } else { SW_HIDE },
        );
        ShowWindow(
            state.import_folder_label,
            if imported { SW_SHOW } else { SW_HIDE },
        );
        ShowWindow(
            state.controls.import_folder,
            if imported { SW_SHOW } else { SW_HIDE },
        );
        ShowWindow(
            state.import_browse,
            if imported { SW_SHOW } else { SW_HIDE },
        );
        EnableWindow(state.controls.custom_bytes, i32::from(custom_capacity));
        EnableWindow(state.controls.import_folder, i32::from(imported));
        for hwnd in [
            state.controls.rate,
            state.controls.bits,
        ] {
            EnableWindow(hwnd, i32::from(!imported));
        }
    }
}

pub(crate) fn audio_note_key(mode: i32) -> &'static str {
    match mode {
        1 => "lpcm_note",
        2 => "import_note",
        _ => "mlp_note",
    }
}

pub(crate) fn raw_or_environment(values: &Map<String, Value>, key: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            values
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into()
        })
}

pub(crate) fn populate(state: &mut UiState, options: &AppOptions) {
    let raw = options.profile_values();
    set_text(state.controls.source, &options.text("SourceDirectory"));
    set_text(state.controls.final_dir, &options.text("FinalDirectory"));
    set_text(state.controls.title, &options.text("Title"));
    set_combo_value(
        state.controls.capacity,
        capacity_index(options.integer("DiscBytes")),
    );
    set_text(
        state.controls.custom_bytes,
        &options.integer("DiscBytes").to_string(),
    );
    set_text(state.controls.work_dir, &options.text("BuildDirectory"));
    set_text(
        state.controls.iso_prefix,
        &raw_or_environment(&raw, "DVDA_ISO_PREFIX"),
    );
    set_text(
        state.controls.group_limit,
        &options.integer("GroupTrackLimit").to_string(),
    );
    combo_select(state.controls.mode, mode_index(&options.text("MlpSource")));
    set_text(
        state.controls.import_folder,
        &raw_or_environment(&raw, "DVDA_MLP_EXTERNAL_DIR"),
    );
    populate_numeric_choice(
        state.controls.rate,
        SAMPLE_RATES,
        SAMPLE_RATE_LABELS,
        options.integer("MlpSampleRate"),
    );
    populate_numeric_choice(
        state.controls.bits,
        SAMPLE_BITS,
        &["16", "20", "24"],
        options.integer("MlpBits"),
    );
    check(state.controls.menu, options.boolean("MenuEnabled"));
    check(state.controls.stills, options.boolean("MenuStillPictures"));
    set_text(
        state.controls.tracks,
        &options.integer("MenuTracksPerPage").to_string(),
    );
    set_text(
        state.controls.cover,
        &options.integer("MenuCoverDim").to_string(),
    );
    combo_select(state.controls.norm, norm_index(&options.text("MenuNorm")));

    let menu_font = options.text("MenuFont");
    let menu_font_japanese = {
        let configured = options.text("MenuFontJapanese");
        if configured.trim().is_empty() {
            menu_font.clone()
        } else {
            configured
        }
    };
    let menu_font_korean = {
        let configured = options.text("MenuFontKorean");
        if configured.trim().is_empty() {
            menu_font.clone()
        } else {
            configured
        }
    };
    set_text(state.controls.font_sc, &menu_font);
    set_text(state.controls.font_jp, &menu_font_japanese);
    set_text(state.controls.font_kr, &menu_font_korean);


    check(
        state.controls.keep_files,
        options.boolean("KeepTemporary") || options.boolean("KeepIntermediate"),
    );

    update_choices(state);
}

pub(crate) fn set_combo_value(hwnd: Hwnd, index: i32) {
    combo_select(hwnd, index);
}

pub(crate) fn overrides(state: &UiState) -> Map<String, Value> {
    let mut values = Map::new();
    let put = |values: &mut Map<String, Value>, key: &str, value: Value| {
        values.insert(key.into(), value);
    };
    put(
        &mut values,
        "SourceDirectory",
        json!(get_text(state.controls.source)),
    );
    put(
        &mut values,
        "FinalDirectory",
        json!(get_text(state.controls.final_dir)),
    );
    put(&mut values, "Title", json!(get_text(state.controls.title)));
    put(
        &mut values,
        "DiscBytes",
        json!(match combo_index(state.controls.capacity) {
            1 => 8_540_123_136i64,
            2 => parse_i64(state.controls.custom_bytes, 4_707_319_808i64),
            _ => 4_707_319_808i64,
        }),
    );
    put(
        &mut values,
        "BuildDirectory",
        json!(get_text(state.controls.work_dir)),
    );
    put(
        &mut values,
        "IsoPrefix",
        json!(get_text(state.controls.iso_prefix)),
    );
    put(
        &mut values,
        "GroupTrackLimit",
        json!(parse_i64(state.controls.group_limit, 99)),
    );
    put(
        &mut values,
        "PrepareCacheEnabled",
        json!(true),
    );
    put(
        &mut values,
        "ResumeEnabled",
        json!(true),
    );
    put(
        &mut values,
        "MlpSource",
        json!(mode_value(combo_index(state.controls.mode))),
    );
    put(
        &mut values,
        "MlpExternalDirectory",
        json!(get_text(state.controls.import_folder)),
    );
    put(
        &mut values,
        "MlpSampleRate",
        json!(numeric_choice(state.controls.rate, SAMPLE_RATES)),
    );
    put(
        &mut values,
        "MlpBits",
        json!(numeric_choice(state.controls.bits, SAMPLE_BITS)),
    );

    put(
        &mut values,
        "MenuEnabled",
        json!(checked(state.controls.menu)),
    );
    put(
        &mut values,
        "MenuStillPictures",
        json!(checked(state.controls.stills)),
    );
    put(
        &mut values,
        "MenuTracksPerPage",
        json!(parse_i64(state.controls.tracks, 12)),
    );
    put(
        &mut values,
        "MenuCoverDim",
        json!(parse_i64(state.controls.cover, 35)),
    );
    put(
        &mut values,
        "MenuNorm",
        json!(norm_value(combo_index(state.controls.norm))),
    );

    put(
        &mut values,
        "MenuFont",
        json!(get_text(state.controls.font_sc)),
    );
    put(
        &mut values,
        "MenuFontJapanese",
        json!(get_text(state.controls.font_jp)),
    );
    put(
        &mut values,
        "MenuFontKorean",
        json!(get_text(state.controls.font_kr)),
    );

    let keep_files = checked(state.controls.keep_files);
    put(&mut values, "KeepTemporary", json!(keep_files));
    put(&mut values, "KeepIntermediate", json!(keep_files));
    values
}

pub(crate) fn update_advanced(state: &UiState) {
    let show = checked(state.controls.advanced);
    for control in &state.advanced_controls {
         {
            ShowWindow(*control, if show { SW_SHOW } else { SW_HIDE });
        }
    }
     {
        ShowWindow(state.advanced_hint, if show { SW_HIDE } else { SW_SHOW });
    }
}
pub(crate) fn update_statistics(state: &mut UiState) {
    let elapsed = state
        .started
        .map_or(state.elapsed_seconds, |s| s.elapsed().as_secs());
    let problems = state.problem_count;
    let text = tr(state.lang(), "statistics")
        .replace("{0}", &(elapsed / 3600).to_string())
        .replace("{1}", &format!("{:02}", elapsed / 60 % 60))
        .replace("{2}", &format!("{:02}", elapsed % 60))
        .replace("{3}", &problems.to_string());
    let display = localized_detail(state.lang(), &text);
    if state.statistics_text != display {
        set_text(state.controls.statistics, &display);
        state.statistics_text = display;
    }
}
pub(crate) fn valid_integer(value: &str, min: i64, max: i64) -> bool {
    value
        .trim()
        .parse::<i64>()
        .is_ok_and(|n| (min..=max).contains(&n))
}
pub(crate) fn validate_controls(state: &mut UiState, for_task: bool) -> bool {
    let c = &state.controls;
    let mut error = None;
    if for_task && !SAMPLE_RATES.contains(&numeric_choice(c.rate, SAMPLE_RATES)) {
        error = Some((c.rate, tr(state.lang(), "invalid_sample_rate").into()));
    } else if for_task && !SAMPLE_BITS.contains(&numeric_choice(c.bits, SAMPLE_BITS)) {
        error = Some((c.bits, tr(state.lang(), "invalid_bits").into()));
    }
    for (control, key, min, max) in [
        (c.group_limit, "group_limit", 1, 99),
        (c.tracks, "tracks", 1, 32),
        (c.cover, "cover", 0, 100),
    ] {
        if error.is_none() && !valid_integer(&get_text(control), min, max) {
            error = Some((
                control,
                format!(
                    "{}：{}",
                    tr(state.lang(), key),
                    tr(state.lang(), "invalid_integer")
                        .replace("{0}", &min.to_string())
                        .replace("{1}", &max.to_string())
                ),
            ));
            break;
        }
    }
    if error.is_none()
        && combo_index(c.capacity) == 2
        && !valid_integer(&get_text(c.custom_bytes), 0, 10_000_000_000)
    {
        error = Some((
            c.custom_bytes,
            format!(
                "{}：{}",
                tr(state.lang(), "custom_bytes"),
                tr(state.lang(), "invalid_integer")
                    .replace("{0}", "0")
                    .replace("{1}", "10000000000")
            ),
        ));
    }
    if let Some((control, message)) = error {
        check(state.controls.advanced, true);
        update_advanced(state);
        if let Some(index) = state
            .pages
            .iter()
            .position(|p| *p == GetParent(control))
        {
            select_tab(state, index);
        }
         {
            SetFocus(control);
            win::send_int(control, 0x00b1, 0, -1);
        }
        add_log(state, &message, true);
        false
    } else {
        true
    }
}

pub(crate) fn parse_i64(hwnd: Hwnd, fallback: i64) -> i64 {
    get_text(hwnd).trim().parse().unwrap_or(fallback)
}

pub(crate) fn save_profile(state: &mut UiState) {
    if !validate_controls(state, false) {
        return;
    }
    let mut path = PathBuf::from(get_text(state.controls.profile).trim());
    if path.as_os_str().is_empty() {
        path = dvda_core::app::default_profile_path();
    }
    if path.extension().is_none() {
        path.set_extension("json");
    }
    let values = edited_profile_values(state);
    match dvda_core::app::save_profile(&path, &values, state.lang().code()) {
        Ok(()) => {
            set_text(state.controls.profile, &path.to_string_lossy());
            set_text(state.controls.profile_origin, &path.to_string_lossy());
            state.profile_path = path.clone();
            state.base = AppOptions::load(Some(&path))
                .ok()
                .or_else(|| AppOptions::load(None).ok());
            add_log(
                state,
                &format!("{}{}", tr(state.lang(), "saved"), path.display()),
                false,
            );
        }
        Err(error) => add_log(
            state,
            &format!("{}{}", tr(state.lang(), "profile_error"), error),
            true,
        ),
    }
}

pub(crate) fn autosave(state: &mut UiState) {
    let path = dvda_core::app::default_profile_path();
    let values = edited_profile_values(state);
    if let Err(error) = dvda_core::app::save_profile(&path, &values, state.lang().code()) {
        add_log(state, &format!("{}{}", tr(state.lang(), "profile_error"), error), true);
    }
}
pub(crate) fn save_profile_as(hwnd: Hwnd, state: &mut UiState) {
    if !validate_controls(state, false) {
        return;
    }
    if let Some(path) = file_dialog(
        hwnd,
        true,
        &get_text(state.controls.profile),
        tr(state.lang(), "save_as"),
        state.lang(),
        FileKind::Profile,
    ) {
        set_text(state.controls.profile, &path);
        save_profile(state);
        autosave(state);
    }
}

pub(crate) fn edited_profile_values(state: &UiState) -> Map<String, Value> {
    let mut values = state
        .base
        .as_ref()
        .map(AppOptions::profile_values)
        .unwrap_or_default();

    for (key, value) in overrides(state) {
        let raw = match key.as_str() {
            "SourceDirectory" => "DVDA_SRC",
            "FinalDirectory" => "DVDA_FINAL_DIR",
            "Title" => "DVDA_TITLE",
            "DiscBytes" => "DVDA_DISC_BYTES",
            "BuildDirectory" => "DVDA_BUILD_DIR",
            "IsoPrefix" => "DVDA_ISO_PREFIX",
            "GroupTrackLimit" => "DVDA_GROUP_TRACK_LIMIT",
            "MlpSource" => "DVDA_MLP_SOURCE",
            "MlpExternalDirectory" => "DVDA_MLP_EXTERNAL_DIR",
            "MlpSampleRate" => "DVDA_MLP_SAMPLE_RATE",
            "MlpBits" => "DVDA_MLP_BITS",
            "MenuEnabled" => "DVDA_MENU",
            "MenuStillPictures" => "DVDA_MENU_STILLPICS",
            "MenuTracksPerPage" => "DVDA_MENU_TRACKS_PER_PAGE",
            "MenuCoverDim" => "DVDA_MENU_COVER_DIM",
            "MenuNorm" => "DVDA_MENU_NORM",
            "MenuFont" => "DVDA_MENU_FONT",
            "MenuFontJapanese" => "DVDA_MENU_FONT_JP",
            "MenuFontKorean" => "DVDA_MENU_FONT_KR",
            "ResumeEnabled" => "DVDA_RESUME",
            _ => continue,
        };
        let stored = Value::String(value_to_profile_string(value));

        values.insert(raw.into(), stored);
    }
    // The album index threshold is no longer user-configurable; keep the
    // historical default of four albums when profiles are written.
    values.insert(
        "DVDA_MENU_INDEX_MIN_ALBUMS".into(),
        Value::String("4".into()),
    );
    values
}

pub(crate) fn value_to_profile_string(value: Value) -> String {
    match value {
        Value::String(s) => s,
        Value::Bool(v) => {
            if v {
                "on".into()
            } else {
                "off".into()
            }
        }
        Value::Number(v) => v.to_string(),
        _ => String::new(),
    }
}

pub(crate) fn choose_and_load_profile(hwnd: Hwnd, state: &mut UiState) {
    let Some(path) = file_dialog(
        hwnd,
        false,
        &get_text(state.controls.profile),
        tr(state.lang(), "select_profile"),
        state.lang(),
        FileKind::Profile,
    ) else {
        return;
    };
    let path = PathBuf::from(path);
    match AppOptions::load(Some(&path)) {
        Ok(options) => {
            state.profile_path = path.clone();
            set_text(state.controls.profile, &path.to_string_lossy());
            set_text(state.controls.profile_origin, &path.to_string_lossy());
            state.base = Some(options.clone());
            populate(state, &options);
            if let Some(code) = profile_language(&path) {
                set_language(state, Lang::from_code(&code));
            }
            add_log(state, tr(state.lang(), "loaded"), false);
        }
        Err(error) => add_log(
            state,
            &format!("{}{}", tr(state.lang(), "profile_error"), error),
            true,
        ),
    }
}

pub(crate) fn setting_help(lang: Lang, key: &str) -> String {
    static HELP: OnceLock<HashMap<String, String>> = OnceLock::new();
    let help = HELP.get_or_init(|| {
        serde_json::from_str(include_str!("../locales/help.json")).expect("embedded settings help")
    });
    help.get(key)
        .map(|text| localization::translate(lang.code(), text))
        .unwrap_or_default()
}

pub(crate) fn profile_language(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let value: Value = serde_json::from_str(&dvda_core::config_files::decode_text(&bytes)).ok()?;
    value
        .get("Language")
        .and_then(Value::as_str)
        .map(str::to_owned)
}
