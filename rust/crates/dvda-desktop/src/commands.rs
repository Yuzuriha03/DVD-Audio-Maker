//! WM_COMMAND dispatch for buttons and combo boxes, the language switch and
//! settings refits.

use super::*;

pub(crate) fn handle_command(hwnd: Hwnd, state: &mut UiState, id: usize, notify: u16) {
    handle_command_inner(hwnd, state, id, notify);
    // Tabs, the advanced switch and the audio mode change which fields the
    // current page shows.
    refit_settings(state);
}

/// Fits the settings to the current page's content again if it changed.
pub(crate) fn refit_settings(state: &mut UiState) {
    if state.splitter_height.is_some() || state.main_window.is_null() {
        return;
    }
    let page = state.pages[state.tab_index().min(state.pages.len() - 1)];
    let fitted = unscale(page_content_height(page)) + 44;
    let mut client: Rect = Rect::default();
    GetClientRect(state.main_window, &mut client);
    if Some(fitted) != state.default_tab_height && client.bottom > 0 {
        layout(state.main_window, state);
    }
}

pub(crate) fn handle_command_inner(hwnd: Hwnd, state: &mut UiState, id: usize, notify: u16) {
    if (ID_MINIMIZE..=ID_THEME).contains(&id) && notify == BN_CLICKED {
        match id {
            ID_MINIMIZE => {
                ShowWindow(hwnd, SW_MINIMIZE);
            }
            _ => toggle_theme(hwnd, state),
        }
        return;
    }
    if state.busy
        && !matches!(
            id,
            ID_CANCEL
                | ID_LOG_VIEW
                | ID_ONLY_ISSUES
                | ID_LIVE
                | ID_COPY
                | ID_EXPORT
                | ID_OPEN_OUTPUT
        )
    {
        return;
    }

    if (ID_TAB..ID_TAB + 3).contains(&id) {
        if notify == BN_CLICKED {
            select_tab(state, id - ID_TAB);
        }
        return;
    }
    if id == ID_LANGUAGE && notify == CBN_SELCHANGE {
        set_language(
            state,
            match combo_index(state.controls.language) {
                1 => Lang::ZhHant,
                2 => Lang::En,
                3 => Lang::Ja,
                4 => Lang::Ko,
                _ => Lang::Zh,
            },
        );
        autosave(state);
        return;
    }
    if id == ID_ADVANCED {
        update_advanced(state);
        return;
    }
    if id == ID_LOG_VIEW && notify == CBN_SELCHANGE {
        render_log(state);
        return;
    }
    if (id == ID_CAPACITY || id == ID_MODE) && notify == CBN_SELCHANGE {
        update_choices(state);
        return;
    }
    if id == ID_ONLY_ISSUES || id == ID_LIVE {
        if id == ID_ONLY_ISSUES || checked(state.controls.live) {
            render_log(state);
        }
        return;
    }
    if notify != BN_CLICKED && id < 1000 {
        return;
    }
    if id >= 1000 {
        browse_target(hwnd, state, id);
        return;
    }
    if state.busy
        && matches!(
            id,
            ID_PREPARE | ID_BUILD | ID_VERIFY | ID_OPEN_PROFILE | ID_SAVE_PROFILE | ID_SAVE_AS
        )
    {
        return;
    }
    match id {
        ID_OPEN_PROFILE => choose_and_load_profile(hwnd, state),
        ID_SAVE_PROFILE => {
            save_profile(state);
            autosave(state);
        }
        ID_SAVE_AS => save_profile_as(hwnd, state),
        ID_PREPARE | ID_BUILD | ID_VERIFY => start_operation(state, id),
        ID_CANCEL => cancel_operation(state),
        ID_OPEN_OUTPUT => open_output(hwnd, state),
        ID_COPY => copy_log(state),
        ID_EXPORT => export_log(hwnd, state),
        _ => {}
    }
}

pub(crate) fn set_language(state: &mut UiState, lang: Lang) {
    state.set_lang(lang);
    state.statistics_text.clear();
    combo_select(
        state.controls.language,
        match lang {
            Lang::Zh => 0,
            Lang::ZhHant => 1,
            Lang::En => 2,
            Lang::Ja => 3,
            Lang::Ko => 4,
        },
    );
    for (hwnd, key) in &state.labels {
        set_text(*hwnd, tr(lang, key));
    }
    for (hwnd, key) in &state.buttons {
        set_text(*hwnd, tr(lang, key));
    }
    for hwnd in &state.browse_buttons {
        set_text(*hwnd, tr(lang, "browse"));
    }
    for tooltip in &mut state.tooltips {
        tooltip.text = wide(&setting_help(lang, tooltip.key));
        let info = tooltip_info(tooltip);
        unsafe {
            SendMessageW(
                state.tooltip_window,
                0x0439,
                0,
                &info as *const ToolInfo as isize,
            );
        }
    }
    let capacity = combo_index(state.controls.capacity);
    combo_items(
        state.controls.capacity,
        &[tr(lang, "dvd5"), tr(lang, "dvd9"), tr(lang, "custom")],
        capacity,
    );
    let mode = combo_index(state.controls.mode);
    combo_items(
        state.controls.mode,
        &[tr(lang, "mlp"), tr(lang, "lpcm"), tr(lang, "import")],
        mode,
    );
    let norm = combo_index(state.controls.norm);
    combo_items(
        state.controls.norm,
        &[
            tr(lang, "norm_pal"),
            tr(lang, "norm_secam"),
            tr(lang, "norm_ntsc"),
        ],
        norm,
    );
    let log_view = combo_index(state.controls.log_view);
    combo_items(
        state.controls.log_view,
        &[tr(lang, "summary"), tr(lang, "detail")],
        log_view,
    );
    for (index, key) in ["start", "audio", "menu"].iter().enumerate() {
        if let Some(tab) = state.tabs.get(index) {
            set_text(*tab, tr(lang, key));
        }
    }
    set_text(state.controls.status, tr(lang, state.status_key));
    set_text(
        state.controls.activity,
        &localized_log(lang, &state.activity_raw),
    );
    update_choices(state);
    render_log(state);
    update_statistics(state);
    let main_window = state.main_window;
    layout(main_window, state);
}
