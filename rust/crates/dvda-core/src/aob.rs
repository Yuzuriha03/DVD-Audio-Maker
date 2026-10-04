use serde_json::Value;
use std::{io::Read, sync::OnceLock};

pub const SECTOR_SIZE: usize = 2048;
pub const MISSING_PTS: i64 = -1;
pub const SHORT_SECTOR: i64 = -2;
static FORMATS: OnceLock<Result<dvda_native::NativeFormats, String>> = OnceLock::new();

fn sector_pts_with(
    sector: &[u8],
    require_pack: bool,
    parse: impl FnOnce(&[u8]) -> Result<i64, String>,
) -> Result<i64, String> {
    if sector.len() < 64 {
        return Ok(SHORT_SECTOR);
    }
    if require_pack && sector[..4] != [0, 0, 1, 0xba] {
        return Ok(MISSING_PTS);
    }
    let Some(relative) = sector[4..64].windows(4).position(|s| s == [0, 0, 1, 0xbd]) else {
        return Ok(MISSING_PTS);
    };
    let marker = relative + 4;
    if marker + 14 > sector.len() || sector[marker + 7] & 0x80 == 0 {
        return Ok(MISSING_PTS);
    }
    parse(&sector[marker + 9..marker + 14])
}

pub fn sector_pts(sector: &[u8], require_pack: bool) -> Result<i64, String> {
    sector_pts_with(sector, require_pack, |pts| {
        FORMATS
            .get_or_init(dvda_native::NativeFormats::load)
            .as_ref()
            .map_err(Clone::clone)?
            .parse_pts(pts)
    })
}

/// The caller supplies whole records; partial AOB tails are excluded by its
/// chunk iterator. Audit records may be shorter than the 2048-byte DVD sector.
pub fn scan_pts(
    data: &[u8],
    stride: usize,
    require_pack: bool,
    output: &mut [i64],
) -> Result<(), String> {
    if stride == 0 || !data.len().is_multiple_of(stride) || output.len() != data.len() / stride {
        return Err("Invalid AOB record size or output capacity".into());
    }
    for (sector, result) in data.chunks_exact(stride).zip(output.iter_mut()) {
        *result = sector_pts(sector, require_pack)?;
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct FileScan {
    sector_count: i32,
    values: Vec<i64>,
}

fn scan_file(path: &str, maximum: Option<i32>) -> Result<FileScan, ScanError> {
    let mut file = crate::identity::open_read(path).map_err(ScanError::Io)?;
    let mut buffer = vec![0u8; 128 * 1024];
    let mut sector_count = 0i32;
    let mut values = Vec::new();
    loop {
        let limit = maximum.filter(|n| *n > 0).unwrap_or(i32::MAX);
        let request = buffer
            .len()
            .min((limit - sector_count) as usize * SECTOR_SIZE);
        if request == 0 {
            break;
        }
        let mut count = 0;
        while count < request {
            match file.read(&mut buffer[count..request]) {
                Ok(0) => break,
                Ok(n) => count += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(ScanError::Io(e)),
            }
        }
        for sector in buffer[..count].as_chunks::<SECTOR_SIZE>().0 {
            let pts = sector_pts(sector, true).map_err(ScanError::Native)?;
            sector_count += 1;
            if pts >= 0 {
                values.push(pts);
            }
        }
        if count < request {
            break;
        }
    }
    Ok(FileScan {
        sector_count,
        values,
    })
}

enum ScanError {
    Io(std::io::Error),
    Native(String),
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuditState {
    pub previous: i64,
    pub sector_count: i32,
    pub missing_sector: i32,
}

impl Default for AuditState {
    fn default() -> Self {
        Self {
            previous: -1,
            sector_count: 0,
            missing_sector: -1,
        }
    }
}

impl AuditState {
    pub fn observe(&mut self, data: &[u8]) -> Result<i32, String> {
        if self.missing_sector >= 0 {
            return Ok(-1);
        }
        let timestamp = sector_pts(data, false)?;
        self.observe_timestamp(timestamp)
    }

    fn observe_timestamp(&mut self, timestamp: i64) -> Result<i32, String> {
        if self.missing_sector >= 0 {
            return Ok(-1);
        }
        let index = self.sector_count;
        self.sector_count = index.checked_add(1).ok_or("AOB sector count overflow")?;
        match timestamp {
            SHORT_SECTOR => Ok(-1),
            MISSING_PTS => {
                self.missing_sector = index;
                Ok(-1)
            }
            current => {
                let drop = if self.previous >= 0 && current < self.previous {
                    index
                } else {
                    -1
                };
                self.previous = current;
                Ok(drop)
            }
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuditRow {
    title: i32,
    first: i32,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuditRequest {
    drops: Vec<i32>,
    missing_sector: i32,
    sector_count: i32,
    rows: Vec<AuditRow>,
}

fn audit_diagnostics(request: Value) -> Result<Value, String> {
    let request: AuditRequest = serde_json::from_value(request).map_err(|e| e.to_string())?;
    let boundaries: std::collections::HashSet<_> = request.rows.iter().map(|r| r.first).collect();
    let drops: std::collections::HashSet<_> = request.drops.iter().copied().collect();
    let mut issues = Vec::new();
    for index in &request.drops {
        if !boundaries.contains(index) {
            issues.push(serde_json::json!({"Code": "PTS_DROP_OFF_BOUNDARY", "Sector": index, "Title": null}));
        }
    }
    if request.missing_sector >= 0 {
        issues.push(serde_json::json!({"Code": "PTS_MISSING", "Sector": null, "Title": null}));
    }
    let mut previous_title = -1;
    for row in request.rows {
        if row.title == previous_title {
            continue;
        }
        if row.first != 0 && row.first < request.sector_count && !drops.contains(&row.first) {
            issues.push(serde_json::json!({"Code": "PTS_RESET_MISSING", "Sector": row.first, "Title": row.title}));
        }
        previous_title = row.title;
    }
    Ok(Value::Array(issues))
}

fn value<'a>(object: &'a Value, key: &str) -> Option<&'a Value> {
    object.as_object().and_then(|map| {
        map.get(key)
            .or_else(|| map.get(&key.to_ascii_lowercase()))
            .or_else(|| map.get(snake_case(key).as_str()))
    })
}

fn snake_case(key: &str) -> String {
    let mut result = String::with_capacity(key.len() + 4);
    for (index, character) in key.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                result.push('_');
            }
            result.push(character.to_ascii_lowercase());
        } else {
            result.push(character);
        }
    }
    result
}

#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PtsStatistics {
    pub first_pts: Option<i64>,
    pub last_pts: Option<i64>,
    pub minimum_step: i64,
    pub maximum_step: i64,
    pub negative_steps: i32,
    pub zero_steps: i32,
    pub median_step: f64,
    pub abnormal_steps: i32,
    pub abnormal_ratio: f64,
    pub steps: Vec<i64>,
    pub issue_codes: Vec<String>,
}

pub fn pts_statistics(values: &[i64]) -> PtsStatistics {
    let first_pts = values.first().copied();
    let last_pts = values.last().copied();
    if values.len() < 3 {
        return PtsStatistics {
            first_pts,
            last_pts,
            minimum_step: 0,
            maximum_step: 0,
            negative_steps: 0,
            zero_steps: 0,
            median_step: 0.0,
            abnormal_steps: 0,
            abnormal_ratio: 0.0,
            steps: Vec::new(),
            issue_codes: vec!["PTS_TOO_FEW".to_string()],
        };
    }

    let steps: Vec<i64> = values.windows(2).map(|pair| pair[1] - pair[0]).collect();
    let mut ordered = steps.clone();
    ordered.sort_unstable();
    let middle = ordered.len() / 2;
    let median_step = if ordered.len() % 2 == 1 {
        ordered[middle] as f64
    } else {
        (ordered[middle - 1] as f64 + ordered[middle] as f64) / 2.0
    };
    let abnormal_steps = steps
        .iter()
        .filter(|&&step| step <= 0 || (step as f64) > median_step * 20.0)
        .count() as i32;
    let abnormal_ratio = abnormal_steps as f64 * 100.0 / steps.len() as f64;
    let mut issue_codes = Vec::new();
    if first_pts == last_pts {
        issue_codes.push("PTS_NOT_ADVANCING".to_string());
    }
    if abnormal_ratio > 1.0 {
        issue_codes.push("PTS_ABNORMAL_RATIO".to_string());
    }
    PtsStatistics {
        first_pts,
        last_pts,
        minimum_step: *steps.iter().min().expect("non-empty steps"),
        maximum_step: *steps.iter().max().expect("non-empty steps"),
        negative_steps: steps.iter().filter(|&&step| step < 0).count() as i32,
        zero_steps: steps.iter().filter(|&&step| step == 0).count() as i32,
        median_step,
        abnormal_steps,
        abnormal_ratio,
        steps,
        issue_codes,
    }
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "aob.audit_diagnostics" => audit_diagnostics(request),
        "aob.scan_file" => {
            let path = value(&request, "Path")
                .and_then(Value::as_str)
                .ok_or("Missing AOB path")?;
            let maximum = value(&request, "MaximumSectors")
                .filter(|v| !v.is_null())
                .map(|v| {
                    v.as_i64()
                        .and_then(|v| i32::try_from(v).ok())
                        .ok_or("Invalid sector limit")
                })
                .transpose()?;
            match scan_file(path, maximum) {
                Ok(scan) => Ok(serde_json::json!({"Scan": scan, "ErrorCode": null})),
                Err(ScanError::Io(error)) => Ok(
                    serde_json::json!({"Scan": null, "ErrorCode": error.raw_os_error().unwrap_or(87)}),
                ),
                Err(ScanError::Native(error)) => Err(error),
            }
        }
        "aob.pts_statistics" => {
            let values = value(&request, "Values")
                .and_then(Value::as_array)
                .ok_or("Missing AOB PTS values")?
                .iter()
                .map(|item| item.as_i64().ok_or("AOB PTS value must be an integer"))
                .collect::<Result<Vec<_>, _>>()?;
            serde_json::to_value(pts_statistics(&values)).map_err(|error| error.to_string())
        }
        _ => Err(format!("Unsupported Rust AOB operation: {operation}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn audit_freezes_on_missing_but_counts_short_records() {
        let mut state = AuditState::default();
        assert_eq!(std::mem::size_of::<AuditState>(), 16);
        for pts in [300, SHORT_SECTOR, 200] {
            let drop = state.observe_timestamp(pts).unwrap();
            assert_eq!(drop, if pts == 200 { 2 } else { -1 });
        }
        assert_eq!(state.sector_count, 3);
        assert_eq!(state.previous, 200);
        state.observe_timestamp(MISSING_PTS).unwrap();
        state.observe_timestamp(0).unwrap();
        assert_eq!(state.sector_count, 4);
        assert_eq!(state.missing_sector, 3);
        assert_eq!(state.previous, 200);
    }

    #[test]
    fn sector_search_preserves_audit_and_timeline_boundaries() {
        for length in [0, 1, 63] {
            assert_eq!(
                sector_pts_with(&vec![0; length], false, |_| panic!("short")).unwrap(),
                SHORT_SECTOR
            );
        }
        let mut sector = [0; SECTOR_SIZE];
        sector[60..64].copy_from_slice(&[0, 0, 1, 0xbd]);
        sector[67] = 0x80;
        assert_eq!(sector_pts_with(&sector, false, |_| Ok(17)).unwrap(), 17);
        assert_eq!(
            sector_pts_with(&sector[..73], false, |_| panic!("truncated")).unwrap(),
            MISSING_PTS
        );
        assert_eq!(
            sector_pts_with(&sector, true, |_| panic!("pack header")).unwrap(),
            MISSING_PTS
        );
        sector[..4].copy_from_slice(&[0, 0, 1, 0xba]);
        assert_eq!(sector_pts_with(&sector, true, |_| Ok(17)).unwrap(), 17);
        sector[67] = 0;
        assert_eq!(
            sector_pts_with(&sector, true, |_| panic!("flag")).unwrap(),
            MISSING_PTS
        );
    }

    #[test]
    fn computes_steps_and_abnormal_ratio() {
        let result = pts_statistics(&[100, 200, 300, 300]);
        assert_eq!(result.first_pts, Some(100));
        assert_eq!(result.last_pts, Some(300));
        assert_eq!(result.steps, vec![100, 100, 0]);
        assert_eq!(result.minimum_step, 0);
        assert_eq!(result.maximum_step, 100);
        assert_eq!(result.zero_steps, 1);
        assert_eq!(result.abnormal_steps, 1);
        assert!(
            result
                .issue_codes
                .contains(&"PTS_ABNORMAL_RATIO".to_string())
        );
    }

    #[test]
    fn reports_too_few_pts() {
        let result = pts_statistics(&[100, 200]);
        assert_eq!(result.issue_codes, vec!["PTS_TOO_FEW"]);
        assert!(result.steps.is_empty());
    }

    #[test]
    fn dispatch_rejects_non_integer_values() {
        let result = dispatch("aob.pts_statistics", json!({"Values": [1, "2", 3]}));
        assert!(result.is_err());
    }
}
