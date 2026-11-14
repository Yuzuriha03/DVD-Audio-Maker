//! Manifest loading for the build stage.
use crate::disc::Track;
use serde_json::{Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn field<'a>(object: &'a Map<String, Value>, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| {
        object
            .get(*name)
            .or_else(|| object.get(&name.to_ascii_lowercase()))
    })
}
fn text(object: &Map<String, Value>, names: &[&str]) -> String {
    field(object, names)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into()
}
fn integer(object: &Map<String, Value>, names: &[&str]) -> i64 {
    field(object, names).and_then(Value::as_i64).unwrap_or(0)
}
fn optional_integer(object: &Map<String, Value>, names: &[&str]) -> Option<i32> {
    field(object, names).and_then(|value| {
        if value.is_null() {
            None
        } else {
            value.as_i64()?.try_into().ok()
        }
    })
}
fn optional_value(object: &Map<String, Value>, names: &[&str]) -> Option<Value> {
    field(object, names)
        .filter(|value| !value.is_null())
        .cloned()
}
fn number(object: &Map<String, Value>, names: &[&str]) -> f64 {
    field(object, names).and_then(Value::as_f64).unwrap_or(0.)
}
fn path_text(value: &str) -> String {
    #[cfg(windows)]
    {
        value.replace('/', "\\")
    }
    #[cfg(not(windows))]
    {
        value.replace('\\', "/")
    }
}

fn file_size(path: &Path) -> i64 {
    fs::metadata(path).map_or(0, |metadata| {
        i64::try_from(metadata.len()).expect("Windows file sizes fit signed 64-bit offsets")
    })
}
/// Loads manifest tracks and sorts them by date, track number, and UTF-16 title.
/// Unavailable source or MLP file metadata contributes a zero size.
///
/// # Errors
/// Returns an error if the manifest cannot be read as UTF-8 or parsed as a JSON
/// object, a group sample rate or bit depth is outside its destination integer
/// range, or an entry in a group's files array is not an object.
pub fn read(path: &str, mlp_directory: &str) -> Result<Vec<Track>, String> {
    let contents = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let root: Map<String, Value> =
        serde_json::from_str(&contents).map_err(|e| format!("{path}: {e}"))?;
    let mut tracks = Vec::new();
    for group in root.values().filter_map(Value::as_object) {
        let rate = integer(group, &["sr", "SampleRate"])
            .try_into()
            .map_err(|_| "Sample rate overflow")?;
        let bits = integer(group, &["bits", "Bits"])
            .try_into()
            .map_err(|_| "Bit depth overflow")?;
        for item in field(group, &["files", "Files"])
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let Some(item) = item.as_object() else {
                return Err("Manifest track must be an object".into());
            };
            let source = path_text(&text(item, &["src", "Source"]));
            let manifest_name = text(item, &["name", "Name"]);
            let mlp_path =
                PathBuf::from(mlp_directory).join(manifest_name.replace('/', "__") + ".mlp");
            let source_size = file_size(Path::new(&source));
            let mlp_size = file_size(&mlp_path);
            let title = text(item, &["title", "Title"]);
            let album = {
                let value = text(item, &["album", "Album"]);
                if value.is_empty() {
                    title.clone()
                } else {
                    value
                }
            };
            tracks.push(Track {
                date: text(item, &["date", "Date"]),
                track: text(item, &["track", "Track"]),
                title,
                album,
                sample_rate: rate,
                bits,
                source_path: source,
                manifest_name,
                duration: number(item, &["dur", "Duration"]),
                resample_to: optional_integer(item, &["resample_to", "ResampleTo"]),
                source_size,
                mlp_path: mlp_path.to_string_lossy().into_owned(),
                mlp_size,
                channels: optional_integer(item, &["channels", "Channels"]),
                channel_mask: None,
                source_sample_rate: optional_integer(
                    item,
                    &["source_sample_rate", "SourceSampleRate"],
                ),
                source_bits: optional_integer(item, &["source_bits", "SourceBits"]),
                source_identity: optional_value(item, &["source_identity", "SourceIdentity"]),
                prepared_pcm_path: field(item, &["prepared_pcm_path", "PreparedPcmPath"])
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                prepared_pcm_identity: optional_value(
                    item,
                    &["prepared_pcm_identity", "PreparedPcmIdentity"],
                ),
                prepared_pcm_signature: field(
                    item,
                    &["prepared_pcm_signature", "PreparedPcmSignature"],
                )
                .and_then(Value::as_str)
                .map(str::to_owned),
                prepared_mlp_path: field(item, &["prepared_mlp_path", "PreparedMlpPath"])
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                prepared_mlp_identity: optional_value(
                    item,
                    &["prepared_mlp_identity", "PreparedMlpIdentity"],
                ),
                prepared_mlp_signature: field(
                    item,
                    &["prepared_mlp_signature", "PreparedMlpSignature"],
                )
                .and_then(Value::as_str)
                .map(str::to_owned),
                mlp_source: "mlp-batch".into(),
                external_resampled: false,
                external_rebitded: false,
                parameters_changed: false,
            });
        }
    }
    tracks.sort_by(|left, right| {
        left.date
            .encode_utf16()
            .cmp(right.date.encode_utf16())
            .then(
                crate::planner::track_number(&left.track)
                    .cmp(&crate::planner::track_number(&right.track)),
            )
            .then(left.title.encode_utf16().cmp(right.title.encode_utf16()))
    });
    Ok(tracks)
}
/// Loads manifest tracks from an owned JSON request.
///
/// # Errors
/// Rejects non-object requests or missing/non-string `ManifestPath` and
/// `MlpDirectory` fields. Propagates [`read`] errors and track serialization errors.
pub fn dispatch(request: Value) -> Result<Value, String> {
    let Value::Object(object) = request else {
        return Err("Expected manifest request".into());
    };
    let path = field(&object, &["ManifestPath"])
        .and_then(Value::as_str)
        .ok_or("Missing manifest path")?;
    let directory = field(&object, &["MlpDirectory"])
        .and_then(Value::as_str)
        .ok_or("Missing MLP directory")?;
    serde_json::to_value(read(path, directory)?).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    #[test]
    fn reads_manifest_and_sorts_utf16_track_order() {
        let root = std::env::temp_dir().join(format!(
            "dvda-manifest-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("mlp")).unwrap();
        let source = root.join("音频.flac");
        let mlp = root.join("mlp/group__b.mlp");
        fs::write(&source, [1, 2, 3]).unwrap();
        fs::write(&mlp, [0; 4096]).unwrap();
        let manifest = root.join("manifest.json");
        let value = serde_json::json!({"group_48000_24":{"sr":48000,"bits":24,"files":[
            {"n":1,"src":source.to_string_lossy(),"name":"group/b","title":"😀","track":"12","album":"A","dur":1.5},
            {"n":2,"src":source.to_string_lossy(),"name":"group/b","title":"先","track":"2","album":"","dur":1.5},
            {"n":3,"src":source.to_string_lossy(),"name":"group/b","title":"后","track":"1","album":"A","dur":1.5}] }});
        fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
        let tracks = read(
            &manifest.to_string_lossy(),
            &root.join("mlp").to_string_lossy(),
        )
        .unwrap();
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[0].track, "1");
        assert_eq!(tracks[1].track, "2");
        assert_eq!(tracks[0].album, "A");
        assert_eq!(tracks[1].album, "先");
        assert_eq!(tracks[0].source_size, 3);
        assert_eq!(tracks[0].mlp_size, 4096);
        let _ = fs::remove_dir_all(root);
    }
}
