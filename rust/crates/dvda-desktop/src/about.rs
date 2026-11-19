#![allow(
    clippy::wildcard_imports,
    reason = "The page composes the shared window primitives from the crate root"
)]
//! About page built from the shared Windows popup style.
//!
//! The information hierarchy follows the Android author/credits page: author
//! block, project and version lines, then a licence selector with a reading
//! pane. Chrome, palette, fonts, buttons, DPI and keyboard ownership come from
//! `dialog_ui`, so the page matches the main window and every other popup.

use std::sync::{
    OnceLock,
    atomic::{AtomicBool, AtomicIsize, Ordering},
};

include!(concat!(env!("OUT_DIR"), "/licenses.rs"));

use super::controls::caption_button;
use super::dialog_ui::{
    self, ButtonKind, Theme, create_button, create_content_host, create_label,
    create_page_scrollbar, measure_button,
};
use super::draw::{begin_window_paint, draw_owner_item_contents, finish_window_paint};
use super::frame::{begin_outline_resize, borderless_hit_test, borderless_nccalcsize};
use super::updates;
use super::*;

const IDOK: usize = 1;
const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
const EM_SCROLLCARET: u32 = 0x00b7;
const WS_VSCROLL: u32 = 0x0020_0000;
const WM_MOUSEWHEEL_LOCAL: u32 = 0x020a;
const SB_VERT: i32 = 1;
const SIF_ALL: u32 = 0x0017;
const SB_LINEUP: i32 = 0;
const SB_LINEDOWN: i32 = 1;
const SB_PAGEUP: i32 = 2;
const SB_PAGEDOWN: i32 = 3;
const SB_THUMBPOSITION: i32 = 4;
const SB_THUMBTRACK: i32 = 5;
const SB_TOP: i32 = 6;
const SB_BOTTOM: i32 = 7;
const ES_AUTOVSCROLL_LOCAL: u32 = 0x0040;
const EDIT_MIN_HEIGHT_DIP: i32 = 150;
const MIN_WIDTH_DIP: i32 = 640;
const MIN_HEIGHT_DIP: i32 = 520;
const ID_LICENSE_BASE: usize = 7000;
const ID_UPDATE_CHECK: usize = 7010;

const DOCUMENTS: [(&str, &str); 6] = [
    ("LICENSE", PROJECT_LICENSE),
    ("FFMPEG-LICENSE.txt", FFMPEG_LICENSE),
    ("THIRD-PARTY.txt", THIRD_PARTY_ZH),
    ("THIRD-PARTY.en.txt", THIRD_PARTY_EN),
    ("NOTICE-Image.txt", IMAGE_NOTICES),
    ("NOTICE-Menu.txt", MENU_NOTICES),
];

#[repr(C)]
struct ScrollInfo {
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
    fn SetScrollInfo(hwnd: Hwnd, bar: i32, info: *const ScrollInfo, redraw: i32) -> i32;
    fn ShowScrollBar(hwnd: Hwnd, bar: i32, show: i32) -> i32;
}

#[derive(Clone, Copy)]
enum FontRole {
    Title,
    Body,
    Small,
    Strong,
}

struct AboutState {
    owner: Hwnd,
    theme: Theme,
    lang: Lang,
    title: String,
    name: Hwnd,
    role: Hwnd,
    bio: Hwnd,
    project: Hwnd,
    version: Hwnd,
    licenses: Hwnd,
    selected: Hwnd,
    edit: Hwnd,
    close: Hwnd,
    update_check: Hwnd,
    checking_updates: bool,
    footer: Hwnd,
    page_bar: Hwnd,
    content: Hwnd,
    license_buttons: Vec<Hwnd>,
    labels: Vec<(Hwnd, FontRole)>,
    bio_text: String,
    project_text: String,
    licenses_text: String,
    selected_index: usize,
    scroll: i32,
    content_height: i32,
}

static REGISTERED: OnceLock<()> = OnceLock::new();
static ABOUT_WINDOW: AtomicIsize = AtomicIsize::new(0);
static ABOUT_CREATING: AtomicBool = AtomicBool::new(false);
const SB_VERT_SCROLLBAR: i32 = 1;

fn window_dpi(hwnd: Hwnd) -> u32 {
    let dpi = GetDpiForWindow(hwnd);
    if dpi >= 48 { dpi } else { 96 }
}

fn scale_for(dpi: u32, value: i32) -> i32 {
    let scaled = (i64::from(value) * i64::from(dpi) + 48) / 96;
    i32::try_from(scaled).unwrap_or(i32::MAX)
}

fn clamp_minimum(desired: i32, work_area: i32, maximum: i32) -> i32 {
    let limit = if work_area > 0 {
        work_area
    } else if maximum > 0 {
        maximum
    } else {
        desired
    };
    desired.min(limit).max(1)
}

fn version_text(lang: Lang) -> String {
    format!(
        "{} · {}",
        tr(lang, "author_version"),
        updates::display_version()
    )
}

fn apply_fonts(state: &AboutState) {
    unsafe {
        for (hwnd, role) in &state.labels {
            let font = match role {
                FontRole::Title => state.theme.title,
                FontRole::Body => state.theme.body,
                FontRole::Small => state.theme.small,
                FontRole::Strong => state.theme.strong,
            };
            SendMessageW(*hwnd, WM_SETFONT, font as usize, 1);
        }
        SendMessageW(state.edit, WM_SETFONT, state.theme.mono as usize, 1);
    }
}

fn select_document(_window: Hwnd, state: &mut AboutState, index: usize) {
    let Some((name, text)) = DOCUMENTS.get(index) else {
        return;
    };
    state.selected_index = index;
    set_text(state.selected, name);
    set_text(state.edit, text);
    unsafe {
        SendMessageW(state.edit, EM_SETSEL, 0, 0);
        SendMessageW(state.edit, EM_SCROLLCARET, 0, 0);
        for button in &state.license_buttons {
            InvalidateRect(*button, ptr::null(), 0);
        }
    }
    refresh_edit_scrollbar(state);
}

fn about_state(hwnd: Hwnd) -> Option<&'static mut AboutState> {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AboutState;
    unsafe { pointer.as_mut() }
}

fn scroll_page(window: Hwnd, state: &mut AboutState, requested: i32) {
    let mut client = Rect::default();
    GetClientRect(window, &mut client);
    let footer_top = client.bottom - state.theme.sd(18) - state.theme.sd(34);
    let maximum = (state.content_height - footer_top).max(0);
    state.scroll = requested.clamp(0, maximum);
    layout(window);
}

#[allow(
    clippy::too_many_lines,
    reason = "One responsive layout table keeps all About regions aligned"
)]
fn layout(window: Hwnd) {
    let Some(state) = about_state(window) else {
        return;
    };
    let mut client = Rect::default();
    GetClientRect(window, &mut client);
    let margin = state.theme.sd(18);
    let header = state.theme.header_height();
    let width = (client.right - margin * 2).max(state.theme.sd(200));
    let caption_width = state.theme.sd(40);
    let caption_height = state.theme.sd(30);
    let close_x = client.right - state.theme.sd(18) - caption_width;
    MoveWindow(
        state.close,
        close_x,
        (header - caption_height) / 2,
        caption_width,
        caption_height,
        1,
    );

    let name_height = state.theme.sd(32);
    let role_height = state.theme.sd(20);
    let bio_height = dialog_ui::wrapped_height(&state.bio_text, state.theme.body, width);
    let project_height = dialog_ui::wrapped_height(&state.project_text, state.theme.small, width);
    let licenses_height = dialog_ui::wrapped_height(&state.licenses_text, state.theme.small, width);

    let name_top = header + state.theme.sd(14);
    let role_top = name_top + name_height;
    let bio_top = role_top + role_height + state.theme.sd(6);
    let project_top = bio_top + bio_height + state.theme.sd(10);
    let version_top = project_top + project_height + state.theme.sd(4);
    let licenses_top = version_top + state.theme.sd(20) + state.theme.sd(14);

    let columns: usize = if width >= state.theme.sd(700) { 1 } else { 2 };
    let column_count = i32::try_from(columns).unwrap_or(1);
    let gap = state.theme.sd(10);
    let row_height = state.theme.sd(26);
    let row_gap = state.theme.sd(4);
    let column_width = (width - gap * (column_count - 1)) / column_count;
    let rows = state.license_buttons.len().div_ceil(columns);
    let selector_top = licenses_top + licenses_height + state.theme.sd(8);
    let selector_height = i32::try_from(rows).unwrap_or(i32::MAX) * (row_height + row_gap);
    let selected_top = selector_top + selector_height + state.theme.sd(10);
    let edit_top = selected_top + state.theme.sd(22) + state.theme.sd(4);

    let footer_height = state.theme.sd(34);
    let footer_top = client.bottom - margin - footer_height;
    let natural_bottom = edit_top + state.theme.sd(EDIT_MIN_HEIGHT_DIP);
    let view_height = (footer_top - header).max(1);
    let scroll_max = (natural_bottom - footer_top).max(0);
    state.scroll = state.scroll.clamp(0, scroll_max);
    state.content_height = natural_bottom;
    let extra = (view_height - (natural_bottom - header)).max(0);
    let edit_height = state.theme.sd(EDIT_MIN_HEIGHT_DIP) + extra;
    let offset = |value: i32| value - header - state.scroll;
    MoveWindow(
        state.content,
        0,
        header,
        client.right.max(1),
        view_height.max(1),
        1,
    );

    MoveWindow(state.name, margin, offset(name_top), width, name_height, 1);
    MoveWindow(state.role, margin, offset(role_top), width, role_height, 1);
    MoveWindow(state.bio, margin, offset(bio_top), width, bio_height, 1);
    MoveWindow(
        state.project,
        margin,
        offset(project_top),
        width,
        project_height,
        1,
    );
    MoveWindow(
        state.version,
        margin,
        offset(version_top),
        width,
        state.theme.sd(20),
        1,
    );
    MoveWindow(
        state.licenses,
        margin,
        offset(licenses_top),
        width,
        licenses_height,
        1,
    );
    for (index, button) in state.license_buttons.iter().enumerate() {
        let row = i32::try_from(index / columns).unwrap_or(i32::MAX);
        let column = i32::try_from(index % columns).unwrap_or(i32::MAX);
        let x = margin + column * (column_width + gap);
        let top = selector_top + row * (row_height + row_gap);
        MoveWindow(*button, x, offset(top), column_width, row_height, 1);
    }
    MoveWindow(
        state.selected,
        margin,
        offset(selected_top),
        width,
        state.theme.sd(22),
        1,
    );
    MoveWindow(
        state.edit,
        margin,
        offset(selected_top + state.theme.sd(22) + state.theme.sd(4)),
        width,
        edit_height,
        1,
    );
    let footer_width = measure_button(&state.theme, &get_text(state.footer));
    let update_width = measure_button(&state.theme, tr(state.lang, "check_updates")).max(
        measure_button(&state.theme, tr(state.lang, "checking_updates")),
    );
    let update_width = update_width.min(
        (client.right - margin * 2 - footer_width - state.theme.sd(8)).max(state.theme.sd(80)),
    );
    MoveWindow(
        state.update_check,
        margin,
        footer_top,
        update_width,
        footer_height,
        1,
    );
    MoveWindow(
        state.footer,
        client.right - margin - footer_width,
        footer_top,
        footer_width,
        footer_height,
        1,
    );

    let info = ScrollInfo {
        size: u32::try_from(std::mem::size_of::<ScrollInfo>()).unwrap(),
        mask: SIF_ALL,
        minimum: 0,
        maximum: (natural_bottom - 1).max(0),
        page: u32::try_from(view_height).unwrap_or(u32::MAX),
        position: state.scroll,
        track: 0,
    };
    unsafe {
        SetScrollInfo(window, SB_VERT, &raw const info, 1);
        InvalidateRect(state.content, ptr::null(), 1);
    }
    let bar_width = state.theme.sd(10);
    let bar_top = header;
    let bar_height = (footer_top - bar_top).max(0);
    let show_bar = scroll_max > 0;
    ShowWindow(state.page_bar, if show_bar { SW_SHOW } else { 0 });
    MoveWindow(
        state.page_bar,
        (client.right - bar_width).max(0),
        bar_top,
        bar_width,
        bar_height,
        1,
    );
    unsafe {
        InvalidateRect(state.page_bar, ptr::null(), 0);
    }
    refresh_edit_scrollbar(state);
}

#[allow(
    clippy::too_many_lines,
    reason = "The window procedure keeps the About lifecycle in one ordered dispatch"
)]
unsafe extern "system" fn about_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    match message {
        WM_SIZE => {
            layout(hwnd);
            0
        }
        WM_PAINT => {
            if let Some(state) = about_state(hwnd) {
                unsafe {
                    let mut paint = PaintStruct::default();
                    let buffer = begin_window_paint(hwnd, &mut paint, c_body());
                    dialog_ui::paint_chrome(
                        hwnd,
                        buffer.memory,
                        &state.theme,
                        dialog_ui::HEADER_TITLE,
                        &state.title,
                        c_accent(),
                    );
                    finish_window_paint(hwnd, &paint, buffer);
                }
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
        WM_DRAWITEM => {
            let item = unsafe { &*(lparam as *const DrawItemStruct) };
            if let Some(state) = about_state(hwnd) {
                let id = usize::try_from(item.control_id).unwrap_or_default();
                if id == ID_CLOSE {
                    unsafe {
                        draw_owner_item_contents(hwnd, item);
                    }
                    return 1;
                }
                if id == IDOK {
                    unsafe {
                        dialog_ui::draw_button(item, ButtonKind::Primary, &state.theme, false);
                    }
                    return 1;
                }
                if (ID_LICENSE_BASE..ID_LICENSE_BASE + DOCUMENTS.len()).contains(&id) {
                    unsafe {
                        dialog_ui::draw_button(
                            item,
                            ButtonKind::ListItem,
                            &state.theme,
                            id - ID_LICENSE_BASE == state.selected_index,
                        );
                    }
                    return 1;
                }
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_COMMAND => {
            let id = wparam & 0xffff;
            if id == ID_UPDATE_CHECK {
                if let Some(state) = about_state(hwnd) {
                    state.checking_updates = true;
                    set_text(state.update_check, tr(state.lang, "checking_updates"));
                    updates::request_check(state.owner, true, state.lang);
                }
                return 0;
            }
            if id == ID_CLOSE || id == IDOK {
                DestroyWindow(hwnd);
                return 0;
            }
            if (ID_LICENSE_BASE..ID_LICENSE_BASE + DOCUMENTS.len()).contains(&id)
                && let Some(state) = about_state(hwnd)
            {
                select_document(hwnd, state, id - ID_LICENSE_BASE);
                return 0;
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_VSCROLL => {
            if let Some(state) = about_state(hwnd) {
                let code = i32::try_from(wparam & 0xffff).unwrap();
                let line = state.theme.sd(34);
                let mut client = Rect::default();
                GetClientRect(hwnd, &mut client);
                let page = (client.bottom - state.theme.sd(52)).max(line);
                let requested = match code {
                    SB_LINEUP => state.scroll - line,
                    SB_LINEDOWN => state.scroll + line,
                    SB_PAGEUP => state.scroll - page,
                    SB_PAGEDOWN => state.scroll + page,
                    SB_THUMBPOSITION | SB_THUMBTRACK => i32::from(
                        u16::try_from((wparam >> 16) & 0xffff)
                            .unwrap()
                            .cast_signed(),
                    ),
                    SB_TOP => 0,
                    SB_BOTTOM => i32::MAX,
                    _ => state.scroll,
                };
                scroll_page(hwnd, state, requested);
            }
            0
        }
        WM_MOUSEWHEEL_LOCAL => {
            if let Some(state) = about_state(hwnd) {
                let delta = i32::from(
                    u16::try_from((wparam >> 16) & 0xffff)
                        .unwrap()
                        .cast_signed(),
                );
                let requested = state.scroll - (delta * state.theme.sd(40)) / 120;
                scroll_page(hwnd, state, requested);
            }
            0
        }
        WM_NCCALCSIZE => unsafe { borderless_nccalcsize(hwnd, message, wparam, lparam) },
        WM_NCHITTEST => unsafe { borderless_hit_test(hwnd, message, wparam, lparam) },
        WM_NCLBUTTONDOWN => {
            if unsafe { begin_outline_resize(hwnd, wparam, lparam) } {
                0
            } else {
                unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
            }
        }
        WM_DPICHANGED => {
            let new_dpi = u32::try_from(wparam & 0xffff).unwrap();
            if let Some(state) = about_state(hwnd)
                && new_dpi >= 48
                && new_dpi != state.theme.dpi
            {
                let old = std::mem::replace(&mut state.theme, Theme::for_dpi(new_dpi));
                apply_fonts(state);
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
            layout(hwnd);
            0
        }
        WM_GETMINMAXINFO => {
            if lparam != 0 {
                let dimensions = unsafe { &mut *(lparam as *mut [i32; 10]) };
                let dpi =
                    about_state(hwnd).map_or_else(|| window_dpi(hwnd), |state| state.theme.dpi);
                dimensions[6] =
                    clamp_minimum(scale_for(dpi, MIN_WIDTH_DIP), dimensions[2], dimensions[8]);
                dimensions[7] =
                    clamp_minimum(scale_for(dpi, MIN_HEIGHT_DIP), dimensions[3], dimensions[9]);
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            let _ = ABOUT_WINDOW.compare_exchange(
                hwnd as isize,
                0,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AboutState;
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

fn register_class() {
    REGISTERED.get_or_init(|| {
        let class_name = wide("DVD_AUDIO_ABOUT_RUST");
        let registration = WndClass {
            style: 0x0002_0000,
            procedure: about_proc as WndProc,
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

fn create_license_edit(window: Hwnd) -> Hwnd {
    let instance = unsafe { GetModuleHandleW(ptr::null()) };
    let edit = unsafe {
        CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            ptr::null(),
            WS_CHILD
                | WS_VISIBLE
                | WS_VSCROLL
                | WS_CLIPSIBLINGS
                | WS_TABSTOP
                | ES_MULTILINE
                | ES_AUTOVSCROLL_LOCAL
                | ES_READONLY,
            0,
            0,
            10,
            10,
            window,
            ptr::null_mut(),
            instance,
            ptr::null_mut(),
        )
    };
    unsafe {
        SendMessageW(edit, EM_SETLIMITTEXT, 2_000_000, 0);
    }
    edit
}

/// Summary line listing every bundled licence actually offered by the selector.
fn license_summary(lang: Lang) -> String {
    let names: Vec<&str> = DOCUMENTS.iter().map(|(name, _)| *name).collect();
    let separator = match lang {
        Lang::Zh | Lang::ZhHant | Lang::Ja | Lang::Ko => "：",
        Lang::En => ": ",
    };
    format!(
        "{}{separator}{}",
        tr(lang, "author_licenses"),
        names.join(" · ")
    )
}

/// Live About window, used as the owner of manual update-check dialogs.
pub(crate) fn current_window() -> Option<Hwnd> {
    let window = ABOUT_WINDOW.load(Ordering::Acquire) as Hwnd;
    (!window.is_null() && IsWindow(window) != 0).then_some(window)
}

pub(crate) fn reset_update_check() {
    let window = ABOUT_WINDOW.load(Ordering::Acquire) as Hwnd;
    if window.is_null() || IsWindow(window) == 0 {
        return;
    }
    let Some(state) = about_state(window) else {
        return;
    };
    state.checking_updates = false;
    set_text(state.update_check, tr(state.lang, "check_updates"));
}

/// Applies the main window's dark scrollbar theme to the About page and its
/// licence reading pane; native light scrollbars would break the popup style.
fn apply_dark_scrollbars(window: Hwnd, edit: Hwnd) {
    let dark = super::theme::dark_theme();
    unsafe {
        super::theme::uxtheme_allow_dark(window, dark);
        super::theme::uxtheme_allow_dark(edit, dark);
        if dark {
            let dark_name = wide("DarkMode_Explorer");
            SetWindowTheme(window, dark_name.as_ptr(), ptr::null());
            SetWindowTheme(edit, dark_name.as_ptr(), ptr::null());
        } else {
            let light_name = wide("Explorer");
            SetWindowTheme(window, light_name.as_ptr(), ptr::null());
            SetWindowTheme(edit, light_name.as_ptr(), ptr::null());
        }
    }
}

fn edit_has_overflow(edit: Hwnd) -> bool {
    const WM_GETFONT: u32 = 0x0031;
    const EM_GETLINECOUNT: u32 = 0x00ba;
    const EM_LINEINDEX: u32 = 0x00bb;
    const EM_POSFROMCHAR: u32 = 0x00d6;
    const EM_LINELENGTH: u32 = 0x00c1;
    let mut lines = unsafe { SendMessageW(edit, EM_GETLINECOUNT, 0, 0) };
    while lines > 1 {
        let last = unsafe { SendMessageW(edit, EM_LINEINDEX, (lines - 1).cast_unsigned(), 0) };
        if last < 0 || unsafe { SendMessageW(edit, EM_LINELENGTH, last.cast_unsigned(), 0) } != 0 {
            break;
        }
        lines -= 1;
    }
    if lines < 1 {
        return false;
    }
    let last_start = unsafe { SendMessageW(edit, EM_LINEINDEX, (lines - 1).cast_unsigned(), 0) };
    if last_start < 0 {
        return false;
    }
    let last_y = unsafe { SendMessageW(edit, EM_POSFROMCHAR, last_start.cast_unsigned(), 0) } >> 16;
    let mut client = Rect::default();
    GetClientRect(edit, &mut client);
    let dc = CreateCompatibleDC(ptr::null_mut());
    if dc.is_null() {
        return false;
    }
    let font = unsafe { SendMessageW(edit, WM_GETFONT, 0, 0) } as Handle;
    let mut metrics = TextMetric::default();
    let old = if font.is_null() {
        ptr::null_mut()
    } else {
        SelectObject(dc, font)
    };
    GetTextMetricsW(dc, &mut metrics);
    if !old.is_null() {
        SelectObject(dc, old);
    }
    DeleteDC(dc);
    let text_height = metrics.height;
    if last_y < 0 {
        // EM_POSFROMCHAR only returns a position for a visible character; a
        // long document's last line is off-screen, so fall back to the line
        // count and measured line height.
        let second = unsafe { SendMessageW(edit, EM_LINEINDEX, 1, 0) };
        let first_y = unsafe { SendMessageW(edit, EM_POSFROMCHAR, 0, 0) } >> 16;
        let second_y =
            unsafe { SendMessageW(edit, EM_POSFROMCHAR, second.cast_unsigned(), 0) } >> 16;
        let line_height = i32::try_from(second_y - first_y).unwrap_or(0);
        return line_height > 0
            && lines > isize::try_from(client.bottom / line_height).unwrap_or(0);
    }
    let content_bottom = i32::try_from(last_y).unwrap_or(i32::MAX) + text_height.max(1);
    content_bottom > client.bottom
}

fn refresh_edit_scrollbar(state: &AboutState) {
    let overflow = edit_has_overflow(state.edit);
    unsafe {
        ShowScrollBar(state.edit, SB_VERT_SCROLLBAR, i32::from(overflow));
    }
}

fn apply_language(window: Hwnd, state: &mut AboutState, lang: Lang) {
    state.lang = lang;
    let title = tr(lang, "author_page_title").to_owned();
    state.title.clone_from(&title);
    unsafe {
        SetWindowTextW(window, wide(&title).as_ptr());
    }
    set_text(state.name, tr(lang, "author_name"));
    set_text(state.role, tr(lang, "author_role"));
    tr(lang, "author_bio").clone_into(&mut state.bio_text);
    set_text(state.bio, &state.bio_text);
    state.project_text = format!("{}\n{}", tr(lang, "project_home"), tr(lang, "project_url"));
    set_text(state.project, &state.project_text);
    set_text(state.version, &version_text(lang));
    set_text(
        state.update_check,
        tr(
            lang,
            if state.checking_updates {
                "checking_updates"
            } else {
                "check_updates"
            },
        ),
    );
    state.licenses_text = license_summary(lang);
    set_text(state.licenses, &state.licenses_text);
    set_text(state.selected, DOCUMENTS[state.selected_index].0);
    set_text(state.footer, tr(lang, "dialog_close"));
    apply_fonts(state);
    select_document(window, state, state.selected_index);
    layout(window);
}

fn focus_about(window: Hwnd) {
    ShowWindow(window, SW_SHOW);
    SetForegroundWindow(window);
    if let Some(state) = about_state(window)
        && let Some(first) = state.license_buttons.first()
    {
        SetFocus(*first);
    }
    invalidate_about(window);
}

fn invalidate_about(window: Hwnd) {
    unsafe {
        RedrawWindow(
            window,
            ptr::null(),
            ptr::null_mut(),
            0x0001 | 0x0004 | 0x0010 | 0x0085,
        );
    }
}

/// Re-applies theme-dependent native resources to a live About page and
/// repaints it, so switching the main window theme updates the page at once.
pub(crate) fn refresh_theme() {
    let window = ABOUT_WINDOW.load(Ordering::Acquire) as Hwnd;
    if window.is_null() || IsWindow(window) == 0 {
        return;
    }
    if let Some(state) = about_state(window) {
        apply_dark_scrollbars(window, state.edit);
        refresh_edit_scrollbar(state);
    }
    invalidate_about(window);
}

/// Updates a live About page after the main window language changes.
pub(crate) fn refresh_language(lang: Lang) {
    let window = ABOUT_WINDOW.load(Ordering::Acquire) as Hwnd;
    if window.is_null() || IsWindow(window) == 0 {
        return;
    }
    if let Some(state) = about_state(window)
        && state.lang != lang
    {
        apply_language(window, state, lang);
    }
}

/// Shows the About page owned by the parent window and returns the new window,
/// or null when creation failed.
#[allow(
    clippy::too_many_lines,
    reason = "Content assembly keeps the visible credits hierarchy in one place"
)]
pub(crate) fn show_about(parent: Hwnd, lang: Lang) -> Hwnd {
    register_class();
    let existing = ABOUT_WINDOW.load(Ordering::Acquire) as Hwnd;
    if !existing.is_null() && IsWindow(existing) != 0 {
        if let Some(state) = about_state(existing)
            && state.lang != lang
        {
            apply_language(existing, state, lang);
        }
        refresh_theme();
        focus_about(existing);
        return existing;
    }
    // A duplicate WM_COMMAND that re-enters during window creation must not
    // create a second page; the first creation publishes the handle below.
    if ABOUT_CREATING.swap(true, Ordering::AcqRel) {
        return ptr::null_mut();
    }
    let theme = Theme::new(parent);
    let instance = unsafe { GetModuleHandleW(ptr::null()) };
    let title = tr(lang, "author_page_title").to_owned();
    let title_wide = wide(&title);
    let width = theme.sd(940);
    let height = theme.sd(720);
    let window = unsafe {
        CreateWindowExW(
            0,
            wide("DVD_AUDIO_ABOUT_RUST").as_ptr(),
            title_wide.as_ptr(),
            WS_POPUP | WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_CLIPCHILDREN,
            0,
            0,
            width,
            height,
            parent,
            ptr::null_mut(),
            instance,
            ptr::null_mut(),
        )
    };
    if window.is_null() {
        theme.delete();
        ABOUT_CREATING.store(false, Ordering::Release);
        return window;
    }
    let content = create_content_host(window);
    let close = caption_button(window, ID_CLOSE);
    let mut labels: Vec<(Hwnd, FontRole)> = Vec::new();
    let mut label = |text: &str, role: FontRole| {
        let hwnd = create_label(content, text);
        labels.push((hwnd, role));
        hwnd
    };
    let name = label(tr(lang, "author_name"), FontRole::Title);
    let role = label(tr(lang, "author_role"), FontRole::Small);
    let bio_text = tr(lang, "author_bio").to_owned();
    let bio = label(&bio_text, FontRole::Body);
    let project_text = format!("{}\n{}", tr(lang, "project_home"), tr(lang, "project_url"));
    let project = label(&project_text, FontRole::Small);
    let version = label(&version_text(lang), FontRole::Small);
    let licenses_text = license_summary(lang);
    let licenses = label(&licenses_text, FontRole::Small);
    let selected = label(DOCUMENTS[0].0, FontRole::Strong);
    let mut license_buttons = Vec::with_capacity(DOCUMENTS.len());
    for (index, (name, _)) in DOCUMENTS.iter().enumerate() {
        license_buttons.push(create_button(
            content,
            name,
            ID_LICENSE_BASE + index,
            ButtonKind::ListItem,
        ));
    }
    let update_check = create_button(
        window,
        tr(lang, "check_updates"),
        ID_UPDATE_CHECK,
        ButtonKind::Primary,
    );
    let footer = create_button(window, tr(lang, "dialog_close"), IDOK, ButtonKind::Primary);
    let edit = create_license_edit(content);
    let page_bar = create_page_scrollbar(window);
    apply_dark_scrollbars(window, edit);
    let state = Box::new(AboutState {
        owner: parent,
        theme,
        lang,
        title,
        name,
        role,
        bio,
        project,
        version,
        licenses,
        selected,
        edit,
        close,
        update_check,
        checking_updates: false,
        footer,
        page_bar,
        content,
        license_buttons,
        labels,
        bio_text,
        project_text,
        licenses_text,
        selected_index: 0,
        scroll: 0,
        content_height: 0,
    });
    unsafe {
        SetWindowLongPtrW(window, GWLP_USERDATA, Box::into_raw(state) as isize);
        if let Some(state) = about_state(window) {
            apply_fonts(state);
            select_document(window, state, 0);
        }
        dialog_ui::center_on_owner(window, parent, width, height);
        layout(window);
        ShowWindow(window, SW_SHOW);
        SetForegroundWindow(window);
        if let Some(state) = about_state(window)
            && let Some(first) = state.license_buttons.first()
        {
            SetFocus(*first);
        }
        ABOUT_WINDOW.store(window as isize, Ordering::Release);
        ABOUT_CREATING.store(false, Ordering::Release);
    }
    window
}

#[cfg(test)]
mod tests {
    use super::super::gdi::window_rect;
    use super::super::{
        DispatchMessageW, GetWindowTextW, IsDialogMessageW, IsWindow, Lang, Message, VK_ESCAPE,
        VK_RETURN, WM_KEYDOWN, dialog_target, get_text, handle_keydown, ui_test_guard,
    };
    use super::*;

    const PM_REMOVE: u32 = 0x0001;
    const VK_TAB: usize = 0x09;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetScrollInfo(hwnd: Hwnd, bar: i32, info: *mut ScrollInfo) -> i32;
        fn PeekMessageW(
            message: *mut Message,
            hwnd: Hwnd,
            minimum: u32,
            maximum: u32,
            remove: u32,
        ) -> i32;
    }

    #[repr(C)]
    struct ScrollBarInfo {
        size: u32,
        scroll_bar: Rect,
        line_button: i32,
        thumb_top: i32,
        thumb_bottom: i32,
        reserved: i32,
        state: [u32; 6],
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetScrollBarInfo(hwnd: Hwnd, object: i32, info: *mut ScrollBarInfo) -> i32;
    }

    fn vertical_scrollbar_visible(hwnd: Hwnd) -> bool {
        const OBJID_VSCROLL: i32 = -5;
        const STATE_SYSTEM_INVISIBLE: u32 = 0x0000_8000;
        let mut info = ScrollBarInfo {
            size: u32::try_from(std::mem::size_of::<ScrollBarInfo>()).unwrap(),
            scroll_bar: Rect::default(),
            line_button: 0,
            thumb_top: 0,
            thumb_bottom: 0,
            reserved: 0,
            state: [0; 6],
        };
        unsafe {
            GetScrollBarInfo(hwnd, OBJID_VSCROLL, &raw mut info);
        }
        info.state[0] & STATE_SYSTEM_INVISIBLE == 0
    }

    fn px(dpi: u32, value: i32) -> i32 {
        i32::try_from((i64::from(value) * i64::from(dpi) + 48) / 96).unwrap_or(i32::MAX)
    }

    fn state(window: Hwnd) -> &'static AboutState {
        about_state(window).expect("about state")
    }

    fn dpi_message(window: Hwnd, dpi: u32) {
        let mut suggested = window_rect(window);
        let wparam = usize::try_from(dpi | (dpi << 16)).unwrap();
        unsafe {
            SendMessageW(window, WM_DPICHANGED, wparam, (&raw mut suggested) as isize);
        }
    }

    fn assert_children_inside(window: Hwnd) {
        let state = state(window);
        let bounds = window_rect(window);
        let mut controls = vec![
            state.name,
            state.role,
            state.bio,
            state.project,
            state.version,
            state.licenses,
            state.selected,
            state.edit,
            state.footer,
            state.close,
            state.update_check,
        ];
        controls.extend(state.license_buttons.iter().copied());
        for control in controls {
            let rect = window_rect(control);
            let fixed = GetParent(control) == window;
            assert!(
                rect.left >= bounds.left && rect.right <= bounds.right,
                "control {control:p} [{} {} {} {}] escapes about [{} {} {} {}] at dpi {:?}",
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                bounds.left,
                bounds.top,
                bounds.right,
                bounds.bottom,
                state.theme.dpi
            );
            if fixed {
                assert!(
                    rect.top >= bounds.top && rect.bottom <= bounds.bottom,
                    "fixed control {control:p} escapes about at dpi {:?}",
                    state.theme.dpi
                );
            }
        }
        assert!(
            window_rect(state.footer).bottom <= window_rect(state.edit).bottom.max(bounds.bottom)
        );
    }

    fn scroll_bar(window: Hwnd) -> ScrollInfo {
        let mut info = ScrollInfo {
            size: u32::try_from(std::mem::size_of::<ScrollInfo>()).unwrap(),
            mask: SIF_ALL,
            minimum: 0,
            maximum: 0,
            page: 0,
            position: 0,
            track: 0,
        };
        unsafe {
            GetScrollInfo(window, SB_VERT, &raw mut info);
        }
        info
    }

    #[test]
    fn all_licenses_are_embedded() {
        for (name, text) in DOCUMENTS {
            assert_ne!(name, "");
            assert!(text.len() > 100, "{name} was not embedded");
        }
    }

    #[test]
    fn license_summary_lists_every_selector_entry_in_five_languages() {
        let _ui_test_lock = ui_test_guard();
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja, Lang::Ko] {
            let summary = license_summary(lang);
            for (name, _) in DOCUMENTS {
                assert!(
                    summary.contains(name),
                    "{} summary misses {name}: {summary}",
                    lang.code()
                );
            }
            assert_eq!(summary.matches(" · ").count(), DOCUMENTS.len() - 1);
        }
    }

    #[test]
    fn about_hierarchy_matches_android_credits_in_five_languages() {
        let _ui_test_lock = ui_test_guard();
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja, Lang::Ko] {
            let window = show_about(ptr::null_mut(), lang);
            assert!(!window.is_null());
            let state = state(window);
            assert_eq!(get_text(state.name), tr(lang, "author_name"));
            assert_eq!(get_text(state.role), tr(lang, "author_role"));
            assert_eq!(get_text(state.bio), tr(lang, "author_bio"));
            assert_eq!(get_text(state.version), version_text(lang));
            let summary = get_text(state.licenses);
            assert!(
                summary.starts_with(tr(lang, "author_licenses")),
                "{summary}"
            );
            for (name, _) in DOCUMENTS {
                assert!(summary.contains(name), "{summary} misses {name}");
            }
            let project = get_text(state.project);
            assert!(project.contains(tr(lang, "project_home")), "{project}");
            assert!(project.contains(tr(lang, "project_url")), "{project}");
            assert_eq!(get_text(state.selected), DOCUMENTS[0].0);
            assert_eq!(get_text(state.edit), DOCUMENTS[0].1);
            assert_eq!(get_text(state.footer), tr(lang, "dialog_close"));
            DestroyWindow(window);
        }
    }

    #[test]
    fn about_license_selector_and_footer_keep_keyboard_focus_and_enter() {
        let _ui_test_lock = ui_test_guard();
        let window = show_about(ptr::null_mut(), Lang::En);
        assert!(!window.is_null());
        let state = state(window);
        for button in &state.license_buttons {
            let style = u32::try_from(GetWindowLongPtrW(*button, -16) & 0xffff_ffff).unwrap();
            assert!(style & WS_TABSTOP != 0, "license row is not tabbable");
            assert!(
                style & 0x0000_000b == 0x0000_000b,
                "license row is owner-drawn"
            );
        }
        let footer_style =
            u32::try_from(GetWindowLongPtrW(state.footer, -16) & 0xffff_ffff).unwrap();
        assert!(
            footer_style & WS_TABSTOP != 0,
            "close button is not tabbable"
        );
        let close_style = u32::try_from(GetWindowLongPtrW(state.close, -16) & 0xffff_ffff).unwrap();
        assert!(
            close_style & WS_TABSTOP == 0,
            "caption close must not join the tab chain"
        );
        assert_eq!(
            dialog_target(ptr::null_mut(), state.license_buttons[0]),
            window
        );
        assert_eq!(dialog_target(state.bio, state.license_buttons[0]), window);

        let second = state.license_buttons[1];
        let enter = Message {
            hwnd: second,
            id: WM_KEYDOWN,
            wparam: VK_RETURN,
            lparam: 0,
            time: 0,
            point_x: 0,
            point_y: 0,
        };
        assert!(handle_keydown(&enter, ptr::null_mut(), second));
        assert_eq!(get_text(state.edit), DOCUMENTS[1].1);
        assert_eq!(get_text(state.selected), DOCUMENTS[1].0);
        assert_eq!(state.selected_index, 1);

        let escape = Message {
            wparam: VK_ESCAPE,
            ..enter
        };
        assert!(handle_keydown(&escape, ptr::null_mut(), second));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut closed = false;
        loop {
            let mut queued = Message {
                hwnd: ptr::null_mut(),
                id: 0,
                wparam: 0,
                lparam: 0,
                time: 0,
                point_x: 0,
                point_y: 0,
            };
            if unsafe { PeekMessageW(&raw mut queued, window, 0, 0, PM_REMOVE) } != 0 {
                if queued.id == WM_CLOSE {
                    unsafe {
                        DispatchMessageW(&raw const queued);
                    }
                    closed = true;
                    break;
                }
                unsafe {
                    DispatchMessageW(&raw const queued);
                }
                continue;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(closed, "Escape did not close the about page");
        assert_eq!(IsWindow(window), 0);
    }

    #[test]
    fn about_reflows_at_five_dpis_and_five_languages() {
        let _ui_test_lock = ui_test_guard();
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja, Lang::Ko] {
            let window = show_about(ptr::null_mut(), lang);
            assert!(!window.is_null());
            assert_eq!(
                window_title(window),
                tr(lang, "author_page_title"),
                "{}",
                lang.code()
            );
            for dpi in [96u32, 120, 144, 168, 192] {
                dpi_message(window, dpi);
                for width_dip in [640, 900] {
                    MoveWindow(window, 0, 0, px(dpi, width_dip), px(dpi, 680), 0);
                    assert_children_inside(window);
                    let state = state(window);
                    assert_eq!(get_text(state.update_check), tr(lang, "check_updates"));
                    assert!(get_text(state.version).contains(&updates::display_version()));
                    let check = window_rect(state.update_check);
                    let close = window_rect(state.footer);
                    assert!(check.right <= close.left);
                    assert!(
                        check.right - check.left
                            >= measure_button(&state.theme, tr(lang, "check_updates"),)
                    );
                }
            }
            DestroyWindow(window);
        }
    }

    #[test]
    fn about_uses_two_selector_rows_at_640_and_one_column_when_wide() {
        let _ui_test_lock = ui_test_guard();
        let window = show_about(ptr::null_mut(), Lang::En);
        MoveWindow(window, 0, 0, 640, 680, 0);
        let state = state(window);
        let rows: std::collections::BTreeSet<i32> = state
            .license_buttons
            .iter()
            .map(|button| window_rect(*button).top)
            .collect();
        assert_eq!(rows.len(), 3, "640-DIP selector should use three rows");
        MoveWindow(window, 0, 0, 900, 680, 0);
        let rows: std::collections::BTreeSet<i32> = state
            .license_buttons
            .iter()
            .map(|button| window_rect(*button).top)
            .collect();
        assert_eq!(rows.len(), 6, "wide selector should be one column");
        DestroyWindow(window);
    }

    #[test]
    fn short_about_windows_scroll_and_keep_the_footer_inside() {
        let _ui_test_lock = ui_test_guard();
        let window = show_about(ptr::null_mut(), Lang::En);
        MoveWindow(window, 0, 0, 640, 380, 0);
        let state = state(window);
        let before = scroll_bar(window);
        assert!(
            before.maximum >= i32::try_from(before.page).unwrap_or(i32::MAX),
            "{:?}",
            before.maximum
        );
        assert!(state.content_height > 380);
        let edit_before = window_rect(state.edit).top;
        unsafe {
            SendMessageW(window, WM_VSCROLL, SB_BOTTOM as usize, 0);
        }
        assert!(state.scroll > 0);
        assert!(window_rect(state.edit).top < edit_before);
        let window_bounds = window_rect(window);
        let footer = window_rect(state.footer);
        assert!(footer.bottom <= window_bounds.bottom);
        assert!(footer.top >= window_bounds.top);
        DestroyWindow(window);
    }

    #[test]
    fn scrollable_content_is_clipped_to_the_content_host_below_the_header() {
        let _ui_test_lock = ui_test_guard();
        let window = show_about(ptr::null_mut(), Lang::En);
        MoveWindow(window, 0, 0, 640, 520, 0);
        let state = state(window);
        assert!(!state.content.is_null());
        assert_eq!(GetParent(state.bio), state.content);
        assert_eq!(GetParent(state.edit), state.content);
        assert_eq!(GetParent(state.footer), window);
        let content = window_rect(state.content);
        let window_bounds = window_rect(window);
        assert!(content.top > window_bounds.top);
        assert!(content.bottom <= window_bounds.bottom);
        unsafe {
            SendMessageW(window, WM_VSCROLL, SB_BOTTOM as usize, 0);
        }
        assert!(state.scroll > 0);
        // Children scroll inside the host; the host clips them below the fixed
        // header and above the fixed footer.
        assert_eq!(GetParent(state.bio), state.content);
        assert_eq!(GetParent(state.edit), state.content);
        // WS_EX_CONTROLPARENT keeps the dialog manager's tab traversal inside
        // the nested content host.
        let first = state.license_buttons[0];
        let second = state.license_buttons[1];
        SetFocus(first);
        let mut tab = Message {
            hwnd: first,
            id: WM_KEYDOWN,
            wparam: VK_TAB,
            lparam: 0,
            time: 0,
            point_x: 0,
            point_y: 0,
        };
        assert_ne!(unsafe { IsDialogMessageW(window, &raw mut tab) }, 0);
        assert_eq!(GetFocus(), second);
        DestroyWindow(window);
    }

    #[test]
    fn about_stays_singleton_and_reopens_after_being_closed() {
        let _ui_test_lock = ui_test_guard();
        let first = show_about(ptr::null_mut(), Lang::En);
        assert!(!first.is_null());
        let again = show_about(ptr::null_mut(), Lang::En);
        assert_eq!(
            again, first,
            "duplicate About command must reuse the window"
        );
        DestroyWindow(first);
        assert_eq!(IsWindow(first), 0);
        let reopened = show_about(ptr::null_mut(), Lang::En);
        assert!(!reopened.is_null());
        assert_ne!(reopened, first);
        DestroyWindow(reopened);
    }

    #[test]
    fn page_and_reading_scrollbars_only_show_while_overflowing() {
        let _ui_test_lock = ui_test_guard();
        let window = show_about(ptr::null_mut(), Lang::En);
        assert!(!window.is_null());
        MoveWindow(window, 0, 0, 640, 520, 0);
        let state = state(window);
        assert!(IsWindowVisible(state.page_bar) != 0);
        assert!(vertical_scrollbar_visible(state.edit));
        MoveWindow(window, 0, 0, 940, 1400, 0);
        assert_eq!(IsWindowVisible(state.page_bar), 0);
        set_text(state.edit, "short");
        layout(window);
        assert!(!vertical_scrollbar_visible(state.edit));
        set_text(state.edit, DOCUMENTS[0].1);
        layout(window);
        assert!(vertical_scrollbar_visible(state.edit));
        DestroyWindow(window);
    }

    #[test]
    fn open_about_follows_main_language_without_recreating_the_window() {
        let _ui_test_lock = ui_test_guard();
        let window = show_about(ptr::null_mut(), Lang::En);
        assert!(!window.is_null());
        refresh_language(Lang::Zh);
        assert!(state(window).lang == Lang::Zh);
        assert_eq!(get_text(state(window).footer), tr(Lang::Zh, "dialog_close"));
        assert_eq!(show_about(ptr::null_mut(), Lang::Zh), window);
        DestroyWindow(window);
    }

    #[test]
    fn about_caption_close_reuses_main_window_close() {
        let _ui_test_lock = ui_test_guard();
        let window = show_about(ptr::null_mut(), Lang::En);
        assert!(!window.is_null());
        let state = state(window);
        assert_eq!(GetDlgCtrlID(state.close), i32::try_from(ID_CLOSE).unwrap());
        let close_style = u32::try_from(GetWindowLongPtrW(state.close, -16) & 0xffff_ffff).unwrap();
        assert_eq!(close_style & WS_TABSTOP, 0);
        assert_eq!(close_style & 0x0000_000b, 0x0000_000b);
        let close = window_rect(state.close);
        assert_eq!(close.bottom - close.top, state.theme.sd(30));
        unsafe {
            SendMessageW(window, WM_COMMAND, ID_CLOSE, 0);
        }
        assert_eq!(IsWindow(window), 0);
    }

    #[test]
    fn trailing_newlines_do_not_show_the_reading_scrollbar() {
        let _ui_test_lock = ui_test_guard();
        let window = show_about(ptr::null_mut(), Lang::En);
        MoveWindow(window, 0, 0, 640, 520, 0);
        let state = state(window);
        let text = format!("one visible line{}", "\n".repeat(40));
        set_text(state.edit, &text);
        layout(window);
        assert!(!vertical_scrollbar_visible(state.edit));
        DestroyWindow(window);
    }

    #[test]
    fn about_uses_the_main_window_resize_frame() {
        let _ui_test_lock = ui_test_guard();
        let window = show_about(ptr::null_mut(), Lang::En);
        assert!(!window.is_null());
        let style = u32::try_from(GetWindowLongPtrW(window, -16) & 0xffff_ffff).unwrap();
        assert_ne!(style & WS_THICKFRAME, 0, "About is not resizable");
        let bounds = window_rect(window);
        let right_hit = unsafe {
            SendMessageW(
                window,
                WM_NCHITTEST,
                0,
                isize::try_from(
                    ((bounds.top + (bounds.bottom - bounds.top) / 2) << 16)
                        | ((bounds.right - 2) & 0xffff),
                )
                .unwrap(),
            )
        };
        assert_eq!(right_hit, HTRIGHT as Lresult);
        let corner_hit = unsafe {
            SendMessageW(
                window,
                WM_NCHITTEST,
                0,
                isize::try_from(((bounds.bottom - 2) << 16) | ((bounds.right - 2) & 0xffff))
                    .unwrap(),
            )
        };
        assert_eq!(corner_hit, HTBOTTOMRIGHT as Lresult);
        let left_hit = unsafe {
            SendMessageW(
                window,
                WM_NCHITTEST,
                0,
                isize::try_from(
                    ((bounds.top + (bounds.bottom - bounds.top) / 2) << 16)
                        | ((bounds.left + 2) & 0xffff),
                )
                .unwrap(),
            )
        };
        assert_eq!(left_hit, HTLEFT as Lresult);
        let content = state(window).content;
        let content_right = unsafe {
            SendMessageW(
                content,
                WM_NCHITTEST,
                0,
                isize::try_from(((bounds.top + 200) << 16) | ((bounds.right - 2) & 0xffff))
                    .unwrap(),
            )
        };
        assert_eq!(content_right, HTRIGHT as Lresult);
        let content_left = unsafe {
            SendMessageW(
                content,
                WM_NCHITTEST,
                0,
                isize::try_from(((bounds.top + 200) << 16) | ((bounds.left + 2) & 0xffff)).unwrap(),
            )
        };
        assert_eq!(content_left, HTLEFT as Lresult);
        DestroyWindow(window);
    }

    #[test]
    fn about_minimum_size_is_clamped_to_the_work_area() {
        let _ui_test_lock = ui_test_guard();
        let window = show_about(ptr::null_mut(), Lang::En);
        assert!(!window.is_null());
        assert_eq!(min_track_size(window, 800, 600, 1920, 1080), (640, 520));
        assert_eq!(min_track_size(window, 900, 300, 1920, 1080), (640, 300));
        assert_eq!(min_track_size(window, 0, 0, 700, 350), (640, 350));
        DestroyWindow(window);
    }

    fn min_track_size(
        window: Hwnd,
        work_width: i32,
        work_height: i32,
        max_width: i32,
        max_height: i32,
    ) -> (i32, i32) {
        let mut dimensions = [0i32; 10];
        dimensions[2] = work_width;
        dimensions[3] = work_height;
        dimensions[8] = max_width;
        dimensions[9] = max_height;
        unsafe {
            SendMessageW(window, WM_GETMINMAXINFO, 0, (&raw mut dimensions) as isize);
        }
        (dimensions[6], dimensions[7])
    }

    fn window_title(window: Hwnd) -> String {
        let mut buffer = [0u16; 256];
        let length = unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), 256) };
        let length = usize::try_from(length).unwrap_or_default();
        String::from_utf16_lossy(&buffer[..length.min(buffer.len())])
    }
}
