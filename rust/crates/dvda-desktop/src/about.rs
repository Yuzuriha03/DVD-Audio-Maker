use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/licenses.rs"));

use super::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetModuleHandleW,
    GetWindowLongPtrW, Hwnd, LoadCursorW, Lparam, Lresult, MoveWindow, RegisterClassW, SW_SHOW,
    SendMessageW, SetWindowLongPtrW, ShowWindow, UiState, WndClass, WndProc, Wparam, ptr, scale,
    set_text, tr, ui_font, wide,
};

const WS_OVERLAPPEDWINDOW: u32 = 0x00cf_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_VSCROLL: u32 = 0x0020_0000;
const WS_EX_CLIENTEDGE: u32 = 0x0000_0200;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const ES_READONLY: u32 = 0x0800;
const EM_SETLIMITTEXT: u32 = 0x00c5;
const EM_SETSEL: u32 = 0x00b1;
const EM_SCROLLCARET: u32 = 0x00b7;
const WM_SIZE: u32 = 0x0005;
const WM_COMMAND: u32 = 0x0111;
const WM_CLOSE: u32 = 0x0010;
const WM_DESTROY: u32 = 0x0002;
const CW_USEDEFAULT: i32 = i32::MIN;
const GWLP_USERDATA: i32 = -21;
const ID_LICENSE_BASE: usize = 7000;

const DOCUMENTS: [(&str, &str); 6] = [
    ("LICENSE", PROJECT_LICENSE),
    ("FFMPEG-LICENSE.txt", FFMPEG_LICENSE),
    ("THIRD-PARTY.txt", THIRD_PARTY_ZH),
    ("THIRD-PARTY.en.txt", THIRD_PARTY_EN),
    ("NOTICE-Image.txt", IMAGE_NOTICES),
    ("NOTICE-Menu.txt", MENU_NOTICES),
];

struct AboutState {
    edit: Hwnd,
}

static REGISTERED: OnceLock<()> = OnceLock::new();

unsafe extern "system" fn about_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    match message {
        WM_SIZE => {
            let state =
                unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AboutState).as_ref() };
            if let Some(state) = state {
                let mut rect = super::Rect::default();
                GetClientRect(hwnd, &mut rect);
                let margin = scale(14);
                MoveWindow(
                    state.edit,
                    margin,
                    scale(140),
                    (rect.right - margin * 2).max(1),
                    (rect.bottom - scale(154)).max(1),
                    1,
                );
            }
            0
        }
        WM_COMMAND => {
            let id = wparam & 0xffff;
            if (ID_LICENSE_BASE..ID_LICENSE_BASE + DOCUMENTS.len()).contains(&id) {
                let state =
                    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AboutState).as_ref() };
                if let Some(state) = state {
                    let (_, text) = DOCUMENTS[id - ID_LICENSE_BASE];
                    set_text(state.edit, text);
                    unsafe {
                        SendMessageW(state.edit, EM_SETSEL, 0, 0);
                        SendMessageW(state.edit, EM_SCROLLCARET, 0, 0);
                    }
                }
                return 0;
            }
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            let state =
                unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AboutState).as_ref() };
            if state.is_some() {
                unsafe {
                    drop(Box::from_raw(
                        GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AboutState
                    ));
                }
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn register_class(instance: super::Hinstance) {
    REGISTERED.get_or_init(|| {
        let class_name = wide("DVD_AUDIO_ABOUT_RUST");
        let registration = WndClass {
            style: 0,
            procedure: about_proc as WndProc,
            extra_class: 0,
            extra_window: 0,
            instance,
            icon: ptr::null_mut(),
            cursor: unsafe { LoadCursorW(ptr::null_mut(), 32512usize as *const u16) },
            background: 16usize as *mut std::ffi::c_void,
            menu: ptr::null(),
            class_name: class_name.as_ptr(),
        };
        unsafe { RegisterClassW(&raw const registration) };
    });
}

pub(crate) fn show_about(parent: Hwnd, state: &UiState) {
    let instance = unsafe { GetModuleHandleW(ptr::null()) };
    register_class(instance);
    let title = wide(tr(state.lang(), "author_page_title"));
    let class_name = wide("DVD_AUDIO_ABOUT_RUST");
    let window = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale(900),
            scale(680),
            parent,
            ptr::null_mut(),
            instance,
            ptr::null_mut(),
        )
    };
    if window.is_null() {
        return;
    }
    let font = ui_font();
    let summary = format!(
        "{}\n{}\n\n{}\n\n{}",
        tr(state.lang(), "author_name"),
        tr(state.lang(), "author_role"),
        tr(state.lang(), "author_bio"),
        tr(state.lang(), "project_url")
    );
    let summary = wide(&summary);
    let summary_control = unsafe {
        CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            summary.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            scale(14),
            scale(12),
            scale(860),
            scale(88),
            window,
            ptr::null_mut(),
            instance,
            ptr::null_mut(),
        )
    };
    unsafe { SendMessageW(summary_control, 0x0030, font as usize, 1) };
    for (index, (name, _)) in DOCUMENTS.iter().enumerate() {
        let label = wide(name);
        let button = unsafe {
            CreateWindowExW(
                0,
                wide("BUTTON").as_ptr(),
                label.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                scale(14 + i32::try_from(index).unwrap() * 142),
                scale(100),
                scale(136),
                scale(28),
                window,
                (ID_LICENSE_BASE + index) as super::Hmenu,
                instance,
                ptr::null_mut(),
            )
        };
        unsafe { SendMessageW(button, 0x0030, font as usize, 1) };
    }
    let edit = unsafe {
        CreateWindowExW(
            WS_EX_CLIENTEDGE,
            wide("EDIT").as_ptr(),
            ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_VSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_READONLY,
            scale(14),
            scale(140),
            scale(860),
            scale(500),
            window,
            ptr::null_mut(),
            instance,
            ptr::null_mut(),
        )
    };
    unsafe {
        SendMessageW(edit, 0x0030, font as usize, 1);
        SendMessageW(edit, EM_SETLIMITTEXT, 2_000_000, 0);
        set_text(edit, PROJECT_LICENSE);
        let state = Box::into_raw(Box::new(AboutState { edit }));
        SetWindowLongPtrW(window, GWLP_USERDATA, state as isize);
        ShowWindow(window, SW_SHOW);
    }
}

#[cfg(test)]
mod tests {
    use super::DOCUMENTS;

    #[test]
    fn all_licenses_are_embedded() {
        for (name, text) in DOCUMENTS {
            assert_ne!(name, "");
            assert!(text.len() > 100, "{name} was not embedded");
        }
    }
}
