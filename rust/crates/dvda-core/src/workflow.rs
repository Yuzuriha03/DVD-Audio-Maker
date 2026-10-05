use serde_json::{Value, json};

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

fn text<'a>(object: &'a Value, key: &str) -> Option<&'a str> {
    value(object, key).and_then(Value::as_str)
}

fn integer(object: &Value, key: &str) -> Option<i64> {
    value(object, key).and_then(Value::as_i64)
}

fn integer_aliases(object: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| integer(object, key))
}

fn identity_equal(left: &Value, right: &Value) -> bool {
    integer(left, "Size") == integer(right, "Size")
        && integer(left, "LastWriteUtcTicks") == integer(right, "LastWriteUtcTicks")
        && text(left, "HeadHash") == text(right, "HeadHash")
        && text(left, "TailHash") == text(right, "TailHash")
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "identity.equal" => identity_equal_operation(request),
        "cache.mlp_match" => mlp_cache_match(request),
        "resume.signature_match" => resume_signature_match(request),
        "prepare.cache_reusable" => prepare_cache_reusable(request),
        "disk.group_requirements" => disk_group_requirements(request),
        "disk.estimate_requirements" => disk_estimate_requirements(request),
        _ => Err(format!("Unsupported Rust workflow operation: {operation}")),
    }
}

fn identity_equal_operation(request: Value) -> Result<Value, String> {
    let left = value(&request, "Left").ok_or("Missing left file identity")?;
    let right = value(&request, "Right").ok_or("Missing right file identity")?;
    Ok(json!(identity_equal(left, right)))
}

fn mlp_cache_match(request: Value) -> Result<Value, String> {
    let entry = value(&request, "Entry").ok_or("Missing MLP cache entry")?;
    let source = value(&request, "Source").ok_or("Missing source identity")?;
    let entry_source = value(entry, "Source");
    let matches = entry_source.is_some_and(|identity| identity_equal(identity, source))
        && text(entry, "Encoder") == text(&request, "Encoder")
        && integer(entry, "Bits") == integer(&request, "Bits")
        && optional_integer(entry, "ResampleTo") == optional_integer(&request, "ResampleTo")
        && integer(entry, "MaxInterval") == integer(&request, "MaxInterval");
    Ok(json!(matches))
}

fn optional_integer(object: &Value, key: &str) -> Option<i64> {
    value(object, key).and_then(|item| if item.is_null() { None } else { item.as_i64() })
}

fn optional_integer_aliases(object: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| optional_integer(object, key))
}

fn resume_signature_match(request: Value) -> Result<Value, String> {
    let entry = value(&request, "Entry").ok_or("Missing resume entry")?;
    Ok(json!(
        text(entry, "Signature") == text(&request, "Signature")
    ))
}

fn prepare_cache_reusable(request: Value) -> Result<Value, String> {
    let entry = value(&request, "Entry").ok_or("Missing preparation cache entry")?;
    let validation = value(entry, "Validation").ok_or("Missing validation facts")?;
    let track = value(&request, "Track").ok_or("Missing audio track")?;
    let decoded = integer_aliases(validation, &["DecodedSamples", "decoded"]).unwrap_or(0);
    let sample_rate = integer_aliases(validation, &["SampleRate", "sr"]);
    let track_sample_rate = integer_aliases(track, &["SampleRate", "sr"]);
    let bits = integer_aliases(validation, &["Bits", "bits"]);
    let track_bits = integer_aliases(track, &["Bits", "bits"]);
    let expected = optional_integer(&request, "Expected");
    let validation_expected =
        optional_integer_aliases(validation, &["ExpectedSamples", "expected"]);
    let resample_to = optional_integer(validation, "ResampleTo");
    let track_resample_to = optional_integer(track, "ResampleTo");
    let repaired_ok = value(&request, "RepairedFileValid")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    Ok(json!(
        decoded > 0
            && sample_rate == track_sample_rate
            && bits == track_bits
            && resample_to == track_resample_to
            && validation_expected == expected
            && repaired_ok
    ))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SpaceItem {
    root: String,
    purpose: String,
    bytes: i64,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct SpaceGroup {
    root: String,
    purpose: String,
    required_bytes: i64,
}

fn disk_group_requirements(request: Value) -> Result<Value, String> {
    let items: Vec<SpaceItem> = serde_json::from_value(request)
        .map_err(|error| format!("Invalid disk space items: {error}"))?;
    serde_json::to_value(group_space_items(items, false)?).map_err(|error| error.to_string())
}

fn group_space_items(items: Vec<SpaceItem>, sort: bool) -> Result<Vec<SpaceGroup>, String> {
    let mut groups: Vec<SpaceGroup> = Vec::new();
    for item in items {
        let Some(group) = groups
            .iter_mut()
            .find(|group| group.root.eq_ignore_ascii_case(&item.root))
        else {
            groups.push(SpaceGroup {
                root: item.root,
                purpose: item.purpose,
                required_bytes: item.bytes,
            });
            continue;
        };
        group.required_bytes = group
            .required_bytes
            .checked_add(item.bytes)
            .ok_or("Disk space requirement overflow")?;
        if !group
            .purpose
            .split(" + ")
            .any(|value| value == item.purpose)
        {
            group.purpose.push_str(" + ");
            group.purpose.push_str(&item.purpose);
        }
    }
    if sort {
        groups.sort_by_key(|group| group.root.to_uppercase());
    }
    Ok(groups)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SpaceEstimateTrack {
    mlp_size: i64,
    source_size: i64,
    duration: f64,
    channels: Option<i32>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SpaceEstimateDisc {
    estimated_aob_bytes: i64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SpaceEstimateRequest {
    mlp_source: String,
    mlp_root: String,
    build_root: String,
    output_root: String,
    final_root: String,
    keep_intermediate: bool,
    menu_enabled: bool,
    sample_rate: i32,
    bits: i32,
    tracks: Vec<SpaceEstimateTrack>,
    discs: Option<Vec<SpaceEstimateDisc>>,
}

fn sum_checked(values: impl IntoIterator<Item = i64>, context: &str) -> Result<i64, String> {
    values.into_iter().try_fold(0_i64, |total, value| {
        total
            .checked_add(value)
            .ok_or_else(|| format!("{context} overflow"))
    })
}

fn estimate_lpcm_track_bytes(
    track: &SpaceEstimateTrack,
    sample_rate: i32,
    bits: i32,
) -> Result<i64, String> {
    let duration = track.duration.max(0.0);
    let bytes = duration
        * f64::from(sample_rate)
        * f64::from(bits / 8)
        * f64::from(track.channels.unwrap_or(6));
    if !bytes.is_finite() || bytes.ceil() > i64::MAX as f64 {
        return Err("LPCM disk space estimate overflow".into());
    }
    (bytes.ceil() as i64)
        .checked_add(128)
        .ok_or_else(|| "LPCM disk space estimate overflow".into())
}

fn disk_estimate_requirements(request: Value) -> Result<Value, String> {
    let request: SpaceEstimateRequest = serde_json::from_value(request)
        .map_err(|error| format!("Invalid disk space estimate request: {error}"))?;
    let mut items = Vec::new();
    if request.mlp_source == "surcode-batch" {
        let missing = sum_checked(
            request
                .tracks
                .iter()
                .filter(|track| track.mlp_size <= 0)
                .map(|track| track.source_size.max(0)),
            "MLP disk space estimate",
        )?;
        if missing > 0 {
            items.push(SpaceItem {
                root: request.mlp_root,
                purpose: "MLP 编码输出".into(),
                bytes: missing,
            });
        }
    }
    if request.mlp_source == "lpcm" && request.discs.is_none() {
        let pcm = sum_checked(
            request
                .tracks
                .iter()
                .map(|track| estimate_lpcm_track_bytes(track, request.sample_rate, request.bits))
                .collect::<Result<Vec<_>, _>>()?,
            "LPCM disk space estimate",
        )?;
        let temporary = pcm
            .checked_mul(3)
            .ok_or("LPCM disk space estimate overflow")?;
        items.push(SpaceItem {
            root: request.build_root,
            purpose: "LPCM 音频缓存与转换临时文件".into(),
            bytes: temporary,
        });
    }
    if let Some(discs) = request.discs.filter(|discs| !discs.is_empty()) {
        let iso = sum_checked(
            discs.into_iter().map(|disc| disc.estimated_aob_bytes),
            "ISO disk space estimate",
        )?;
        let copies = if request.keep_intermediate { 2 } else { 1 };
        let mut intermediate = iso
            .checked_mul(copies)
            .ok_or("ISO disk space estimate overflow")?;
        if request.menu_enabled {
            intermediate = intermediate
                .checked_add(32 * 1024 * 1024)
                .ok_or("ISO disk space estimate overflow")?;
        }
        items.push(SpaceItem {
            root: request.output_root,
            purpose: "author 中间产物与暂存 ISO".into(),
            bytes: intermediate,
        });
        items.push(SpaceItem {
            root: request.final_root,
            purpose: "成品 ISO 集合".into(),
            bytes: iso,
        });
    }
    serde_json::to_value(group_space_items(items, true)?).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_ignores_path_but_compares_content_evidence() {
        let left = json!({
            "Path": "C:/one.flac",
            "Size": 12,
            "LastWriteUtcTicks": 99,
            "HeadHash": "ABCD",
            "TailHash": "EFGH"
        });
        let right = json!({
            "Path": "D:/two.flac",
            "Size": 12,
            "LastWriteUtcTicks": 99,
            "HeadHash": "ABCD",
            "TailHash": "EFGH"
        });
        assert!(identity_equal(&left, &right));
    }

    #[test]
    fn cache_match_requires_encoder_and_all_parameters() {
        let request = json!({
            "Entry": {
                "Source": {"Size": 1, "LastWriteUtcTicks": 2, "HeadHash": "A", "TailHash": "B"},
                "Encoder": "encoder|1",
                "Bits": 24,
                "ResampleTo": null,
                "MaxInterval": 8
            },
            "Source": {"Size": 1, "LastWriteUtcTicks": 2, "HeadHash": "A", "TailHash": "B"},
            "Encoder": "encoder|1",
            "Bits": 24,
            "ResampleTo": null,
            "MaxInterval": 8
        });
        assert_eq!(mlp_cache_match(request).unwrap(), json!(true));
    }

    #[test]
    fn disk_groups_are_case_insensitive_and_stable() {
        let result = disk_group_requirements(json!([
            {"Root":"Z:/", "Purpose":"final", "Bytes":2},
            {"Root":"c:/", "Purpose":"build", "Bytes":3},
            {"Root":"z:/", "Purpose":"build", "Bytes":4}
        ]))
        .unwrap();
        assert_eq!(result[0]["Root"], "Z:/");
        assert_eq!(result[0]["RequiredBytes"], 6);
        assert_eq!(result[0]["Purpose"], "final + build");
        assert_eq!(result[1]["Root"], "c:/");
    }

    #[test]
    fn disk_estimate_combines_source_space_and_disc_outputs() {
        let request = json!({
            "MlpSource":"surcode-batch",
            "MlpRoot":"C:\\",
            "BuildRoot":"C:\\",
            "OutputRoot":"C:\\",
            "FinalRoot":"c:\\",
            "KeepIntermediate":true,
            "MenuEnabled":true,
            "SampleRate":48000,
            "Bits":24,
            "Tracks":[
                {"MlpSize":0,"SourceSize":5000,"Duration":1.0,"Channels":2},
                {"MlpSize":4096,"SourceSize":1000,"Duration":1.0,"Channels":2}
            ],
            "Discs":[{"EstimatedAobBytes":1000000},{"EstimatedAobBytes":2000000}]
        });
        let groups = crate::dispatch("disk.estimate_requirements", request).unwrap();
        assert_eq!(groups.as_array().unwrap().len(), 1);
        assert_eq!(groups[0]["Root"], "C:\\");
        assert_eq!(
            groups[0]["Purpose"],
            "MLP 编码输出 + author 中间产物与暂存 ISO + 成品 ISO 集合"
        );
        assert_eq!(groups[0]["RequiredBytes"], 42_559_432);
    }

    #[test]
    fn disk_estimate_reserves_lpcm_cache_when_disc_plan_is_absent() {
        let request = json!({
            "MlpSource":"lpcm",
            "MlpRoot":"?",
            "BuildRoot":"D:\\",
            "OutputRoot":"?",
            "FinalRoot":"?",
            "KeepIntermediate":false,
            "MenuEnabled":false,
            "SampleRate":48000,
            "Bits":24,
            "Tracks":[{"MlpSize":0,"SourceSize":0,"Duration":1.0,"Channels":2}],
            "Discs":null
        });
        let groups = disk_estimate_requirements(request).unwrap();
        assert_eq!(groups[0]["RequiredBytes"], 864_384);
        assert_eq!(groups[0]["Purpose"], "LPCM 音频缓存与转换临时文件");
    }
}
