#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[macro_use]
mod macros;
mod presentation;
mod win;

#[cfg(test)]
/// Tests share the process-wide UI DPI and control registries; they run one at a time.
static UI_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
fn ui_test_guard() -> std::sync::MutexGuard<'static, ()> {
    UI_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

mod touch;

use dvda_core::localization;
include!(concat!(env!("OUT_DIR"), "/runtime.rs"));

use dvda_core::app::AppOptions;
use dvda_native::media::Callbacks;
use serde_json::{Map, Value, json};
use std::{
    cell::{Cell, RefCell, RefMut},
    collections::HashMap,
    ffi::{OsStr, c_void},
    fs,
    io::Write,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr,
    rc::Rc,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    thread,
    time::{Duration, Instant},
};

// The UI split by topic. Each module sees the declarations in this file
// through `use super::*`.
mod commands;
mod controls;
mod dialogs;
mod draw;
mod frame;
mod gdi;
mod i18n;
mod layout;
mod logs;
mod pages;
mod scrollbar;
mod settings;
mod task;
mod theme;
mod window;
#[cfg(test)]
mod presentation_tests;

use commands::*;
use controls::*;
use dialogs::*;
use draw::*;
use frame::*;
use gdi::*;
use i18n::*;
use layout::*;
use logs::*;
use pages::*;
use scrollbar::*;
use settings::*;
use task::*;
use theme::*;
use window::*;

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
#[derive(Default)]
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
#[derive(Default)]
#[repr(C)]
struct MonitorInfo {
    size: u32,
    monitor: Rect,
    work: Rect,
    flags: u32,
}

/// TEXTMETRICW
#[derive(Default)]
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

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[derive(Clone, Copy, Default)]
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
    safe fn GetDlgCtrlID(hwnd: Hwnd) -> i32;
    safe fn GetDlgItem(parent: Hwnd, id: i32) -> Hwnd;
    safe fn GetParent(hwnd: Hwnd) -> Hwnd;
    safe fn GetAncestor(hwnd: Hwnd, flags: u32) -> Hwnd;
    safe fn IsWindow(hwnd: Hwnd) -> i32;
    fn GetClassNameW(hwnd: Hwnd, text: *mut u16, length: i32) -> i32;
    safe fn ShowWindow(hwnd: Hwnd, command: i32) -> i32;
    safe fn UpdateWindow(hwnd: Hwnd) -> i32;
    safe fn IsWindowVisible(hwnd: Hwnd) -> i32;
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
    safe fn MoveWindow(hwnd: Hwnd, x: i32, y: i32, width: i32, height: i32, repaint: i32) -> i32;
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
    safe fn GetClientRect(hwnd: Hwnd, rect: &mut Rect) -> i32;
    safe fn GetWindowRect(hwnd: Hwnd, rect: &mut Rect) -> i32;
    safe fn GetMessageW(message: &mut Message, hwnd: Hwnd, minimum: u32, maximum: u32) -> i32;
    safe fn TranslateMessage(message: &Message) -> i32;
    fn DispatchMessageW(message: *const Message) -> Lresult;
    fn IsDialogMessageW(hwnd: Hwnd, message: *mut Message) -> i32;
    safe fn PostQuitMessage(code: i32);
    fn PostMessageW(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> i32;
    fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, value: Lresult) -> Lresult;
    safe fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> Lresult;
    safe fn DestroyWindow(hwnd: Hwnd) -> i32;
    fn SetWindowTextW(hwnd: Hwnd, text: *const u16) -> i32;
    fn GetWindowTextLengthW(hwnd: Hwnd) -> i32;
    fn GetWindowTextW(hwnd: Hwnd, text: *mut u16, length: i32) -> i32;
    fn DrawTextW(dc: Handle, text: *const u16, length: i32, rect: *mut Rect, format: u32) -> i32;
    fn DrawFocusRect(dc: Handle, rect: *const Rect) -> i32;
    safe fn FillRect(dc: Handle, rect: &Rect, brush: Hbrush) -> i32;
    fn SendMessageW(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
    fn SetTimer(hwnd: Hwnd, id: usize, millis: u32, callback: *const c_void) -> usize;
    safe fn KillTimer(hwnd: Hwnd, id: usize) -> i32;
    safe fn SetFocus(hwnd: Hwnd) -> Hwnd;
    safe fn SetCapture(hwnd: Hwnd) -> Hwnd;
    safe fn ReleaseCapture() -> i32;
    safe fn GetCapture() -> Hwnd;
    safe fn GetCursorPos(point: &mut Point) -> i32;
    safe fn SetLayeredWindowAttributes(hwnd: Hwnd, key: u32, alpha: u8, flags: u32) -> i32;
    safe fn GetSystemMetrics(index: i32) -> i32;
    safe fn MonitorFromPoint(point: Point, flags: u32) -> Handle;
    safe fn GetMonitorInfoW(monitor: Handle, info: &mut MonitorInfo) -> i32;
    safe fn GetDpiForWindow(hwnd: Hwnd) -> u32;
    safe fn GetWindowPlacement(hwnd: Hwnd, placement: &mut WindowPlacement) -> i32;
    safe fn SetWindowPlacement(hwnd: Hwnd, placement: &WindowPlacement) -> i32;
    safe fn SetCursor(cursor: Handle) -> Handle;
    safe fn EnableWindow(hwnd: Hwnd, enable: i32) -> i32;
    fn MessageBoxW(hwnd: Hwnd, text: *const u16, caption: *const u16, flags: u32) -> i32;
    fn LoadCursorW(instance: Hinstance, name: *const u16) -> Handle;
    safe fn GetSysColorBrush(index: i32) -> Hbrush;
    fn InvalidateRect(hwnd: Hwnd, rect: *const Rect, erase: i32) -> i32;
    fn BeginPaint(hwnd: Hwnd, paint: *mut PaintStruct) -> Handle;
    fn EndPaint(hwnd: Hwnd, paint: *const PaintStruct) -> i32;
    #[cfg(test)]
    safe fn GetDC(hwnd: Hwnd) -> Handle;
    #[cfg(test)]
    safe fn ReleaseDC(hwnd: Hwnd, dc: Handle) -> i32;
    safe fn GetFocus() -> Hwnd;
    safe fn IsWindowEnabled(hwnd: Hwnd) -> i32;
    safe fn IsZoomed(hwnd: Hwnd) -> i32;
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
    safe fn GetDpiForSystem() -> u32;
    fn EnumChildWindows(hwnd: Hwnd, callback: EnumChildProc, parameter: Lparam) -> i32;
    fn SystemParametersInfoW(action: u32, param: u32, data: *mut c_void, update: u32) -> i32;
    safe fn ScreenToClient(hwnd: Hwnd, point: &mut Point) -> i32;
    safe fn ClientToScreen(hwnd: Hwnd, point: &mut Point) -> i32;
    safe fn OpenClipboard(owner: Hwnd) -> i32;
    safe fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, data: Handle) -> Handle;
    safe fn CloseClipboard() -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Hinstance;
    fn LoadLibraryW(name: *const u16) -> Hinstance;
    fn GetProcAddress(module: Hinstance, name: *const u8) -> *const c_void;
    safe fn GetUserDefaultUILanguage() -> u16;
    fn GlobalAlloc(flags: u32, bytes: usize) -> Handle;
    fn GlobalLock(handle: Handle) -> *mut c_void;
    safe fn GlobalUnlock(handle: Handle) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    safe fn GetStockObject(index: i32) -> Handle;
    safe fn SetBkMode(dc: Handle, mode: i32) -> i32;
    safe fn SetTextColor(dc: Handle, color: u32) -> u32;
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
    safe fn CreateSolidBrush(color: u32) -> Hbrush;
    safe fn CreatePen(style: i32, width: i32, color: u32) -> Handle;
    fn MoveToEx(dc: Handle, x: i32, y: i32, previous: *mut Point) -> i32;
    safe fn LineTo(dc: Handle, x: i32, y: i32) -> i32;
    safe fn Ellipse(dc: Handle, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    safe fn DeleteObject(object: Handle) -> i32;
    safe fn SelectObject(dc: Handle, object: Handle) -> Handle;
    safe fn CreateCompatibleDC(dc: Handle) -> Handle;
    safe fn GetTextMetricsW(dc: Handle, metrics: &mut TextMetric) -> i32;
    safe fn CreateCompatibleBitmap(dc: Handle, width: i32, height: i32) -> Handle;
    safe fn BitBlt(dest: Handle, x: i32, y: i32, width: i32, height: i32, source: Handle, source_x: i32, source_y: i32, rop: u32) -> i32;
    safe fn DeleteDC(dc: Handle) -> i32;
    safe fn SetBkColor(dc: Handle, color: u32) -> u32;
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

type FontKey = (i32, i32, u8);

static FONT_CACHE: OnceLock<Mutex<HashMap<FontKey, usize>>> = OnceLock::new();

static BRUSH_CACHE: OnceLock<Mutex<HashMap<u32, usize>>> = OnceLock::new();

/// Design rectangle (x, y, width, height in DIPs) of each control, rescaled on DPI change.
type ControlRect = (isize, i32, i32, i32, i32);

static CONTROL_RECTS: OnceLock<Mutex<Vec<ControlRect>>> = OnceLock::new();

static COMBO_OVERLAYS: OnceLock<Mutex<Vec<(isize, isize)>>> = OnceLock::new();

static ICON_CACHE: OnceLock<Mutex<HashMap<u32, usize>>> = OnceLock::new();

static DARK_THEME: AtomicBool = AtomicBool::new(false);

static DEFER_CONTROL_PAINT: AtomicBool = AtomicBool::new(false);

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
    Retention(Box<AppOptions>),
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

/// What the paint and scrollbar code reads from the main window. It lives apart from
/// `UiState`, so paint that runs inside a layout (which holds the state's borrow)
/// still reads it. The changing fields are `Cell`s, written by `UiState`'s setters.
struct PaintFields {
    lang: Cell<Lang>,
    tab_index: Cell<usize>,
    splitter_position: Cell<i32>,
    pages: Vec<Hwnd>,
    page_scrollbar: Hwnd,
    log: Hwnd,
    log_scrollbar: Hwnd,
    muted: Vec<Hwnd>,
}

thread_local! {
    static PAINT_FIELDS: RefCell<HashMap<usize, Rc<PaintFields>>> = RefCell::new(HashMap::new());
}

/// The paint fields of the main window `main`, or `None` if it is not set up.
fn paint_fields(main: Hwnd) -> Option<Rc<PaintFields>> {
    PAINT_FIELDS.with(|fields| fields.borrow().get(&(main as usize)).cloned())
}

struct UiState {
    paint: Rc<PaintFields>,
    main_window: Hwnd,
    controls: Controls,
    custom_bytes_label: Hwnd,
    import_folder_label: Hwnd,
    import_browse: Hwnd,
    pages: Vec<Hwnd>,
    wheel_remainders: HashMap<usize, i32>,
    tabs: Vec<Hwnd>,
    labels: Vec<(Hwnd, &'static str)>,
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

impl UiState {
    fn lang(&self) -> Lang {
        self.paint.lang.get()
    }

    fn set_lang(&mut self, lang: Lang) {
        self.paint.lang.set(lang);
    }

    fn tab_index(&self) -> usize {
        self.paint.tab_index.get()
    }

    fn set_tab_index(&mut self, index: usize) {
        self.paint.tab_index.set(index);
    }

    fn splitter_position(&self) -> i32 {
        self.paint.splitter_position.get()
    }

    fn set_splitter_position(&mut self, position: i32) {
        self.paint.splitter_position.set(position);
    }
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
    let system_dpi = GetDpiForSystem();
    UI_DPI.store(system_dpi.max(96), Ordering::Relaxed);
    DARK_THEME.store(system_prefers_dark(), Ordering::Relaxed);
    uxtheme_preferred_app_mode(dark_theme());
    if cfg!(debug_assertions)
        && let Ok(value) = std::env::var("DVDA_UI_DPI")
        && let Ok(value) = value.parse::<u32>()
        && value >= 48
    {
        UI_DPI.store(value, Ordering::Relaxed);
    }
    let icon = unsafe { LoadIconW(instance, ptr::without_provenance::<u16>(1)) };
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
        let status = GetMessageW(&mut message, ptr::null_mut(), 0, 0);
        if status <= 0 {
            break;
        }
        unsafe {
            // The primary action owns Enter, but focused text fields, drop-down
            // lists and buttons keep their own keyboard behaviour.
            if message.message == WM_KEYDOWN && message.wparam == VK_RETURN {
                let focus = GetFocus();
                if focus.is_null() {
                    win::send_int(window, WM_COMMAND, ID_BUILD, 0);
                    continue;
                }
                let class = window_class(focus);
                if class == "Edit" {
                    // Let the edit field consume Enter (and never start a build
                    // while the user is editing a path or title).
                } else if class == "ComboBox" {
                    if win::send_int(focus, CB_GETDROPPEDSTATE, 0, 0) == 0 {
                        win::send_int(window, WM_COMMAND, ID_BUILD, 0);
                        continue;
                    }
                } else if class == "Button" {
                    let id = GetDlgCtrlID(focus);
                    if id > 0 {
                        win::send_int(window, WM_COMMAND, id as usize, 0);
                    }
                    continue;
                } else {
                    win::send_int(window, WM_COMMAND, ID_BUILD, 0);
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

/// The main window's `UiState`, or `None` if `hwnd` is not the main window, is not
/// set up, or is already borrowed.
///
/// Win32 can call back into the window procedure synchronously (for example while
/// a layout moves or shows controls). A nested call that finds the state already
/// borrowed gets `None` and takes the same path as a window without state, so no
/// two mutable references to the `UiState` ever exist at once.
///
/// # Safety
/// The user data of a main window must be a live `RefCell<UiState>` (set up in
/// `WM_CREATE`, freed in `WM_DESTROY`). The returned guard must not outlive that
/// window's `WM_DESTROY`.
unsafe fn state<'a>(hwnd: Hwnd) -> Option<RefMut<'a, UiState>> {
    if window_class(hwnd) != "DVD_AUDIO_MAKER_RUST" {
        return None;
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<UiState>;
    // SAFETY: per this function's contract, the pointer is the live cell of this window.
    unsafe { pointer.as_ref() }?.try_borrow_mut().ok()
}
