//! The main window procedure.

use super::*;

/// # Safety
/// Window procedure called by Windows. `hwnd` is a valid window handle, and for messages
/// that pass pointers in `lparam` (such as WM_DRAWITEM or WM_NCCALCSIZE) it points to the
/// structure the system defines for that message, valid for the duration of the call.
pub(crate) unsafe extern "system" fn window_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    if message == WM_CLOSE {
        let Some(mut state) = (unsafe { state(hwnd) }) else {
            if GetWindowLongPtrW(hwnd, GWLP_USERDATA) != 0 {
                // The state is borrowed further up the stack (a modal dialog is open
                // inside a handler). Destroying the window now would free it under that
                // borrow, so the next WM_TIMER closes once the borrow has ended.
                CLOSE_REQUESTED.store(true, Ordering::Relaxed);
                return 0;
            }
            DestroyWindow(hwnd);
            return 0;
        };
        if state.busy {
            state.close_when_done = true;
            cancel_operation(&mut state);
            return 0;
        }
        autosave(&mut state);
        drop(state);
        DestroyWindow(hwnd);
        return 0;
    }
    if message == WM_DESTROY {
         {
            KillTimer(hwnd, 1);
        }
        PAINT_FIELDS.with(|fields| fields.borrow_mut().remove(&(hwnd as usize)));
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<UiState>;
        if !pointer.is_null() {
            // Detach first: nothing reached while the state drops may find it again.
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(pointer));
            }
        }
         {
            PostQuitMessage(0);
        }
        return 0;
    }
    match message {
        WM_CREATE => {
            let ui = Box::new(RefCell::new(create_controls(hwnd)));
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(ui) as isize);
                SetTimer(hwnd, 1, 100, ptr::null());
            }
            if let Some(mut state) = unsafe { state(hwnd) } {
                apply_theme(hwnd, &mut state, dark_theme());
            }
            0
        }
        WM_TIMER => {
            if let Some(mut state) = unsafe { state(hwnd) } {
                if CLOSE_REQUESTED.swap(false, Ordering::Relaxed) {
                    unsafe {
                        PostMessageW(hwnd, WM_CLOSE, 0, 0);
                    }
                }
                drain_worker_logs(hwnd, &mut state);
                update_statistics(&mut state);
                unsafe {
                    refresh_dynamic_indicators(&state);
                }
            }
            0
        }
        WM_SETTINGCHANGE => {
            if let Some(mut state) = unsafe { state(hwnd) }
                && state.theme_override.is_none()
            {
                let system = system_prefers_dark();
                if system != dark_theme() {
                    apply_theme(hwnd, &mut state, system);
                }
            }
            0
        }
        WM_NCLBUTTONDOWN => {
            // Every sizing edge and corner (HTLEFT..=HTBOTTOMRIGHT) uses the outline.
            if matches!(wparam as i32, HTLEFT..=HTBOTTOMRIGHT) && IsZoomed(hwnd) == 0 {
                let origin = Point {
                    x: (lparam & 0xffff) as u16 as i16 as i32,
                    y: ((lparam >> 16) & 0xffff) as u16 as i16 as i32,
                };
                unsafe { outline_resize(hwnd, wparam, origin) };
                0
            } else {
                unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
            }
        }
        WM_NCLBUTTONDBLCLK => {
            if wparam as i32 == HTCAPTION {
                 {
                    if IsZoomed(hwnd) != 0 {
                        ShowWindow(hwnd, SW_RESTORE);
                    } else {
                        ShowWindow(hwnd, SW_MAXIMIZE);
                    }
                }
                return 0;
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        0x0200..=0x0202 => {
            if let Some(mut state) = unsafe { state(hwnd) } {
                let y = (lparam >> 16) as i16 as i32;
                let over = (state.splitter_position() - scale(5)
                    ..state.splitter_position() + scale(10))
                    .contains(&y);
                if message == 0x0201 && over {
                    state.dragging_splitter = true;
                    unsafe {
                        SetCapture(hwnd);
                        update_splitter_preview(&mut state, hwnd, y);
                    }
                } else if message == 0x0202 && state.dragging_splitter {
                    state.dragging_splitter = false;
                     {
                        ReleaseCapture();
                    }
                    let preview = state.splitter_preview;
                    hide_splitter_preview(&mut state);
                    if let Some(preview_y) = preview {
                        state.splitter_height = Some(
                            (unscale(preview_y) - state.splitter_tabs_y).clamp(104, state.splitter_max.max(104)),
                        );
                        // layout() moves children with SWP_NOREDRAW and then
                        // repaints the whole tree synchronously.
                        layout(hwnd, &mut state);
                    }
                } else if message == 0x0200 && state.dragging_splitter && wparam & 1 != 0 {
                    unsafe { update_splitter_preview(&mut state, hwnd, y) };
                }
                if over || state.dragging_splitter {
                    unsafe {
                        SetCursor(LoadCursorW(ptr::null_mut(), 32645usize as *const u16));
                    }
                    return 0;
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_SIZE => {
            if wparam != 1
                && let Some(mut state) = unsafe { state(hwnd) }
            {
                layout(hwnd, &mut state);
            }
            0
        }
        0x0231 | 0x0232 => {
            // WM_ENTERSIZEMOVE / WM_EXITSIZEMOVE: the log keeps its size while
            // the user drags an edge and gets the final size once at the end.
            if let Some(mut state) = unsafe { state(hwnd) } {
                state.sizing = message == 0x0231;
                if !state.sizing && state.log_frozen {
                    layout(hwnd, &mut state);
                    if state.log_dirty && checked(state.controls.live) {
                        render_log(&mut state);
                    }
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_NCCALCSIZE => {
            if wparam != 0 {
                // Borderless frame: leave the proposed window rectangle in
                // place so the client area covers the whole window.
                // By default Windows then copies the old client pixels to the
                // new client's top-left corner. Dragging the left or top edge
                // thus showed right/bottom-anchored controls one step off
                // before the repaint, so they jittered back and forth. Mark a
                // single identical pixel as valid instead so nothing is
                // copied; layout() repaints the client synchronously. (DWM
                // can still present an occasional frame early.)
                let rects = lparam as *mut [Rect; 3];
                unsafe {
                    let client = (*rects)[0];
                    let pixel = Rect {
                        left: client.left,
                        top: client.top,
                        right: client.left + 1,
                        bottom: client.top + 1,
                    };
                    (*rects)[1] = pixel;
                    (*rects)[2] = pixel;
                }
                0x0400 // WVR_VALIDRECTS
            } else {
                unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
            }
        }
        WM_NCHITTEST => {
            let result = unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
            let x = (lparam & 0xffff) as u16 as i16 as i32;
            let y = ((lparam >> 16) & 0xffff) as u16 as i16 as i32;
            let mut point = Point { x, y };
            let mut rect = Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
             {
                ScreenToClient(hwnd, &mut point);
                GetClientRect(hwnd, &mut rect);
            }
            // Borderless windows still need a usable sizing frame.
            if IsZoomed(hwnd) == 0 {
                let edge = scale(6).max(4);
                let left = point.x < edge;
                let right = point.x >= rect.right - edge;
                let top = point.y < edge;
                let bottom = point.y >= rect.bottom - edge;
                if left || right || top || bottom {
                    let code = if top && left {
                        HTTOPLEFT
                    } else if top && right {
                        HTTOPRIGHT
                    } else if bottom && left {
                        HTBOTTOMLEFT
                    } else if bottom && right {
                        HTBOTTOMRIGHT
                    } else if left {
                        HTLEFT
                    } else if right {
                        HTRIGHT
                    } else if top {
                        HTTOP
                    } else {
                        HTBOTTOM
                    };
                    return code as Lresult;
                }
            }
            if result == HTCLIENT as Lresult
                && point.x >= 0
                && point.x < rect.right
                && point.y >= 0
                && point.y < header_height()
            {
                return HTCAPTION as Lresult;
            }
            result
        }
        WM_GETMINMAXINFO => {
            if lparam != 0 {
                // MINMAXINFO consists of five POINTs; minimum tracking size is fourth.
                let dimensions = unsafe { &mut *(lparam as *mut [i32; 10]) };
                let mut min_width = scale(640);
                let mut min_height = scale(360);
                let mut work = Rect {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                };
                if unsafe {
                    SystemParametersInfoW(0x0030, 0, &mut work as *mut Rect as *mut c_void, 0)
                } != 0
                {
                    // A very small or heavily scaled screen must still be usable.
                    let work_width = work.right - work.left;
                    let work_height = work.bottom - work.top;
                    min_width = min_width.min(work_width);
                    min_height = min_height.min(work_height);
                    // The borderless window must not cover the taskbar when
                    // maximized.
                    dimensions[2] = work_width;
                    dimensions[3] = work_height;
                    dimensions[4] = work.left;
                    dimensions[5] = work.top;
                    dimensions[8] = work_width;
                    dimensions[9] = work_height;
                }
                dimensions[6] = min_width;
                dimensions[7] = min_height;
            }
            0
        }
        WM_DPICHANGED => {
            let new_dpi = (wparam & 0xffff) as u32;
            if new_dpi >= 48 {
                UI_DPI.store(new_dpi, Ordering::Relaxed);
            }
            if lparam != 0 {
                let suggested = unsafe { &*(lparam as *const Rect) };
                unsafe {
                    SetWindowPos(
                        hwnd,
                        ptr::null_mut(),
                        suggested.left,
                        suggested.top,
                        suggested.right - suggested.left,
                        suggested.bottom - suggested.top,
                        0x0004 | 0x0010,
                    );
                }
            }
            if let Some(mut state) = unsafe { state(hwnd) } {
                rescale_ui(hwnd, &mut state);
            }
            0
        }
        WM_COMMAND => {
            let id = wparam & 0xffff;
            let notify = ((wparam >> 16) & 0xffff) as u16;
            // Window commands run before the UiState is borrowed: ShowWindow and
            // WM_CLOSE send messages back to this procedure synchronously, and a
            // borrowed state would skip their layout and close checks.
            match id {
                ID_MAXIMIZE => {
                    if IsZoomed(hwnd) != 0 {
                        ShowWindow(hwnd, SW_RESTORE);
                    } else {
                        ShowWindow(hwnd, SW_MAXIMIZE);
                    }
                    return 0;
                }
                ID_CLOSE => {
                    win::send_int(hwnd, WM_CLOSE, 0, 0);
                    return 0;
                }
                _ => {}
            }
            if let Some(mut state) = unsafe { state(hwnd) } {
                handle_command(hwnd, &mut state, id, notify);
            }
            0
        }
        WM_DRAWITEM => {
            if lparam != 0 {
                let item = unsafe { &*(lparam as *const DrawItemStruct) };
                if unsafe { draw_owner_item(hwnd, item) } {
                    return 1;
                }
            }
            0
        }
        WM_MEASUREITEM => {
            if lparam != 0 {
                let item = unsafe { &mut *(lparam as *mut MeasureItemStruct) };
                item.item_height = scale(26).max(scale(20)) as u32;
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            unsafe { paint_window(hwnd) };
            0
        }
        WM_NOTIFY => 0,
        WM_CTLCOLORSTATIC => {
            let dc = wparam as Handle;
            let control = lparam as Hwnd;
            SetBkMode(dc, 1);
            if IsWindowEnabled(control) == 0 && window_class(control) == "Edit" {
                SetBkColor(dc, c_disabled_fill());
                SetTextColor(dc, c_disabled_text());
                brush(c_disabled_fill()) as Lresult
            } else if GetDlgCtrlID(control) as usize == ID_LOG {
                // The log is a read-only EDIT, so it asks for its colour here. Its id
                // identifies it without the UiState, which a layout may hold.
                SetBkColor(dc, c_card());
                SetTextColor(dc, c_ink());
                card_brush() as Lresult
            } else {
                SetBkColor(dc, c_body());
                let muted = paint_fields(hwnd).is_some_and(|paint| paint.muted.contains(&control));
                SetTextColor(dc, if muted { c_muted() } else { c_ink() });
                body_brush() as Lresult
            }
        },
        WM_CTLCOLOREDIT =>  {
            let dc = wparam as Handle;
            SetBkMode(dc, 1);
            SetBkColor(dc, c_card());
            SetTextColor(dc, c_ink());
            card_brush() as Lresult
        },
        WM_CTLCOLORLISTBOX =>  {
            SetBkMode(wparam as Handle, 1);
            SetBkColor(wparam as Handle, c_card());
            SetTextColor(wparam as Handle, c_ink());
            card_brush() as Lresult
        },
        WM_CTLCOLORBTN =>  {
            SetBkMode(wparam as Handle, 1);
            SetBkColor(wparam as Handle, c_body());
            body_brush() as Lresult
        },
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}
