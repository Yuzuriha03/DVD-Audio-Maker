#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod presentation;

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
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
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

const WM_CREATE: u32 = 0x0001;
const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_GETMINMAXINFO: u32 = 0x0024;
const WM_CLOSE: u32 = 0x0010;
const WM_DRAWITEM: u32 = 0x002b;
const WM_TIMER: u32 = 0x0113;
const ID_ADVANCED: usize = 117;
const WM_COMMAND: u32 = 0x0111;
const WM_NOTIFY: u32 = 0x004e;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const LOG_QUEUE_CAPACITY: usize = 4096;
const LOG_BATCH_LIMIT: usize = 4096;
const LOG_BATCH_BUDGET: Duration = Duration::from_millis(12);
const WM_SETFONT: u32 = 0x0030;
const EM_SETLIMITTEXT: u32 = 0x00c5;
const EM_GETFIRSTVISIBLELINE: u32 = 0x00ce;
const EM_LINESCROLL: u32 = 0x00b6;
const BM_GETCHECK: u32 = 0x00f0;
const BM_SETCHECK: u32 = 0x00f1;
const CB_ADDSTRING: u32 = 0x0143;
const CB_GETCURSEL: u32 = 0x0147;
const CB_RESETCONTENT: u32 = 0x014b;
const CB_SETCURSEL: u32 = 0x014e;
const PBM_SETPOS: u32 = 0x0402;
const TCM_GETCURSEL: u32 = 0x130b;
const TCM_SETITEMW: u32 = 0x133d;
const TCM_INSERTITEMW: u32 = 0x133e;
const WS_OVERLAPPEDWINDOW: u32 = 0x00cf0000;
const WS_CHILD: u32 = 0x40000000;
const WS_VISIBLE: u32 = 0x10000000;
const WS_BORDER: u32 = 0x00800000;
const WS_VSCROLL: u32 = 0x00200000;
const WS_TABSTOP: u32 = 0x00010000;
const WS_EX_APPWINDOW: u32 = 0x00040000;
const WS_EX_CLIENTEDGE: u32 = 0x00000200;
const ES_AUTOHSCROLL: u32 = 0x0080;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const ES_READONLY: u32 = 0x0800;
const ES_WANTRETURN: u32 = 0x1000;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const BS_PUSHBUTTON: u32 = 0x0000;
const BS_DEFPUSHBUTTON: u32 = 0x0001;
const BS_OWNERDRAW: u32 = 0x0000000b;
const ODS_SELECTED: u32 = 0x0001;
const ODS_DISABLED: u32 = 0x0004;
const DT_CENTER: u32 = 0x00000001;
const DT_VCENTER: u32 = 0x00000004;
const DT_SINGLELINE: u32 = 0x00000020;
const RDW_INVALIDATE: u32 = 0x0001;
const RDW_ERASE: u32 = 0x0004;
const RDW_UPDATENOW: u32 = 0x0100;
const RDW_ALLCHILDREN: u32 = 0x0080;
const SW_SHOW: i32 = 5;
const SW_HIDE: i32 = 0;
const GWLP_USERDATA: i32 = -21;
const BN_CLICKED: u16 = 0;
const CBN_SELCHANGE: u16 = 1;
const TCN_SELCHANGE: i32 = -551;
const ID_PROFILE: usize = 100;
const ID_OPEN_PROFILE: usize = 101;
const ID_SAVE_PROFILE: usize = 102;
const ID_LANGUAGE: usize = 103;
const ID_TAB: usize = 104;
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
const ID_SOURCE: usize = 200;
const ID_FINAL: usize = 201;
const ID_TITLE: usize = 202;
const ID_CAPACITY: usize = 203;
const ID_PLANNED_DISCS: usize = 204;
const ID_WORK_DIR: usize = 205;
const ID_ISO_PREFIX: usize = 206;
const ID_GROUP_LIMIT: usize = 207;
const ID_CACHE: usize = 208;
const ID_RESUME: usize = 209;
const ID_CUSTOM_BYTES: usize = 210;
const ID_TITLE_MODE: usize = 211;
const ID_ALBUM_LIMIT: usize = 212;
const ID_PCM_TEMP: usize = 306;
const ID_MLP_STAGE: usize = 307;
const ID_MODE: usize = 300;
const ID_RATE: usize = 301;
const ID_BITS: usize = 302;
const ID_JOBS: usize = 303;
const ID_METADATA: usize = 304;
const ID_IMPORT_FOLDER: usize = 305;
const ID_MENU: usize = 400;
const ID_STILLS: usize = 401;
const ID_TRACKS: usize = 402;
const ID_COVER: usize = 403;
const ID_INDEX: usize = 404;
const ID_FONT_SC: usize = 405;
const ID_FONT_JP: usize = 406;
const ID_FONT_KR: usize = 407;
const ID_AUTHOR: usize = 500;
const ID_AUTHOR_SRC: usize = 501;
const ID_KEEP_TMP: usize = 502;
const ID_KEEP_INTERMEDIATE: usize = 503;
const ID_LOSS_WARN: usize = 504;
const ID_LOSS_ERROR: usize = 505;
const ID_BROWSE_SOURCE: usize = 1000;
const ID_BROWSE_FINAL: usize = 1001;
const ID_BROWSE_WORK: usize = 1002;
const ID_BROWSE_METADATA: usize = 1003;
const ID_BROWSE_AUTHOR: usize = 1004;
const ID_BROWSE_AUTHOR_SRC: usize = 1005;
const ID_BROWSE_IMPORT: usize = 1006;

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

#[derive(Clone, Copy)]
#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
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
struct ScrollInfo {
    size: u32,
    mask: u32,
    min: i32,
    max: i32,
    page: u32,
    pos: i32,
    track: i32,
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
struct NotifyHeader {
    hwnd_from: Hwnd,
    id_from: usize,
    code: i32,
}

#[repr(C)]
struct TabItem {
    mask: u32,
    state: u32,
    state_mask: u32,
    text: *mut u16,
    text_max: i32,
    image: i32,
    param: isize,
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
    fn ShowWindow(hwnd: Hwnd, command: i32) -> i32;
    fn UpdateWindow(hwnd: Hwnd) -> i32;
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
    fn SetScrollInfo(hwnd: Hwnd, bar: i32, info: *const ScrollInfo, redraw: i32) -> i32;
    fn ScrollWindowEx(
        hwnd: Hwnd,
        dx: i32,
        dy: i32,
        scroll: *const Rect,
        clip: *const Rect,
        region: Handle,
        update: *mut Rect,
        flags: u32,
    ) -> i32;
    fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
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
    fn FillRect(dc: Handle, rect: *const Rect, brush: Hbrush) -> i32;
    fn SendMessageW(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
    fn SetTimer(hwnd: Hwnd, id: usize, millis: u32, callback: *const c_void) -> usize;
    fn KillTimer(hwnd: Hwnd, id: usize) -> i32;
    fn SetFocus(hwnd: Hwnd) -> Hwnd;
    fn SetCapture(hwnd: Hwnd) -> Hwnd;
    fn ReleaseCapture() -> i32;
    fn SetCursor(cursor: Handle) -> Handle;
    fn EnableWindow(hwnd: Hwnd, enable: i32) -> i32;
    fn MessageBoxW(hwnd: Hwnd, text: *const u16, caption: *const u16, flags: u32) -> i32;
    fn LoadCursorW(instance: Hinstance, name: *const u16) -> Handle;
    fn GetSysColorBrush(index: i32) -> Hbrush;
    fn GetSysColor(index: i32) -> u32;
    fn OpenClipboard(owner: Hwnd) -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, data: Handle) -> Handle;
    fn CloseClipboard() -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Hinstance;
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
    fn DeleteObject(object: Handle) -> i32;
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
static UI_FONT: OnceLock<usize> = OnceLock::new();
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
            "profile" => "设置方案",
            "open" => "打开方案",
            "save" => "保存方案",
            "save_as" => "另存方案",
            "advanced" => "显示高级设置",
            "font_filter" => "字体文件",
            "title_mode" => "标题分组（album / one / 每组曲数）",
            "album_limit" => "限制专辑数量（0 表示不限）",
            "pcm_temp" => "PCM 临时目录（留空使用工作目录）",
            "mlp_stage" => "MLP 暂存目录（留空使用工作目录）",
            "language" => "语言",
            "source" => "音源文件夹",
            "final" => "成品保存位置",
            "title" => "光盘名称",
            "capacity" => "光盘容量",
            "planned_discs" => "计划光盘数",
            "work_dir" => "工作文件夹",
            "iso_prefix" => "镜像文件名前缀",
            "group_limit" => "每组最多曲目",
            "cache" => "复用检查结果",
            "resume" => "继续未完成的制作",
            "audio_mode" => "音频编码方式",
            "sample_rate" => "采样率",
            "bits" => "音频位深",
            "jobs" => "同时编码音轨数",
            "metadata" => "旧版 MLP 文件匹配参数",
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
            "author_src" => "菜单素材文件夹",
            "keep_tmp" => "保留临时文件",
            "keep_intermediate" => "保留制作中间文件",
            "loss_warn" => "时长差异提醒 (秒)",
            "loss_error" => "时长差异上限 (秒)",
            "browse" => "浏览...",
            "start" => "开始设置",
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
            "import" => "导入已有 MLP 文件",
            "import_folder" => "MLP 文件夹",
            "custom" => "自定义容量",
            "custom_bytes" => "自定义容量 (字节)",
            "no_output" => "尚未找到成品文件夹，请先完成制作。",
            "language_zh" => "中文",
            "language_en" => "English",
            "language_ja" => "日本語",
            "mlp" => "MLP 编码",
            "lpcm" => "LPCM 编码",
            "dvd5" => "DVD5 (4.7 GB)",
            "dvd9" => "DVD9 (8.5 GB)",
            "import_note" => {
                "MLP 无损压缩、占用较小；LPCM 不压缩。导入已有 MLP 时，选择与音源目录结构对应的文件夹。"
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
                "发布包已内置必要组件，不需要安装 FFmpeg、ImageMagick、eac3to 或 SurCode。"
            }
            _ => "",
        },
        Lang::En => match key {
            "app_subtitle" => "Create DVD-Audio disc images from your music",
            "profile" => "Settings profile",
            "open" => "Open profile",
            "save" => "Save profile",
            "save_as" => "Save as…",
            "advanced" => "Advanced settings",
            "font_filter" => "Font files",
            "title_mode" => "Title grouping (album / one / tracks per title)",
            "album_limit" => "Album limit (0 = unlimited)",
            "pcm_temp" => "PCM temporary folder (blank = work folder)",
            "mlp_stage" => "MLP staging folder (blank = work folder)",
            "language" => "Language",
            "source" => "Source folder",
            "final" => "Output folder",
            "title" => "Disc title",
            "capacity" => "Disc capacity",
            "planned_discs" => "Planned discs to create",
            "work_dir" => "Work folder",
            "iso_prefix" => "ISO filename prefix",
            "group_limit" => "Tracks per group",
            "cache" => "Reuse source checks",
            "resume" => "Resume unfinished work",
            "audio_mode" => "Audio encoding",
            "sample_rate" => "Sample rate",
            "bits" => "Bit depth",
            "jobs" => "Concurrent tracks (auto)",
            "metadata" => "Legacy MLP file matching context",
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
            "author_src" => "Menu resource folder",
            "keep_tmp" => "Keep temporary files",
            "keep_intermediate" => "Keep intermediate files",
            "loss_warn" => "Duration warning (seconds)",
            "loss_error" => "Duration limit (seconds)",
            "browse" => "Browse...",
            "start" => "Getting started",
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
            "import_folder" => "MLP folder",
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
                "The release includes required components; FFmpeg, ImageMagick, eac3to and SurCode are not required."
            }
            _ => "",
        },
        Lang::Ja => match key {
            "app_subtitle" => "音楽から DVD-Audio ディスクイメージを作成",
            "profile" => "設定プロファイル",
            "open" => "プロファイルを開く",
            "save" => "プロファイルを保存",
            "save_as" => "名前を付けて保存",
            "advanced" => "詳細設定を表示",
            "font_filter" => "フォントファイル",
            "title_mode" => "タイトルの区切り（album / one / 曲数）",
            "album_limit" => "アルバム数の上限（0 = 無制限）",
            "pcm_temp" => "PCM 一時フォルダー（空欄 = 作業先）",
            "mlp_stage" => "MLP 一時保存先（空欄 = 作業先）",
            "language" => "言語",
            "source" => "音源フォルダー",
            "final" => "出力フォルダー",
            "title" => "ディスク名",
            "capacity" => "ディスク容量",
            "planned_discs" => "作成予定枚数",
            "work_dir" => "作業フォルダー",
            "iso_prefix" => "ISO ファイル名の接頭辞",
            "group_limit" => "グループの曲数",
            "cache" => "検査結果を再利用",
            "resume" => "未完了の作業を再開",
            "audio_mode" => "音声エンコード",
            "sample_rate" => "サンプルレート",
            "bits" => "ビット深度",
            "jobs" => "同時処理数（自動）",
            "metadata" => "過去ファイルのメタデータ",
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
            "author_src" => "メニュー素材フォルダー",
            "keep_tmp" => "一時ファイルを保持",
            "keep_intermediate" => "中間ファイルを保持",
            "loss_warn" => "時間差の警告 (秒)",
            "loss_error" => "時間差の上限 (秒)",
            "browse" => "参照...",
            "start" => "開始設定",
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
            "import_folder" => "MLP フォルダー",
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
                "必要なコンポーネントは同梱されています。FFmpeg、ImageMagick、eac3to、SurCode は不要です。"
            }
            _ => "",
        },
    }
}

#[derive(Clone, Copy)]
enum BrowseKind {
    Folder,
    File,
    Font,
}

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
    Finished(UiEvent),
}

struct Controls {
    statistics: Hwnd,
    advanced: Hwnd,
    title_mode: Hwnd,
    album_limit: Hwnd,
    pcm_temp: Hwnd,
    mlp_stage: Hwnd,
    profile: Hwnd,
    language: Hwnd,
    tabs: Hwnd,
    source: Hwnd,
    final_dir: Hwnd,
    title: Hwnd,
    capacity: Hwnd,
    custom_bytes: Hwnd,
    planned_discs: Hwnd,
    work_dir: Hwnd,
    iso_prefix: Hwnd,
    group_limit: Hwnd,
    cache: Hwnd,
    resume: Hwnd,
    mode: Hwnd,
    rate: Hwnd,
    bits: Hwnd,
    jobs: Hwnd,
    metadata: Hwnd,
    import_folder: Hwnd,
    menu: Hwnd,
    stills: Hwnd,
    tracks: Hwnd,
    cover: Hwnd,
    index: Hwnd,
    font_sc: Hwnd,
    font_jp: Hwnd,
    font_kr: Hwnd,
    author: Hwnd,
    author_src: Hwnd,
    keep_tmp: Hwnd,
    keep_intermediate: Hwnd,
    loss_warn: Hwnd,
    loss_error: Hwnd,
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
    controls: Controls,
    pages: Vec<Hwnd>,
    labels: Vec<(Hwnd, &'static str)>,
    buttons: Vec<(Hwnd, &'static str)>,
    base: Option<AppOptions>,
    profile_path: PathBuf,
    browse: HashMap<usize, (Hwnd, BrowseKind)>,
    browse_buttons: Vec<Hwnd>,
    tooltip_window: Hwnd,
    tooltips: Vec<Tooltip>,
    logs: Vec<LogLine>,
    log_file: Option<(fs::File, PathBuf)>,
    log_sender: SyncSender<WorkerEvent>,
    log_receiver: Receiver<WorkerEvent>,
    log_dirty: bool,
    log_first_visible_line: i32,
    log_hold_line: Option<i32>,
    log_hold_until: Option<Instant>,
    activity_dirty: bool,
    pending_finish: Option<UiEvent>,
    status_key: &'static str,
    activity_raw: String,
    busy: bool,
    close_when_done: bool,
    cancel: Option<Arc<AtomicBool>>,
    started: Option<Instant>,
    elapsed_seconds: u64,
    problem_count: usize,
    statistics_text: String,
    splitter_height: Option<i32>,
    splitter_position: i32,
    dragging_splitter: bool,
    advanced_controls: Vec<Hwnd>,
}

struct WorkerCallbacks {
    logs: SyncSender<WorkerEvent>,
    cancel: Arc<AtomicBool>,
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
    let class_name = wide("DVD_AUDIO_MAKER_RUST");
    let registration = WndClass {
        style: 0x0002,
        procedure: window_proc,
        extra_class: 0,
        extra_window: 0,
        instance,
        icon: ptr::null_mut(),
        cursor: unsafe { LoadCursorW(ptr::null_mut(), 32512usize as *const u16) },
        background: unsafe { GetSysColorBrush(15) },
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
        background: unsafe { GetSysColorBrush(15) },
        menu: ptr::null(),
        class_name: page_class_name.as_ptr(),
    };
    if unsafe { RegisterClassW(&page_registration) } == 0 {
        return Err("Cannot register desktop page class".into());
    }
    let title = wide("DVD-Audio Maker");
    let window = unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE | 0x02000000,
            100,
            70,
            1180,
            820,
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
    match message {
        WM_CREATE => {
            let state = Box::new(create_controls(hwnd));
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
                SetTimer(hwnd, 1, 100, ptr::null());
            }
            0
        }
        WM_TIMER => {
            if let Some(state) = unsafe { state(hwnd) } {
                drain_worker_logs(hwnd, state);
                update_statistics(state);
            }
            0
        }
        0x0200..=0x0202 => {
            if let Some(state) = unsafe { state(hwnd) } {
                let y = (lparam >> 16) as i16 as i32;
                let over = (state.splitter_position..state.splitter_position + 10).contains(&y);
                if message == 0x0201 && over {
                    state.dragging_splitter = true;
                    unsafe {
                        SetCapture(hwnd);
                    }
                } else if message == 0x0202 && state.dragging_splitter {
                    state.dragging_splitter = false;
                    unsafe {
                        ReleaseCapture();
                    }
                } else if message == 0x0200 && state.dragging_splitter && wparam & 1 != 0 {
                    state.splitter_height = Some(y - 70);
                    layout(hwnd, state);
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
            if let Some(state) = unsafe { state(hwnd) } {
                layout(hwnd, state);
            }
            0
        }
        WM_GETMINMAXINFO => {
            if lparam != 0 {
                // MINMAXINFO consists of five POINTs; minimum tracking size is fourth.
                let dimensions = unsafe { &mut *(lparam as *mut [i32; 10]) };
                dimensions[6] = 1040;
                dimensions[7] = 560;
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
                if item.control_id as usize == ID_CANCEL {
                    unsafe { draw_stop_button(item) };
                    return 1;
                }
            }
            0
        }
        WM_NOTIFY => {
            if lparam != 0 {
                let header = unsafe { &*(lparam as *const NotifyHeader) };
                if header.code == TCN_SELCHANGE
                    && let Some(state) = unsafe { state(hwnd) }
                {
                    show_page(state, tab_index(state.controls.tabs));
                }
            }
            0
        }
        WM_CTLCOLORSTATIC => unsafe {
            SetBkMode(wparam as Handle, 1);
            SetTextColor(wparam as Handle, GetSysColor(18));
            GetSysColorBrush(15) as Lresult
        },
        WM_CLOSE => {
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
            0
        }
        WM_DESTROY => {
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
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

unsafe extern "system" fn page_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    match message {
        WM_SIZE => {
            scroll_page(hwnd, 0, false);
            resize_page_fields(hwnd);
            unsafe {
                RedrawWindow(
                    hwnd,
                    ptr::null(),
                    ptr::null_mut(),
                    RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW | RDW_ALLCHILDREN,
                );
            }
            0
        }
        0x0115 => {
            let old = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as i32;
            let next = match wparam & 0xffff {
                0 => old - 24,
                1 => old + 24,
                2 => old - 120,
                3 => old + 120,
                4 | 5 => (wparam >> 16) as i32,
                6 => 0,
                7 => 310,
                _ => old,
            };
            scroll_page(hwnd, next, true);
            0
        }
        0x020a => {
            let old = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as i32;
            scroll_page(hwnd, old - ((wparam >> 16) as i16 as i32 / 120) * 40, true);
            0
        }
        WM_COMMAND | WM_NOTIFY => {
            let parent = unsafe { GetParent(hwnd) };
            if parent.is_null() {
                0
            } else {
                unsafe { SendMessageW(parent, message, wparam, lparam) }
            }
        }
        WM_CTLCOLORSTATIC => unsafe {
            SetBkMode(wparam as Handle, 1);
            SetTextColor(wparam as Handle, GetSysColor(18));
            GetSysColorBrush(15) as Lresult
        },
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
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
    let pos = if explicit { requested } else { old }.clamp(0, (310 - height).max(0));
    let info = ScrollInfo {
        size: std::mem::size_of::<ScrollInfo>() as u32,
        mask: 7,
        min: 0,
        max: 309,
        page: height as u32,
        pos,
        track: 0,
    };
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, pos as isize);
        if pos != old {
            ScrollWindowEx(
                hwnd,
                0,
                old - pos,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                7,
            );
        }
        SetScrollInfo(hwnd, 1, &info, 1);
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
    let width = rect.right.max(1);
    // Browse rows share the same geometry. Keeping the edit and its button
    // inside the page client area prevents a clipped right border after a
    // window resize or a DPI/layout change.
    let edit_width = (width - 14 - 100).max(120);
    for (edit_id, browse_id, y) in [
        (ID_SOURCE, ID_BROWSE_SOURCE, 38),
        (ID_FINAL, ID_BROWSE_FINAL, 82),
        (ID_WORK_DIR, ID_BROWSE_WORK, 170),
        (ID_METADATA, ID_BROWSE_METADATA, 100),
        (ID_IMPORT_FOLDER, ID_BROWSE_IMPORT, 156),
        (ID_AUTHOR, ID_BROWSE_AUTHOR, 38),
        (ID_AUTHOR_SRC, ID_BROWSE_AUTHOR_SRC, 82),
    ] {
        let edit = unsafe { GetDlgItem(page, edit_id as i32) };
        let browse = unsafe { GetDlgItem(page, browse_id as i32) };
        if !edit.is_null() {
            unsafe { MoveWindow(edit, 14, y, edit_width, 25, 1) };
        }
        if !browse.is_null() {
            unsafe { MoveWindow(browse, 14 + edit_width + 10, y - 2, 90, 29, 1) };
        }
    }
}

unsafe fn state<'a>(hwnd: Hwnd) -> Option<&'a mut UiState> {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut UiState;
    (!pointer.is_null()).then(|| unsafe { &mut *pointer })
}

fn create_controls(parent: Hwnd) -> UiState {
    let profile_path = PROFILE_OVERRIDE
        .get()
        .and_then(Clone::clone)
        .unwrap_or_else(dvda_core::app::default_profile_path);
    let lang = startup_language(&profile_language(&profile_path).unwrap_or_else(|| "auto".into()));
    let mut labels = Vec::new();
    let mut buttons = Vec::new();
    let mut browse = HashMap::new();
    let mut browse_buttons = Vec::new();
    let profile_label = label(
        parent,
        tr(lang, "profile"),
        "profile",
        18,
        14,
        100,
        22,
        &mut labels,
    );
    let profile = create(
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
    let language_label = label(
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
    label(
        parent,
        tr(lang, "app_subtitle"),
        "app_subtitle",
        18,
        42,
        360,
        20,
        &mut labels,
    );
    let origin = label(
        parent,
        &profile_path.to_string_lossy(),
        "",
        390,
        42,
        1030,
        20,
        &mut Vec::new(),
    );
    let tabs = create(
        parent,
        "SysTabControl32",
        "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        18,
        70,
        1140,
        350,
        ID_TAB,
    );
    let tab_keys = ["start", "audio", "menu", "tools"];
    for (index, key) in tab_keys.iter().enumerate() {
        insert_tab(tabs, index, tr(lang, key));
    }
    let mut pages = Vec::new();
    let mut c = Controls {
        statistics: ptr::null_mut(),
        advanced: ptr::null_mut(),
        title_mode: ptr::null_mut(),
        album_limit: ptr::null_mut(),
        pcm_temp: ptr::null_mut(),
        mlp_stage: ptr::null_mut(),
        profile,
        language,
        tabs,
        source: ptr::null_mut(),
        final_dir: ptr::null_mut(),
        title: ptr::null_mut(),
        capacity: ptr::null_mut(),
        custom_bytes: ptr::null_mut(),
        planned_discs: ptr::null_mut(),
        work_dir: ptr::null_mut(),
        iso_prefix: ptr::null_mut(),
        group_limit: ptr::null_mut(),
        cache: ptr::null_mut(),
        resume: ptr::null_mut(),
        mode: ptr::null_mut(),
        rate: ptr::null_mut(),
        bits: ptr::null_mut(),
        jobs: ptr::null_mut(),
        metadata: ptr::null_mut(),
        import_folder: ptr::null_mut(),
        menu: ptr::null_mut(),
        stills: ptr::null_mut(),
        tracks: ptr::null_mut(),
        cover: ptr::null_mut(),
        index: ptr::null_mut(),
        font_sc: ptr::null_mut(),
        font_jp: ptr::null_mut(),
        font_kr: ptr::null_mut(),
        author: ptr::null_mut(),
        author_src: ptr::null_mut(),
        keep_tmp: ptr::null_mut(),
        keep_intermediate: ptr::null_mut(),
        loss_warn: ptr::null_mut(),
        loss_error: ptr::null_mut(),
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
    c.source = browse_edit(
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
    c.final_dir = browse_edit(
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
    c.title = text_field(
        page_start,
        tr(lang, "title"),
        "title",
        ID_TITLE,
        14,
        106,
        470,
        &mut labels,
    );
    c.capacity = combo_field(
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
    c.planned_discs = text_field(
        page_start,
        tr(lang, "planned_discs"),
        "planned_discs",
        ID_PLANNED_DISCS,
        760,
        106,
        170,
        &mut labels,
    );
    c.work_dir = browse_edit(
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
    c.iso_prefix = text_field(
        page_start,
        tr(lang, "iso_prefix"),
        "iso_prefix",
        ID_ISO_PREFIX,
        14,
        194,
        460,
        &mut labels,
    );
    c.group_limit = text_field(
        page_start,
        tr(lang, "group_limit"),
        "group_limit",
        ID_GROUP_LIMIT,
        520,
        194,
        220,
        &mut labels,
    );
    c.cache = check_field(
        page_start,
        tr(lang, "cache"),
        "cache",
        ID_CACHE,
        760,
        204,
        &mut labels,
    );
    c.resume = check_field(
        page_start,
        tr(lang, "resume"),
        "resume",
        ID_RESUME,
        760,
        241,
        &mut labels,
    );
    c.custom_bytes = text_field(
        page_start,
        tr(lang, "custom_bytes"),
        "custom_bytes",
        ID_CUSTOM_BYTES,
        14,
        245,
        460,
        &mut labels,
    );
    pages.push(create_page(parent));
    let page_audio = *pages.last().unwrap();
    c.mode = combo_field(
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
    c.rate = combo_field(
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
    c.bits = combo_field(
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
    c.jobs = text_field(
        page_audio,
        tr(lang, "jobs"),
        "jobs",
        ID_JOBS,
        760,
        18,
        170,
        &mut labels,
    );
    c.metadata = browse_edit(
        page_audio,
        tr(lang, "metadata"),
        "metadata",
        ID_METADATA,
        ID_BROWSE_METADATA,
        14,
        80,
        &mut labels,
        &mut browse,
        &mut browse_buttons,
        lang,
    );
    c.import_folder = browse_edit(
        page_audio,
        tr(lang, "import_folder"),
        "import_folder",
        ID_IMPORT_FOLDER,
        ID_BROWSE_IMPORT,
        14,
        136,
        &mut labels,
        &mut browse,
        &mut browse_buttons,
        lang,
    );
    c.audio_note = label(
        page_audio,
        tr(lang, "mlp_note"),
        "audio_note",
        14,
        194,
        900,
        48,
        &mut labels,
    );
    c.pcm_temp = text_field(
        page_audio,
        tr(lang, "pcm_temp"),
        "pcm_temp",
        ID_PCM_TEMP,
        14,
        245,
        460,
        &mut labels,
    );
    c.mlp_stage = text_field(
        page_audio,
        tr(lang, "mlp_stage"),
        "mlp_stage",
        ID_MLP_STAGE,
        506,
        245,
        460,
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
    c.tracks = text_field(
        page_menu,
        tr(lang, "tracks"),
        "tracks",
        ID_TRACKS,
        14,
        66,
        220,
        &mut labels,
    );
    c.cover = text_field(
        page_menu,
        tr(lang, "cover"),
        "cover",
        ID_COVER,
        260,
        66,
        220,
        &mut labels,
    );
    c.index = text_field(
        page_menu,
        tr(lang, "index"),
        "index",
        ID_INDEX,
        506,
        66,
        220,
        &mut labels,
    );
    c.font_sc = text_field(
        page_menu,
        tr(lang, "font_sc"),
        "font_sc",
        ID_FONT_SC,
        14,
        120,
        300,
        &mut labels,
    );
    c.font_jp = text_field(
        page_menu,
        tr(lang, "font_jp"),
        "font_jp",
        ID_FONT_JP,
        340,
        120,
        300,
        &mut labels,
    );
    c.font_kr = text_field(
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
    pages.push(create_page(parent));
    let page_tools = *pages.last().unwrap();
    c.author = browse_edit(
        page_tools,
        tr(lang, "author"),
        "author",
        ID_AUTHOR,
        ID_BROWSE_AUTHOR,
        14,
        18,
        &mut labels,
        &mut browse,
        &mut browse_buttons,
        lang,
    );
    c.author_src = browse_edit(
        page_tools,
        tr(lang, "author_src"),
        "author_src",
        ID_AUTHOR_SRC,
        ID_BROWSE_AUTHOR_SRC,
        14,
        62,
        &mut labels,
        &mut browse,
        &mut browse_buttons,
        lang,
    );
    c.keep_tmp = check_field(
        page_tools,
        tr(lang, "keep_tmp"),
        "keep_tmp",
        ID_KEEP_TMP,
        14,
        116,
        &mut labels,
    );
    c.keep_intermediate = check_field(
        page_tools,
        tr(lang, "keep_intermediate"),
        "keep_intermediate",
        ID_KEEP_INTERMEDIATE,
        300,
        116,
        &mut labels,
    );
    c.loss_warn = text_field(
        page_tools,
        tr(lang, "loss_warn"),
        "loss_warn",
        ID_LOSS_WARN,
        14,
        164,
        220,
        &mut labels,
    );
    c.loss_error = text_field(
        page_tools,
        tr(lang, "loss_error"),
        "loss_error",
        ID_LOSS_ERROR,
        260,
        164,
        220,
        &mut labels,
    );
    c.title_mode = text_field(
        page_tools,
        tr(lang, "title_mode"),
        "title_mode",
        ID_TITLE_MODE,
        14,
        228,
        460,
        &mut labels,
    );
    c.album_limit = text_field(
        page_tools,
        tr(lang, "album_limit"),
        "album_limit",
        ID_ALBUM_LIMIT,
        506,
        228,
        460,
        &mut labels,
    );
    let _operation_section = label(
        parent,
        tr(lang, "operation_section"),
        "operation_section",
        18,
        426,
        220,
        18,
        &mut labels,
    );
    let status = label(
        parent,
        tr(lang, "ready"),
        "ready",
        18,
        448,
        220,
        24,
        &mut labels,
    );
    let activity = label(
        parent,
        tr(lang, "ready_hint"),
        "ready_hint",
        250,
        448,
        650,
        24,
        &mut labels,
    );
    let progress = create(
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
    let _log_section = label(
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
    );
    let live = checkbox(parent, tr(lang, "live"), ID_LIVE, 330, 508, 150, 24);
    check(live, true);
    let copy = button(parent, tr(lang, "copy"), ID_COPY, 720, 478, 125, 28);
    let export = button(parent, tr(lang, "export"), ID_EXPORT, 855, 478, 130, 28);
    let log = create(
        parent,
        "EDIT",
        "",
        WS_CHILD
            | WS_VISIBLE
            | WS_VSCROLL
            | ES_MULTILINE
            | ES_AUTOVSCROLL
            | ES_READONLY
            | ES_WANTRETURN,
        18,
        540,
        1140,
        215,
        ID_LOG,
    );
    unsafe {
        SendMessageW(log, EM_SETLIMITTEXT, 1_000_000, 0);
    }
    // Keep the legacy command IDs for automation/tests, but expose the
    // complete prepare-build-verify workflow through the single build action.
    let prepare = create(parent, "BUTTON", "", WS_CHILD, 18, 740, 145, 34, ID_PREPARE);
    let build = primary_button(parent, tr(lang, "build"), ID_BUILD, 175, 740, 145, 34);
    let verify = create(parent, "BUTTON", "", WS_CHILD, 332, 740, 145, 34, ID_VERIFY);
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
    c.advanced = checkbox(parent, tr(lang, "advanced"), ID_ADVANCED, 440, 42, 215, 22);
    labels.push((c.advanced, "advanced"));
    // Preserve the old distinction between everyday and advanced settings.
    let mut advanced_controls = vec![
        c.work_dir,
        c.iso_prefix,
        c.group_limit,
        c.cache,
        c.resume,
        c.jobs,
        c.metadata,
        c.pcm_temp,
        c.mlp_stage,
        c.index,
        c.font_sc,
        c.font_jp,
        c.font_kr,
        c.keep_tmp,
        c.keep_intermediate,
        c.loss_warn,
        c.loss_error,
        c.title_mode,
        c.album_limit,
    ];
    let advanced_keys = [
        "work_dir",
        "iso_prefix",
        "group_limit",
        "cache",
        "resume",
        "jobs",
        "metadata",
        "pcm_temp",
        "mlp_stage",
        "index",
        "font_sc",
        "font_jp",
        "font_kr",
        "keep_tmp",
        "keep_intermediate",
        "loss_warn",
        "loss_error",
        "title_mode",
        "album_limit",
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
    c.statistics = label(parent, "", "", 810, 700, 330, 26, &mut Vec::new());
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
    let mut state = UiState {
        lang,
        controls: c,
        pages,
        labels,
        buttons,
        base,
        profile_path,
        browse,
        browse_buttons,
        tooltip_window: ptr::null_mut(),
        tooltips: Vec::new(),
        logs: Vec::new(),
        log_file: session_log.ok(),
        log_sender,
        log_receiver,
        log_dirty: false,
        log_first_visible_line: 0,
        log_hold_line: None,
        log_hold_until: None,
        activity_dirty: false,
        pending_finish: None,
        status_key: "ready",
        activity_raw: tr(Lang::Zh, "ready_hint").into(),
        busy: false,
        close_when_done: false,
        cancel: None,
        started: None,
        elapsed_seconds: 0,
        problem_count: 0,
        statistics_text: String::new(),
        splitter_height: None,
        splitter_position: 420,
        dragging_splitter: false,
        advanced_controls,
    };
    unsafe {
        EnableWindow(state.controls.cancel, 0);
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
    create(
        parent,
        "DVD_AUDIO_PAGE_RUST",
        "",
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_VSCROLL | 0x02000000,
        22,
        104,
        1130,
        310,
        0,
    )
}

#[allow(clippy::too_many_arguments)] // Explicit Win32 control geometry and ownership.
fn label(
    parent: Hwnd,
    text: &str,
    key: &'static str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    labels: &mut Vec<(Hwnd, &'static str)>,
) -> Hwnd {
    let hwnd = create(parent, "STATIC", text, WS_CHILD | WS_VISIBLE, x, y, w, h, 0);
    if !key.is_empty() {
        labels.push((hwnd, key));
    }
    hwnd
}

#[allow(clippy::too_many_arguments)] // Explicit Win32 control geometry and ownership.
fn text_field(
    parent: Hwnd,
    caption: &str,
    key: &'static str,
    id: usize,
    x: i32,
    y: i32,
    width: i32,
    labels: &mut Vec<(Hwnd, &'static str)>,
) -> Hwnd {
    label(parent, caption, key, x, y, width, 20, labels);
    create(
        parent,
        "EDIT",
        "",
        WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL | WS_TABSTOP,
        x,
        y + 20,
        width,
        25,
        id,
    )
}

#[allow(clippy::too_many_arguments)] // Explicit Win32 control geometry and ownership.
fn browse_edit(
    parent: Hwnd,
    caption: &str,
    key: &'static str,
    id: usize,
    browse_id: usize,
    x: i32,
    y: i32,
    labels: &mut Vec<(Hwnd, &'static str)>,
    browse: &mut HashMap<usize, (Hwnd, BrowseKind)>,
    browse_buttons: &mut Vec<Hwnd>,
    lang: Lang,
) -> Hwnd {
    label(parent, caption, key, x, y, 520, 20, labels);
    let edit = create(
        parent,
        "EDIT",
        "",
        WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL | WS_TABSTOP,
        x,
        y + 20,
        820,
        25,
        id,
    );
    let browse_button = button(
        parent,
        tr(lang, "browse"),
        browse_id,
        x + 830,
        y + 18,
        90,
        29,
    );
    browse_buttons.push(browse_button);
    browse.insert(
        browse_id,
        (
            edit,
            if matches!(browse_id, ID_BROWSE_AUTHOR | ID_BROWSE_METADATA) {
                BrowseKind::File
            } else {
                BrowseKind::Folder
            },
        ),
    );
    edit
}

#[allow(clippy::too_many_arguments)] // Explicit Win32 control geometry and ownership.
fn combo_field(
    parent: Hwnd,
    caption: &str,
    key: &'static str,
    id: usize,
    x: i32,
    y: i32,
    width: i32,
    labels: &mut Vec<(Hwnd, &'static str)>,
) -> Hwnd {
    label(parent, caption, key, x, y, width, 20, labels);
    combo(parent, id, x, y + 20, width, 160)
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
    let hwnd = checkbox(parent, caption, id, x, y, 230, 28);
    labels.push((hwnd, key));
    hwnd
}

fn checkbox(parent: Hwnd, text: &str, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {
    create(
        parent,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX,
        x,
        y,
        w,
        h,
        id,
    )
}
fn combo(parent: Hwnd, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {
    create(
        parent,
        "COMBOBOX",
        "",
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | CBS_DROPDOWNLIST,
        x,
        y,
        w,
        h,
        id,
    )
}

fn button(parent: Hwnd, text: &str, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {
    create(
        parent,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON,
        x,
        y,
        w,
        h,
        id,
    )
}

fn primary_button(parent: Hwnd, text: &str, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {
    create(
        parent,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON,
        x,
        y,
        w,
        h,
        id,
    )
}

fn danger_button(parent: Hwnd, text: &str, id: usize, x: i32, y: i32, w: i32, h: i32) -> Hwnd {
    create(
        parent,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        x,
        y,
        w,
        h,
        id,
    )
}

#[allow(clippy::too_many_arguments)] // Mirrors CreateWindowExW, omitting fixed parameters.
fn create(
    parent: Hwnd,
    class: &str,
    title: &str,
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    id: usize,
) -> Hwnd {
    let class = wide(class);
    let title = wide(title);
    let hwnd = unsafe {
        CreateWindowExW(
            if class.as_slice() == wide("EDIT").as_slice() {
                WS_EX_CLIENTEDGE
            } else if class.as_slice() == wide("DVD_AUDIO_PAGE_RUST").as_slice() {
                0x00010000 // WS_EX_CONTROLPARENT: keyboard navigation into pages.
            } else {
                0
            },
            class.as_ptr(),
            title.as_ptr(),
            style,
            x,
            y,
            width,
            height,
            parent,
            id as Hmenu,
            GetModuleHandleW(ptr::null()),
            ptr::null_mut(),
        )
    };
    if !hwnd.is_null() {
        unsafe {
            SendMessageW(hwnd, WM_SETFONT, ui_font() as usize, 1);
        }
    }
    hwnd
}

fn ui_font() -> Handle {
    *UI_FONT.get_or_init(|| unsafe {
        let font = CreateFontW(
            -16,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            1,
            0,
            0,
            5,
            0,
            wide("Segoe UI").as_ptr(),
        );
        if font.is_null() {
            GetStockObject(17) as usize
        } else {
            font as usize
        }
    }) as Handle
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
fn tab_index(hwnd: Hwnd) -> i32 {
    unsafe { SendMessageW(hwnd, TCM_GETCURSEL, 0, 0) as i32 }
}
fn show_page(state: &UiState, index: i32) {
    for (i, page) in state.pages.iter().enumerate() {
        unsafe {
            ShowWindow(
                *page,
                if i == index.max(0) as usize {
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
}
fn insert_tab(hwnd: Hwnd, index: usize, text: &str) {
    let mut text = wide(text);
    let item = TabItem {
        mask: 1,
        state: 0,
        state_mask: 0,
        text: text.as_mut_ptr(),
        text_max: text.len() as i32,
        image: 0,
        param: 0,
    };
    unsafe {
        SendMessageW(
            hwnd,
            TCM_INSERTITEMW,
            index,
            &item as *const TabItem as isize,
        );
    }
}
fn set_tab(hwnd: Hwnd, index: usize, text: &str) {
    let mut text = wide(text);
    let item = TabItem {
        mask: 1,
        state: 0,
        state_mask: 0,
        text: text.as_mut_ptr(),
        text_max: text.len() as i32,
        image: 0,
        param: 0,
    };
    unsafe {
        SendMessageW(hwnd, TCM_SETITEMW, index, &item as *const TabItem as isize);
    }
}

fn layout(hwnd: Hwnd, state: &mut UiState) {
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe {
        GetClientRect(hwnd, &mut rect);
    }
    let width = (rect.right - rect.left).max(1000);
    let height = (rect.bottom - rect.top).max(510);
    let tab_height = state
        .splitter_height
        .unwrap_or((height - 380).clamp(160, 350))
        .clamp(160, (height - 330).max(160));
    state.splitter_position = 70 + tab_height;
    let delta = tab_height - 350;
    let margin = 18;
    let bottom = height - 14;
    unsafe {
        MoveWindow(state.controls.profile, 162, 11, width - 770, 26, 1);
        MoveWindow(state.controls.language, width - 125, 10, 105, 120, 1);
        for (handle, key) in &state.buttons {
            match *key {
                "open" => {
                    MoveWindow(*handle, width - 595, 10, 150, 28, 1);
                }
                "save" => {
                    MoveWindow(*handle, width - 440, 10, 150, 28, 1);
                }
                "save_as" => {
                    MoveWindow(*handle, width - 285, 10, 150, 28, 1);
                }
                "copy" => {
                    MoveWindow(*handle, width - 350, 508 + delta, 150, 28, 1);
                }
                "export" => {
                    MoveWindow(*handle, width - 190, 508 + delta, 170, 28, 1);
                }
                _ => {}
            }
        }
        for (handle, key) in &state.labels {
            match *key {
                "profile" => {
                    MoveWindow(*handle, 18, 14, 140, 22, 1);
                }
                "language" => {
                    ShowWindow(*handle, SW_HIDE);
                }
                "app_subtitle" => {
                    MoveWindow(*handle, 18, 42, 410, 20, 1);
                }
                "operation_section" => {
                    MoveWindow(*handle, margin, 426 + delta, 220, 18, 1);
                }
                "log_section" => {
                    MoveWindow(*handle, margin, 488 + delta, 220, 18, 1);
                }
                _ => {}
            }
        }
        MoveWindow(state.controls.advanced, 438, 42, 210, 22, 1);
        MoveWindow(state.controls.profile_origin, 660, 42, width - 680, 20, 1);
        MoveWindow(
            state.controls.statistics,
            820,
            bottom - 29,
            width - 840,
            27,
            1,
        );
        MoveWindow(
            state.controls.tabs,
            margin,
            70,
            width - margin * 2,
            tab_height,
            1,
        );
        for page in &state.pages {
            MoveWindow(
                *page,
                margin + 4,
                101,
                width - margin * 2 - 8,
                tab_height - 35,
                1,
            );
        }
        MoveWindow(state.controls.status, margin, 448 + delta, 300, 24, 1);
        MoveWindow(
            state.controls.activity,
            330,
            448 + delta,
            width - 350,
            24,
            1,
        );
        MoveWindow(
            state.controls.progress,
            margin,
            476 + delta,
            width - margin * 2,
            8,
            1,
        );
        MoveWindow(state.controls.log_view, margin, 508 + delta, 140, 120, 1);
        MoveWindow(state.controls.only_issues, 170, 508 + delta, 150, 24, 1);
        MoveWindow(state.controls.live, 330, 508 + delta, 150, 24, 1);
        MoveWindow(
            state.controls.log,
            margin,
            540 + delta,
            width - margin * 2,
            (height - 618 - delta).max(80),
            1,
        );
        MoveWindow(state.controls.prepare, margin, bottom - 34, 1, 1, 0);
        MoveWindow(state.controls.build, margin, bottom - 34, 145, 34, 1);
        MoveWindow(state.controls.verify, margin + 157, bottom - 34, 1, 1, 0);
        MoveWindow(state.controls.cancel, margin + 157, bottom - 34, 145, 34, 1);
        MoveWindow(
            state.controls.open_output,
            margin + 314,
            bottom - 34,
            145,
            34,
            1,
        );
        RedrawWindow(
            hwnd,
            ptr::null(),
            ptr::null_mut(),
            RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW | RDW_ALLCHILDREN,
        );
    }
}

unsafe fn draw_stop_button(item: &DrawItemStruct) {
    // Owner-drawing keeps the stop action visually distinct even when the
    // Windows theme ignores WM_CTLCOLORBTN for regular buttons.
    let color = if item.state & ODS_DISABLED != 0 {
        0x00808080
    } else if item.state & ODS_SELECTED != 0 {
        0x001818a8
    } else {
        0x002828c6
    };
    let brush = unsafe { CreateSolidBrush(color) };
    if brush.is_null() {
        return;
    }
    unsafe {
        FillRect(item.dc, &item.rect, brush);
        DeleteObject(brush as Handle);
        SetBkMode(item.dc, 1);
        SetTextColor(
            item.dc,
            if item.state & ODS_DISABLED != 0 {
                0x00d0d0d0
            } else {
                0x00ffffff
            },
        );
        let length = GetWindowTextLengthW(item.item).max(0) as usize;
        let mut text = vec![0u16; length + 1];
        let count = GetWindowTextW(item.item, text.as_mut_ptr(), text.len() as i32);
        let mut rect = item.rect;
        DrawTextW(
            item.dc,
            text.as_ptr(),
            count,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
}

fn handle_command(hwnd: Hwnd, state: &mut UiState, id: usize, notify: u16) {
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
    for (index, key) in ["start", "audio", "menu", "tools"].iter().enumerate() {
        set_tab(state.controls.tabs, index, tr(lang, key));
    }
    set_text(state.controls.status, tr(lang, state.status_key));
    set_text(
        state.controls.activity,
        &localized_log(lang, &state.activity_raw),
    );
    update_choices(state);
    render_log(state);
    update_statistics(state);
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
        _ => "surcode-batch",
    }
}
fn update_choices(state: &UiState) {
    let mode = combo_index(state.controls.mode);
    let imported = mode == 2;
    set_text(
        state.controls.audio_note,
        tr(state.lang, audio_note_key(mode)),
    );
    unsafe {
        EnableWindow(
            state.controls.custom_bytes,
            i32::from(combo_index(state.controls.capacity) == 2),
        );
        EnableWindow(state.controls.import_folder, i32::from(imported));
        for hwnd in [
            state.controls.rate,
            state.controls.bits,
            state.controls.jobs,
            state.controls.metadata,
            state.controls.pcm_temp,
            state.controls.mlp_stage,
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
    let planned_discs = options.integer("PlannedDiscs");
    let planned_discs_text = if planned_discs <= 0 {
        "auto".to_owned()
    } else {
        planned_discs.to_string()
    };
    set_text(state.controls.planned_discs, &planned_discs_text);
    set_text(state.controls.work_dir, &options.text("BuildDirectory"));
    set_text(
        state.controls.iso_prefix,
        &raw_or_environment(&raw, "DVDA_ISO_PREFIX"),
    );
    set_text(
        state.controls.group_limit,
        &options.integer("GroupTrackLimit").to_string(),
    );
    check(state.controls.cache, options.boolean("PrepareCacheEnabled"));
    check(state.controls.resume, options.boolean("ResumeEnabled"));
    combo_select(state.controls.mode, mode_index(&options.text("MlpSource")));
    set_text(
        state.controls.import_folder,
        &raw_or_environment(&raw, "DVDA_MLP_EXTERNAL_DIR"),
    );
    populate_numeric_choice(
        state.controls.rate,
        SAMPLE_RATES,
        SAMPLE_RATE_LABELS,
        options.integer("MlpSurcodeSampleRate"),
    );
    populate_numeric_choice(
        state.controls.bits,
        SAMPLE_BITS,
        &["16", "20", "24"],
        options.integer("MlpSurcodeBits"),
    );
    let jobs = options.integer("MlpJobs");
    let jobs_text = if jobs <= 0 {
        "auto".to_owned()
    } else {
        jobs.to_string()
    };
    set_text(state.controls.jobs, &jobs_text);
    set_text(state.controls.metadata, &options.text("MlpMetadataContext"));
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
    set_text(
        state.controls.index,
        &options.integer("MenuIndexMinimumAlbums").to_string(),
    );
    set_text(state.controls.font_sc, &options.text("MenuFont"));
    set_text(state.controls.font_jp, &options.text("MenuFontJapanese"));
    set_text(state.controls.font_kr, &options.text("MenuFontKorean"));
    set_text(state.controls.author, &options.text("DvdaAuthor"));
    set_text(
        state.controls.title_mode,
        &options.text("DiagnosticTitleMode"),
    );
    set_text(
        state.controls.album_limit,
        &options
            .value("DiagnosticAlbumLimit")
            .as_i64()
            .unwrap_or(0)
            .to_string(),
    );
    set_text(
        state.controls.pcm_temp,
        &options.text("MlpBatchTempDirectory"),
    );
    set_text(
        state.controls.mlp_stage,
        &options.text("MlpBatchOutputDirectory"),
    );
    set_text(state.controls.author_src, &options.text("AuthorSource"));
    check(state.controls.keep_tmp, options.boolean("KeepTemporary"));
    check(
        state.controls.keep_intermediate,
        options.boolean("KeepIntermediate"),
    );
    set_text(
        state.controls.loss_warn,
        &f64::from_bits(options.integer("LossWarningSeconds") as u64).to_string(),
    );
    set_text(
        state.controls.loss_error,
        &f64::from_bits(options.integer("LossErrorSeconds") as u64).to_string(),
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
    let planned_discs = get_text(state.controls.planned_discs);
    let planned_discs = if planned_discs.trim().is_empty() || planned_discs.trim() == "0" {
        "auto".to_owned()
    } else {
        planned_discs
    };
    put(&mut values, "PlannedDiscs", json!(planned_discs));
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
        json!(checked(state.controls.cache)),
    );
    put(
        &mut values,
        "ResumeEnabled",
        json!(checked(state.controls.resume)),
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
        "MlpSurcodeSampleRate",
        json!(numeric_choice(state.controls.rate, SAMPLE_RATES)),
    );
    put(
        &mut values,
        "MlpSurcodeBits",
        json!(numeric_choice(state.controls.bits, SAMPLE_BITS)),
    );
    let jobs = get_text(state.controls.jobs);
    let jobs = if jobs.trim().is_empty() || jobs.trim() == "0" {
        "auto".to_owned()
    } else {
        jobs
    };
    put(&mut values, "MlpJobs", json!(jobs));
    put(
        &mut values,
        "MlpMetadataContext",
        json!(get_text(state.controls.metadata)),
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
        "MenuIndexMinimumAlbums",
        json!(parse_i64(state.controls.index, 4)),
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
    put(
        &mut values,
        "DvdaAuthor",
        json!(get_text(state.controls.author)),
    );
    put(
        &mut values,
        "AuthorSource",
        json!(get_text(state.controls.author_src)),
    );
    put(
        &mut values,
        "KeepTemporary",
        json!(checked(state.controls.keep_tmp)),
    );
    put(
        &mut values,
        "KeepIntermediate",
        json!(checked(state.controls.keep_intermediate)),
    );
    put(
        &mut values,
        "LossWarningSeconds",
        json!(f64::to_bits(parse_f64(state.controls.loss_warn, 0.005)) as i64),
    );
    put(
        &mut values,
        "LossErrorSeconds",
        json!(f64::to_bits(parse_f64(state.controls.loss_error, 0.05)) as i64),
    );
    values
}

fn update_advanced(state: &UiState) {
    let show = checked(state.controls.advanced);
    for control in &state.advanced_controls {
        unsafe {
            ShowWindow(*control, if show { SW_SHOW } else { SW_HIDE });
        }
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
fn valid_jobs(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("auto") || valid_integer(value, 1, 16)
}
fn valid_planned_discs(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("auto") || valid_integer(value, 0, 999)
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
        (c.index, "index", 0, 1000),
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
    if error.is_none() && !valid_planned_discs(&get_text(c.planned_discs)) {
        error = Some((
            c.planned_discs,
            format!("{}：auto 或 1–999", tr(state.lang, "planned_discs")),
        ));
    }
    if error.is_none() && !valid_jobs(&get_text(c.jobs)) {
        error = Some((c.jobs, format!("{}：auto 或 1–16", tr(state.lang, "jobs"))));
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
    for (control, key) in [(c.loss_warn, "loss_warn"), (c.loss_error, "loss_error")] {
        if error.is_none()
            && !get_text(control)
                .trim()
                .parse::<f64>()
                .is_ok_and(|n| n.is_finite() && n >= 0.0)
        {
            error = Some((
                control,
                format!(
                    "{}：{}",
                    tr(state.lang, key),
                    tr(state.lang, "invalid_nonnegative")
                ),
            ));
        }
    }
    let title = get_text(c.title_mode);
    if error.is_none()
        && !matches!(title.trim().to_ascii_lowercase().as_str(), "album" | "one")
        && !valid_integer(&title, 1, i32::MAX as i64)
    {
        error = Some((c.title_mode, tr(state.lang, "invalid_title_mode").into()));
    }
    if error.is_none()
        && !get_text(c.album_limit).trim().is_empty()
        && !valid_integer(&get_text(c.album_limit), 0, i32::MAX as i64)
    {
        error = Some((
            c.album_limit,
            format!(
                "{}：{}",
                tr(state.lang, "album_limit"),
                tr(state.lang, "invalid_album_limit")
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
            unsafe {
                SendMessageW(state.controls.tabs, 0x130c, index, 0);
            }
            show_page(state, index as i32);
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
fn parse_f64(hwnd: Hwnd, fallback: f64) -> f64 {
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
    // Invalid startup profiles must not overwrite the user's last good settings.
    if state.base.is_none() || !validate_controls(state, false) {
        return;
    }
    let path = dvda_core::app::default_profile_path();
    if let Err(error) =
        dvda_core::app::save_profile(&path, &edited_profile_values(state), state.lang.code())
    {
        add_log(
            state,
            &format!("{}{}", tr(state.lang, "profile_error"), error),
            true,
        );
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
        .unwrap()
        .join("logs");
    fs::create_dir_all(&root)?;
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
        .create_new(true)
        .open(&path)?;
    Ok((file, path))
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
    state.log_dirty = false;
    state.log_first_visible_line = 0;
    state.log_hold_line = None;
    state.log_hold_until = None;
    state.activity_dirty = false;
    state.problem_count = 0;
    state.log_file = None;
    match open_session_log() {
        Ok(file) => state.log_file = Some(file),
        Err(error) => add_log(
            state,
            &format!(
                "[警告] {} {error}",
                presentation::phrase(
                    state.lang,
                    "无法保存完整日志，请检查磁盘空间或目录权限。",
                    "Cannot save the full log; check disk space and folder permissions.",
                    "完全なログを保存できません。空き容量とフォルダー権限を確認してください。"
                )
            ),
            true,
        ),
    }
    set_text(state.controls.log, "");
}

fn edited_profile_values(state: &UiState) -> Map<String, Value> {
    let mut values = state
        .base
        .as_ref()
        .map(AppOptions::profile_values)
        .unwrap_or_default();
    for (key, handle) in [
        ("DVDA_TITLE_MODE", state.controls.title_mode),
        ("DVDA_ALBUM_LIMIT", state.controls.album_limit),
        ("DVDA_MLP_BATCH_TEMP_DIR", state.controls.pcm_temp),
        ("DVDA_MLP_BATCH_OUTPUT_DIR", state.controls.mlp_stage),
    ] {
        values.insert(key.into(), json!(get_text(handle)));
    }
    for (key, value) in overrides(state) {
        let raw = match key.as_str() {
            "SourceDirectory" => "DVDA_SRC",
            "FinalDirectory" => "DVDA_FINAL_DIR",
            "Title" => "DVDA_TITLE",
            "DiscBytes" => "DVDA_DISC_BYTES",
            "PlannedDiscs" => "DVDA_PLANNED_DISCS",
            "BuildDirectory" => "DVDA_BUILD_DIR",
            "IsoPrefix" => "DVDA_ISO_PREFIX",
            "GroupTrackLimit" => "DVDA_GROUP_TRACK_LIMIT",
            "MlpSource" => "DVDA_MLP_SOURCE",
            "MlpExternalDirectory" => "DVDA_MLP_EXTERNAL_DIR",
            "MlpSurcodeSampleRate" => "DVDA_MLP_SURCODE_SAMPLE_RATE",
            "MlpSurcodeBits" => "DVDA_MLP_SURCODE_BITS",
            "MlpJobs" => "DVDA_MLP_JOBS",
            "MlpMetadataContext" => "DVDA_MLP_METADATA_CONTEXT",
            "MenuEnabled" => "DVDA_MENU",
            "MenuStillPictures" => "DVDA_MENU_STILLPICS",
            "MenuTracksPerPage" => "DVDA_MENU_TRACKS_PER_PAGE",
            "MenuCoverDim" => "DVDA_MENU_COVER_DIM",
            "MenuIndexMinimumAlbums" => "DVDA_MENU_INDEX_MIN_ALBUMS",
            "MenuFont" => "DVDA_MENU_FONT",
            "MenuFontJapanese" => "DVDA_MENU_FONT_JP",
            "MenuFontKorean" => "DVDA_MENU_FONT_KR",
            "DvdaAuthor" => "DVDA_AUTHOR",
            "AuthorSource" => "DVDA_AUTHOR_SRC",
            "PrepareCacheEnabled" => "DVDA_PREPARE_CACHE",
            "ResumeEnabled" => "DVDA_RESUME",
            "KeepTemporary" => "DVDA_KEEP_TMP",
            "KeepIntermediate" => "DVDA_KEEP_INTERMEDIATE",
            "LossWarningSeconds" => "DVDA_LOSS_WARN_S",
            "LossErrorSeconds" => "DVDA_LOSS_ERROR_S",
            _ => continue,
        };
        let stored = match key.as_str() {
            "LossWarningSeconds" | "LossErrorSeconds" => value
                .as_i64()
                .map(|bits| Value::String(f64::from_bits(bits as u64).to_string()))
                .unwrap_or_else(|| Value::String(value_to_profile_string(value))),
            _ => Value::String(value_to_profile_string(value)),
        };
        values.insert(raw.into(), stored);
    }
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
    let cancel = Arc::new(AtomicBool::new(false));
    autosave(state);
    state.cancel = Some(Arc::clone(&cancel));
    state.busy = true;
    state.started = Some(Instant::now());
    state.elapsed_seconds = 0;
    reset_task_log(state);
    set_busy_controls(state, true);
    update_statistics(state);
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
    unsafe {
        let progress = state.controls.progress;
        SetWindowLongPtrW(progress, -16, GetWindowLongPtrW(progress, -16) | 8);
        SendMessageW(progress, 0x040a, 1, 40);
    }
    let lang = state.lang;
    let logs = state.log_sender.clone();
    thread::spawn(move || {
        let mut callbacks = WorkerCallbacks {
            logs,
            cancel: Arc::clone(&cancel),
        };
        let (ok, text) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match command {
            ID_PREPARE => {
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
                    match dvda_core::app::run_build(&options, false, &mut callbacks) {
                        Ok(result) => {
                            emit_diagnostics(&mut callbacks, &result.diagnostics);
                            if !result.succeeded {
                                (false, tr(lang, "failed").into())
                            } else {
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
            _ => match dvda_core::app::run_verify(&options, &mut callbacks) {
                Ok(result) => {
                    emit_diagnostics(&mut callbacks, &result.diagnostics);
                    (
                        result.succeeded,
                        tr(lang, if result.succeeded { "done" } else { "failed" }).into(),
                    )
                }
                Err(error) => (false, error),
            },
        }))
        .unwrap_or_else(|_| (false, "任务异常终止，请查看详细日志。".into()));
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

fn finish_operation(state: &mut UiState, kind: u32, text: &str) {
    unsafe {
        let progress = state.controls.progress;
        SendMessageW(progress, 0x040a, 0, 0);
        SetWindowLongPtrW(progress, -16, GetWindowLongPtrW(progress, -16) & !8);
    }
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
    unsafe {
        SendMessageW(
            state.controls.progress,
            PBM_SETPOS,
            if kind == 1 { 100 } else { 0 },
            0,
        );
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

fn set_busy_controls(state: &UiState, busy: bool) {
    let enabled = i32::from(!busy);
    for hwnd in state.pages.iter().copied().chain([
        state.controls.tabs,
        state.controls.profile,
        state.controls.language,
    ]) {
        unsafe {
            EnableWindow(hwnd, enabled);
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
            state.logs.push(LogLine {
                raw: format!("[警告] {message} {error}"),
                problem: true,
                timestamp: timestamp.clone(),
            });
            state.problem_count += 1;
        }
        let problem = problem || log_is_problem(text);
        state.problem_count += usize::from(problem);
        state.logs.push(LogLine {
            raw: text.to_owned(),
            problem,
            timestamp: timestamp.clone(),
        });
    }
    if state.logs.len() > 2500 {
        state.logs.drain(..state.logs.len() - 2000);
    }
    state.activity_raw = text.to_owned();
    state.log_dirty = true;
    state.activity_dirty = true;
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
                WorkerEvent::Finished(event) => state.pending_finish = Some(event),
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
    } else if state.log_dirty && checked(state.controls.live) {
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
    unsafe {
        EnableWindow(state.controls.only_issues, i32::from(!detail));
    }
    set_text(
        state.controls.log,
        &display_log(
            state.lang,
            &state.logs,
            detail,
            checked(state.controls.only_issues),
        ),
    );
    let hold_line = state
        .log_hold_until
        .filter(|deadline| Instant::now() < *deadline)
        .and(state.log_hold_line);
    unsafe {
        // WM_SETTEXT resets the edit control's viewport. Keep the user's
        // recorded first line while the ten-second hold is active; otherwise
        // explicitly request the newest line.
        if let Some(line) = hold_line {
            SendMessageW(state.controls.log, EM_LINESCROLL, 0, line as isize);
        } else {
            SendMessageW(state.controls.log, 0x00b1, usize::MAX, -1);
            SendMessageW(state.controls.log, 0x00b7, 0, 0);
            SendMessageW(state.controls.log, 0x0115, 7, 0);
        }
    }
    if hold_line.is_none() {
        state.log_hold_line = None;
        state.log_hold_until = None;
    }
    state.log_first_visible_line = first_visible_log_line(state.controls.log);
    state.log_dirty = false;
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
        (c.planned_discs, "DVDA_PLANNED_DISCS"),
        (c.work_dir, "DVDA_BUILD_DIR"),
        (c.iso_prefix, "DVDA_ISO_PREFIX"),
        (c.group_limit, "DVDA_GROUP_TRACK_LIMIT"),
        (c.cache, "DVDA_PREPARE_CACHE"),
        (c.resume, "DVDA_RESUME"),
        (c.mode, "DVDA_MLP_SOURCE"),
        (c.rate, "DVDA_MLP_SURCODE_SAMPLE_RATE"),
        (c.bits, "DVDA_MLP_SURCODE_BITS"),
        (c.jobs, "DVDA_MLP_JOBS"),
        (c.metadata, "DVDA_MLP_METADATA_CONTEXT"),
        (c.import_folder, "DVDA_MLP_EXTERNAL_DIR"),
        (c.menu, "DVDA_MENU"),
        (c.tracks, "DVDA_MENU_TRACKS_PER_PAGE"),
        (c.stills, "DVDA_MENU_STILLPICS"),
        (c.cover, "DVDA_MENU_COVER_DIM"),
        (c.index, "DVDA_MENU_INDEX_MIN_ALBUMS"),
        (c.font_sc, "DVDA_MENU_FONT"),
        (c.font_jp, "DVDA_MENU_FONT_JP"),
        (c.font_kr, "DVDA_MENU_FONT_KR"),
        (c.author, "DVDA_AUTHOR"),
        (c.author_src, "DVDA_AUTHOR_SRC"),
        (c.keep_tmp, "DVDA_KEEP_TMP"),
        (c.keep_intermediate, "DVDA_KEEP_INTERMEDIATE"),
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
    if let Some(message) = batch_message(lang, text) {
        return message;
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
            WorkerEvent::Finished(event) => state.pending_finish = Some(event),
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
        BrowseKind::File => file_dialog(
            hwnd,
            false,
            &current,
            tr(state.lang, "browse"),
            state.lang,
            FileKind::Any,
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
    Any,
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
        FileKind::Any => (all, ""),
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
                (FileKind::Any, ""),
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
        let parent = create(ptr::null_mut(), "STATIC", "", 0, 0, 0, 1180, 820, 0);
        assert!(!parent.is_null());
        let mut ui = create_controls(parent);
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
        assert_eq!(
            edited_profile_values(&ui)["DVDA_MLP_SURCODE_SAMPLE_RATE"],
            "12345"
        );
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
        ui.busy = true;
        set_busy_controls(&ui, true);
        assert_eq!(unsafe { IsWindowEnabled(ui.controls.language) }, 0);
        assert_eq!(unsafe { IsWindowEnabled(ui.controls.profile) }, 0);
        for page in &ui.pages {
            assert_eq!(unsafe { IsWindowEnabled(*page) }, 0);
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
        assert!(!ui.busy);
        // More records than a Win32 message queue can hold must all reach the
        // archive before completion. Exercise the actual controls while live
        // viewing is paused, resumed and then completed with warnings.
        ui.close_when_done = false;
        ui.logs.clear();
        ui.problem_count = 0;
        ui.busy = true;
        ui.started = Some(Instant::now() - Duration::from_secs(2));
        set_busy_controls(&ui, true);
        let stress_path = root.join("stress.log");
        ui.log_file = Some((fs::File::create(&stress_path).unwrap(), stress_path.clone()));
        let producer_logs = ui.log_sender.clone();
        let producer = thread::spawn(move || {
            let mut callbacks = WorkerCallbacks {
                logs: producer_logs,
                cancel: Arc::new(AtomicBool::new(false)),
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
        assert_eq!(help.len(), 30);
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
