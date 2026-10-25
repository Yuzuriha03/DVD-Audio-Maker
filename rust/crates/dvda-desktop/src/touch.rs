use super::*;

const WM_GESTURE: u32 = 0x0119;
const GID_PAN: u32 = 4;
const PAN_WANT: u32 = 1 | 2 | 8 | 16;
const PAN_BLOCK: u32 = 4;

#[repr(C)]
struct GestureInfo {
    size: u32,
    flags: u32,
    id: u32,
    target: Hwnd,
    x: i16,
    y: i16,
    instance: u32,
    sequence: u32,
    arguments: u64,
    extra_size: u32,
}

#[repr(C)]
struct GestureConfig { id: u32, want: u32, block: u32 }

#[link(name = "user32")]
unsafe extern "system" {
    fn SetGestureConfig(hwnd: Hwnd, reserved: u32, count: u32, config: *const GestureConfig, size: u32) -> i32;
    fn GetGestureInfo(handle: Handle, info: *mut GestureInfo) -> i32;
    fn CloseGestureInfoHandle(handle: Handle) -> i32;
}

type SubclassProc = unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam, usize, usize) -> Lresult;
#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(hwnd: Hwnd, proc: SubclassProc, id: usize, data: usize) -> i32;
    fn RemoveWindowSubclass(hwnd: Hwnd, proc: SubclassProc, id: usize) -> i32;
    fn DefSubclassProc(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
}

struct Pan { last_y: Option<i16>, remainder: i32, log: bool }

#[repr(C)]
struct ComboInfo {
    size: u32,
    item: Rect,
    button: Rect,
    button_state: u32,
    combo: Hwnd,
    edit: Hwnd,
    list: Hwnd,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetComboBoxInfo(hwnd: Hwnd, info: *mut ComboInfo) -> i32;
}

pub(super) fn install_combo(hwnd: Hwnd) {
    let mut info: ComboInfo = unsafe { std::mem::zeroed() };
    info.size = std::mem::size_of::<ComboInfo>() as u32;
    if unsafe { GetComboBoxInfo(hwnd, &mut info) } != 0 {
        install(info.list, false);
    }
}

pub(super) fn install(hwnd: Hwnd, log: bool) {
    let config = GestureConfig { id: GID_PAN, want: PAN_WANT, block: PAN_BLOCK };
    unsafe {
        if SetGestureConfig(hwnd, 0, 1, &config, std::mem::size_of::<GestureConfig>() as u32) == 0 { return; }
        let data = Box::into_raw(Box::new(Pan { last_y: None, remainder: 0, log }));
        if SetWindowSubclass(hwnd, pan_proc, 1, data as usize) == 0 {
            drop(Box::from_raw(data));
        }
    }
}

fn line_delta(remainder: &mut i32, delta: i32, height: i32) -> i32 {
    *remainder += delta;
    let lines = *remainder / height;
    *remainder %= height;
    lines
}

unsafe extern "system" fn pan_proc(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam, id: usize, data: usize) -> Lresult {
    if message == WM_NCDESTROY {
        unsafe {
            RemoveWindowSubclass(hwnd, pan_proc, id);
            drop(Box::from_raw(data as *mut Pan));
            return DefSubclassProc(hwnd, message, wparam, lparam);
        }
    }
    if message == WM_GESTURE {
        let mut info: GestureInfo = unsafe { std::mem::zeroed() };
        info.size = std::mem::size_of::<GestureInfo>() as u32;
        if unsafe { GetGestureInfo(lparam as Handle, &mut info) } != 0 && info.id == GID_PAN {
            let pan = unsafe { &mut *(data as *mut Pan) };
            if info.flags & 1 != 0 { pan.last_y = None; pan.remainder = 0; }
            if let Some(last) = pan.last_y {
                let delta = i32::from(last) - i32::from(info.y);
                if pan.log {
                    let lines = line_delta(&mut pan.remainder, delta, scale(19).max(1));
                    win::send_int(hwnd, EM_LINESCROLL, 0, lines as isize);
                    // The log EDIT sits in a frame inside the main window.
                    if let Some(ui) = unsafe { state(GetParent(GetParent(hwnd))) } { observe_log_scroll(ui); }
                } else if window_class(hwnd) == "ComboLBox" {
                    let lines = line_delta(&mut pan.remainder, delta, scale(44).max(1));
                    unsafe {
                        let top = SendMessageW(hwnd, 0x018e, 0, 0); // LB_GETTOPINDEX
                        let count = SendMessageW(hwnd, 0x018b, 0, 0); // LB_GETCOUNT
                        let next = (top + lines as isize).clamp(0, (count - 1).max(0));
                        SendMessageW(hwnd, 0x0197, next as usize, 0); // LB_SETTOPINDEX
                    }
                } else {
                    // Page controls, and combo overlays one level deeper.
                    let mut page = hwnd;
                    for _ in 0..2 {
                        if window_class(page) == "DVD_AUDIO_PAGE_RUST" {
                            break;
                        }
                        page = GetParent(page);
                    }
                    if window_class(page) == "DVD_AUDIO_PAGE_RUST" {
                        let old = GetWindowLongPtrW(page, GWLP_USERDATA) as i32;
                        scroll_page(page, old.saturating_add(delta), true);
                    }
                }
            }
            pan.last_y = if info.flags & 4 != 0 { None } else { Some(info.y) };
            unsafe { CloseGestureInfoHandle(lparam as Handle); }
            return 0;
        }
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_window_enables_pan_and_vertical_single_finger_input() {
        let _ui_test_lock = ui_test_guard();
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetGestureConfig(hwnd: Hwnd, reserved: u32, flags: u32, count: *mut u32, config: *mut GestureConfig, size: u32) -> i32;
        }
        let hwnd = unsafe {
            CreateWindowExW(0, wide("STATIC").as_ptr(), wide("").as_ptr(), 0,
                0, 0, 100, 100, ptr::null_mut(), ptr::null_mut(),
                GetModuleHandleW(ptr::null()), ptr::null_mut())
        };
        assert!(!hwnd.is_null());
        install(hwnd, false);
        let mut config = GestureConfig { id: GID_PAN, want: 0, block: 0 };
        let mut count = 1;
        let result = unsafe {
            GetGestureConfig(hwnd, 0, 0, &mut count, &mut config,
                std::mem::size_of::<GestureConfig>() as u32)
        };
        DestroyWindow(hwnd);
        assert_ne!(result, 0);
        assert_eq!(config.want, PAN_WANT);
        assert_eq!(config.block, PAN_BLOCK);
        assert_ne!(config.want & 1, 0, "GC_PAN is required to enable pan gestures");
    }

    #[test]
    fn pan_accumulates_subline_motion_in_both_directions() {
        let _ui_test_lock = ui_test_guard();
        let mut remainder = 0;
        assert_eq!(line_delta(&mut remainder, 12, 19), 0);
        assert_eq!(line_delta(&mut remainder, 12, 19), 1);
        assert_eq!(line_delta(&mut remainder, -24, 19), -1);
        assert_eq!(remainder, 0);
    }
}
