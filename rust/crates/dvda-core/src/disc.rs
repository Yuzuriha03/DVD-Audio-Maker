//! DVD-Audio disc splitting and author audio-group planning.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Track {
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub track: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub sample_rate: i32,
    #[serde(default)]
    pub bits: i32,
    #[serde(default)]
    pub source_path: String,
    #[serde(default)]
    pub manifest_name: String,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub resample_to: Option<i32>,
    #[serde(default)]
    pub source_size: i64,
    #[serde(default)]
    pub mlp_path: String,
    #[serde(default)]
    pub mlp_size: i64,
    #[serde(default)]
    pub channels: Option<i32>,
    #[serde(default)]
    pub channel_mask: Option<u32>,
    #[serde(default)]
    pub source_sample_rate: Option<i32>,
    #[serde(default)]
    pub source_bits: Option<i32>,
    #[serde(default = "default_source")]
    pub mlp_source: String,
    #[serde(default)]
    pub external_resampled: bool,
    #[serde(default)]
    pub external_rebitded: bool,
    #[serde(default)]
    pub parameters_changed: bool,
}
fn default_source() -> String {
    "surcode-batch".into()
}
type GroupKey = (i32, i32, String, i32, u32);
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Album {
    pub name: String,
    pub tracks: Vec<Track>,
    pub mlp_bytes: i64,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Group {
    pub number: usize,
    pub sample_rate: i32,
    pub bits: i32,
    pub tracks: Vec<Track>,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Disc {
    pub number: usize,
    pub albums: Vec<Album>,
    pub groups: Vec<Group>,
    pub tracks: Vec<Track>,
    pub mlp_bytes: i64,
    pub estimated_aob_bytes: i64,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Diagnostic {
    pub severity: i32,
    pub code: String,
    pub message: String,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Plan {
    pub tracks: Vec<Track>,
    pub discs: Vec<Disc>,
    pub diagnostics: Vec<Diagnostic>,
    pub disc_content_limit_bytes: i64,
    pub has_errors: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Request {
    tracks: Vec<Track>,
    disc_bytes: i64,
    max_discs: i32,
    group_track_limit: i32,
}
fn estimate(bytes: i64) -> Result<i64, String> {
    let value = (bytes as f64 * 1.025).ceil();
    if !value.is_finite() || value >= 9223372036854775808.0 {
        return Err("AOB size overflow".into());
    }
    Ok(value as i64)
}
fn diagnostic(list: &mut Vec<Diagnostic>, severity: i32, code: &str, message: String) {
    list.push(Diagnostic {
        severity,
        code: code.into(),
        message,
    });
}
pub fn plan(
    tracks: Vec<Track>,
    disc_bytes: i64,
    max_discs: i32,
    group_limit: i32,
) -> Result<Plan, String> {
    let mut diagnostics = Vec::new();
    for track in &tracks {
        if track.mlp_size <= 0 {
            diagnostic(
                &mut diagnostics,
                2,
                "MLP_MISSING",
                format!(
                    "找不到或无法读取 MLP: {}（{}）",
                    track.mlp_path, track.title
                ),
            );
        }
    }
    let limit = disc_bytes.saturating_sub(8 * 1024 * 1024).max(0);
    let mut albums: Vec<(String, Vec<Track>, i64)> = Vec::new();
    for track in &tracks {
        if let Some((_, members, size)) =
            albums.iter_mut().find(|(name, _, _)| name == &track.album)
        {
            *size = size
                .checked_add(track.mlp_size)
                .ok_or("MLP size overflow")?;
            members.push(track.clone());
        } else {
            albums.push((track.album.clone(), vec![track.clone()], track.mlp_size));
        }
    }
    let mut disc_albums: Vec<Vec<(String, Vec<Track>, i64)>> = Vec::new();
    let mut current = Vec::new();
    let mut current_size = 0i64;
    for album in albums {
        let combined = current_size
            .checked_add(album.2)
            .ok_or("MLP size overflow")?;
        if !current.is_empty() && estimate(combined)? > limit {
            disc_albums.push(std::mem::take(&mut current));
            current_size = 0;
        }
        current_size = current_size
            .checked_add(album.2)
            .ok_or("MLP size overflow")?;
        current.push(album);
    }
    if !current.is_empty() {
        disc_albums.push(current);
    }
    let mut discs = Vec::new();
    for (disc_i, items) in disc_albums.into_iter().enumerate() {
        let total = items.iter().try_fold(0i64, |sum, (_, _, size)| {
            sum.checked_add(*size).ok_or("MLP size overflow")
        })?;
        let estimated = estimate(total)?;
        if items.len() == 1 && estimated > limit {
            diagnostic(
                &mut diagnostics,
                1,
                "ALBUM_EXCEEDS_DISC",
                format!(
                    "第 {} 盘仅含专辑“{}”，估算 AOB 已超过容量上限；专辑不会被拆分。",
                    disc_i + 1,
                    items[0].0
                ),
            );
        }
        let mut bucket: Vec<(GroupKey, Vec<Track>)> = Vec::new();
        for (_, members, _) in &items {
            for track in members {
                let is_lpcm = track.mlp_source == "lpcm";
                let key = (
                    track.sample_rate,
                    track.bits,
                    if is_lpcm { "lpcm" } else { "mlp" }.into(),
                    if is_lpcm {
                        track.channels.unwrap_or(0)
                    } else {
                        0
                    },
                    if is_lpcm {
                        track.channel_mask.unwrap_or(0)
                    } else {
                        0
                    },
                );
                if let Some((_, list)) = bucket.iter_mut().find(|(candidate, _)| *candidate == key)
                {
                    list.push(track.clone());
                } else {
                    bucket.push((key, vec![track.clone()]));
                }
            }
        }
        let mut groups = Vec::new();
        for (key, members) in bucket {
            let mut chunks: Vec<Vec<Track>> = Vec::new();
            let mut chunk: Vec<Track> = Vec::new();
            for track in members {
                if !chunk.is_empty()
                    && track.album != chunk.last().unwrap().album
                    && chunk.len() as i32 >= group_limit
                {
                    chunks.push(std::mem::take(&mut chunk));
                }
                chunk.push(track);
            }
            if !chunk.is_empty() {
                chunks.push(chunk);
            }
            for chunk in chunks {
                if chunk.len() as i32 > group_limit {
                    diagnostic(
                        &mut diagnostics,
                        1,
                        "ALBUM_EXCEEDS_GROUP_LIMIT",
                        format!(
                            "专辑“{}”在 {}Hz/{}bit 组中有 {} 轨，超过组轨上限 {}；为保持专辑完整未拆分。",
                            chunk[0].album,
                            key.0,
                            key.1,
                            chunk.len(),
                            group_limit
                        ),
                    );
                }
                groups.push(Group {
                    number: groups.len() + 1,
                    sample_rate: key.0,
                    bits: key.1,
                    tracks: chunk,
                });
            }
        }
        let tracks_on_disc: Vec<_> = items
            .iter()
            .flat_map(|(_, tracks, _)| tracks.iter().cloned())
            .collect();
        discs.push(Disc {
            number: disc_i + 1,
            albums: items
                .into_iter()
                .map(|(name, tracks, mlp_bytes)| Album {
                    name,
                    tracks,
                    mlp_bytes,
                })
                .collect(),
            groups,
            tracks: tracks_on_disc,
            mlp_bytes: total,
            estimated_aob_bytes: estimated,
        });
    }
    if max_discs > 0 && discs.len() as i32 > max_discs {
        diagnostic(
            &mut diagnostics,
            1,
            "DISC_COUNT_EXCEEDED",
            format!(
                "按容量需 {} 张盘，超出期望的 {} 张。",
                discs.len(),
                max_discs
            ),
        );
    }
    let has_errors = diagnostics.iter().any(|item| item.severity == 2);
    Ok(Plan {
        tracks,
        discs,
        diagnostics,
        disc_content_limit_bytes: limit,
        has_errors,
    })
}
pub fn dispatch(request: Value) -> Result<Value, String> {
    let request: Request = serde_json::from_value(request).map_err(|e| e.to_string())?;
    serde_json::to_value(plan(
        request.tracks,
        request.disc_bytes,
        request.max_discs,
        request.group_track_limit,
    )?)
    .map_err(|e| e.to_string())
}

pub fn limit_albums(request: Value) -> Result<Value, String> {
    let object = request.as_object().ok_or("Expected album limit request")?;
    let tracks: Vec<Track> = serde_json::from_value(
        object
            .get("Tracks")
            .cloned()
            .ok_or("Missing build tracks")?,
    )
    .map_err(|error| format!("Invalid build tracks: {error}"))?;
    let Some(limit) = object.get("AlbumLimit").filter(|value| !value.is_null()) else {
        return serde_json::to_value(tracks).map_err(|error| error.to_string());
    };
    let limit = limit
        .as_i64()
        .ok_or("Album limit must be an integer")?
        .max(0) as usize;
    let mut seen = HashSet::new();
    let retained: HashSet<String> = tracks
        .iter()
        .map(|track| track.album.as_str())
        .filter(|album| seen.insert((*album).to_owned()))
        .take(limit)
        .map(str::to_owned)
        .collect();
    let result: Vec<Track> = tracks
        .into_iter()
        .filter(|track| retained.contains(&track.album))
        .collect();
    serde_json::to_value(result).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn track(album: &str, title: &str, mlp: i64) -> Track {
        Track {
            album: album.into(),
            title: title.into(),
            mlp_size: mlp,
            mlp_path: format!("{title}.mlp"),
            sample_rate: 48_000,
            bits: 24,
            track: title.into(),
            ..Track::default()
        }
    }
    #[test]
    fn keeps_album_together_when_capacity_splits() {
        let result = plan(
            vec![
                track("A", "a1", 3 * 1024 * 1024 * 1024),
                track("A", "a2", 1024 * 1024 * 1024),
                track("B", "b1", 1024 * 1024 * 1024),
            ],
            4_707_319_808,
            0,
            70,
        )
        .unwrap();
        assert_eq!(result.discs.len(), 2);
        assert_eq!(result.discs[0].albums.len(), 1);
        assert_eq!(result.discs[0].albums[0].name, "A");
    }
    #[test]
    fn groups_only_at_album_boundaries_and_reports_oversize() {
        let result = plan(
            vec![
                track("A", "a1", 10),
                track("A", "a2", 10),
                track("B", "b1", 10),
                track("B", "b2", 10),
            ],
            i64::MAX / 2,
            0,
            2,
        )
        .unwrap();
        assert_eq!(result.discs[0].groups.len(), 2);
        let oversized = plan(
            vec![
                track("A", "a1", 10),
                track("A", "a2", 10),
                track("A", "a3", 10),
            ],
            i64::MAX / 2,
            0,
            2,
        )
        .unwrap();
        assert!(
            oversized
                .diagnostics
                .iter()
                .any(|item| item.code == "ALBUM_EXCEEDS_GROUP_LIMIT")
        );
    }
    #[test]
    fn rejects_missing_mlp_as_error_and_preserves_lpcm_group_key() {
        let mut missing = track("A", "a", 0);
        missing.mlp_path = "missing".into();
        let mut lpcm = track("A", "b", 10);
        lpcm.mlp_source = "lpcm".into();
        lpcm.channels = Some(2);
        lpcm.channel_mask = Some(3);
        let result = plan(vec![missing, lpcm], i64::MAX / 2, 1, 70).unwrap();
        assert!(result.has_errors);
        assert_eq!(result.discs[0].groups.len(), 2);
    }

    #[test]
    fn album_limit_keeps_input_order_and_whole_albums() {
        let request = json!({
            "AlbumLimit":2,
            "Tracks":[
                {"Album":"B","Title":"B1"},
                {"Album":"A","Title":"A1"},
                {"Album":"B","Title":"B2"},
                {"Album":"C","Title":"C1"}
            ]
        });
        let limited = limit_albums(request).unwrap();
        let tracks: Vec<Track> = serde_json::from_value(limited).unwrap();
        assert_eq!(
            tracks
                .iter()
                .map(|track| track.title.as_str())
                .collect::<Vec<_>>(),
            ["B1", "A1", "B2"]
        );
    }
}
