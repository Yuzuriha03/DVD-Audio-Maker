//! Settings pages: the page window procedure, scrolling and the browse rows.

use super::*;

/// # Safety
/// Window procedure called by Windows. `hwnd` is a valid window handle, and for messages
/// that pass pointers in `lparam` (such as WM_DRAWITEM or WM_NCCALCSIZE) it points to the
/// structure the system defines for that message, valid for the duration of the call.
pub(crate) unsafe extern "system" fn page_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    match message {
        WM_SIZE => {
            if DEFER_CONTROL_PAINT.load(Ordering::Relaxed) {
                return 0;
            }
            scroll_page(hwnd, 0, false);
            resize_page_fields(hwnd);
            unsafe {
                RedrawWindow(
                    hwnd,
                    ptr::null(),
                    ptr::null_mut(),
                    if DEFER_CONTROL_PAINT.load(Ordering::Relaxed) {
                        RDW_INVALIDATE
                    } else {
                        RDW_INVALIDATE | RDW_ALLCHILDREN
                    },
                );
            }
            0
        }
        0x0115 => {
            let old = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as i32;
            let next = match wparam & 0xffff {
                0 => old - scale(24),
                1 => old + scale(24),
                2 => old - scale(120),
                3 => old + scale(120),
                4 | 5 => (wparam >> 16) as i32,
                6 => 0,
                7 => i32::MAX,
                _ => old,
            };
            scroll_page(hwnd, next, true);
            0
        }
        0x020a => {
            let old = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as i32;
            let delta = (wparam >> 16) as i16 as i32;
            let pixels = if let Some(mut state) = unsafe { state(GetParent(hwnd)) } {
                wheel_pixels(
                    delta,
                    scale(40),
                    state.wheel_remainders.entry(hwnd as usize).or_default(),
                )
            } else {
                wheel_pixels(delta, scale(40), &mut 0)
            };
            scroll_page(hwnd, old - pixels, true);
            0
        }
        WM_COMMAND | WM_NOTIFY | WM_DRAWITEM | WM_MEASUREITEM => {
            let parent = GetParent(hwnd);
            if parent.is_null() {
                0
            } else {
                unsafe { SendMessageW(parent, message, wparam, lparam) }
            }
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            unsafe { paint_page(hwnd) };
            0
        }
        WM_CTLCOLORSTATIC => {
            let dc = wparam as Handle;
            let control = lparam as Hwnd;
            let parent = GetParent(hwnd);
            let muted = paint_fields(parent).is_some_and(|paint| paint.muted.contains(&control));
            SetBkMode(dc, 1);
            if (IsWindowEnabled(hwnd) == 0 || IsWindowEnabled(control) == 0)
                && window_class(control) == "Edit"
            {
                SetBkColor(dc, c_disabled_fill());
                SetTextColor(dc, c_disabled_text());
                brush(c_disabled_fill()) as Lresult
            } else {
                SetBkColor(dc, c_panel());
                SetTextColor(dc, if muted { c_muted() } else { c_ink() });
                panel_brush() as Lresult
            }
        },
        WM_CTLCOLOREDIT =>  {
            let dc = wparam as Handle;
            SetBkMode(dc, 1);
            if IsWindowEnabled(hwnd) == 0 {
                SetBkColor(dc, c_disabled_fill());
                SetTextColor(dc, c_disabled_text());
                brush(c_disabled_fill()) as Lresult
            } else {
                SetBkColor(dc, c_card());
                SetTextColor(dc, c_ink());
                card_brush() as Lresult
            }
        },
        WM_CTLCOLORLISTBOX =>  {
            SetBkMode(wparam as Handle, 1);
            SetBkColor(wparam as Handle, c_card());
            SetTextColor(wparam as Handle, c_ink());
            card_brush() as Lresult
        },
        WM_CTLCOLORBTN =>  {
            SetBkMode(wparam as Handle, 1);
            SetBkColor(wparam as Handle, c_panel());
            panel_brush() as Lresult
        },
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

pub(crate) fn wheel_pixels(delta: i32, step: i32, remainder: &mut i32) -> i32 {
    // Preserve sub-pixel input from high-resolution wheels across messages.
    let total = delta * step + *remainder;
    *remainder = total % 120;
    total / 120
}

#[cfg(test)]
mod wheel_tests {
    use super::{ui_test_guard, wheel_pixels};

    #[test]
    fn partial_notches_move_immediately() {
        let _ui_test_lock = ui_test_guard();
        let mut remainder = 0;
        assert_eq!(wheel_pixels(30, 40, &mut remainder), 10);
        assert_eq!(wheel_pixels(-30, 40, &mut remainder), -10);
    }

    #[test]
    fn small_deltas_preserve_full_notch_distance_at_each_scale() {
        let _ui_test_lock = ui_test_guard();
        for step in [40, 50, 60, 80] {
            for direction in [-1, 1] {
                let mut remainder = 0;
                let distance: i32 = (0..120)
                    .map(|_| wheel_pixels(direction, step, &mut remainder))
                    .sum();
                assert_eq!(distance, direction * step);
                assert_eq!(remainder, 0);
                assert_eq!(wheel_pixels(direction * 240, step, &mut remainder), direction * step * 2);
            }
        }
    }

    #[test]
    fn fractional_input_cancels_on_direction_reversal() {
        let _ui_test_lock = ui_test_guard();
        let mut remainder = 0;
        assert_eq!(wheel_pixels(1, 40, &mut remainder), 0);
        assert_eq!(wheel_pixels(-1, 40, &mut remainder), 0);
        assert_eq!(remainder, 0);
    }
}

/// # Safety
/// Window procedure called by Windows. `hwnd` is a valid window handle, and for messages
/// that pass pointers in `lparam` (such as WM_DRAWITEM or WM_NCCALCSIZE) it points to the
/// structure the system defines for that message, valid for the duration of the call.
pub(crate) unsafe extern "system" fn scroll_page_child(hwnd: Hwnd, parameter: Lparam) -> i32 {
    let (page, delta) = unsafe { &*(parameter as *const (Hwnd, i32)) };
    if GetParent(hwnd) != *page {
        return 1;
    }
    let rect = window_rect(hwnd);
    let mut point = Point { x: rect.left, y: rect.top };
    unsafe {
        ScreenToClient(*page, &mut point);
        SetWindowPos(hwnd, ptr::null_mut(), point.x, point.y + delta, 0, 0, 0x011D);
    }
    1
}

/// Height of a page's content in pixels: the lowest visible control (at
/// scroll position 0) plus a bottom margin.
pub(crate) fn page_content_height(page: Hwnd) -> i32 {
    unsafe extern "system" fn lowest(hwnd: Hwnd, parameter: Lparam) -> i32 {
        let (page, bottom) = unsafe { &mut *(parameter as *mut (Hwnd, i32)) };
        if GetParent(hwnd) == *page && (GetWindowLongPtrW(hwnd, -16) as u32) & WS_VISIBLE != 0 {
            let mut rect: Rect = Rect::default();
            GetWindowRect(hwnd, &mut rect);
            let mut corner = Point { x: rect.left, y: rect.bottom };
            ScreenToClient(*page, &mut corner);
            *bottom = (*bottom).max(corner.y);
        }
        1
    }
    let mut found = (page, 0);
    unsafe { EnumChildWindows(page, lowest, &mut found as *mut _ as Lparam) };
    let scroll = GetWindowLongPtrW(page, GWLP_USERDATA) as i32;
    if found.1 <= 0 { scale(100) } else { found.1 + scroll + scale(14) }
}

pub(crate) fn page_is_narrow(width: i32) -> bool {
    width < 1000
}

/// Scrolls a settings page; refreshes the main window's page scrollbar.
pub(crate) fn scroll_page(hwnd: Hwnd, requested: i32, explicit: bool) {
    let main = GetParent(hwnd);
    let bar = if main.is_null() {
        ptr::null_mut()
    } else {
        // SAFETY: `main` is the window that owns the settings pages; the borrow ends
        // with this statement.
        paint_fields(main).map_or(ptr::null_mut(), |paint| paint.page_scrollbar)
    };
    scroll_page_with_bar(hwnd, requested, explicit, bar);
}

/// `scroll_page` for callers that already hold the UiState: `bar` is its page
/// scrollbar, invalidated after the scroll (null if there is none).
pub(crate) fn scroll_page_with_bar(hwnd: Hwnd, requested: i32, explicit: bool, bar: Hwnd) {
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
     {
        GetClientRect(hwnd, &mut rect);
    }
    let old = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as i32;
    let height = (rect.bottom - rect.top).max(1);
    let content = page_content_height(hwnd);
    let pos = if explicit { requested } else { old }.clamp(0, (content - height).max(0));
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, pos as isize);
        if pos != old {
            let delta = old - pos;
            if DEFER_CONTROL_PAINT.load(Ordering::Relaxed) || IsWindowVisible(hwnd) == 0 {
                // Layout repaints everything afterwards; just move the children.
                let movement = (hwnd, delta);
                EnumChildWindows(hwnd, scroll_page_child, &movement as *const _ as Lparam);
            } else {
                // Shift the shown pixels and every child in one operation, then
                // paint only the uncovered strip. Moving and repainting each
                // control separately showed mixed old/new frames (ghosting).
                const SW_SCROLLCHILDREN: u32 = 0x0001;
                const SW_INVALIDATE: u32 = 0x0002;
                ScrollWindowEx(
                    hwnd,
                    0,
                    delta,
                    ptr::null(),
                    ptr::null(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    SW_SCROLLCHILDREN | SW_INVALIDATE,
                );
                // The 1-pixel page border was shifted with the content.
                let band = delta.abs() + 2;
                for strip in [
                    Rect { left: 0, top: 0, right: rect.right, bottom: band },
                    Rect { left: 0, top: rect.bottom - band, right: rect.right, bottom: rect.bottom },
                ] {
                    RedrawWindow(hwnd, &strip, ptr::null_mut(), RDW_INVALIDATE | RDW_ALLCHILDREN);
                }
                RedrawWindow(hwnd, ptr::null(), ptr::null_mut(), RDW_ALLCHILDREN | 0x0100); // RDW_UPDATENOW
            }
        }
    }
    if !bar.is_null() {
        unsafe {
            InvalidateRect(bar, ptr::null(), 0);
        }
    }
    if !DEFER_CONTROL_PAINT.load(Ordering::Relaxed) {
        sync_combo_overlays();
    }
}

pub(crate) fn browse_edit_width(page_width: i32) -> i32 {
    // Reserve both page margins as well as the button and its gap.
    (page_width - 14 - 14 - 90 - 10).max(120)
}

#[cfg(test)]
mod browse_layout_tests {
    use super::{browse_edit_width, ui_test_guard};

    #[test]
    fn browse_rows_preserve_right_margin_at_half_and_full_width() {
        let _ui_test_lock = ui_test_guard();
        for page_width in [420, 640, 780, 800, 1120, 1600] {
            let edit_right = 14 + browse_edit_width(page_width);
            let button_left = edit_right + 10;
            assert_eq!(page_width - (button_left + 90), 14);
        }
    }
}

pub(crate) fn resize_page_fields(page: Hwnd) {
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
     {
        GetClientRect(page, &mut rect);
    }
    let width = unscale(rect.right).max(1);
    let scroll = GetWindowLongPtrW(page, GWLP_USERDATA) as i32;
    // Browse rows share the same geometry. Keeping the edit and its button
    // inside the page client area prevents a clipped right border after a
    // window resize or a DPI/layout change.
    let edit_width = browse_edit_width(width);
    for (edit_id, browse_id, y) in [
        (ID_SOURCE, ID_BROWSE_SOURCE, 38),
        (ID_FINAL, ID_BROWSE_FINAL, 86),
        (ID_WORK_DIR, ID_BROWSE_WORK, 174),
    ] {
        let edit = GetDlgItem(page, edit_id as i32);
        let browse = GetDlgItem(page, browse_id as i32);
        let top = y - unscale(scroll);
        if !edit.is_null() {
            place(edit, 14, top, edit_width, 24);
        }
        if !browse.is_null() {
            place(browse, 14 + edit_width + 10, top, 90, 24);
        }
    }
}

pub(crate) fn select_tab(state: &mut UiState, index: usize) {
    let index = index.min(state.pages.len().saturating_sub(1));
    state.set_tab_index(index);
    show_page(state, index as i32);
    for tab in &state.tabs {
        unsafe {
            InvalidateRect(*tab, ptr::null(), 0);
        }
    }
}

pub(crate) fn show_page(state: &UiState, index: i32) {
    for (i, page) in state.pages.iter().enumerate() {
        unsafe {
            ShowWindow(
                *page,
                if i == index.max(0) as usize && !state.settings_hidden {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
            if i == index.max(0) as usize {
                SetWindowPos(*page, ptr::null_mut(), 0, 0, 0, 0, 0x0013);
            }
        }
    }
    unsafe {
        InvalidateRect(state.page_scrollbar, ptr::null(), 0);
    }
    sync_combo_overlays();
}
