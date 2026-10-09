use super::PcmComparison;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

pub(super) fn compare(
    source: &Path,
    decoded: &Path,
    frame: u32,
    max_frames: u32,
) -> Result<PcmComparison, String> {
    if max_frames > 0 && frame == 0 {
        return Err("PCM comparison failed: -4".into());
    }
    let left = File::open(source).map_err(|_| "PCM comparison failed: -2".to_owned())?;
    let right = File::open(decoded).map_err(|_| "PCM comparison failed: -2".to_owned())?;
    let source_bytes = left
        .metadata()
        .map_err(|_| "PCM comparison failed: -2".to_owned())?
        .len();
    let decoded_bytes = right
        .metadata()
        .map_err(|_| "PCM comparison failed: -2".to_owned())?
        .len();
    let mut result = PcmComparison {
        matches: false,
        reason_code: 0,
        source_bytes,
        decoded_bytes,
        trailing_zero_bytes: 0,
        first_mismatch_offset: 0,
    };
    let extra = decoded_bytes.checked_sub(source_bytes);
    let padding = max_frames > 0
        && extra.is_some_and(|n| {
            n > 0
                && source_bytes % u64::from(frame) == 0
                && n % u64::from(frame) == 0
                && n <= u64::from(frame) * u64::from(max_frames)
        });
    if source_bytes != decoded_bytes && !padding {
        result.reason_code = 1;
        return Ok(result);
    }
    let mut left = BufReader::with_capacity(128 * 1024, left);
    let mut right = BufReader::with_capacity(128 * 1024, right);
    let mut lhs = [0u8; 128 * 1024];
    let mut rhs = [0u8; 128 * 1024];
    let mut offset = 0;
    while offset < source_bytes {
        let n = (source_bytes - offset).min(lhs.len() as u64) as usize;
        if left.read_exact(&mut lhs[..n]).is_err() || right.read_exact(&mut rhs[..n]).is_err() {
            result.reason_code = 4;
            result.first_mismatch_offset = offset;
            return Ok(result);
        }
        if let Some(index) = lhs[..n].iter().zip(&rhs[..n]).position(|(a, b)| a != b) {
            result.reason_code = 2;
            result.first_mismatch_offset = offset + index as u64;
            return Ok(result);
        }
        offset += n as u64;
    }
    if padding {
        let extra = extra.unwrap();
        let mut remaining = extra;
        while remaining > 0 {
            let n = remaining.min(rhs.len() as u64) as usize;
            if right.read_exact(&mut rhs[..n]).is_err() {
                result.reason_code = 4;
                result.first_mismatch_offset = source_bytes + extra - remaining;
                return Ok(result);
            }
            if let Some(index) = rhs[..n].iter().position(|b| *b != 0) {
                result.reason_code = 3;
                result.first_mismatch_offset = source_bytes + extra - remaining + index as u64;
                return Ok(result);
            }
            remaining -= n as u64;
        }
        result.trailing_zero_bytes = extra;
    }
    result.matches = true;
    Ok(result)
}
