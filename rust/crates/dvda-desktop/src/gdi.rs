//! DPI scaling, fonts, brushes, the header icon and text measurement.

use super::*;

/// Window bounds in screen coordinates.
pub(crate) fn window_rect(hwnd: Hwnd) -> Rect {
    let mut rect: Rect = Rect::default();
    GetWindowRect(hwnd, &mut rect);
    rect
}

/// Client bounds: left and top are 0 for a client rectangle.
pub(crate) fn client_rect(hwnd: Hwnd) -> Rect {
    let mut rect: Rect = Rect::default();
    GetClientRect(hwnd, &mut rect);
    rect
}

pub(crate) fn dpi() -> i32 {
    UI_DPI.load(Ordering::Relaxed).max(48) as i32
}
pub(crate) fn scale(value: i32) -> i32 {
    ((value as i64 * dpi() as i64 + 48) / 96) as i32
}
pub(crate) fn unscale(value: i32) -> i32 {
    if value <= 0 {
        value
    } else {
        ((value as i64 * 96 + dpi() as i64 / 2) / dpi() as i64) as i32
    }
}
pub(crate) fn header_height() -> i32 {
    scale(HEADER_HEIGHT)
}

pub(crate) fn load_font(height: i32, weight: i32, face: &str) -> Handle {
    unsafe {
        let font = CreateFontW(
            height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            1,
            0,
            0,
            5,
            0,
            wide(face).as_ptr(),
        );
        if font.is_null() {
            GetStockObject(17)
        } else {
            font
        }
    }
}

pub(crate) fn font(height: i32, weight: i32, face: u8) -> Handle {
    let height = scale(height);
    let key = (height, weight, face);
    let cache = FONT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(value) = cache.get(&key) {
        return *value as Handle;
    }
    let name = match face {
        1 => "Segoe UI Semibold",
        _ => "Segoe UI",
    };
    let handle = load_font(height, weight, name);
    cache.insert(key, handle as usize);
    handle
}

pub(crate) fn ui_font() -> Handle {
    font(UI_FONT_HEIGHT, 400, 0)
}
pub(crate) fn semibold_font() -> Handle {
    font(UI_FONT_HEIGHT, 600, 1)
}
pub(crate) fn title_font() -> Handle {
    font(-25, 600, 1)
}
pub(crate) fn subtitle_font() -> Handle {
    font(-14, 400, 0)
}
pub(crate) fn small_font() -> Handle {
    font(-14, 400, 0)
}
pub(crate) fn brush(color: u32) -> Hbrush {
    let cache = BRUSH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(value) = cache.get(&color) {
        return *value as Hbrush;
    }
    let handle = {
        let brush = CreateSolidBrush(color);
        if brush.is_null() {
            GetSysColorBrush(15)
        } else {
            brush
        }
    };
    cache.insert(color, handle as usize);
    handle
}
pub(crate) fn body_brush() -> Hbrush {
    brush(c_body())
}
pub(crate) fn card_brush() -> Hbrush {
    brush(c_card())
}
pub(crate) fn panel_brush() -> Hbrush {
    brush(c_panel())
}
pub(crate) fn border_brush() -> Hbrush {
    brush(c_border())
}
pub(crate) fn header_icon() -> Handle {
    let current = dpi() as u32;
    let cache = ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(value) = cache.get(&current) {
        return *value as Handle;
    }
    let size = scale(40);
    let icon = unsafe {
        let icon = LoadImageW(
            GetModuleHandleW(ptr::null()),
            ptr::without_provenance::<u16>(1),
            1,
            size,
            size,
            0,
        );
        if icon.is_null() {
            LoadIconW(
                GetModuleHandleW(ptr::null()),
                ptr::without_provenance::<u16>(1),
            )
        } else {
            icon
        }
    };
    cache.insert(current, icon as usize);
    icon
}
pub(crate) fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Height in DIPs of `text` wrapped at `width` DIPs (at least one line).
pub(crate) fn wrapped_text_height(text: &str, font: Handle, width: i32) -> i32 {
    let dc = CreateCompatibleDC(ptr::null_mut());
    if dc.is_null() {
        return 20;
    }
    let wide_text = wide(text);
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: scale(width).max(1),
        bottom: 0,
    };
    unsafe {
        let old = SelectObject(dc, font);
        DrawTextW(
            dc,
            wide_text.as_ptr(),
            -1,
            &mut rect,
            0x0010 | DT_CALCRECT | DT_NOPREFIX,
        ); // DT_WORDBREAK
        SelectObject(dc, old);
        DeleteDC(dc);
    }
    unscale(rect.bottom).max(20)
}

pub(crate) fn measured_text_width(_hwnd: Hwnd, text: &str, font: Handle) -> i32 {
    // Measure on a memory DC so layout never touches a window DC mid-resize.
    let dc = CreateCompatibleDC(ptr::null_mut());
    if dc.is_null() {
        return 0;
    }
    let text = wide(text);
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        let old = SelectObject(dc, font);
        DrawTextW(
            dc,
            text.as_ptr(),
            (text.len() - 1) as i32,
            &mut rect,
            DT_SINGLELINE | DT_CALCRECT | DT_NOPREFIX,
        );
        SelectObject(dc, old);
        DeleteDC(dc);
    }
    rect.right
}

/// # Safety
/// Window procedure called by Windows. `hwnd` is a valid window handle, and for messages
/// that pass pointers in `lparam` (such as WM_DRAWITEM or WM_NCCALCSIZE) it points to the
/// structure the system defines for that message, valid for the duration of the call.
pub(crate) unsafe extern "system" fn font_child_proc(hwnd: Hwnd, parameter: Lparam) -> i32 {
    {
        win::send_int(hwnd, WM_SETFONT, parameter as usize, 1);
    }
    1
}

pub(crate) fn refresh_fonts(hwnd: Hwnd, state: &UiState) {
    unsafe {
        EnumChildWindows(hwnd, font_child_proc, ui_font() as usize as Lparam);
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
        win::send_int(state.advanced_hint, WM_SETFONT, small_font() as usize, 1);
    }
}
