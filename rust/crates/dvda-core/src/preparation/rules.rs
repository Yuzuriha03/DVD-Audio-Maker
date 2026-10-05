use super::models::{Issue, Patch};
use crate::audio::{DecodeResult, Metadata};
use std::collections::{BTreeMap, HashMap};

/// Count ties keep the first occurrence, as does the original stable GroupBy.
pub fn normalize_albums(tracks: &mut [Metadata]) {
    let mut albums: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, track) in tracks.iter().enumerate() {
        let name = if track.album.is_empty() {
            &track.title
        } else {
            &track.album
        };
        albums.entry(name.clone()).or_default().push(index);
    }
    for indexes in albums.values() {
        let mut rates: Vec<(i32, usize)> = Vec::new();
        for index in indexes {
            count(&mut rates, tracks[*index].sample_rate);
        }
        let rate = majority(&rates);
        let mut bits: Vec<(i32, usize)> = Vec::new();
        for index in indexes {
            if tracks[*index].sample_rate == rate {
                count(&mut bits, tracks[*index].bits);
            }
        }
        let bits = majority(&bits);
        for index in indexes {
            let track = &mut tracks[*index];
            if (track.sample_rate, track.bits) != (rate, bits) {
                track.sample_rate = rate;
                track.bits = bits;
                track.resample_to = Some(rate);
            }
        }
    }
}
fn count(counts: &mut Vec<(i32, usize)>, value: i32) {
    if let Some((_, n)) = counts.iter_mut().find(|(v, _)| *v == value) {
        *n += 1;
    } else {
        counts.push((value, 1));
    }
}
fn majority(counts: &[(i32, usize)]) -> i32 {
    let mut best = counts[0];
    for &(value, count) in &counts[1..] {
        if count > best.1 {
            best = (value, count);
        }
    }
    best.0
}
pub fn validate_channels(tracks: &[Metadata], source: &str, lpcm: bool) -> Vec<Issue> {
    if lpcm {
        return Vec::new();
    }
    let mut groups: Vec<((i32, i32), Vec<&Metadata>)> = Vec::new();
    for track in tracks {
        let key = (track.sample_rate, track.bits);
        if let Some((_, group)) = groups.iter_mut().find(|(k, _)| *k == key) {
            group.push(track);
        } else {
            groups.push((key, vec![track]));
        }
    }
    let mut issues = Vec::new();
    for ((rate, bits), group) in groups {
        let mut counts = BTreeMap::new();
        for track in &group {
            *counts.entry(track.channels).or_insert(0usize) += 1;
        }
        if counts.len() > 1 {
            issues.push(Issue {
                level: "FAIL".into(),
                title: format!("音频组 {rate}Hz/{bits}bit 声道数不一致"),
                path: source.into(),
                reason: counts
                    .iter()
                    .map(|(ch, n)| format!("{ch} 声道 × {n} 首"))
                    .collect::<Vec<_>>()
                    .join("; "),
                detail: group
                    .iter()
                    .take(10)
                    .map(|track| format!("     {}", track.title))
                    .collect(),
            });
        }
    }
    issues
}
pub fn expected_samples(track: &Metadata) -> Option<i64> {
    (track.duration > 0.0)
        .then(|| (track.duration * f64::from(track.sample_rate)).round_ties_even() as i64)
}
pub fn evaluate(
    track: &Metadata,
    path: &str,
    check: &DecodeResult,
    expected: Option<i64>,
    repaired: usize,
    error_seconds: f64,
    warning_seconds: f64,
) -> Option<Issue> {
    let mut level = None;
    let mut reasons = Vec::new();
    let mut detail = Vec::new();
    if repaired > 0 {
        detail.push(format!(
            "已修复 ALAC END 标记 {repaired} 处（原文件未改动）"
        ));
    }
    if check.error_count > 0 {
        level = Some("FAIL");
        reasons.push(format!("解码报错 {} 处", check.error_count));
        detail.extend(check.error_lines.iter().cloned());
    }
    if let Some(samples) = check.samples {
        let loss = expected.map(|value| {
            value.wrapping_sub(samples) as f64 / f64::from(track.sample_rate) * 1000.0
        });
        let direction = if loss.is_some_and(|v| v > 0.0) {
            "少"
        } else {
            "多"
        };
        detail.push(format!(
            "源声明 {:.3} 秒 -> 期望 {} 采样 / 实解 {samples} 采样 / {}",
            track.duration,
            expected.map_or_else(|| "?".into(), |v| v.to_string()),
            loss.map_or_else(|| "?".into(), |v| format!("{direction} {:.0} ms", v.abs()))
        ));
        if let Some(loss) = loss {
            if loss.abs() > error_seconds * 1000.0 {
                level = Some("FAIL");
                reasons.push(format!("解码采样数{direction} {:.0} ms", loss.abs()));
            } else if level.is_none() && loss.abs() > warning_seconds * 1000.0 {
                level = Some("WARN");
                reasons.push(format!("解码采样数{direction} {:.0} ms", loss.abs()));
            }
        }
    } else {
        level = Some("FAIL");
        reasons.push("未能读到解码采样数(astats 无输出)".into());
        detail.push("ffmpeg 未输出 'Number of samples',无法校验完整性".into());
    }
    level.map(|level| Issue {
        level: level.into(),
        title: track.title.clone(),
        path: path.into(),
        reason: reasons.join("; "),
        detail,
    })
}
pub fn repair_detail(patch: &Patch) -> String {
    let minutes = (patch.presentation_time / 60.0) as i32;
    let seconds = patch.presentation_time - f64::from(minutes) * 60.0;
    format!(
        "{:10.3}s ({minutes}分{seconds:05.2}秒)  标记 {:03b} -> 111  ({} 采样)",
        patch.presentation_time, patch.previous_bits, patch.sample_count
    )
}
pub fn ordered_groups(tracks: &[Metadata]) -> BTreeMap<(i32, i32), Vec<&Metadata>> {
    let mut groups: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for track in tracks {
        groups
            .entry((track.sample_rate, track.bits))
            .or_default()
            .push(track);
    }
    for group in groups.values_mut() {
        group.sort_by(|a, b| {
            a.date
                .encode_utf16()
                .cmp(b.date.encode_utf16())
                .then(
                    crate::planner::track_number(&a.track)
                        .cmp(&crate::planner::track_number(&b.track)),
                )
                .then(a.title.encode_utf16().cmp(b.title.encode_utf16()))
        });
    }
    groups
}
