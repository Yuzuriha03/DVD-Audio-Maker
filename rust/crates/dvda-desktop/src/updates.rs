//! Non-blocking `WinHTTP` update checks shared by startup and the About page.
//!
//! This module only reads the release JSON and, after an explicit user
//! confirmation, asks the shell to open the published download URL. It never
//! executes or installs anything.

use std::{
    ffi::c_void,
    fmt,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use dvda_core::updates::{Platform, RELEASE_API_URL, Update, select_update};

use super::{
    Hwnd, IsWindow, Lang, Lparam, PostMessageW, ShellExecuteW, dialog_ui, header_icon, ptr, tr,
    wide,
};

/// Custom message posted to the main window after a background check finishes.
pub(crate) const WM_UPDATE_CHECK_RESULT: u32 = 0x8000 + 0x51;

const WINHTTP_QUERY_STATUS_CODE: u32 = 19;
const WINHTTP_QUERY_FLAG_NUMBER: u32 = 0x2000_0000;
const WINHTTP_FLAG_SECURE: u32 = 0x0080_0000;
const WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY: u32 = 4;
const WINHTTP_OPTION_DISABLE_FEATURE: u32 = 63;
const WINHTTP_DISABLE_REDIRECTS: u32 = 2;
const SW_SHOW: i32 = 5;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const NETWORK_TIMEOUT_MS: i32 = 15_000;

const PRODUCT_VERSION: &str = env!("DVDA_DESKTOP_PRODUCT_VERSION");

/// Product version shown by About and sent to the release selector.
pub(crate) fn update_version() -> &'static str {
    PRODUCT_VERSION.trim().trim_start_matches(['v', 'V'])
}

/// Display form for the About page; packaged values keep their leading v marker.
pub(crate) fn display_version() -> String {
    format!("v{}", update_version())
}

pub(crate) struct CheckResult {
    manual: bool,
    pub(crate) lang: Lang,
    outcome: Result<Option<Update>, UpdateCheckError>,
}

struct CheckState {
    running: bool,
    manual: bool,
    result: Option<CheckResult>,
}

static CHECK: Mutex<CheckState> = Mutex::new(CheckState {
    running: false,
    manual: false,
    result: None,
});

/// At most one worker runs. A manual request can take ownership of either
/// an in-flight check or its result before the UI processes the notification.
pub(crate) fn request_check(owner: Hwnd, manual: bool, lang: Lang) {
    if manual {
        remove_auto_update_notification();
    }
    {
        let Ok(mut state) = CHECK.lock() else {
            return;
        };
        if let Some(result) = &mut state.result {
            if !manual {
                return;
            }
            state.result = None;
        }
        if state.running {
            state.manual |= manual;
            return;
        }
        state.running = true;
        state.manual = manual;
    }
    let owner = owner as isize;
    if let Err(error) = thread::Builder::new()
        .name("update-check".into())
        .spawn(move || finish_worker(owner, lang, check_blocking_inner()))
    {
        finish_worker(
            owner,
            lang,
            Err(UpdateCheckError::Transport(error.to_string())),
        );
    }
}

fn finish_worker(owner: isize, lang: Lang, outcome: Result<Option<Update>, UpdateCheckError>) {
    let Ok(mut state) = CHECK.lock() else {
        return;
    };
    state.running = false;
    state.result = Some(CheckResult {
        manual: state.manual,
        lang,
        outcome,
    });
    // PostMessageW queues a message; it will not call back while CHECK is locked.
    if owner == 0 || unsafe { PostMessageW(owner as Hwnd, WM_UPDATE_CHECK_RESULT, 0, 0) } == 0 {
        state.result = None;
    }
}

pub(crate) fn take_result() -> Option<CheckResult> {
    CHECK.lock().ok().and_then(|mut state| state.result.take())
}

/// Startup notifications are limited to an available update; manual checks
/// always surface their result through the project's modal dialogs.
pub(crate) fn finish_check(main: Hwnd, lang: Lang, result: CheckResult) {
    super::about::reset_update_check();
    if IsWindow(main) == 0 {
        return;
    }
    let dialog_owner = super::about::current_window().unwrap_or(main);
    let CheckResult {
        manual, outcome, ..
    } = result;
    match outcome {
        Ok(Some(update)) if manual => {
            show_manual_update(dialog_owner, lang, &update.version, &update.download_url);
        }
        Ok(Some(update)) => {
            show_auto_update_notification(main, lang, &update.version, &update.download_url);
        }
        Ok(None) if manual => dialog_ui::show_message(
            dialog_owner,
            lang,
            dialog_ui::DialogKind::Warning,
            tr(lang, "check_updates"),
            tr(lang, "update_none"),
        ),
        Err(error) if manual => {
            let key = if matches!(&error, UpdateCheckError::Network(_)) {
                "update_network_failed"
            } else {
                "update_failed"
            };
            dialog_ui::show_message(
                dialog_owner,
                lang,
                dialog_ui::DialogKind::Error,
                tr(lang, "check_updates"),
                &tr(lang, key).replace("{0}", &error.to_string()),
            );
        }
        Ok(None) | Err(_) => {}
    }
}

fn show_manual_update(main: Hwnd, lang: Lang, version: &str, download_url: &str) {
    let title = tr(lang, "update_available_title");
    let message = tr(lang, "update_available_message").replace("{0}", version);
    if dialog_ui::show_choice(
        main,
        lang,
        dialog_ui::DialogKind::Warning,
        title,
        &message,
        tr(lang, "update_download"),
        tr(lang, "update_later"),
    ) == Some(true)
        && let Err(error) = open_download(main, download_url)
    {
        dialog_ui::show_message(
            main,
            lang,
            dialog_ui::DialogKind::Error,
            title,
            &tr(lang, "update_open_failed").replace("{0}", &error),
        );
    }
}

const WM_UPDATE_NOTIFICATION: u32 = 0x8000 + 0x52;
const NIN_BALLOONUSERCLICK: u32 = 0x0405;
const NIN_BALLOONTIMEOUT: u32 = 0x0404;
const NIN_BALLOONHIDE: u32 = 0x0403;
const NIM_ADD: u32 = 0;
const NIM_MODIFY: u32 = 1;
const NIM_DELETE: u32 = 2;
const NIF_MESSAGE: u32 = 1;
const NIF_ICON: u32 = 2;
const NIF_TIP: u32 = 4;
const NIF_INFO: u32 = 0x10;
const NIIF_INFO: u32 = 1;
const NOTIFICATION_ID: u32 = 1;
const NOTIFICATION_TIMEOUT_MS: u32 = 10_000;

#[allow(
    dead_code,
    reason = "Windows consumes these NOTIFYICONDATAW fields through its FFI ABI"
)]
#[repr(C)]
struct NotifyIconData {
    size: u32,
    hwnd: Hwnd,
    id: u32,
    flags: u32,
    callback_message: u32,
    icon: super::Handle,
    tip: [u16; 128],
    state: u32,
    state_mask: u32,
    info: [u16; 256],
    timeout_or_version: u32,
    info_title: [u16; 64],
    info_flags: u32,
    guid: [u8; 16],
    balloon_icon: super::Handle,
}

static NOTIFICATION_ADDED: AtomicBool = AtomicBool::new(false);
static NOTIFICATION_URL: Mutex<Option<String>> = Mutex::new(None);

fn show_auto_update_notification(main: Hwnd, lang: Lang, version: &str, download_url: &str) {
    if !download_url.starts_with("https://") {
        return;
    }
    remove_auto_update_notification();
    let mut data: NotifyIconData = unsafe { std::mem::zeroed() };
    data.size = u32::try_from(std::mem::size_of::<NotifyIconData>()).unwrap_or(0);
    data.hwnd = main;
    data.id = NOTIFICATION_ID;
    data.flags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.callback_message = WM_UPDATE_NOTIFICATION;
    data.icon = header_icon();
    copy_wide(&mut data.tip, "DVD-Audio Maker");
    if unsafe { Shell_NotifyIconW(NIM_ADD, &raw const data) } == 0 {
        return;
    }
    NOTIFICATION_OWNER.store(main as isize, Ordering::Release);
    NOTIFICATION_ADDED.store(true, Ordering::Release);
    if let Ok(mut url) = NOTIFICATION_URL.lock() {
        *url = Some(download_url.to_owned());
    }
    data.flags = NIF_INFO;
    data.info_title = [0; 64];
    data.info = [0; 256];
    copy_wide(
        &mut data.info,
        &tr(lang, "update_available_message").replace("{0}", version),
    );
    copy_wide(&mut data.info_title, tr(lang, "update_available_title"));
    data.info_flags = NIIF_INFO;
    data.timeout_or_version = NOTIFICATION_TIMEOUT_MS;
    if unsafe { Shell_NotifyIconW(NIM_MODIFY, &raw const data) } == 0 {
        remove_auto_update_notification();
    }
}

pub(crate) fn handle_notification_callback(owner: Hwnd, message: u32, lparam: Lparam) -> bool {
    if message != WM_UPDATE_NOTIFICATION {
        return false;
    }
    let event = u32::try_from(lparam & 0xffff).unwrap_or(0);
    match event {
        NIN_BALLOONUSERCLICK => {
            let url = NOTIFICATION_URL.lock().ok().and_then(|mut url| url.take());
            remove_auto_update_notification();
            if let Some(url) = url {
                let _ = open_download(owner, &url);
            }
        }
        NIN_BALLOONTIMEOUT | NIN_BALLOONHIDE => remove_auto_update_notification(),
        _ => {}
    }
    true
}

pub(crate) fn remove_auto_update_notification() {
    if let Ok(mut url) = NOTIFICATION_URL.lock() {
        *url = None;
    }
    if NOTIFICATION_ADDED.swap(false, Ordering::AcqRel) {
        let mut data: NotifyIconData = unsafe { std::mem::zeroed() };
        data.size = u32::try_from(std::mem::size_of::<NotifyIconData>()).unwrap_or(0);
        data.hwnd = NOTIFICATION_OWNER.load(Ordering::Acquire) as Hwnd;
        data.id = NOTIFICATION_ID;
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &raw const data);
        }
    }
}

static NOTIFICATION_OWNER: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

fn copy_wide(destination: &mut [u16], text: &str) {
    let source = wide(text);
    let copy = destination.len().min(source.len());
    destination[..copy].copy_from_slice(&source[..copy]);
    if copy == destination.len() {
        destination[copy - 1] = 0;
    }
}

fn open_download(owner: Hwnd, url: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("Download URL is not HTTPS".into());
    }
    let verb = wide("open");
    let target = wide(url);
    let result = unsafe {
        ShellExecuteW(
            owner,
            verb.as_ptr(),
            target.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOW,
        )
    };
    if (result as usize) > 32 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

fn check_blocking_inner() -> Result<Option<Update>, UpdateCheckError> {
    let (host, port, path) =
        parse_https_url(RELEASE_API_URL).map_err(UpdateCheckError::Transport)?;
    let agent = wide("DVD-Audio-Maker");
    let session = Session(unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            ptr::null(),
            ptr::null(),
            0,
        )
    });
    if session.0.is_null() {
        return Err(UpdateCheckError::Network(last_error("WinHttpOpen")));
    }
    if unsafe {
        WinHttpSetTimeouts(
            session.0,
            NETWORK_TIMEOUT_MS,
            NETWORK_TIMEOUT_MS,
            NETWORK_TIMEOUT_MS,
            NETWORK_TIMEOUT_MS,
        )
    } == 0
    {
        return Err(UpdateCheckError::Network(last_error("WinHttpSetTimeouts")));
    }
    let host_wide = wide(&host);
    let connect = Connect(unsafe { WinHttpConnect(session.0, host_wide.as_ptr(), port, 0) });
    if connect.0.is_null() {
        return Err(UpdateCheckError::Network(last_error("WinHttpConnect")));
    }
    let verb = wide("GET");
    let object = wide(&path);
    let request = Request(unsafe {
        WinHttpOpenRequest(
            connect.0,
            verb.as_ptr(),
            object.as_ptr(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    });
    if request.0.is_null() {
        return Err(UpdateCheckError::Network(last_error("WinHttpOpenRequest")));
    }
    let disabled = WINHTTP_DISABLE_REDIRECTS;
    if unsafe {
        WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_DISABLE_FEATURE,
            (&raw const disabled).cast(),
            4,
        )
    } == 0
    {
        return Err(UpdateCheckError::Transport(last_error("WinHttpSetOption")));
    }
    let headers = wide("Accept: application/vnd.github+json\r\n");
    if unsafe {
        WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            u32::MAX,
            ptr::null_mut(),
            0,
            0,
            0,
        )
    } == 0
    {
        return Err(UpdateCheckError::Network(last_error("WinHttpSendRequest")));
    }
    if unsafe { WinHttpReceiveResponse(request.0, ptr::null_mut()) } == 0 {
        return Err(UpdateCheckError::Network(last_error(
            "WinHttpReceiveResponse",
        )));
    }
    require_http_ok(&request)?;
    let response = read_release_json(&request)?;

    let platform = if cfg!(target_arch = "aarch64") {
        Platform::WindowsArm64
    } else {
        Platform::WindowsX64
    };
    select_update(update_version(), &response, platform).map_err(UpdateCheckError::Update)
}

fn require_http_ok(request: &Request) -> Result<(), UpdateCheckError> {
    let mut status: u32 = 0;
    let mut status_len = u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4);
    if unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(),
            (&raw mut status).cast::<c_void>(),
            &raw mut status_len,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(UpdateCheckError::Transport(last_error(
            "WinHttpQueryHeaders",
        )));
    }
    if status != 200 {
        return Err(UpdateCheckError::Http(status));
    }
    Ok(())
}

fn read_release_json(request: &Request) -> Result<String, UpdateCheckError> {
    let mut body = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let mut read: u32 = 0;
        if unsafe {
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr().cast::<c_void>(),
                u32::try_from(chunk.len()).unwrap_or(0),
                &raw mut read,
            )
        } == 0
        {
            return Err(UpdateCheckError::Network(last_error("WinHttpReadData")));
        }
        let read = usize::try_from(read).unwrap_or(0);
        if read == 0 {
            break;
        }
        if body.len().saturating_add(read) > MAX_RESPONSE_BYTES {
            return Err(UpdateCheckError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk[..read]);
    }
    String::from_utf8(body).map_err(|_| UpdateCheckError::InvalidUtf8)
}

fn parse_https_url(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| "Release API URL must use HTTPS".to_owned())?;
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    if authority.is_empty() || authority.contains('@') || authority.contains('[') {
        return Err("Release API URL has an unsupported authority".into());
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => (
            host,
            port.parse::<u16>()
                .map_err(|_| "Release API URL has an invalid port".to_owned())?,
        ),
        Some(_) => return Err("Release API URL has an invalid port".into()),
        None => (authority, 443),
    };
    if host.is_empty() {
        return Err("Release API URL has no host".into());
    }
    Ok((host.to_owned(), port, path.to_owned()))
}

fn last_error(operation: &str) -> String {
    format!("{operation}: {}", std::io::Error::last_os_error())
}

struct Session(*mut c_void);
impl Drop for Session {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
}

struct Connect(*mut c_void);
impl Drop for Connect {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
}

struct Request(*mut c_void);
impl Drop for Request {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
}

enum UpdateCheckError {
    Network(String),
    Transport(String),
    Http(u32),
    ResponseTooLarge,
    InvalidUtf8,
    Update(dvda_core::updates::UpdateError),
}

impl fmt::Display for UpdateCheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(message) | Self::Transport(message) => formatter.write_str(message),
            Self::Http(status) => write!(formatter, "release API returned HTTP {status}"),
            Self::ResponseTooLarge => formatter.write_str("release API response exceeds 1 MiB"),
            Self::InvalidUtf8 => formatter.write_str("release API response is not UTF-8"),
            Self::Update(error) => write!(formatter, "{error}"),
        }
    }
}

type HInternet = *mut c_void;

#[link(name = "shell32")]
unsafe extern "system" {
    fn Shell_NotifyIconW(message: u32, data: *const NotifyIconData) -> i32;
}

#[link(name = "winhttp")]
unsafe extern "system" {
    fn WinHttpOpen(
        agent: *const u16,
        access_type: u32,
        proxy: *const u16,
        bypass: *const u16,
        flags: u32,
    ) -> HInternet;
    fn WinHttpConnect(
        session: HInternet,
        server: *const u16,
        port: u16,
        reserved: u32,
    ) -> HInternet;
    fn WinHttpOpenRequest(
        connect: HInternet,
        verb: *const u16,
        object: *const u16,
        version: *const u16,
        referrer: *const u16,
        accept_types: *const *const u16,
        flags: u32,
    ) -> HInternet;
    fn WinHttpSendRequest(
        request: HInternet,
        headers: *const u16,
        headers_length: u32,
        optional: *mut c_void,
        optional_length: u32,
        total_length: u32,
        context: usize,
    ) -> i32;
    fn WinHttpReceiveResponse(request: HInternet, reserved: *mut c_void) -> i32;
    fn WinHttpQueryHeaders(
        request: HInternet,
        info_level: u32,
        name: *const u16,
        buffer: *mut c_void,
        buffer_length: *mut u32,
        index: *mut u32,
    ) -> i32;
    fn WinHttpReadData(
        request: HInternet,
        buffer: *mut c_void,
        bytes_to_read: u32,
        bytes_read: *mut u32,
    ) -> i32;
    fn WinHttpSetTimeouts(
        session: HInternet,
        resolve: i32,
        connect: i32,
        send: i32,
        receive: i32,
    ) -> i32;
    fn WinHttpSetOption(handle: HInternet, option: u32, buffer: *const c_void, length: u32) -> i32;
    fn WinHttpCloseHandle(handle: HInternet) -> i32;
}
