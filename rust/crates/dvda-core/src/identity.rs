//! Bounded file fingerprints compatible with the existing preparation cache.
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
};

const SAMPLE_BYTES: usize = 64 * 1024;
const FILETIME_EPOCH_TICKS: u64 = 504_911_232_000_000_000;

/// Opens a file for reading while allowing only concurrent readers.
///
/// # Errors
/// Returns the OS error if opening the file fails, including sharing conflicts.
pub fn open_read(path: &str) -> io::Result<File> {
    // Match File.OpenRead: concurrent readers are allowed, writers/deletion are not.
    OpenOptions::new().read(true).share_mode(1).open(path)
}

/// Computes size, timestamp, and bounded head/tail hashes for a regular file.
///
/// # Errors
/// Returns an error if metadata lookup, opening, seeking, or reading fails,
/// the path is not a regular file, or its timestamp exceeds the supported range.
pub fn compute(path: &str) -> io::Result<Value> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Not a regular file",
        ));
    }
    let size = metadata.file_size();
    let ticks = metadata
        .last_write_time()
        .checked_add(FILETIME_EPOCH_TICKS)
        .filter(|value| i64::try_from(*value).is_ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "File timestamp overflow"))?;
    let mut file = open_read(path)?;
    let mut head = vec![0; SAMPLE_BYTES.min(usize::try_from(size).unwrap_or(usize::MAX))];
    let mut tail = vec![0; head.len()];
    file.read_exact(&mut head)?;
    file.seek(SeekFrom::Start(size.saturating_sub(tail.len() as u64)))?;
    file.read_exact(&mut tail)?;
    Ok(
        json!({"Path": path, "Size": size, "LastWriteUtcTicks": ticks,
        "HeadHash": &crate::hash::hex_digest(&head)[..32],
        "TailHash": &crate::hash::hex_digest(&tail)[..32]}),
    )
}

pub(crate) fn equivalent(left: &Value, right: &Value) -> bool {
    ["Size", "LastWriteUtcTicks", "HeadHash", "TailHash"]
        .iter()
        .all(|key| left.get(*key) == right.get(*key))
}

/// Returns a file identity, or JSON null when the identity cannot be computed.
///
/// # Errors
/// Returns an error if the request is not a string containing a file path.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Public JSON dispatch preserves the owned-request API used by crate::dispatch"
)]
pub fn dispatch(request: Value) -> Result<Value, String> {
    let path = request.as_str().ok_or("Expected identity file path")?;
    Ok(compute(path).unwrap_or(Value::Null))
}
