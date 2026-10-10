//! DVD-Audio authoring and a streaming ISO9660/UDF writer.
pub mod amg;
pub mod aob;
pub mod asvs;
pub mod atsi;
pub mod audio;
pub mod command;
pub mod iso;
pub mod menu;
pub mod samg;
pub mod timestamps;

/// Synchronous author progress and cancellation; no callback is retained.
pub trait Callbacks {
    fn emit(&mut self, stream: i32, text: &str);
    fn cancelled(&mut self) -> bool;
    fn progress(&mut self, _completed: u64, _total: u64) {}
}

#[derive(Default)]
pub struct Silent;
impl Callbacks for Silent {
    fn emit(&mut self, _stream: i32, _text: &str) {}
    fn cancelled(&mut self) -> bool {
        false
    }
}
