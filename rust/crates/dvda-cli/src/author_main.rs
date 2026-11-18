use dvda_native::media::Callbacks;

mod console;

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
        crate::console::cancelled()
    }
}
fn main() {
    crate::console::install_cancel_handler();
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = dvda_core::author_runtime::execute(&arguments, &mut Console);
    std::process::exit(result.exit_code.unwrap_or(1));
}
