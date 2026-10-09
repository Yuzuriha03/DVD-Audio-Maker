//! Native Rust DVD-Audio format inspection, alignment, PTS, and PCM comparison.
pub mod compression;
pub mod disc_verify;
pub mod encoder;
pub mod files;
pub mod images;
pub mod media;
mod mlp_formats;
mod pcm_formats;
pub mod process;

#[cfg(test)]
mod format_parity;

use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct MlpInspection {
    pub size: u64,
    pub access_unit_count: u32,
    pub major_sync_count: u32,
    pub major_sync_interval: f64,
    pub major_sync_error_count: i32,
    pub access_unit_parity_error_count: i32,
    pub substream_error_count: i32,
    pub has_end_of_stream: bool,
    pub peak_bitrate_raw: i32,
    pub extended_substream_info: i32,
    pub sample_rate: i32,
    pub is_valid: bool,
    pub error_code: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PcmComparison {
    pub matches: bool,
    pub reason_code: i32,
    pub source_bytes: u64,
    pub decoded_bytes: u64,
    pub trailing_zero_bytes: u64,
    pub first_mismatch_offset: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MlpAlignment {
    pub data: Vec<u8>,
    pub peak_changes: i32,
    pub extended_changes: i32,
    pub checksum_changes: i32,
    pub inserted_end_of_stream: bool,
    pub old_header: i32,
    pub new_header: i32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NativeFormats;

pub fn sample_pts() -> [i64; 4] {
    [0, 1, 90_000, 0x1_FFFF_FFFFi64]
}

impl NativeFormats {
    pub fn load() -> Result<Self, String> {
        Ok(Self)
    }

    pub fn inspect_file(&self, path: &Path) -> Result<MlpInspection, String> {
        mlp_formats::inspect_file(path)
    }

    pub fn compare_pcm(
        &self,
        source: &Path,
        decoded: &Path,
        bytes_per_frame: u32,
        max_zero_frames: u32,
    ) -> Result<PcmComparison, String> {
        pcm_formats::compare(source, decoded, bytes_per_frame, max_zero_frames)
    }

    pub fn parse_pts(&self, data: &[u8]) -> Result<i64, String> {
        if data.len() < 5 {
            return Err("PTS parsing failed: -4".into());
        }
        Ok((i64::from((data[0] >> 1) & 7) << 30)
            | (i64::from(u16::from_be_bytes([data[1], data[2]]) >> 1) << 15)
            | i64::from(u16::from_be_bytes([data[3], data[4]]) >> 1))
    }

    pub fn align(&self, data: &[u8]) -> Result<MlpAlignment, String> {
        mlp_formats::align(data)
    }
}
