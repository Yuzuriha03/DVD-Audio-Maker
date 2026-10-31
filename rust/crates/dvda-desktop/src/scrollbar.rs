//! The custom 10-DIP scrollbars of the settings page and the log.

use super::*;

#[derive(Clone, Copy)]
pub(crate) struct BarMetrics {
    value: i32,
    pub(crate) max: i32,
    page: i32,
}

#[derive(Clone, Copy)]
pub(crate) struct BarGeometry {
    track_height: i32,
    thumb_top: i32,
    thumb_height: i32,
}

pub(crate) fn bar_geometry(client_height: i32, metrics: &BarMetrics) -> BarGeometry {
    let pad = scale(2).max(1);
    let track_top = pad;
    let track_height = (client_height - pad * 2).max(1);
    if metrics.max <= 0 || metrics.page <= 0 {
        return BarGeometry {
            track_height,
            thumb_top: track_top,
            thumb_height: track_height,
        };
    }
    let total = (metrics.page + metrics.max) as f64;
    let mut thumb_height = (track_height as f64 * metrics.page as f64 / total) as i32;
    thumb_height = thumb_height.clamp(scale(18).max(8).min(track_height), track_height);
    let travel = (track_height - thumb_height).max(0);
    let ratio = (metrics.value as f64 / metrics.max as f64).clamp(0.0, 1.0);
    BarGeometry {
        track_height,
        thumb_top: track_top + (travel as f64 * ratio) as i32,
        thumb_height,
    }
}

pub(crate) fn page_bar_metrics(state: &PaintFields) -> BarMetrics {
    let page = match state.pages.get(state.tab_index.get()) {
        Some(page) => *page,
        None => {
            return BarMetrics {
                value: 0,
                max: 0,
                page: 1,
            };
        }
    };
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    {
        GetClientRect(page, &mut rect);
    }
    let page_height = (rect.bottom - rect.top).max(1);
    let content = page_content_height(page);
    let max = (content - page_height).max(0);
    let value = (GetWindowLongPtrW(page, GWLP_USERDATA) as i32).clamp(0, max);
    BarMetrics {
        value,
        max,
        page: page_height,
    }
}

pub(crate) fn log_bar_metrics(log: Hwnd) -> BarMetrics {
    let total = (win::send_int(log, EM_GETLINECOUNT, 0, 0) as i32).max(1);
    let first = (win::send_int(log, EM_GETFIRSTVISIBLELINE, 0, 0) as i32).max(0);
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut frame: Rect = Rect::default();
    {
        GetClientRect(log, &mut rect);
        GetClientRect(GetParent(log), &mut frame);
    }
    // While live sizing freezes the EDIT, only the frame's part is visible.
    let visible_height = if frame.bottom > 0 {
        rect.bottom.min(frame.bottom)
    } else {
        rect.bottom
    };
    let height = (visible_height - rect.top).max(1);
    let line_height = scale(19).max(1);
    let visible = (height / line_height).max(1);
    BarMetrics {
        value: first,
        max: (total - visible).max(0),
        page: visible,
    }
}

/// # Safety
/// `hwnd` must be a scrollbar created by this application and not yet installed, and
/// `main` must be the main window. The visual state it allocates is owned by `hwnd`
/// and freed when it is destroyed.
pub(crate) unsafe fn install_scrollbar(hwnd: Hwnd, main: Hwnd) {
    if hwnd.is_null() {
        return;
    }
    let visual = Box::new(ScrollbarVisual {
        main,
        hover: false,
        dragging: false,
        drag_start_cursor: 0,
        drag_start_value: 0,
        last_value: -1,
        last_max: -1,
        last_page: -1,
    });
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(visual) as isize);
    }
}

pub(crate) fn scrollbar_metrics(hwnd: Hwnd, state: &PaintFields) -> BarMetrics {
    if hwnd == state.page_scrollbar {
        page_bar_metrics(state)
    } else {
        log_bar_metrics(state.log)
    }
}

/// # Safety
/// `main` must be the main window with a live `UiState`, and `hwnd` must be one of its
/// scrollbars.
pub(crate) unsafe fn apply_scrollbar_value(main: Hwnd, hwnd: Hwnd, value: i32) {
    let Some(state) = paint_fields(main) else {
        return;
    };
    if hwnd == state.page_scrollbar {
        if let Some(page) = state.pages.get(state.tab_index.get()).copied() {
            let metrics = page_bar_metrics(&state);
            scroll_page_with_bar(
                page,
                value.clamp(0, metrics.max),
                true,
                state.page_scrollbar,
            );
        }
    } else if hwnd == state.log_scrollbar {
        let log = state.log;
        if log.is_null() {
            return;
        }
        let metrics = log_bar_metrics(log);
        let target = value.clamp(0, metrics.max);
        if target != metrics.value {
            {
                win::send_int(log, EM_LINESCROLL, 0, (target - metrics.value) as isize);
            }
        }
    }
    unsafe {
        InvalidateRect(hwnd, ptr::null(), 0);
    }
}

/// # Safety
/// `hwnd` must be a scrollbar installed by `install_scrollbar`, and this must run inside
/// its WM_PAINT.
pub(crate) unsafe fn draw_scrollbar(hwnd: Hwnd) {
    // Composed off-screen: the track and the thumb reach the screen together.
    let mut paint: PaintStruct = unsafe { std::mem::zeroed() };
    let buffer = unsafe { begin_buffered_paint(hwnd, &mut paint, c_body()) };
    let dc = buffer.memory;
    let rect = buffer.rect;
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ScrollbarVisual;
    if !pointer.is_null() && rect.right > 0 && rect.bottom > 0 {
        let main = unsafe { (*pointer).main };
        if let Some(state) = paint_fields(main) {
            let metrics = scrollbar_metrics(hwnd, &state);
            let geometry = bar_geometry(rect.bottom, &metrics);
            if metrics.max > 0 {
                let inset = scale(2).max(1);
                let thumb = Rect {
                    left: inset,
                    top: geometry.thumb_top,
                    right: rect.right - inset,
                    bottom: geometry.thumb_top + geometry.thumb_height,
                };
                let hovered = unsafe { (*pointer).hover || (*pointer).dragging };
                let color = if hovered { c_muted() } else { c_border() };
                {
                    FillRect(dc, &thumb, brush(color));
                }
            }
        }
    }
    unsafe {
        finish_buffered_paint(hwnd, &paint, &buffer);
    }
}

/// # Safety
/// Window procedure called by Windows. `hwnd` is a valid window handle, and for messages
/// that pass pointers in `lparam` (such as WM_DRAWITEM or WM_NCCALCSIZE) it points to the
/// structure the system defines for that message, valid for the duration of the call.
pub(crate) unsafe extern "system" fn scrollbar_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ScrollbarVisual;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    match message {
        WM_PAINT => {
            unsafe { draw_scrollbar(hwnd) };
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEMOVE => {
            let mut rect = Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            {
                GetClientRect(hwnd, &mut rect);
            }
            let y = (lparam >> 16) as i16 as i32;
            let main = unsafe { (*pointer).main };
            let mut geometry = None;
            if let Some(state) = paint_fields(main) {
                let metrics = scrollbar_metrics(hwnd, &state);
                geometry = Some((metrics, bar_geometry(rect.bottom, &metrics)));
            }
            unsafe {
                if (*pointer).dragging {
                    if let Some((metrics, geometry)) = geometry {
                        let travel = (geometry.track_height - geometry.thumb_height).max(0);
                        if travel > 0 && metrics.max > 0 {
                            let delta = y - (*pointer).drag_start_cursor;
                            let value = (*pointer).drag_start_value
                                + (delta as f64 * metrics.max as f64 / travel as f64) as i32;
                            apply_scrollbar_value(main, hwnd, value);
                        }
                    }
                    return 0;
                }
                if !(*pointer).hover {
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
            0
        }
        WM_MOUSELEAVE => {
            unsafe {
                if (*pointer).hover {
                    (*pointer).hover = false;
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
            0
        }
        WM_LBUTTONDOWN => {
            let mut rect = Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            {
                GetClientRect(hwnd, &mut rect);
            }
            let y = (lparam >> 16) as i16 as i32;
            let main = unsafe { (*pointer).main };
            if let Some(state) = paint_fields(main) {
                let metrics = scrollbar_metrics(hwnd, &state);
                let geometry = bar_geometry(rect.bottom, &metrics);
                if y >= geometry.thumb_top && y < geometry.thumb_top + geometry.thumb_height {
                    unsafe {
                        (*pointer).dragging = true;
                        (*pointer).drag_start_cursor = y;
                        (*pointer).drag_start_value = metrics.value;
                        SetCapture(hwnd);
                    }
                } else {
                    let step = metrics.page.max(1);
                    let value = if y < geometry.thumb_top {
                        metrics.value - step
                    } else {
                        metrics.value + step
                    };
                    unsafe { apply_scrollbar_value(main, hwnd, value) };
                }
            }
            0
        }
        WM_LBUTTONUP => {
            unsafe {
                if (*pointer).dragging {
                    (*pointer).dragging = false;
                    ReleaseCapture();
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
            0
        }
        WM_NCDESTROY => {
            unsafe {
                drop(Box::from_raw(pointer));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// # Safety
/// `state` must be the live `UiState` of the main window, whose scrollbars and combo
/// overlays were installed by this application. Runs on the UI thread.
pub(crate) unsafe fn refresh_dynamic_indicators(state: &UiState) {
    for (hwnd, is_page) in [(state.page_scrollbar, true), (state.log_scrollbar, false)] {
        if hwnd.is_null() {
            continue;
        }
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ScrollbarVisual;
        if pointer.is_null() {
            continue;
        }
        let metrics = if is_page {
            page_bar_metrics(&state.paint)
        } else {
            log_bar_metrics(state.controls.log)
        };
        let visual = unsafe { &mut *pointer };
        if visual.last_value != metrics.value
            || visual.last_max != metrics.max
            || visual.last_page != metrics.page
        {
            visual.last_value = metrics.value;
            visual.last_max = metrics.max;
            visual.last_page = metrics.page;
            unsafe {
                InvalidateRect(hwnd, ptr::null(), 0);
            }
        }
    }
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
        // The overlay removes itself from the registry when it is destroyed, so a window
        // found here is still the arrow button that owns a `ComboArrowVisual`. Page
        // combos are also subclassed by comctl32, so its window procedure cannot be compared.
        if IsWindow(overlay) == 0 {
            continue;
        }
        let pointer = GetWindowLongPtrW(overlay, GWLP_USERDATA) as *mut ComboArrowVisual;
        if pointer.is_null() {
            continue;
        }
        let mut index = win::send_int(combo, CB_GETCURSEL, 0, 0) as i32;
        if index < 0 {
            index = -1;
        }
        let enabled = IsWindowEnabled(combo) != 0;
        let visual = unsafe { &mut *pointer };
        if visual.last_index != index || visual.last_enabled != enabled {
            visual.last_index = index;
            visual.last_enabled = enabled;
            unsafe {
                InvalidateRect(overlay, ptr::null(), 0);
            }
        }
    }
}
