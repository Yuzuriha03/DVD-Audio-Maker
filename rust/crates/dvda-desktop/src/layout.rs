//! Window layout: the main layout pass, page contents, batched placement, the log
//! toolbar row and the log frame.

use super::*;

/// Horizontal layout of the log toolbar, in DIPs relative to the left margin.
#[derive(Debug, PartialEq)]
pub(crate) struct LogToolbar {
    label_width: i32,
    combo_x: i32,
    check_x: [i32; 2],
    check_width: [i32; 2],
    button_x: [i32; 2],
    button_width: [i32; 2],
    /// The copy/export buttons moved to a second row.
    wrap: bool,
}

pub(crate) const LOG_COMBO_WIDTH: i32 = 132;
pub(crate) const LOG_BUTTON_WIDTHS: [i32; 2] = [130, 148];

/// Fits `(minimum, preferred, priority)` slots into `available`, returning
/// `None` when even the minimums do not fit. Free space restores slots one at
/// a time, lowest priority value first, so every slot and position changes
/// monotonically with the width and nothing jitters while resizing.
pub(crate) fn fit_row(slots: &[(i32, i32, u8)], available: i32) -> Option<Vec<i32>> {
    let mut sizes: Vec<i32> = slots.iter().map(|(min, _, _)| *min).collect();
    let mut free = available - sizes.iter().sum::<i32>();
    if free < 0 {
        return None;
    }
    for priority in 0..=2 {
        for (size, (min, pref, slot_priority)) in sizes.iter_mut().zip(slots) {
            if *slot_priority == priority {
                let grow = (pref - min).min(free);
                *size += grow;
                free -= grow;
            }
        }
    }
    Some(sizes)
}

/// Text widths are measured DIPs. Spacing shrinks first (the empty space
/// after the title and checkbox texts, then the gaps between controls), then
/// the button padding; the buttons wrap only when even that cannot fit.
pub(crate) fn log_toolbar_layout(available: i32, label_text: i32, check_text: [i32; 2], button_text: [i32; 2]) -> LogToolbar {
    const BUTTON: u8 = 0;
    const GAP: u8 = 1;
    const SLACK: u8 = 2;
    let check = |text: i32| (text + 27, (text + 27).max(130), SLACK);
    let button = |text: i32, width: i32| (text + 24, (text + 24).max(width), BUTTON);
    let left = [
        (label_text + 4, (label_text + 8).max(120), SLACK),
        (6, 8, GAP),
        (LOG_COMBO_WIDTH, LOG_COMBO_WIDTH, BUTTON),
        (8, 16, GAP),
        check(check_text[0]),
        (6, 8, GAP),
        check(check_text[1]),
    ];
    let right = [
        button(button_text[0], LOG_BUTTON_WIDTHS[0]),
        (6, 8, GAP),
        button(button_text[1], LOG_BUTTON_WIDTHS[1]),
    ];
    let single: Vec<_> = left.iter().chain(&[(12, 12, GAP)]).chain(&right).copied().collect();
    let (left_sizes, right_sizes, wrap) = match fit_row(&single, available) {
        Some(sizes) => (sizes[..7].to_vec(), sizes[8..].to_vec(), false),
        None => (
            fit_row(&left, available).unwrap_or_else(|| left.iter().map(|(min, _, _)| *min).collect()),
            fit_row(&right, available).unwrap_or_else(|| right.iter().map(|(min, _, _)| *min).collect()),
            true,
        ),
    };
    let mut x = [0; 7];
    for index in 1..7 {
        x[index] = x[index - 1] + left_sizes[index - 1];
    }
    // Buttons stay right-aligned; leftover space opens up before them.
    let export_x = available - right_sizes[2];
    let copy_x = export_x - right_sizes[1] - right_sizes[0];
    LogToolbar {
        label_width: left_sizes[0],
        combo_x: x[2],
        check_x: [x[4], x[6]],
        check_width: [left_sizes[4], left_sizes[6]],
        button_x: [copy_x, export_x],
        button_width: [right_sizes[0], right_sizes[2]],
        wrap,
    }
}

#[cfg(test)]
mod log_toolbar_tests {
    use super::*;

    #[test]
    fn wide_rows_keep_preferred_spacing() {
        let _ui_test_lock = ui_test_guard();
        let bar = log_toolbar_layout(1200, 60, [80, 70], [90, 100]);
        assert!(!bar.wrap);
        assert_eq!((bar.label_width, bar.combo_x, bar.check_x, bar.check_width), (120, 128, [276, 414], [130, 130]));
        assert_eq!((bar.button_x, bar.button_width), ([1200 - 148 - 8 - 130, 1200 - 148], [130, 148]));
    }

    #[test]
    fn spacing_shrinks_before_wrapping() {
        let _ui_test_lock = ui_test_guard();
        let (label, checks, buttons) = (60, [80, 70], [90, 100]);
        // Minimum single row: 64+6+132+8+107+6+97 + 12 + 114+6+124.
        let minimum = 676;
        let tight = log_toolbar_layout(minimum, label, checks, buttons);
        assert!(!tight.wrap);
        assert_eq!(tight.check_width, [107, 97]);
        assert_eq!(tight.button_width, [114, 124]);
        assert_eq!(tight.button_x[0], tight.check_x[1] + tight.check_width[1] + 12);
        assert!(log_toolbar_layout(minimum - 1, label, checks, buttons).wrap);
        let mut previous = tight;
        for available in minimum + 1..900 {
            let bar = log_toolbar_layout(available, label, checks, buttons);
            assert!(!bar.wrap);
            assert!(bar.combo_x >= previous.combo_x && bar.check_x[1] >= previous.check_x[1]);
            assert!(bar.button_x[0] >= previous.button_x[0], "copy moved left at {available}");
            assert!(bar.button_x[0] >= bar.check_x[1] + bar.check_width[1] + 12);
            assert_eq!(bar.button_x[1] + bar.button_width[1], available);
            previous = bar;
        }
    }

    #[test]
    fn wrapped_rows_fit_independently() {
        let _ui_test_lock = ui_test_guard();
        let bar = log_toolbar_layout(604, 130, [80, 70], [90, 100]);
        assert!(bar.wrap);
        assert!(bar.check_x[1] + bar.check_width[1] <= 604);
        assert_eq!(bar.button_x[1] + bar.button_width[1], 604);
        assert!(bar.button_x[0] >= 0);
    }
}

/// Control moves recorded while a layout batch is open. Only the last
/// target of each control counts: layout code may position a control more
/// than once, and a DeferWindowPos batch must not contain the same window
/// twice (Windows then keeps the first entry, so consecutive layouts would
/// alternate between both positions). The batch end applies every move with
/// one BeginDeferWindowPos per parent, recomputing visible regions once
/// instead of after every control.
#[derive(Default)]
pub(crate) struct PlacementBatch {
    /// (control, parent, target client rectangle) in first-recorded order.
    pending: Vec<(Hwnd, Hwnd, Rect)>,
}

thread_local! {
    static PLACEMENT_BATCH: std::cell::RefCell<Option<PlacementBatch>> = const { std::cell::RefCell::new(None) };
}

pub(crate) const PLACE_FLAGS: u32 = 0x011C; // NOZORDER | NOREDRAW | NOACTIVATE | NOCOPYBITS

/// Starts batching control moves until `end_placement_batch`. Code in
/// between must not read back the geometry of controls it moved.
pub(crate) fn begin_placement_batch() {
    PLACEMENT_BATCH.with(|batch| {
        let mut batch = batch.borrow_mut();
        debug_assert!(batch.is_none(), "placement batches do not nest");
        *batch = Some(PlacementBatch::default());
    });
}

/// Whether the control already occupies `target` in its parent's client
/// coordinates. Closed combo boxes keep their own height; only the
/// drop-down height is requested, so it is not compared.
pub(crate) fn placement_matches(hwnd: Hwnd, parent: Hwnd, target: &Rect) -> bool {
    let mut rect: Rect = Rect::default();
    GetWindowRect(hwnd, &mut rect);
    let mut origin = Point { x: rect.left, y: rect.top };
    ScreenToClient(parent, &mut origin);
    let fixed_combo_height = COMBO_IDS.contains(&(GetDlgCtrlID(hwnd) as usize));
    origin.x == target.left
        && origin.y == target.top
        && rect.right - rect.left == target.right - target.left
        && (fixed_combo_height || rect.bottom - rect.top == target.bottom - target.top)
}

/// Applies every recorded move at once.
pub(crate) fn end_placement_batch() {
    let Some(batch) = PLACEMENT_BATCH.with(|batch| batch.borrow_mut().take()) else {
        return;
    };
    let moves: Vec<(Hwnd, Hwnd, Rect)> = batch
        .pending
        .into_iter()
        .filter(|(hwnd, parent, target)| !placement_matches(*hwnd, *parent, target))
        .collect();
    let mut parents: Vec<Hwnd> = Vec::new();
    for (_, parent, _) in &moves {
        if !parents.contains(parent) {
            parents.push(*parent);
        }
    }
    for parent in parents {
        let group: Vec<&(Hwnd, Hwnd, Rect)> = moves.iter().filter(|(_, owner, _)| *owner == parent).collect();
        let mut handle = unsafe { BeginDeferWindowPos(group.len() as i32) };
        for (hwnd, _, target) in &group {
            if handle.is_null() {
                break;
            }
            handle = unsafe {
                DeferWindowPos(
                    handle,
                    *hwnd,
                    ptr::null_mut(),
                    target.left,
                    target.top,
                    target.right - target.left,
                    target.bottom - target.top,
                    PLACE_FLAGS,
                )
            };
        }
        if handle.is_null() {
            // A failed DeferWindowPos frees the whole batch; move directly.
            for (hwnd, _, target) in &group {
                apply_placement(*hwnd, *target);
            }
        } else {
            unsafe { EndDeferWindowPos(handle) };
            for (hwnd, _, _) in &group {
                unsafe { InvalidateRect(*hwnd, ptr::null(), 0) };
            }
        }
    }
}

pub(crate) fn apply_placement(hwnd: Hwnd, target: Rect) {
    unsafe {
        SetWindowPos(
            hwnd,
            ptr::null_mut(),
            target.left,
            target.top,
            target.right - target.left,
            target.bottom - target.top,
            PLACE_FLAGS,
        );
        InvalidateRect(hwnd, ptr::null(), 0);
    }
}

/// The multi-line log EDIT re-wraps all of its text whenever its size
/// changes (about 120 ms with 2500 lines). It lives in a frame window that
/// follows the layout on every step; during live sizing the EDIT keeps its
/// size inside the frame (clipped by it, or with the frame's log background
/// beyond it) and gets the final size once when the drag ends.
/// `revealed` marks a log that becomes visible in this layout: it gets its
/// real size right away instead of staying at its hidden size until the end.
pub(crate) fn place_log(state: &mut UiState, target: Rect, revealed: bool) {
    place_px(state.log_frame, target);
    let log = state.controls.log;
    let mut current: Rect = Rect::default();
    GetWindowRect(log, &mut current);
    let size = (target.right - target.left, target.bottom - target.top);
    let current_size = (current.right - current.left, current.bottom - current.top);
    if state.sizing && !revealed && current_size.0 > 0 && current_size.1 > 0 {
        state.log_frozen = current_size != size;
    } else {
        place_px(log, Rect { left: 0, top: 0, right: size.0, bottom: size.1 });
        state.log_frozen = false;
    }
}

/// Frame around the log EDIT: paints the log background and passes the
/// EDIT's notifications and colour requests on to the main window.
/// # Safety
/// Window procedure called by Windows. `hwnd` is a valid window handle, and for messages
/// that pass pointers in `lparam` (such as WM_DRAWITEM or WM_NCCALCSIZE) it points to the
/// structure the system defines for that message, valid for the duration of the call.
pub(crate) unsafe extern "system" fn log_frame_proc(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult {
    match message {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut paint: PaintStruct = unsafe { std::mem::zeroed() };
            let buffer = unsafe { begin_buffered_paint(hwnd, &mut paint, c_card()) };
            unsafe { finish_buffered_paint(hwnd, &paint, &buffer) };
            0
        }
        WM_COMMAND | WM_NOTIFY | WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT => unsafe {
            SendMessageW(GetParent(hwnd), message, wparam, lparam)
        },
        // A multi-line EDIT in dialog navigation answers Esc with WM_CLOSE to
        // its parent; letting DefWindowProc destroy the frame would take the
        // log with it. Esc in the log does nothing.
        WM_CLOSE => 0,
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

pub(crate) static LOG_FRAME_CLASS_READY: OnceLock<()> = OnceLock::new();

pub(crate) fn create_log_frame(parent: Hwnd) -> Hwnd {
    LOG_FRAME_CLASS_READY.get_or_init(|| unsafe {
        let class_name = wide("DVD_AUDIO_LOG_FRAME_RUST");
        let registration = WndClass {
            style: 0,
            procedure: log_frame_proc,
            extra_class: 0,
            extra_window: 0,
            instance: GetModuleHandleW(ptr::null()),
            icon: ptr::null_mut(),
            cursor: LoadCursorW(ptr::null_mut(), 32512usize as *const u16),
            background: ptr::null_mut(),
            menu: ptr::null(),
            class_name: class_name.as_ptr(),
        };
        let _ = RegisterClassW(&registration);
    });
    create!(parent, "DVD_AUDIO_LOG_FRAME_RUST", "", WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN, 18, 540, 1140, 215, 0)
}

pub(crate) fn place(hwnd: Hwnd, x: i32, y: i32, width: i32, height: i32) {
    place_px(hwnd, Rect {
        left: scale(x),
        top: scale(y),
        right: scale(x) + scale(width),
        bottom: scale(y) + scale(height),
    });
}

/// Like place(), with a target rectangle in pixels.
pub(crate) fn place_px(hwnd: Hwnd, target: Rect) {
    let parent = GetParent(hwnd);
    let recorded = PLACEMENT_BATCH.with(|batch| {
        let mut batch = batch.borrow_mut();
        let Some(batch) = batch.as_mut() else {
            return false;
        };
        match batch.pending.iter_mut().find(|(control, _, _)| *control == hwnd) {
            Some(entry) => entry.2 = target,
            None => batch.pending.push((hwnd, parent, target)),
        }
        true
    });
    if !recorded && !placement_matches(hwnd, parent, &target) {
        apply_placement(hwnd, target);
    }
}

pub(crate) fn rescale_ui(hwnd: Hwnd, state: &mut UiState) {
    if let Some(registry) = CONTROL_RECTS.get()
        && let Ok(registry) = registry.lock()
    {
        for (handle, x, y, width, height) in registry.iter() {
            // Only this window's controls (their root is the main window).
            if GetAncestor(*handle as Hwnd, 2) != hwnd {
                continue;
            }
            // layout() places the log and its frame; moving the EDIT here
            // would look like a user scroll and pause live follow.
            if *handle as Hwnd == state.controls.log || *handle as Hwnd == state.log_frame {
                continue;
            }
             {
                MoveWindow(
                    *handle as Hwnd,
                    scale(*x),
                    scale(*y),
                    scale(*width),
                    scale(*height),
                    1,
                );
            }
        }
    }
    refresh_fonts(hwnd, state);
    layout(hwnd, state);
}

/// Settings height before the first full-size layout, in DIPs.
pub(crate) const DEFAULT_SETTINGS_HEIGHT: i32 = 300;
/// The log shows at least one line.
pub(crate) const MIN_LOG_HEIGHT: i32 = 28;
/// The default split shrinks the settings before the log drops below this.
pub(crate) const DEFAULT_LOG_FLOOR: i32 = 60;

pub(crate) fn layout(hwnd: Hwnd, state: &mut UiState) {
    // Record a real user scroll before resizing the log changes its first
    // visible line; the change caused by the resize itself is taken over
    // below, so it does not pause live follow.
    observe_log_scroll(state);
    DEFER_CONTROL_PAINT.store(true, Ordering::Relaxed);
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
     {
        GetClientRect(hwnd, &mut rect);
    }
    let width = unscale(rect.right - rect.left).max(640);
    let height = unscale(rect.bottom - rect.top).max(1);
    // Short-window switch at 500 DIPs with a small dead zone, so a dragged
    // edge hovering around the threshold does not flip the whole layout.
    let compact = match state.compact_layout {
        Some(true) => height < 504,
        Some(false) => height < 496,
        None => height < 500,
    };
    state.compact_layout = Some(compact);
    let margin = 18;
    let bottom = height - 14;
    let toolbar_y = HEADER_HEIGHT + 14;
    let tabs_y = toolbar_y + 42;
    let actions_top = bottom - 34;
    let language_width = 104;
    // Profile actions follow their translated label so Japanese and English
    // never clip the text.
    let mut header_widths = [92i32; 3];
    for (handle, key) in &state.buttons {
        let index = match *key {
            "open" => 0,
            "save" => 1,
            "save_as" => 2,
            _ => continue,
        };
        let text = get_text(*handle);
        header_widths[index] =
            (unscale(measured_text_width(hwnd, &text, ui_font())) + 30).clamp(92, 240);
    }
    let mut header_x = [0i32; 3];
    let mut right = width - margin;
    for index in (0..3).rev() {
        header_x[index] = right - header_widths[index];
        right = header_x[index] - 8;
    }
    let profile_label_width = unscale(measured_text_width(hwnd, tr(state.lang(), "profile"), ui_font())) + 8;
    let profile_x = margin + profile_label_width + 6;
    let stacked_profile = header_x[0] - 12 - profile_x < 120;
    let action_toolbar_y = toolbar_y + if stacked_profile { 34 } else { 0 };
    let tabs_y = tabs_y + if stacked_profile { 34 } else { 0 };
    // The log toolbar is sized from its translated texts; narrow windows
    // tighten its spacing first and wrap copy/export only when necessary.
    let lang = state.lang();
    let text_width = |key| unscale(measured_text_width(hwnd, tr(lang, key), ui_font()));
    let log_bar = log_toolbar_layout(
        width - margin * 2,
        text_width("log_section"),
        [text_width("issues"), text_width("live")],
        [text_width("copy"), text_width("export")],
    );
    // The settings keep their height (the default or the user's splitter
    // position) while the window height changes: the log absorbs the change
    // down to one line, so a dragged bottom edge moves only the log's lower
    // edge and the actions row. Moving the whole task area with the edge
    // made it visibly shake, as DWM shows it a frame early or late.
    let task_rows = if log_bar.wrap { 144 } else { 110 };
    // A dragged splitter may leave the log one line; the default split keeps
    // a few lines for a running task.
    let settings_room = |log_floor: i32| (actions_top - 12 - log_floor - task_rows - tabs_y).max(104);
    // By default the settings are exactly as tall as the current page's
    // content (it depends on the width, the tab and the shown fields, never
    // on the window height), so no empty band is left below it.
    let tab = state.tab_index();
    let page = state.pages[tab.min(state.pages.len() - 1)];
    state.default_tab_height = Some(unscale(page_content_height(page)) + 44);
    let tab_height = match state.splitter_height {
        Some(dragged) => dragged.clamp(104, settings_room(MIN_LOG_HEIGHT)),
        None => state
            .default_tab_height
            .unwrap_or(DEFAULT_SETTINGS_HEIGHT)
            .clamp(104, settings_room(DEFAULT_LOG_FLOOR)),
    };
    state.splitter_tabs_y = tabs_y;
    state.splitter_max = settings_room(MIN_LOG_HEIGHT);
    // Windows too short for both areas show one of them: the settings while
    // idle, the task status and log while a task runs. Neither has a splitter.
    let settings_only = compact && !state.busy;
    let task_only = compact && state.busy;
    state.settings_hidden = task_only;
    let page_y = tabs_y + 44;
    let page_height = if settings_only { actions_top - 12 - page_y } else { tab_height - 44 };
    let splitter = if task_only { tabs_y - 12 } else { tabs_y + tab_height };
    let section_width = text_width("operation_section") + 8;
    let log_buttons_y = splitter + if log_bar.wrap { 110 } else { 76 };
    let log_top = log_buttons_y + 34;
    // Short task views hide the log first, then any toolbar row that would
    // reach the action buttons.
    let task_shown = !settings_only;
    let log_shown = task_shown && actions_top - 12 - log_top >= MIN_LOG_HEIGHT;
    let log_filters_shown = task_shown && splitter + 104 <= actions_top - 6;
    let log_buttons_shown = task_shown && log_buttons_y + 28 <= actions_top - 6;
    state.set_splitter_position(if compact { i32::MIN / 4 } else { scale(splitter) });
    let caption_width = 40;
    let caption_gap = 2;
    let close_x = width - margin - caption_width;
    let maximize_x = close_x - caption_gap - caption_width;
    let minimize_x = maximize_x - caption_gap - caption_width;
    let theme_x = minimize_x - caption_gap - caption_width;
    let language_x = theme_x - 8 - language_width;
    // Main-window controls (including the pages) move in one batch; the page
    // contents follow in a second batch once the page sizes are applied.
    begin_placement_batch();
    unsafe {
        place(
            state.controls.profile,
            profile_x,
            toolbar_y,
            if stacked_profile { width - margin - profile_x } else { header_x[0] - 12 - profile_x },
            26,
        );
        place(
            state.controls.language,
            language_x,
            (HEADER_HEIGHT - 28) / 2,
            language_width,
            120,
        );
        for (index, button) in [
            (minimize_x, state.caption_minimize),
            (maximize_x, state.caption_maximize),
            (close_x, state.caption_close),
            (theme_x, state.theme_button),
        ] {
            place(button, index, (HEADER_HEIGHT - 30) / 2, caption_width, 30);
            if !state.dragging_splitter {
                InvalidateRect(button, ptr::null(), 0);
            }
        }
        for (handle, key) in &state.buttons {
            match *key {
                "open" => {
                    place(*handle, header_x[0], action_toolbar_y, header_widths[0], 28);
                }
                "save" => {
                    place(*handle, header_x[1], action_toolbar_y, header_widths[1], 28);
                }
                "save_as" => {
                    place(*handle, header_x[2], action_toolbar_y, header_widths[2], 28);
                }
                "copy" | "export" => {
                    let index = usize::from(*key == "export");
                    place(*handle, margin + log_bar.button_x[index], log_buttons_y, log_bar.button_width[index], 28);
                    ShowWindow(*handle, if log_buttons_shown { SW_SHOW } else { SW_HIDE });
                }
                _ => {}
            }
        }
        for (handle, key) in &state.labels {
            match *key {
                "profile" => {
                    place(*handle, margin, toolbar_y + 4, profile_label_width, 20);
                }
                "language" => {
                    ShowWindow(*handle, SW_HIDE);
                }
                "operation_section" => {
                    place(*handle, margin, splitter + 12, section_width, 18);
                    ShowWindow(*handle, if task_shown { SW_SHOW } else { SW_HIDE });
                }
                "log_section" => {
                    place(*handle, margin, splitter + 78, log_bar.label_width, 22);
                    ShowWindow(*handle, if log_filters_shown { SW_SHOW } else { SW_HIDE });
                }
                _ => {}
            }
        }
        let advanced_width =
            (unscale(measured_text_width(hwnd, &get_text(state.controls.advanced), ui_font())) + 33).max(120);
        place(
            state.controls.advanced,
            width - margin - advanced_width,
            tabs_y + 8,
            advanced_width,
            22,
        );
        let settings_visible = if task_only { SW_HIDE } else { SW_SHOW };
        ShowWindow(state.controls.advanced, settings_visible);
        ShowWindow(state.controls.profile_origin, SW_HIDE);
        // The active tab uses the semibold font; size every tab for it.
        let mut tab_x = margin;
        for tab in &state.tabs {
            let tab_width =
                (unscale(measured_text_width(hwnd, &get_text(*tab), semibold_font())) + 28).max(132);
            place(*tab, tab_x, tabs_y, tab_width, 34);
            ShowWindow(*tab, settings_visible);
            tab_x += tab_width + 6;
        }
        for (index, page) in state.pages.iter().enumerate() {
            if settings_only {
                place_px(*page, Rect {
                    left: scale(margin),
                    top: scale(page_y),
                    right: scale(width - margin - 18),
                    bottom: (rect.bottom - scale(60)).max(scale(page_y) + 1),
                });
            } else {
                place(
                    *page,
                    margin,
                    page_y,
                    width - margin * 2 - 18,
                    page_height,
                );
            }
            let shown = !task_only && index == state.tab_index();
            ShowWindow(*page, if shown { SW_SHOW } else { SW_HIDE });
        }
        place_px(state.page_scrollbar, Rect {
            left: scale(width - margin - 12),
            top: scale(page_y + 2),
            right: scale(width - margin - 2),
            bottom: if settings_only {
                rect.bottom - scale(62)
            } else {
                scale(page_y + 2) + scale((page_height - 4).max(20))
            }
            .max(scale(page_y + 2) + 1),
        });
        ShowWindow(state.page_scrollbar, settings_visible);
        // Status sentences get the whole row; the activity hint follows the
        // section title. Both end in "..." when a message is still longer.
        place(state.controls.status, margin, splitter + 30, width - margin * 2, 24);
        ShowWindow(state.controls.status, if task_shown { SW_SHOW } else { SW_HIDE });
        let activity_x = margin + section_width + 12;
        let activity_width = width - margin - activity_x;
        if task_shown && activity_width >= 120 {
            place(state.controls.activity, activity_x, splitter + 12, activity_width, 18);
            ShowWindow(state.controls.activity, SW_SHOW);
        } else {
            ShowWindow(state.controls.activity, SW_HIDE);
        }
        place(
            state.controls.progress,
            margin,
            splitter + 62,
            width - margin * 2,
            7,
        );
        ShowWindow(state.controls.progress, if task_shown { SW_SHOW } else { SW_HIDE });
        place(
            state.controls.log_view,
            margin + log_bar.combo_x,
            splitter + 76,
            LOG_COMBO_WIDTH,
            120,
        );
        for (index, check) in [state.controls.only_issues, state.controls.live].into_iter().enumerate() {
            place(check, margin + log_bar.check_x[index], splitter + 78, log_bar.check_width[index], 22);
        }
        for control in [state.controls.log_view, state.controls.only_issues, state.controls.live] {
            ShowWindow(control, if log_filters_shown { SW_SHOW } else { SW_HIDE });
        }
        // Rows tied to the bottom edge are placed from the client's pixel
        // bottom: rounding them in DIPs made their gap to a dragged edge
        // wobble by a pixel at 125%/150%.
        let from_bottom = |offset: i32| rect.bottom - scale(offset);
        let log_bottom = from_bottom(60).max(scale(log_top) + 1);
        let log_target = Rect { left: scale(margin), top: scale(log_top), right: scale(width - margin - 18), bottom: log_bottom };
        let log_was_shown = state.log_box.is_some();
        state.log_box = log_shown.then_some(log_target);
        place_log(state, log_target, log_shown && !log_was_shown);
        place_px(
            state.log_scrollbar,
            Rect {
                left: scale(width - margin - 12),
                top: scale(log_top + 2),
                right: scale(width - margin - 2),
                bottom: (log_bottom - scale(2)).max(scale(log_top + 2) + 1),
            },
        );
        let log_visible = if log_shown { SW_SHOW } else { SW_HIDE };
        ShowWindow(state.log_frame, log_visible);
        ShowWindow(state.controls.log, log_visible);
        ShowWindow(state.log_scrollbar, log_visible);
        let actions_px = from_bottom(48);
        let action = |x: i32, width: i32, height: i32, top: i32| Rect {
            left: scale(x),
            top,
            right: scale(x) + scale(width),
            bottom: top + scale(height),
        };
        place_px(state.controls.prepare, action(margin, 1, 1, actions_px));
        place_px(state.controls.build, action(margin, 140, 34, actions_px));
        place_px(state.controls.verify, action(margin + 148, 1, 1, actions_px));
        place_px(state.controls.cancel, action(margin + 148, 120, 34, actions_px));
        place_px(state.controls.open_output, action(margin + 276, 120, 34, actions_px));
        let (statistics_x, statistics_width) = if width >= 900 {
            (width - margin - 360, 360)
        } else {
            (margin + 404, (width - margin * 2 - 404).max(120))
        };
        place_px(state.controls.statistics, action(statistics_x, statistics_width, 24, from_bottom(42)));
        end_placement_batch();
        state.log_first_visible_line = first_visible_log_line(state.controls.log);
        for page in &state.pages {
            scroll_page_with_bar(*page, 0, false, state.page_scrollbar);
        }
        begin_placement_batch();
        for page in &state.pages {
            resize_page_fields(*page);
        }
        layout_page_contents(state);
        end_placement_batch();
        // The scroll range follows the content just laid out.
        for page in &state.pages {
            scroll_page_with_bar(*page, 0, false, state.page_scrollbar);
        }
        // Laying out the pages can change the current page's content height
        // (narrow layout, shown fields); fit the settings to it once more.
        static REFITTING: AtomicBool = AtomicBool::new(false);
        let fitted = unscale(page_content_height(state.pages[state.tab_index().min(state.pages.len() - 1)])) + 44;
        if state.splitter_height.is_none()
            && Some(fitted) != state.default_tab_height
            && !REFITTING.swap(true, Ordering::Relaxed)
        {
            layout(hwnd, state);
            REFITTING.store(false, Ordering::Relaxed);
            return;
        }
        sync_combo_overlays();
        DEFER_CONTROL_PAINT.store(false, Ordering::Relaxed);
        // Splitter motion does not change the header or profile toolbar.
        // Keep their native controls out of the repeated paint cycle.
        let splitter_dirty = Rect {
            left: 0,
            top: scale(page_y),
            right: rect.right,
            bottom: rect.bottom,
        };
        RedrawWindow(
            hwnd,
            if state.dragging_splitter { &splitter_dirty } else { ptr::null() },
            ptr::null_mut(),
            if state.dragging_splitter {
                RDW_INVALIDATE
            } else {
                RDW_INVALIDATE | RDW_ALLCHILDREN | 0x0100 // RDW_UPDATENOW: finish the composite before the next sizing event.
            },
        );
    }
}

pub(crate) fn label_handle(state: &UiState, key: &str) -> Hwnd {
    state
        .labels
        .iter()
        .find(|(_, label_key)| *label_key == key)
        .map(|(handle, _)| *handle)
        .unwrap_or(ptr::null_mut())
}

pub(crate) fn move_child(page: Hwnd, id: usize, x: i32, y: i32, width: i32, height: i32) {
    let control = GetDlgItem(page, id as i32);
    if !control.is_null() {
        place(control, x, y, width, height);
    }
}

pub(crate) fn move_label(state: &UiState, key: &str, x: i32, y: i32, width: i32, height: i32) {
    let handle = label_handle(state, key);
    if !handle.is_null() {
        place(handle, x, y, width, height);
    }
}

pub(crate) fn layout_page_contents(state: &UiState) {
    for (index, page) in state.pages.iter().enumerate() {
        let mut rect = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        GetClientRect(*page, &mut rect);
        let page_width = unscale(rect.right).max(1);
        let scroll = GetWindowLongPtrW(*page, GWLP_USERDATA) as i32;
        let sy = unscale(scroll);
        let narrow = page_is_narrow(page_width);
        match index {
            0 => {
                if narrow {
                    let field_width = page_width - 28;
                     {
                        move_label(state, "title", 14, 110 - sy, field_width, 20);
                        move_child(*page, ID_TITLE, 14, 130 - sy, field_width, 24);
                        move_label(state, "capacity", 14, 154 - sy, 220, 20);
                        move_child(*page, ID_CAPACITY, 14, 174 - sy, 220, 120);
                        let custom_x = 250;
                        let custom_width = (page_width - custom_x - 14).max(160);
                        place(
                            state.custom_bytes_label,
                            custom_x,
                            154 - sy,
                            custom_width,
                            20,
                        );
                        move_child(
                            *page,
                            ID_CUSTOM_BYTES,
                            custom_x,
                            174 - sy,
                            custom_width,
                            25,
                        );
                        let hint = get_text(label_handle(state, "advanced_hint"));
                        move_label(state, "advanced_hint", 14, 208 - sy, field_width, wrapped_text_height(&hint, ui_font(), field_width));
                        move_label(state, "work_dir", 14, 214 - sy, field_width, 22);
                        move_child(
                            *page,
                            ID_WORK_DIR,
                            14,
                            238 - sy,
                            (field_width - 100).max(120),
                            25,
                        );
                        move_child(
                            *page,
                            ID_BROWSE_WORK,
                            page_width - 14 - 90,
                            238 - sy,
                            90,
                            24,
                        );
                        move_label(state, "iso_prefix", 14, 272 - sy, field_width, 20);
                        move_child(*page, ID_ISO_PREFIX, 14, 292 - sy, field_width, 25);
                        move_label(state, "group_limit", 14, 326 - sy, field_width, 20);
                        move_child(*page, ID_GROUP_LIMIT, 14, 346 - sy, field_width, 25);
                        place(state.controls.keep_files, 14, 380 - sy, 230, 28);
                    }
                } else {
                     {
                        move_label(state, "title", 14, 110 - sy, 470, 20);
                        move_child(*page, ID_TITLE, 14, 130 - sy, 470, 24);
                        move_label(state, "capacity", 520, 110 - sy, 220, 20);
                        move_child(*page, ID_CAPACITY, 520, 130 - sy, 220, 120);
                        place(state.custom_bytes_label, 760, 110 - sy, page_width - 774, 20);
                        move_child(*page, ID_CUSTOM_BYTES, 760, 130 - sy, page_width - 774, 25);
                        let hint = get_text(label_handle(state, "advanced_hint"));
                        move_label(state, "advanced_hint", 14, 170 - sy, 780, wrapped_text_height(&hint, ui_font(), 780));
                        move_label(state, "work_dir", 14, 150 - sy, 520, 22);
                        move_label(state, "iso_prefix", 14, 198 - sy, 460, 20);
                        move_child(*page, ID_ISO_PREFIX, 14, 218 - sy, 460, 25);
                        move_label(state, "group_limit", 520, 198 - sy, 220, 20);
                        move_child(*page, ID_GROUP_LIMIT, 520, 218 - sy, 220, 25);
                        place(state.controls.keep_files, 760, 198 - sy, 230, 28);
                    }
                }
            }
            1 => {
                if narrow {
                    let field_width = page_width - 28;
                     {
                        // Rates and depths are short; the mode list holds the
                        // longest texts and takes the remaining width.
                        let mode_width = (page_width - 28 - 32 - 260).clamp(200, 300);
                        let rate_x = 14 + mode_width + 16;
                        let bits_x = rate_x + 130 + 16;
                        move_label(state, "audio_mode", 14, 18 - sy, mode_width, 20);
                        move_child(*page, ID_MODE, 14, 38 - sy, mode_width, 120);
                        move_label(state, "sample_rate", rate_x, 18 - sy, 130, 20);
                        move_child(*page, ID_RATE, rate_x, 38 - sy, 130, 120);
                        move_label(state, "bits", bits_x, 18 - sy, 130, 20);
                        move_child(*page, ID_BITS, bits_x, 38 - sy, 130, 120);
                        // The note follows the lists; the MLP path follows the note.
                        let note = get_text(label_handle(state, "audio_note"));
                        let note_height = wrapped_text_height(&note, ui_font(), field_width);
                        move_label(state, "audio_note", 14, 80 - sy, field_width, note_height);
                        let import_y = 80 + note_height + 12;
                        move_label(state, "import_folder", 14, import_y - sy, field_width, 20);
                        move_child(
                            *page,
                            ID_IMPORT_FOLDER,
                            14,
                            import_y + 20 - sy,
                            (field_width - 100).max(120),
                            25,
                        );
                        move_child(
                            *page,
                            ID_BROWSE_IMPORT,
                            page_width - 14 - 90,
                            import_y + 20 - sy,
                            90,
                            24,
                        );

                    }
                } else {
                     {
                        move_label(state, "audio_mode", 14, 18 - sy, 300, 20);
                        move_child(*page, ID_MODE, 14, 38 - sy, 300, 120);
                        move_label(state, "sample_rate", 340, 18 - sy, 210, 20);
                        move_child(*page, ID_RATE, 340, 38 - sy, 210, 120);
                        move_label(state, "bits", 575, 18 - sy, 160, 20);
                        move_child(*page, ID_BITS, 575, 38 - sy, 160, 120);
                        let note_width = 900.min(page_width - 28);
                        let note = get_text(label_handle(state, "audio_note"));
                        let note_height = wrapped_text_height(&note, ui_font(), note_width);
                        move_label(state, "audio_note", 14, 80 - sy, note_width, note_height);
                        let import_y = 80 + note_height + 12;
                        move_label(state, "import_folder", 14, import_y - sy, 520, 20);
                        let edit_width = browse_edit_width(page_width);
                        move_child(*page, ID_IMPORT_FOLDER, 14, import_y + 20 - sy, edit_width, 24);
                        move_child(*page, ID_BROWSE_IMPORT, 14 + edit_width + 10, import_y + 20 - sy, 90, 24);
                    }
                }
            }
            2 => {
                let switch_width = |hwnd: Hwnd| {
                    (unscale(measured_text_width(*page, &get_text(hwnd), ui_font())) + 33).max(120)
                };
                let menu_width = switch_width(state.controls.menu);
                let stills_x = (14 + menu_width + 24).max(300);
                let stills_width = switch_width(state.controls.stills).min(page_width - 14 - stills_x);
                place(state.controls.menu, 14, 18 - sy, menu_width, 28);
                place(state.controls.stills, stills_x, 18 - sy, stills_width, 28);
                if narrow {
                    let column = (page_width - 28 - 16) / 2;
                     {
                        move_label(state, "tracks", 14, 66 - sy, column, 20);
                        move_child(*page, ID_TRACKS, 14, 86 - sy, column, 25);
                        move_label(state, "cover", 14 + column + 16, 66 - sy, column, 20);
                        move_child(
                            *page,
                            ID_COVER,
                            14 + column + 16,
                            86 - sy,
                            column,
                            25,
                        );
                    }
                    let font_column = (page_width - 28 - 32) / 3;
                    let font_label_height = ["font_sc", "font_jp", "font_kr"]
                        .iter()
                        .map(|key| {
                            let text = get_text(label_handle(state, key));
                            if unscale(measured_text_width(*page, &text, ui_font())) > font_column {
                                40
                            } else {
                                20
                            }
                        })
                        .max()
                        .unwrap_or(20);
                    for (index, (label_key, field_id, browse_id)) in [
                        ("font_sc", ID_FONT_SC, 1007usize),
                        ("font_jp", ID_FONT_JP, 1008usize),
                        ("font_kr", ID_FONT_KR, 1009usize),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let x = 14 + index as i32 * (font_column + 16);
                         {
                            move_label(state, label_key, x, 120 - sy, font_column, font_label_height);
                            move_child(*page, field_id, x, 120 + font_label_height - sy, font_column, 25);
                            move_child(*page, browse_id, x, 154 + font_label_height - sy, font_column, 28);
                        }
                    }
                } else {
                     {
                        move_label(state, "tracks", 14, 66 - sy, 220, 20);
                        move_child(*page, ID_TRACKS, 14, 86 - sy, 220, 25);
                        move_label(state, "cover", 260, 66 - sy, 220, 20);
                        move_child(*page, ID_COVER, 260, 86 - sy, 220, 25);
                    }
                    for (index, (label_key, field_id, browse_id)) in [
                        ("font_sc", ID_FONT_SC, 1007usize),
                        ("font_jp", ID_FONT_JP, 1008usize),
                        ("font_kr", ID_FONT_KR, 1009usize),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let x = 14 + index as i32 * 326;
                         {
                            move_label(state, label_key, x, 120 - sy, 300, 20);
                            move_child(*page, field_id, x, 140 - sy, 300, 25);
                            move_child(*page, browse_id, x, 174 - sy, 150, 28);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
