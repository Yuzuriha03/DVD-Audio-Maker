//! Owner drawing: buttons, tabs, caption buttons, combo items, labels, the window
//! background and buffered painting.

use super::*;

pub(crate) fn color16(value: u32) -> u16 {
    (value.min(255) * 257) as u16
}
pub(crate) fn rgb_components(color: u32) -> (u32, u32, u32) {
    (color & 0xff, (color >> 8) & 0xff, (color >> 16) & 0xff)
}

pub(crate) fn draw_text_run(
    text: &str,
    dc: Handle,
    rect: &mut Rect,
    font: Handle,
    color: u32,
    format: u32,
) {
    let text = wide(text);
    unsafe {
        let old = SelectObject(dc, font);
        SetBkMode(dc, 1);
        SetTextColor(dc, color);
        DrawTextW(
            dc,
            text.as_ptr(),
            (text.len() - 1) as i32,
            rect,
            format,
        );
        SelectObject(dc, old);
    }
}

pub(crate) fn fill_round_rect(dc: Handle, rect: &Rect, radius: i32, fill: u32, border: u32) {
    let outer = CreateSolidBrush(border);
    if outer.is_null() {
        return;
    }
    let inner = CreateSolidBrush(fill);
    if inner.is_null() {
        DeleteObject(outer);
        return;
    }
    unsafe {
        let old_pen = SelectObject(dc, GetStockObject(8));
        let old_brush = SelectObject(dc, outer);
        RoundRect(
            dc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            radius * 2,
            radius * 2,
        );
        SelectObject(dc, inner);
        let inset = radius.saturating_sub(1) * 2;
        RoundRect(
            dc,
            rect.left + 1,
            rect.top + 1,
            rect.right - 1,
            rect.bottom - 1,
            inset,
            inset,
        );
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
        DeleteObject(outer);
        DeleteObject(inner);
    }
}

/// # Safety
/// `item` must be the `DrawItemStruct` of a WM_DRAWITEM message for one of this
/// application's owner-draw buttons, and it is only valid during that message.
pub(crate) unsafe fn draw_secondary_button(item: &DrawItemStruct) {
    let disabled = item.state & ODS_DISABLED != 0;
    let pressed = item.state & ODS_SELECTED != 0;
    let hover = !disabled && button_hovered(item.item);
    let (fill, border, text) = if disabled {
        (c_disabled_fill(), c_border_soft(), c_disabled_text())
    } else if pressed {
        (c_secondary_pressed(), c_border(), c_ink())
    } else if hover {
        (c_secondary_hover(), c_border(), c_ink())
    } else {
        (c_card(), c_border(), c_ink())
    };
    unsafe {
        fill_round_rect(item.dc, &item.rect, scale(6), fill, border);
        let label = get_text(item.item);
        let mut text_rect = item.rect;
        draw_text_run(
            &label,
            item.dc,
            &mut text_rect,
            ui_font(),
            text,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        if item.state & ODS_FOCUS != 0 {
            let inset = scale(4);
            let focus = Rect {
                left: item.rect.left + inset,
                top: item.rect.top + inset,
                right: item.rect.right - inset,
                bottom: item.rect.bottom - inset,
            };
            DrawFocusRect(item.dc, &focus);
        }
    }
}

/// # Safety
/// `item` must be the `DrawItemStruct` of a WM_DRAWITEM message for one of this
/// application's owner-draw buttons, and it is only valid during that message.
pub(crate) unsafe fn draw_action_button(item: &DrawItemStruct, primary: bool) {
    let disabled = item.state & ODS_DISABLED != 0;
    let pressed = item.state & ODS_SELECTED != 0;
    let hover = button_hovered(item.item);
    let (fill, border, text) = if primary {
        if disabled {
            (c_disabled_fill(), c_disabled_fill(), c_disabled_text())
        } else if pressed {
            (c_accent_pressed(), c_accent_pressed(), c_header_top())
        } else if hover {
            (c_accent_hover(), c_accent_hover(), c_header_top())
        } else {
            (c_accent(), c_accent(), c_header_top())
        }
    } else if disabled {
        (c_disabled_fill(), c_border_soft(), c_disabled_text())
    } else if pressed {
        (c_danger_pressed(), c_danger_border(), c_danger())
    } else if hover {
        (c_danger_hover(), c_danger_border(), c_danger())
    } else {
        (c_card(), c_danger_border(), c_danger())
    };
    unsafe {
        fill_round_rect(item.dc, &item.rect, scale(8), fill, border);
        let label = get_text(item.item);
        let mut text_rect = item.rect;
        draw_text_run(
            &label,
            item.dc,
            &mut text_rect,
            if primary {
                semibold_font()
            } else {
                ui_font()
            },
            text,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        if item.state & ODS_FOCUS != 0 {
            let inset = scale(4);
            let focus = Rect {
                left: item.rect.left + inset,
                top: item.rect.top + inset,
                right: item.rect.right - inset,
                bottom: item.rect.bottom - inset,
            };
            DrawFocusRect(item.dc, &focus);
        }
    }
}

/// # Safety
/// `item` must be the `DrawItemStruct` of a WM_DRAWITEM message for one of this
/// application's owner-draw buttons, and it is only valid during that message.
pub(crate) unsafe fn draw_tab_button(item: &DrawItemStruct, active: bool) {
    let disabled = item.state & ODS_DISABLED != 0;
    let hover = !disabled && button_hovered(item.item);
    unsafe {
        FillRect(item.dc, &item.rect, body_brush());
        if active && !disabled {
            let underline = Rect {
                left: item.rect.left + scale(10),
                top: item.rect.bottom - scale(3),
                right: item.rect.right - scale(10),
                bottom: item.rect.bottom,
            };
            FillRect(item.dc, &underline, brush(c_accent()));
        } else if hover {
            let underline = Rect {
                left: item.rect.left + scale(24),
                top: item.rect.bottom - scale(2),
                right: item.rect.right - scale(24),
                bottom: item.rect.bottom - scale(1),
            };
            FillRect(item.dc, &underline, border_brush());
        }
        let label = get_text(item.item);
        let mut text_rect = Rect {
            left: item.rect.left,
            top: item.rect.top - scale(2),
            right: item.rect.right,
            bottom: item.rect.bottom - scale(6),
        };
        draw_text_run(
            &label,
            item.dc,
            &mut text_rect,
            if active {
                semibold_font()
            } else {
                ui_font()
            },
            if disabled {
                c_disabled_text()
            } else if active || hover {
                c_ink()
            } else {
                c_muted()
            },
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        if item.state & ODS_FOCUS != 0 && !disabled {
            let focus = Rect {
                left: item.rect.left + scale(4),
                top: item.rect.top + scale(4),
                right: item.rect.right - scale(4),
                bottom: item.rect.bottom - scale(6),
            };
            DrawFocusRect(item.dc, &focus);
        }
    }
}

pub(crate) fn header_gradient_at(y: i32) -> u32 {
    let height = header_height().max(1);
    let ratio = y.clamp(0, height) as f64 / height as f64;
    let (top_r, top_g, top_b) = rgb_components(c_header_top());
    let (bottom_r, bottom_g, bottom_b) = rgb_components(c_header_bottom());
    let mix = |top: u32, bottom: u32| -> u32 {
        (top as f64 + (bottom as f64 - top as f64) * ratio) as u32
    };
    rgb(mix(top_r, bottom_r), mix(top_g, bottom_g), mix(top_b, bottom_b))
}

pub(crate) fn lighten(color: u32, amount: i32) -> u32 {
    let (red, green, blue) = rgb_components(color);
    let adjust = |value: u32| (value as i32 + amount).clamp(0, 255) as u32;
    rgb(adjust(red), adjust(green), adjust(blue))
}

/// # Safety
/// `item` must be the `DrawItemStruct` of a WM_DRAWITEM message for one of this
/// application's caption buttons, and it is only valid during that message.
pub(crate) unsafe fn draw_caption_button(item: &DrawItemStruct, kind: u8) {
    let hover = button_hovered(item.item);
    let center_y = (item.rect.top + item.rect.bottom) / 2;
    let base = header_gradient_at(center_y);
    let fill = if hover {
        if kind == 2 {
            rgb(196, 43, 28)
        } else {
            lighten(base, 18)
        }
    } else {
        base
    };
    let glyph = if kind == 2 && hover {
        rgb(255, 255, 255)
    } else {
        c_header_title()
    };
    unsafe {
        FillRect(item.dc, &item.rect, brush(fill));
        let pen = CreatePen(0, scale(1).max(1), glyph);
        if pen.is_null() {
            return;
        }
        let old_pen = SelectObject(item.dc, pen);
        let center_x = (item.rect.left + item.rect.right) / 2;
        let unit = scale(6);
        match kind {
            0 => {
                MoveToEx(item.dc, center_x - unit, center_y, ptr::null_mut());
                LineTo(item.dc, center_x + unit, center_y);
            }
            1 => {
                let rect = if IsZoomed(GetParent(item.item)) != 0 {
                    Rect {
                        left: center_x - unit + scale(2),
                        top: center_y - unit,
                        right: center_x + unit - scale(1),
                        bottom: center_y + unit - scale(3),
                    }
                } else {
                    Rect {
                        left: center_x - unit,
                        top: center_y - unit,
                        right: center_x + unit,
                        bottom: center_y + unit,
                    }
                };
                stroke_rect(item.dc, &rect);
                if IsZoomed(GetParent(item.item)) != 0 {
                    let back = Rect {
                        left: center_x - unit,
                        top: center_y - unit + scale(3),
                        right: center_x + unit - scale(3),
                        bottom: center_y + unit,
                    };
                    stroke_rect(item.dc, &back);
                }
            }
            2 => {
                MoveToEx(
                    item.dc,
                    center_x - unit,
                    center_y - unit,
                    ptr::null_mut(),
                );
                LineTo(item.dc, center_x + unit, center_y + unit);
                MoveToEx(
                    item.dc,
                    center_x + unit,
                    center_y - unit,
                    ptr::null_mut(),
                );
                LineTo(item.dc, center_x - unit, center_y + unit);
            }
            _ => {
                if dark_theme() {
                    // Drawn crescent: the cut-out opens towards the upper left.
                    let moon_radius = unit;
                    let old_brush = SelectObject(item.dc, brush(glyph));
                    Ellipse(
                        item.dc,
                        center_x - moon_radius,
                        center_y - moon_radius,
                        center_x + moon_radius,
                        center_y + moon_radius,
                    );
                    let carve = moon_radius - scale(1);
                    let carve_pen = CreatePen(0, scale(1).max(1), fill);
                    if !carve_pen.is_null() {
                        let previous_pen = SelectObject(item.dc, carve_pen);
                        SelectObject(item.dc, brush(fill));
                        let carve_x = center_x - scale(4);
                        let carve_y = center_y - scale(4);
                        Ellipse(
                            item.dc,
                            carve_x - carve,
                            carve_y - carve,
                            carve_x + carve,
                            carve_y + carve,
                        );
                        SelectObject(item.dc, previous_pen);
                        DeleteObject(carve_pen);
                    }
                    SelectObject(item.dc, old_brush);
                } else {
                    // Drawn sun: filled disc with eight separate rays.
                    let radius = unit - scale(1);
                    let old_brush = SelectObject(item.dc, brush(glyph));
                    Ellipse(
                        item.dc,
                        center_x - radius,
                        center_y - radius,
                        center_x + radius,
                        center_y + radius,
                    );
                    SelectObject(item.dc, old_brush);
                    for index in 0..8 {
                        let angle = index as f64 * std::f64::consts::PI / 4.0;
                        let inner = radius as f64 + scale(3) as f64;
                        let outer = radius as f64 + scale(6) as f64;
                        MoveToEx(
                            item.dc,
                            center_x + (angle.cos() * inner) as i32,
                            center_y + (angle.sin() * inner) as i32,
                            ptr::null_mut(),
                        );
                        LineTo(
                            item.dc,
                            center_x + (angle.cos() * outer) as i32,
                            center_y + (angle.sin() * outer) as i32,
                        );
                    }
                }
            }        }
        SelectObject(item.dc, old_pen);
        DeleteObject(pen);
    }
}

pub(crate) fn stroke_rect(dc: Handle, rect: &Rect) {
    unsafe {
        MoveToEx(dc, rect.left, rect.top, ptr::null_mut());
        LineTo(dc, rect.right, rect.top);
        LineTo(dc, rect.right, rect.bottom);
        LineTo(dc, rect.left, rect.bottom);
        LineTo(dc, rect.left, rect.top);
    }
}

/// # Safety
/// `hwnd` must be a combo box of this process: the message writes into a buffer that
/// this function allocates in this process, so the window must not belong to another
/// process. `index` may be any value; invalid indices return an empty string.
pub(crate) unsafe fn combo_item_text(hwnd: Hwnd, index: i32) -> String {
    if index < 0 {
        return String::new();
    }
    let length = win::send_int(hwnd, CB_GETLBTEXTLEN, index as usize, 0);
    if length < 0 {
        return String::new();
    }
    let mut buffer = vec![0u16; length as usize + 1];
    let count = unsafe {
        SendMessageW(
            hwnd,
            CB_GETLBTEXT,
            index as usize,
            buffer.as_mut_ptr() as isize,
        )
    };
    if count < 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buffer[..count as usize])
    }
}

/// # Safety
/// `item` must be the `DrawItemStruct` of a WM_DRAWITEM message for a combo box of this
/// application, and it is only valid during that message.
pub(crate) unsafe fn draw_combo_item(item: &DrawItemStruct) {
    let selected = item.item_id == u32::MAX;
    let highlighted = item.state & ODS_SELECTED != 0;
    let disabled = item.state & ODS_DISABLED != 0;
    let index = if selected {
        win::send_int(item.item, CB_GETCURSEL, 0, 0) as i32
    } else {
        item.item_id as i32
    };
    let text = unsafe { combo_item_text(item.item, index) };
    let background = if highlighted && !selected {
        c_accent_soft()
    } else {
        c_card()
    };
     {
        FillRect(item.dc, &item.rect, brush(background));
        let mut rect = item.rect;
        rect.left += scale(8);
        rect.right -= scale(4);
        draw_text_run(
            &text,
            item.dc,
            &mut rect,
            ui_font(),
            if disabled {
                c_disabled_text()
            } else {
                c_ink()
            },
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
        );
    }
}

/// # Safety
/// `hwnd` must be the window that received this WM_DRAWITEM, and `item` must be that
/// message's `DrawItemStruct`, valid only during the message.
pub(crate) unsafe fn draw_owner_item(hwnd: Hwnd, item: &DrawItemStruct) -> bool {
    unsafe {
        let memory = CreateCompatibleDC(item.dc);
        let bitmap = CreateCompatibleBitmap(item.dc, item.rect.right.max(1), item.rect.bottom.max(1));
        if memory.is_null() || bitmap.is_null() {
            if !memory.is_null() { DeleteDC(memory); }
            if !bitmap.is_null() { DeleteObject(bitmap); }
            return draw_owner_item_contents(hwnd, item);
        }
        let previous = SelectObject(memory, bitmap);
        // Rounded corners must show the parent's background. Copying the
        // screen instead would preserve stale pixels from earlier layouts,
        // because the clipped parent never repaints under its children.
        let background = SendMessageW(GetParent(item.item), WM_CTLCOLORBTN, memory as Wparam, item.item as Lparam) as Hbrush;
        FillRect(memory, &item.rect, if background.is_null() { body_brush() } else { background });
        let buffered = DrawItemStruct {
            control_type: item.control_type, control_id: item.control_id,
            item_id: item.item_id, action: item.action, state: item.state,
            item: item.item, dc: memory,
            rect: Rect { left: item.rect.left, top: item.rect.top, right: item.rect.right, bottom: item.rect.bottom },
            data: item.data,
        };
        let handled = draw_owner_item_contents(hwnd, &buffered);
        BitBlt(item.dc, item.rect.left, item.rect.top,
            item.rect.right - item.rect.left, item.rect.bottom - item.rect.top,
            memory, item.rect.left, item.rect.top, 0x00cc0020);
        SelectObject(memory, previous);
        DeleteObject(bitmap);
        DeleteDC(memory);
        handled
    }
}

/// # Safety
/// `hwnd` must be the window that received this WM_DRAWITEM, and `item` must be that
/// message's `DrawItemStruct`, valid only during the message.
pub(crate) unsafe fn draw_owner_item_contents(hwnd: Hwnd, item: &DrawItemStruct) -> bool {
    let id = item.control_id as usize;
    if COMBO_IDS.contains(&id) {
        unsafe { draw_combo_item(item) };
        return true;
    }
    if (ID_MINIMIZE..=ID_THEME).contains(&id) {
        unsafe { draw_caption_button(item, (id - ID_MINIMIZE) as u8) };
        return true;
    }
    if id == ID_BUILD {
        unsafe { draw_action_button(item, true) };
        return true;
    }
    if id == ID_CANCEL {
        unsafe { draw_action_button(item, false) };
        return true;
    }
    if (ID_TAB..ID_TAB + 3).contains(&id) {
        let active = paint_fields(hwnd)
            .map(|paint| paint.tab_index.get() == id - ID_TAB)
            .unwrap_or(false);
        unsafe { draw_tab_button(item, active) };
        return true;
    }
    unsafe { draw_secondary_button(item) };
    true
}

pub(crate) struct BufferedPaint {
    target: Handle,
    pub(crate) memory: Handle,
    bitmap: Handle,
    previous: Handle,
    rect: Rect,
}

/// # Safety
/// `hwnd` must be inside its WM_PAINT, and `paint` must be the storage the same
/// BeginPaint call fills. Each call must be followed by `finish_buffered_paint` with
/// the same arguments, before the paint cycle ends.
pub(crate) unsafe fn begin_buffered_paint(
    hwnd: Hwnd,
    paint: &mut PaintStruct,
    background: u32,
) -> BufferedPaint {
    let target = unsafe { BeginPaint(hwnd, paint) };
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
     {
        GetClientRect(hwnd, &mut rect);
    }
    let width = rect.right.max(1);
    let height = rect.bottom.max(1);
    let memory = CreateCompatibleDC(target);
    let bitmap = CreateCompatibleBitmap(target, width, height);
    if memory.is_null() || bitmap.is_null() {
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        if !memory.is_null() {
            DeleteDC(memory);
        }
        return BufferedPaint {
            target,
            memory: target,
            bitmap: ptr::null_mut(),
            previous: ptr::null_mut(),
            rect,
        };
    }
    let previous = SelectObject(memory, bitmap);
     {
        FillRect(memory, &rect, brush(background));
    }
    BufferedPaint {
        target,
        memory,
        bitmap,
        previous,
        rect,
    }
}

/// # Safety
/// Must be called exactly once, with the `hwnd`, `paint` and `buffer` returned by the
/// matching `begin_buffered_paint` call in the same WM_PAINT.
pub(crate) unsafe fn finish_buffered_paint(hwnd: Hwnd, paint: &PaintStruct, buffer: &BufferedPaint) {
    unsafe {
        if !buffer.bitmap.is_null() {
            BitBlt(
                buffer.target,
                0,
                0,
                buffer.rect.right,
                buffer.rect.bottom,
                buffer.memory,
                0,
                0,
                0x00cc0020,
            );
            SelectObject(buffer.memory, buffer.previous);
            DeleteObject(buffer.bitmap);
            DeleteDC(buffer.memory);
        }
        EndPaint(hwnd, paint);
    }
}
/// # Safety
/// `hwnd` must be the main window, inside its WM_PAINT.
pub(crate) unsafe fn paint_window(hwnd: Hwnd) {
    let mut paint: PaintStruct = unsafe { std::mem::zeroed() };
    let buffer = unsafe { begin_buffered_paint(hwnd, &mut paint, c_body()) };
    let dc = buffer.memory;
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        GetClientRect(hwnd, &mut rect);
        let width = rect.right;
        let header = header_height();
        let (top_r, top_g, top_b) = rgb_components(c_header_top());
        let (bottom_r, bottom_g, bottom_b) = rgb_components(c_header_bottom());
        let mut vertices = [
            TriVertex {
                x: 0,
                y: 0,
                red: color16(top_r),
                green: color16(top_g),
                blue: color16(top_b),
                alpha: 0xff00,
            },
            TriVertex {
                x: width,
                y: header,
                red: color16(bottom_r),
                green: color16(bottom_g),
                blue: color16(bottom_b),
                alpha: 0xff00,
            },
        ];
        let mesh = [GradientRect {
            upper_left: 0,
            lower_right: 1,
        }];
        let gradient = GradientFill(dc, vertices.as_mut_ptr(), 2, mesh.as_ptr(), 1, 1);
        if gradient == 0 {
            let header_rect = Rect {
                left: 0,
                top: 0,
                right: width,
                bottom: header,
            };
            FillRect(dc, &header_rect, brush(c_header_top()));
        }
        let accent = Rect {
            left: 0,
            top: header - scale(2),
            right: width,
            bottom: header,
        };
        FillRect(dc, &accent, brush(c_accent()));
        let icon_size = scale(40);
        DrawIconEx(
            dc,
            scale(20),
            (header - icon_size) / 2,
            header_icon(),
            icon_size,
            icon_size,
            0,
            ptr::null_mut(),
            3,
        );
        // Text may run up to the language picker (296 DIPs from the right).
        let text_right = (width - scale(308)).max(scale(240));
        let mut title = Rect {
            left: scale(72),
            top: scale(8),
            right: text_right,
            bottom: scale(42),
        };
        draw_text_run(
            "DVD-Audio Maker",
            dc,
            &mut title,
            title_font(),
            c_header_title(),
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        let lang = paint_fields(hwnd).map_or(Lang::Zh, |paint| paint.lang.get());
        let mut subtitle = Rect {
            left: scale(73),
            top: scale(38),
            right: text_right,
            bottom: scale(60),
        };
        draw_text_run(
            tr(lang, "app_subtitle"),
            dc,
            &mut subtitle,
            subtitle_font(),
            c_header_subtitle(),
            DT_LEFT | DT_TOP | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        let splitter = paint_fields(hwnd)
            .map(|paint| paint.splitter_position.get())
            .unwrap_or(0);
        if splitter > header && splitter < rect.bottom - scale(60) {
            // Ends at the settings panel's right edge, which leaves room for the page
            // scrollbar (the page is placed `margin * 2 + 18` narrower than the window).
            let divider = Rect {
                left: scale(18),
                top: splitter - 1,
                right: width - scale(36),
                bottom: splitter,
            };
            FillRect(dc, &divider, border_brush());
            let center = width / 2;
            let grip = Rect {
                left: center - scale(22),
                top: splitter + scale(2),
                right: center + scale(22),
                bottom: splitter + scale(5),
            };
            FillRect(dc, &grip, border_brush());
        }
        finish_buffered_paint(hwnd, &paint, &buffer);
    }
}

/// # Safety
/// Window procedure called by Windows. `hwnd` is a valid window handle, and for messages
/// that pass pointers in `lparam` (such as WM_DRAWITEM or WM_NCCALCSIZE) it points to the
/// structure the system defines for that message, valid for the duration of the call.
pub(crate) unsafe extern "system" fn buffered_label_proc(
    hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam, id: usize, _data: usize,
) -> Lresult {
    #[link(name = "comctl32")]
    unsafe extern "system" {
        fn DefSubclassProc(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
        fn RemoveWindowSubclass(hwnd: Hwnd, proc: unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam, usize, usize) -> Lresult, id: usize) -> i32;
    }
    unsafe {
        match message {
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut paint = std::mem::zeroed();
                let background = if window_class(GetParent(hwnd)) == "DVD_AUDIO_PAGE_RUST" {
                    c_panel()
                } else {
                    c_body()
                };
                let buffer = begin_buffered_paint(hwnd, &mut paint, background);
                draw_label(hwnd, buffer.memory);
                finish_buffered_paint(hwnd, &paint, &buffer);
                0
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(hwnd, buffered_label_proc, id);
                DefSubclassProc(hwnd, message, wparam, lparam)
            }
            _ => DefSubclassProc(hwnd, message, wparam, lparam),
        }
    }
}

/// Vertical offset for text drawn into a box shorter than its line(s): lift
/// it by up to the font's internal leading so the descenders stay visible and
/// only the empty space above the glyphs is cut.
pub(crate) fn text_lift(dc: Handle, needed: i32, available: i32) -> i32 {
    let overflow = needed - available;
    if overflow <= 0 {
        return 0;
    }
    let mut metrics: TextMetric = TextMetric::default();
    GetTextMetricsW(dc, &mut metrics);
    overflow.min(metrics.internal_leading.max(0))
}

/// Paints a STATIC label with the native text semantics (alignment, word
/// wrap, prefixes, ellipsis styles) into the label's paint buffer.
/// # Safety
/// `hwnd` must be a STATIC label of this application, and `dc` the device context being
/// painted for it during WM_PAINT or WM_CTLCOLORSTATIC.
pub(crate) unsafe fn draw_label(hwnd: Hwnd, dc: Handle) {
    const DT_WORDBREAK: u32 = 0x0010;
    const DT_EXPANDTABS: u32 = 0x0040;
    const DT_END_ELLIPSIS: u32 = 0x8000;
    unsafe {
        let mut rect = client_rect(hwnd);
        // The parent picks the brush and text colour (muted, log, disabled).
        let brush = SendMessageW(GetParent(hwnd), WM_CTLCOLORSTATIC, dc as Wparam, hwnd as Lparam) as Hbrush;
        if !brush.is_null() {
            FillRect(dc, &rect, brush);
        }
        let text = get_text(hwnd);
        if text.is_empty() {
            return;
        }
        let style = GetWindowLongPtrW(hwnd, -16) as u32;
        let kind = style & 0x1f;
        let ellipsis = style & 0x4000 != 0; // SS_ENDELLIPSIS
        let mut flags = DT_EXPANDTABS
            | match kind {
                1 => DT_CENTER,
                2 => 0x0002, // DT_RIGHT
                _ => DT_LEFT,
            };
        if style & 0x0080 != 0 {
            flags |= DT_NOPREFIX; // SS_NOPREFIX
        }
        if kind == 0x0c || ellipsis {
            flags |= DT_SINGLELINE; // SS_LEFTNOWORDWRAP
        } else {
            flags |= DT_WORDBREAK;
        }
        if ellipsis {
            flags |= DT_END_ELLIPSIS;
        }
        let font = SendMessageW(hwnd, WM_SETFONT + 1, 0, 0) as Handle; // WM_GETFONT
        let old = SelectObject(dc, if font.is_null() { ui_font() } else { font });
        let wide_text = wide(&text);
        let mut needed = Rect { left: 0, top: 0, right: rect.right, bottom: 0 };
        DrawTextW(dc, wide_text.as_ptr(), -1, &mut needed, flags | DT_CALCRECT);
        rect.top -= text_lift(dc, needed.bottom, rect.bottom);
        DrawTextW(dc, wide_text.as_ptr(), -1, &mut rect, flags);
        SelectObject(dc, old);
    }
}

/// # Safety
/// `hwnd` must be a settings page of this application, inside its WM_PAINT.
pub(crate) unsafe fn paint_page(hwnd: Hwnd) {
    let mut paint: PaintStruct = unsafe { std::mem::zeroed() };
    let buffer = unsafe { begin_buffered_paint(hwnd, &mut paint, c_panel()) };
    let dc = buffer.memory;
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        GetClientRect(hwnd, &mut rect);
        FillRect(dc, &rect, panel_brush());
        let edges = [
            Rect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.top + 1,
            },
            Rect {
                left: rect.left,
                top: rect.bottom - 1,
                right: rect.right,
                bottom: rect.bottom,
            },
            Rect {
                left: rect.left,
                top: rect.top,
                right: rect.left + 1,
                bottom: rect.bottom,
            },
            Rect {
                left: rect.right - 1,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            },
        ];
        for edge in &edges {
            FillRect(dc, edge, border_brush());
        }
        finish_buffered_paint(hwnd, &paint, &buffer);
    }
}
