//! Safe wrapper for window messages that carry plain integers.
//!
//! `SendMessageW` is unsafe in general because a receiving window procedure may
//! interpret `lparam` or `wparam` as a pointer. Each message allowed here passes
//! only integers, handles or flags in both parameters, so no receiver can use the
//! arguments to reach memory in this process. Messages outside the list are
//! rejected, which keeps the function sound for every argument value.

use super::*;

/// Messages whose `wparam` and `lparam` are integers, handles or flags only.
const INTEGER_MESSAGES: &[u32] = &[
    0x000b, // WM_SETREDRAW
    0x0010, // WM_CLOSE
    0x0030, // WM_SETFONT (wparam: font handle)
    0x0031, // WM_GETFONT
    0x0111, // WM_COMMAND
    0x0115, // WM_VSCROLL
    0x00b1, // EM_SETSEL
    0x00b6, // EM_LINESCROLL
    0x00ba, // EM_GETLINECOUNT
    0x00c5, // EM_SETLIMITTEXT
    0x00ce, // EM_GETFIRSTVISIBLELINE
    0x00cf, // EM_SETREADONLY
    0x00d3, // EM_SETMARGINS (lparam: packed margins)
    0x00f0, // BM_GETCHECK
    0x00f1, // BM_SETCHECK
    0x0146, // CB_GETCOUNT
    0x0147, // CB_GETCURSEL
    0x0149, // CB_GETLBTEXTLEN
    0x014b, // CB_RESETCONTENT
    0x014e, // CB_SETCURSEL
    0x014f, // CB_SHOWDROPDOWN
    0x0157, // CB_GETDROPPEDSTATE
    0x015f, // CB_GETDROPPEDWIDTH
    0x0402, // PBM_SETPOS
    0x0406, // PBM_SETRANGE32
    0x0418, // TTM_SETMAXTIPWIDTH
];

/// Sends an integer-only message and returns its result.
///
/// Messages outside `INTEGER_MESSAGES` are refused (returning 0), so the call
/// never hands a pointer to another window.
pub(crate) fn send_int(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize {
    if !INTEGER_MESSAGES.contains(&message) {
        debug_assert!(false, "message {message:#x} carries pointers; use an unsafe call");
        return 0;
    }
    // SAFETY: the message is in the integer-only list, so neither parameter is
    // dereferenced by the receiver. An invalid handle only makes the call fail.
    unsafe { SendMessageW(hwnd, message, wparam, lparam) }
}
