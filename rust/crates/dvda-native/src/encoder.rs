//! Shared streaming PCM and metadata types for the linked Rust MLP encoder.
use std::io;

pub struct Stamp {
    pub start: u64,
    pub packet: Vec<u8>,
    pub valid_bits: u32,
}
pub struct Request {
    pub sample_rate: u32,
    pub bits: u32,
    pub channels: u32,
    pub frames: u64,
    pub assignment: u32,
    pub metadata: Vec<Stamp>,
}
#[derive(Debug)]
pub struct ResultInfo {
    pub status: i32,
    pub access_units: u32,
    pub input_frames: u64,
    pub encoded_frames: u64,
    pub output_bytes: u64,
    pub error: String,
}
/// Both methods run synchronously on the invoking thread, with borrowed buffers.
pub trait Stream {
    /// Returns complete frames, each containing exactly the declared channel count.
    ///
    /// # Errors
    /// Propagates input and short-read failures from the PCM source.
    fn read(&mut self, samples: &mut [i32]) -> io::Result<usize>;
    /// Write one encoded byte sequence synchronously.
    ///
    /// # Errors
    /// Propagates output failures from the destination.
    fn write(&mut self, bytes: &[u8]) -> io::Result<()>;
}
