//! Colours and light/dark theme switching.

use super::*;

// Visual language follows the application icon: deep navy, cool silver and a
// cyan waveform accent. COLORREF is 0x00BBGGRR, so rgb() converts from the
// familiar 0xRRGGBB order.
pub(crate) const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    (b << 16) | (g << 8) | r
}

pub(crate) fn c_header_top() -> u32 {
    if dark_theme() {
        rgb(7, 11, 18)
    } else {
        rgb(10, 23, 39)
    }
}

pub(crate) fn c_header_bottom() -> u32 {
    if dark_theme() {
        rgb(17, 32, 49)
    } else {
        rgb(21, 51, 82)
    }
}

pub(crate) fn c_accent() -> u32 {
    rgb(88, 205, 224)
}

pub(crate) fn c_accent_hover() -> u32 {
    rgb(120, 219, 235)
}

pub(crate) fn c_accent_soft() -> u32 {
    if dark_theme() {
        rgb(30, 58, 68)
    } else {
        rgb(224, 245, 249)
    }
}

pub(crate) fn c_accent_pressed() -> u32 {
    rgb(56, 172, 192)
}

pub(crate) fn c_body() -> u32 {
    if dark_theme() {
        rgb(13, 19, 27)
    } else {
        rgb(243, 246, 250)
    }
}

pub(crate) fn c_panel() -> u32 {
    if dark_theme() {
        rgb(19, 27, 38)
    } else {
        rgb(240, 245, 251)
    }
}

pub(crate) fn c_card() -> u32 {
    if dark_theme() {
        rgb(26, 35, 47)
    } else {
        rgb(255, 255, 255)
    }
}

pub(crate) fn c_secondary_hover() -> u32 {
    if dark_theme() {
        rgb(32, 43, 57)
    } else {
        rgb(244, 248, 252)
    }
}

pub(crate) fn c_secondary_pressed() -> u32 {
    if dark_theme() {
        rgb(39, 52, 68)
    } else {
        rgb(233, 240, 248)
    }
}

pub(crate) fn c_border() -> u32 {
    if dark_theme() {
        rgb(45, 58, 74)
    } else {
        rgb(214, 223, 233)
    }
}

pub(crate) fn c_border_soft() -> u32 {
    if dark_theme() {
        rgb(33, 44, 57)
    } else {
        rgb(228, 235, 242)
    }
}

pub(crate) fn c_ink() -> u32 {
    if dark_theme() {
        rgb(230, 238, 247)
    } else {
        rgb(24, 40, 58)
    }
}

pub(crate) fn c_muted() -> u32 {
    if dark_theme() {
        rgb(139, 156, 176)
    } else {
        rgb(99, 116, 138)
    }
}

pub(crate) fn c_header_title() -> u32 {
    rgb(240, 247, 252)
}

pub(crate) fn c_header_subtitle() -> u32 {
    if dark_theme() {
        rgb(143, 166, 191)
    } else {
        rgb(160, 184, 208)
    }
}

pub(crate) fn c_danger() -> u32 {
    if dark_theme() {
        rgb(255, 138, 133)
    } else {
        rgb(180, 35, 24)
    }
}

pub(crate) fn c_danger_hover() -> u32 {
    if dark_theme() {
        rgb(58, 29, 30)
    } else {
        rgb(253, 232, 232)
    }
}

pub(crate) fn c_danger_pressed() -> u32 {
    if dark_theme() {
        rgb(74, 38, 39)
    } else {
        rgb(248, 214, 214)
    }
}

pub(crate) fn c_danger_border() -> u32 {
    if dark_theme() {
        rgb(107, 52, 52)
    } else {
        rgb(228, 184, 184)
    }
}

pub(crate) fn c_disabled_fill() -> u32 {
    if dark_theme() {
        rgb(35, 44, 57)
    } else {
        rgb(226, 233, 240)
    }
}

pub(crate) fn c_disabled_text() -> u32 {
    if dark_theme() {
        rgb(110, 124, 142)
    } else {
        rgb(146, 160, 175)
    }
}

pub(crate) fn c_bar_track() -> u32 {
    if dark_theme() {
        rgb(29, 39, 51)
    } else {
        rgb(228, 235, 243)
    }
}

pub(crate) fn dark_theme() -> bool {
    DARK_THEME.load(Ordering::Relaxed)
}

pub(crate) fn system_prefers_dark() -> bool {
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

pub(crate) static UXTHEME_ALLOW_DARK: OnceLock<Option<isize>> = OnceLock::new();

pub(crate) fn uxtheme_allow_dark(hwnd: Hwnd, allow: bool) {
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

pub(crate) static UXTHEME_SET_PREFERRED: OnceLock<Option<isize>> = OnceLock::new();

pub(crate) fn uxtheme_preferred_app_mode(dark: bool) {
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

/// # Safety
/// Window procedure called by Windows. `hwnd` is a valid window handle, and for messages
/// that pass pointers in `lparam` (such as WM_DRAWITEM or WM_NCCALCSIZE) it points to the
/// structure the system defines for that message, valid for the duration of the call.
pub(crate) unsafe extern "system" fn theme_child_proc(hwnd: Hwnd, _parameter: Lparam) -> i32 {
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

pub(crate) fn apply_theme(hwnd: Hwnd, state: &mut UiState, dark: bool) {
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

pub(crate) fn toggle_theme(hwnd: Hwnd, state: &mut UiState) {
    let next = !dark_theme();
    // A manual choice overrides the system only for this session.
    state.theme_override = Some(next);
    apply_theme(hwnd, state, next);
}
