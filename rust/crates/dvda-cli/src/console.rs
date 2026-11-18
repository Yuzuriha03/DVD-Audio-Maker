//! Shared Windows cancellation handler for both developer binaries.
use std::sync::atomic::{AtomicBool, Ordering};

static CANCELLED: AtomicBool = AtomicBool::new(false);
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}
extern "system" fn control(event: u32) -> i32 {
    if event <= 1 {
        CANCELLED.store(true, Ordering::Release);
        1
    } else {
        0
    }
}

pub(crate) fn install_cancel_handler() {
    // SAFETY: The handler has the Win32 callback ABI and accesses only a static atomic.
    unsafe { SetConsoleCtrlHandler(Some(control), 1) };
}

pub(crate) fn cancelled() -> bool {
    CANCELLED.load(Ordering::Acquire)
}
