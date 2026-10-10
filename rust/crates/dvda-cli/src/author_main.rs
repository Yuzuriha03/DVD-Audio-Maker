use dvda_native::media::Callbacks;
use std::sync::atomic::{AtomicBool, Ordering};
static CANCELLED: AtomicBool = AtomicBool::new(false);
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}
unsafe extern "system" fn control(event: u32) -> i32 {
    if event <= 1 {
        CANCELLED.store(true, Ordering::Release);
        1
    } else {
        0
    }
}
struct Console;
impl Callbacks for Console {
    fn emit(&mut self, stream: i32, text: &str) {
        if stream == 2 {
            eprintln!("{text}");
        } else {
            println!("{text}");
        }
    }
    fn cancelled(&mut self) -> bool {
        CANCELLED.load(Ordering::Acquire)
    }
}
fn main() {
    unsafe {
        SetConsoleCtrlHandler(Some(control), 1);
    }
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = dvda_core::author_runtime::execute(&arguments, &mut Console);
    std::process::exit(result.exit_code.unwrap_or(1));
}
