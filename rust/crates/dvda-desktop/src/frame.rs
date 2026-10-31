//! Borderless-frame sizing: window-size clamps, the outline resize and the splitter preview.

use super::*;

pub(crate) fn clamp_window_size(width: i32, height: i32) -> (i32, i32) {
    let mut work = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if unsafe { SystemParametersInfoW(0x0030, 0, &mut work as *mut Rect as *mut c_void, 0) } != 0 {
        let available_w = unscale(work.right - work.left) - 24;
        let available_h = unscale(work.bottom - work.top) - 24;
        (
            width.min(available_w.max(800)),
            height.min(available_h.max(500)),
        )
    } else {
        (width, height)
    }
}

pub(crate) static SPLITTER_PREVIEW_CLASS_READY: OnceLock<()> = OnceLock::new();

/// # Safety
/// Window procedure called by Windows. `hwnd` is a valid window handle, and for messages
/// that pass pointers in `lparam` (such as WM_DRAWITEM or WM_NCCALCSIZE) it points to the
/// structure the system defines for that message, valid for the duration of the call.
pub(crate) unsafe extern "system" fn splitter_preview_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    match message {
        WM_PAINT => {
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
                FillRect(dc, &rect, brush(c_accent()));
                EndPaint(hwnd, &paint);
            }
            0
        }
        WM_ERASEBKGND => 1,
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// # Safety
/// `owner` must be a live window created on the UI thread. The returned popup is owned by it.
pub(crate) unsafe fn ensure_splitter_preview(owner: Hwnd) -> Hwnd {
    SPLITTER_PREVIEW_CLASS_READY.get_or_init(|| unsafe {
        let class_name = wide("DVD_AUDIO_SPLITTER_PREVIEW_RUST");
        let registration = WndClass {
            style: 0,
            procedure: splitter_preview_proc,
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
    let class_name = wide("DVD_AUDIO_SPLITTER_PREVIEW_RUST");
    unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            class_name.as_ptr(),
            ptr::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            owner,
            ptr::null_mut(),
            GetModuleHandleW(ptr::null()),
            ptr::null_mut(),
        )
    }
}

/// Moving the left or top edge moves the window origin, and DWM then shows
/// the old content at the new position for a frame before the repaint, so
/// right/bottom-anchored controls jumped back and forth. Such drags show a
/// translucent outline of the new bounds instead and resize once on release
/// (Esc or a lost capture cancels). Right/bottom drags stay live.
/// Whether Windows Snap arranged the window (IsWindowArranged, Windows 10+).
pub(crate) fn window_arranged(hwnd: Hwnd) -> bool {
    type IsWindowArranged = unsafe extern "system" fn(Hwnd) -> i32;
    static FUNCTION: OnceLock<usize> = OnceLock::new();
    let address = *FUNCTION.get_or_init(|| unsafe {
        let module = GetModuleHandleW(wide("user32.dll").as_ptr());
        if module.is_null() {
            0
        } else {
            GetProcAddress(module, c"IsWindowArranged".as_ptr().cast()) as usize
        }
    });
    address != 0 && unsafe { std::mem::transmute::<usize, IsWindowArranged>(address)(hwnd) } != 0
}

/// # Safety
/// `hwnd` must be the main window and this must run on its UI thread, from its
/// WM_NCLBUTTONDOWN handler. It runs a nested message loop that dispatches messages
/// to windows of this process.
pub(crate) unsafe fn outline_resize(hwnd: Hwnd, hit: usize, origin: Point) {
    unsafe {
        let start = window_rect(hwnd);
        // Same limits as the window manager would apply.
        let mut limits = [0i32; 10];
        limits[8] = i32::MAX / 4;
        limits[9] = i32::MAX / 4;
        SendMessageW(hwnd, WM_GETMINMAXINFO, 0, limits.as_mut_ptr() as Lparam);
        let (min_w, min_h) = (limits[6].max(1), limits[7].max(1));
        let (max_w, max_h) = (limits[8].max(min_w), limits[9].max(min_h));
        let left = matches!(hit as i32, HTLEFT | HTTOPLEFT | HTBOTTOMLEFT);
        let top = matches!(hit as i32, HTTOP | HTTOPLEFT | HTTOPRIGHT);
        let right = matches!(hit as i32, HTRIGHT | HTTOPRIGHT | HTBOTTOMRIGHT);
        let bottom = matches!(hit as i32, HTBOTTOM | HTBOTTOMLEFT | HTBOTTOMRIGHT);
        let target = |cursor: Point| {
            let (dx, dy) = (cursor.x - origin.x, cursor.y - origin.y);
            let mut rect = start;
            if left {
                rect.left = (start.left + dx).clamp(start.right - max_w, start.right - min_w);
            }
            if right {
                rect.right = (start.right + dx).clamp(start.left + min_w, start.left + max_w);
            }
            if top {
                rect.top = (start.top + dy).clamp(start.bottom - max_h, start.bottom - min_h);
                // Like Snap: a top edge taken to the top of the screen makes the
                // window as tall as the work area.
                let mut info = MonitorInfo {
                    size: std::mem::size_of::<MonitorInfo>() as u32,
                    ..std::mem::zeroed()
                };
                if GetMonitorInfoW(MonitorFromPoint(cursor, 2), &mut info) != 0
                    && cursor.y <= info.monitor.top
                {
                    rect.top = info.work.top;
                    rect.bottom = info.work.bottom;
                }
            }
            if bottom {
                rect.bottom = (start.bottom + dy).clamp(start.top + min_h, start.top + max_h);
                // Mirror of the top edge: a bottom edge taken to the bottom of the
                // screen makes the window as tall as the work area.
                let mut info = MonitorInfo {
                    size: std::mem::size_of::<MonitorInfo>() as u32,
                    ..std::mem::zeroed()
                };
                if GetMonitorInfoW(MonitorFromPoint(cursor, 2), &mut info) != 0
                    && cursor.y >= info.monitor.bottom - 1
                {
                    rect.top = info.work.top;
                    rect.bottom = info.work.bottom;
                }
            }
            rect
        };
        let mut outline: Hwnd = ptr::null_mut();
        let show = |outline: Hwnd, rect: Rect| {
            SetWindowPos(
                outline,
                -1isize as Hwnd,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                0x0050, // SWP_NOACTIVATE | SWP_SHOWWINDOW
            );
        };
        // The outline appears only once the pointer really drags, so a click
        // or a double-click on the edge does not flash it.
        let (drag_x, drag_y) = (GetSystemMetrics(68).max(1), GetSystemMetrics(69).max(1)); // SM_CXDRAG, SM_CYDRAG
        let mut dragging = false;
        let mut current = start;
        SetCapture(hwnd);
        let mut apply = false;
        let mut message: Message = std::mem::zeroed();
        // WM_QUIT (GetMessageW returns 0) must reach the outer loop again, or the
        // process would keep running after the window is destroyed mid-drag.
        let mut quit = None;
        while GetCapture() == hwnd {
            match GetMessageW(&mut message, ptr::null_mut(), 0, 0) {
                0 => {
                    quit = Some(message.wparam as i32);
                    break;
                }
                -1 => break,
                _ => {}
            }
            match message.message {
                0x0200 => {
                    // WM_MOUSEMOVE
                    let mut cursor = Point { x: 0, y: 0 };
                    GetCursorPos(&mut cursor);
                    if !dragging
                        && (cursor.x - origin.x).abs() < drag_x
                        && (cursor.y - origin.y).abs() < drag_y
                    {
                        continue;
                    }
                    if !dragging {
                        dragging = true;
                        outline = ensure_splitter_preview(hwnd);
                        if !outline.is_null() {
                            // Translucent accent fill (LWA_ALPHA) over the new bounds.
                            let style = GetWindowLongPtrW(outline, -20) as u32;
                            SetWindowLongPtrW(outline, -20, (style | 0x0008_0000) as isize); // WS_EX_LAYERED
                            SetLayeredWindowAttributes(outline, 0, 70, 0x2);
                        }
                    }
                    current = target(cursor);
                    if !outline.is_null() {
                        show(outline, current);
                    }
                }
                0x0202 => {
                    // WM_LBUTTONUP
                    apply = true;
                    break;
                }
                0x0100 if message.wparam == 0x1b => break, // Esc cancels
                // Like the system sizing loop, other keys do nothing meanwhile.
                0x0100..=0x0109 => {}
                _ => {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
        if GetCapture() == hwnd {
            ReleaseCapture();
        }
        if !outline.is_null() {
            ShowWindow(outline, SW_HIDE);
            DestroyWindow(outline);
        }
        if let Some(code) = quit {
            PostQuitMessage(code);
            return;
        }
        if apply
            && dragging
            && (current.left != start.left
                || current.top != start.top
                || current.right != start.right
                || current.bottom != start.bottom)
        {
            let dpi = GetDpiForWindow(hwnd);
            if window_arranged(hwnd) {
                // A snapped window leaves the snap layout with the new bounds
                // as its normal (restore) position, so a later caption drag
                // does not jump back to the size from before the snap.
                // rcNormalPosition uses workspace coordinates.
                let mut info = MonitorInfo {
                    size: std::mem::size_of::<MonitorInfo>() as u32,
                    ..std::mem::zeroed()
                };
                let center = Point {
                    x: (current.left + current.right) / 2,
                    y: (current.top + current.bottom) / 2,
                };
                let (dx, dy) = if GetMonitorInfoW(MonitorFromPoint(center, 2), &mut info) != 0 {
                    (
                        info.work.left - info.monitor.left,
                        info.work.top - info.monitor.top,
                    )
                } else {
                    (0, 0)
                };
                let mut placement = WindowPlacement {
                    length: std::mem::size_of::<WindowPlacement>() as u32,
                    ..WindowPlacement::default()
                };
                GetWindowPlacement(hwnd, &mut placement);
                placement.flags = 0;
                placement.show = 1; // SW_SHOWNORMAL
                placement.normal = Rect {
                    left: current.left - dx,
                    top: current.top - dy,
                    right: current.right - dx,
                    bottom: current.bottom - dy,
                };
                SetWindowPlacement(hwnd, &placement);
            }
            let apply_rect = || {
                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    current.left,
                    current.top,
                    current.right - current.left,
                    current.bottom - current.top,
                    0x0014, // SWP_NOZORDER | SWP_NOACTIVATE
                )
            };
            apply_rect();
            // Landing on a monitor with another DPI makes WM_DPICHANGED apply
            // its scaled suggestion; keep the bounds the outline showed.
            if GetDpiForWindow(hwnd) != dpi {
                apply_rect();
            }
        }
    }
}

/// # Safety
/// `main` must be the window that owns `state`, and this must run on its UI thread.
pub(crate) unsafe fn update_splitter_preview(state: &mut UiState, main: Hwnd, client_y: i32) {
    let mut preview = state.splitter_preview_window;
    if preview.is_null() {
        preview = unsafe { ensure_splitter_preview(main) };
        state.splitter_preview_window = preview;
    }
    if preview.is_null() {
        return;
    }
    // Same range as layout() allows, measured from the real tabs top.
    let low = scale(state.splitter_tabs_y + 104);
    let client_y = client_y.clamp(
        low,
        scale(state.splitter_tabs_y + state.splitter_max.max(104)).max(low),
    );
    state.splitter_preview = Some(client_y);
    let mut point = Point { x: 0, y: client_y };
    let mut window_rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        ClientToScreen(main, &mut point);
        GetWindowRect(main, &mut window_rect);
        let height = scale(3).max(2);
        SetWindowPos(
            preview,
            -1isize as Hwnd,
            window_rect.left,
            point.y - height / 2,
            window_rect.right - window_rect.left,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

pub(crate) fn hide_splitter_preview(state: &mut UiState) {
    if !state.splitter_preview_window.is_null() {
        {
            ShowWindow(state.splitter_preview_window, SW_HIDE);
        }
    }
    state.splitter_preview = None;
}
