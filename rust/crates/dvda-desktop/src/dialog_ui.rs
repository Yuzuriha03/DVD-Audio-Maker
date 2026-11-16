#![allow(
    clippy::wildcard_imports,
    reason = "The dialog shell composes the shared window primitives from the crate root"
)]
//! Shared Windows popup style, layout and modal behaviour.
//!
//! Message dialogs, task choices and the About page all use this module for the
//! same borderless chrome, palette, fonts, spacing, owner-draw buttons, DPI
//! handling and keyboard ownership as the main window.

use std::{cell::Cell, rc::Rc, sync::OnceLock};

use super::controls::owner_draw_button;
use super::draw::{
    begin_window_paint, color16, draw_text_run, fill_round_rect, finish_window_paint,
    rgb_components, stroke_rect,
};
use super::gdi::load_font;
use super::*;

const WM_NCHITTEST_LOCAL: u32 = 0x0084;
const WM_MOUSEWHEEL_LOCAL: u32 = 0x020a;
const WM_NCHITTEST: u32 = 0x0084;
const WM_NCLBUTTONDOWN: u32 = 0x00a1;
const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
const WS_EX_CONTROLPARENT: u32 = 0x0001_0000;
const SB_PAGEUP: i32 = 2;
const SB_PAGEDOWN: i32 = 3;
const SB_THUMBTRACK: i32 = 5;
const HTCLIENT_LOCAL: isize = 1;
const HTCAPTION_LOCAL: isize = 2;
const CS_DROPSHADOW: u32 = 0x0002_0000;
const IDOK: usize = 1;
const IDCANCEL: usize = 2;
const RESULT_NONE: i32 = i32::MIN;
const HEADER_TITLE: &str = "DVD-Audio Maker";

/// Popup severity only selects the header accent and the default title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DialogKind {
    Warning,
    Error,
}

impl DialogKind {
    fn accent(self) -> u32 {
        match self {
            Self::Warning => c_header_bottom(),
            Self::Error => c_danger(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ButtonKind {
    Primary,
    Danger,
    ListItem,
    Close,
}

/// Fonts and DPI for one popup, created from the owning window DPI.
#[derive(Clone, Copy)]
pub(crate) struct Theme {
    pub(crate) dpi: u32,
    pub(crate) title: Handle,
    pub(crate) subtitle: Handle,
    pub(crate) body: Handle,
    pub(crate) small: Handle,
    pub(crate) strong: Handle,
    pub(crate) mono: Handle,
}

impl Theme {
    pub(crate) fn new(owner: Hwnd) -> Self {
        let dpi = if owner.is_null() {
            96
        } else {
            let value = GetDpiForWindow(owner);
            if value >= 48 { value } else { 96 }
        };
        Self::for_dpi(dpi)
    }

    pub(crate) fn for_dpi(dpi: u32) -> Self {
        Self {
            dpi,
            title: load_font(sd(dpi, -25), 600, "Segoe UI Semibold"),
            subtitle: load_font(sd(dpi, -14), 400, "Segoe UI"),
            body: load_font(sd(dpi, -16), 400, "Segoe UI"),
            small: load_font(sd(dpi, -13), 400, "Segoe UI"),
            strong: load_font(sd(dpi, -16), 600, "Segoe UI Semibold"),
            mono: load_font(sd(dpi, -13), 400, "Consolas"),
        }
    }

    pub(crate) fn sd(&self, value: i32) -> i32 {
        sd(self.dpi, value)
    }

    pub(crate) fn header_height(&self) -> i32 {
        self.sd(HEADER_HEIGHT)
    }

    pub(crate) fn delete(self) {
        for font in [
            self.title,
            self.subtitle,
            self.body,
            self.small,
            self.strong,
            self.mono,
        ] {
            DeleteObject(font);
        }
    }
}

/// Converts a design value in DIPs to physical pixels at the given DPI.
fn sd(dpi: u32, value: i32) -> i32 {
    let scaled = (i64::from(value) * i64::from(dpi) + 48) / 96;
    i32::try_from(scaled).unwrap_or(i32::MAX)
}

pub(crate) fn wrapped_height(text: &str, font: Handle, width: i32) -> i32 {
    let dc = CreateCompatibleDC(ptr::null_mut());
    if dc.is_null() {
        return 20;
    }
    let text = wide(text);
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: width.max(1),
        bottom: 0,
    };
    unsafe {
        let old = SelectObject(dc, font);
        DrawTextW(
            dc,
            text.as_ptr(),
            -1,
            &raw mut rect,
            DT_CALCRECT | DT_NOPREFIX | 0x0010, // DT_WORDBREAK
        );
        SelectObject(dc, old);
        DeleteDC(dc);
    }
    rect.bottom.max(1)
}

pub(crate) fn measure_button(theme: &Theme, text: &str) -> i32 {
    (measured_text_width(ptr::null_mut(), text, theme.strong) + theme.sd(36)).max(theme.sd(88))
}

pub(crate) fn create_button(parent: Hwnd, text: &str, id: usize, kind: ButtonKind) -> Hwnd {
    let _ = kind;
    owner_draw_button(
        parent,
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_CLIPSIBLINGS | BS_OWNERDRAW,
        id,
        (0, 0, 10, 10),
    )
}

pub(crate) fn create_label(parent: Hwnd, text: &str) -> Hwnd {
    let text = wide(text);
    unsafe {
        CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
            0,
            0,
            10,
            10,
            parent,
            ptr::null_mut(),
            GetModuleHandleW(ptr::null()),
            ptr::null_mut(),
        )
    }
}

/// Paints the shared main-window chrome for any application popup.
pub(crate) unsafe fn paint_chrome(
    hwnd: Hwnd,
    dc: Handle,
    theme: &Theme,
    subtitle: &str,
    accent: u32,
) {
    unsafe {
        let mut rect = Rect::default();
        GetClientRect(hwnd, &mut rect);
        let header = theme.header_height();
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
                x: rect.right,
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
        if GradientFill(dc, vertices.as_mut_ptr(), 2, mesh.as_ptr(), 1, 1) == 0 {
            let header_rect = Rect {
                left: 0,
                top: 0,
                right: rect.right,
                bottom: header,
            };
            FillRect(dc, &header_rect, brush(c_header_top()));
        }
        let strip = Rect {
            left: 0,
            top: header - theme.sd(2),
            right: rect.right,
            bottom: header,
        };
        FillRect(dc, &strip, brush(accent));
        let icon_size = theme.sd(40);
        DrawIconEx(
            dc,
            theme.sd(20),
            (header - icon_size) / 2,
            header_icon(),
            icon_size,
            icon_size,
            0,
            ptr::null_mut(),
            3,
        );
        let mut title = Rect {
            left: theme.sd(72),
            top: theme.sd(8),
            right: (rect.right - theme.sd(70)).max(theme.sd(160)),
            bottom: theme.sd(42),
        };
        draw_text_run(
            HEADER_TITLE,
            dc,
            &mut title,
            theme.title,
            c_header_title(),
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        let mut subtitle_rect = Rect {
            left: theme.sd(73),
            top: theme.sd(38),
            right: title.right,
            bottom: theme.sd(60),
        };
        draw_text_run(
            subtitle,
            dc,
            &mut subtitle_rect,
            theme.subtitle,
            c_header_subtitle(),
            DT_LEFT | DT_TOP | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
    }
}

/// Creates the shared close caption button used by every popup header.
pub(crate) fn create_close_button(parent: Hwnd) -> Hwnd {
    owner_draw_button(
        parent,
        "",
        WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | BS_OWNERDRAW,
        ID_CLOSE,
        (0, 0, 10, 10),
    )
}

fn header_color_at(theme: &Theme, y: i32) -> u32 {
    let height = theme.header_height().max(1);
    let ratio = f64::from(y.clamp(0, height)) / f64::from(height);
    let (top_r, top_g, top_b) = rgb_components(c_header_top());
    let (bottom_r, bottom_g, bottom_b) = rgb_components(c_header_bottom());
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Interpolated RGB channels stay within 0..=255 and intentionally truncate fractional channel values"
    )]
    let mix = |top: u32, bottom: u32| -> u32 {
        (f64::from(top) + (f64::from(bottom) - f64::from(top)) * ratio) as u32
    };
    rgb(
        mix(top_r, bottom_r),
        mix(top_g, bottom_g),
        mix(top_b, bottom_b),
    )
}

/// Caption close control: matches the borderless header caption buttons. Only
/// keyboard focus draws an outline; default and hover states have no stroke.
unsafe fn draw_close_button(item: &DrawItemStruct, theme: &Theme) {
    let hover = button_hovered(item.item);
    let center_y = i32::midpoint(item.rect.top, item.rect.bottom);
    let background = if hover {
        rgb(196, 43, 28)
    } else {
        header_color_at(theme, center_y)
    };
    let glyph = if hover {
        rgb(255, 255, 255)
    } else {
        c_header_title()
    };
    unsafe {
        FillRect(item.dc, &item.rect, brush(background));
        let center_x = i32::midpoint(item.rect.left, item.rect.right);
        let unit = theme.sd(6);
        let pen = CreatePen(0, theme.sd(1).max(1), glyph);
        if !pen.is_null() {
            let old = SelectObject(item.dc, pen);
            MoveToEx(item.dc, center_x - unit, center_y - unit, ptr::null_mut());
            LineTo(item.dc, center_x + unit, center_y + unit);
            MoveToEx(item.dc, center_x + unit, center_y - unit, ptr::null_mut());
            LineTo(item.dc, center_x - unit, center_y + unit);
            SelectObject(item.dc, old);
            DeleteObject(pen);
        }
        if item.state & ODS_FOCUS != 0 {
            let inset = theme.sd(4);
            let focus = Rect {
                left: item.rect.left + inset,
                top: item.rect.top + inset,
                right: item.rect.right - inset,
                bottom: item.rect.bottom - inset,
            };
            DrawFocusRect(item.dc, &raw const focus);
        }
    }
}

/// Draws one shared owner-draw button.
#[allow(
    clippy::too_many_lines,
    reason = "One button renderer keeps every popup control family visually identical"
)]
pub(crate) unsafe fn draw_button(
    item: &DrawItemStruct,
    kind: ButtonKind,
    theme: &Theme,
    selected: bool,
) {
    if kind == ButtonKind::Close {
        unsafe { draw_close_button(item, theme) };
        return;
    }
    unsafe {
        let parent = GetParent(item.item);
        if !parent.is_null() {
            let background =
                SendMessageW(parent, WM_CTLCOLORBTN, item.dc as usize, item.item as isize)
                    as Hbrush;
            if !background.is_null() {
                FillRect(item.dc, &item.rect, background);
            }
        }
        let disabled = item.state & ODS_DISABLED != 0;
        let pressed = item.state & ODS_SELECTED != 0;
        let hover = !disabled && button_hovered(item.item);
        let radius = theme.sd(if kind == ButtonKind::Primary { 8 } else { 6 });
        let (fill, border, text, font) = match kind {
            ButtonKind::Primary => {
                let fill = if disabled {
                    c_disabled_fill()
                } else if pressed {
                    c_accent_pressed()
                } else if hover {
                    c_accent_hover()
                } else {
                    c_accent()
                };
                (fill, fill, c_header_top(), theme.strong)
            }
            ButtonKind::Danger => {
                let fill = if disabled {
                    c_disabled_fill()
                } else if pressed {
                    c_danger_pressed()
                } else if hover {
                    c_danger_hover()
                } else {
                    c_card()
                };
                (
                    fill,
                    c_danger_border(),
                    if disabled {
                        c_disabled_text()
                    } else {
                        c_danger()
                    },
                    theme.body,
                )
            }
            ButtonKind::Close => (c_body(), c_body(), c_ink(), theme.body),
            ButtonKind::ListItem => {
                let fill = if selected {
                    c_accent_soft()
                } else if hover {
                    c_secondary_hover()
                } else {
                    c_card()
                };
                (fill, fill, c_ink(), theme.body)
            }
        };
        fill_round_rect(item.dc, &item.rect, radius, fill, border);
        let label = get_text(item.item);
        let mut text_rect = item.rect;
        if kind == ButtonKind::ListItem {
            text_rect.left += theme.sd(14);
        }
        draw_text_run(
            &label,
            item.dc,
            &mut text_rect,
            font,
            text,
            if kind == ButtonKind::ListItem {
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX
            } else {
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX
            },
        );
        if item.state & ODS_FOCUS != 0 {
            let inset = theme.sd(4);
            let focus = Rect {
                left: item.rect.left + inset,
                top: item.rect.top + inset,
                right: item.rect.right - inset,
                bottom: item.rect.bottom - inset,
            };
            if kind == ButtonKind::ListItem {
                // The selected-row border alone is too weak when the page is
                // inactive; draw the same solid focus ring as the main window's
                // primary action so keyboard users can see where they are.
                let pen = CreatePen(0, theme.sd(2).max(1), c_accent());
                if !pen.is_null() {
                    let old_pen = SelectObject(item.dc, pen);
                    stroke_rect(item.dc, &focus);
                    SelectObject(item.dc, old_pen);
                    DeleteObject(pen);
                }
            } else {
                DrawFocusRect(item.dc, &raw const focus);
            }
        }
    }
}

/// Lays buttons out from the right edge, wrapping into extra rows when the
/// width cannot hold them. Returns the top-left corner of each button.
pub(crate) fn layout_button_row(
    theme: &Theme,
    width: i32,
    margin: i32,
    bottom: i32,
    buttons: &[(Hwnd, i32)],
) -> Vec<(i32, i32, i32, i32)> {
    let gap = theme.sd(10);
    let height = theme.sd(34);
    let mut placed = vec![(0, 0, 0, 0); buttons.len()];
    let mut row_y = bottom - height;
    let mut cursor = width - margin;
    let mut row: Vec<(usize, i32)> = Vec::new();
    for (index, (_, button_width)) in buttons.iter().enumerate() {
        let needed = if row.is_empty() {
            *button_width
        } else {
            row.iter().map(|(_, w)| *w).sum::<i32>() + gap
        };
        if !row.is_empty() && cursor - needed < margin {
            flush_row(theme, &row, &mut placed, row_y, cursor);
            row.clear();
            row_y -= height + gap;
            cursor = width - margin;
        }
        let width_here = *button_width;
        cursor -= width_here;
        row.push((index, width_here));
        if !row.is_empty() && row.len() > 1 {
            cursor -= gap;
        }
    }
    flush_row(theme, &row, &mut placed, row_y, cursor);
    placed
}

fn flush_row(
    theme: &Theme,
    row: &[(usize, i32)],
    placed: &mut [(i32, i32, i32, i32)],
    y: i32,
    mut x: i32,
) {
    let gap = theme.sd(10);
    let height = theme.sd(34);
    for (index, width) in row {
        placed[*index] = (x, y, *width, height);
        x += width + gap;
    }
}

struct DialogState {
    theme: Theme,
    kind: DialogKind,
    title: String,
    message: String,
    label: Hwnd,
    close: Hwnd,
    buttons: Vec<(Hwnd, usize, ButtonKind)>,
    default_id: usize,
    cancel_id: usize,
    result: Rc<Cell<i32>>,
}

fn dialog_state(hwnd: Hwnd) -> Option<&'static mut DialogState> {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DialogState;
    unsafe { pointer.as_mut() }
}

fn layout_dialog(hwnd: Hwnd) {
    let Some(state) = dialog_state(hwnd) else {
        return;
    };
    let mut client = Rect::default();
    GetClientRect(hwnd, &mut client);
    let theme = &state.theme;
    let margin = theme.sd(20);
    let header = theme.header_height();
    let close_size = theme.sd(40);
    MoveWindow(
        state.close,
        client.right - close_size,
        (header - close_size) / 2,
        close_size,
        close_size,
        1,
    );
    let width = (client.right - margin * 2).max(theme.sd(160));
    let text_top = header + theme.sd(18);
    let text_height = wrapped_height(&state.message, theme.body, width);
    MoveWindow(state.label, margin, text_top, width, text_height, 1);
    let button_top =
        (text_top + text_height + theme.sd(18)).max(client.bottom - margin - theme.sd(34));
    let specs: Vec<(Hwnd, i32)> = state
        .buttons
        .iter()
        .map(|(hwnd, _, _)| (*hwnd, measure_button(theme, &get_text(*hwnd))))
        .collect();
    let placed = layout_button_row(theme, client.right, margin, client.bottom - margin, &specs);
    for ((hwnd, _, _), (x, y, width, height)) in state.buttons.iter().zip(placed) {
        let y = y.max(button_top);
        MoveWindow(*hwnd, x, y, width, height, 1);
    }
    unsafe {
        InvalidateRect(hwnd, ptr::null(), 0);
    }
}

fn apply_theme(state: &DialogState) {
    unsafe {
        SendMessageW(state.label, WM_SETFONT, state.theme.body as usize, 1);
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "The dialog procedure keeps popup lifecycle and owner-draw dispatch together"
)]
unsafe extern "system" fn dialog_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    match message {
        WM_SIZE => {
            layout_dialog(hwnd);
            0
        }
        WM_PAINT => {
            if let Some(state) = dialog_state(hwnd) {
                unsafe {
                    let mut paint: PaintStruct = std::mem::zeroed();
                    let buffer = begin_window_paint(hwnd, &mut paint, c_body());
                    paint_chrome(
                        hwnd,
                        buffer.memory,
                        &state.theme,
                        &state.title,
                        state.kind.accent(),
                    );
                    finish_window_paint(hwnd, &paint, buffer);
                }
            }
            0
        }
        WM_ERASEBKGND => {
            let mut client = Rect::default();
            GetClientRect(hwnd, &mut client);
            FillRect(wparam as Handle, &client, brush(c_body()));
            1
        }
        WM_CTLCOLORSTATIC => {
            let dc = wparam as Handle;
            SetBkMode(dc, 1);
            SetTextColor(dc, c_ink());
            body_brush() as Lresult
        }
        WM_CTLCOLORBTN => body_brush() as Lresult,
        WM_DRAWITEM => {
            let item = unsafe { &*(lparam as *const DrawItemStruct) };
            if let Some(state) = dialog_state(hwnd) {
                let id = usize::try_from(item.control_id).unwrap_or_default();
                if id == ID_CLOSE {
                    unsafe {
                        draw_button(item, ButtonKind::Close, &state.theme, false);
                    }
                    return 1;
                }
                if let Some((_, _, kind)) = state
                    .buttons
                    .iter()
                    .find(|(control, _, _)| *control == item.item)
                {
                    unsafe {
                        draw_button(item, *kind, &state.theme, false);
                    }
                    return 1;
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_COMMAND => {
            let id = wparam & 0xffff;
            if let Some(state) = dialog_state(hwnd) {
                if id == state.cancel_id || id == ID_CLOSE || id == IDCANCEL {
                    state
                        .result
                        .set(i32::try_from(state.cancel_id).unwrap_or(-1));
                    return 0;
                }
                if state
                    .buttons
                    .iter()
                    .any(|(_, button_id, _)| *button_id == id)
                {
                    state.result.set(i32::try_from(id).unwrap_or(-1));
                    return 0;
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_CLOSE => {
            if let Some(state) = dialog_state(hwnd) {
                state
                    .result
                    .set(i32::try_from(state.cancel_id).unwrap_or(-1));
            }
            0
        }
        WM_NCHITTEST_LOCAL => {
            let y = i32::from(
                u16::try_from((lparam >> 16) & 0xffff)
                    .unwrap()
                    .cast_signed(),
            );
            let header = dialog_state(hwnd).map_or(0, |state| state.theme.header_height());
            if y < header {
                HTCAPTION_LOCAL as Lresult
            } else {
                HTCLIENT_LOCAL as Lresult
            }
        }
        WM_DPICHANGED => {
            let new_dpi = u32::try_from(wparam & 0xffff).unwrap();
            if let Some(state) = dialog_state(hwnd)
                && new_dpi >= 48
                && new_dpi != state.theme.dpi
            {
                let old = std::mem::replace(&mut state.theme, Theme::for_dpi(new_dpi));
                apply_theme(state);
                old.delete();
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
                        0x0014,
                    );
                }
            }
            layout_dialog(hwnd);
            0
        }
        WM_DESTROY => {
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DialogState;
            if !pointer.is_null() {
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    let state = Box::from_raw(pointer);
                    state.theme.delete();
                    drop(state);
                }
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

#[repr(C)]
struct PageScrollInfo {
    size: u32,
    mask: u32,
    minimum: i32,
    maximum: i32,
    page: u32,
    position: i32,
    track: i32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetScrollInfo(hwnd: Hwnd, bar: i32, info: *mut PageScrollInfo) -> i32;
}

struct PageScrollVisual {
    host: Hwnd,
    dragging: bool,
    drag_offset: i32,
}

static PAGE_SCROLLBAR_REGISTERED: OnceLock<()> = OnceLock::new();

fn page_scrollbar_geometry(hwnd: Hwnd, info: &PageScrollInfo) -> Option<(i32, i32, i32)> {
    let mut client = Rect::default();
    GetClientRect(hwnd, &mut client);
    let track = client.bottom.max(1);
    let total = info.maximum.saturating_sub(info.minimum) + 1;
    if total <= 0 || i32::try_from(info.page).unwrap_or(0) >= total {
        return None;
    }
    let visible = i32::try_from(info.page).unwrap_or(0).max(1).min(total);
    let minimum_thumb = (client.right * 2).max(16);
    let thumb = ((visible * track) / total).max(minimum_thumb).min(track);
    let maximum_scroll = total - visible;
    let top = if maximum_scroll <= 0 {
        0
    } else {
        ((info.position - info.minimum).clamp(0, maximum_scroll) * (track - thumb)) / maximum_scroll
    };
    Some((top, thumb, track))
}

#[allow(
    clippy::too_many_lines,
    reason = "Keeps the shared page scrollbar paint, hit-test and drag lifecycle together"
)]
unsafe extern "system" fn page_scrollbar_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PageScrollVisual;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let visual = unsafe { &mut *pointer };
    match message {
        WM_NCHITTEST => unsafe { SendMessageW(GetParent(hwnd), message, wparam, lparam) },
        WM_NCLBUTTONDOWN => {
            unsafe { SendMessageW(GetParent(hwnd), message, wparam, lparam) };
            0
        }
        WM_PAINT => {
            unsafe {
                let mut paint: PaintStruct = std::mem::zeroed();
                let buffer = begin_window_paint(hwnd, &mut paint, c_body());
                let mut info: PageScrollInfo = std::mem::zeroed();
                info.size = u32::try_from(std::mem::size_of::<PageScrollInfo>()).unwrap();
                info.mask = 0x0017;
                if GetScrollInfo(visual.host, 1, &raw mut info) == 0 {
                    // Fall through with zeroed metrics; the thumb is skipped.
                    info.maximum = -1;
                }
                if let Some((top, height, _track)) = page_scrollbar_geometry(hwnd, &info) {
                    let mut rect = Rect::default();
                    GetClientRect(hwnd, &mut rect);
                    let inset = 2;
                    let thumb = Rect {
                        left: inset,
                        top,
                        right: (rect.right - inset).max(inset + 1),
                        bottom: top + height,
                    };
                    FillRect(
                        buffer.memory,
                        &thumb,
                        brush(if visual.dragging {
                            c_muted()
                        } else {
                            c_border()
                        }),
                    );
                }
                finish_window_paint(hwnd, &paint, buffer);
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONDOWN => {
            let mut info: PageScrollInfo = unsafe { std::mem::zeroed() };
            info.size = u32::try_from(std::mem::size_of::<PageScrollInfo>()).unwrap();
            info.mask = 0x0017;
            unsafe {
                GetScrollInfo(visual.host, 1, &raw mut info);
            }
            let y = i32::from(
                u16::try_from((lparam >> 16) & 0xffff)
                    .unwrap()
                    .cast_signed(),
            );
            if let Some((top, height, _track)) = page_scrollbar_geometry(hwnd, &info) {
                if y >= top && y < top + height {
                    visual.dragging = true;
                    visual.drag_offset = y - top;
                    SetCapture(hwnd);
                } else {
                    unsafe {
                        SendMessageW(
                            visual.host,
                            WM_VSCROLL,
                            usize::try_from(if y < top { SB_PAGEUP } else { SB_PAGEDOWN })
                                .unwrap_or(0),
                            0,
                        );
                    }
                }
            }
            0
        }
        WM_MOUSEMOVE => {
            if visual.dragging {
                let mut info: PageScrollInfo = unsafe { std::mem::zeroed() };
                info.size = u32::try_from(std::mem::size_of::<PageScrollInfo>()).unwrap();
                info.mask = 0x0017;
                unsafe {
                    GetScrollInfo(visual.host, 1, &raw mut info);
                }
                if let Some((_top, height, track)) = page_scrollbar_geometry(hwnd, &info) {
                    let y = i32::from(
                        u16::try_from((lparam >> 16) & 0xffff)
                            .unwrap()
                            .cast_signed(),
                    );
                    let thumb_top = (y - visual.drag_offset).clamp(0, (track - height).max(0));
                    let total = info.maximum.saturating_sub(info.minimum) + 1;
                    let visible = i32::try_from(info.page).unwrap_or(0).max(1).min(total);
                    let maximum_scroll = total - visible;
                    let position = if maximum_scroll <= 0 {
                        0
                    } else {
                        (thumb_top * maximum_scroll) / (track - height).max(1)
                    };
                    let packed = usize::try_from((position & 0xffff) << 16).unwrap_or(0);
                    unsafe {
                        SendMessageW(
                            visual.host,
                            WM_VSCROLL,
                            (SB_THUMBTRACK as usize) | packed,
                            0,
                        );
                    }
                }
            }
            0
        }
        WM_LBUTTONUP => {
            if visual.dragging {
                visual.dragging = false;
                ReleaseCapture();
            }
            0
        }
        WM_DESTROY => {
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PageScrollVisual;
            if !pointer.is_null() {
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    drop(Box::from_raw(pointer));
                }
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// Shared dark page scrollbar for popups whose content scrolls above a fixed
/// footer. It paints with the same palette as the main window's scrollbars.
pub(crate) fn create_page_scrollbar(parent: Hwnd) -> Hwnd {
    PAGE_SCROLLBAR_REGISTERED.get_or_init(|| {
        let class_name = wide("DVD_AUDIO_DIALOG_SCROLLBAR_RUST");
        let registration = WndClass {
            style: 0,
            procedure: page_scrollbar_proc as WndProc,
            extra_class: 0,
            extra_window: 0,
            instance: unsafe { GetModuleHandleW(ptr::null()) },
            icon: ptr::null_mut(),
            cursor: unsafe { LoadCursorW(ptr::null_mut(), 32512usize as *const u16) },
            background: 16usize as *mut std::ffi::c_void,
            menu: ptr::null(),
            class_name: class_name.as_ptr(),
        };
        unsafe { RegisterClassW(&raw const registration) };
    });
    let window = unsafe {
        CreateWindowExW(
            0,
            wide("DVD_AUDIO_DIALOG_SCROLLBAR_RUST").as_ptr(),
            ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
            0,
            0,
            10,
            10,
            parent,
            ptr::null_mut(),
            GetModuleHandleW(ptr::null()),
            ptr::null_mut(),
        )
    };
    if !window.is_null() {
        let visual = Box::new(PageScrollVisual {
            host: parent,
            dragging: false,
            drag_offset: 0,
        });
        unsafe {
            SetWindowLongPtrW(window, GWLP_USERDATA, Box::into_raw(visual) as isize);
        }
    }
    window
}

/// Scrollable content host: children are clipped by this child window, so the
/// fixed chrome around it can never be overpainted. `WS_EX_CONTROLPARENT`
/// keeps Tab traversal inside the nested content.
pub(crate) fn create_content_host(parent: Hwnd) -> Hwnd {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    REGISTERED.get_or_init(|| {
        let class_name = wide("DVD_AUDIO_CONTENT_HOST_RUST");
        let registration = WndClass {
            style: 0,
            procedure: content_host_proc as WndProc,
            extra_class: 0,
            extra_window: 0,
            instance: unsafe { GetModuleHandleW(ptr::null()) },
            icon: ptr::null_mut(),
            cursor: unsafe { LoadCursorW(ptr::null_mut(), 32512usize as *const u16) },
            background: 16usize as *mut std::ffi::c_void,
            menu: ptr::null(),
            class_name: class_name.as_ptr(),
        };
        unsafe { RegisterClassW(&raw const registration) };
    });
    unsafe {
        CreateWindowExW(
            WS_EX_CONTROLPARENT,
            wide("DVD_AUDIO_CONTENT_HOST_RUST").as_ptr(),
            ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
            0,
            0,
            10,
            10,
            parent,
            ptr::null_mut(),
            GetModuleHandleW(ptr::null()),
            ptr::null_mut(),
        )
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "The nested content host keeps scrolling paint, colors and command forwarding together"
)]
unsafe extern "system" fn content_host_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    match message {
        WM_NCHITTEST => unsafe { SendMessageW(GetParent(hwnd), message, wparam, lparam) },
        WM_NCLBUTTONDOWN => {
            unsafe { SendMessageW(GetParent(hwnd), message, wparam, lparam) };
            0
        }
        WM_PAINT => {
            unsafe {
                let mut paint: PaintStruct = std::mem::zeroed();
                let buffer = begin_window_paint(hwnd, &mut paint, c_body());
                finish_window_paint(hwnd, &paint, buffer);
            }
            0
        }
        WM_ERASEBKGND => {
            let mut client = Rect::default();
            GetClientRect(hwnd, &mut client);
            FillRect(wparam as Handle, &client, body_brush());
            1
        }
        WM_CTLCOLORSTATIC => {
            let dc = wparam as Handle;
            SetBkMode(dc, 1);
            SetTextColor(dc, c_ink());
            body_brush() as Lresult
        }
        WM_CTLCOLOREDIT => {
            let dc = wparam as Handle;
            SetBkMode(dc, 1);
            SetBkColor(dc, c_card());
            SetTextColor(dc, c_ink());
            card_brush() as Lresult
        }
        WM_CTLCOLORBTN => body_brush() as Lresult,
        WM_DRAWITEM | WM_COMMAND | WM_MOUSEWHEEL_LOCAL => {
            let parent = GetParent(hwnd);
            if parent.is_null() {
                0
            } else {
                unsafe { SendMessageW(parent, message, wparam, lparam) }
            }
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}
/// # Safety
/// Modal popup procedure called by Windows; lparam pointers follow the message contract.
unsafe extern "system" fn dialog_wnd_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    unsafe { dialog_proc(hwnd, message, wparam, lparam) }
}

static REGISTERED: OnceLock<()> = OnceLock::new();

fn register_class() {
    REGISTERED.get_or_init(|| {
        let class_name = wide("DVD_AUDIO_DIALOG_RUST");
        let registration = WndClass {
            style: CS_DROPSHADOW,
            procedure: dialog_wnd_proc as WndProc,
            extra_class: 0,
            extra_window: 0,
            instance: unsafe { GetModuleHandleW(ptr::null()) },
            icon: ptr::null_mut(),
            cursor: unsafe { LoadCursorW(ptr::null_mut(), 32512usize as *const u16) },
            background: 16usize as *mut std::ffi::c_void,
            menu: ptr::null(),
            class_name: class_name.as_ptr(),
        };
        unsafe { RegisterClassW(&raw const registration) };
    });
}

pub(crate) fn center_on_owner(hwnd: Hwnd, owner: Hwnd, width: i32, height: i32) {
    if owner.is_null() {
        let screen_width = GetSystemMetrics(0);
        let screen_height = GetSystemMetrics(1);
        MoveWindow(
            hwnd,
            (screen_width - width) / 2,
            (screen_height - height) / 2,
            width,
            height,
            1,
        );
        return;
    }
    let mut rect = Rect::default();
    GetWindowRect(owner, &mut rect);
    MoveWindow(
        hwnd,
        rect.left + ((rect.right - rect.left) - width) / 2,
        rect.top + ((rect.bottom - rect.top) - height) / 2,
        width,
        height,
        1,
    );
}

fn dialog_keydown(message: &Message, dialog: Hwnd, default_id: usize, cancel_id: usize) -> bool {
    if message.id != WM_KEYDOWN {
        return false;
    }
    if message.wparam == VK_ESCAPE {
        unsafe {
            SendMessageW(dialog, WM_COMMAND, cancel_id, 0);
        }
        return true;
    }
    if message.wparam != VK_RETURN {
        return false;
    }
    let focus = GetFocus();
    let class = window_class(focus);
    if class == "Edit" {
        return false;
    }
    let id = if class == "Button" {
        let id = GetDlgCtrlID(focus);
        if id > 0 {
            usize::try_from(id).unwrap_or(default_id)
        } else {
            default_id
        }
    } else {
        default_id
    };
    unsafe {
        SendMessageW(dialog, WM_COMMAND, id, 0);
    }
    true
}

fn run_modal(hwnd: Hwnd, owner: Hwnd, result: &Rc<Cell<i32>>) {
    if !owner.is_null() {
        EnableWindow(owner, 0);
    }
    ShowWindow(hwnd, SW_SHOW);
    UpdateWindow(hwnd);
    if let Some(state) = dialog_state(hwnd) {
        let default = state
            .buttons
            .iter()
            .find(|(_, id, _)| *id == state.default_id);
        let focus = default.map_or(state.close, |(hwnd, _, _)| *hwnd);
        SetFocus(focus);
    }
    let mut message = Message {
        hwnd: ptr::null_mut(),
        id: 0,
        wparam: 0,
        lparam: 0,
        time: 0,
        point_x: 0,
        point_y: 0,
    };
    loop {
        if result.get() != RESULT_NONE || IsWindow(hwnd) == 0 {
            break;
        }
        let status = GetMessageW(&mut message, ptr::null_mut(), 0, 0);
        if status <= 0 {
            break;
        }
        let (default_id, cancel_id) = dialog_state(hwnd).map_or((IDOK, IDCANCEL), |state| {
            (state.default_id, state.cancel_id)
        });
        unsafe {
            if dialog_keydown(&message, hwnd, default_id, cancel_id) {
                continue;
            }
            if IsDialogMessageW(hwnd, &raw mut message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&raw const message);
            }
        }
    }
    if !owner.is_null() {
        EnableWindow(owner, 1);
        SetFocus(owner);
    }
    DestroyWindow(hwnd);
}

fn create_dialog(
    owner: Hwnd,
    kind: DialogKind,
    title: &str,
    message: &str,
    specs: &[(String, ButtonKind, usize)],
    default_id: usize,
    cancel_id: usize,
) -> Option<(Hwnd, Rc<Cell<i32>>)> {
    register_class();
    let theme = Theme::new(owner);
    let width = theme.sd(560);
    let margin = theme.sd(20);
    let text_width = width - margin * 2;
    let text_height = wrapped_height(message, theme.body, text_width);
    let height =
        theme.header_height() + theme.sd(18) + text_height + theme.sd(18) + theme.sd(34) + margin;
    let title_wide = wide(title);
    let window = unsafe {
        CreateWindowExW(
            0,
            wide("DVD_AUDIO_DIALOG_RUST").as_ptr(),
            title_wide.as_ptr(),
            WS_POPUP | WS_CLIPCHILDREN,
            0,
            0,
            width,
            height,
            owner,
            ptr::null_mut(),
            GetModuleHandleW(ptr::null()),
            ptr::null_mut(),
        )
    };
    if window.is_null() {
        theme.delete();
        return None;
    }
    let result = Rc::new(Cell::new(RESULT_NONE));
    let close = create_close_button(window);
    let label = create_label(window, message);
    let mut buttons = Vec::with_capacity(specs.len());
    for (text, kind, id) in specs {
        let button = create_button(window, text, *id, *kind);
        buttons.push((button, *id, *kind));
    }
    let state = Box::new(DialogState {
        theme,
        kind,
        title: title.to_owned(),
        message: message.to_owned(),
        label,
        close,
        buttons,
        default_id,
        cancel_id,
        result: Rc::clone(&result),
    });
    unsafe {
        SetWindowLongPtrW(window, GWLP_USERDATA, Box::into_raw(state) as isize);
        if let Some(state) = dialog_state(window) {
            apply_theme(state);
        }
        center_on_owner(window, owner, width, height);
        layout_dialog(window);
    }
    Some((window, result))
}

pub(crate) fn create_message_dialog(
    owner: Hwnd,
    lang: Lang,
    kind: DialogKind,
    title: &str,
    message: &str,
) -> Option<(Hwnd, Rc<Cell<i32>>)> {
    let label = tr(lang, "dialog_ok").to_owned();
    let specs = [(label, ButtonKind::Primary, IDOK)];
    create_dialog(owner, kind, title, message, &specs, IDOK, IDOK)
}

pub(crate) fn create_choice_dialog(
    owner: Hwnd,
    kind: DialogKind,
    title: &str,
    message: &str,
    primary_label: &str,
    secondary_label: &str,
) -> Option<(Hwnd, Rc<Cell<i32>>)> {
    let specs = [
        (primary_label.to_owned(), ButtonKind::Primary, 1001),
        (secondary_label.to_owned(), ButtonKind::Danger, 1002),
    ];
    create_dialog(owner, kind, title, message, &specs, 1001, IDCANCEL)
}

pub(crate) fn show_message(owner: Hwnd, lang: Lang, kind: DialogKind, title: &str, message: &str) {
    if let Some((window, result)) = create_message_dialog(owner, lang, kind, title, message) {
        run_modal(window, owner, &result);
    }
}

pub(crate) fn show_choice(
    owner: Hwnd,
    _lang: Lang,
    kind: DialogKind,
    title: &str,
    message: &str,
    primary_label: &str,
    secondary_label: &str,
) -> Option<bool> {
    let (window, result) =
        create_choice_dialog(owner, kind, title, message, primary_label, secondary_label)?;
    run_modal(window, owner, &result);
    match result.get() {
        1001 => Some(true),
        1002 => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::ui_test_guard;
    use super::*;

    fn px(dpi: u32, value: i32) -> i32 {
        i32::try_from((i64::from(value) * i64::from(dpi) + 48) / 96).unwrap_or(i32::MAX)
    }

    fn rect_of(hwnd: Hwnd) -> Rect {
        let mut rect = Rect::default();
        GetWindowRect(hwnd, &mut rect);
        rect
    }

    fn dpi_message(window: Hwnd, dpi: u32) {
        let mut suggested = rect_of(window);
        let wparam = usize::try_from(dpi | (dpi << 16)).unwrap();
        unsafe {
            SendMessageW(window, WM_DPICHANGED, wparam, (&raw mut suggested) as isize);
        }
    }

    fn dialog_state_of(window: Hwnd) -> &'static DialogState {
        dialog_state(window).expect("dialog state")
    }

    #[test]
    fn message_dialog_uses_shared_style_and_localized_ok_in_five_languages() {
        let _ui_test_lock = ui_test_guard();
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja, Lang::Ko] {
            let (window, _result) = create_message_dialog(
                ptr::null_mut(),
                lang,
                DialogKind::Error,
                "title",
                "message body",
            )
            .unwrap();
            let state = dialog_state_of(window);
            assert_eq!(get_text(state.label), "message body");
            assert_eq!(state.buttons.len(), 1);
            let (button, id, _) = state.buttons[0];
            assert_eq!(id, IDOK);
            assert_eq!(get_text(button), tr(lang, "dialog_ok"));
            let style = u32::try_from(GetWindowLongPtrW(button, -16) & 0xffff_ffff).unwrap();
            assert!(style & WS_TABSTOP != 0);
            assert!(style & 0x0000_000b == 0x0000_000b, "owner-draw button");
            let header = state.theme.header_height();
            assert!(header > 0);
            DestroyWindow(window);
        }
    }

    #[test]
    fn message_dialog_stays_inside_640_width_at_every_dpi() {
        let _ui_test_lock = ui_test_guard();
        for dpi in [96u32, 120, 144, 168, 192] {
            let (window, _result) = create_message_dialog(
                ptr::null_mut(),
                Lang::En,
                DialogKind::Error,
                "Error",
                "A long wrapped message that must never cross the right edge of the dialog even at the narrowest supported width.",
            )
            .unwrap();
            dpi_message(window, dpi);
            MoveWindow(window, 0, 0, px(dpi, 640), px(dpi, 420), 0);
            let state = dialog_state_of(window);
            let bounds = rect_of(window);
            for control in
                std::iter::once(state.label).chain(state.buttons.iter().map(|(hwnd, _, _)| *hwnd))
            {
                let rect = rect_of(control);
                assert!(
                    rect.left >= bounds.left && rect.right <= bounds.right,
                    "control escapes right edge at {dpi} DPI"
                );
                assert!(
                    rect.bottom <= bounds.bottom,
                    "control escapes bottom at {dpi} DPI"
                );
            }
            DestroyWindow(window);
        }
    }

    #[test]
    fn choice_dialog_enter_and_escape_target_the_expected_buttons() {
        let _ui_test_lock = ui_test_guard();
        let (window, result) = create_choice_dialog(
            ptr::null_mut(),
            DialogKind::Warning,
            "choice",
            "keep or delete?",
            "Keep",
            "Delete",
        )
        .unwrap();
        let state = dialog_state_of(window);
        let primary = state.buttons[0].0;
        let enter = Message {
            hwnd: primary,
            id: WM_KEYDOWN,
            wparam: VK_RETURN,
            lparam: 0,
            time: 0,
            point_x: 0,
            point_y: 0,
        };
        SetFocus(primary);
        assert!(dialog_keydown(&enter, window, IDOK, IDCANCEL));
        assert_eq!(result.get(), 1001);
        DestroyWindow(window);

        let (window, result) = create_choice_dialog(
            ptr::null_mut(),
            DialogKind::Warning,
            "choice",
            "keep or delete?",
            "Keep",
            "Delete",
        )
        .unwrap();
        let state = dialog_state_of(window);
        let secondary = state.buttons[1].0;
        let enter = Message {
            hwnd: secondary,
            id: WM_KEYDOWN,
            wparam: VK_RETURN,
            lparam: 0,
            time: 0,
            point_x: 0,
            point_y: 0,
        };
        SetFocus(secondary);
        assert!(dialog_keydown(&enter, window, IDOK, IDCANCEL));
        assert_eq!(result.get(), 1002);
        DestroyWindow(window);

        let (window, result) = create_choice_dialog(
            ptr::null_mut(),
            DialogKind::Warning,
            "choice",
            "keep or delete?",
            "Keep",
            "Delete",
        )
        .unwrap();
        let escape = Message {
            hwnd: window,
            id: WM_KEYDOWN,
            wparam: VK_ESCAPE,
            lparam: 0,
            time: 0,
            point_x: 0,
            point_y: 0,
        };
        assert!(dialog_keydown(&escape, window, IDOK, IDCANCEL));
        assert_eq!(result.get(), i32::try_from(IDCANCEL).unwrap());
        DestroyWindow(window);
    }

    #[test]
    fn close_button_and_buttons_share_the_header_and_footer_focus_chain() {
        let _ui_test_lock = ui_test_guard();
        let (window, _result) = create_choice_dialog(
            ptr::null_mut(),
            DialogKind::Warning,
            "title",
            "message",
            "Keep",
            "Delete",
        )
        .unwrap();
        let state = dialog_state_of(window);
        let close_style = u32::try_from(GetWindowLongPtrW(state.close, -16) & 0xffff_ffff).unwrap();
        assert!(close_style & WS_TABSTOP == 0, "close stays mouse/Esc only");
        let mut tops = std::collections::BTreeSet::new();
        for (button, _, _) in &state.buttons {
            tops.insert(rect_of(*button).top);
        }
        assert_eq!(tops.len(), 1, "buttons share one footer row");
        DestroyWindow(window);
    }
}
