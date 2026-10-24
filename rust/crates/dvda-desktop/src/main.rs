#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod presentation;
mod touch;

use dvda_core::localization;
include!(concat!(env!("OUT_DIR"), "/runtime.rs"));

use dvda_core::app::AppOptions;
use dvda_native::media::Callbacks;
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    ffi::{OsStr, c_void},
    fs,
    io::Write,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    thread,
    time::{Duration, Instant},
};

type Handle = *mut c_void;
type Hwnd = Handle;
type Hinstance = Handle;
type Hmenu = Handle;
type Hbrush = Handle;
type Lparam = isize;
type Wparam = usize;
type Lresult = isize;
type WndProc = unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam) -> Lresult;
type EnumChildProc = unsafe extern "system" fn(Hwnd, Lparam) -> i32;

const WM_CREATE: u32 = 0x0001;
const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_GETMINMAXINFO: u32 = 0x0024;
const WM_CLOSE: u32 = 0x0010;
const WM_SETREDRAW: u32 = 0x000b;
const WM_DRAWITEM: u32 = 0x002b;
const WM_MEASUREITEM: u32 = 0x002c;
const WM_SETFOCUS: u32 = 0x0007;
const WM_KILLFOCUS: u32 = 0x0008;
const WM_TIMER: u32 = 0x0113;
const WM_DPICHANGED: u32 = 0x02e0;
const WM_SETTINGCHANGE: u32 = 0x001a;
const WM_KEYDOWN: u32 = 0x0100;
const WM_PAINT: u32 = 0x000f;
const WM_ERASEBKGND: u32 = 0x0014;
const ID_ADVANCED: usize = 117;
const WM_COMMAND: u32 = 0x0111;
const WM_NOTIFY: u32 = 0x004e;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_CTLCOLORLISTBOX: u32 = 0x0134;
const WM_CTLCOLORBTN: u32 = 0x0135;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_MOUSELEAVE: u32 = 0x02a3;
const WM_NCDESTROY: u32 = 0x0082;
const WM_MOUSEMOVE: u32 = 0x0200;
const VK_RETURN: usize = 0x0d;
const PBM_SETBARCOLOR: u32 = 0x0409;
const PBM_SETBKCOLOR: u32 = 0x2001;
const CB_GETDROPPEDSTATE: u32 = 0x0157;
const TME_LEAVE: u32 = 0x0002;
const LOG_QUEUE_CAPACITY: usize = 4096;
const LOG_BATCH_LIMIT: usize = 4096;
const LOG_BATCH_BUDGET: Duration = Duration::from_millis(12);
const WM_SETFONT: u32 = 0x0030;
const EM_SETLIMITTEXT: u32 = 0x00c5;
const EM_SETMARGINS: u32 = 0x00d3;
const EM_GETFIRSTVISIBLELINE: u32 = 0x00ce;
const EM_GETLINECOUNT: u32 = 0x00ba;
const WM_VSCROLL: u32 = 0x0115;
const SB_BOTTOM: usize = 7;
const EM_LINESCROLL: u32 = 0x00b6;
const EM_GETSEL: u32 = 0x00b0;
const EM_SETSEL: u32 = 0x00b1;
const EM_REPLACESEL: u32 = 0x00c2;
const EM_SETREADONLY: u32 = 0x00cf;
const BM_GETCHECK: u32 = 0x00f0;
const BM_SETCHECK: u32 = 0x00f1;
const CB_ADDSTRING: u32 = 0x0143;
const CB_GETCURSEL: u32 = 0x0147;
const CB_RESETCONTENT: u32 = 0x014b;
const CB_SETCURSEL: u32 = 0x014e;
const CB_GETLBTEXT: u32 = 0x0148;
const CB_GETLBTEXTLEN: u32 = 0x0149;
const CB_SHOWDROPDOWN: u32 = 0x014f;
const PBM_SETPOS: u32 = 0x0402;
const PBM_SETRANGE32: u32 = 0x0406;
const WS_THICKFRAME: u32 = 0x00040000;
const WS_CAPTION: u32 = 0x00c00000;
const WS_MINIMIZEBOX: u32 = 0x00020000;
const WS_MAXIMIZEBOX: u32 = 0x00010000;
const WS_SYSMENU: u32 = 0x00080000;
const WS_CHILD: u32 = 0x40000000;
const WS_VISIBLE: u32 = 0x10000000;
const WS_TABSTOP: u32 = 0x00010000;
const WS_CLIPCHILDREN: u32 = 0x02000000;
const WS_POPUP: u32 = 0x80000000;
const WS_EX_TOPMOST: u32 = 0x00000008;
const WS_EX_TRANSPARENT: u32 = 0x00000020;
const WS_EX_TOOLWINDOW: u32 = 0x00000080;
const WS_EX_NOACTIVATE: u32 = 0x08000000;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_SHOWWINDOW: u32 = 0x0040;
const WS_EX_APPWINDOW: u32 = 0x00040000;
const ES_AUTOHSCROLL: u32 = 0x0080;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const ES_READONLY: u32 = 0x0800;
const ES_WANTRETURN: u32 = 0x1000;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const CBS_OWNERDRAWFIXED: u32 = 0x0010;
const CBS_HASSTRINGS: u32 = 0x0200;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const BS_OWNERDRAW: u32 = 0x0000000b;
const ODS_SELECTED: u32 = 0x0001;
const ODS_DISABLED: u32 = 0x0004;
const ODS_FOCUS: u32 = 0x0010;
const DT_CENTER: u32 = 0x00000001;
const DT_VCENTER: u32 = 0x00000004;
const DT_SINGLELINE: u32 = 0x00000020;
const DT_CALCRECT: u32 = 0x00000400;
const DT_LEFT: u32 = 0x00000000;
const DT_TOP: u32 = 0x00000000;
const DT_NOPREFIX: u32 = 0x00000800;
const DT_END_ELLIPSIS: u32 = 0x00008000;
const RDW_INVALIDATE: u32 = 0x0001;
const RDW_ALLCHILDREN: u32 = 0x0080;
const SW_SHOW: i32 = 5;
const SW_HIDE: i32 = 0;
const SW_MAXIMIZE: i32 = 3;
const SW_MINIMIZE: i32 = 6;
const SW_RESTORE: i32 = 9;
const WM_NCCALCSIZE: u32 = 0x0083;
const WM_NCHITTEST: u32 = 0x0084;
const WM_NCLBUTTONDOWN: u32 = 0x00a1;
const WM_NCLBUTTONDBLCLK: u32 = 0x00a3;
const HTCLIENT: i32 = 1;
const HTCAPTION: i32 = 2;
const HTLEFT: i32 = 10;
const HTRIGHT: i32 = 11;
const HTTOP: i32 = 12;
const HTTOPLEFT: i32 = 13;
const HTTOPRIGHT: i32 = 14;
const HTBOTTOM: i32 = 15;
const HTBOTTOMLEFT: i32 = 16;
const HTBOTTOMRIGHT: i32 = 17;
const GWLP_USERDATA: i32 = -21;
const GWLP_WNDPROC: i32 = -4;
const BN_CLICKED: u16 = 0;
const CBN_SELCHANGE: u16 = 1;
const ID_PROFILE: usize = 100;
const ID_OPEN_PROFILE: usize = 101;
const ID_SAVE_PROFILE: usize = 102;
const ID_LANGUAGE: usize = 103;
const ID_TAB: usize = 2000;
const ID_PREPARE: usize = 105;
const ID_BUILD: usize = 106;
const ID_VERIFY: usize = 107;
const ID_CANCEL: usize = 108;
const ID_OPEN_OUTPUT: usize = 109;
const ID_LOG_VIEW: usize = 110;
const ID_ONLY_ISSUES: usize = 111;
const ID_LIVE: usize = 112;
const ID_COPY: usize = 113;
const ID_EXPORT: usize = 114;
const ID_LOG: usize = 115;
const ID_SAVE_AS: usize = 116;
const ID_MINIMIZE: usize = 6000;
const ID_MAXIMIZE: usize = 6001;
const ID_CLOSE: usize = 6002;
const ID_THEME: usize = 6003;
const ID_SOURCE: usize = 200;
const ID_FINAL: usize = 201;
const ID_TITLE: usize = 202;
const ID_CAPACITY: usize = 203;
const ID_WORK_DIR: usize = 205;
const ID_ISO_PREFIX: usize = 206;
const ID_GROUP_LIMIT: usize = 207;
const ID_CUSTOM_BYTES: usize = 210;
const ID_MODE: usize = 300;
const ID_RATE: usize = 301;
const ID_BITS: usize = 302;
const ID_IMPORT_FOLDER: usize = 305;
const ID_MENU: usize = 400;
const ID_STILLS: usize = 401;
const ID_TRACKS: usize = 402;
const ID_COVER: usize = 403;
const ID_FONT_SC: usize = 405;
const ID_FONT_JP: usize = 406;
const ID_FONT_KR: usize = 407;
const ID_KEEP_FILES: usize = 506;
const ID_BROWSE_SOURCE: usize = 1000;
const ID_BROWSE_FINAL: usize = 1001;
const ID_BROWSE_WORK: usize = 1002;
const ID_BROWSE_IMPORT: usize = 1006;
const COMBO_IDS: &[usize] = &[
    ID_LANGUAGE,
    ID_LOG_VIEW,
    ID_CAPACITY,
    ID_MODE,
    ID_RATE,
    ID_BITS,
];

// Visual language follows the application icon: deep navy, cool silver and a
// cyan waveform accent. COLORREF is 0x00BBGGRR, so rgb() converts from the
// familiar 0xRRGGBB order.
const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    (b << 16) | (g << 8) | r
}
fn c_header_top() -> u32 {
    if dark_theme() {
        rgb(7, 11, 18)
    } else {
        rgb(10, 23, 39)
    }
}
fn c_header_bottom() -> u32 {
    if dark_theme() {
        rgb(17, 32, 49)
    } else {
        rgb(21, 51, 82)
    }
}
fn c_accent() -> u32 {
    rgb(88, 205, 224)
}
fn c_accent_hover() -> u32 {
    rgb(120, 219, 235)
}
fn c_accent_soft() -> u32 {
    if dark_theme() {
        rgb(30, 58, 68)
    } else {
        rgb(224, 245, 249)
    }
}
fn c_accent_pressed() -> u32 {
    rgb(56, 172, 192)
}
fn c_body() -> u32 {
    if dark_theme() {
        rgb(13, 19, 27)
    } else {
        rgb(243, 246, 250)
    }
}
fn c_panel() -> u32 {
    if dark_theme() {
        rgb(19, 27, 38)
    } else {
        rgb(240, 245, 251)
    }
}
fn c_card() -> u32 {
    if dark_theme() {
        rgb(26, 35, 47)
    } else {
        rgb(255, 255, 255)
    }
}
fn c_secondary_hover() -> u32 {
    if dark_theme() {
        rgb(32, 43, 57)
    } else {
        rgb(244, 248, 252)
    }
}
fn c_secondary_pressed() -> u32 {
    if dark_theme() {
        rgb(39, 52, 68)
    } else {
        rgb(233, 240, 248)
    }
}
fn c_border() -> u32 {
    if dark_theme() {
        rgb(45, 58, 74)
    } else {
        rgb(214, 223, 233)
    }
}
fn c_border_soft() -> u32 {
    if dark_theme() {
        rgb(33, 44, 57)
    } else {
        rgb(228, 235, 242)
    }
}
fn c_ink() -> u32 {
    if dark_theme() {
        rgb(230, 238, 247)
    } else {
        rgb(24, 40, 58)
    }
}
fn c_muted() -> u32 {
    if dark_theme() {
        rgb(139, 156, 176)
    } else {
        rgb(99, 116, 138)
    }
}
fn c_header_title() -> u32 {
    rgb(240, 247, 252)
}
fn c_header_subtitle() -> u32 {
    if dark_theme() {
        rgb(143, 166, 191)
    } else {
        rgb(160, 184, 208)
    }
}
fn c_danger() -> u32 {
    if dark_theme() {
        rgb(255, 138, 133)
    } else {
        rgb(180, 35, 24)
    }
}
fn c_danger_hover() -> u32 {
    if dark_theme() {
        rgb(58, 29, 30)
    } else {
        rgb(253, 232, 232)
    }
}
fn c_danger_pressed() -> u32 {
    if dark_theme() {
        rgb(74, 38, 39)
    } else {
        rgb(248, 214, 214)
    }
}
fn c_danger_border() -> u32 {
    if dark_theme() {
        rgb(107, 52, 52)
    } else {
        rgb(228, 184, 184)
    }
}
fn c_disabled_fill() -> u32 {
    if dark_theme() {
        rgb(35, 44, 57)
    } else {
        rgb(226, 233, 240)
    }
}
fn c_disabled_text() -> u32 {
    if dark_theme() {
        rgb(110, 124, 142)
    } else {
        rgb(146, 160, 175)
    }
}
fn c_bar_track() -> u32 {
    if dark_theme() {
        rgb(29, 39, 51)
    } else {
        rgb(228, 235, 243)
    }
}
const HEADER_HEIGHT: i32 = 70;
const UI_FONT_HEIGHT: i32 = -16;

#[repr(C)]
struct PaintStruct {
    dc: Handle,
    erase: i32,
    paint: Rect,
    restore: i32,
    inc_update: i32,
    reserved: [u8; 32],
}

#[repr(C)]
struct TriVertex {
    x: i32,
    y: i32,
    red: u16,
    green: u16,
    blue: u16,
    alpha: u16,
}

#[repr(C)]
struct GradientRect {
    upper_left: u32,
    lower_right: u32,
}

#[repr(C)]
struct TrackMouseEventData {
    size: u32,
    flags: u32,
    track: Hwnd,
    hover_time: u32,
}

struct ButtonVisual {
    hover: bool,
    original: WndProc,
}

struct CheckVisual {
    hover: bool,
    panel: bool,
    original: WndProc,
}

struct ComboArrowVisual {
    hover: bool,
    combo: Hwnd,
    original: WndProc,
    last_index: i32,
    last_enabled: bool,
}

struct ScrollbarVisual {
    main: Hwnd,
    hover: bool,
    dragging: bool,
    drag_start_cursor: i32,
    drag_start_value: i32,
    last_value: i32,
    last_max: i32,
    last_page: i32,
}

#[repr(C)]
struct MeasureItemStruct {
    control_type: u32,
    control_id: u32,
    item_id: u32,
    item_width: u32,
    item_height: u32,
    item_data: usize,
}


#[repr(C)]
struct WndClass {
    style: u32,
    procedure: WndProc,
    extra_class: i32,
    extra_window: i32,
    instance: Hinstance,
    icon: Handle,
    cursor: Handle,
    background: Hbrush,
    menu: *const u16,
    class_name: *const u16,
}

#[repr(C)]
struct Message {
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
    time: u32,
    point_x: i32,
    point_y: i32,
}

/// WINDOWPLACEMENT
#[repr(C)]
struct WindowPlacement {
    length: u32,
    flags: u32,
    show: u32,
    minimized: Point,
    maximized: Point,
    normal: Rect,
}

/// MONITORINFO
#[repr(C)]
struct MonitorInfo {
    size: u32,
    monitor: Rect,
    work: Rect,
    flags: u32,
}

/// TEXTMETRICW
#[repr(C)]
struct TextMetric {
    height: i32,
    ascent: i32,
    descent: i32,
    internal_leading: i32,
    external_leading: i32,
    average_width: i32,
    maximum_width: i32,
    weight: i32,
    overhang: i32,
    aspect_x: i32,
    aspect_y: i32,
    first_char: u16,
    last_char: u16,
    default_char: u16,
    break_char: u16,
    italic: u8,
    underlined: u8,
    struck_out: u8,
    pitch_and_family: u8,
    char_set: u8,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct DrawItemStruct {
    control_type: u32,
    control_id: u32,
    item_id: u32,
    action: u32,
    state: u32,
    item: Hwnd,
    dc: Handle,
    rect: Rect,
    data: usize,
}


#[repr(C)]
struct ToolInfo {
    size: u32,
    flags: u32,
    owner: Hwnd,
    id: usize,
    rect: Rect,
    instance: Hinstance,
    text: *mut u16,
    param: isize,
    reserved: *mut c_void,
}

struct Tooltip {
    control: Hwnd,
    key: &'static str,
    text: Vec<u16>,
}

#[repr(C)]
struct InitCommonControlsEx {
    size: u32,
    classes: u32,
}

#[repr(C)]
struct BrowseInfo {
    owner: Hwnd,
    root: Handle,
    display_name: *mut u16,
    title: *const u16,
    flags: u32,
    callback: Option<unsafe extern "system" fn(Hwnd, u32, Lparam, Lparam) -> i32>,
    param: Lparam,
    image: i32,
}

#[repr(C)]
struct OpenFileName {
    size: u32,
    owner: Hwnd,
    instance: Hinstance,
    filter: *const u16,
    custom_filter: *mut u16,
    custom_filter_max: u32,
    filter_index: u32,
    file: *mut u16,
    file_max: u32,
    file_title: *mut u16,
    file_title_max: u32,
    initial_dir: *const u16,
    title: *const u16,
    flags: u32,
    file_offset: u16,
    file_extension: u16,
    def_ext: *const u16,
    data: isize,
    hook: Handle,
    template_name: *const u16,
    reserved: *mut c_void,
    reserved2: u32,
    flags_ex: u32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassW(class: *const WndClass) -> u16;
    fn LoadIconW(instance: Hinstance, name: *const u16) -> Handle;
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: Hmenu,
        instance: Hinstance,
        parameter: *mut c_void,
    ) -> Hwnd;
    fn DefWindowProcW(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
    fn GetDlgCtrlID(hwnd: Hwnd) -> i32;
    fn GetDlgItem(parent: Hwnd, id: i32) -> Hwnd;
    fn GetParent(hwnd: Hwnd) -> Hwnd;
    fn GetClassNameW(hwnd: Hwnd, text: *mut u16, length: i32) -> i32;
    fn ShowWindow(hwnd: Hwnd, command: i32) -> i32;
    fn UpdateWindow(hwnd: Hwnd) -> i32;
    fn IsWindowVisible(hwnd: Hwnd) -> i32;
    fn ScrollWindowEx(
        hwnd: Hwnd,
        dx: i32,
        dy: i32,
        scroll: *const Rect,
        clip: *const Rect,
        update_region: Handle,
        update: *mut Rect,
        flags: u32,
    ) -> i32;
    fn MoveWindow(hwnd: Hwnd, x: i32, y: i32, width: i32, height: i32, repaint: i32) -> i32;
    fn RedrawWindow(hwnd: Hwnd, update: *const Rect, region: Handle, flags: u32) -> i32;
    fn SetWindowPos(
        hwnd: Hwnd,
        after: Hwnd,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
    fn BeginDeferWindowPos(count: i32) -> Handle;
    fn DeferWindowPos(
        batch: Handle,
        hwnd: Hwnd,
        after: Hwnd,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> Handle;
    fn EndDeferWindowPos(batch: Handle) -> i32;
    fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    fn GetMessageW(message: *mut Message, hwnd: Hwnd, minimum: u32, maximum: u32) -> i32;
    fn TranslateMessage(message: *const Message) -> i32;
    fn DispatchMessageW(message: *const Message) -> Lresult;
    fn IsDialogMessageW(hwnd: Hwnd, message: *mut Message) -> i32;
    fn PostQuitMessage(code: i32);
    fn PostMessageW(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> i32;
    fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, value: Lresult) -> Lresult;
    fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> Lresult;
    fn DestroyWindow(hwnd: Hwnd) -> i32;
    fn SetWindowTextW(hwnd: Hwnd, text: *const u16) -> i32;
    fn GetWindowTextLengthW(hwnd: Hwnd) -> i32;
    fn GetWindowTextW(hwnd: Hwnd, text: *mut u16, length: i32) -> i32;
    fn DrawTextW(dc: Handle, text: *const u16, length: i32, rect: *mut Rect, format: u32) -> i32;
    fn DrawFocusRect(dc: Handle, rect: *const Rect) -> i32;
    fn FillRect(dc: Handle, rect: *const Rect, brush: Hbrush) -> i32;
    fn SendMessageW(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
    fn SetTimer(hwnd: Hwnd, id: usize, millis: u32, callback: *const c_void) -> usize;
    fn KillTimer(hwnd: Hwnd, id: usize) -> i32;
    fn SetFocus(hwnd: Hwnd) -> Hwnd;
    fn SetCapture(hwnd: Hwnd) -> Hwnd;
    fn ReleaseCapture() -> i32;
    fn GetCapture() -> Hwnd;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn SetLayeredWindowAttributes(hwnd: Hwnd, key: u32, alpha: u8, flags: u32) -> i32;
    fn GetSystemMetrics(index: i32) -> i32;
    fn MonitorFromPoint(point: Point, flags: u32) -> Handle;
    fn GetMonitorInfoW(monitor: Handle, info: *mut MonitorInfo) -> i32;
    fn GetDpiForWindow(hwnd: Hwnd) -> u32;
    fn GetWindowPlacement(hwnd: Hwnd, placement: *mut WindowPlacement) -> i32;
    fn SetWindowPlacement(hwnd: Hwnd, placement: *const WindowPlacement) -> i32;
    fn SetCursor(cursor: Handle) -> Handle;
    fn EnableWindow(hwnd: Hwnd, enable: i32) -> i32;
    fn MessageBoxW(hwnd: Hwnd, text: *const u16, caption: *const u16, flags: u32) -> i32;
    fn LoadCursorW(instance: Hinstance, name: *const u16) -> Handle;
    fn GetSysColorBrush(index: i32) -> Hbrush;
    fn InvalidateRect(hwnd: Hwnd, rect: *const Rect, erase: i32) -> i32;
    fn BeginPaint(hwnd: Hwnd, paint: *mut PaintStruct) -> Handle;
    fn EndPaint(hwnd: Hwnd, paint: *const PaintStruct) -> i32;
    #[cfg(test)]
    fn GetDC(hwnd: Hwnd) -> Handle;
    #[cfg(test)]
    fn ReleaseDC(hwnd: Hwnd, dc: Handle) -> i32;
    fn GetFocus() -> Hwnd;
    fn IsWindowEnabled(hwnd: Hwnd) -> i32;
    fn IsZoomed(hwnd: Hwnd) -> i32;
    fn DrawIconEx(
        dc: Handle,
        x: i32,
        y: i32,
        icon: Handle,
        width: i32,
        height: i32,
        step: u32,
        brush: Hbrush,
        flags: u32,
    ) -> i32;
    fn LoadImageW(
        instance: Hinstance,
        name: *const u16,
        kind: u32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> Handle;
    fn TrackMouseEvent(event: *mut TrackMouseEventData) -> i32;
    fn GetDpiForSystem() -> u32;
    fn EnumChildWindows(hwnd: Hwnd, callback: EnumChildProc, parameter: Lparam) -> i32;
    fn SystemParametersInfoW(action: u32, param: u32, data: *mut c_void, update: u32) -> i32;
    fn ScreenToClient(hwnd: Hwnd, point: *mut Point) -> i32;
    fn ClientToScreen(hwnd: Hwnd, point: *mut Point) -> i32;
    fn OpenClipboard(owner: Hwnd) -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, data: Handle) -> Handle;
    fn CloseClipboard() -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Hinstance;
    fn LoadLibraryW(name: *const u16) -> Hinstance;
    fn GetProcAddress(module: Hinstance, name: *const u8) -> *const c_void;
    fn GetUserDefaultUILanguage() -> u16;
    fn GlobalAlloc(flags: u32, bytes: usize) -> Handle;
    fn GlobalLock(handle: Handle) -> *mut c_void;
    fn GlobalUnlock(handle: Handle) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn GetStockObject(index: i32) -> Handle;
    fn SetBkMode(dc: Handle, mode: i32) -> i32;
    fn SetTextColor(dc: Handle, color: u32) -> u32;
    fn CreateFontW(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strikeout: u32,
        charset: u32,
        output_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch: u32,
        face: *const u16,
    ) -> Handle;
    fn CreateSolidBrush(color: u32) -> Hbrush;
    fn CreatePen(style: i32, width: i32, color: u32) -> Handle;
    fn MoveToEx(dc: Handle, x: i32, y: i32, previous: *mut Point) -> i32;
    fn LineTo(dc: Handle, x: i32, y: i32) -> i32;
    fn Ellipse(dc: Handle, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    fn DeleteObject(object: Handle) -> i32;
    fn SelectObject(dc: Handle, object: Handle) -> Handle;
    fn CreateCompatibleDC(dc: Handle) -> Handle;
    fn GetTextMetricsW(dc: Handle, metrics: *mut TextMetric) -> i32;
    fn CreateCompatibleBitmap(dc: Handle, width: i32, height: i32) -> Handle;
    fn BitBlt(dest: Handle, x: i32, y: i32, width: i32, height: i32, source: Handle, source_x: i32, source_y: i32, rop: u32) -> i32;
    fn DeleteDC(dc: Handle) -> i32;
    fn SetBkColor(dc: Handle, color: u32) -> u32;
    fn RoundRect(
        dc: Handle,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        width: i32,
        height: i32,
    ) -> i32;
}

#[link(name = "msimg32")]
unsafe extern "system" {
    fn GradientFill(
        dc: Handle,
        vertices: *mut TriVertex,
        vertex_count: u32,
        mesh: *const GradientRect,
        mesh_count: u32,
        mode: u32,
    ) -> i32;
}

#[link(name = "uxtheme")]
unsafe extern "system" {
    fn SetWindowTheme(hwnd: Hwnd, sub_id: *const u16, sub_id2: *const u16) -> i32;
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegGetValueW(
        key: Handle,
        sub_key: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> i32;
}

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(hwnd: Hwnd, attribute: u32, value: *const c_void, size: u32) -> i32;
}

#[link(name = "comctl32")]
unsafe extern "system" {
    fn InitCommonControlsEx(init: *const InitCommonControlsEx) -> i32;
}

#[link(name = "comdlg32")]
unsafe extern "system" {
    fn GetOpenFileNameW(file: *mut OpenFileName) -> i32;
    fn GetSaveFileNameW(file: *mut OpenFileName) -> i32;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn SHBrowseForFolderW(info: *const BrowseInfo) -> Handle;
    fn SHGetPathFromIDListW(item: Handle, path: *mut u16) -> i32;
    fn ShellExecuteW(
        hwnd: Hwnd,
        verb: *const u16,
        file: *const u16,
        params: *const u16,
        directory: *const u16,
        show: i32,
    ) -> Handle;
}

#[link(name = "ole32")]
unsafe extern "system" {
    fn CoTaskMemFree(pointer: *mut c_void);
    fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> i32;
    fn CoUninitialize();
}

static PROFILE_OVERRIDE: OnceLock<Option<PathBuf>> = OnceLock::new();
static LANGUAGE_OVERRIDE: OnceLock<Option<String>> = OnceLock::new();
static UI_DPI: AtomicU32 = AtomicU32::new(96);
static FONT_CACHE: OnceLock<Mutex<HashMap<(i32, i32, u8), usize>>> = OnceLock::new();
static BRUSH_CACHE: OnceLock<Mutex<HashMap<u32, usize>>> = OnceLock::new();
static CONTROL_RECTS: OnceLock<Mutex<Vec<(isize, i32, i32, i32, i32)>>> = OnceLock::new();
static COMBO_OVERLAYS: OnceLock<Mutex<Vec<(isize, isize)>>> = OnceLock::new();
static ICON_CACHE: OnceLock<Mutex<HashMap<u32, usize>>> = OnceLock::new();
static DARK_THEME: AtomicBool = AtomicBool::new(false);
static DEFER_CONTROL_PAINT: AtomicBool = AtomicBool::new(false);

fn dpi() -> i32 {
    UI_DPI.load(Ordering::Relaxed).max(48) as i32
}
fn scale(value: i32) -> i32 {
    ((value as i64 * dpi() as i64 + 48) / 96) as i32
}
fn unscale(value: i32) -> i32 {
    if value <= 0 {
        value
    } else {
        ((value as i64 * 96 + dpi() as i64 / 2) / dpi() as i64) as i32
    }
}
fn header_height() -> i32 {
    scale(HEADER_HEIGHT)
}
fn dark_theme() -> bool {
    DARK_THEME.load(Ordering::Relaxed)
}
fn system_prefers_dark() -> bool {
    let mut value: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let mut kind = 0u32;
    let result = unsafe {
        RegGetValueW(
            0x8000_0001usize as Handle,
            wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize").as_ptr(),
            wide("AppsUseLightTheme").as_ptr(),
            0x0000_0010,
            &mut kind,
            &mut value as *mut u32 as *mut c_void,
            &mut size,
        )
    };
    result == 0 && value == 0
}
static UXTHEME_ALLOW_DARK: OnceLock<Option<isize>> = OnceLock::new();
fn uxtheme_allow_dark(hwnd: Hwnd, allow: bool) {
    let pointer = *UXTHEME_ALLOW_DARK.get_or_init(|| {
        let module = unsafe { LoadLibraryW(wide("uxtheme.dll").as_ptr()) };
        if module.is_null() {
            return None;
        }
        let address = unsafe { GetProcAddress(module, 133usize as *const u8) };
        (!address.is_null()).then_some(address as isize)
    });
    if let Some(pointer) = pointer {
        let allow_dark: unsafe extern "system" fn(Hwnd, i32) -> i32 =
            unsafe { std::mem::transmute(pointer as *const c_void) };
        unsafe {
            allow_dark(hwnd, i32::from(allow));
        }
    }
}
static UXTHEME_SET_PREFERRED: OnceLock<Option<isize>> = OnceLock::new();
fn uxtheme_preferred_app_mode(dark: bool) {
    let pointer = *UXTHEME_SET_PREFERRED.get_or_init(|| {
        let module = unsafe { LoadLibraryW(wide("uxtheme.dll").as_ptr()) };
        if module.is_null() {
            return None;
        }
        let address = unsafe { GetProcAddress(module, 135usize as *const u8) };
        (!address.is_null()).then_some(address as isize)
    });
    if let Some(pointer) = pointer {
        let set_preferred: unsafe extern "system" fn(i32) -> i32 =
            unsafe { std::mem::transmute(pointer as *const c_void) };
        unsafe {
            set_preferred(if dark { 2 } else { 0 });
        }
    }
}
fn clamp_window_size(width: i32, height: i32) -> (i32, i32) {
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

const SAMPLE_RATES: &[i64] = &[44100, 48000, 88200, 96000, 176400, 192000];
const SAMPLE_BITS: &[i64] = &[16, 20, 24];
const SAMPLE_RATE_LABELS: &[&str] = &[
    "44.1 kHz",
    "48 kHz",
    "88.2 kHz",
    "96 kHz",
    "176.4 kHz",
    "192 kHz",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    Zh,
    En,
    Ja,
}

impl Lang {
    fn from_code(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" | "zh-hans" | "中文" => Self::Zh,
            "en" | "en-us" | "en-gb" | "english" => Self::En,
            "ja" | "ja-jp" | "japanese" | "日本語" => Self::Ja,
            _ => match unsafe { GetUserDefaultUILanguage() } & 0x3ff {
                4 => Self::Zh,
                17 => Self::Ja,
                _ => Self::En,
            },
        }
    }
    fn code(self) -> &'static str {
        match self {
            Self::Zh => "zh-CN",
            Self::En => "en",
            Self::Ja => "ja",
        }
    }
}

fn tr(lang: Lang, key: &str) -> &'static str {
    match lang {
        Lang::Zh => match key {
            "app_subtitle" => "把音乐制作成 DVD-Audio 光盘镜像",
            "advanced_hint" => {
                "这些设置已经足够制作第一张光盘。需要临时工作路径、菜单字体等高级选项时，请开启上方的“显示高级设置”。"
            }
            "profile" => "设置方案",
            "open" => "打开方案",
            "save" => "保存方案",
            "save_as" => "另存方案",
            "advanced" => "显示高级设置",
            "font_filter" => "字体文件",
            "title_mode" => "标题分组（album / one / 每组曲数）",
            "album_limit" => "限制专辑数量（0 表示不限）",
            "language" => "语言",
            "source" => "音源路径",
            "final" => "成品保存路径",
            "title" => "光盘名称",
            "capacity" => "光盘容量",
            "work_dir" => "临时工作路径",
            "iso_prefix" => "镜像文件名前缀",
            "group_limit" => "每组最多曲目",
            "cache" => "复用检查结果",
            "audio_mode" => "音频编码方式",
            "sample_rate" => "采样率",
            "bits" => "音频位深",
            "menu" => "光盘菜单",
            "menu_enable" => "制作选曲菜单",
            "stills" => "在菜单中显示专辑封面",
            "tracks" => "每页显示几首",
            "cover" => "封面暗化程度 (%)",
            "index" => "专辑目录启用门槛",
            "font_sc" => "中文菜单字体",
            "font_jp" => "日文菜单字体",
            "font_kr" => "韩文菜单字体",
            "tools" => "其他设置",
            "author" => "光盘制作工具",
            "keep_tmp" => "保留临时文件",
            "keep_intermediate" => "保留制作中间文件",
            "keep_files" => "保留临时/中间文件",
            "loss_warn" => "时长差异提醒 (秒)",
            "loss_error" => "时长差异上限 (秒)",
            "browse" => "浏览...",
            "start" => "基本设置",
            "audio" => "音频编码",
            "check" => "检查音源",
            "build" => "开始制作",
            "verify" => "验证成品",
            "cancel" => "停止任务",
            "open_output" => "查看成品",
            "summary" => "任务摘要",
            "detail" => "详细日志",
            "issues" => "只看提醒",
            "live" => "实时更新",
            "copy" => "复制当前内容",
            "export" => "导出详细日志...",
            "ready" => "准备就绪",
            "ready_hint" => "选择音源和成品位置，然后点击“开始制作”。",
            "operation_section" => "任务状态",
            "log_section" => "活动日志",
            "statistics" => "用时 {0}:{1}:{2} · 提醒 {3}",
            "running_confirm" => "任务仍在运行。要停止任务并退出吗？",
            "log_found_audio" => "发现 {0} 个音频文件，正在读取曲目信息。",
            "log_resample_none" => "音源采样率符合设置，无需重采样。",
            "log_resample_count" => "有 {0} 首音频需要调整采样率。",
            "log_cache_metadata" => "音源检查缓存：复用 {0} 首，重新检查 {1} 首。",
            "log_cache_verified" => "解码检查缓存：复用 {0} 首，重新检查 {1} 首。",
            "log_encoding" => "正在进行 MLP 无损编码：{0}",
            "log_lpcm" => "LPCM：{0}",
            "log_author_disc" => "正在制作第 {0} 张光盘。",
            "log_loaded_tracks" => "已读取 {0} 首曲目，开始规划光盘。",
            "log_total_tracks" => "已完成 {0} 首音频检查。",
            "log_report_written" => "音源检查报告已保存：{0}",
            "log_manifest_written" => "音源清单已生成：{0}",
            "log_warning_prefix" => "提醒：",
            "log_error_prefix" => "错误：",
            "loaded" => "方案已加载。",
            "saved" => "方案已保存：",
            "profile_missing" => "方案文件不存在：",
            "profile_error" => "方案无法加载：",
            "started_check" => "正在检查音源...",
            "started_build" => "正在检查音源、制作光盘并验证成品...",
            "started_verify" => "正在验证成品...",
            "done" => "任务完成。",
            "failed" => "任务失败。",
            "cancelled" => "任务已取消。",
            "stopping" => "正在停止任务，请稍候...",
            "json_filter" => "JSON 设置方案",
            "text_filter" => "文本日志",
            "all_filter" => "所有文件",
            "export_failed" => "日志保存失败：",
            "log_started" => "开始处理：{0}",
            "log_pcm_started" => "正在准备音频：{0}",
            "log_pcm_progress" => "正在准备音频：{0}（{1}%）",
            "log_pcm_finished" => "音频准备完成，开始 MLP 编码：{0}",
            "log_encoded" => "MLP 编码完成：{0}；{1} Hz / {2} 位 / {3} 声道，{4} 字节",
            "log_pcm_error" => "音频转换提示：{0}；{1}",
            "import" => "导入现有 MLP 文件",
            "import_folder" => "MLP 文件路径",
            "custom" => "自定义容量",
            "custom_bytes" => "自定义容量 (字节)",
            "no_output" => "尚未找到成品路径，请先完成制作。",
            "language_zh" => "中文",
            "language_en" => "English",
            "language_ja" => "日本語",
            "mlp" => "MLP 编码",
            "lpcm" => "LPCM 编码",
            "dvd5" => "DVD5 (4.7 GB)",
            "dvd9" => "DVD9 (8.5 GB)",
            "import_note" => {
                "MLP 无损压缩、占用较小；LPCM 不压缩。导入现有 MLP 时，选择与音源目录结构对应的路径。"
            }
            "mlp_note" => {
                "MLP 编码会进行无损压缩，适合大多数 DVD-Audio 制作。采样率和位深按右侧设置处理。"
            }
            "lpcm_note" => "LPCM 不压缩音频，速度较快但占用空间更大。请确认光盘容量足够。",
            "invalid_sample_rate" => "MLP 目标采样率无效。",
            "invalid_bits" => "MLP 目标位深必须为 16、20 或 24。",
            "invalid_integer" => "请输入 {0} 至 {1} 的整数。",
            "invalid_nonnegative" => "请输入非负有限数值。",
            "invalid_title_mode" => "标题分组规则须为 album、one 或正整数。",
            "invalid_album_limit" => "请输入非负整数，或留空。",
            "select_profile" => "选择 JSON 方案",
            "select_log" => "保存日志",
            "copy_ok" => "日志已复制到剪贴板。",
            "external_tools" => {
                "发布包已内置必要组件，不需要安装 FFmpeg、ImageMagick、eac3to 或外部 MLP 编码器。"
            }
            _ => "",
        },
        Lang::En => match key {
            "app_subtitle" => "Create DVD-Audio disc images from your music",
            "advanced_hint" => {
                "These settings are enough for a first disc. Turn on Advanced settings above for the temporary folder, menu fonts and other options."
            }
            "profile" => "Settings profile",
            "open" => "Open profile",
            "save" => "Save profile",
            "save_as" => "Save as…",
            "advanced" => "Advanced settings",
            "font_filter" => "Font files",
            "title_mode" => "Title grouping (album / one / tracks per title)",
            "album_limit" => "Album limit (0 = unlimited)",
            "language" => "Language",
            "source" => "Source path",
            "final" => "Output path",
            "title" => "Disc title",
            "capacity" => "Disc capacity",
            "work_dir" => "Temporary work path",
            "iso_prefix" => "ISO filename prefix",
            "group_limit" => "Tracks per group",
            "cache" => "Reuse source checks",
            "audio_mode" => "Audio encoding",
            "sample_rate" => "Sample rate",
            "bits" => "Bit depth",
            "menu" => "Disc menu",
            "menu_enable" => "Create track menus",
            "stills" => "Show album covers in menus",
            "tracks" => "Tracks per page",
            "cover" => "Cover dimming (%)",
            "index" => "Album index threshold",
            "font_sc" => "Chinese menu font",
            "font_jp" => "Japanese menu font",
            "font_kr" => "Korean menu font",
            "tools" => "Other settings",
            "author" => "Disc author",
            "keep_tmp" => "Keep temporary files",
            "keep_intermediate" => "Keep intermediate files",
            "keep_files" => "Keep temp/intermediate files",
            "loss_warn" => "Duration warning (seconds)",
            "loss_error" => "Duration limit (seconds)",
            "browse" => "Browse...",
            "start" => "Basic settings",
            "audio" => "Audio",
            "check" => "Check sources",
            "build" => "Build discs",
            "verify" => "Verify output",
            "cancel" => "Stop task",
            "open_output" => "Open output",
            "summary" => "Task summary",
            "detail" => "Detailed log",
            "issues" => "Problems only",
            "live" => "Live update",
            "copy" => "Copy current view",
            "export" => "Export full log...",
            "ready" => "Ready",
            "ready_hint" => "Choose a source and output folder, then select Build discs.",
            "operation_section" => "Task status",
            "log_section" => "Activity log",
            "statistics" => "Elapsed {0}:{1}:{2} · Notices {3}",
            "running_confirm" => "A task is still running. Stop it and exit?",
            "log_found_audio" => "Found {0} audio files. Reading track information.",
            "log_resample_none" => {
                "Source sample rates match the selected settings; no resampling is needed."
            }
            "log_resample_count" => "{0} tracks need sample-rate conversion.",
            "log_cache_metadata" => "Source-check cache: reused {0}; inspected again {1}.",
            "log_cache_verified" => "Decode-check cache: reused {0}; checked again {1}.",
            "log_encoding" => "Encoding MLP audio: {0}",
            "log_lpcm" => "LPCM: {0}",
            "log_author_disc" => "Building disc {0}.",
            "log_loaded_tracks" => "Loaded {0} tracks. Planning discs.",
            "log_total_tracks" => "Completed source checks for {0} tracks.",
            "log_report_written" => "Source-check report saved: {0}",
            "log_manifest_written" => "Source manifest created: {0}",
            "log_warning_prefix" => "Warning: ",
            "log_error_prefix" => "Error: ",
            "loaded" => "Profile loaded.",
            "saved" => "Profile saved: ",
            "profile_missing" => "Profile does not exist: ",
            "profile_error" => "Profile could not be loaded: ",
            "started_check" => "Checking sources...",
            "started_build" => "Checking the source, building discs, and verifying the output...",
            "started_verify" => "Verifying output...",
            "done" => "Task completed.",
            "failed" => "Task failed.",
            "cancelled" => "Task cancelled.",
            "stopping" => "Stopping the task. Please wait...",
            "json_filter" => "JSON settings profiles",
            "text_filter" => "Text logs",
            "all_filter" => "All files",
            "export_failed" => "Could not save the log: ",
            "log_started" => "Processing: {0}",
            "log_pcm_started" => "Preparing audio: {0}",
            "log_pcm_progress" => "Preparing audio: {0} ({1}%)",
            "log_pcm_finished" => "Audio ready. Starting MLP encoding: {0}",
            "log_encoded" => {
                "MLP encoding completed: {0}; {1} Hz / {2} bit / {3} channels, {4} bytes"
            }
            "log_pcm_error" => "Audio conversion message: {0}; {1}",
            "import" => "Import existing MLP files",
            "import_folder" => "MLP file path",
            "custom" => "Custom capacity",
            "custom_bytes" => "Custom capacity (bytes)",
            "no_output" => "The output folder does not exist yet.",
            "language_zh" => "中文",
            "language_en" => "English",
            "language_ja" => "日本語",
            "mlp" => "MLP encoding",
            "lpcm" => "LPCM encoding",
            "dvd5" => "DVD5 (4.7 GB)",
            "dvd9" => "DVD9 (8.5 GB)",
            "import_note" => {
                "MLP uses lossless compression; LPCM is uncompressed. To import MLP files, select a folder matching your source folder structure."
            }
            "mlp_note" => {
                "MLP uses lossless compression and is suitable for most DVD-Audio projects. The sample rate and bit depth follow the settings above."
            }
            "lpcm_note" => {
                "LPCM stores uncompressed audio. It is quick, but uses more disc space; make sure the selected capacity is sufficient."
            }
            "invalid_sample_rate" => "The MLP target sample rate is invalid.",
            "invalid_bits" => "MLP bit depth must be 16, 20, or 24.",
            "invalid_integer" => "Enter an integer from {0} to {1}.",
            "invalid_nonnegative" => "Enter a finite non-negative number.",
            "invalid_title_mode" => "Title grouping must be album, one, or a positive integer.",
            "invalid_album_limit" => "Enter a non-negative integer, or leave this blank.",
            "select_profile" => "Select JSON profile",
            "select_log" => "Save log",
            "copy_ok" => "Log copied to the clipboard.",
            "external_tools" => {
                "The release includes required components; FFmpeg, ImageMagick, eac3to and external MLP encoders are not required."
            }
            _ => "",
        },
        Lang::Ja => match key {
            "app_subtitle" => "音楽から DVD-Audio ディスクイメージを作成",
            "advanced_hint" => {
                "最初のディスク作成にはこの設定で十分です。一時フォルダーやメニューフォントなどの詳細設定は、上の「詳細設定を表示」で変更できます。"
            }
            "profile" => "設定プロファイル",
            "open" => "プロファイルを開く",
            "save" => "プロファイルを保存",
            "save_as" => "名前を付けて保存",
            "advanced" => "詳細設定を表示",
            "font_filter" => "フォントファイル",
            "title_mode" => "タイトルの区切り（album / one / 曲数）",
            "album_limit" => "アルバム数の上限（0 = 無制限）",
            "language" => "言語",
            "source" => "音源パス",
            "final" => "出力パス",
            "title" => "ディスク名",
            "capacity" => "ディスク容量",
            "work_dir" => "一時作業パス",
            "iso_prefix" => "ISO ファイル名の接頭辞",
            "group_limit" => "グループの曲数",
            "cache" => "検査結果を再利用",
            "audio_mode" => "音声エンコード",
            "sample_rate" => "サンプルレート",
            "bits" => "ビット深度",
            "menu" => "ディスクメニュー",
            "menu_enable" => "曲目メニューを作成",
            "stills" => "メニューにアルバム画像を表示",
            "tracks" => "ページごとの曲数",
            "cover" => "画像の暗さ (%)",
            "index" => "アルバム索引のしきい値",
            "font_sc" => "中国語メニュー字体",
            "font_jp" => "日本語メニュー字体",
            "font_kr" => "韓国語メニュー字体",
            "tools" => "その他の設定",
            "author" => "ディスク作成ツール",
            "keep_tmp" => "一時ファイルを保持",
            "keep_intermediate" => "中間ファイルを保持",
            "keep_files" => "一時/中間ファイルを保持",
            "loss_warn" => "時間差の警告 (秒)",
            "loss_error" => "時間差の上限 (秒)",
            "browse" => "参照...",
            "start" => "基本設定",
            "audio" => "音声",
            "check" => "音源を検査",
            "build" => "ディスクを作成",
            "verify" => "出力を検証",
            "cancel" => "タスクを停止",
            "open_output" => "出力を開く",
            "summary" => "タスク概要",
            "detail" => "詳細ログ",
            "issues" => "問題のみ",
            "live" => "ライブ更新",
            "copy" => "表示をコピー",
            "export" => "ログを保存...",
            "ready" => "準備完了",
            "ready_hint" => "音源と出力フォルダーを選び、「ディスクを作成」を押してください。",
            "operation_section" => "タスクの状態",
            "log_section" => "アクティビティログ",
            "statistics" => "経過時間 {0}:{1}:{2} · 注意 {3}",
            "running_confirm" => "タスクが実行中です。停止して終了しますか？",
            "log_found_audio" => "音声ファイルが {0} 件見つかりました。曲情報を確認しています。",
            "log_resample_none" => {
                "音源のサンプリング周波数は設定と一致しています。リサンプリングは不要です。"
            }
            "log_resample_count" => "サンプリング周波数の変換が必要な曲は {0} 件です。",
            "log_cache_metadata" => "音源確認キャッシュ：{0} 件を再利用、{1} 件を再確認。",
            "log_cache_verified" => "デコード確認キャッシュ：{0} 件を再利用、{1} 件を再確認。",
            "log_encoding" => "MLP ロスレスエンコード中：{0}",
            "log_lpcm" => "LPCM：{0}",
            "log_author_disc" => "ディスク {0} を作成しています。",
            "log_loaded_tracks" => "{0} 曲を読み込みました。ディスクを計画しています。",
            "log_total_tracks" => "{0} 曲の音源確認が完了しました。",
            "log_report_written" => "音源確認レポートを保存しました：{0}",
            "log_manifest_written" => "音源マニフェストを作成しました：{0}",
            "log_warning_prefix" => "注意：",
            "log_error_prefix" => "エラー：",
            "loaded" => "プロファイルを読み込みました。",
            "saved" => "プロファイルを保存しました: ",
            "profile_missing" => "プロファイルがありません: ",
            "profile_error" => "プロファイルを読み込めません: ",
            "started_check" => "音源を検査しています...",
            "started_build" => "音源を確認し、ディスクを作成して完成品を検証しています...",
            "started_verify" => "出力を検証しています...",
            "done" => "タスクが完了しました。",
            "failed" => "タスクに失敗しました。",
            "cancelled" => "タスクをキャンセルしました。",
            "stopping" => "タスクを停止しています。しばらくお待ちください...",
            "json_filter" => "JSON 設定プロファイル",
            "text_filter" => "テキストログ",
            "all_filter" => "すべてのファイル",
            "export_failed" => "ログを保存できません：",
            "log_started" => "処理を開始：{0}",
            "log_pcm_started" => "音声を準備中：{0}",
            "log_pcm_progress" => "音声を準備中：{0}（{1}%）",
            "log_pcm_finished" => "音声の準備完了。MLP エンコードを開始：{0}",
            "log_encoded" => {
                "MLP エンコード完了：{0}；{1} Hz / {2} ビット / {3} チャンネル、{4} バイト"
            }
            "log_pcm_error" => "音声変換メッセージ：{0}；{1}",
            "import" => "既存の MLP ファイルを読み込む",
            "import_folder" => "MLP ファイルパス",
            "custom" => "容量を指定",
            "custom_bytes" => "指定容量 (バイト)",
            "no_output" => "出力フォルダーがまだありません。",
            "language_zh" => "中文",
            "language_en" => "English",
            "language_ja" => "日本語",
            "mlp" => "MLP エンコード",
            "lpcm" => "LPCM エンコード",
            "dvd5" => "DVD5 (4.7 GB)",
            "dvd9" => "DVD9 (8.5 GB)",
            "import_note" => {
                "MLP はロスレス圧縮、LPCM は非圧縮です。MLP を読み込む場合は、音源と同じフォルダー構成の場所を選択してください。"
            }
            "mlp_note" => {
                "MLP はロスレス圧縮で、多くの DVD-Audio 制作に適しています。サンプルレートとビット深度は上の設定を使用します。"
            }
            "lpcm_note" => {
                "LPCM は非圧縮音声です。処理は速いですが容量を多く使うため、ディスク容量を確認してください。"
            }
            "invalid_sample_rate" => "MLP の目標サンプルレートが無効です。",
            "invalid_bits" => "MLP のビット深度は 16、20、24 のいずれかです。",
            "invalid_integer" => "{0} から {1} までの整数を入力してください。",
            "invalid_nonnegative" => "0 以上の有限な数値を入力してください。",
            "invalid_title_mode" => "タイトルの区切りは album、one、または正の整数です。",
            "invalid_album_limit" => "0 以上の整数を入力するか、空欄にしてください。",
            "select_profile" => "JSON プロファイルを選択",
            "select_log" => "ログを保存",
            "copy_ok" => "ログをクリップボードにコピーしました。",
            "external_tools" => {
                "必要なコンポーネントは同梱されています。FFmpeg、ImageMagick、eac3to、外部 MLP エンコーダーは不要です。"
            }
            _ => "",
        },
    }
}

#[derive(Clone, Copy)]
enum BrowseKind {
    Folder,
    Font,
}

#[derive(Clone)]
struct LogLine {
    raw: String,
    problem: bool,
    timestamp: String,
}

struct UiEvent {
    kind: u32,
    text: String,
}

enum WorkerEvent {
    Log(String),
    Progress(u16),
    Finished(UiEvent),
    Retention(AppOptions),
}

struct Controls {
    statistics: Hwnd,
    advanced: Hwnd,
    profile: Hwnd,
    language: Hwnd,
    source: Hwnd,
    final_dir: Hwnd,
    title: Hwnd,
    capacity: Hwnd,
    custom_bytes: Hwnd,
    work_dir: Hwnd,
    iso_prefix: Hwnd,
    group_limit: Hwnd,
    keep_files: Hwnd,
    mode: Hwnd,
    rate: Hwnd,
    bits: Hwnd,
    import_folder: Hwnd,
    menu: Hwnd,
    stills: Hwnd,
    tracks: Hwnd,
    cover: Hwnd,
    font_sc: Hwnd,
    font_jp: Hwnd,
    font_kr: Hwnd,
    status: Hwnd,
    activity: Hwnd,
    audio_note: Hwnd,
    progress: Hwnd,
    log_view: Hwnd,
    only_issues: Hwnd,
    live: Hwnd,
    log: Hwnd,
    prepare: Hwnd,
    build: Hwnd,
    verify: Hwnd,
    cancel: Hwnd,
    open_output: Hwnd,
    profile_origin: Hwnd,
}

struct UiState {
    lang: Lang,
    main_window: Hwnd,
    controls: Controls,
    custom_bytes_label: Hwnd,
    import_folder_label: Hwnd,
    import_browse: Hwnd,
    pages: Vec<Hwnd>,
    wheel_remainders: HashMap<usize, i32>,
    tabs: Vec<Hwnd>,
    tab_index: usize,
    labels: Vec<(Hwnd, &'static str)>,
    muted: Vec<Hwnd>,
    buttons: Vec<(Hwnd, &'static str)>,
    base: Option<AppOptions>,
    profile_path: PathBuf,
    browse: HashMap<usize, (Hwnd, BrowseKind)>,
    browse_buttons: Vec<Hwnd>,
    tooltip_window: Hwnd,
    tooltips: Vec<Tooltip>,
    logs: Vec<LogLine>,
    summary_logs: Vec<LogLine>,
    log_file: Option<(fs::File, PathBuf)>,
    log_sender: SyncSender<WorkerEvent>,
    log_receiver: Receiver<WorkerEvent>,
    log_dirty: bool,
    log_rendered_count: usize,
    log_render_language: Option<Lang>,
    log_render_detail: bool,
    log_render_issues_only: bool,
    log_first_visible_line: i32,
    log_hold_line: Option<i32>,
    log_hold_until: Option<Instant>,
    activity_dirty: bool,
    pending_finish: Option<UiEvent>,
    status_key: &'static str,
    activity_raw: String,
    busy: bool,
    /// Short windows show either the settings or the task area; set while
    /// the settings (tabs and pages) give way to a running task.
    settings_hidden: bool,
    busy_disabled_controls: Vec<Hwnd>,
    close_when_done: bool,
    cancel: Option<Arc<AtomicBool>>,
    started: Option<Instant>,
    elapsed_seconds: u64,
    progress_value: u16,
    problem_count: usize,
    statistics_text: String,
    splitter_height: Option<i32>,
    /// Settings height chosen from the first full-size layout; it then stays
    /// fixed so height changes go to the log instead of moving the task area.
    default_tab_height: Option<i32>,
    /// Splitter range of the last layout: tabs top and the largest settings
    /// height that still leaves the log one line.
    splitter_tabs_y: i32,
    splitter_max: i32,
    /// Whether the last layout was the short-window one (with hysteresis).
    compact_layout: Option<bool>,
    splitter_position: i32,
    dragging_splitter: bool,
    /// Inside the modal sizing loop (WM_ENTERSIZEMOVE..WM_EXITSIZEMOVE).
    sizing: bool,
    /// Log area of the last layout, in client pixels.
    log_box: Option<Rect>,
    /// Clipping frame that hosts the log EDIT.
    log_frame: Hwnd,
    /// The log EDIT kept its size during live sizing and differs from its frame.
    log_frozen: bool,
    splitter_preview: Option<i32>,
    splitter_preview_window: Hwnd,
    advanced_controls: Vec<Hwnd>,
    advanced_hint: Hwnd,
    caption_minimize: Hwnd,
    caption_maximize: Hwnd,
    caption_close: Hwnd,
    theme_button: Hwnd,
    theme_override: Option<bool>,
    page_scrollbar: Hwnd,
    log_scrollbar: Hwnd,
}

struct WorkerCallbacks {
    logs: SyncSender<WorkerEvent>,
    cancel: Arc<AtomicBool>,
    range: (u16, u16),
    last_progress: u16,
}

impl WorkerCallbacks {
    fn set_progress_range(&mut self, start: u16, end: u16) {
        let start = start.min(100);
        self.range = (start, end.min(100).max(start));
        self.progress(0, 100);
    }
}

impl Callbacks for WorkerCallbacks {
    fn emit(&mut self, _stream: i32, text: &str) {
        // Backpressure bounds pending memory without dropping diagnostics or
        // flooding the Win32 message queue with individual progress messages.
        let _ = self.logs.send(WorkerEvent::Log(text.to_owned()));
    }
    fn cancelled(&mut self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn progress(&mut self, completed: u64, total: u64) {
        if total == 0 {
            return;
        }
        let portion = completed
            .min(total)
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(100) as u16;
        let value = (self.range.0 + (self.range.1 - self.range.0).saturating_mul(portion) / 100)
            .min(99)
            .max(self.last_progress);
        if value != self.last_progress {
            self.last_progress = value;
            let _ = self.logs.send(WorkerEvent::Progress(value));
        }
    }
}

fn main() {
    let started = parse_startup_args(
        std::env::args().skip(1),
        std::env::var("DVDA_LANGUAGE").ok().as_deref(),
    )
    .and_then(|args| {
        let _ = PROFILE_OVERRIDE.set(args.profile);
        let _ = LANGUAGE_OVERRIDE.set(args.language);
        // SAFETY: entrypoint has not started the GUI, worker threads or native libraries.
        unsafe { dvda_core::runtime::initialize(EMBEDDED_RUNTIME) }.and_then(|_| run())
    });
    if let Err(error) = started {
        if let Ok((mut file, _)) = open_session_log() {
            let _ = writeln!(file, "[startup] {error}");
        }
        let profile_language = PROFILE_OVERRIDE
            .get()
            .and_then(Option::as_ref)
            .and_then(|p| profile_language(p))
            .unwrap_or_else(|| "auto".into());
        message_box(
            ptr::null_mut(),
            &localized_detail(startup_language(&profile_language), &error),
            "DVD-Audio Maker",
            0x10,
        );
        std::process::exit(1);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct StartupArgs {
    profile: Option<PathBuf>,
    language: Option<String>,
}

fn parse_startup_args(
    args: impl IntoIterator<Item = String>,
    environment_language: Option<&str>,
) -> Result<StartupArgs, String> {
    let usage = "Usage: DVD-Audio-Maker [--profile profile.json | --config profile.json] [--language auto|en|zh-CN|ja]";
    let mut result = StartupArgs {
        profile: None,
        language: None,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--profile" | "--config" => {
                let value = args
                    .next()
                    .filter(|v| !v.is_empty() && !v.starts_with("--"))
                    .ok_or_else(|| usage.to_owned())?;
                result.profile = Some(PathBuf::from(value));
            }
            "--language" => {
                if result.language.is_some() {
                    return Err(usage.into());
                }
                let value = args
                    .next()
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| usage.to_owned())?;
                result.language = Some(normalize_startup_language(&value)?);
            }
            _ => return Err(format!("Unknown argument: {arg}\n{usage}")),
        }
    }
    if result.language.is_none() {
        result.language = environment_language
            .map(normalize_startup_language)
            .transpose()?;
    }
    Ok(result)
}

fn normalize_startup_language(value: &str) -> Result<String, String> {
    if !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "auto"
            | "zh"
            | "zh-cn"
            | "zh-hans"
            | "中文"
            | "en"
            | "en-us"
            | "en-gb"
            | "english"
            | "ja"
            | "ja-jp"
            | "japanese"
            | "日本語"
    ) {
        return Err("Language must be auto, en, zh-CN, or ja.".into());
    }
    Ok(Lang::from_code(value).code().into())
}

fn startup_language(profile_language: &str) -> Lang {
    Lang::from_code(
        LANGUAGE_OVERRIDE
            .get()
            .and_then(Option::as_deref)
            .unwrap_or(profile_language),
    )
}

fn run() -> Result<(), String> {
    struct ComApartment(bool);
    impl Drop for ComApartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }
    let _com = ComApartment(unsafe { CoInitializeEx(ptr::null_mut(), 2) } >= 0);
    unsafe {
        let init = InitCommonControlsEx {
            size: std::mem::size_of::<InitCommonControlsEx>() as u32,
            classes: 0xffff,
        };
        InitCommonControlsEx(&init);
    }
    let instance = unsafe { GetModuleHandleW(ptr::null()) };
    if instance.is_null() {
        return Err("Cannot get application instance".into());
    }
    let system_dpi = unsafe { GetDpiForSystem() };
    UI_DPI.store(system_dpi.max(96), Ordering::Relaxed);
    DARK_THEME.store(system_prefers_dark(), Ordering::Relaxed);
    uxtheme_preferred_app_mode(dark_theme());
    if cfg!(debug_assertions) {
        if let Ok(value) = std::env::var("DVDA_UI_DPI") {
            if let Ok(value) = value.parse::<u32>() {
                if value >= 48 {
                    UI_DPI.store(value, Ordering::Relaxed);
                }
            }
        }
    }
    let icon = unsafe { LoadIconW(instance, 1usize as *const u16) };
    if icon.is_null() {
        return Err("Cannot load application icon".into());
    }
    let class_name = wide("DVD_AUDIO_MAKER_RUST");
    let registration = WndClass {
        style: 0,
        procedure: window_proc,
        extra_class: 0,
        extra_window: 0,
        instance,
        icon,
        cursor: unsafe { LoadCursorW(ptr::null_mut(), 32512usize as *const u16) },
        background: ptr::null_mut(),
        menu: ptr::null(),
        class_name: class_name.as_ptr(),
    };
    if unsafe { RegisterClassW(&registration) } == 0 {
        return Err("Cannot register desktop window".into());
    }
    let page_class_name = wide("DVD_AUDIO_PAGE_RUST");
    let page_registration = WndClass {
        style: 0,
        procedure: page_proc,
        extra_class: 0,
        extra_window: 0,
        instance,
        icon: ptr::null_mut(),
        cursor: unsafe { LoadCursorW(ptr::null_mut(), 32512usize as *const u16) },
        background: panel_brush(),
        menu: ptr::null(),
        class_name: page_class_name.as_ptr(),
    };
    if unsafe { RegisterClassW(&page_registration) } == 0 {
        return Err("Cannot register desktop page class".into());
    }
    let scroll_class_name = wide("DVD_AUDIO_SCROLLBAR_RUST");
    let scroll_registration = WndClass {
        style: 0,
        procedure: scrollbar_proc,
        extra_class: 0,
        extra_window: 0,
        instance,
        icon: ptr::null_mut(),
        cursor: unsafe { LoadCursorW(ptr::null_mut(), 32512usize as *const u16) },
        background: ptr::null_mut(),
        menu: ptr::null(),
        class_name: scroll_class_name.as_ptr(),
    };
    if unsafe { RegisterClassW(&scroll_registration) } == 0 {
        return Err("Cannot register desktop scrollbar class".into());
    }
    let title = wide("DVD-Audio Maker");
    let (window_width, window_height) = clamp_window_size(1180, 880);
    let mut window_rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut window_x = scale(100);
    let mut window_y = scale(70);
    if unsafe { SystemParametersInfoW(0x0030, 0, &mut window_rect as *mut Rect as *mut c_void, 0) }
        != 0
    {
        window_x = window_x
            .min(window_rect.right - scale(window_width) - scale(16))
            .max(window_rect.left + scale(8));
        window_y = window_y
            .min(window_rect.bottom - scale(window_height) - scale(16))
            .max(window_rect.top + scale(8));
    }
    let window = unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_CAPTION
                | WS_THICKFRAME
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_SYSMENU
                | WS_VISIBLE
                | WS_CLIPCHILDREN,
            window_x,
            window_y,
            scale(window_width),
            scale(window_height),
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            ptr::null_mut(),
        )
    };
    if window.is_null() {
        return Err("Cannot create desktop window".into());
    }
    unsafe {
        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
        sync_combo_overlays();
        let dark: u32 = 1;
        let mut dark_applied = DwmSetWindowAttribute(
            window,
            20,
            &dark as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        ) == 0;
        if !dark_applied {
            dark_applied = DwmSetWindowAttribute(
                window,
                19,
                &dark as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            ) == 0;
        }
        if dark_applied {
            // Windows 11 lets the caption follow the branded header instead of
            // remaining a light strip above it.
            let caption: u32 = c_header_top();
            DwmSetWindowAttribute(
                window,
                35,
                &caption as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            );
            let caption_text: u32 = c_header_title();
            DwmSetWindowAttribute(
                window,
                36,
                &caption_text as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
    let mut message = Message {
        hwnd: ptr::null_mut(),
        message: 0,
        wparam: 0,
        lparam: 0,
        time: 0,
        point_x: 0,
        point_y: 0,
    };
    loop {
        let status = unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) };
        if status <= 0 {
            break;
        }
        unsafe {
            // The primary action owns Enter, but focused text fields, drop-down
            // lists and buttons keep their own keyboard behaviour.
            if message.message == WM_KEYDOWN && message.wparam == VK_RETURN {
                let focus = GetFocus();
                if focus.is_null() {
                    SendMessageW(window, WM_COMMAND, ID_BUILD, 0);
                    continue;
                }
                let class = window_class(focus);
                if class == "Edit" {
                    // Let the edit field consume Enter (and never start a build
                    // while the user is editing a path or title).
                } else if class == "ComboBox" {
                    if SendMessageW(focus, CB_GETDROPPEDSTATE, 0, 0) == 0 {
                        SendMessageW(window, WM_COMMAND, ID_BUILD, 0);
                        continue;
                    }
                } else if class == "Button" {
                    let id = GetDlgCtrlID(focus);
                    if id > 0 {
                        SendMessageW(window, WM_COMMAND, id as usize, 0);
                    }
                    continue;
                } else {
                    SendMessageW(window, WM_COMMAND, ID_BUILD, 0);
                    continue;
                }
            }
            if IsDialogMessageW(window, &mut message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    if message == WM_CLOSE {
        if let Some(state) = unsafe { state(hwnd) }
            && state.busy
        {
            state.close_when_done = true;
            cancel_operation(state);
            return 0;
        }
        if let Some(state) = unsafe { state(hwnd) } {
            autosave(state);
        }
        unsafe {
            DestroyWindow(hwnd);
        }
        return 0;
    }
    if message == WM_DESTROY {
        unsafe {
            KillTimer(hwnd, 1);
        }
        let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut UiState;
        if !pointer.is_null() {
            unsafe {
                drop(Box::from_raw(pointer));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
        }
        unsafe {
            PostQuitMessage(0);
        }
        return 0;
    }
    match message {
        WM_CREATE => {
            let ui = Box::new(create_controls(hwnd));
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(ui) as isize);
                SetTimer(hwnd, 1, 100, ptr::null());
            }
            if let Some(state) = unsafe { state(hwnd) } {
                apply_theme(hwnd, state, dark_theme());
            }
            0
        }
        WM_TIMER => {
            if let Some(state) = unsafe { state(hwnd) } {
                drain_worker_logs(hwnd, state);
                update_statistics(state);
                unsafe {
                    refresh_dynamic_indicators(state);
                }
            }
            0
        }
        WM_SETTINGCHANGE => {
            if let Some(state) = unsafe { state(hwnd) }
                && state.theme_override.is_none()
            {
                let system = system_prefers_dark();
                if system != dark_theme() {
                    apply_theme(hwnd, state, system);
                }
            }
            0
        }
        WM_NCLBUTTONDOWN => {
            // HTLEFT, HTTOP, HTTOPLEFT, HTTOPRIGHT, HTBOTTOMLEFT
            if matches!(wparam, 10 | 12 | 13 | 14 | 16) && unsafe { IsZoomed(hwnd) } == 0 {
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
                unsafe {
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
            if let Some(state) = unsafe { state(hwnd) } {
                let y = (lparam >> 16) as i16 as i32;
                let over = (state.splitter_position - scale(5)
                    ..state.splitter_position + scale(10))
                    .contains(&y);
                if message == 0x0201 && over {
                    state.dragging_splitter = true;
                    unsafe {
                        SetCapture(hwnd);
                        update_splitter_preview(state, hwnd, y);
                    }
                } else if message == 0x0202 && state.dragging_splitter {
                    state.dragging_splitter = false;
                    unsafe {
                        ReleaseCapture();
                    }
                    let preview = state.splitter_preview;
                    unsafe { hide_splitter_preview(state) };
                    if let Some(preview_y) = preview {
                        state.splitter_height = Some(
                            (unscale(preview_y) - state.splitter_tabs_y).clamp(104, state.splitter_max.max(104)),
                        );
                        // layout() moves children with SWP_NOREDRAW and then
                        // repaints the whole tree synchronously.
                        layout(hwnd, state);
                    }
                } else if message == 0x0200 && state.dragging_splitter && wparam & 1 != 0 {
                    unsafe { update_splitter_preview(state, hwnd, y) };
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
            if wparam != 1 {
                if let Some(state) = unsafe { state(hwnd) } {
                    layout(hwnd, state);
                }
            }
            0
        }
        0x0231 | 0x0232 => {
            // WM_ENTERSIZEMOVE / WM_EXITSIZEMOVE: the log keeps its size while
            // the user drags an edge and gets the final size once at the end.
            if let Some(state) = unsafe { state(hwnd) } {
                state.sizing = message == 0x0231;
                if !state.sizing && state.log_frozen {
                    layout(hwnd, state);
                    if state.log_dirty && checked(state.controls.live) {
                        render_log(state);
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
            unsafe {
                ScreenToClient(hwnd, &mut point);
                GetClientRect(hwnd, &mut rect);
            }
            // Borderless windows still need a usable sizing frame.
            if unsafe { IsZoomed(hwnd) } == 0 {
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
        WM_GETMINMAXINFO => {            if lparam != 0 {
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
            if let Some(state) = unsafe { state(hwnd) } {
                rescale_ui(hwnd, state);
            }
            0
        }
        WM_COMMAND => {
            let id = wparam & 0xffff;
            let notify = ((wparam >> 16) & 0xffff) as u16;
            if let Some(state) = unsafe { state(hwnd) } {
                handle_command(hwnd, state, id, notify);
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
        WM_CTLCOLORSTATIC => unsafe {
            let dc = wparam as Handle;
            let control = lparam as Hwnd;
            SetBkMode(dc, 1);
            if IsWindowEnabled(control) == 0 && window_class(control) == "Edit" {
                SetBkColor(dc, c_disabled_fill());
                SetTextColor(dc, c_disabled_text());
                brush(c_disabled_fill()) as Lresult
            } else if state(hwnd).is_some_and(|state| state.controls.log == control) {
                SetBkColor(dc, c_card());
                SetTextColor(dc, c_ink());
                card_brush() as Lresult
            } else {
                SetBkColor(dc, c_body());
                let muted = state(hwnd).is_some_and(|state| state.muted.contains(&control));
                SetTextColor(dc, if muted { c_muted() } else { c_ink() });
                body_brush() as Lresult
            }
        },
        WM_CTLCOLOREDIT => unsafe {
            let dc = wparam as Handle;
            SetBkMode(dc, 1);
            SetBkColor(dc, c_card());
            SetTextColor(dc, c_ink());
            card_brush() as Lresult
        },
        WM_CTLCOLORLISTBOX => unsafe {
            SetBkMode(wparam as Handle, 1);
            SetBkColor(wparam as Handle, c_card());
            SetTextColor(wparam as Handle, c_ink());
            card_brush() as Lresult
        },
        WM_CTLCOLORBTN => unsafe {
            SetBkMode(wparam as Handle, 1);
            SetBkColor(wparam as Handle, c_body());
            body_brush() as Lresult
        },
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

static SPLITTER_PREVIEW_CLASS_READY: OnceLock<()> = OnceLock::new();

unsafe extern "system" fn splitter_preview_proc(
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

unsafe fn ensure_splitter_preview(owner: Hwnd) -> Hwnd {
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
fn window_arranged(hwnd: Hwnd) -> bool {
    type IsWindowArranged = unsafe extern "system" fn(Hwnd) -> i32;
    static FUNCTION: OnceLock<usize> = OnceLock::new();
    let address = *FUNCTION.get_or_init(|| unsafe {
        let module = GetModuleHandleW(wide("user32.dll").as_ptr());
        if module.is_null() {
            0
        } else {
            GetProcAddress(module, b"IsWindowArranged\0".as_ptr()) as usize
        }
    });
    address != 0 && unsafe { std::mem::transmute::<usize, IsWindowArranged>(address)(hwnd) } != 0
}

unsafe fn outline_resize(hwnd: Hwnd, hit: usize, origin: Point) {
    unsafe {
        let mut start: Rect = std::mem::zeroed();
        GetWindowRect(hwnd, &mut start);
        // Same limits as the window manager would apply.
        let mut limits = [0i32; 10];
        limits[8] = i32::MAX / 4;
        limits[9] = i32::MAX / 4;
        SendMessageW(hwnd, WM_GETMINMAXINFO, 0, limits.as_mut_ptr() as Lparam);
        let (min_w, min_h) = (limits[6].max(1), limits[7].max(1));
        let (max_w, max_h) = (limits[8].max(min_w), limits[9].max(min_h));
        let left = matches!(hit, 10 | 13 | 16);
        let top = matches!(hit, 12 | 13 | 14);
        let right = hit == 14;
        let bottom = hit == 16;
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
                let mut info = MonitorInfo { size: std::mem::size_of::<MonitorInfo>() as u32, ..std::mem::zeroed() };
                if GetMonitorInfoW(MonitorFromPoint(cursor, 2), &mut info) != 0 && cursor.y <= info.monitor.top {
                    rect.top = info.work.top;
                    rect.bottom = info.work.bottom;
                }
            }
            if bottom {
                rect.bottom = (start.bottom + dy).clamp(start.top + min_h, start.top + max_h);
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
        while GetCapture() == hwnd && GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {
            match message.message {
                0x0200 => {
                    // WM_MOUSEMOVE
                    let mut cursor = Point { x: 0, y: 0 };
                    GetCursorPos(&mut cursor);
                    if !dragging && (cursor.x - origin.x).abs() < drag_x && (cursor.y - origin.y).abs() < drag_y {
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
                let mut info = MonitorInfo { size: std::mem::size_of::<MonitorInfo>() as u32, ..std::mem::zeroed() };
                let center = Point { x: (current.left + current.right) / 2, y: (current.top + current.bottom) / 2 };
                let (dx, dy) = if GetMonitorInfoW(MonitorFromPoint(center, 2), &mut info) != 0 {
                    (info.work.left - info.monitor.left, info.work.top - info.monitor.top)
                } else {
                    (0, 0)
                };
                let mut placement: WindowPlacement = std::mem::zeroed();
                placement.length = std::mem::size_of::<WindowPlacement>() as u32;
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

unsafe fn update_splitter_preview(state: &mut UiState, main: Hwnd, client_y: i32) {
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
    let client_y = client_y.clamp(low, scale(state.splitter_tabs_y + state.splitter_max.max(104)).max(low));
    state.splitter_preview = Some(client_y);
    let mut point = Point {
        x: 0,
        y: client_y,
    };
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

unsafe fn hide_splitter_preview(state: &mut UiState) {
    if !state.splitter_preview_window.is_null() {
        unsafe {
            ShowWindow(state.splitter_preview_window, SW_HIDE);
        }
    }
    state.splitter_preview = None;
}
unsafe extern "system" fn page_proc(
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
            let old = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as i32;
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
            let old = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as i32;
            let delta = (wparam >> 16) as i16 as i32;
            let pixels = if let Some(state) = unsafe { state(GetParent(hwnd)) } {
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
            let parent = unsafe { GetParent(hwnd) };
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
        WM_CTLCOLORSTATIC => unsafe {
            let dc = wparam as Handle;
            let control = lparam as Hwnd;
            let parent = GetParent(hwnd);
            let muted = state(parent).is_some_and(|state| state.muted.contains(&control));
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
        WM_CTLCOLOREDIT => unsafe {
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
        WM_CTLCOLORLISTBOX => unsafe {
            SetBkMode(wparam as Handle, 1);
            SetBkColor(wparam as Handle, c_card());
            SetTextColor(wparam as Handle, c_ink());
            card_brush() as Lresult
        },
        WM_CTLCOLORBTN => unsafe {
            SetBkMode(wparam as Handle, 1);
            SetBkColor(wparam as Handle, c_panel());
            panel_brush() as Lresult
        },
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn wheel_pixels(delta: i32, step: i32, remainder: &mut i32) -> i32 {
    // Preserve sub-pixel input from high-resolution wheels across messages.
    let total = delta * step + *remainder;
    *remainder = total % 120;
    total / 120
}

#[cfg(test)]
mod wheel_tests {
    use super::wheel_pixels;

    #[test]
    fn partial_notches_move_immediately() {
        let mut remainder = 0;
        assert_eq!(wheel_pixels(30, 40, &mut remainder), 10);
        assert_eq!(wheel_pixels(-30, 40, &mut remainder), -10);
    }

    #[test]
    fn small_deltas_preserve_full_notch_distance_at_each_scale() {
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
        let mut remainder = 0;
        assert_eq!(wheel_pixels(1, 40, &mut remainder), 0);
        assert_eq!(wheel_pixels(-1, 40, &mut remainder), 0);
        assert_eq!(remainder, 0);
    }
}

unsafe extern "system" fn scroll_page_child(hwnd: Hwnd, parameter: Lparam) -> i32 {
    let (page, delta) = unsafe { &*(parameter as *const (Hwnd, i32)) };
    if unsafe { GetParent(hwnd) } != *page {
        return 1;
    }
    let mut rect: Rect = unsafe { std::mem::zeroed() };
    unsafe { GetWindowRect(hwnd, &mut rect); }
    let mut point = Point { x: rect.left, y: rect.top };
    unsafe {
        ScreenToClient(*page, &mut point);
        SetWindowPos(hwnd, ptr::null_mut(), point.x, point.y + delta, 0, 0, 0x011D);
    }
    1
}
/// Height of a page's content in pixels: the lowest visible control (at
/// scroll position 0) plus a bottom margin.
fn page_content_height(page: Hwnd) -> i32 {
    unsafe extern "system" fn lowest(hwnd: Hwnd, parameter: Lparam) -> i32 {
        let (page, bottom) = unsafe { &mut *(parameter as *mut (Hwnd, i32)) };
        if unsafe { GetParent(hwnd) } == *page && (unsafe { GetWindowLongPtrW(hwnd, -16) } as u32) & WS_VISIBLE != 0 {
            let mut rect: Rect = unsafe { std::mem::zeroed() };
            unsafe { GetWindowRect(hwnd, &mut rect) };
            let mut corner = Point { x: rect.left, y: rect.bottom };
            unsafe { ScreenToClient(*page, &mut corner) };
            *bottom = (*bottom).max(corner.y);
        }
        1
    }
    let mut found = (page, 0);
    unsafe { EnumChildWindows(page, lowest, &mut found as *mut _ as Lparam) };
    let scroll = unsafe { GetWindowLongPtrW(page, GWLP_USERDATA) } as i32;
    if found.1 <= 0 { scale(100) } else { found.1 + scroll + scale(14) }
}

fn page_is_narrow(width: i32) -> bool {
    width < 1000
}

fn scroll_page(hwnd: Hwnd, requested: i32, explicit: bool) {
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        GetClientRect(hwnd, &mut rect);
    }
    let old = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as i32;
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
    let main = unsafe { GetParent(hwnd) };
    if !main.is_null() {
        if let Some(state) = unsafe { state(main) } {
            unsafe {
                InvalidateRect(state.page_scrollbar, ptr::null(), 0);
            }
        }
    }
    if !DEFER_CONTROL_PAINT.load(Ordering::Relaxed) {
        sync_combo_overlays();
    }
}

/// Horizontal layout of the log toolbar, in DIPs relative to the left margin.
#[derive(Debug, PartialEq)]
struct LogToolbar {
    label_width: i32,
    combo_x: i32,
    check_x: [i32; 2],
    check_width: [i32; 2],
    button_x: [i32; 2],
    button_width: [i32; 2],
    /// The copy/export buttons moved to a second row.
    wrap: bool,
}

const LOG_COMBO_WIDTH: i32 = 132;
const LOG_BUTTON_WIDTHS: [i32; 2] = [130, 148];

/// Fits `(minimum, preferred, priority)` slots into `available`, returning
/// `None` when even the minimums do not fit. Free space restores slots one at
/// a time, lowest priority value first, so every slot and position changes
/// monotonically with the width and nothing jitters while resizing.
fn fit_row(slots: &[(i32, i32, u8)], available: i32) -> Option<Vec<i32>> {
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
fn log_toolbar_layout(available: i32, label_text: i32, check_text: [i32; 2], button_text: [i32; 2]) -> LogToolbar {
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
        let bar = log_toolbar_layout(1200, 60, [80, 70], [90, 100]);
        assert!(!bar.wrap);
        assert_eq!((bar.label_width, bar.combo_x, bar.check_x, bar.check_width), (120, 128, [276, 414], [130, 130]));
        assert_eq!((bar.button_x, bar.button_width), ([1200 - 148 - 8 - 130, 1200 - 148], [130, 148]));
    }

    #[test]
    fn spacing_shrinks_before_wrapping() {
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
        let bar = log_toolbar_layout(604, 130, [80, 70], [90, 100]);
        assert!(bar.wrap);
        assert!(bar.check_x[1] + bar.check_width[1] <= 604);
        assert_eq!(bar.button_x[1] + bar.button_width[1], 604);
        assert!(bar.button_x[0] >= 0);
    }
}

fn browse_edit_width(page_width: i32) -> i32 {
    // Reserve both page margins as well as the button and its gap.
    (page_width - 14 - 14 - 90 - 10).max(120)
}

#[cfg(test)]
mod browse_layout_tests {
    use super::browse_edit_width;

    #[test]
    fn browse_rows_preserve_right_margin_at_half_and_full_width() {
        for page_width in [420, 640, 780, 800, 1120, 1600] {
            let edit_right = 14 + browse_edit_width(page_width);
            let button_left = edit_right + 10;
            assert_eq!(page_width - (button_left + 90), 14);
        }
    }
}

fn resize_page_fields(page: Hwnd) {
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        GetClientRect(page, &mut rect);
    }
    let width = unscale(rect.right).max(1);
    let scroll = unsafe { GetWindowLongPtrW(page, GWLP_USERDATA) } as i32;
    // Browse rows share the same geometry. Keeping the edit and its button
    // inside the page client area prevents a clipped right border after a
    // window resize or a DPI/layout change.
    let edit_width = browse_edit_width(width);
    for (edit_id, browse_id, y) in [
        (ID_SOURCE, ID_BROWSE_SOURCE, 38),
        (ID_FINAL, ID_BROWSE_FINAL, 82),
        (ID_WORK_DIR, ID_BROWSE_WORK, 170),
    ] {
        let edit = unsafe { GetDlgItem(page, edit_id as i32) };
        let browse = unsafe { GetDlgItem(page, browse_id as i32) };
        let top = y - unscale(scroll);
        if !edit.is_null() {
            unsafe { place(edit, 14, top, edit_width, 24) };
        }
        if !browse.is_null() {
            unsafe { place(browse, 14 + edit_width + 10, top, 90, 24) };
        }
    }
}

unsafe fn state<'a>(hwnd: Hwnd) -> Option<&'a mut UiState> {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut UiState;
    (!pointer.is_null()).then(|| unsafe { &mut *pointer })
}

macro_rules! create {
    ($parent:expr, $class:expr, $title:expr, $style:expr, $x:expr, $y:expr, $width:expr, $height:expr, $id:expr $(,)?) => {
        create_from_spec(CreateSpec {
            parent: $parent,
            class: $class,
            title: $title,
            style: $style,
            x: $x,
            y: $y,
            width: $width,
            height: $height,
            id: $id,
        })
    };
}

struct CreateSpec<'a> {
    parent: Hwnd,
    class: &'a str,
    title: &'a str,
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    id: usize,
}

macro_rules! label {
    ($parent:expr, $text:expr, $key:expr, $x:expr, $y:expr, $w:expr, $h:expr, $labels:expr $(,)?) => {{
        let hwnd = create!(
            $parent,
            "STATIC",
            $text,
            WS_CHILD | WS_VISIBLE,
            $x,
            $y,
            $w,
            $h,
            0
        );
        if !$key.is_empty() {
            $labels.push((hwnd, $key));
        }
        hwnd
    }};
}

macro_rules! text_field {
    ($parent:expr, $caption:expr, $key:expr, $id:expr, $x:expr, $y:expr, $width:expr, $labels:expr $(,)?) => {{
        label!($parent, $caption, $key, $x, $y, $width, 20, $labels);
        create!(
            $parent,
            "EDIT",
            "",
            WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL | WS_TABSTOP,
            $x,
            $y + 20,
            $width,
            25,
            $id
        )
    }};
}

macro_rules! browse_edit {
    ($parent:expr, $caption:expr, $key:expr, $id:expr, $browse_id:expr, $x:expr, $y:expr, $labels:expr, $browse:expr, $browse_buttons:expr, $lang:expr $(,)?) => {{
        label!($parent, $caption, $key, $x, $y, 520, 18, $labels);
        let edit = create!(
            $parent,
            "EDIT",
            "",
            WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL | WS_TABSTOP,
            $x,
            $y + 20,
            820,
            24,
            $id
        );
        // End before the next row's label (24 logical pixels below the edit).
        let browse_button = button(
            $parent,
            tr($lang, "browse"),
            $browse_id,
            $x + 830,
            $y + 20,
            90,
            24,
        );
        $browse_buttons.push(browse_button);
        $browse.insert(
            $browse_id,
            (edit, BrowseKind::Folder),
        );
        edit
    }};
}

macro_rules! combo_field {
    ($parent:expr, $caption:expr, $key:expr, $id:expr, $x:expr, $y:expr, $width:expr, $labels:expr $(,)?) => {{
        label!($parent, $caption, $key, $x, $y, $width, 20, $labels);
        combo($parent, $id, $x, $y + 20, $width, 160)
    }};
}

fn create_controls(parent: Hwnd) -> UiState {
    if let Some(registry) = CONTROL_RECTS.get() {
        if let Ok(mut registry) = registry.lock() {
            registry.clear();
        }
    }
    if let Some(overlays) = COMBO_OVERLAYS.get() {
        if let Ok(mut overlays) = overlays.lock() {
            overlays.clear();
        }
    }

    let profile_path = PROFILE_OVERRIDE
        .get()
        .and_then(Clone::clone)
        .unwrap_or_else(dvda_core::app::default_profile_path);
    let lang = startup_language(&profile_language(&profile_path).unwrap_or_else(|| "auto".into()));
    let mut labels = Vec::new();
    let mut buttons = Vec::new();
    let mut browse = HashMap::new();
    let mut browse_buttons = Vec::new();
    let profile_label = label!(
        parent,
        tr(lang, "profile"),
        "profile",
        18,
        14,
        100,
        22,
        &mut labels,
    );
    let profile = create!(
        parent,
        "EDIT",
        "",
        WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL | WS_TABSTOP,
        120,
        11,
        620,
        26,
        ID_PROFILE,
    );
    let open_profile = button(parent, tr(lang, "open"), ID_OPEN_PROFILE, 750, 10, 110, 28);
    let save_profile_button = button(parent, tr(lang, "save"), ID_SAVE_PROFILE, 870, 10, 110, 28);
    let save_as_button = button(parent, tr(lang, "save_as"), ID_SAVE_AS, 870, 10, 110, 28);
    let language_label = label!(
        parent,
        tr(lang, "language"),
        "language",
        995,
        14,
        60,
        22,
        &mut labels,
    );
    let language = combo(parent, ID_LANGUAGE, 1055, 10, 100, 120);
    combo_add(language, tr(lang, "language_zh"));
    combo_add(language, tr(lang, "language_en"));
    combo_add(language, tr(lang, "language_ja"));
    combo_select(
        language,
        match lang {
            Lang::Zh => 0,
            Lang::En => 1,
            Lang::Ja => 2,
        },
    );
    let origin = label!(
        parent,
        &profile_path.to_string_lossy(),
        "",
        390,
        42,
        1030,
        20,
        &mut Vec::new(),
    );
    let mut tabs = Vec::new();
    for (index, key) in ["start", "audio", "menu"].iter().enumerate() {
        tabs.push(tab_button(
            parent,
            tr(lang, key),
            ID_TAB + index,
            18 + index as i32 * 138,
            126,
            132,
            34,
        ));
    }
    let mut pages = Vec::new();
    let mut c = Controls {
        statistics: ptr::null_mut(),
        advanced: ptr::null_mut(),
        profile,
        language,
        source: ptr::null_mut(),
        final_dir: ptr::null_mut(),
        title: ptr::null_mut(),
        capacity: ptr::null_mut(),
        custom_bytes: ptr::null_mut(),
        work_dir: ptr::null_mut(),
        iso_prefix: ptr::null_mut(),
        group_limit: ptr::null_mut(),
        keep_files: ptr::null_mut(),
        mode: ptr::null_mut(),
        rate: ptr::null_mut(),
        bits: ptr::null_mut(),
        import_folder: ptr::null_mut(),
        menu: ptr::null_mut(),
        stills: ptr::null_mut(),
        tracks: ptr::null_mut(),
        cover: ptr::null_mut(),
        font_sc: ptr::null_mut(),
        font_jp: ptr::null_mut(),
        font_kr: ptr::null_mut(),
        status: ptr::null_mut(),
        activity: ptr::null_mut(),
        audio_note: ptr::null_mut(),
        progress: ptr::null_mut(),
        log_view: ptr::null_mut(),
        only_issues: ptr::null_mut(),
        live: ptr::null_mut(),
        log: ptr::null_mut(),
        prepare: ptr::null_mut(),
        build: ptr::null_mut(),
        verify: ptr::null_mut(),
        cancel: ptr::null_mut(),
        open_output: ptr::null_mut(),
        profile_origin: origin,
    };
    let page_start = create_page(parent);
    pages.push(page_start);
    c.source = browse_edit!(
        page_start,
        tr(lang, "source"),
        "source",
        ID_SOURCE,
        ID_BROWSE_SOURCE,
        14,
        18,
        &mut labels,
        &mut browse,
        &mut browse_buttons,
        lang,
    );
    c.final_dir = browse_edit!(
        page_start,
        tr(lang, "final"),
        "final",
        ID_FINAL,
        ID_BROWSE_FINAL,
        14,
        62,
        &mut labels,
        &mut browse,
        &mut browse_buttons,
        lang,
    );
    c.title = text_field!(
        page_start,
        tr(lang, "title"),
        "title",
        ID_TITLE,
        14,
        106,
        470,
        &mut labels,
    );
    c.capacity = combo_field!(
        page_start,
        tr(lang, "capacity"),
        "capacity",
        ID_CAPACITY,
        520,
        106,
        220,
        &mut labels,
    );
    combo_add(c.capacity, tr(lang, "dvd5"));
    combo_add(c.capacity, tr(lang, "dvd9"));
    combo_add(c.capacity, tr(lang, "custom"));
    combo_select(c.capacity, 0);
    c.work_dir = browse_edit!(
        page_start,
        tr(lang, "work_dir"),
        "work_dir",
        ID_WORK_DIR,
        ID_BROWSE_WORK,
        14,
        150,
        &mut labels,
        &mut browse,
        &mut browse_buttons,
        lang,
    );
    c.iso_prefix = text_field!(
        page_start,
        tr(lang, "iso_prefix"),
        "iso_prefix",
        ID_ISO_PREFIX,
        14,
        194,
        460,
        &mut labels,
    );
    c.group_limit = text_field!(
        page_start,
        tr(lang, "group_limit"),
        "group_limit",
        ID_GROUP_LIMIT,
        520,
        194,
        220,
        &mut labels,
    );
    c.keep_files = check_field(
        page_start,
        tr(lang, "keep_files"),
        "keep_files",
        ID_KEEP_FILES,
        760,
        194,
        &mut labels,
    );    c.custom_bytes = text_field!(
        page_start,
        tr(lang, "custom_bytes"),
        "custom_bytes",
        ID_CUSTOM_BYTES,
        760,
        106,
        360,
        &mut labels,
    );
    let custom_bytes_label = labels
        .last()
        .map(|(handle, _)| *handle)
        .unwrap_or(ptr::null_mut());
    unsafe {
        ShowWindow(custom_bytes_label, SW_HIDE);
        ShowWindow(c.custom_bytes, SW_HIDE);
    }
    let advanced_hint = label!(
        page_start,
        tr(lang, "advanced_hint"),
        "advanced_hint",
        14,
        166,
        780,
        48,
        &mut labels,
    );
    unsafe {
        SendMessageW(advanced_hint, WM_SETFONT, small_font() as usize, 1);
    }
    pages.push(create_page(parent));
    let page_audio = *pages.last().unwrap();
    c.mode = combo_field!(
        page_audio,
        tr(lang, "audio_mode"),
        "audio_mode",
        ID_MODE,
        14,
        18,
        300,
        &mut labels,
    );
    combo_add(c.mode, tr(lang, "mlp"));
    combo_add(c.mode, tr(lang, "lpcm"));
    combo_add(c.mode, tr(lang, "import"));
    combo_select(c.mode, 0);
    c.rate = combo_field!(
        page_audio,
        tr(lang, "sample_rate"),
        "sample_rate",
        ID_RATE,
        340,
        18,
        210,
        &mut labels,
    );
    for item in SAMPLE_RATE_LABELS {
        combo_add(c.rate, item);
    }
    c.bits = combo_field!(
        page_audio,
        tr(lang, "bits"),
        "bits",
        ID_BITS,
        575,
        18,
        160,
        &mut labels,
    );
    for item in ["16", "20", "24"] {
        combo_add(c.bits, item);
    }
    c.import_folder = browse_edit!(
        page_audio,
        tr(lang, "import_folder"),
        "import_folder",
        ID_IMPORT_FOLDER,
        ID_BROWSE_IMPORT,
        14,
        80,
        &mut labels,
        &mut browse,
        &mut browse_buttons,
        lang,
    );
    let import_folder_label = labels
        .last()
        .map(|(handle, _)| *handle)
        .unwrap_or(ptr::null_mut());
    let import_browse = browse_buttons.last().copied().unwrap_or(ptr::null_mut());
    unsafe {
        ShowWindow(import_folder_label, SW_HIDE);
        ShowWindow(c.import_folder, SW_HIDE);
        ShowWindow(import_browse, SW_HIDE);
    }    c.audio_note = label!(
        page_audio,
        tr(lang, "mlp_note"),
        "audio_note",
        14,
        136,
        900,
        48,
        &mut labels,
    );
    pages.push(create_page(parent));
    let page_menu = *pages.last().unwrap();
    c.menu = check_field(
        page_menu,
        tr(lang, "menu_enable"),
        "menu_enable",
        ID_MENU,
        14,
        18,
        &mut labels,
    );
    c.stills = check_field(
        page_menu,
        tr(lang, "stills"),
        "stills",
        ID_STILLS,
        300,
        18,
        &mut labels,
    );
    c.tracks = text_field!(
        page_menu,
        tr(lang, "tracks"),
        "tracks",
        ID_TRACKS,
        14,
        66,
        220,
        &mut labels,
    );
    c.cover = text_field!(
        page_menu,
        tr(lang, "cover"),
        "cover",
        ID_COVER,
        260,
        66,
        220,
        &mut labels,
    );
    c.font_sc = text_field!(
        page_menu,
        tr(lang, "font_sc"),
        "font_sc",
        ID_FONT_SC,
        14,
        120,
        300,
        &mut labels,
    );
    c.font_jp = text_field!(
        page_menu,
        tr(lang, "font_jp"),
        "font_jp",
        ID_FONT_JP,
        340,
        120,
        300,
        &mut labels,
    );
    c.font_kr = text_field!(
        page_menu,
        tr(lang, "font_kr"),
        "font_kr",
        ID_FONT_KR,
        666,
        120,
        300,
        &mut labels,
    );
    for (field, id, x) in [
        (c.font_sc, 1007, 14),
        (c.font_jp, 1008, 340),
        (c.font_kr, 1009, 666),
    ] {
        let handle = button(page_menu, tr(lang, "browse"), id, x, 174, 150, 28);
        browse.insert(id, (field, BrowseKind::Font));
        browse_buttons.push(handle);
    }
    let _operation_section = label!(
        parent,
        tr(lang, "operation_section"),
        "operation_section",
        18,
        426,
        220,
        18,
        &mut labels,
    );
    let status = label!(
        parent,
        tr(lang, "ready"),
        "ready",
        18,
        448,
        220,
        24,
        &mut labels,
    );
    let activity = label!(
        parent,
        tr(lang, "ready_hint"),
        "ready_hint",
        250,
        448,
        650,
        24,
        &mut labels,
    );
    let progress = create!(
        parent,
        "msctls_progress32",
        "",
        WS_CHILD | WS_VISIBLE,
        18,
        476,
        1140,
        8,
        0,
    );
    let _log_section = label!(
        parent,
        tr(lang, "log_section"),
        "log_section",
        18,
        488,
        220,
        18,
        &mut labels,
    );
    let log_view = combo(parent, ID_LOG_VIEW, 18, 508, 140, 120);
    combo_add(log_view, tr(lang, "summary"));
    combo_add(log_view, tr(lang, "detail"));
    combo_select(log_view, 0);
    let only_issues = checkbox(
        parent,
        tr(lang, "issues"),
        ID_ONLY_ISSUES,
        170,
        508,
        150,
        24,
        false,
    );
    let live = checkbox(parent, tr(lang, "live"), ID_LIVE, 330, 508, 150, 24, false);
    check(live, true);
    let copy = button(parent, tr(lang, "copy"), ID_COPY, 720, 478, 125, 28);
    let export = button(parent, tr(lang, "export"), ID_EXPORT, 855, 478, 130, 28);
    let log_frame = create_log_frame(parent);
    let log = create!(
        log_frame,
        "EDIT",
        "",
        WS_CHILD
            | WS_VISIBLE
            | ES_MULTILINE
            | ES_AUTOVSCROLL
            | ES_READONLY
            | ES_WANTRETURN,
        0,
        0,
        1140,
        215,
        ID_LOG,
    );
    let page_scrollbar = create!(
        parent,
        "DVD_AUDIO_SCROLLBAR_RUST",
        "",
        WS_CHILD | WS_VISIBLE,
        0,
        0,
        12,
        200,
        0,
    );
    let log_scrollbar = create!(
        parent,
        "DVD_AUDIO_SCROLLBAR_RUST",
        "",
        WS_CHILD | WS_VISIBLE,
        0,
        0,
        12,
        200,
        0,
    );
    unsafe {
        install_scrollbar(page_scrollbar, parent);
        install_scrollbar(log_scrollbar, parent);
        touch::install(log, true);
    }
    unsafe {
        SendMessageW(log, EM_SETLIMITTEXT, 1_000_000, 0);
    }
    // Keep the legacy command IDs for automation/tests, but expose the
    // complete prepare-build-verify workflow through the single build action.
    let prepare = create!(parent, "BUTTON", "", WS_CHILD, 18, 740, 145, 34, ID_PREPARE);
    let build = primary_button(parent, tr(lang, "build"), ID_BUILD, 175, 740, 145, 34);
    let verify = create!(parent, "BUTTON", "", WS_CHILD, 332, 740, 145, 34, ID_VERIFY);
    let cancel = danger_button(parent, tr(lang, "cancel"), ID_CANCEL, 490, 740, 145, 34);
    let open_output = button(
        parent,
        tr(lang, "open_output"),
        ID_OPEN_OUTPUT,
        650,
        740,
        145,
        34,
    );
    c.status = status;
    c.activity = activity;
    c.progress = progress;
    c.log_view = log_view;
    c.only_issues = only_issues;
    c.live = live;
    c.log = log;
    c.prepare = prepare;
    c.build = build;
    c.verify = verify;
    c.cancel = cancel;
    c.open_output = open_output;
    labels.extend([(only_issues, "issues"), (live, "live")]);
    buttons.extend([
        (open_profile, "open"),
        (save_profile_button, "save"),
        (save_as_button, "save_as"),
        (prepare, "check"),
        (build, "build"),
        (verify, "verify"),
        (cancel, "cancel"),
        (open_output, "open_output"),
        (copy, "copy"),
        (export, "export"),
    ]);
    c.advanced = checkbox(parent, tr(lang, "advanced"), ID_ADVANCED, 440, 42, 215, 22, false);
    labels.push((c.advanced, "advanced"));
    let caption_minimize = caption_button(parent, ID_MINIMIZE);
    let caption_maximize = caption_button(parent, ID_MAXIMIZE);
    let caption_close = caption_button(parent, ID_CLOSE);
    let theme_button = caption_button(parent, ID_THEME);
    // Preserve the old distinction between everyday and advanced settings.
    let mut advanced_controls = vec![
        c.work_dir,
        c.iso_prefix,
        c.group_limit,
        c.keep_files,
        c.font_sc,
        c.font_jp,
        c.font_kr,
    ];
    let advanced_keys = [
        "work_dir",
        "iso_prefix",
        "group_limit",
        "keep_files",
        "font_sc",
        "font_jp",
        "font_kr",
    ];
    advanced_controls.extend(
        labels
            .iter()
            .filter(|(_, key)| advanced_keys.contains(key))
            .map(|(h, _)| *h),
    );
    for (id, (field, _)) in &browse {
        if advanced_controls.contains(field)
            && let Some(button) = browse_buttons
                .iter()
                .find(|h| unsafe { GetDlgCtrlID(**h) } as usize == *id)
        {
            advanced_controls.push(*button);
        }
    }
    c.statistics = label!(parent, "", "", 810, 700, 330, 26, &mut Vec::new());
    let loaded = if profile_path.exists() || PROFILE_OVERRIDE.get().is_some_and(Option::is_some) {
        AppOptions::load(Some(profile_path.as_path()))
    } else {
        AppOptions::load(None)
    };
    let startup_error = loaded.as_ref().err().cloned();
    let base = loaded.ok();
    let session_log = open_session_log();
    let session_log_error = session_log.as_ref().err().map(ToString::to_string);
    let (log_sender, log_receiver) = sync_channel(LOG_QUEUE_CAPACITY);
    for line in [status, activity, c.statistics] {
        single_line_label(line);
    }
    let muted = vec![
        profile_label,
        origin,
        _operation_section,
        _log_section,
        activity,
        c.statistics,
        advanced_hint,
    ];
    let mut state = UiState {
        lang,
        main_window: parent,
        controls: c,
        custom_bytes_label,
        import_folder_label,
        import_browse,
        pages,
        wheel_remainders: HashMap::new(),
        tabs,
        tab_index: 0,
        labels,
        muted,
        buttons,
        base,
        profile_path,
        browse,
        browse_buttons,
        tooltip_window: ptr::null_mut(),
        tooltips: Vec::new(),
        logs: Vec::new(),
        summary_logs: Vec::new(),
        log_file: session_log.ok(),
        log_sender,
        log_receiver,
        log_dirty: false,
        log_rendered_count: 0,
        log_render_language: None,
        log_render_detail: false,
        log_render_issues_only: false,
        log_first_visible_line: 0,
        log_hold_line: None,
        log_hold_until: None,
        activity_dirty: false,
        pending_finish: None,
        status_key: "ready",
        activity_raw: tr(Lang::Zh, "ready_hint").into(),
        busy: false,
        settings_hidden: false,
        busy_disabled_controls: Vec::new(),
        close_when_done: false,
        cancel: None,
        started: None,
        elapsed_seconds: 0,
        progress_value: 0,
        problem_count: 0,
        statistics_text: String::new(),
        splitter_height: None,
        default_tab_height: None,
        splitter_tabs_y: HEADER_HEIGHT + 56,
        splitter_max: 350,
        compact_layout: None,
        splitter_position: 420,
        dragging_splitter: false,
        sizing: false,
        log_box: None,
        log_frame,
        log_frozen: false,
        splitter_preview: None,
        splitter_preview_window: ptr::null_mut(),
        advanced_controls,
        advanced_hint,
        caption_minimize,
        caption_maximize,
        caption_close,
        theme_button,
        theme_override: None,
        page_scrollbar,
        log_scrollbar,
    };
    unsafe {
        EnableWindow(state.controls.cancel, 0);
        SetWindowTheme(
            state.controls.progress,
            wide("").as_ptr(),
            wide("").as_ptr(),
        );
        SendMessageW(
            state.controls.progress,
            PBM_SETBKCOLOR,
            0,
            c_bar_track() as isize,
        );
        SendMessageW(
            state.controls.progress,
            PBM_SETBARCOLOR,
            0,
            c_accent() as isize,
        );
        SendMessageW(
            state.controls.status,
            WM_SETFONT,
            semibold_font() as usize,
            1,
        );
        SendMessageW(
            state.controls.statistics,
            WM_SETFONT,
            small_font() as usize,
            1,
        );
        SendMessageW(state.controls.activity, WM_SETFONT, ui_font() as usize, 1);
    }
    set_text(
        state.controls.profile,
        &state.profile_path.to_string_lossy(),
    );
    if let Some(base) = state.base.clone() {
        populate(&mut state, &base);
    }
    initialize_tooltips(parent, &mut state);
    set_language(&mut state, lang);
    if let Some(error) = startup_error {
        add_log(&mut state, &format!("[错误] {error}"), true);
    }
    if let Some(error) = session_log_error {
        let message = presentation::phrase(
            lang,
            "无法保存完整日志，请检查磁盘空间或目录权限。",
            "Cannot save the full log; check disk space and folder permissions.",
            "完全なログを保存できません。空き容量とフォルダー権限を確認してください。",
        );
        add_log(&mut state, &format!("[警告] {message} {error}"), true);
    }
    show_page(&state, 0);
    update_advanced(&state);
    update_statistics(&mut state);
    let _ = profile_label;
    let _ = language_label;
    let _ = copy;
    let _ = export;
    state
}

fn create_page(parent: Hwnd) -> Hwnd {
    let hwnd = create!(
        parent,
        "DVD_AUDIO_PAGE_RUST",
        "",
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
        22,
        170,
        1130,
        310,
        0,
    );
    touch::install(hwnd, false);
    hwnd
}

fn check_field(
    parent: Hwnd,
    caption: &str,
    key: &'static str,
    id: usize,
    x: i32,
    y: i32,
    labels: &mut Vec<(Hwnd, &'static str)>,
) -> Hwnd {
    let hwnd = checkbox(parent, caption, id, x, y, 230, 28, true);
    labels.push((hwnd, key));
    hwnd
}

fn checkbox(
    parent: Hwnd,
    text: &str,
    id: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    panel: bool,
) -> Hwnd {
    let hwnd = create!(
        parent,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX,
        x,
        y,
        w,
        h,
        id,
    );
    if !hwnd.is_null() {
        install_check_visual(hwnd, panel);
    }
    hwnd
}
fn combo(parent: Hwnd, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {

    let hwnd = create!(
        parent,
        "COMBOBOX",
        "",
        WS_CHILD
            | WS_VISIBLE
            | WS_CLIPCHILDREN // The closed-field overlay child owns these pixels.
            | WS_TABSTOP
            | CBS_DROPDOWNLIST
            | CBS_OWNERDRAWFIXED
            | CBS_HASSTRINGS,
        x,
        y,
        w,
        h,
        id,
    );
    if hwnd.is_null() {
        return hwnd;
    }

    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        GetClientRect(hwnd, &mut rect);
    }
    touch::install_combo(hwnd);
    // The overlay owns the whole closed combo so the themed border and text
    // replace the system-drawn field completely. As a child of the combo it
    // moves, hides and clips together with it; a sibling overlay lagged one
    // step behind during live resizing and exposed the white native field.
    let overlay = create_from_spec(CreateSpec {
        parent: hwnd,
        class: "BUTTON",
        title: "",
        style: WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        x: 0,
        y: 0,
        width: unscale(rect.right).max(20),
        height: unscale(rect.bottom).max(20),
        id: 0,
    });
    if !overlay.is_null() {
        install_combo_arrow(overlay, hwnd);
        if window_class(parent) == "DVD_AUDIO_PAGE_RUST" {
            touch::install(overlay, false);
        }
        let registry = COMBO_OVERLAYS.get_or_init(|| Mutex::new(Vec::new()));
        if let Ok(mut registry) = registry.lock() {
            registry.push((hwnd as isize, overlay as isize));
        }
    }
    hwnd
}

fn button(parent: Hwnd, text: &str, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {
    owner_draw_button(
        parent,
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        id,
        x,
        y,
        w,
        h,
    )
}

fn primary_button(parent: Hwnd, text: &str, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {
    owner_draw_button(
        parent,
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        id,
        x,
        y,
        w,
        h,
    )
}

fn danger_button(parent: Hwnd, text: &str, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {
    owner_draw_button(
        parent,
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        id,
        x,
        y,
        w,
        h,
    )
}

fn tab_button(parent: Hwnd, text: &str, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {
    owner_draw_button(
        parent,
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        id,
        x,
        y,
        w,
        h,
    )
}

fn caption_button(parent: Hwnd, id: usize) -> Hwnd {
    owner_draw_button(
        parent,
        "",
        WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        id,
        0,
        0,
        40,
        30,
    )
}

fn owner_draw_button(
    parent: Hwnd,
    text: &str,
    style: u32,
    id: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Hwnd {
    let hwnd = create!(parent, "BUTTON", text, style, x, y, w, h, id);
    if !hwnd.is_null() {
        install_button_visual(hwnd);
    }
    hwnd
}

fn install_button_visual(hwnd: Hwnd) {
    unsafe {
        let original: WndProc = std::mem::transmute(GetWindowLongPtrW(hwnd, GWLP_WNDPROC));
        let visual = Box::new(ButtonVisual {
            hover: false,
            original,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(visual) as isize);
        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, button_proc as *const () as isize);
    }
}

unsafe extern "system" fn button_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut ButtonVisual;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    match message {
        // WM_DRAWITEM paints the whole button off-screen; the native erase
        // would flash the bare WM_CTLCOLORBTN brush between the two passes.
        WM_ERASEBKGND => return 1,
        WM_MOUSEMOVE => {
            if !unsafe { (*pointer).hover } {
                unsafe {
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
        }
        WM_MOUSELEAVE => {
            if unsafe { (*pointer).hover } {
                unsafe {
                    (*pointer).hover = false;
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
        }
        WM_NCDESTROY => {
            let original = unsafe { Box::from_raw(pointer) }.original;
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                SetWindowLongPtrW(hwnd, GWLP_WNDPROC, original as *const () as isize);
            }
            return unsafe { original(hwnd, message, wparam, lparam) };
        }
        _ => {}
    }
    let original = unsafe { (*pointer).original };
    unsafe { original(hwnd, message, wparam, lparam) }
}

fn button_hovered(hwnd: Hwnd) -> bool {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const ButtonVisual;
    !pointer.is_null() && unsafe { (*pointer).hover }
}

fn install_check_visual(hwnd: Hwnd, panel: bool) {
    unsafe {
        let original: WndProc = std::mem::transmute(GetWindowLongPtrW(hwnd, GWLP_WNDPROC));
        let visual = Box::new(CheckVisual {
            hover: false,
            panel,
            original,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(visual) as isize);
        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, checkbox_proc as *const () as isize);
    }
}

unsafe extern "system" fn checkbox_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut CheckVisual;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    match message {
        WM_PAINT => {
            unsafe { draw_checkbox(hwnd) };
            return 0;
        }
        WM_ERASEBKGND => return 1,
        WM_MOUSEMOVE => {
            if !unsafe { (*pointer).hover } {
                unsafe {
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
        }
        WM_MOUSELEAVE => {
            if unsafe { (*pointer).hover } {
                unsafe {
                    (*pointer).hover = false;
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            let original = unsafe { (*pointer).original };
            let result = unsafe { original(hwnd, message, wparam, lparam) };
            unsafe { InvalidateRect(hwnd, ptr::null(), 0) };
            return result;
        }
        WM_NCDESTROY => {
            let original = unsafe { Box::from_raw(pointer) }.original;
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                SetWindowLongPtrW(hwnd, GWLP_WNDPROC, original as *const () as isize);
            }
            return unsafe { original(hwnd, message, wparam, lparam) };
        }
        _ => {}
    }
    let original = unsafe { (*pointer).original };
    unsafe { original(hwnd, message, wparam, lparam) }
}

unsafe fn draw_checkbox(hwnd: Hwnd) {
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
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const CheckVisual;
        let panel = !pointer.is_null() && (*pointer).panel;
        let hover = !pointer.is_null() && (*pointer).hover;
        FillRect(dc, &rect, if panel { panel_brush() } else { body_brush() });
        let checked = SendMessageW(hwnd, BM_GETCHECK, 0, 0) == 1;
        let enabled = IsWindowEnabled(hwnd) != 0;
        let box_size = scale(16);
        let top = (rect.bottom - box_size) / 2;
        let left = scale(2);
        let fill = if !enabled {
            c_disabled_fill()
        } else if checked {
            if hover {
                c_accent_hover()
            } else {
                c_accent()
            }
        } else {
            c_card()
        };
        let border = if !enabled {
            c_disabled_text()
        } else if checked || hover {
            c_accent()
        } else {
            c_border()
        };
        fill_round_rect(
            dc,
            &Rect {
                left,
                top,
                right: left + box_size,
                bottom: top + box_size,
            },
            scale(3),
            fill,
            border,
        );
        if checked {
            let pen = CreatePen(0, scale(2).max(1), c_header_top());
            if !pen.is_null() {
                let old = SelectObject(dc, pen);
                let cx = left + box_size / 2;
                let cy = top + box_size / 2;
                MoveToEx(dc, left + scale(4), cy, ptr::null_mut());
                LineTo(dc, cx - scale(1), top + box_size - scale(4));
                LineTo(dc, left + box_size - scale(3), top + scale(4));
                SelectObject(dc, old);
                DeleteObject(pen);
            }
        }
        let label = get_text(hwnd);
        let mut text_rect = Rect {
            left: left + box_size + scale(7),
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        draw_text_run(
            &label,
            dc,
            &mut text_rect,
            ui_font(),
            if enabled { c_ink() } else { c_disabled_text() },
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        if enabled && GetFocus() == hwnd {
            let focus = Rect {
                left: scale(1),
                top: scale(1),
                right: rect.right - scale(1),
                bottom: rect.bottom - scale(1),
            };
            DrawFocusRect(dc, &focus);
        }
        EndPaint(hwnd, &paint);
    }
}

fn install_combo_arrow(hwnd: Hwnd, combo: Hwnd) {
    unsafe {
        let original: WndProc = std::mem::transmute(GetWindowLongPtrW(hwnd, GWLP_WNDPROC));
        let visual = Box::new(ComboArrowVisual {
            hover: false,
            combo,
            original,
            last_index: -2,
            last_enabled: false,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(visual) as isize);
        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, combo_arrow_proc as *const () as isize);
    }
}

unsafe extern "system" fn combo_arrow_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut ComboArrowVisual;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    match message {
        WM_PAINT => {
            unsafe { draw_combo_arrow(hwnd) };
            return 0;
        }
        WM_ERASEBKGND => return 1,
        WM_MOUSEMOVE => {
            if !unsafe { (*pointer).hover } {
                unsafe {
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
        }
        WM_MOUSELEAVE => {
            if unsafe { (*pointer).hover } {
                unsafe {
                    (*pointer).hover = false;
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
        }
        0x020a => {
            // WM_MOUSEWHEEL over a closed drop-down scrolls its page instead of
            // stopping there; an open list still scrolls itself.
            let combo = unsafe { (*pointer).combo };
            if unsafe { SendMessageW(combo, CB_GETDROPPEDSTATE, 0, 0) } == 0 {
                let page = unsafe { GetParent(combo) };
                if window_class(page) == "DVD_AUDIO_PAGE_RUST" {
                    return unsafe { SendMessageW(page, message, wparam, lparam) };
                }
            }
        }
        WM_LBUTTONDOWN => return 0,
        WM_LBUTTONUP => {
            let combo = unsafe { (*pointer).combo };
            unsafe {
                if IsWindowEnabled(combo) != 0 {
                    SetFocus(combo);
                    SendMessageW(combo, CB_SHOWDROPDOWN, 1, 0);
                }
            }
            return 0;
        }
        WM_NCDESTROY => {
            let original = unsafe { Box::from_raw(pointer) }.original;
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                SetWindowLongPtrW(hwnd, GWLP_WNDPROC, original as *const () as isize);
            }
            return unsafe { original(hwnd, message, wparam, lparam) };
        }
        _ => {}
    }
    let original = unsafe { (*pointer).original };
    unsafe { original(hwnd, message, wparam, lparam) }
}

unsafe fn draw_combo_arrow(hwnd: Hwnd) {
    let mut paint: PaintStruct = unsafe { std::mem::zeroed() };
    let buffer = unsafe { begin_buffered_paint(hwnd, &mut paint, c_card()) };
    let dc = buffer.memory;
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        GetClientRect(hwnd, &mut rect);
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ComboArrowVisual;
        let combo = if pointer.is_null() {
            ptr::null_mut()
        } else {
            (*pointer).combo
        };
        let hover = !pointer.is_null() && (*pointer).hover;
        let enabled = !combo.is_null() && IsWindowEnabled(combo) != 0;
        let fill = if !enabled {
            c_disabled_fill()
        } else if hover {
            c_secondary_hover()
        } else {
            c_card()
        };
        FillRect(dc, &rect, brush(fill));
        let border = if enabled && GetFocus() == combo {
            c_accent()
        } else {
            c_border()
        };
        let border_width = scale(1).max(1);
        for edge in [
            Rect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.top + border_width,
            },
            Rect {
                left: rect.left,
                top: rect.bottom - border_width,
                right: rect.right,
                bottom: rect.bottom,
            },
            Rect {
                left: rect.left,
                top: rect.top,
                right: rect.left + border_width,
                bottom: rect.bottom,
            },
            Rect {
                left: rect.right - border_width,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            },
        ] {
            FillRect(dc, &edge, brush(border));
        }
        if !combo.is_null() {
            let index = SendMessageW(combo, CB_GETCURSEL, 0, 0) as i32;
            let text = combo_item_text(combo, index);
            let mut text_rect = Rect {
                left: rect.left + scale(8),
                top: rect.top,
                right: rect.right - scale(28),
                bottom: rect.bottom,
            };
            draw_text_run(
                &text,
                dc,
                &mut text_rect,
                ui_font(),
                if enabled { c_ink() } else { c_disabled_text() },
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
            );
            let color = if !enabled {
                c_disabled_text()
            } else if hover || GetFocus() == combo {
                c_accent()
            } else {
                c_ink()
            };
            let pen = CreatePen(0, scale(1).max(1), color);
            if !pen.is_null() {
                let old = SelectObject(dc, pen);
                let center_x = rect.right - scale(13);
                let center_y = (rect.top + rect.bottom) / 2;
                let half = scale(4);
                let drop = scale(2);
                MoveToEx(dc, center_x - half, center_y - drop, ptr::null_mut());
                LineTo(dc, center_x, center_y + drop);
                LineTo(dc, center_x + half, center_y - drop);
                SelectObject(dc, old);
                DeleteObject(pen);
            }
        }
        finish_buffered_paint(hwnd, &paint, &buffer);
    }
}
fn sync_combo_overlays() {
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
        // The overlay is a child of its combo, so only the closed-field size
        // can drift (width changes or a DPI/font change of the item height).
        let mut combo_rect: Rect = unsafe { std::mem::zeroed() };
        let mut overlay_rect: Rect = unsafe { std::mem::zeroed() };
        unsafe {
            GetClientRect(combo, &mut combo_rect);
            GetClientRect(overlay, &mut overlay_rect);
        }
        if overlay_rect.right == combo_rect.right && overlay_rect.bottom == combo_rect.bottom {
            continue;
        }
        unsafe {
            SetWindowPos(
                overlay,
                ptr::null_mut(),
                0,
                0,
                combo_rect.right.max(1),
                combo_rect.bottom.max(1),
                0x0114, // NOZORDER | NOACTIVATE | NOCOPYBITS
            );
            InvalidateRect(overlay, ptr::null(), 0);
        }
    }
}

#[derive(Clone, Copy)]
struct BarMetrics {
    value: i32,
    max: i32,
    page: i32,
}

#[derive(Clone, Copy)]
struct BarGeometry {
    track_height: i32,
    thumb_top: i32,
    thumb_height: i32,
}

fn bar_geometry(client_height: i32, metrics: &BarMetrics) -> BarGeometry {
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
    let mut thumb_height =
        (track_height as f64 * metrics.page as f64 / total) as i32;
    thumb_height = thumb_height.clamp(scale(18).max(8).min(track_height), track_height);
    let travel = (track_height - thumb_height).max(0);
    let ratio = (metrics.value as f64 / metrics.max as f64).clamp(0.0, 1.0);
    BarGeometry {
        track_height,
        thumb_top: track_top + (travel as f64 * ratio) as i32,
        thumb_height,
    }
}

unsafe fn page_bar_metrics(state: &UiState) -> BarMetrics {
    let page = match state.pages.get(state.tab_index) {
        Some(page) => *page,
        None => {
            return BarMetrics {
                value: 0,
                max: 0,
                page: 1,
            }
        }
    };
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        GetClientRect(page, &mut rect);
    }
    let page_height = (rect.bottom - rect.top).max(1);
    let content = page_content_height(page);
    let max = (content - page_height).max(0);
    let value = (unsafe { GetWindowLongPtrW(page, GWLP_USERDATA) } as i32).clamp(0, max);
    BarMetrics {
        value,
        max,
        page: page_height,
    }
}

unsafe fn log_bar_metrics(log: Hwnd) -> BarMetrics {
    let total = (unsafe { SendMessageW(log, EM_GETLINECOUNT, 0, 0) } as i32).max(1);
    let first =
        (unsafe { SendMessageW(log, EM_GETFIRSTVISIBLELINE, 0, 0) } as i32).max(0);
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut frame: Rect = unsafe { std::mem::zeroed() };
    unsafe {
        GetClientRect(log, &mut rect);
        GetClientRect(GetParent(log), &mut frame);
    }
    // While live sizing freezes the EDIT, only the frame's part is visible.
    let visible_height = if frame.bottom > 0 { rect.bottom.min(frame.bottom) } else { rect.bottom };
    let height = (visible_height - rect.top).max(1);
    let line_height = scale(19).max(1);
    let visible = (height / line_height).max(1);
    BarMetrics {
        value: first,
        max: (total - visible).max(0),
        page: visible,
    }
}

unsafe fn install_scrollbar(hwnd: Hwnd, main: Hwnd) {
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

unsafe fn scrollbar_metrics(hwnd: Hwnd, state: &UiState) -> BarMetrics {
    if hwnd == state.page_scrollbar {
        unsafe { page_bar_metrics(state) }
    } else {
        unsafe { log_bar_metrics(state.controls.log) }
    }
}

unsafe fn apply_scrollbar_value(main: Hwnd, hwnd: Hwnd, value: i32) {
    let Some(state) = (unsafe { state(main) }) else {
        return;
    };
    if hwnd == state.page_scrollbar {
        if let Some(page) = state.pages.get(state.tab_index).copied() {
            let metrics = unsafe { page_bar_metrics(state) };
            scroll_page(page, value.clamp(0, metrics.max), true);
        }
    } else if hwnd == state.log_scrollbar {
        let log = state.controls.log;
        if log.is_null() {
            return;
        }
        let metrics = unsafe { log_bar_metrics(log) };
        let target = value.clamp(0, metrics.max);
        if target != metrics.value {
            unsafe {
                SendMessageW(log, EM_LINESCROLL, 0, (target - metrics.value) as isize);
            }
        }
    }
    unsafe {
        InvalidateRect(hwnd, ptr::null(), 0);
    }
}

unsafe fn draw_scrollbar(hwnd: Hwnd) {
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
    }
    unsafe {
        FillRect(dc, &rect, brush(c_body()));
    }
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut ScrollbarVisual;
    if !pointer.is_null() && rect.right > 0 && rect.bottom > 0 {
        let main = unsafe { (*pointer).main };
        if let Some(state) = unsafe { state(main) } {
            let metrics = unsafe { scrollbar_metrics(hwnd, state) };
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
                unsafe {
                    FillRect(dc, &thumb, brush(color));
                }
            }
        }
    }
    unsafe {
        EndPaint(hwnd, &paint);
    }
}

unsafe extern "system" fn scrollbar_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut ScrollbarVisual;
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
            unsafe {
                GetClientRect(hwnd, &mut rect);
            }
            let y = (lparam >> 16) as i16 as i32;
            let main = unsafe { (*pointer).main };
            let mut geometry = None;
            if let Some(state) = unsafe { state(main) } {
                let metrics = unsafe { scrollbar_metrics(hwnd, state) };
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
            unsafe {
                GetClientRect(hwnd, &mut rect);
            }
            let y = (lparam >> 16) as i16 as i32;
            let main = unsafe { (*pointer).main };
            if let Some(state) = unsafe { state(main) } {
                let metrics = unsafe { scrollbar_metrics(hwnd, state) };
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

unsafe fn refresh_dynamic_indicators(state: &UiState) {
    for (hwnd, is_page) in [
        (state.page_scrollbar, true),
        (state.log_scrollbar, false),
    ] {
        if hwnd.is_null() {
            continue;
        }
        let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut ScrollbarVisual;
        if pointer.is_null() {
            continue;
        }
        let metrics = if is_page {
            unsafe { page_bar_metrics(state) }
        } else {
            unsafe { log_bar_metrics(state.controls.log) }
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
        let pointer = unsafe { GetWindowLongPtrW(overlay, GWLP_USERDATA) } as *mut ComboArrowVisual;
        if pointer.is_null() {
            continue;
        }
        let mut index = unsafe { SendMessageW(combo, CB_GETCURSEL, 0, 0) } as i32;
        if index < 0 {
            index = -1;
        }
        let enabled = unsafe { IsWindowEnabled(combo) } != 0;
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
fn create_from_spec(spec: CreateSpec<'_>) -> Hwnd {    let CreateSpec {
        parent,
        class,
        title,
        style,
        x,
        y,
        width,
        height,
        id,
    } = spec;
    let design = (x, y, width, height);
    let class = wide(class);
    let title = wide(title);
    let is_edit = class.as_slice() == wide("EDIT").as_slice();
    let hwnd = unsafe {
        CreateWindowExW(
            if is_edit {
                // Filled field style: the white edit surface sits on the tinted
                // panel, so no 3D client edge is needed.
                0
            } else if class.as_slice() == wide("DVD_AUDIO_PAGE_RUST").as_slice()
                || class.as_slice() == wide("DVD_AUDIO_LOG_FRAME_RUST").as_slice()
            {
                0x00010000 // WS_EX_CONTROLPARENT
            } else {
                0
            },
            class.as_ptr(),
            title.as_ptr(),
            if class.as_slice() == wide("DVD_AUDIO_PAGE_RUST").as_slice() {
                style | WS_CLIPCHILDREN
            } else {
                style
            },
            scale(x),
            scale(y),
            scale(width),
            scale(height),
            parent,
            id as Hmenu,
            GetModuleHandleW(ptr::null()),
            ptr::null_mut(),
        )
    };
    if !hwnd.is_null() {
        if window_class(hwnd) == "Static" {
            #[link(name = "comctl32")]
            unsafe extern "system" {
                fn SetWindowSubclass(hwnd: Hwnd, proc: unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam, usize, usize) -> Lresult, id: usize, data: usize) -> i32;
            }
            unsafe { SetWindowSubclass(hwnd, buffered_label_proc, 2, 0); }
        }
        if window_class(parent) == "DVD_AUDIO_PAGE_RUST" {
            touch::install(hwnd, false);
        }
        let registry = CONTROL_RECTS.get_or_init(|| Mutex::new(Vec::new()));
        if let Ok(mut registry) = registry.lock() {
            registry.push((hwnd as isize, design.0, design.1, design.2, design.3));
        }
        unsafe {
            SendMessageW(hwnd, WM_SETFONT, ui_font() as usize, 1);
            if is_edit {
                // Breathing room inside the borderless field.
                let margin = scale(6);
                SendMessageW(hwnd, EM_SETMARGINS, 3, (margin | (margin << 16)) as isize);
            }
        }
    }
    hwnd
}

fn load_font(height: i32, weight: i32, face: &str) -> Handle {
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

fn font(height: i32, weight: i32, face: u8) -> Handle {
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

fn ui_font() -> Handle {
    font(UI_FONT_HEIGHT, 400, 0)
}
fn semibold_font() -> Handle {
    font(UI_FONT_HEIGHT, 600, 1)
}
fn title_font() -> Handle {
    font(-25, 600, 1)
}
fn subtitle_font() -> Handle {
    font(-14, 400, 0)
}
fn small_font() -> Handle {
    font(-14, 400, 0)
}
fn brush(color: u32) -> Hbrush {
    let cache = BRUSH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(value) = cache.get(&color) {
        return *value as Hbrush;
    }
    let handle = unsafe {
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
fn body_brush() -> Hbrush {
    brush(c_body())
}
fn card_brush() -> Hbrush {
    brush(c_card())
}
fn panel_brush() -> Hbrush {
    brush(c_panel())
}
fn border_brush() -> Hbrush {
    brush(c_border())
}
fn header_icon() -> Handle {
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
            1usize as *const u16,
            1,
            size,
            size,
            0,
        );
        if icon.is_null() {
            LoadIconW(GetModuleHandleW(ptr::null()), 1usize as *const u16)
        } else {
            icon
        }
    };
    cache.insert(current, icon as usize);
    icon
}
fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
fn set_text(hwnd: Hwnd, text: &str) {
    let text = wide(text);
    unsafe {
        SetWindowTextW(hwnd, text.as_ptr());
    }
}
fn get_text(hwnd: Hwnd) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
    let mut value = vec![0u16; len + 1];
    let count =
        unsafe { GetWindowTextW(hwnd, value.as_mut_ptr(), value.len() as i32) }.max(0) as usize;
    String::from_utf16_lossy(&value[..count])
}
fn window_class(hwnd: Hwnd) -> String {
    let mut buffer = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
    if len > 0 {
        String::from_utf16_lossy(&buffer[..len as usize])
    } else {
        String::new()
    }
}
fn check(hwnd: Hwnd, value: bool) {
    unsafe {
        SendMessageW(hwnd, BM_SETCHECK, usize::from(value), 0);
    }
}
fn checked(hwnd: Hwnd) -> bool {
    unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) == 1 }
}
fn combo_add(hwnd: Hwnd, text: &str) {
    let text = wide(text);
    unsafe {
        SendMessageW(hwnd, CB_ADDSTRING, 0, text.as_ptr() as isize);
    }
}
fn combo_select(hwnd: Hwnd, index: i32) {
    unsafe {
        SendMessageW(hwnd, CB_SETCURSEL, index.max(0) as usize, 0);
    }
}
fn combo_items(hwnd: Hwnd, items: &[&str], selected: i32) {
    unsafe {
        SendMessageW(hwnd, CB_RESETCONTENT, 0, 0);
    }
    for item in items {
        combo_add(hwnd, item);
    }
    combo_select(hwnd, selected.min(items.len().saturating_sub(1) as i32));
}
fn combo_index(hwnd: Hwnd) -> i32 {
    unsafe { SendMessageW(hwnd, CB_GETCURSEL, 0, 0) as i32 }
}
fn populate_numeric_choice(hwnd: Hwnd, values: &[i64], labels: &[&str], value: i64) {
    combo_items(hwnd, labels, 0);
    let index = values.iter().position(|v| *v == value).unwrap_or_else(|| {
        // Retain unknown profile values until the user explicitly corrects them.
        combo_add(hwnd, &value.to_string());
        values.len()
    });
    combo_select(hwnd, index as i32);
}
fn numeric_choice(hwnd: Hwnd, values: &[i64]) -> i64 {
    usize::try_from(combo_index(hwnd))
        .ok()
        .and_then(|index| values.get(index).copied())
        .unwrap_or_else(|| parse_i64(hwnd, 0))
}
fn select_tab(state: &mut UiState, index: usize) {
    let index = index.min(state.pages.len().saturating_sub(1));
    state.tab_index = index;
    show_page(state, index as i32);
    for tab in &state.tabs {
        unsafe {
            InvalidateRect(*tab, ptr::null(), 0);
        }
    }
}
fn show_page(state: &UiState, index: i32) {
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

/// Height in DIPs of `text` wrapped at `width` DIPs (at least one line).
fn wrapped_text_height(text: &str, font: Handle, width: i32) -> i32 {
    let dc = unsafe { CreateCompatibleDC(ptr::null_mut()) };
    if dc.is_null() {
        return 20;
    }
    let wide_text = wide(text);
    let mut rect = Rect { left: 0, top: 0, right: scale(width).max(1), bottom: 0 };
    unsafe {
        let old = SelectObject(dc, font);
        DrawTextW(dc, wide_text.as_ptr(), -1, &mut rect, 0x0010 | DT_CALCRECT | DT_NOPREFIX); // DT_WORDBREAK
        SelectObject(dc, old);
        DeleteDC(dc);
    }
    unscale(rect.bottom).max(20)
}

fn measured_text_width(_hwnd: Hwnd, text: &str, font: Handle) -> i32 {
    // Measure on a memory DC so layout never touches a window DC mid-resize.
    let dc = unsafe { CreateCompatibleDC(ptr::null_mut()) };
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

/// Control moves recorded while a layout batch is open. Only the last
/// target of each control counts: layout code may position a control more
/// than once, and a DeferWindowPos batch must not contain the same window
/// twice (Windows then keeps the first entry, so consecutive layouts would
/// alternate between both positions). The batch end applies every move with
/// one BeginDeferWindowPos per parent, recomputing visible regions once
/// instead of after every control.
#[derive(Default)]
struct PlacementBatch {
    /// (control, parent, target client rectangle) in first-recorded order.
    pending: Vec<(Hwnd, Hwnd, Rect)>,
}

thread_local! {
    static PLACEMENT_BATCH: std::cell::RefCell<Option<PlacementBatch>> = const { std::cell::RefCell::new(None) };
}

const PLACE_FLAGS: u32 = 0x011C; // NOZORDER | NOREDRAW | NOACTIVATE | NOCOPYBITS

/// Starts batching control moves until `end_placement_batch`. Code in
/// between must not read back the geometry of controls it moved.
fn begin_placement_batch() {
    PLACEMENT_BATCH.with(|batch| {
        let mut batch = batch.borrow_mut();
        debug_assert!(batch.is_none(), "placement batches do not nest");
        *batch = Some(PlacementBatch::default());
    });
}

/// Whether the control already occupies `target` in its parent's client
/// coordinates. Closed combo boxes keep their own height; only the
/// drop-down height is requested, so it is not compared.
fn placement_matches(hwnd: Hwnd, parent: Hwnd, target: &Rect) -> bool {
    let mut rect: Rect = unsafe { std::mem::zeroed() };
    unsafe { GetWindowRect(hwnd, &mut rect) };
    let mut origin = Point { x: rect.left, y: rect.top };
    unsafe { ScreenToClient(parent, &mut origin) };
    let fixed_combo_height = COMBO_IDS.contains(&(unsafe { GetDlgCtrlID(hwnd) } as usize));
    origin.x == target.left
        && origin.y == target.top
        && rect.right - rect.left == target.right - target.left
        && (fixed_combo_height || rect.bottom - rect.top == target.bottom - target.top)
}

/// Applies every recorded move at once.
fn end_placement_batch() {
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

fn apply_placement(hwnd: Hwnd, target: Rect) {
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
fn place_log(state: &mut UiState, target: Rect, revealed: bool) {
    place_px(state.log_frame, target);
    let log = state.controls.log;
    let mut current: Rect = unsafe { std::mem::zeroed() };
    unsafe { GetWindowRect(log, &mut current) };
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
unsafe extern "system" fn log_frame_proc(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult {
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

static LOG_FRAME_CLASS_READY: OnceLock<()> = OnceLock::new();

fn create_log_frame(parent: Hwnd) -> Hwnd {
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

unsafe fn place(hwnd: Hwnd, x: i32, y: i32, width: i32, height: i32) {
    place_px(hwnd, Rect {
        left: scale(x),
        top: scale(y),
        right: scale(x) + scale(width),
        bottom: scale(y) + scale(height),
    });
}

/// Like place(), with a target rectangle in pixels.
fn place_px(hwnd: Hwnd, target: Rect) {
    let parent = unsafe { GetParent(hwnd) };
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

unsafe extern "system" fn font_child_proc(hwnd: Hwnd, parameter: Lparam) -> i32 {
    unsafe {
        SendMessageW(hwnd, WM_SETFONT, parameter as usize, 1);
    }
    1
}

fn refresh_fonts(hwnd: Hwnd, state: &UiState) {
    unsafe {
        EnumChildWindows(hwnd, font_child_proc, ui_font() as usize as Lparam);
        SendMessageW(
            state.controls.status,
            WM_SETFONT,
            semibold_font() as usize,
            1,
        );
        SendMessageW(
            state.controls.statistics,
            WM_SETFONT,
            small_font() as usize,
            1,
        );
        SendMessageW(state.controls.activity, WM_SETFONT, ui_font() as usize, 1);
        SendMessageW(state.advanced_hint, WM_SETFONT, small_font() as usize, 1);
    }
}

fn rescale_ui(hwnd: Hwnd, state: &mut UiState) {
    if let Some(registry) = CONTROL_RECTS.get() {
        if let Ok(registry) = registry.lock() {
            for (handle, x, y, width, height) in registry.iter() {
                // layout() places the log and its frame; moving the EDIT here
                // would look like a user scroll and pause live follow.
                if *handle as Hwnd == state.controls.log || *handle as Hwnd == state.log_frame {
                    continue;
                }
                unsafe {
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
    }
    refresh_fonts(hwnd, state);
    layout(hwnd, state);
}

unsafe extern "system" fn theme_child_proc(hwnd: Hwnd, _parameter: Lparam) -> i32 {
    uxtheme_allow_dark(hwnd, dark_theme());
    let class = window_class(hwnd);
    if class == "Edit" || class == "ComboBox" || class == "DVD_AUDIO_PAGE_RUST" {
        let dark_name = wide("DarkMode_Explorer");
        unsafe {
            SetWindowTheme(
                hwnd,
                if dark_theme() {
                    dark_name.as_ptr()
                } else {
                    ptr::null()
                },
                ptr::null(),
            );
        }
    }
    1
}

fn apply_theme(hwnd: Hwnd, state: &mut UiState, dark: bool) {
    DARK_THEME.store(dark, Ordering::Relaxed);
    uxtheme_preferred_app_mode(dark);
    unsafe {
        let dark_flag: u32 = if dark { 1 } else { 0 };
        DwmSetWindowAttribute(
            hwnd,
            20,
            &dark_flag as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );
        DwmSetWindowAttribute(
            hwnd,
            19,
            &dark_flag as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );
        EnumChildWindows(hwnd, theme_child_proc, 0);
        let empty = wide("");
        SetWindowTheme(state.controls.progress, empty.as_ptr(), empty.as_ptr());
        SendMessageW(
            state.controls.progress,
            PBM_SETBKCOLOR,
            0,
            c_bar_track() as isize,
        );
        SendMessageW(
            state.controls.progress,
            PBM_SETBARCOLOR,
            0,
            c_accent() as isize,
        );
        if !state.tooltip_window.is_null() {
            let dark_name = wide("DarkMode_Explorer");
            SetWindowTheme(
                state.tooltip_window,
                if dark { dark_name.as_ptr() } else { ptr::null() },
                ptr::null(),
            );
        }
        RedrawWindow(
            hwnd,
            ptr::null(),
            ptr::null_mut(),
            RDW_INVALIDATE | RDW_ALLCHILDREN,
        );
    }
}

fn toggle_theme(hwnd: Hwnd, state: &mut UiState) {
    let next = !dark_theme();
    // A manual choice overrides the system only for this session.
    state.theme_override = Some(next);
    apply_theme(hwnd, state, next);
}

/// Settings height before the first full-size layout, in DIPs.
const DEFAULT_SETTINGS_HEIGHT: i32 = 300;
/// The log shows at least one line.
const MIN_LOG_HEIGHT: i32 = 28;
/// The default split shrinks the settings before the log drops below this.
const DEFAULT_LOG_FLOOR: i32 = 60;

fn layout(hwnd: Hwnd, state: &mut UiState) {
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
    unsafe {
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
    let profile_label_width = unscale(measured_text_width(hwnd, tr(state.lang, "profile"), ui_font())) + 8;
    let profile_x = margin + profile_label_width + 6;
    let stacked_profile = header_x[0] - 12 - profile_x < 120;
    let action_toolbar_y = toolbar_y + if stacked_profile { 34 } else { 0 };
    let tabs_y = tabs_y + if stacked_profile { 34 } else { 0 };
    // The log toolbar is sized from its translated texts; narrow windows
    // tighten its spacing first and wrap copy/export only when necessary.
    let text_width = |key| unscale(measured_text_width(hwnd, tr(state.lang, key), ui_font()));
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
    let page = state.pages[state.tab_index.min(state.pages.len() - 1)];
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
    state.splitter_position = if compact { i32::MIN / 4 } else { scale(splitter) };
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
            let shown = !task_only && index == state.tab_index;
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
            scroll_page(*page, 0, false);
        }
        begin_placement_batch();
        for page in &state.pages {
            resize_page_fields(*page);
        }
        layout_page_contents(state);
        end_placement_batch();
        // The scroll range follows the content just laid out.
        for page in &state.pages {
            scroll_page(*page, 0, false);
        }
        // Laying out the pages can change the current page's content height
        // (narrow layout, shown fields); fit the settings to it once more.
        static REFITTING: AtomicBool = AtomicBool::new(false);
        let fitted = unscale(page_content_height(state.pages[state.tab_index.min(state.pages.len() - 1)])) + 44;
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

fn label_handle(state: &UiState, key: &str) -> Hwnd {
    state
        .labels
        .iter()
        .find(|(_, label_key)| *label_key == key)
        .map(|(handle, _)| *handle)
        .unwrap_or(ptr::null_mut())
}

unsafe fn move_child(page: Hwnd, id: usize, x: i32, y: i32, width: i32, height: i32) {
    let control = unsafe { GetDlgItem(page, id as i32) };
    if !control.is_null() {
        unsafe { place(control, x, y, width, height) };
    }
}

unsafe fn move_label(state: &UiState, key: &str, x: i32, y: i32, width: i32, height: i32) {
    let handle = label_handle(state, key);
    if !handle.is_null() {
        unsafe { place(handle, x, y, width, height) };
    }
}

unsafe fn layout_page_contents(state: &UiState) {
    for (index, page) in state.pages.iter().enumerate() {
        let mut rect = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe { GetClientRect(*page, &mut rect) };
        let page_width = unscale(rect.right).max(1);
        let scroll = unsafe { GetWindowLongPtrW(*page, GWLP_USERDATA) } as i32;
        let sy = unscale(scroll);
        let narrow = page_is_narrow(page_width);
        match index {
            0 => {
                if narrow {
                    let field_width = page_width - 28;
                    unsafe {
                        move_label(state, "title", 14, 106 - sy, field_width, 20);
                        move_child(*page, ID_TITLE, 14, 126 - sy, field_width, 24);
                        move_label(state, "capacity", 14, 150 - sy, 220, 20);
                        move_child(*page, ID_CAPACITY, 14, 170 - sy, 220, 120);
                        let custom_x = 250;
                        let custom_width = (page_width - custom_x - 14).max(160);
                        place(
                            state.custom_bytes_label,
                            custom_x,
                            150 - sy,
                            custom_width,
                            20,
                        );
                        move_child(
                            *page,
                            ID_CUSTOM_BYTES,
                            custom_x,
                            170 - sy,
                            custom_width,
                            25,
                        );
                        let hint = get_text(label_handle(state, "advanced_hint"));
                        move_label(state, "advanced_hint", 14, 204 - sy, field_width, wrapped_text_height(&hint, ui_font(), field_width));
                        move_label(state, "work_dir", 14, 214 - sy, field_width, 20);
                        move_child(
                            *page,
                            ID_WORK_DIR,
                            14,
                            234 - sy,
                            (field_width - 100).max(120),
                            25,
                        );
                        move_child(
                            *page,
                            ID_BROWSE_WORK,
                            page_width - 14 - 90,
                            234 - sy,
                            90,
                            24,
                        );
                        move_label(state, "iso_prefix", 14, 268 - sy, field_width, 20);
                        move_child(*page, ID_ISO_PREFIX, 14, 288 - sy, field_width, 25);
                        move_label(state, "group_limit", 14, 322 - sy, field_width, 20);
                        move_child(*page, ID_GROUP_LIMIT, 14, 342 - sy, field_width, 25);
                        place(state.controls.keep_files, 14, 376 - sy, 230, 28);
                    }
                } else {
                    unsafe {
                        move_label(state, "title", 14, 106 - sy, 470, 20);
                        move_child(*page, ID_TITLE, 14, 126 - sy, 470, 24);
                        move_label(state, "capacity", 520, 106 - sy, 220, 20);
                        move_child(*page, ID_CAPACITY, 520, 126 - sy, 220, 120);
                        place(state.custom_bytes_label, 760, 106 - sy, page_width - 774, 20);
                        move_child(*page, ID_CUSTOM_BYTES, 760, 126 - sy, page_width - 774, 25);
                        let hint = get_text(label_handle(state, "advanced_hint"));
                        move_label(state, "advanced_hint", 14, 166 - sy, 780, wrapped_text_height(&hint, ui_font(), 780));
                        move_label(state, "work_dir", 14, 150 - sy, 520, 20);
                        move_label(state, "iso_prefix", 14, 194 - sy, 460, 20);
                        move_child(*page, ID_ISO_PREFIX, 14, 214 - sy, 460, 25);
                        move_label(state, "group_limit", 520, 194 - sy, 220, 20);
                        move_child(*page, ID_GROUP_LIMIT, 520, 214 - sy, 220, 25);
                        place(state.controls.keep_files, 760, 194 - sy, 230, 28);
                    }
                }
            }
            1 => {
                if narrow {
                    let field_width = page_width - 28;
                    unsafe {
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
                    unsafe {
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
                unsafe {
                    place(state.controls.menu, 14, 18 - sy, menu_width, 28);
                    place(state.controls.stills, stills_x, 18 - sy, stills_width, 28);
                }
                if narrow {
                    let column = (page_width - 28 - 16) / 2;
                    unsafe {
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
                        unsafe {
                            move_label(state, label_key, x, 120 - sy, font_column, font_label_height);
                            move_child(*page, field_id, x, 120 + font_label_height - sy, font_column, 25);
                            move_child(*page, browse_id, x, 154 + font_label_height - sy, font_column, 28);
                        }
                    }
                } else {
                    unsafe {
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
                        unsafe {
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

fn color16(value: u32) -> u16 {
    (value.min(255) * 257) as u16
}
fn rgb_components(color: u32) -> (u32, u32, u32) {
    (color & 0xff, (color >> 8) & 0xff, (color >> 16) & 0xff)
}

unsafe fn draw_text_run(
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

unsafe fn fill_round_rect(dc: Handle, rect: &Rect, radius: i32, fill: u32, border: u32) {
    let outer = unsafe { CreateSolidBrush(border) };
    if outer.is_null() {
        return;
    }
    let inner = unsafe { CreateSolidBrush(fill) };
    if inner.is_null() {
        unsafe { DeleteObject(outer) };
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

unsafe fn draw_secondary_button(item: &DrawItemStruct) {
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

unsafe fn draw_action_button(item: &DrawItemStruct, primary: bool) {
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

unsafe fn draw_tab_button(item: &DrawItemStruct, active: bool) {
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

fn header_gradient_at(y: i32) -> u32 {
    let height = header_height().max(1);
    let ratio = y.clamp(0, height) as f64 / height as f64;
    let (top_r, top_g, top_b) = rgb_components(c_header_top());
    let (bottom_r, bottom_g, bottom_b) = rgb_components(c_header_bottom());
    let mix = |top: u32, bottom: u32| -> u32 {
        (top as f64 + (bottom as f64 - top as f64) * ratio) as u32
    };
    rgb(mix(top_r, bottom_r), mix(top_g, bottom_g), mix(top_b, bottom_b))
}

fn lighten(color: u32, amount: i32) -> u32 {
    let (red, green, blue) = rgb_components(color);
    let adjust = |value: u32| (value as i32 + amount).clamp(0, 255) as u32;
    rgb(adjust(red), adjust(green), adjust(blue))
}

unsafe fn draw_caption_button(item: &DrawItemStruct, kind: u8) {
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

unsafe fn stroke_rect(dc: Handle, rect: &Rect) {
    unsafe {
        MoveToEx(dc, rect.left, rect.top, ptr::null_mut());
        LineTo(dc, rect.right, rect.top);
        LineTo(dc, rect.right, rect.bottom);
        LineTo(dc, rect.left, rect.bottom);
        LineTo(dc, rect.left, rect.top);
    }
}

unsafe fn combo_item_text(hwnd: Hwnd, index: i32) -> String {
    if index < 0 {
        return String::new();
    }
    let length = unsafe { SendMessageW(hwnd, CB_GETLBTEXTLEN, index as usize, 0) };
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

unsafe fn draw_combo_item(item: &DrawItemStruct) {
    let selected = item.item_id == u32::MAX;
    let highlighted = item.state & ODS_SELECTED != 0;
    let disabled = item.state & ODS_DISABLED != 0;
    let index = if selected {
        unsafe { SendMessageW(item.item, CB_GETCURSEL, 0, 0) as i32 }
    } else {
        item.item_id as i32
    };
    let text = unsafe { combo_item_text(item.item, index) };
    let background = if highlighted && !selected {
        c_accent_soft()
    } else {
        c_card()
    };
    unsafe {
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

unsafe fn draw_owner_item(hwnd: Hwnd, item: &DrawItemStruct) -> bool {
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

unsafe fn draw_owner_item_contents(hwnd: Hwnd, item: &DrawItemStruct) -> bool {
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
        let active = unsafe { state(hwnd) }
            .map(|state| state.tab_index == id - ID_TAB)
            .unwrap_or(false);
        unsafe { draw_tab_button(item, active) };
        return true;
    }
    unsafe { draw_secondary_button(item) };
    true
}

struct BufferedPaint {
    target: Handle,
    memory: Handle,
    bitmap: Handle,
    previous: Handle,
    rect: Rect,
}

unsafe fn begin_buffered_paint(
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
    unsafe {
        GetClientRect(hwnd, &mut rect);
    }
    let width = rect.right.max(1);
    let height = rect.bottom.max(1);
    let memory = unsafe { CreateCompatibleDC(target) };
    let bitmap = unsafe { CreateCompatibleBitmap(target, width, height) };
    if memory.is_null() || bitmap.is_null() {
        if !bitmap.is_null() {
            unsafe { DeleteObject(bitmap) };
        }
        if !memory.is_null() {
            unsafe { DeleteDC(memory) };
        }
        return BufferedPaint {
            target,
            memory: target,
            bitmap: ptr::null_mut(),
            previous: ptr::null_mut(),
            rect,
        };
    }
    let previous = unsafe { SelectObject(memory, bitmap) };
    unsafe {
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

unsafe fn finish_buffered_paint(hwnd: Hwnd, paint: &PaintStruct, buffer: &BufferedPaint) {
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
unsafe fn paint_window(hwnd: Hwnd) {
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
        let lang = state(hwnd).map(|state| state.lang).unwrap_or(Lang::Zh);
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
        let splitter = state(hwnd)
            .map(|state| state.splitter_position)
            .unwrap_or(0);
        if splitter > header && splitter < rect.bottom - scale(60) {
            let divider = Rect {
                left: scale(18),
                top: splitter - 1,
                right: width - scale(18),
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

unsafe extern "system" fn buffered_label_proc(
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
unsafe fn text_lift(dc: Handle, needed: i32, available: i32) -> i32 {
    let overflow = needed - available;
    if overflow <= 0 {
        return 0;
    }
    let mut metrics: TextMetric = unsafe { std::mem::zeroed() };
    unsafe { GetTextMetricsW(dc, &mut metrics) };
    overflow.min(metrics.internal_leading.max(0))
}

/// Paints a STATIC label with the native text semantics (alignment, word
/// wrap, prefixes, ellipsis styles) into the label's paint buffer.
unsafe fn draw_label(hwnd: Hwnd, dc: Handle) {
    const DT_WORDBREAK: u32 = 0x0010;
    const DT_EXPANDTABS: u32 = 0x0040;
    const DT_END_ELLIPSIS: u32 = 0x8000;
    unsafe {
        let mut rect: Rect = std::mem::zeroed();
        GetClientRect(hwnd, &mut rect);
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

/// Status lines show one line and end in "..." instead of wrapping into
/// an invisible second line.
fn single_line_label(hwnd: Hwnd) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, -16) as u32;
        // SS_LEFTNOWORDWRAP | SS_NOPREFIX | SS_ENDELLIPSIS
        SetWindowLongPtrW(hwnd, -16, ((style & !0x1f) | 0x0c | 0x0080 | 0x4000) as isize);
    }
}

unsafe fn paint_page(hwnd: Hwnd) {
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

fn handle_command(hwnd: Hwnd, state: &mut UiState, id: usize, notify: u16) {
    handle_command_inner(hwnd, state, id, notify);
    // Tabs, the advanced switch and the audio mode change which fields the
    // current page shows.
    refit_settings(state);
}

/// Fits the settings to the current page's content again if it changed.
fn refit_settings(state: &mut UiState) {
    if state.splitter_height.is_some() || state.main_window.is_null() {
        return;
    }
    let page = state.pages[state.tab_index.min(state.pages.len() - 1)];
    let fitted = unscale(page_content_height(page)) + 44;
    let mut client: Rect = unsafe { std::mem::zeroed() };
    unsafe { GetClientRect(state.main_window, &mut client) };
    if Some(fitted) != state.default_tab_height && client.bottom > 0 {
        layout(state.main_window, state);
    }
}

fn handle_command_inner(hwnd: Hwnd, state: &mut UiState, id: usize, notify: u16) {
    if (ID_MINIMIZE..=ID_THEME).contains(&id) && notify == BN_CLICKED {
        match id {
            ID_MINIMIZE => unsafe {
                ShowWindow(hwnd, SW_MINIMIZE);
            },
            ID_MAXIMIZE => unsafe {
                if IsZoomed(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                } else {
                    ShowWindow(hwnd, SW_MAXIMIZE);
                }
            },
            ID_CLOSE => unsafe {
                SendMessageW(hwnd, WM_CLOSE, 0, 0);
            },
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
                1 => Lang::En,
                2 => Lang::Ja,
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

fn set_language(state: &mut UiState, lang: Lang) {
    state.lang = lang;
    state.statistics_text.clear();
    combo_select(
        state.controls.language,
        match lang {
            Lang::Zh => 0,
            Lang::En => 1,
            Lang::Ja => 2,
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

fn capacity_index(bytes: i64) -> i32 {
    match bytes {
        0 | 4_707_319_808 => 0,
        8_540_123_136 => 1,
        _ => 2,
    }
}
fn mode_index(mode: &str) -> i32 {
    match mode {
        "lpcm" => 1,
        "external" => 2,
        _ => 0,
    }
}
fn mode_value(index: i32) -> &'static str {
    match index {
        1 => "lpcm",
        2 => "external",
        _ => "mlp-batch",
    }
}
fn update_choices(state: &UiState) {
    let mode = combo_index(state.controls.mode);
    let imported = mode == 2;
    set_text(
        state.controls.audio_note,
        tr(state.lang, audio_note_key(mode)),
    );
    let custom_capacity = combo_index(state.controls.capacity) == 2;
    unsafe {
        ShowWindow(
            state.custom_bytes_label,
            if custom_capacity { SW_SHOW } else { SW_HIDE },
        );
        ShowWindow(
            state.controls.custom_bytes,
            if custom_capacity { SW_SHOW } else { SW_HIDE },
        );
        ShowWindow(
            state.import_folder_label,
            if imported { SW_SHOW } else { SW_HIDE },
        );
        ShowWindow(
            state.controls.import_folder,
            if imported { SW_SHOW } else { SW_HIDE },
        );
        ShowWindow(
            state.import_browse,
            if imported { SW_SHOW } else { SW_HIDE },
        );
        EnableWindow(state.controls.custom_bytes, i32::from(custom_capacity));
        EnableWindow(state.controls.import_folder, i32::from(imported));
        for hwnd in [
            state.controls.rate,
            state.controls.bits,
        ] {
            EnableWindow(hwnd, i32::from(!imported));
        }
    }
}

fn audio_note_key(mode: i32) -> &'static str {
    match mode {
        1 => "lpcm_note",
        2 => "import_note",
        _ => "mlp_note",
    }
}

fn raw_or_environment(values: &Map<String, Value>, key: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            values
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into()
        })
}

fn populate(state: &mut UiState, options: &AppOptions) {
    let raw = options.profile_values();
    set_text(state.controls.source, &options.text("SourceDirectory"));
    set_text(state.controls.final_dir, &options.text("FinalDirectory"));
    set_text(state.controls.title, &options.text("Title"));
    set_combo_value(
        state.controls.capacity,
        capacity_index(options.integer("DiscBytes")),
    );
    set_text(
        state.controls.custom_bytes,
        &options.integer("DiscBytes").to_string(),
    );
    set_text(state.controls.work_dir, &options.text("BuildDirectory"));
    set_text(
        state.controls.iso_prefix,
        &raw_or_environment(&raw, "DVDA_ISO_PREFIX"),
    );
    set_text(
        state.controls.group_limit,
        &options.integer("GroupTrackLimit").to_string(),
    );
    combo_select(state.controls.mode, mode_index(&options.text("MlpSource")));
    set_text(
        state.controls.import_folder,
        &raw_or_environment(&raw, "DVDA_MLP_EXTERNAL_DIR"),
    );
    populate_numeric_choice(
        state.controls.rate,
        SAMPLE_RATES,
        SAMPLE_RATE_LABELS,
        options.integer("MlpSampleRate"),
    );
    populate_numeric_choice(
        state.controls.bits,
        SAMPLE_BITS,
        &["16", "20", "24"],
        options.integer("MlpBits"),
    );
    check(state.controls.menu, options.boolean("MenuEnabled"));
    check(state.controls.stills, options.boolean("MenuStillPictures"));
    set_text(
        state.controls.tracks,
        &options.integer("MenuTracksPerPage").to_string(),
    );
    set_text(
        state.controls.cover,
        &options.integer("MenuCoverDim").to_string(),
    );

    let menu_font = options.text("MenuFont");
    let menu_font_japanese = {
        let configured = options.text("MenuFontJapanese");
        if configured.trim().is_empty() {
            menu_font.clone()
        } else {
            configured
        }
    };
    let menu_font_korean = {
        let configured = options.text("MenuFontKorean");
        if configured.trim().is_empty() {
            menu_font.clone()
        } else {
            configured
        }
    };
    set_text(state.controls.font_sc, &menu_font);
    set_text(state.controls.font_jp, &menu_font_japanese);
    set_text(state.controls.font_kr, &menu_font_korean);


    check(
        state.controls.keep_files,
        options.boolean("KeepTemporary") || options.boolean("KeepIntermediate"),
    );

    update_choices(state);
}

fn set_combo_value(hwnd: Hwnd, index: i32) {
    combo_select(hwnd, index);
}

fn overrides(state: &UiState) -> Map<String, Value> {
    let mut values = Map::new();
    let put = |values: &mut Map<String, Value>, key: &str, value: Value| {
        values.insert(key.into(), value);
    };
    put(
        &mut values,
        "SourceDirectory",
        json!(get_text(state.controls.source)),
    );
    put(
        &mut values,
        "FinalDirectory",
        json!(get_text(state.controls.final_dir)),
    );
    put(&mut values, "Title", json!(get_text(state.controls.title)));
    put(
        &mut values,
        "DiscBytes",
        json!(match combo_index(state.controls.capacity) {
            1 => 8_540_123_136i64,
            2 => parse_i64(state.controls.custom_bytes, 4_707_319_808i64),
            _ => 4_707_319_808i64,
        }),
    );
    put(
        &mut values,
        "BuildDirectory",
        json!(get_text(state.controls.work_dir)),
    );
    put(
        &mut values,
        "IsoPrefix",
        json!(get_text(state.controls.iso_prefix)),
    );
    put(
        &mut values,
        "GroupTrackLimit",
        json!(parse_i64(state.controls.group_limit, 99)),
    );
    put(
        &mut values,
        "PrepareCacheEnabled",
        json!(true),
    );
    put(
        &mut values,
        "ResumeEnabled",
        json!(true),
    );
    put(
        &mut values,
        "MlpSource",
        json!(mode_value(combo_index(state.controls.mode))),
    );
    put(
        &mut values,
        "MlpExternalDirectory",
        json!(get_text(state.controls.import_folder)),
    );
    put(
        &mut values,
        "MlpSampleRate",
        json!(numeric_choice(state.controls.rate, SAMPLE_RATES)),
    );
    put(
        &mut values,
        "MlpBits",
        json!(numeric_choice(state.controls.bits, SAMPLE_BITS)),
    );

    put(
        &mut values,
        "MenuEnabled",
        json!(checked(state.controls.menu)),
    );
    put(
        &mut values,
        "MenuStillPictures",
        json!(checked(state.controls.stills)),
    );
    put(
        &mut values,
        "MenuTracksPerPage",
        json!(parse_i64(state.controls.tracks, 12)),
    );
    put(
        &mut values,
        "MenuCoverDim",
        json!(parse_i64(state.controls.cover, 35)),
    );

    put(
        &mut values,
        "MenuFont",
        json!(get_text(state.controls.font_sc)),
    );
    put(
        &mut values,
        "MenuFontJapanese",
        json!(get_text(state.controls.font_jp)),
    );
    put(
        &mut values,
        "MenuFontKorean",
        json!(get_text(state.controls.font_kr)),
    );

    let keep_files = checked(state.controls.keep_files);
    put(&mut values, "KeepTemporary", json!(keep_files));
    put(&mut values, "KeepIntermediate", json!(keep_files));
    values
}

fn update_advanced(state: &UiState) {
    let show = checked(state.controls.advanced);
    for control in &state.advanced_controls {
        unsafe {
            ShowWindow(*control, if show { SW_SHOW } else { SW_HIDE });
        }
    }
    unsafe {
        ShowWindow(state.advanced_hint, if show { SW_HIDE } else { SW_SHOW });
    }
}
fn update_statistics(state: &mut UiState) {
    let elapsed = state
        .started
        .map_or(state.elapsed_seconds, |s| s.elapsed().as_secs());
    let problems = state.problem_count;
    let text = tr(state.lang, "statistics")
        .replace("{0}", &(elapsed / 3600).to_string())
        .replace("{1}", &format!("{:02}", elapsed / 60 % 60))
        .replace("{2}", &format!("{:02}", elapsed % 60))
        .replace("{3}", &problems.to_string());
    let display = localized_detail(state.lang, &text);
    if state.statistics_text != display {
        set_text(state.controls.statistics, &display);
        state.statistics_text = display;
    }
}
fn valid_integer(value: &str, min: i64, max: i64) -> bool {
    value
        .trim()
        .parse::<i64>()
        .is_ok_and(|n| (min..=max).contains(&n))
}
fn validate_controls(state: &mut UiState, for_task: bool) -> bool {
    let c = &state.controls;
    let mut error = None;
    if for_task && !SAMPLE_RATES.contains(&numeric_choice(c.rate, SAMPLE_RATES)) {
        error = Some((c.rate, tr(state.lang, "invalid_sample_rate").into()));
    } else if for_task && !SAMPLE_BITS.contains(&numeric_choice(c.bits, SAMPLE_BITS)) {
        error = Some((c.bits, tr(state.lang, "invalid_bits").into()));
    }
    for (control, key, min, max) in [
        (c.group_limit, "group_limit", 1, 99),
        (c.tracks, "tracks", 1, 32),
        (c.cover, "cover", 0, 100),
    ] {
        if error.is_none() && !valid_integer(&get_text(control), min, max) {
            error = Some((
                control,
                format!(
                    "{}：{}",
                    tr(state.lang, key),
                    tr(state.lang, "invalid_integer")
                        .replace("{0}", &min.to_string())
                        .replace("{1}", &max.to_string())
                ),
            ));
            break;
        }
    }
    if error.is_none()
        && combo_index(c.capacity) == 2
        && !valid_integer(&get_text(c.custom_bytes), 0, 10_000_000_000)
    {
        error = Some((
            c.custom_bytes,
            format!(
                "{}：{}",
                tr(state.lang, "custom_bytes"),
                tr(state.lang, "invalid_integer")
                    .replace("{0}", "0")
                    .replace("{1}", "10000000000")
            ),
        ));
    }
    if let Some((control, message)) = error {
        check(state.controls.advanced, true);
        update_advanced(state);
        if let Some(index) = state
            .pages
            .iter()
            .position(|p| *p == unsafe { GetParent(control) })
        {
            select_tab(state, index);
        }
        unsafe {
            SetFocus(control);
            SendMessageW(control, 0x00b1, 0, -1);
        }
        add_log(state, &message, true);
        false
    } else {
        true
    }
}

fn parse_i64(hwnd: Hwnd, fallback: i64) -> i64 {
    get_text(hwnd).trim().parse().unwrap_or(fallback)
}

fn save_profile(state: &mut UiState) {
    if !validate_controls(state, false) {
        return;
    }
    let mut path = PathBuf::from(get_text(state.controls.profile).trim());
    if path.as_os_str().is_empty() {
        path = dvda_core::app::default_profile_path();
    }
    if path.extension().is_none() {
        path.set_extension("json");
    }
    let values = edited_profile_values(state);
    match dvda_core::app::save_profile(&path, &values, state.lang.code()) {
        Ok(()) => {
            set_text(state.controls.profile, &path.to_string_lossy());
            set_text(state.controls.profile_origin, &path.to_string_lossy());
            state.profile_path = path.clone();
            state.base = AppOptions::load(Some(&path))
                .ok()
                .or_else(|| AppOptions::load(None).ok());
            add_log(
                state,
                &format!("{}{}", tr(state.lang, "saved"), path.display()),
                false,
            );
        }
        Err(error) => add_log(
            state,
            &format!("{}{}", tr(state.lang, "profile_error"), error),
            true,
        ),
    }
}

fn autosave(state: &mut UiState) {
    let path = dvda_core::app::default_profile_path();
    let values = edited_profile_values(state);
    if let Err(error) = dvda_core::app::save_profile(&path, &values, state.lang.code()) {
        add_log(state, &format!("{}{}", tr(state.lang, "profile_error"), error), true);
    }
}
fn save_profile_as(hwnd: Hwnd, state: &mut UiState) {
    if !validate_controls(state, false) {
        return;
    }
    if let Some(path) = file_dialog(
        hwnd,
        true,
        &get_text(state.controls.profile),
        tr(state.lang, "save_as"),
        state.lang,
        FileKind::Profile,
    ) {
        set_text(state.controls.profile, &path);
        save_profile(state);
        autosave(state);
    }
}
fn open_session_log() -> std::io::Result<(fs::File, PathBuf)> {
    let root = dvda_core::app::default_profile_path()
        .parent()
        .ok_or_else(|| std::io::Error::other("Missing application directory"))?
        .to_owned();
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = root.join(format!(
        "gui-{timestamp}-{}-{}.log",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let file = fs::OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(&path)?;
    prune_session_logs(&root, &path)?;
    Ok((file, path))
}

fn prune_session_logs(root: &std::path::Path, current: &std::path::Path) -> std::io::Result<()> {
    let mut previous = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(identifier) = name.strip_prefix("gui-").and_then(|s| s.strip_suffix(".log")) else {
            continue;
        };
        let parts: Vec<_> = identifier.split('-').collect();
        if parts.len() != 3 || parts.iter().any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit())) {
            continue;
        }
        if entry.path() != current && entry.file_type()?.is_file() {
            previous.push((entry.metadata()?.modified()?, entry.path()));
        }
    }
    previous.sort_by(|a, b| b.cmp(a));
    for (_, path) in previous.into_iter().skip(2) {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn log_timestamp() -> String {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(value: *mut [u16; 8]);
    }
    let mut value = [0u16; 8];
    unsafe {
        GetLocalTime(&mut value);
    }
    format!("{:02}:{:02}:{:02}", value[4], value[5], value[6])
}

fn reset_task_log(state: &mut UiState) {
    state.logs.clear();
    state.summary_logs.clear();
    state.log_dirty = false;
    state.log_rendered_count = 0;
    state.log_render_language = None;
    state.log_first_visible_line = 0;
    state.log_hold_line = None;
    state.log_hold_until = None;
    state.activity_dirty = false;
    state.problem_count = 0;
    if state.log_file.is_none() {
        match open_session_log() {
            Ok(file) => state.log_file = Some(file),
            Err(error) => add_log(state, &format!("Cannot save the full log: {error}"), true),
        }
    }
    set_text(state.controls.log, "");
}

fn edited_profile_values(state: &UiState) -> Map<String, Value> {
    let mut values = state
        .base
        .as_ref()
        .map(AppOptions::profile_values)
        .unwrap_or_default();

    for (key, value) in overrides(state) {
        let raw = match key.as_str() {
            "SourceDirectory" => "DVDA_SRC",
            "FinalDirectory" => "DVDA_FINAL_DIR",
            "Title" => "DVDA_TITLE",
            "DiscBytes" => "DVDA_DISC_BYTES",
            "BuildDirectory" => "DVDA_BUILD_DIR",
            "IsoPrefix" => "DVDA_ISO_PREFIX",
            "GroupTrackLimit" => "DVDA_GROUP_TRACK_LIMIT",
            "MlpSource" => "DVDA_MLP_SOURCE",
            "MlpExternalDirectory" => "DVDA_MLP_EXTERNAL_DIR",
            "MlpSampleRate" => "DVDA_MLP_SAMPLE_RATE",
            "MlpBits" => "DVDA_MLP_BITS",
            "MenuEnabled" => "DVDA_MENU",
            "MenuStillPictures" => "DVDA_MENU_STILLPICS",
            "MenuTracksPerPage" => "DVDA_MENU_TRACKS_PER_PAGE",
            "MenuCoverDim" => "DVDA_MENU_COVER_DIM",
            "MenuFont" => "DVDA_MENU_FONT",
            "MenuFontJapanese" => "DVDA_MENU_FONT_JP",
            "MenuFontKorean" => "DVDA_MENU_FONT_KR",
            "ResumeEnabled" => "DVDA_RESUME",
            _ => continue,
        };
        let stored = Value::String(value_to_profile_string(value));

        values.insert(raw.into(), stored);
    }
    // The album index threshold is no longer user-configurable; keep the
    // historical default of four albums when profiles are written.
    values.insert(
        "DVDA_MENU_INDEX_MIN_ALBUMS".into(),
        Value::String("4".into()),
    );
    values
}

fn value_to_profile_string(value: Value) -> String {
    match value {
        Value::String(s) => s,
        Value::Bool(v) => {
            if v {
                "on".into()
            } else {
                "off".into()
            }
        }
        Value::Number(v) => v.to_string(),
        _ => String::new(),
    }
}

fn choose_and_load_profile(hwnd: Hwnd, state: &mut UiState) {
    let Some(path) = file_dialog(
        hwnd,
        false,
        &get_text(state.controls.profile),
        tr(state.lang, "select_profile"),
        state.lang,
        FileKind::Profile,
    ) else {
        return;
    };
    let path = PathBuf::from(path);
    match AppOptions::load(Some(&path)) {
        Ok(options) => {
            state.profile_path = path.clone();
            set_text(state.controls.profile, &path.to_string_lossy());
            set_text(state.controls.profile_origin, &path.to_string_lossy());
            state.base = Some(options.clone());
            populate(state, &options);
            if let Some(code) = profile_language(&path) {
                set_language(state, Lang::from_code(&code));
            }
            add_log(state, tr(state.lang, "loaded"), false);
        }
        Err(error) => add_log(
            state,
            &format!("{}{}", tr(state.lang, "profile_error"), error),
            true,
        ),
    }
}

fn emit_preparation_issues(
    callbacks: &mut dyn Callbacks,
    issues: &[dvda_core::preparation::models::Issue],
) {
    for issue in issues {
        let mut text = format!("[{}] {}\n    {}", issue.level, issue.title, issue.reason);
        if !issue.path.is_empty() {
            text.push_str(&format!("\n    {}", issue.path));
        }
        for detail in &issue.detail {
            text.push_str(&format!("\n    {detail}"));
        }
        callbacks.emit(if issue.level == "FAIL" { 2 } else { 1 }, &text);
    }
}

fn recovery_choice(lang: Lang) -> Result<bool, String> {
    task_choice(lang, false)
}

fn retention_choice(lang: Lang) -> Result<bool, String> {
    task_choice(lang, true)
}

fn task_choice(lang: Lang, retention: bool) -> Result<bool, String> {
    #[repr(C, packed)]
    struct Button { id: i32, text: *const u16 }
    #[repr(C, packed)]
    struct Config {
        size: u32, parent: Hwnd, instance: *mut std::ffi::c_void, flags: u32,
        common: u32, title: *const u16, icon: *const u16, instruction: *const u16,
        content: *const u16, count: u32, buttons: *const Button, default_button: i32,
        radio_count: u32, radios: *const Button, default_radio: i32,
        verification: *const u16, expanded: *const u16, expanded_control: *const u16,
        collapsed_control: *const u16, footer_icon: *const u16, footer: *const u16,
        callback: usize, callback_data: isize, width: u32,
    }
    #[link(name = "comctl32")]
    unsafe extern "system" {
        fn TaskDialogIndirect(config: *const Config, button: *mut i32, radio: *mut i32, verified: *mut i32) -> i32;
    }
    let (title, content, continue_text, restart_text) = if retention {
        match lang {
            Lang::Zh => ("制作未完成", "是否保留本次进度，以便下次继续制作？删除进度只会删除此任务的中间文件，不会删除音源或已输出的 ISO。", "保留进度", "删除进度"),
            Lang::Ja => ("作成が完了していません", "次回再開するため進捗を保存しますか？削除するのはタスクの一時ファイルのみです。", "進捗を保存", "進捗を削除"),
            _ => ("Production unfinished", "Keep progress to continue next time? Deletion removes only this task's intermediate files, not sources or published ISOs.", "Keep progress", "Delete progress"),
        }
    } else { match lang {
        Lang::Zh => ("发现未完成的制作", "继续制作会校验并使用已保存的进度；重新制作会删除该任务的中间文件，从头开始。音源和已输出的 ISO 不会删除。", "继续制作", "重新制作"),
        Lang::Ja => ("未完了の作業があります", "保存済みの進捗を確認して再開するか、このタスクの一時ファイルを削除して最初から作成します。音源と出力済み ISO は削除しません。", "作成を続ける", "最初から作成"),
        _ => ("Unfinished production found", "Continue validates saved progress. Restart deletes this task's intermediate files and starts over. Sources and published ISOs are preserved.", "Continue production", "Restart production"),
    } };
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let title = wide(title); let content = wide(content);
    let continue_text = wide(continue_text); let restart_text = wide(restart_text);
    let buttons = [Button { id: 1001, text: continue_text.as_ptr() }, Button { id: 1002, text: restart_text.as_ptr() }];
    let mut config: Config = unsafe { std::mem::zeroed() };
    config.size = std::mem::size_of::<Config>() as u32;
    config.title = title.as_ptr(); config.instruction = title.as_ptr();
    config.content = content.as_ptr(); config.count = 2;
    config.buttons = buttons.as_ptr(); config.default_button = 1001;
    let mut button = 0;
    let result = unsafe { TaskDialogIndirect(&config, &mut button, std::ptr::null_mut(), std::ptr::null_mut()) };
    if result < 0 || !matches!(button, 1001 | 1002) {
        return Err("Cannot display unfinished-task recovery choice; task files have been preserved.".into());
    }
    Ok(button == 1002)
}

fn start_operation(state: &mut UiState, command: usize) {
    if !validate_controls(state, true) {
        return;
    }
    let path = PathBuf::from(get_text(state.controls.profile).trim());
    let base = if path.is_file() {
        AppOptions::load(Some(&path))
    } else {
        AppOptions::load(None)
    };
    let mut options =
        match base.and_then(|base| base.with_profile_values(edited_profile_values(state))) {
            Ok(options) => options,
            Err(error) => {
                add_log(
                    state,
                    &format!("{}{}", tr(state.lang, "profile_error"), error),
                    true,
                );
                return;
            }
        };
    options.language = state.lang.code().into();
    let options = match if command == ID_VERIFY {
        Ok(options)
    } else if command == ID_PREPARE {
        options.scratch_options()
    } else if options.has_unfinished_task() {
        recovery_choice(state.lang).and_then(|restart| options.recover_task(restart))
    } else {
        options.task_options()
    } {
        Ok(options) => options,
        Err(error) => {
            add_log(state, &error, true);
            return;
        }
    };
    let cancel = Arc::new(AtomicBool::new(false));
    autosave(state);
    state.cancel = Some(Arc::clone(&cancel));
    state.busy = true;
    state.started = Some(Instant::now());
    state.elapsed_seconds = 0;
    state.progress_value = 0;
    reset_task_log(state);
    set_busy_controls(state, true);
    update_statistics(state);
    relayout_for_task(state);
    for hwnd in [
        state.controls.prepare,
        state.controls.build,
        state.controls.verify,
    ] {
        unsafe {
            EnableWindow(hwnd, 0);
        }
    }
    unsafe {
        EnableWindow(state.controls.cancel, 1);
    }
    let start_key = match command {
        ID_PREPARE => "started_check",
        ID_BUILD => "started_build",
        _ => "started_verify",
    };
    state.status_key = start_key;
    state.activity_raw = tr(Lang::Zh, start_key).into();
    set_text(state.controls.status, tr(state.lang, start_key));
    set_text(state.controls.activity, tr(state.lang, start_key));
    // Verification can spend time in detailed-only checks, so keep a useful
    // summary row visible even when its low-level progress is filtered out.
    add_log(state, tr(Lang::Zh, start_key), false);
    if checked(state.controls.live) {
        render_log(state);
    }
    unsafe {
        let progress = state.controls.progress;
        SendMessageW(progress, PBM_SETRANGE32, 0, 100);
        SendMessageW(progress, PBM_SETPOS, 0, 0);
    }
    let lang = state.lang;
    let logs = state.log_sender.clone();
    thread::spawn(move || {
        let mut callbacks = WorkerCallbacks {
            logs,
            cancel: Arc::clone(&cancel),
            range: (0, 100),
            last_progress: 0,
        };
        let (ok, text) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match command {
            ID_PREPARE => {
                callbacks.set_progress_range(0, 100);
                let outcome = dvda_core::app::run_prepare(&options, false, &mut callbacks);
                if let Some(data) = &outcome.data {
                    emit_preparation_issues(&mut callbacks, &data.issues);
                }
                (
                    outcome.succeeded(),
                    outcome.failure.map(|f| f.message).unwrap_or_else(|| {
                        tr(
                            lang,
                            if outcome
                                .data
                                .as_ref()
                                .is_some_and(|data| data.failure_count == 0)
                            {
                                "done"
                            } else {
                                "failed"
                            },
                        )
                        .into()
                    }),
                )
            }
            ID_BUILD => {
                callbacks.set_progress_range(0, 15);
                let prepared = dvda_core::app::run_prepare(&options, false, &mut callbacks);
                if let Some(data) = &prepared.data {
                    emit_preparation_issues(&mut callbacks, &data.issues);
                }
                if !prepared.succeeded() {
                    (
                        false,
                        prepared
                            .failure
                            .map(|failure| failure.message)
                            .unwrap_or_else(|| tr(lang, "failed").into()),
                    )
                } else {
                    callbacks.set_progress_range(15, 65);
                    match dvda_core::app::run_build(&options, false, &mut callbacks) {
                        Ok(result) => {
                            emit_diagnostics(&mut callbacks, &result.diagnostics);
                            if !result.succeeded {
                                (false, tr(lang, "failed").into())
                            } else {
                                callbacks.set_progress_range(65, 100);
                                match dvda_core::app::run_verify(&options, &mut callbacks) {
                                    Ok(verification) => {
                                        emit_diagnostics(&mut callbacks, &verification.diagnostics);
                                        (
                                            verification.succeeded,
                                            tr(
                                                lang,
                                                if verification.succeeded {
                                                    "done"
                                                } else {
                                                    "failed"
                                                },
                                            )
                                            .into(),
                                        )
                                    }
                                    Err(error) => (false, error),
                                }
                            }
                        }
                        Err(error) => (false, error),
                    }
                }
            }
            _ => {
                callbacks.set_progress_range(0, 100);
                match dvda_core::app::run_verify(&options, &mut callbacks) {
                    Ok(result) => {
                        emit_diagnostics(&mut callbacks, &result.diagnostics);
                        (
                            result.succeeded,
                            tr(lang, if result.succeeded { "done" } else { "failed" }).into(),
                        )
                    }
                    Err(error) => (false, error),
                }
            }
        }))
        .unwrap_or_else(|_| (false, "任务异常终止，请查看详细日志。".into()));
        if ok && command == ID_BUILD {
            options.finish_task();
        }
        if !ok && command == ID_BUILD {
            let _ = callbacks.logs.send(WorkerEvent::Retention(options));
        } else {
            drop(options);
        }
        let kind = if cancel.load(Ordering::Relaxed) {
            2
        } else if ok {
            1
        } else {
            0
        };
        let _ = callbacks
            .logs
            .send(WorkerEvent::Finished(UiEvent { kind, text }));
    });
}

fn emit_diagnostics(callbacks: &mut dyn Callbacks, diagnostics: &[Value]) {
    for diagnostic in diagnostics {
        if let Some(message) = diagnostic.get("Message").and_then(Value::as_str) {
            let severity = diagnostic
                .get("Severity")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let code = diagnostic.get("Code").and_then(Value::as_str).unwrap_or("");
            let prefix = match severity {
                2 => "[错误]",
                1 => "[警告]",
                _ => "[信息]",
            };
            callbacks.emit(
                if severity == 2 { 2 } else { 1 },
                &format!("{prefix} {message} [{code}]"),
            );
        }
    }
}

fn update_progress(state: &mut UiState, value: u16) {
    state.progress_value = state.progress_value.max(value.min(99));
    unsafe {
        SendMessageW(
            state.controls.progress,
            PBM_SETPOS,
            state.progress_value as usize,
            0,
        );
    }
    if state.busy {
        set_text(
            state.controls.status,
            &format!(
                "{} \u{00b7} {}%",
                tr(state.lang, state.status_key),
                state.progress_value
            ),
        );
    }
}

fn finish_operation(state: &mut UiState, kind: u32, text: &str) {
    state.cancel = None;
    state.elapsed_seconds = state.started.map_or(0, |s| s.elapsed().as_secs());
    state.started = None;
    let key = match kind {
        1 => "done",
        2 => "cancelled",
        _ => "failed",
    };
    state.status_key = key;
    let text = if kind == 2 {
        tr(state.lang, "cancelled")
    } else {
        text
    };
    set_text(state.controls.status, tr(state.lang, key));
    set_text(state.controls.activity, &localized_log(state.lang, text));
    add_log(state, text, kind == 0);
    if kind == 1 && state.problem_count > 0 {
        set_text(
            state.controls.status,
            &format!(
                "{} · {}",
                tr(state.lang, key),
                presentation::phrase(
                    state.lang,
                    "有提示待查看",
                    "Review the warnings",
                    "警告を確認してください"
                )
            ),
        );
    }
    if kind == 0 {
        let advice = presentation::error_advice(state.lang, text);
        add_log(state, &format!("[信息] {advice}"), false);
        set_text(state.controls.activity, &advice);
    }
    render_log(state);
    if kind == 1 {
        state.progress_value = 100;
        unsafe {
            SendMessageW(state.controls.progress, PBM_SETPOS, 100, 0);
        }
    }
    // Publish idle only after the final diagnostics, progress and status are
    // complete. A new task must not be accepted inside an unfinished callback.
    unsafe {
        EnableWindow(state.controls.cancel, 0);
    }
    set_busy_controls(state, false);
    for hwnd in [
        state.controls.prepare,
        state.controls.build,
        state.controls.verify,
    ] {
        unsafe {
            EnableWindow(hwnd, 1);
        }
    }
    state.busy = false;
    update_statistics(state);
    relayout_for_task(state);
}

/// Short windows switch between the settings and the task area when a task
/// starts or ends.
fn relayout_for_task(state: &mut UiState) {
    let mut rect: Rect = unsafe { std::mem::zeroed() };
    unsafe { GetClientRect(state.main_window, &mut rect) };
    // Minimized windows lay out again when restored.
    if rect.bottom > 0 {
        layout(state.main_window, state);
    }
}
fn cancel_operation(state: &mut UiState) {
    if let Some(cancel) = &state.cancel {
        cancel.store(true, Ordering::Relaxed);
        unsafe {
            EnableWindow(state.controls.cancel, 0);
        }
        state.status_key = "stopping";
        set_text(state.controls.status, tr(state.lang, "stopping"));
        add_log(state, tr(Lang::Zh, "stopping"), false);
    }
}

unsafe extern "system" fn disable_busy_child(hwnd: Hwnd, parameter: Lparam) -> i32 {
    let disabled = unsafe { &mut *(parameter as *mut Vec<Hwnd>) };
    if window_class(hwnd) != "Static" && unsafe { IsWindowEnabled(hwnd) } != 0 {
        disabled.push(hwnd);
        unsafe { EnableWindow(hwnd, 0) };
    }
    1
}

fn set_busy_controls(state: &mut UiState, busy: bool) {
    let enabled = i32::from(!busy);
    if busy {
        for page in &state.pages {
            unsafe {
                EnumChildWindows(
                    *page,
                    disable_busy_child,
                    &mut state.busy_disabled_controls as *mut Vec<Hwnd> as Lparam,
                );
            }
        }
    } else {
        for hwnd in state.busy_disabled_controls.drain(..) {
            unsafe { EnableWindow(hwnd, 1) };
        }
    }
    for page in &state.pages {
        unsafe {
            RedrawWindow(*page, ptr::null(), ptr::null_mut(), RDW_INVALIDATE | RDW_ALLCHILDREN);
        }
    }
    for hwnd in state
        .tabs
        .iter()
        .copied()
        .chain([state.controls.profile, state.controls.language])
    {
        unsafe {
            EnableWindow(hwnd, enabled);
            RedrawWindow(
                hwnd,
                ptr::null(),
                ptr::null_mut(),
                RDW_INVALIDATE | RDW_ALLCHILDREN,
            );
        }
    }
    for (hwnd, key) in &state.buttons {
        if matches!(*key, "open" | "save" | "save_as") {
            unsafe {
                EnableWindow(*hwnd, enabled);
            }
        }
    }
}

fn add_log(state: &mut UiState, text: &str, problem: bool) {
    let timestamp = log_timestamp();
    // Keep the source path and continuation lines of a multiline diagnostic in
    // the problem view together with its severity-bearing first line.
    let problem = problem || log_is_problem(text);
    for text in text.split(['\r', '\n']).filter(|line| !line.is_empty()) {
        if let Some((file, _)) = &mut state.log_file
            && let Err(error) = writeln!(file, "[{timestamp}] {text}").and_then(|_| file.flush())
        {
            state.log_file = None;
            let message = presentation::phrase(
                state.lang,
                "无法继续保存完整日志，窗口仅保留最近记录。请检查磁盘空间或目录权限。",
                "Cannot continue saving the full log. Only recent records remain; check disk space and folder permissions.",
                "完全なログを保存できません。最近の記録のみ表示されます。空き容量とフォルダー権限を確認してください。",
            );
            push_log_line(
                state,
                LogLine {
                    raw: format!("[警告] {message} {error}"),
                    problem: true,
                    timestamp: timestamp.clone(),
                },
            );
            state.problem_count += 1;
        }
        let problem = problem || log_is_problem(text);
        // Repetitive category warnings are represented by their explicit
        // stage summary, not hundreds of identical entries in the counter.
        state.problem_count += usize::from(
            problem
                && !text
                    .trim()
                    .starts_with("WARN: Button y coordinates are odd for button "),
        );
        push_log_line(
            state,
            LogLine {
                raw: text.to_owned(),
                problem,
                timestamp: timestamp.clone(),
            },
        );
    }
    if presentation::activity(state.lang, text).is_some() {
        state.activity_raw = text.to_owned();
        state.activity_dirty = true;
    }
    state.log_dirty = true;
}

fn push_log_line(state: &mut UiState, line: LogLine) {
    // Decoder progress can fill the detail buffer before the next track ends.
    // Keep task history separately so that chatter cannot evict summary rows.
    if presentation::summary(state.lang, &line.raw, line.problem).is_some() {
        state.summary_logs.push(line.clone());
        if state.summary_logs.len() > 2500 {
            state.summary_logs.drain(..state.summary_logs.len() - 2000);
            if !state.log_render_detail {
                state.log_render_language = None;
            }
        }
    }
    state.logs.push(line);
    if state.logs.len() > 2500 {
        state.logs.drain(..state.logs.len() - 2000);
        if state.log_render_detail {
            state.log_render_language = None;
        }
    }
}

fn drain_worker_logs(hwnd: Hwnd, state: &mut UiState) {
    observe_log_scroll(state);
    let started = Instant::now();
    let mut processed = 0;
    let mut drained = false;
    while processed < LOG_BATCH_LIMIT {
        let batch: Vec<_> = state.log_receiver.try_iter().take(64).collect();
        if batch.is_empty() {
            drained = true;
            break;
        }
        processed += batch.len();
        for event in batch {
            match event {
                WorkerEvent::Log(text) => add_log(state, &text, false),
                WorkerEvent::Progress(value) => update_progress(state, value),
                WorkerEvent::Finished(event) => state.pending_finish = Some(event),
                WorkerEvent::Retention(options) => {
                    if retention_choice(state.lang).unwrap_or(false) {
                        options.discard_task();
                    }
                }
            }
        }
        // Return to the message pump regularly so cancel, close, resizing and
        // statistics remain responsive even during continuous native output.
        if started.elapsed() >= LOG_BATCH_BUDGET {
            break;
        }
    }
    if state.activity_dirty
        && state.busy
        && !state
            .cancel
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
    {
        set_text(
            state.controls.activity,
            &localized_log(state.lang, &state.activity_raw),
        );
    }
    state.activity_dirty = false;
    if drained && let Some(event) = state.pending_finish.take() {
        // The worker queues completion only after its last log. Finish and
        // close must wait until every queued diagnostic has been archived.
        finish_operation(state, event.kind, &event.text);
        if state.close_when_done {
            unsafe {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
        }
    } else if state.log_dirty && checked(state.controls.live) && !state.log_frozen {
        // A frozen log (live sizing) is rendered once the drag ends.
        render_log(state);
    }
}
fn display_log(lang: Lang, lines: &[LogLine], detail: bool, only: bool) -> String {
    let mut text = String::new();
    for line in lines {
        let message = if detail {
            Some(localized_detail(lang, &line.raw))
        } else if only && !line.problem {
            None
        } else {
            presentation::summary(lang, &line.raw, line.problem)
        };
        if let Some(message) = message {
            text.push_str(&format!("{}  {}\r\n", line.timestamp, message));
        }
    }
    text
}
fn render_log(state: &mut UiState) {
    let detail = combo_index(state.controls.log_view) == 1;
    let issues_only = checked(state.controls.only_issues);
    unsafe {
        EnableWindow(state.controls.only_issues, i32::from(!detail));
    }
    let lines = if detail {
        &state.logs
    } else {
        &state.summary_logs
    };
    let can_append = state.log_render_language == Some(state.lang)
        && state.log_render_detail == detail
        && state.log_render_issues_only == issues_only
        && state.log_rendered_count <= lines.len();
    let text = display_log(
        state.lang,
        if can_append {
            &lines[state.log_rendered_count..]
        } else {
            lines
        },
        detail,
        issues_only,
    );
    let hold_line = state
        .log_hold_until
        .filter(|deadline| Instant::now() < *deadline)
        .and(state.log_hold_line);
    update_log_text(state.controls.log, &text, can_append, hold_line);
    if hold_line.is_none() {
        state.log_hold_line = None;
        state.log_hold_until = None;
    }
    state.log_rendered_count = lines.len();
    state.log_render_language = Some(state.lang);
    state.log_render_detail = detail;
    state.log_render_issues_only = issues_only;
    state.log_first_visible_line = first_visible_log_line(state.controls.log);
    state.log_dirty = false;
}

fn update_log_text(log: Hwnd, text: &str, append: bool, hold_line: Option<i32>) {
    if append && text.is_empty() {
        return;
    }
    let text = wide(text);
    unsafe {
        let mut selection_start = 0u32;
        let mut selection_end = 0u32;
        SendMessageW(
            log,
            EM_GETSEL,
            &mut selection_start as *mut u32 as usize,
            &mut selection_end as *mut u32 as isize,
        );
        // Keep text changes, selection and viewport positioning in one redraw.
        // Following the tail must not restore an old caret at the first line.
        SendMessageW(log, WM_SETREDRAW, 0, 0);
        if append {
            let end = GetWindowTextLengthW(log).max(0);
            SendMessageW(log, EM_SETREADONLY, 0, 0);
            SendMessageW(log, EM_SETSEL, end as usize, end as isize);
            SendMessageW(log, EM_REPLACESEL, 0, text.as_ptr() as isize);
            SendMessageW(log, EM_SETREADONLY, 1, 0);
        } else {
            SetWindowTextW(log, text.as_ptr());
        }
        if let Some(line) = hold_line {
            SendMessageW(
                log,
                EM_SETSEL,
                selection_start as usize,
                selection_end as isize,
            );
            let current = first_visible_log_line(log);
            SendMessageW(log, EM_LINESCROLL, 0, (line - current) as isize);
        } else {
            let end = GetWindowTextLengthW(log).max(0);
            SendMessageW(log, EM_SETSEL, end as usize, end as isize);
            SendMessageW(log, WM_VSCROLL, SB_BOTTOM, 0);
        }
        SendMessageW(log, WM_SETREDRAW, 1, 0);
        RedrawWindow(log, ptr::null(), ptr::null_mut(), RDW_INVALIDATE);
    }
}

fn first_visible_log_line(log: Hwnd) -> i32 {
    unsafe { SendMessageW(log, EM_GETFIRSTVISIBLELINE, 0, 0) as i32 }.max(0)
}

fn observe_log_scroll(state: &mut UiState) {
    let current = first_visible_log_line(state.controls.log);
    if current != state.log_first_visible_line {
        state.log_first_visible_line = current;
        state.log_hold_line = Some(current);
        state.log_hold_until = Some(Instant::now() + Duration::from_secs(10));
    }
}
fn log_is_problem(text: &str) -> bool {
    presentation::problem(text)
}

fn setting_help(lang: Lang, key: &str) -> String {
    static HELP: OnceLock<HashMap<String, String>> = OnceLock::new();
    let help = HELP.get_or_init(|| {
        serde_json::from_str(include_str!("../locales/help.json")).expect("embedded settings help")
    });
    help.get(key)
        .map(|text| localization::translate(lang.code(), text))
        .unwrap_or_default()
}

fn tooltip_info(tooltip: &Tooltip) -> ToolInfo {
    ToolInfo {
        size: std::mem::size_of::<ToolInfo>() as u32,
        flags: 0x11,
        owner: unsafe { GetParent(tooltip.control) },
        id: tooltip.control as usize,
        rect: Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        instance: ptr::null_mut(),
        text: tooltip.text.as_ptr() as *mut u16,
        param: 0,
        reserved: ptr::null_mut(),
    }
}

fn initialize_tooltips(parent: Hwnd, state: &mut UiState) {
    let tooltip_window = unsafe {
        CreateWindowExW(
            0x8,
            wide("tooltips_class32").as_ptr(),
            ptr::null(),
            0x80000003,
            0,
            0,
            0,
            0,
            parent,
            ptr::null_mut(),
            GetModuleHandleW(ptr::null()),
            ptr::null_mut(),
        )
    };
    if tooltip_window.is_null() {
        return;
    }
    state.tooltip_window = tooltip_window;
    unsafe {
        SendMessageW(tooltip_window, 0x0418, 0, 460);
    }
    let c = &state.controls;
    for (control, key) in [
        (c.source, "DVDA_SRC"),
        (c.final_dir, "DVDA_FINAL_DIR"),
        (c.title, "DVDA_TITLE"),
        (c.capacity, "DVDA_DISC_BYTES"),
        (c.custom_bytes, "DVDA_DISC_BYTES"),
        (c.work_dir, "DVDA_BUILD_DIR"),
        (c.iso_prefix, "DVDA_ISO_PREFIX"),
        (c.group_limit, "DVDA_GROUP_TRACK_LIMIT"),
        (c.mode, "DVDA_MLP_SOURCE"),
        (c.rate, "DVDA_MLP_SAMPLE_RATE"),
        (c.bits, "DVDA_MLP_BITS"),
        (c.import_folder, "DVDA_MLP_EXTERNAL_DIR"),
        (c.menu, "DVDA_MENU"),
        (c.tracks, "DVDA_MENU_TRACKS_PER_PAGE"),
        (c.stills, "DVDA_MENU_STILLPICS"),
        (c.cover, "DVDA_MENU_COVER_DIM"),
        (c.font_sc, "DVDA_MENU_FONT"),
        (c.font_jp, "DVDA_MENU_FONT_JP"),
        (c.font_kr, "DVDA_MENU_FONT_KR"),
        (c.keep_files, "DVDA_KEEP_TMP"),
    ] {
        let tooltip = Tooltip {
            control,
            key,
            text: wide(&setting_help(state.lang, key)),
        };
        let info = tooltip_info(&tooltip);
        unsafe {
            SendMessageW(tooltip_window, 0x0432, 0, &info as *const ToolInfo as isize);
        }
        state.tooltips.push(tooltip);
    }
}

fn numbers(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn localized_log(lang: Lang, raw: &str) -> String {
    let text = raw.trim();
    if text.is_empty() {
        return String::new();
    }
    if let Some(message) = presentation::task_message(lang, text) {
        return message;
    }
    if let Some(message) = batch_message(lang, text) {
        return message;
    }
    if let Some(value) = text.strip_prefix("[MLP-CHECK] start ")
        && !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => format!("正在检查已有 MLP：0/{value} 首"),
            Lang::En => format!("Checking existing MLP files: 0/{value} tracks"),
            Lang::Ja => format!("既存 MLP を確認中：0/{value} 曲"),
        };
    }
    if let Some(value) = text.strip_prefix("[MLP-CHECK] progress ")
        && let Some((position, title)) = value.split_once(' ')
        && let Some((current, total)) = position.split_once('/')
        && !current.is_empty()
        && !total.is_empty()
        && current.bytes().all(|byte| byte.is_ascii_digit())
        && total.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => format!("正在检查已有 MLP：{current}/{total} 首；{title}"),
            Lang::En => format!("Checking existing MLP files: {current}/{total} tracks; {title}"),
            Lang::Ja => format!("既存 MLP を確認中：{current}/{total} 曲；{title}"),
        };
    }
    if let Some(value) = text.strip_prefix("[MLP-CHECK] complete ")
        && let Some((reused, pending)) = value.split_once(' ')
        && !reused.is_empty()
        && !pending.is_empty()
        && reused.bytes().all(|byte| byte.is_ascii_digit())
        && pending.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => format!("MLP 检查完成：可复用 {reused} 首，需编码 {pending} 首。"),
            Lang::En => format!("MLP checks complete: {reused} reusable, {pending} to encode."),
            Lang::Ja => {
                format!("MLP の確認完了：{reused} 曲を再利用、{pending} 曲をエンコードします。")
            }
        };
    }
    if let Some(value) = text.strip_prefix("[MLP-PROGRESS] ")
        && let Some((current, total)) = value.split_once('/')
        && current.bytes().all(|byte| byte.is_ascii_digit())
        && total.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => format!("MLP 编码进度：{current}/{total} 首"),
            Lang::En => format!("MLP encoding progress: {current}/{total} tracks"),
            Lang::Ja => format!("MLP エンコード進捗：{current}/{total} 曲"),
        };
    }
    if let Some(value) = text.strip_prefix("[menu-cover] ")
        && let Some((kind, details)) = value.split_once(' ')
        && let Some((position, album)) = details.split_once(' ')
        && let Some((current, total)) = position.split_once('/')
        && current.bytes().all(|byte| byte.is_ascii_digit())
        && total.bytes().all(|byte| byte.is_ascii_digit())
    {
        let (zh, en, ja) = match kind {
            "background" => ("菜单封面背景", "Menu cover background", "メニュー背景"),
            "still" => ("播放封面静图", "Playback cover image", "再生用カバー画像"),
            _ => return text.to_owned(),
        };
        return match lang {
            Lang::Zh => format!("{zh} {current}/{total}：{album}"),
            Lang::En => format!("{en} {current}/{total}: {album}"),
            Lang::Ja => format!("{ja} {current}/{total}：{album}"),
        };
    }
    if let Some(number) = text.strip_prefix("[menu-cover] oversized ")
        && number.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => format!("播放封面图片 {number} 超过 2 MB，继续制作。"),
            Lang::En => {
                format!("Playback cover image {number} exceeds 2 MB; disc creation will continue.")
            }
            Lang::Ja => {
                format!("再生用カバー画像 {number} は 2 MB を超えています。作成を続行します。")
            }
        };
    }
    if let Some(value) = text.strip_prefix("[verify] pcm ") {
        let mut fields = value.splitn(3, ' ');
        if let (Some(position), Some(result), Some(path)) =
            (fields.next(), fields.next(), fields.next())
            && let Some((current, total)) = position.split_once('/')
        {
            let result = result.strip_suffix(':').unwrap_or(result);
            return match (lang, result) {
                (Lang::Zh, "ok") => format!("成品音频校验：{current}/{total} 首一致：{path}"),
                (Lang::En, "ok") => {
                    format!("Output audio check: {current}/{total} tracks match: {path}")
                }
                (Lang::Ja, "ok") => format!("出力音声検証：{current}/{total} 曲一致：{path}"),
                (Lang::Zh, "failed") => format!("成品音频校验：{current}/{total} 首不一致：{path}"),
                (Lang::En, "failed") => {
                    format!("Output audio check: {current}/{total} tracks differ: {path}")
                }
                (Lang::Ja, "failed") => format!("出力音声検証：{current}/{total} 曲不一致：{path}"),
                _ => text.to_owned(),
            };
        }
    }
    if let Some(value) = text.strip_prefix("[verify] complete ") {
        let mut fields = value.split_whitespace();
        if let (Some(discs), Some("discs"), Some(tracks), Some("tracks"), Some(status)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) {
            let succeeded = status == "ok";
            return match (lang, succeeded) {
                (Lang::Zh, true) => {
                    format!("成品校验完成：检查 {discs} 张光盘、{tracks} 首音轨，结果通过。")
                }
                (Lang::En, true) => format!(
                    "Output verification completed: {discs} discs and {tracks} tracks checked successfully."
                ),
                (Lang::Ja, true) => format!(
                    "出力検証完了：ディスク {discs} 枚、{tracks} 曲を確認し、問題ありません。"
                ),
                (Lang::Zh, false) => {
                    format!("成品校验结束：检查 {discs} 张光盘、{tracks} 首音轨，发现问题。")
                }
                (Lang::En, false) => format!(
                    "Output verification finished: {discs} discs and {tracks} tracks checked; problems were found."
                ),
                (Lang::Ja, false) => format!(
                    "出力検証終了：ディスク {discs} 枚、{tracks} 曲を確認し、問題が見つかりました。"
                ),
            };
        }
    }
    if let Some(value) = text.strip_prefix("发现 ")
        && let Some(count) = numbers(value).first()
    {
        return tr(lang, "log_found_audio").replace("{0}", count);
    }
    if let Some(value) = text.strip_prefix("共需重采样 ")
        && let Some(count) = numbers(value).first()
    {
        return if count == "0" {
            tr(lang, "log_resample_none").into()
        } else {
            tr(lang, "log_resample_count").replace("{0}", count)
        };
    }
    if text.starts_with("[缓存] 复用元数据 ") || text.starts_with("[缓存] 复用已校验结果 ")
    {
        let values = numbers(text);
        if values.len() >= 2 {
            let key = if text.contains("复用元数据") {
                "log_cache_metadata"
            } else {
                "log_cache_verified"
            };
            return tr(lang, key)
                .replace("{0}", &values[0])
                .replace("{1}", &values[1]);
        }
    }
    if let Some(value) = text.strip_prefix("[MLP] ")
        && !value.starts_with("临时目录:")
        && !value.starts_with("MLP 输出目录:")
    {
        return tr(lang, "log_encoding")
            .replace("{0}", &localization::translate(lang.code(), value));
    }
    if let Some(value) = text.strip_prefix("[MLP-FINALIZE] start ")
        && value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => format!("开始整理 MLP 编码结果，共 {value} 首。"),
            Lang::En => format!("Finalizing {value} MLP encoding results."),
            Lang::Ja => format!("MLP エンコード結果を整理中（{value} 曲）。"),
        };
    }
    if let Some(value) = text.strip_prefix("[MLP-FINALIZE] progress ") {
        let mut fields = value.splitn(2, ' ');
        if let (Some(position), Some(title)) = (fields.next(), fields.next())
            && let Some((current, total)) = position.split_once('/')
        {
            return match lang {
                Lang::Zh => format!("正在整理 MLP 编码结果：{current}/{total}，{title}"),
                Lang::En => {
                    format!("Finalizing MLP encoding result {current}/{total}: {title}")
                }
                Lang::Ja => format!("MLP エンコード結果を整理中：{current}/{total}、{title}"),
            };
        }
    }
    if let Some(value) = text.strip_prefix("[MLP-FINALIZE] complete ")
        && value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return match lang {
            Lang::Zh => format!("MLP 编码结果整理完成，共 {value} 首。"),
            Lang::En => format!("MLP encoding results finalized: {value} tracks."),
            Lang::Ja => format!("MLP エンコード結果の整理が完了しました（{value} 曲）。"),
        };
    }
    if let Some(value) = text.strip_prefix("[LPCM] ") {
        return tr(lang, "log_lpcm").replace("{0}", &localization::translate(lang.code(), value));
    }
    if let Some(value) = text.strip_prefix("[author] disc ")
        && value.chars().all(|c| c.is_ascii_digit())
        && let Some(number) = numbers(value).first()
    {
        return tr(lang, "log_author_disc").replace("{0}", number);
    }
    if let Some(value) = text.strip_prefix("[build] ")
        && value.ends_with(" tracks loaded")
        && let Some(number) = numbers(value).first()
    {
        return tr(lang, "log_loaded_tracks").replace("{0}", number);
    }
    if let Some(value) = text.strip_prefix("总计 ")
        && let Some(number) = numbers(value).first()
    {
        return tr(lang, "log_total_tracks").replace("{0}", number);
    }
    if let Some(value) = text.strip_prefix("报告已写入: ") {
        return tr(lang, "log_report_written").replace("{0}", value);
    }
    if let Some(value) = text.strip_prefix("manifest.json 已生成 -> ") {
        return tr(lang, "log_manifest_written").replace("{0}", value);
    }
    localized_detail(lang, text)
}

fn localized_detail(lang: Lang, text: &str) -> String {
    let text = text.trim();
    if let Some(event) = dvda_core::task_log::parse(text)
        && let Some(message) = presentation::task_message(lang, text)
    {
        return if event.detail.is_empty() {
            message
        } else {
            format!("{message} · {}", event.detail)
        };
    }
    if let Some(message) = batch_message(lang, text) {
        return message;
    }
    for prefix in ["[FAIL] ", "[WARN] ", "[WAR] "] {
        if let Some(message) = text.strip_prefix(prefix) {
            return format!("{prefix}{}", localization::translate(lang.code(), message));
        }
    }
    for (prefix, key) in [
        ("[警告] ", "log_warning_prefix"),
        ("[错误] ", "log_error_prefix"),
        ("[信息] ", ""),
    ] {
        if let Some(tail) = text.strip_prefix(prefix) {
            let (message, code) = tail
                .rsplit_once(" [")
                .filter(|(_, code)| code.ends_with(']'))
                .map(|(message, code)| (message, format!(" [{code}")))
                .unwrap_or((tail, String::new()));
            return format!(
                "{}{}{}",
                tr(lang, key),
                localization::translate(lang.code(), message),
                code
            );
        }
    }
    for key in [
        "ready",
        "ready_hint",
        "loaded",
        "done",
        "failed",
        "cancelled",
        "stopping",
        "started_check",
        "started_build",
        "started_verify",
    ] {
        if [Lang::Zh, Lang::En, Lang::Ja]
            .iter()
            .any(|&source| tr(source, key) == text)
        {
            return tr(lang, key).into();
        }
    }
    for key in ["saved", "profile_error"] {
        for source in [Lang::Zh, Lang::En, Lang::Ja] {
            if let Some(tail) = text.strip_prefix(tr(source, key)) {
                return format!(
                    "{}{}",
                    tr(lang, key),
                    if key == "saved" {
                        tail.into()
                    } else {
                        localization::translate(lang.code(), tail)
                    }
                );
            }
        }
    }
    localization::translate(lang.code(), text)
}

fn batch_message(lang: Lang, raw: &str) -> Option<String> {
    if !raw.starts_with('{') {
        return None;
    }
    let value: Value = serde_json::from_str(raw).ok()?;
    let name = value.get("Name")?.as_str()?;
    let detail = value.get("Value").unwrap_or(&Value::Null);
    let key = match value.get("Kind")?.as_str()? {
        "Started" => "log_started",
        "PcmStarted" => "log_pcm_started",
        "PcmProgress" => "log_pcm_progress",
        "PcmFinished" => "log_pcm_finished",
        "Encoded" => "log_encoded",
        "PcmError" => "log_pcm_error",
        "Detail" => {
            return Some(format!(
                "{name}: {}",
                localized_detail(lang, detail.as_str().unwrap_or_default())
            ));
        }
        _ => return None,
    };
    let mut result = tr(lang, key).to_owned();
    if key == "log_encoded" {
        for (i, field) in ["Rate", "Bits", "Channels", "Bytes"].iter().enumerate() {
            result = result.replace(
                &format!("{{{}}}", i + 1),
                &detail
                    .get(field)
                    .map(Value::to_string)
                    .unwrap_or_else(|| "?".into()),
            );
        }
    } else if key == "log_pcm_progress" {
        result = result.replace(
            "{1}",
            &detail
                .as_i64()
                .map(|v| v.clamp(0, 100).to_string())
                .unwrap_or_else(|| "?".into()),
        );
    } else if key == "log_pcm_error" {
        result = result.replace(
            "{1}",
            &localized_detail(lang, detail.as_str().unwrap_or_default()),
        );
    }
    // Substitute track names last so user data containing braces stays verbatim.
    Some(result.replace("{0}", name))
}
fn copy_log(state: &UiState) {
    let text = get_text(state.controls.log);
    let mut data = text
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        if OpenClipboard(ptr::null_mut()) != 0 {
            EmptyClipboard();
            let handle = GlobalAlloc(0x42, data.len() * 2);
            if !handle.is_null() {
                let destination = GlobalLock(handle) as *mut u16;
                if !destination.is_null() {
                    std::ptr::copy_nonoverlapping(data.as_mut_ptr(), destination, data.len());
                    GlobalUnlock(handle);
                    SetClipboardData(13, handle);
                }
            }
            CloseClipboard();
        }
    }
}
fn export_log(hwnd: Hwnd, state: &mut UiState) {
    let Some(path) = file_dialog(
        hwnd,
        true,
        "DVD-Audio-Maker.log.txt",
        tr(state.lang, "select_log"),
        state.lang,
        FileKind::Log,
    ) else {
        return;
    };
    let result = export_log_to(state, Path::new(&path));
    if let Err(error) = result {
        message_box(
            hwnd,
            &format!("{}{}", tr(state.lang, "export_failed"), error),
            "DVD-Audio Maker",
            0x10,
        );
    } else {
        add_log(
            state,
            &format!(
                "[信息] {}",
                presentation::phrase(
                    state.lang,
                    "详细日志已导出，可用于排查问题。",
                    "The full log was exported for troubleshooting.",
                    "詳細ログを保存しました。問題の調査に利用できます。"
                )
            ),
            false,
        );
    }
}

fn export_log_to(state: &mut UiState, destination: &Path) -> Result<(), String> {
    // Include all records already waiting when export was requested. Limit
    // the snapshot to the queue capacity so an active producer cannot keep
    // the export operation in an unbounded drain loop.
    let queued: Vec<_> = state
        .log_receiver
        .try_iter()
        .take(LOG_QUEUE_CAPACITY)
        .collect();
    for event in queued {
        match event {
            WorkerEvent::Log(text) => add_log(state, &text, false),
            WorkerEvent::Progress(value) => update_progress(state, value),
            WorkerEvent::Finished(event) => state.pending_finish = Some(event),
                WorkerEvent::Retention(options) => {
                    if retention_choice(state.lang).unwrap_or(false) {
                        options.discard_task();
                    }
                }
        }
    }
    if let Some((_, source)) = &state.log_file {
        if presentation::same_path(source, destination) {
            return Err(presentation::phrase(
                state.lang,
                "请选择其他文件名，不能覆盖正在记录的日志。",
                "Choose another file name; the active log cannot be overwritten.",
                "別のファイル名を選んでください。記録中のログは上書きできません。",
            )
            .into());
        }
        fs::copy(source, destination).map_err(|e| e.to_string())?;
    } else {
        fs::write(destination, full_log(state.lang, &state.logs)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn full_log(lang: Lang, lines: &[LogLine]) -> String {
    let mut text = String::new();
    for line in lines {
        let translated = localized_detail(lang, &line.raw);
        text.push_str(&format!("[{}] ", line.timestamp));
        text.push_str(&translated);
        text.push_str("\r\n");
        if translated != line.raw.trim() {
            text.push_str("[raw] ");
            text.push_str(&line.raw);
            text.push_str("\r\n");
        }
    }
    text
}
fn open_output(hwnd: Hwnd, state: &UiState) {
    let path = get_text(state.controls.final_dir);
    if !Path::new(&path).is_dir() {
        message_box(hwnd, tr(state.lang, "no_output"), "DVD-Audio Maker", 0x10);
        return;
    }
    let path = wide(&path);
    let verb = wide("open");
    unsafe {
        ShellExecuteW(
            hwnd,
            verb.as_ptr(),
            path.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOW,
        );
    }
}

fn browse_target(hwnd: Hwnd, state: &mut UiState, id: usize) {
    let Some((target, kind)) = state.browse.get(&id).copied() else {
        return;
    };
    let current = get_text(target);
    let result = match kind {
        BrowseKind::Folder => folder_dialog(hwnd, &current, tr(state.lang, "browse")),
        BrowseKind::Font => file_dialog(
            hwnd,
            false,
            &current,
            tr(state.lang, "browse"),
            state.lang,
            FileKind::Font,
        ),

    };
    if let Some(path) = result {
        set_text(target, &path);
    }
}

fn folder_dialog(owner: Hwnd, current: &str, title: &str) -> Option<String> {
    let mut display = vec![0u16; 32768];
    let title = wide(title);
    let info = BrowseInfo {
        owner,
        root: ptr::null_mut(),
        display_name: display.as_mut_ptr(),
        title: title.as_ptr(),
        flags: 0x0041,
        callback: None,
        param: 0,
        image: 0,
    };
    let item = unsafe { SHBrowseForFolderW(&info) };
    if item.is_null() {
        return None;
    }
    let mut path = vec![0u16; 32768];
    let ok = unsafe { SHGetPathFromIDListW(item, path.as_mut_ptr()) };
    unsafe {
        CoTaskMemFree(item);
    }
    if ok == 0 {
        return None;
    }
    let value =
        String::from_utf16_lossy(&path[..path.iter().position(|v| *v == 0).unwrap_or(path.len())]);
    if value.is_empty() && !current.is_empty() {
        Some(current.to_owned())
    } else {
        Some(value)
    }
}

#[derive(Clone, Copy)]
enum FileKind {
    Profile,
    Log,
    Font,
}

fn dialog_filter(lang: Lang, kind: FileKind) -> (String, &'static str) {
    let all = format!("{}\0*.*\0\0", tr(lang, "all_filter"));
    match kind {
        FileKind::Profile => (
            format!("{}\0*.json\0{all}", tr(lang, "json_filter")),
            "json",
        ),
        FileKind::Log => (
            format!("{}\0*.txt;*.log\0{all}", tr(lang, "text_filter")),
            "txt",
        ),
        FileKind::Font => (
            format!("{}\0*.ttf;*.otf;*.ttc\0{all}", tr(lang, "font_filter")),
            "",
        ),
    }
}

fn file_dialog(
    owner: Hwnd,
    save: bool,
    current: &str,
    title: &str,
    lang: Lang,
    kind: FileKind,
) -> Option<String> {
    let (filter, extension) = dialog_filter(lang, kind);
    let filter = wide(&filter);
    let title = wide(title);
    let default_extension = wide(extension);
    let mut file = vec![0u16; 32768];
    for (i, unit) in current.encode_utf16().enumerate().take(file.len() - 1) {
        file[i] = unit;
    }
    let mut dialog = OpenFileName {
        size: std::mem::size_of::<OpenFileName>() as u32,
        owner,
        instance: ptr::null_mut(),
        filter: filter.as_ptr(),
        custom_filter: ptr::null_mut(),
        custom_filter_max: 0,
        filter_index: 1,
        file: file.as_mut_ptr(),
        file_max: file.len() as u32,
        file_title: ptr::null_mut(),
        file_title_max: 0,
        initial_dir: ptr::null(),
        title: title.as_ptr(),
        flags: 0x00000800 | if save { 0x00000002 } else { 0x00001000 },
        file_offset: 0,
        file_extension: 0,
        def_ext: default_extension.as_ptr(),
        data: 0,
        hook: ptr::null_mut(),
        template_name: ptr::null(),
        reserved: ptr::null_mut(),
        reserved2: 0,
        flags_ex: 0,
    };
    let ok = unsafe {
        if save {
            GetSaveFileNameW(&mut dialog)
        } else {
            GetOpenFileNameW(&mut dialog)
        }
    };
    if ok == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(
        &file[..file.iter().position(|v| *v == 0).unwrap_or(file.len())],
    ))
}

fn profile_language(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let value: Value = serde_json::from_str(&dvda_core::config_files::decode_text(&bytes)).ok()?;
    value
        .get("Language")
        .and_then(Value::as_str)
        .map(str::to_owned)
}
fn message_box(hwnd: Hwnd, text: &str, caption: &str, flags: u32) -> i32 {
    let text = wide(text);
    let caption = wide(caption);
    unsafe { MessageBoxW(hwnd, text.as_ptr(), caption.as_ptr(), flags) }
}
#[cfg(test)]
mod presentation_tests {
    #[test]
    fn session_log_retention_keeps_three_and_preserves_unrelated_files() {
        let root = std::env::temp_dir().join(format!("dvda-log-retention-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for name in ["gui-1-1-0.log", "gui-2-1-0.log", "gui-3-1-0.log", "gui-4-1-0.log", "gui-not-ours.log", "notes.log"] {
            std::fs::write(root.join(name), name).unwrap();
        }
        let current = root.join("gui-4-1-0.log");
        super::prune_session_logs(&root, &current).unwrap();
        let count = std::fs::read_dir(&root).unwrap().count();
        assert_eq!(count, 5);
        assert!(current.exists());
        assert!(root.join("gui-not-ours.log").exists());
        assert!(root.join("notes.log").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    use super::*;

    #[test]
    fn startup_arguments_keep_json_alias_and_language_precedence() {
        let parse = |args: &[&str], environment| {
            parse_startup_args(args.iter().map(|arg| (*arg).to_owned()), environment)
        };
        for flag in ["--config", "--profile"] {
            let result = parse(
                &[flag, "日本語 profile.json", "--language", "en"],
                Some("ja"),
            )
            .unwrap();
            assert_eq!(result.profile, Some(PathBuf::from("日本語 profile.json")));
            assert_eq!(result.language.as_deref(), Some("en"));
        }
        assert_eq!(
            parse(&[], Some("ja-JP")).unwrap().language.as_deref(),
            Some("ja")
        );
        assert_eq!(parse(&[], None).unwrap().language, None);
        assert_eq!(
            parse(&["--language", "auto"], Some("ja")).unwrap().language,
            Some(Lang::from_code("auto").code().into())
        );
        for args in [
            vec!["--unknown"],
            vec!["--profile"],
            vec!["--config", "--language", "en"],
            vec!["--language"],
            vec!["--language", "invalid"],
            vec!["--language", "en", "--language", "ja"],
        ] {
            assert!(parse(&args, None).is_err(), "{args:?}");
        }
        assert!(parse(&[], Some("invalid")).is_err());
        assert!(parse(&["--language", "en"], Some("invalid")).is_ok());
    }

    #[test]
    fn source_issue_diagnostics_include_severity_reason_source_and_details() {
        #[derive(Default)]
        struct Capture(Vec<(i32, String)>);
        impl Callbacks for Capture {
            fn emit(&mut self, stream: i32, text: &str) {
                self.0.push((stream, text.into()));
            }
            fn cancelled(&mut self) -> bool {
                false
            }
        }
        let issues = ["FAIL", "WARN"].map(|level| dvda_core::preparation::models::Issue {
            level: level.into(),
            title: "Channel mismatch".into(),
            path: "source.flac".into(),
            reason: "1 channel / 2 channels".into(),
            detail: vec!["first track".into(), "second track".into()],
        });
        let mut captured = Capture::default();
        emit_preparation_issues(&mut captured, &issues);
        assert_eq!(captured.0.len(), 2);
        assert_eq!(captured.0[0].0, 2);
        assert_eq!(captured.0[1].0, 1);
        for (_, text) in captured.0 {
            for required in [
                "Channel mismatch",
                "1 channel / 2 channels",
                "source.flac",
                "first track",
                "second track",
            ] {
                assert!(text.contains(required), "{text}");
            }
        }
    }

    #[test]
    fn every_ui_translation_key_exists_in_all_three_languages() {
        let source = include_str!("main.rs");
        let section = source
            .split("fn tr(")
            .nth(1)
            .unwrap()
            .split("#[derive(Clone, Copy)]")
            .next()
            .unwrap();
        let mut keys = std::collections::BTreeSet::new();
        for line in section.lines() {
            let line = line.trim();
            if let Some(tail) = line.strip_prefix('"')
                && let Some((key, rest)) = tail.split_once('"')
                && rest.trim_start().starts_with("=>")
            {
                keys.insert(key);
            }
        }
        assert!(keys.len() >= 100);
        for key in keys {
            for lang in [Lang::Zh, Lang::En, Lang::Ja] {
                assert!(!tr(lang, key).is_empty(), "Missing {} / {key}", lang.code());
            }
        }
    }

    #[test]
    fn failures_never_become_successful_progress_messages() {
        let message = "[author] disc 2 failed (exit code 17)\ndecoder 0x4321: failure";
        for (lang, token) in [(Lang::Zh, "失败"), (Lang::En, "failed"), (Lang::Ja, "失敗")] {
            let output = localized_log(lang, message);
            assert!(output.contains(token), "{output}");
            assert!(
                output.contains("17") && output.contains("decoder 0x4321: failure"),
                "{output}"
            );
            assert!(log_is_problem(message));
        }
        let output = localized_detail(
            Lang::Ja,
            "[错误] No published ISO images were found [NO_ISO]",
        );
        assert!(
            output.contains("見つかりません") && output.contains("[NO_ISO]"),
            "{output}"
        );
    }

    #[test]
    fn batch_progress_preserves_track_names_and_audio_parameters() {
        let name = "音源_日本語_{0}_{1}.flac";
        for kind in [
            "Started",
            "PcmStarted",
            "PcmProgress",
            "PcmFinished",
            "Encoded",
            "PcmError",
            "Detail",
        ] {
            let value = match kind {
                "Encoded" => {
                    json!({"Frames":192000,"Rate":96000,"Bits":24,"Channels":6,"Bytes":7654321})
                }
                "PcmProgress" => json!(47),
                _ => json!("原始诊断 0xABC"),
            };
            let raw = json!({"Kind":kind,"Track":0,"Name":name,"Value":value}).to_string();
            for lang in [Lang::Zh, Lang::En, Lang::Ja] {
                let output = localized_log(lang, &raw);
                assert!(output.contains(name), "{kind}: {output}");
                assert!(!output.starts_with('{'));
                if kind == "Encoded" {
                    for token in ["96000", "24", "6", "7654321"] {
                        assert!(output.contains(token));
                    }
                }
                if kind == "PcmProgress" {
                    assert!(output.contains("47%"));
                }
            }
        }
    }

    #[test]
    fn menu_cover_progress_is_localized_with_album_and_position() {
        let raw = "[menu-cover] background 2/17 Album 名称";
        for (lang, label) in [
            (Lang::Zh, "菜单封面背景"),
            (Lang::En, "Menu cover background"),
            (Lang::Ja, "メニュー背景"),
        ] {
            let output = localized_log(lang, raw);
            assert!(output.contains(label), "{output}");
            assert!(
                output.contains("2/17") && output.contains("Album 名称"),
                "{output}"
            );
            assert!(!output.contains("720 576"), "{output}");
        }
    }

    #[test]
    fn oversized_playback_cover_warning_is_localized_as_nonfatal() {
        let raw = "[menu-cover] oversized 17";
        for (lang, expected) in [
            (Lang::Zh, "超过 2 MB，继续制作"),
            (Lang::En, "disc creation will continue"),
            (Lang::Ja, "作成を続行します"),
        ] {
            let output = localized_log(lang, raw);
            assert!(output.contains(expected), "{output}");
            assert!(output.contains("17"), "{output}");
        }
    }

    #[test]
    fn full_log_includes_records_beyond_display_limit_and_raw_diagnostics() {
        let mut lines = (0..4500)
            .map(|n| LogLine {
                raw: format!("record {n}"),
                problem: false,
                timestamp: "12:00:00".into(),
            })
            .collect::<Vec<_>>();
        lines.push(LogLine {
            raw: "[错误] No published ISO images were found [NO_ISO]".into(),
            problem: true,
            timestamp: "12:00:00".into(),
        });
        let output = full_log(Lang::Ja, &lines);
        assert!(output.contains("record 0\r\n") && output.contains("record 4499\r\n"));
        assert!(output.contains("[raw] [错误] No published ISO images were found [NO_ISO]"));
    }

    #[test]
    fn file_filters_keep_correct_extensions_and_localized_labels() {
        for lang in [Lang::Zh, Lang::En, Lang::Ja] {
                for (kind, extension) in [
                    (FileKind::Profile, "json"),
                    (FileKind::Log, "txt"),
                    ] {
                    let (filter, actual) = dialog_filter(lang, kind);
                    assert_eq!(actual, extension);
                    assert!(filter.ends_with("\0\0"));
                    assert!(filter.contains(tr(lang, "all_filter")));
                }
            }
        }

        #[test]
        fn import_and_custom_capacity_are_not_replaced_with_defaults() {
            assert_eq!(mode_value(mode_index("external")), "external");
            assert_eq!(mode_value(mode_index("lpcm")), "lpcm");
            assert_eq!(capacity_index(4_000_000_000), 2);
            assert_eq!(capacity_index(4_707_319_808), 0);
            assert_eq!(capacity_index(8_540_123_136), 1);
            assert_eq!(capacity_index(8_543_666_176), 2);
            assert!(valid_integer("0", 0, 10_000_000_000));
            assert!(!valid_integer("10000000001", 0, 10_000_000_000));
        }

        #[test]
        fn audio_mode_note_matches_the_selected_workflow() {
            assert_eq!(audio_note_key(0), "mlp_note");
            assert_eq!(audio_note_key(1), "lpcm_note");
            assert_eq!(audio_note_key(2), "import_note");
            assert_eq!(audio_note_key(99), "mlp_note");
            for lang in [Lang::Zh, Lang::En, Lang::Ja] {
                for mode in [0, 1, 2] {
                    assert!(!tr(lang, audio_note_key(mode)).is_empty());
                }
            }
        }

        #[test]
        fn detailed_view_ignores_summary_filter_and_preserves_chatter() {
            let lines = vec![
                LogLine {
                    raw: "frame=42".into(),
                    problem: false,
                    timestamp: "12:34:56".into(),
                },
                LogLine {
                    raw: "[WAR] warning".into(),
                    problem: true,
                    timestamp: "12:34:57".into(),
                },
            ];
            let detail = display_log(Lang::En, &lines, true, true);
            assert!(detail.contains("12:34:56  frame=42") && detail.contains("[WAR] warning"));
            let summary = display_log(Lang::En, &lines, false, false);
            assert!(!summary.contains("frame=42") && summary.contains("[WAR] warning"));
        }

        #[test]
        fn overall_progress_stays_monotonic_and_reserves_completion_for_success() {
            let (logs, events) = sync_channel(32);
            let mut callbacks = WorkerCallbacks {
                logs,
                cancel: Arc::new(AtomicBool::new(false)),
                range: (0, 100),
                last_progress: 0,
            };
            callbacks.set_progress_range(0, 15);
            callbacks.progress(50, 100);
            callbacks.progress(20, 100);
            callbacks.progress(0, 0);
            callbacks.progress(100, 100);
            callbacks.set_progress_range(15, 65);
            callbacks.progress(40, 100);
            callbacks.set_progress_range(65, 100);
            callbacks.progress(100, 100);
            let values: Vec<_> = events
                .try_iter()
                .map(|event| match event {
                    WorkerEvent::Progress(value) => value,
                    _ => panic!("unexpected log event"),
                })
                .collect();
            assert_eq!(values, [7, 15, 35, 65, 99]);
        }

        #[test]
        fn task_start_is_visible_in_summary_before_progress_events() {
            for (lang, key) in [
                (Lang::Zh, "started_check"),
                (Lang::En, "started_build"),
                (Lang::Ja, "started_verify"),
            ] {
                let lines = [LogLine {
                    raw: tr(Lang::Zh, key).into(),
                    problem: false,
                    timestamp: "12:34:56".into(),
                }];
                assert!(
                    !display_log(lang, &lines, false, false).trim().is_empty(),
                    "{key} should remain visible in the summary"
                );
            }
        }

        #[test]
        fn combo_measurement_preserves_original_item_height() {
            let mut item: MeasureItemStruct = unsafe { std::mem::zeroed() };
            unsafe {
                window_proc(
                    ptr::null_mut(),
                    WM_MEASUREITEM,
                    0,
                    &mut item as *mut MeasureItemStruct as Lparam,
                );
            }
            assert_eq!(item.item_height, scale(26).max(scale(20)) as u32);
        }


    /// Every text the UI shows must fit the rectangle it is drawn into, in
    /// every language, at every supported scale and width. Returns one line
    /// per (kind, text) with the first failing condition.
    fn text_clipping_findings(parent: Hwnd, ui: &mut UiState) -> Vec<String> {
        const DT_WORDBREAK: u32 = 0x10;
        const CB_GETCOUNT: u32 = 0x0146;
        const CB_GETDROPPEDWIDTH: u32 = 0x015f;
        const WM_GETFONT: u32 = 0x0031;
        let measure = |text: &str, font: Handle, width: i32, flags: u32| -> (i32, i32) {
            unsafe {
                let dc = CreateCompatibleDC(ptr::null_mut());
                let old = SelectObject(dc, font);
                let wide_text = wide(text);
                let mut rect = Rect { left: 0, top: 0, right: width.max(1), bottom: 0 };
                DrawTextW(dc, wide_text.as_ptr(), -1, &mut rect, flags | DT_CALCRECT | DT_NOPREFIX);
                SelectObject(dc, old);
                DeleteDC(dc);
                (rect.right, rect.bottom)
            }
        };
        let lift = |font: Handle, needed: i32, available: i32| unsafe {
            let dc = CreateCompatibleDC(ptr::null_mut());
            let old = SelectObject(dc, font);
            let lift = text_lift(dc, needed, available);
            SelectObject(dc, old);
            DeleteDC(dc);
            lift
        };
        let style = |hwnd: Hwnd| unsafe { GetWindowLongPtrW(hwnd, -16) } as u32;
        let shown = |hwnd: Hwnd| {
            let mut current = hwnd;
            while !current.is_null() && current != parent {
                if style(current) & WS_VISIBLE == 0 {
                    return false;
                }
                current = unsafe { GetParent(current) };
            }
            true
        };
        let client = |hwnd: Hwnd| {
            let mut rect: Rect = unsafe { std::mem::zeroed() };
            unsafe { GetClientRect(hwnd, &mut rect) };
            (rect.right, rect.bottom)
        };
        unsafe extern "system" fn collect(hwnd: Hwnd, list: Lparam) -> i32 {
            unsafe { (*(list as *mut Vec<Hwnd>)).push(hwnd) };
            1
        }
        let mut findings: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        let mut note = |key: String, detail: String| {
            findings.entry(key).or_insert(detail);
        };
        let saved_dpi = UI_DPI.load(Ordering::Relaxed);
        let saved_lang = ui.lang;
        for dpi in [96u32, 120, 144, 168] {
            UI_DPI.store(dpi, Ordering::Relaxed);
            rescale_ui(parent, ui);
            for lang in [Lang::Zh, Lang::En, Lang::Ja] {
                set_language(ui, lang);
                for advanced in [false, true] {
                    check(ui.controls.advanced, advanced);
                    update_advanced(ui);
                    for width in [640, 800, 1000, 1400] {
                        unsafe { MoveWindow(parent, 0, 0, scale(width), scale(820), 0) };
                        layout(parent, ui);
                        for tab in 0..ui.pages.len() {
                            select_tab(ui, tab);
                            let at = format!("{} {dpi}dpi {width}w adv={advanced} tab{tab}", lang.code());
                            let mut children: Vec<Hwnd> = Vec::new();
                            unsafe { EnumChildWindows(parent, collect, &mut children as *mut _ as Lparam) };
                            for hwnd in children.into_iter().filter(|hwnd| shown(*hwnd)) {
                                let class = window_class(hwnd);
                                let text = get_text(hwnd);
                                let (cw, ch) = client(hwnd);
                                let id = unsafe { GetDlgCtrlID(hwnd) } as usize;
                                let font = unsafe { SendMessageW(hwnd, WM_GETFONT, 0, 0) } as Handle;
                                let font = if font.is_null() { ui_font() } else { font };
                                match class.as_str() {
                                    // Status lines end in "..." by design; their texts
                                    // are checked below.
                                    "Static" if !text.is_empty() && style(hwnd) & 0x4000 == 0 => {
                                        // Labels wrap at word boundaries; draw_label lifts
                                        // text into the internal leading when needed.
                                        let (w, h) = measure(&text, font, cw, DT_WORDBREAK);
                                        if w > cw || h - lift(font, h, ch) > ch {
                                            note(format!("label {text:?}"), format!("needs {}x{} has {cw}x{ch} at {at}", unscale(w), unscale(h)));
                                        }
                                    }
                                    "Button" if style(hwnd) & 0xf == BS_OWNERDRAW && !text.is_empty() => {
                                        let (font, top, bottom, pad) = if (ID_TAB..ID_TAB + 3).contains(&id) {
                                            (semibold_font(), -2, 6, 8)
                                        } else if id == ID_BUILD {
                                            (semibold_font(), 0, 0, 8)
                                        } else {
                                            (ui_font(), 0, 0, 8)
                                        };
                                        let (w, h) = measure(&text, font, cw, DT_SINGLELINE);
                                        if w > cw - scale(pad) * 2 || h > ch - scale(top) - scale(bottom) {
                                            note(format!("button {text:?}"), format!("needs {}x{} has {}x{} at {at}", unscale(w), unscale(h), unscale(cw) - pad * 2, unscale(ch) - top - bottom));
                                        }
                                    }
                                    "Button" if matches!(style(hwnd) & 0xf, 2 | 3) && !text.is_empty() => {
                                        let available = cw - scale(25);
                                        let (w, h) = measure(&text, ui_font(), cw, DT_SINGLELINE);
                                        if w > available || h > ch {
                                            note(format!("checkbox {text:?}"), format!("needs {}x{} has {}x{} at {at}", unscale(w), unscale(h), unscale(available), unscale(ch)));
                                        }
                                    }
                                    "ComboBox" => {
                                        let count = unsafe { SendMessageW(hwnd, CB_GETCOUNT, 0, 0) } as i32;
                                        let closed = cw - scale(36);
                                        let dropped = (unsafe { SendMessageW(hwnd, CB_GETDROPPEDWIDTH, 0, 0) } as i32).max(cw) - scale(12);
                                        for index in 0..count {
                                            let item = unsafe { combo_item_text(hwnd, index) };
                                            let (w, _) = measure(&item, ui_font(), cw, DT_SINGLELINE);
                                            if w > closed {
                                                note(format!("combo {id} closed {item:?}"), format!("needs {} has {} at {at}", unscale(w), unscale(closed)));
                                            }
                                            if w > dropped {
                                                note(format!("combo {id} list {item:?}"), format!("needs {} has {} at {at}", unscale(w), unscale(dropped)));
                                            }
                                        }
                                    }
                                    "Edit" => {
                                        let (_, h) = measure("Agy国", font, cw, DT_SINGLELINE);
                                        if h > ch {
                                            note(format!("edit {id} height"), format!("needs {} has {} at {at}", unscale(h), unscale(ch)));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            let advanced_rect = {
                                let mut rect: Rect = unsafe { std::mem::zeroed() };
                                unsafe { GetWindowRect(ui.controls.advanced, &mut rect) };
                                rect
                            };
                            let mut last_tab: Rect = unsafe { std::mem::zeroed() };
                            unsafe { GetWindowRect(*ui.tabs.last().unwrap(), &mut last_tab) };
                            if last_tab.right > advanced_rect.left {
                                note("tabs overlap the advanced switch".into(), format!("at {at}"));
                            }
                            // Status texts the task area cycles through.
                            if tab == 0 && !advanced {
                                let (cw, ch) = client(ui.controls.status);
                                let status_font = unsafe { SendMessageW(ui.controls.status, WM_GETFONT, 0, 0) } as Handle;
                                for key in ["ready", "started_check", "started_build", "started_verify", "done", "cancelled", "failed", "stopping"] {
                                    let (w, h) = measure(tr(lang, key), status_font, cw, DT_SINGLELINE);
                                    if w > cw || h > ch {
                                        note(format!("status {:?}", tr(lang, key)), format!("needs {}x{} has {}x{} at {at}", unscale(w), unscale(h), unscale(cw), unscale(ch)));
                                    }
                                }
                                let stats = tr(lang, "statistics").replace("{0}", "99").replace("{1}", "59").replace("{2}", "59").replace("{3}", "9999");
                                let (cw, ch) = client(ui.controls.statistics);
                                let stats_font = unsafe { SendMessageW(ui.controls.statistics, WM_GETFONT, 0, 0) } as Handle;
                                let (w, h) = measure(&stats, stats_font, cw, DT_SINGLELINE);
                                if w > cw || h > ch {
                                    note(format!("statistics {stats:?}"), format!("needs {}x{} has {}x{} at {at}", unscale(w), unscale(h), unscale(cw), unscale(ch)));
                                }
                                // Header title and subtitle, as paint_window draws them.
                                let right = (scale(width) - scale(308)).max(scale(240));
                                let (w, h) = measure("DVD-Audio Maker", title_font(), right, DT_SINGLELINE);
                                if w > right - scale(72) || h > scale(34) {
                                    note("header title".into(), format!("needs {}x{} at {at}", unscale(w), unscale(h)));
                                }
                                // Narrow windows shorten the subtitle with "..." by design.
                                let (w, h) = measure(tr(lang, "app_subtitle"), subtitle_font(), right, DT_SINGLELINE);
                                if (width >= 1000 && w > right - scale(73)) || h > scale(22) {
                                    note(format!("header subtitle {:?}", tr(lang, "app_subtitle")), format!("needs {}x{} has {} at {at}", unscale(w), unscale(h), unscale(right) - 73));
                                }
                            }
                        }
                    }
                }
            }
        }
        UI_DPI.store(saved_dpi, Ordering::Relaxed);
        rescale_ui(parent, ui);
        set_language(ui, saved_lang);
        check(ui.controls.advanced, false);
        update_advanced(ui);
        select_tab(ui, 0);
        findings.into_iter().map(|(key, detail)| format!("{key}: {detail}")).collect()
    }

        #[test]
        fn real_controls_busy_cancel_bounded_archive_and_paused_completion() {
            #[link(name = "user32")]
            unsafe extern "system" {
                fn IsWindowEnabled(hwnd: Hwnd) -> i32;
            }
            use std::sync::atomic::AtomicU64;
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "dvda-gui-controls-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            let profile = root.join("settings.json");
            fs::write(
                &profile,
                b"{\"Version\":1,\"Language\":\"en\",\"Values\":{}}",
            )
            .unwrap();
            PROFILE_OVERRIDE.set(Some(profile)).unwrap();
            let page_class = wide("DVD_AUDIO_PAGE_RUST");
            let registration = WndClass {
                style: 0,
                procedure: page_proc,
                extra_class: 0,
                extra_window: 0,
                instance: unsafe { GetModuleHandleW(ptr::null()) },
                icon: ptr::null_mut(),
                cursor: ptr::null_mut(),
                background: ptr::null_mut(),
                menu: ptr::null(),
                class_name: page_class.as_ptr(),
            };
            unsafe {
                RegisterClassW(&registration);
                InitCommonControlsEx(&InitCommonControlsEx {
                    size: std::mem::size_of::<InitCommonControlsEx>() as u32,
                    classes: 0xffff,
                });
            }
            let parent = create!(ptr::null_mut(), "STATIC", "", 0, 0, 0, 1180, 820, 0);
            assert!(!parent.is_null());
            let mut ui = create_controls(parent);
            layout(parent, &mut ui);
            for (combo, width) in [
                (ui.controls.capacity, 220),
                (ui.controls.mode, 300),
                (ui.controls.rate, 210),
                (ui.controls.bits, 160),
                (ui.controls.language, 104),
                (ui.controls.log_view, 132),
            ] {
                let mut rect: Rect = unsafe { std::mem::zeroed() };
                unsafe { GetWindowRect(combo, &mut rect); }
                assert_eq!(rect.right - rect.left, scale(width));
            }
            let mut target_rect: Rect = unsafe { std::mem::zeroed() };
            unsafe { GetWindowRect(ui.controls.source, &mut target_rect); }
            assert_eq!(target_rect.bottom - target_rect.top, scale(24));
            let page = ui.pages[0];
            let max_scroll = unsafe { page_bar_metrics(&ui) }.max;
            scroll_page(page, scale(100), true);
            assert_eq!(unsafe { GetWindowLongPtrW(page, GWLP_USERDATA) }, scale(100).min(max_scroll) as isize);
            unsafe { layout_page_contents(&mut ui); }
            scroll_page(page, 0, true);
            for height in [500, 600, 820] {
                unsafe { MoveWindow(parent, 0, 0, scale(800), scale(height), 0); }
                layout(parent, &mut ui);
                let mut log_rect: Rect = unsafe { std::mem::zeroed() };
                let mut action_rect: Rect = unsafe { std::mem::zeroed() };
                unsafe {
                    GetWindowRect(ui.controls.log, &mut log_rect);
                    GetWindowRect(ui.controls.build, &mut action_rect);
                }
                assert!(log_rect.bottom <= action_rect.top, "log overlaps actions at height {height}");
            }
            // Layout measures translated labels, so sweep every language.
            let original_lang = ui.lang;
            for lang in [Lang::Zh, Lang::En, Lang::Ja] {
            ui.lang = lang;
            for width in [640, 800, 854, 960, 1053, 1054, 1180, 1400, 960, 640] {
                for height in [360, 500, 820] {
                    unsafe { MoveWindow(parent, 0, 0, scale(width), scale(height), 0); }
                    layout(parent, &mut ui);
                    // The log toolbar sits between the progress bar and the log,
                    // wrapping rather than hiding controls in narrow windows.
                    // Short windows hide the log first and toolbar controls
                    // only where they would reach the action buttons.
                    let rect_of = |hwnd: Hwnd| {
                        let mut rect: Rect = unsafe { std::mem::zeroed() };
                        unsafe { GetWindowRect(hwnd, &mut rect); }
                        rect
                    };
                    let shown = |hwnd: Hwnd| {
                        (unsafe { GetWindowLongPtrW(hwnd, -16) } as u32) & WS_VISIBLE != 0
                    };
                    let mut client: Rect = unsafe { std::mem::zeroed() };
                    unsafe { GetWindowRect(parent, &mut client); }
                    let progress = rect_of(ui.controls.progress);
                    let log_buttons: Vec<Hwnd> = ui.buttons.iter()
                        .filter(|(_, key)| matches!(*key, "copy" | "export"))
                        .map(|(handle, _)| *handle)
                        .collect();
                    assert_eq!(log_buttons.len(), 2);
                    let log = rect_of(ui.controls.log);
                    let actions = rect_of(ui.controls.build);
                    let toolbar = [ui.controls.log_view, ui.controls.only_issues, ui.controls.live]
                        .into_iter()
                        .chain(log_buttons.iter().copied())
                        .chain([label_handle(&ui, "log_section")]);
                    // The test window has a frame; the app's compact layout
                    // depends on its client height.
                    let mut inner: Rect = unsafe { std::mem::zeroed() };
                    unsafe { GetClientRect(parent, &mut inner); }
                    let compact = unscale(inner.bottom) < 500;
                    assert!(compact || shown(ui.controls.log), "log hidden at {width}x{height}");
                    for control in toolbar {
                        assert!(shown(control) || (!shown(ui.controls.log) && compact),
                            "{} hidden at {width}x{height}", get_text(control));
                    }
                    let row: Vec<(Hwnd, Rect)> = [ui.controls.log_view, ui.controls.only_issues, ui.controls.live]
                        .into_iter()
                        .chain(log_buttons.iter().copied())
                        .filter(|hwnd| shown(*hwnd))
                        .map(|hwnd| (hwnd, rect_of(hwnd)))
                        .collect();
                    for (index, (hwnd, rect)) in row.iter().enumerate() {
                        let text = get_text(*hwnd);
                        assert!(rect.top >= progress.bottom,
                            "{text} overlaps the progress bar at {width}x{height}");
                        assert!(rect.right <= client.right && rect.bottom <= client.bottom,
                            "{text} leaves the window at {width}x{height}");
                        assert!((!shown(ui.controls.log) || rect.bottom <= log.top) && rect.bottom <= actions.top,
                            "{text} overlaps the log or actions at {width}x{height}");
                        for (other, other_rect) in &row[index + 1..] {
                            assert!(rect.right <= other_rect.left || other_rect.right <= rect.left
                                || rect.bottom <= other_rect.top || other_rect.bottom <= rect.top,
                                "{text} overlaps {} at {width}x{height}", get_text(*other));
                        }
                    }
                    // Short windows show only the settings while idle and only
                    // the task status and log while a task runs.
                    let page = ui.pages[ui.tab_index];
                    let task_area = [ui.controls.status, ui.controls.progress,
                        label_handle(&ui, "operation_section")];
                    if compact {
                        // The fixture does not register the scrollbar class.
                        let bar_shown = ui.page_scrollbar.is_null() || shown(ui.page_scrollbar);
                        assert!(shown(page) && bar_shown && ui.tabs.iter().all(|tab| shown(*tab)),
                            "settings hidden while idle at {width}x{height}");
                        assert!(task_area.iter().chain([&ui.controls.log]).all(|control| !shown(*control)),
                            "task area shown while idle at {width}x{height}");
                        let page_rect = rect_of(page);
                        assert!(page_rect.bottom <= actions.top
                            && page_rect.bottom >= actions.top - scale(12) - 1,
                            "settings do not fill the short window at {width}x{height}");
                        ui.busy = true;
                        layout(parent, &mut ui);
                        assert!(ui.pages.iter().chain(&ui.tabs).chain([&ui.page_scrollbar, &ui.controls.advanced])
                            .all(|control| !shown(*control)),
                            "settings shown during a task at {width}x{height}");
                        assert!(task_area.iter().all(|control| shown(*control)),
                            "task area hidden during a task at {width}x{height}");
                        let status = rect_of(label_handle(&ui, "operation_section"));
                        let tab = rect_of(ui.tabs[0]);
                        assert!(status.top <= tab.top + scale(2),
                            "task area does not move up during a task at {width}x{height}");
                        for control in [ui.controls.log, ui.controls.log_view, ui.controls.progress]
                            .into_iter()
                            .chain(log_buttons.iter().copied())
                            .filter(|control| shown(*control))
                        {
                            assert!(rect_of(control).bottom <= actions.top,
                                "{} reaches the actions during a task at {width}x{height}", get_text(control));
                        }
                        // Selecting a tab must not reveal a page while hidden.
                        let next = (ui.tab_index + 1) % ui.pages.len();
                        select_tab(&mut ui, next);
                        assert!(ui.pages.iter().all(|page| !shown(*page)));
                        select_tab(&mut ui, 0);
                        ui.busy = false;
                        layout(parent, &mut ui);
                        assert!(shown(ui.pages[0]) && !shown(ui.controls.status));
                    } else {
                        assert!(shown(page) && task_area.iter().all(|control| shown(*control)),
                            "tall windows hide an area at {width}x{height}");
                    }
                    for (page_index, ids) in [
                        (0, &[ID_SOURCE, ID_FINAL, ID_TITLE, ID_CAPACITY, ID_CUSTOM_BYTES,
                            ID_WORK_DIR, ID_BROWSE_WORK, ID_ISO_PREFIX, ID_GROUP_LIMIT][..]),
                        (1, &[ID_MODE, ID_RATE, ID_BITS, ID_IMPORT_FOLDER, ID_BROWSE_IMPORT][..]),
                        (2, &[ID_TRACKS, ID_COVER, ID_FONT_SC, ID_FONT_JP, ID_FONT_KR,
                            1007, 1008, 1009][..]),
                    ] {
                        let page = ui.pages[page_index];
                        let mut bounds: Rect = unsafe { std::mem::zeroed() };
                        unsafe { GetWindowRect(page, &mut bounds); }
                        for id in ids {
                            let child = unsafe { GetDlgItem(page, *id as i32) };
                            assert!(!child.is_null());
                            let mut child_rect: Rect = unsafe { std::mem::zeroed() };
                            unsafe { GetWindowRect(child, &mut child_rect); }
                            assert!(child_rect.left >= bounds.left && child_rect.right <= bounds.right,
                                "control {id} overflows page {page_index} at {width}x{height}");
                        }
                    }
                }
            }
        }
        ui.lang = original_lang;
        let clipped = text_clipping_findings(parent, &mut ui);
        assert!(clipped.is_empty(), "clipped texts:\n{}", clipped.join("\n"));
        // Widening pixel by pixel must move every control edge in one
        // direction, changing course at most once at a layout breakpoint;
        // back-and-forth steps read as jitter while dragging the window edge.
        {
            unsafe extern "system" fn collect(hwnd: Hwnd, list: Lparam) -> i32 {
                unsafe { (*(list as *mut Vec<Hwnd>)).push(hwnd) };
                1
            }
            let saved_dpi = UI_DPI.load(Ordering::Relaxed);
            for dpi_value in [96u32, 144] {
                UI_DPI.store(dpi_value, Ordering::Relaxed);
                let mut children: Vec<Hwnd> = Vec::new();
                unsafe { EnumChildWindows(parent, collect, &mut children as *mut _ as Lparam); }
                let position = |hwnd: Hwnd| {
                    let mut rect: Rect = unsafe { std::mem::zeroed() };
                    unsafe { GetWindowRect(hwnd, &mut rect); }
                    let mut origin = Point { x: 0, y: 0 };
                    unsafe { ClientToScreen(parent, &mut origin); }
                    (rect.left - origin.x, rect.right - origin.x)
                };
                let mut history: Vec<Vec<(i32, i32)>> = vec![Vec::new(); children.len()];
                let mut frame: Rect = unsafe { std::mem::zeroed() };
                unsafe { MoveWindow(parent, 0, 0, scale(700), scale(820), 0); GetWindowRect(parent, &mut frame); }
                let mut client: Rect = unsafe { std::mem::zeroed() };
                unsafe { GetClientRect(parent, &mut client); }
                let border = (frame.right - frame.left) - client.right;
                let widths: Vec<i32> = (scale(640)..scale(1400)).collect();
                for px in &widths {
                    unsafe { MoveWindow(parent, 0, 0, px + border, scale(820), 0); }
                    layout(parent, &mut ui);
                    for (index, child) in children.iter().enumerate() {
                        history[index].push(position(*child));
                    }
                }
                for (index, child) in children.iter().enumerate() {
                    for (side, pick) in [("left", 0usize), ("right", 1)] {
                        let values: Vec<i32> = history[index].iter().map(|p| if pick == 0 { p.0 } else { p.1 }).collect();
                        let mut reversals = Vec::new();
                        let mut direction = 0;
                        for step in 1..values.len() {
                            let delta = values[step] - values[step - 1];
                            if delta == 0 { continue; }
                            let sign = delta.signum();
                            if direction != 0 && sign != direction {
                                reversals.push((unscale(widths[step]), values[step - 1], values[step]));
                            }
                            direction = sign;
                        }
                        assert!(reversals.len() <= 1,
                            "{} {:?} {side} edge jitters at {dpi_value} DPI: {:?}",
                            window_class(*child), get_text(*child), &reversals[..reversals.len().min(4)]);
                    }
                }
            }
            UI_DPI.store(saved_dpi, Ordering::Relaxed);
            unsafe { MoveWindow(parent, 0, 0, scale(1180), scale(820), 0); }
            layout(parent, &mut ui);
        }
        // Growing the window pixel by pixel must move every control edge in
        // one direction, changing course at most once on each side of the
        // short-window switch. Above the default layout's height the task
        // area must stay put: only the log's lower edge and the actions row
        // may follow a dragged bottom edge.
        {
            unsafe extern "system" fn collect(hwnd: Hwnd, list: Lparam) -> i32 {
                unsafe { (*(list as *mut Vec<Hwnd>)).push(hwnd) };
                1
            }
            let saved_dpi = UI_DPI.load(Ordering::Relaxed);
            for dpi_value in [96u32, 144] {
                UI_DPI.store(dpi_value, Ordering::Relaxed);
                let mut children: Vec<Hwnd> = Vec::new();
                unsafe { EnumChildWindows(parent, collect, &mut children as *mut _ as Lparam); }
                let position = |hwnd: Hwnd| {
                    let mut rect: Rect = unsafe { std::mem::zeroed() };
                    unsafe { GetWindowRect(hwnd, &mut rect); }
                    let mut origin = Point { x: 0, y: 0 };
                    unsafe { ClientToScreen(parent, &mut origin); }
                    (rect.top - origin.y, rect.bottom - origin.y)
                };
                let mut frame: Rect = unsafe { std::mem::zeroed() };
                unsafe { MoveWindow(parent, 0, 0, scale(1000), scale(820), 0); GetWindowRect(parent, &mut frame); }
                let mut client: Rect = unsafe { std::mem::zeroed() };
                unsafe { GetClientRect(parent, &mut client); }
                let border = (frame.bottom - frame.top) - client.bottom;
                // Growing switches out of the short layout at 504 DIPs.
                for (low, high) in [(360, 503), (504, 1200)] {
                    let heights: Vec<i32> = (scale(low)..=scale(high)).collect();
                    let mut history: Vec<Vec<(i32, i32)>> = vec![Vec::new(); children.len()];
                    let mut client_heights: Vec<i32> = Vec::new();
                    for px in &heights {
                        unsafe { MoveWindow(parent, 0, 0, scale(1000), px + border, 0); }
                        layout(parent, &mut ui);
                        let mut inner: Rect = unsafe { std::mem::zeroed() };
                        unsafe { GetClientRect(parent, &mut inner); }
                        client_heights.push(inner.bottom);
                        for (index, child) in children.iter().enumerate() {
                            history[index].push(position(*child));
                        }
                    }
                    for (index, child) in children.iter().enumerate() {
                        for (side, pick) in [("top", 0usize), ("bottom", 1)] {
                            let values: Vec<i32> = history[index].iter().map(|p| if pick == 0 { p.0 } else { p.1 }).collect();
                            let mut reversals = Vec::new();
                            let mut direction = 0;
                            for step in 1..values.len() {
                                let delta = values[step] - values[step - 1];
                                if delta == 0 { continue; }
                                let sign = delta.signum();
                                if direction != 0 && sign != direction {
                                    reversals.push((unscale(heights[step]), values[step - 1], values[step]));
                                }
                                direction = sign;
                            }
                            assert!(reversals.len() <= 1,
                                "{} {:?} {side} edge jitters at {dpi_value} DPI: {:?}",
                                window_class(*child), get_text(*child), &reversals[..reversals.len().min(4)]);
                        }
                    }
                    // Rows tied to the bottom edge keep an exact pixel gap to it.
                    for control in [ui.controls.build, ui.controls.statistics] {
                        let index = children.iter().position(|child| *child == control).unwrap();
                        let gaps: std::collections::BTreeSet<i32> = client_heights.iter().zip(&history[index])
                            .map(|(client_height, p)| client_height - p.0)
                            .collect();
                        assert_eq!(gaps.len(), 1, "{} gap to the bottom edge wobbles at {dpi_value} DPI: {gaps:?}",
                            get_text(control));
                    }
                    if low == 504 {
                        // Once the default split leaves the log its floor (tabs at
                        // 126, task rows 110, actions 60), only the log grows.
                        let fixed_from = ui.default_tab_height.unwrap() + 126 + 110 + 60 + DEFAULT_LOG_FLOOR;
                        // The test window cannot exceed the screen; judge the
                        // heights it really reached.
                        let reached = *client_heights.iter().max().unwrap();
                        assert!(reached >= scale(fixed_from + 20), "sweep reached only {} DIPs at {dpi_value} DPI", unscale(reached));
                        let status = children.iter().position(|child| *child == ui.controls.status).unwrap();
                        let log = children.iter().position(|child| *child == ui.controls.log).unwrap();
                        for control in [ui.controls.status, ui.controls.progress, ui.controls.log_view, ui.controls.log] {
                            let index = children.iter().position(|child| *child == control).unwrap();
                            let tops: std::collections::BTreeSet<i32> = client_heights.iter().zip(&history[index])
                                .filter(|(client_height, _)| **client_height >= scale(fixed_from))
                                .map(|(_, p)| p.0)
                                .collect();
                            assert_eq!(tops.len(), 1, "{} moves with the height at {dpi_value} DPI", window_class(control));
                        }
                        // Below that the task area may only follow the edge while
                        // the log is at its floor.
                        for step in 1..history[status].len() {
                            if history[status][step].0 != history[status][step - 1].0 {
                                let (top, bottom) = history[log][step];
                                assert!((bottom - top - scale(DEFAULT_LOG_FLOOR)).abs() <= 1,
                                    "task area moves while the log has room at {dpi_value} DPI: log {} px", bottom - top);
                            }
                        }
                    }
                }
            }
            UI_DPI.store(saved_dpi, Ordering::Relaxed);
            unsafe { MoveWindow(parent, 0, 0, scale(1180), scale(820), 0); }
            layout(parent, &mut ui);
        }
        // Live resizing must not resize, and so re-wrap, a long log: the box
        // keeps its size (clipped to the new log area) until the drag ends.
        {
            let line = "2026-10-24 12:00:00  Encoding track 07 of 12 (MLP, 96 kHz / 24 bit, 5.1)";
            let text: String = (0..2500).map(|i| format!("{i:04} {line}{}{}", 13 as char, 10 as char)).collect();
            set_text(ui.controls.log, &text);
            unsafe { MoveWindow(parent, 0, 0, scale(1180), scale(820), 0); }
            layout(parent, &mut ui);
            let log_size = |hwnd: Hwnd| {
                let mut rect: Rect = unsafe { std::mem::zeroed() };
                unsafe { GetClientRect(hwnd, &mut rect) };
                (rect.right, rect.bottom)
            };
            let before = log_size(ui.controls.log);
            ui.sizing = true;
            for step in 1..=20 {
                unsafe { MoveWindow(parent, 0, 0, scale(1180 - step * 3), scale(820 - step * 3), 0); }
                layout(parent, &mut ui);
                assert_eq!(log_size(ui.controls.log), before, "log resized during live sizing");
            }
            assert!(ui.log_frozen);
            // The frame follows the layout while the EDIT keeps its size.
            let mut frame: Rect = unsafe { std::mem::zeroed() };
            unsafe { GetClientRect(ui.log_frame, &mut frame) };
            let target = ui.log_box.unwrap();
            assert_eq!((frame.right, frame.bottom), (target.right - target.left, target.bottom - target.top));
            ui.sizing = false;
            layout(parent, &mut ui);
            assert!(!ui.log_frozen);
            let target = ui.log_box.unwrap();
            assert_eq!(log_size(ui.controls.log), (target.right - target.left, target.bottom - target.top));
            // The real sizing messages: a resize is not a user scroll, so live
            // follow is not paused, and the log gets its size at the end.
            unsafe { SendMessageW(ui.controls.log, EM_LINESCROLL, 0, 2500) };
            observe_log_scroll(&mut ui);
            ui.log_hold_line = None;
            ui.log_hold_until = None;
            unsafe { SetWindowLongPtrW(parent, GWLP_USERDATA, &mut ui as *mut UiState as isize) };
            unsafe { window_proc(parent, 0x0231, 0, 0) };
            // Growing a log scrolled to its end reveals earlier lines, which
            // changes its first visible line.
            for step in 1..=10 {
                unsafe { MoveWindow(parent, 0, 0, scale(1180), scale(820 + step * 4), 0); }
                layout(parent, &mut ui);
            }
            unsafe { window_proc(parent, 0x0232, 0, 0) };
            unsafe { SetWindowLongPtrW(parent, GWLP_USERDATA, 0) };
            assert!(!ui.sizing && !ui.log_frozen);
            // The log timer observes the scroll position next.
            observe_log_scroll(&mut ui);
            assert!(ui.log_hold_until.is_none(), "resizing paused live follow");
            let target = ui.log_box.unwrap();
            assert_eq!(log_size(ui.controls.log), (target.right - target.left, target.bottom - target.top));
            // Esc in the log posts WM_CLOSE to its parent frame; the log survives.
            {
                #[link(name = "user32")]
                unsafe extern "system" { fn IsWindow(hwnd: Hwnd) -> i32; }
                unsafe { SendMessageW(ui.log_frame, WM_CLOSE, 0, 0) };
                assert!(unsafe { IsWindow(ui.log_frame) } != 0 && unsafe { IsWindow(ui.controls.log) } != 0);
            }
            set_text(ui.controls.log, "");
            unsafe { MoveWindow(parent, 0, 0, scale(1180), scale(820), 0); }
            layout(parent, &mut ui);
        }
        // Exercise the actual controls rather than passing handcrafted blank
        // overrides to the option evaluator: population must retain auto values.
        assert_eq!(get_text(ui.controls.iso_prefix), "");
        assert_eq!(get_text(ui.controls.import_folder), "");
        set_text(ui.controls.title, "Changed title");
        let changed_work = root.join("changed-work");
        set_text(ui.controls.work_dir, &changed_work.to_string_lossy());
        let edited = edited_profile_values(&ui);
        assert_eq!(edited["DVDA_ISO_PREFIX"], "");
        assert_eq!(edited["DVDA_MLP_EXTERNAL_DIR"], "");
        let evaluated = ui
            .base
            .as_ref()
            .unwrap()
            .with_profile_values(edited)
            .unwrap();
        assert_eq!(evaluated.text("IsoPrefix"), "Changed_title");
        assert_eq!(
            PathBuf::from(evaluated.text("MlpExternalDirectory")),
            changed_work.join("mlp")
        );
        populate_numeric_choice(ui.controls.rate, SAMPLE_RATES, SAMPLE_RATE_LABELS, 12345);
        populate_numeric_choice(ui.controls.bits, SAMPLE_BITS, &["16", "20", "24"], 17);
        assert_eq!(get_text(ui.controls.rate), "12345");
        assert_eq!(get_text(ui.controls.bits), "17");
        assert_eq!(edited_profile_values(&ui)["DVDA_MLP_SAMPLE_RATE"], "12345");
        assert!(validate_controls(&mut ui, false));
        assert!(!validate_controls(&mut ui, true));
        combo_select(ui.controls.rate, 1);
        assert!(!validate_controls(&mut ui, true));
        combo_select(ui.controls.bits, 2);
        assert!(validate_controls(&mut ui, true));
        if let Some((file, path)) = ui.log_file.take() {
            drop(file);
            fs::remove_file(path).unwrap();
        }
        let source = PathBuf::from(&root).join("complete.log");
        ui.log_file = Some((fs::File::create(&source).unwrap(), source.clone()));
        ui.logs.clear();
        ui.problem_count = 0;
        check(ui.controls.live, false);
        for index in 0..2700 {
            add_log(&mut ui, &format!("record {index}"), false);
        }
        assert!(ui.logs.len() <= 2500);
        let exported = root.join("export.log");
        ui.log_sender
            .try_send(WorkerEvent::Log("queued-before-export".into()))
            .unwrap();
        export_log_to(&mut ui, &exported).unwrap();
        let archived = fs::read_to_string(&exported).unwrap();
        assert!(archived.contains("record 0\n") && archived.contains("record 2699\n"));
        assert!(archived.contains("queued-before-export\n"));
        assert!(export_log_to(&mut ui, &source).is_err());
        assert_eq!(fs::read_to_string(&source).unwrap(), archived);
        let page = ui.pages[0];
        let initially_disabled = create!(page, "EDIT", "Locked", WS_CHILD, 0, 0, 100, 30, 0);
        let initially_enabled = create!(page, "EDIT", "Editable", WS_CHILD, 0, 40, 100, 30, 0);
        unsafe { EnableWindow(initially_disabled, 0) };
        ui.busy = true;
        set_busy_controls(&mut ui, true);
        assert_eq!(unsafe { IsWindowEnabled(initially_enabled) }, 0);
        assert_eq!(unsafe { IsWindowEnabled(initially_disabled) }, 0);
        for _ in 0..20 {
            scroll_page(page, i32::MAX, true);
            assert!(unsafe { GetWindowLongPtrW(page, GWLP_USERDATA) } > 0);
            scroll_page(page, 0, true);
            assert_eq!(unsafe { GetWindowLongPtrW(page, GWLP_USERDATA) }, 0);
        }
        assert_eq!(unsafe { IsWindowEnabled(ui.controls.language) }, 0);
        assert_eq!(unsafe { IsWindowEnabled(ui.controls.profile) }, 0);
        for page in &ui.pages {
            assert_ne!(unsafe { IsWindowEnabled(*page) }, 0);
        }
        let page = ui.pages[0];
        let label = create!(page, "STATIC", "Busy label", WS_CHILD, 0, 0, 100, 30, 0);
        let edit = create!(page, "EDIT", "Busy input", WS_CHILD, 0, 40, 100, 30, 0);
        unsafe {
            EnableWindow(page, 0);
            let dc = GetDC(page);
            assert!(!dc.is_null());
            assert_eq!(
                page_proc(page, WM_CTLCOLORSTATIC, dc as usize, label as isize),
                panel_brush() as Lresult,
            );
            assert_eq!(
                page_proc(page, WM_CTLCOLOREDIT, dc as usize, edit as isize),
                brush(c_disabled_fill()) as Lresult,
            );
            EnableWindow(edit, 0);
            assert_eq!(
                page_proc(page, WM_CTLCOLORSTATIC, dc as usize, edit as isize),
                brush(c_disabled_fill()) as Lresult,
            );
            EnableWindow(page, 1);
            assert_eq!(
                page_proc(page, WM_CTLCOLORSTATIC, dc as usize, label as isize),
                panel_brush() as Lresult,
            );
            EnableWindow(edit, 1);
            assert_eq!(
                page_proc(page, WM_CTLCOLOREDIT, dc as usize, edit as isize),
                card_brush() as Lresult,
            );
            ReleaseDC(page, dc);
            DestroyWindow(label);
            DestroyWindow(edit);
        }
        let previous_lang = ui.lang.code();
        combo_select(ui.controls.language, 2);
        handle_command(ptr::null_mut(), &mut ui, ID_LANGUAGE, CBN_SELCHANGE);
        assert_eq!(ui.lang.code(), previous_lang);
        let cancel = Arc::new(AtomicBool::new(false));
        ui.cancel = Some(Arc::clone(&cancel));
        unsafe {
            SetWindowLongPtrW(parent, GWLP_USERDATA, &mut ui as *mut UiState as isize);
            window_proc(parent, WM_CLOSE, 0, 0);
            SetWindowLongPtrW(parent, GWLP_USERDATA, 0);
        }
        assert!(ui.close_when_done);
        assert!(cancel.load(Ordering::Relaxed));
        assert_eq!(unsafe { IsWindowEnabled(ui.controls.cancel) }, 0);
        finish_operation(&mut ui, 2, "operation returned interrupted");
        assert!(get_text(ui.controls.log).contains(tr(ui.lang, "cancelled")));
        assert!(!get_text(ui.controls.log).contains("operation returned interrupted"));
        assert_ne!(unsafe { IsWindowEnabled(ui.controls.language) }, 0);
        assert_ne!(unsafe { IsWindowEnabled(initially_enabled) }, 0);
        assert_eq!(unsafe { IsWindowEnabled(initially_disabled) }, 0);
        unsafe {
            DestroyWindow(initially_enabled);
            DestroyWindow(initially_disabled);
        }
        assert!(!ui.busy);
        // More records than a Win32 message queue can hold must all reach the
        // archive before completion. Exercise the actual controls while live
        // viewing is paused, resumed and then completed with warnings.
        ui.close_when_done = false;
        ui.logs.clear();
        ui.problem_count = 0;
        ui.busy = true;
        ui.started = Some(Instant::now() - Duration::from_secs(2));
        set_busy_controls(&mut ui, true);
        let stress_path = root.join("stress.log");
        ui.log_file = Some((fs::File::create(&stress_path).unwrap(), stress_path.clone()));
        let producer_logs = ui.log_sender.clone();
        let producer = thread::spawn(move || {
            let mut callbacks = WorkerCallbacks {
                logs: producer_logs,
                cancel: Arc::new(AtomicBool::new(false)),
                range: (0, 100),
                last_progress: 0,
            };
            for index in 0..12050 {
                callbacks.emit(1, &format!("out_time_us={index}"));
            }
            callbacks.emit(2, "[警告] last warning\n    日本語.flac");
            callbacks
                .logs
                .send(WorkerEvent::Finished(UiEvent {
                    kind: 1,
                    text: "Completed".into(),
                }))
                .unwrap();
        });
        let paused_text = get_text(ui.controls.log);
        drain_worker_logs(ptr::null_mut(), &mut ui);
        assert_eq!(get_text(ui.controls.log), paused_text);
        check(ui.controls.live, true);
        handle_command(ptr::null_mut(), &mut ui, ID_LIVE, BN_CLICKED);
        let deadline = Instant::now() + Duration::from_secs(10);
        while ui.busy && Instant::now() < deadline {
            drain_worker_logs(ptr::null_mut(), &mut ui);
            update_statistics(&mut ui);
            thread::yield_now();
        }
        assert!(!ui.busy, "completion was lost after a progress burst");
        assert_eq!(ui.progress_value, 100);
        producer.join().unwrap();
        assert!(ui.logs.len() <= 2500);
        assert!(get_text(ui.controls.status).contains("Review the warnings"));
        assert!(get_text(ui.controls.log).contains("last warning"));
        assert!(get_text(ui.controls.log).contains("日本語.flac"));
        assert!(ui.elapsed_seconds >= 2);
        assert!(
            get_text(ui.controls.statistics).contains(&format!("0:00:{:02}", ui.elapsed_seconds))
        );
        let archived = fs::read_to_string(&stress_path).unwrap();
        let progress = archived
            .lines()
            .filter_map(|line| {
                line.split_once("out_time_us=")
                    .map(|(_, index)| index.parse::<usize>().unwrap())
            })
            .collect::<Vec<_>>();
        assert_eq!(progress, (0..12050).collect::<Vec<_>>());
        assert!(archived.ends_with("Completed\n"));
        // Decode chatter must not evict task history from the summary view.
        ui.log_file = None;
        ui.logs.clear();
        ui.summary_logs.clear();
        ui.log_render_language = None;
        ui.log_hold_line = None;
        ui.log_hold_until = None;
        combo_select(ui.controls.log_view, 0);
        check(ui.controls.only_issues, false);
        unsafe {
            MoveWindow(ui.controls.log, 0, 0, 600, 80, 0);
        }
        for track in 0..40 {
            add_log(
                &mut ui,
                &format!(
                    "[TASK] {}",
                    json!({
                        "Action": "source_decode", "Current": track, "Total": 40,
                        "Name": format!("track-{track:02}"), "Detail": ""
                    })
                ),
                false,
            );
            for index in 0..100 {
                add_log(&mut ui, &format!("out_time_us={track}{index:02}"), false);
            }
            render_log(&mut ui);
        }
        assert!(ui.logs.len() <= 2500);
        assert_eq!(ui.summary_logs.len(), 40);
        let summary = get_text(ui.controls.log);
        assert!(summary.contains("track-00") && summary.contains("track-39"));
        assert_eq!(summary.lines().count(), 40);
        assert!(
            first_visible_log_line(ui.controls.log) > 30,
            "tail-following did not reveal the newest summary row: first={}",
            first_visible_log_line(ui.controls.log)
        );
        // Preserve exactly the existing hold interval and the user's selection
        // when new rows arrive during upward review.
        unsafe {
            SendMessageW(ui.controls.log, EM_SETSEL, 5, 12);
            SendMessageW(ui.controls.log, EM_LINESCROLL, 0, -1000);
            SendMessageW(ui.controls.log, EM_LINESCROLL, 0, 8);
        }
        observe_log_scroll(&mut ui);
        let held_line = first_visible_log_line(ui.controls.log);
        let held_until = ui.log_hold_until.unwrap();
        assert!(held_line > 0 && held_line < 15);
        add_log(&mut ui, "[build] summary-after-review", false);
        render_log(&mut ui);
        assert_eq!(first_visible_log_line(ui.controls.log), held_line);
        assert_eq!(ui.log_hold_until, Some(held_until));
        let mut start = 0u32;
        let mut end = 0u32;
        unsafe {
            SendMessageW(
                ui.controls.log,
                EM_GETSEL,
                &mut start as *mut u32 as usize,
                &mut end as *mut u32 as isize,
            );
        }
        assert_eq!((start, end), (5, 12));
        ui.log_hold_until = Some(Instant::now() - Duration::from_secs(1));
        add_log(&mut ui, "[build] summary-after-hold", false);
        render_log(&mut ui);
        assert!(first_visible_log_line(ui.controls.log) > 30);
        assert!(ui.log_hold_until.is_none());
        // Detail-buffer rotation invalidates its render cursor even if its
        // length has grown past the old count again before the next frame.
        combo_select(ui.controls.log_view, 1);
        render_log(&mut ui);
        for index in 0..3000 {
            add_log(&mut ui, &format!("detail-rotation-{index}"), false);
        }
        render_log(&mut ui);
        assert_eq!(
            get_text(ui.controls.log),
            display_log(ui.lang, &ui.logs, true, false)
        );
        combo_select(ui.controls.log_view, 0);
        render_log(&mut ui);
        assert!(get_text(ui.controls.log).contains("track-00"));
        assert!(get_text(ui.controls.log).contains("summary-after-hold"));
        // Close the per-test archive before deleting its own temporary folder.
        ui.log_file = None;
        unsafe {
            DestroyWindow(ui.tooltip_window);
            DestroyWindow(parent);
        }
        drop(ui);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn log_queue_applies_backpressure_and_keeps_completion_after_diagnostics() {
        use std::sync::mpsc::TrySendError;
        let (sender, receiver) = sync_channel(LOG_QUEUE_CAPACITY);
        for index in 0..LOG_QUEUE_CAPACITY {
            sender
                .try_send(WorkerEvent::Log(index.to_string()))
                .unwrap();
        }
        assert!(matches!(
            sender.try_send(WorkerEvent::Log("overflow".into())),
            Err(TrySendError::Full(_))
        ));
        let worker = thread::spawn(move || {
            sender
                .send(WorkerEvent::Finished(UiEvent {
                    kind: 1,
                    text: "done".into(),
                }))
                .unwrap();
        });
        for index in 0..LOG_QUEUE_CAPACITY {
            match receiver.recv().unwrap() {
                WorkerEvent::Log(text) => assert_eq!(text, index.to_string()),
                WorkerEvent::Progress(_) => panic!("progress overtook a diagnostic"),
                WorkerEvent::Retention(_) => panic!("unexpected retention request"),
                WorkerEvent::Finished(_) => panic!("completion overtook a diagnostic"),
            }
        }
        assert!(matches!(receiver.recv().unwrap(), WorkerEvent::Finished(_)));
        worker.join().unwrap();
    }

    #[test]
    fn settings_help_and_new_runtime_messages_cover_all_languages() {
        let help: HashMap<String, String> =
            serde_json::from_str(include_str!("../locales/help.json")).unwrap();
        // DVDA_PREPARE_CACHE was retired together with its setting, so the
        // help catalogue now has one entry fewer than the original baseline.
        assert_eq!(help.len(), 27);
        for (key, original) in help {
            assert_eq!(setting_help(Lang::Zh, &key), original);
            for lang in [Lang::En, Lang::Ja] {
                let translated = setting_help(lang, &key);
                assert!(
                    !translated.is_empty() && translated != original,
                    "{key}: {translated}"
                );
            }
        }
        for source in [
            "LPCM 准备已取消。",
            "内置组件缺失: dvda-media.dll",
            "Menu data directory is missing: C:/data",
            "MLP 编码结果无效: C:/music.flac",
        ] {
            for lang in [Lang::En, Lang::Ja] {
                assert_ne!(localized_detail(lang, source), source);
            }
        }
    }

    #[test]
    fn unicode_paths_are_opaque_in_errors_and_problem_classification() {
        let path = "D:/失败/[NO_ISO]/{0}/日本語.flac";
        let error = format!("[错误] 源文件不存在: {path} [SOURCE_MISSING]");
        for lang in [Lang::Zh, Lang::En, Lang::Ja] {
            let translated = localized_detail(lang, &error);
            assert!(translated.contains(path) && translated.ends_with("[SOURCE_MISSING]"));
        }
        let progress = json!({"Kind":"PcmProgress","Name":path,"Track":0,"Value":50}).to_string();
        assert!(!log_is_problem(&progress));
    }
}
