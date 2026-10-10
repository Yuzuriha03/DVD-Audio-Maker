//! Validate embedded-runtime startup in a fresh process without development DLL paths.
use dvda_native::media::{Callbacks, Media, Operation, OutputFormat, Request};
use std::{fs, path::Path};

struct Events;
impl Callbacks for Events {
    fn emit(&mut self, _: i32, text: &str) {
        println!("{text}");
    }
    fn cancelled(&mut self) -> bool {
        false
    }
}
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command, directory, archive] if command == "pack" => {
            fs::write(archive, dvda_core::runtime::pack(Path::new(directory))?)
                .map_err(|e| e.to_string())?;
        }
        [command, archive, input] if command == "probe" => {
            let data = fs::read(archive).map_err(|e| e.to_string())?;
            // SAFETY: this fresh process has not started threads or native components.
            unsafe { dvda_core::runtime::initialize(&data)? };
            let request = Request {
                operation: Operation::Probe,
                input: input.clone(),
                output: None,
                rate: 0,
                bits: 0,
                output_format: OutputFormat::None,
                soxr: false,
                compression: 0,
                cover: false,
                tags: Vec::new(),
            };
            let status = Media::linked()
                .run(&request, &mut Events)
                .map_err(|e| e.to_string())?;
            if status != 0 {
                return Err(format!("Embedded runtime media probe failed: {status}"));
            }
        }
        _ => {
            return Err(
                "Usage: embedded_runtime pack DIRECTORY ARCHIVE | probe ARCHIVE AUDIO".into(),
            );
        }
    }
    Ok(())
}
