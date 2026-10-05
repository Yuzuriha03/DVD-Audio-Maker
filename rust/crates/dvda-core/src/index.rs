//! MLP acquisition index serialization.
use serde_json::{Map, Value, json};
use std::{fs, path::Path};

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value
        .get(name)
        .or_else(|| value.get(name.to_ascii_lowercase()))
        .unwrap_or(&Value::Null)
}
fn text(value: &Value, name: &str) -> String {
    field(value, name).as_str().unwrap_or_default().into()
}
fn node(value: &Value, name: &str) -> Value {
    field(value, name).clone()
}
fn tracks(plan: &Value) -> Vec<Value> {
    field(plan, "Tracks")
        .as_array()
        .cloned()
        .unwrap_or_default()
}
fn discs(plan: &Value) -> Vec<Value> {
    field(plan, "Discs").as_array().cloned().unwrap_or_default()
}
fn duplicate(paths: &[Value]) -> Option<String> {
    let mut seen: Vec<String> = Vec::new();
    for track in paths {
        let path = text(track, "MlpPath");
        if let Some(old) = seen.iter().find(|old| old.eq_ignore_ascii_case(&path)) {
            return Some(old.clone());
        }
        seen.push(path);
    }
    None
}
fn title_ends(disc: &Value, mode: &str) -> Result<Map<String, Value>, String> {
    let result =
        crate::author::dispatch("author.title_ends", json!({"Disc":disc,"TitleMode":mode}))?;
    result
        .as_object()
        .cloned()
        .ok_or("Invalid title boundary result".into())
}
fn write_index(request: Value) -> Result<Value, String> {
    let path = text(&request, "Path");
    let plan = field(&request, "Plan");
    let all_tracks = tracks(plan);
    if let Some(path) = duplicate(&all_tracks) {
        return Err(format!("多首曲目映射到同一个 MLP，索引会发生覆盖: {path}"));
    }
    let build_directory = text(&request, "BuildDirectory");
    let build_log = text(&request, "BuildLogPath");
    let dry_run = field(&request, "DryRun").as_bool().unwrap_or(false);
    let mlp_source = text(&request, "MlpSource");
    let external = text(&request, "MlpExternalDirectory");
    let title_mode = text(&request, "DiagnosticTitleMode");
    let generated = text(&request, "Generated");
    let build_log = if dry_run {
        Path::new(&build_directory)
            .join("build-dryrun.log")
            .to_string_lossy()
            .into_owned()
    } else {
        build_log
    };
    let mut root = Map::new();
    root.insert(
        "__meta__".into(),
        json!({
            "generated":generated,
            "build_log":build_log,
            "dry_run":dry_run,
            "mlp_source":mlp_source,
            "mlp_external_dir":if external.is_empty(){Value::Null}else{json!(external)},
            "discs":discs(plan).len(),
            "tracks":all_tracks.len(),
            "aob_layout":"第 N 组 -> AUDIO_TS/ATS_NN_1.AOB（超过 1 GiB 后续分段）"
        }),
    );
    let mut disc_nodes = Vec::new();
    let mut ends = Map::new();
    for disc in discs(plan) {
        let disc_number = field(&disc, "Number").as_i64().unwrap_or(0);
        let groups = field(&disc, "Groups")
            .as_array()
            .cloned()
            .unwrap_or_default();
        if let Ok(boundaries) = title_ends(&disc, &title_mode) {
            ends.extend(boundaries);
        }
        let mut group_nodes = Vec::new();
        for group in groups {
            let mut group_tracks = Vec::new();
            for track in field(&group, "Tracks")
                .as_array()
                .cloned()
                .unwrap_or_default()
            {
                group_tracks.push(
                    json!({"mlp":node(&track,"MlpPath"),"src":node(&track,"SourcePath"),
                    "title":node(&track,"Title"),"resample_to":node(&track,"ResampleTo")}),
                );
            }
            let number = field(&group, "Number").as_i64().unwrap_or(0);
            group_nodes.push(
                json!({"group":number,"sr":node(&group,"SampleRate"),"bits":node(&group,"Bits"),
                "aob":format!("ATS_{number:02}_1.AOB"),"tracks":group_tracks}),
            );
        }
        let menu = if request["MenuEnabled"].as_bool() == Some(true) {
            let plan = crate::menu::plan(json!({"Disc":disc,"Title":text(&request,"Title"),
                "MenuTracksPerPage":request["MenuTracksPerPage"],"MenuIndexMinimumAlbums":request["MenuIndexMinimumAlbums"]}))?;
            json!({"pages":plan["TotalPages"],"index_pages":plan["IndexPages"],"albums":plan["AlbumPageCount"],
                "stills":if request["MenuStillPictures"].as_bool()==Some(true) {plan["TrackCount"].clone()} else {json!(0)}})
        } else {
            Value::Null
        };
        disc_nodes.push(json!({"disc":disc_number,"volid":format!("{} {}",text(&request,"Title"),disc_number),
            "iso":format!("{}_{}.iso",text(&request,"IsoPrefix"),disc_number),"groups":group_nodes,"menu":menu}));
    }
    root.insert("__discs__".into(), Value::Array(disc_nodes));
    for track in all_tracks {
        let path = text(&track, "MlpPath");
        root.insert(path.clone(),json!({"src":node(&track,"SourcePath"),"dur":node(&track,"Duration"),
            "sr":node(&track,"SampleRate"),"bits":node(&track,"Bits"),"ch":node(&track,"Channels"),
            "channel_mask":node(&track,"ChannelMask"),"src_rate":node(&track,"SourceSampleRate"),
            "src_bits":node(&track,"SourceBits"),"resample_to":node(&track,"ResampleTo"),
            "mlp_source":node(&track,"MlpSource"),"ext_resampled":node(&track,"ExternalResampled"),
            "ext_rebitded":node(&track,"ExternalRebitded"),"param_changed":node(&track,"ParametersChanged"),
            "title":node(&track,"Title"),"lpcm_title_end":if text(&track,"MlpSource")=="lpcm" {
                ends.get(&path).cloned().unwrap_or(Value::Null)} else {Value::Null}}));
    }
    let parent = Path::new(&path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let mut bytes = serde_json::to_vec_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
    bytes.extend_from_slice(b"\r\n");
    fs::write(&path, &bytes).map_err(|e| format!("{path}: {e}"))?;
    Ok(Value::Bool(true))
}
pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    if operation == "build.write_index" {
        write_index(request)
    } else {
        Err(format!("Unsupported index operation: {operation}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn writes_disc_and_track_entries_without_managed_host() {
        let root = std::env::temp_dir().join(format!(
            "dvda-index-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("build").join("mlp_index.json");
        let request = json!({
            "Path":path,
            "BuildDirectory":root.join("build"),
            "BuildLogPath":root.join("build/build.log"),
            "DryRun":false,
            "MlpSource":"surcode-batch",
            "MlpExternalDirectory":"",
            "DiagnosticTitleMode":"album",
            "Title":"Test Disc",
            "IsoPrefix":"test",
            "Generated":"2026-10-05 12:00:00",
            "Plan":{
                "Tracks":[{
                    "MlpPath":"track.mlp","SourcePath":"track.flac","Duration":2.0,
                    "SampleRate":48000,"Bits":24,"Channels":2,"ChannelMask":3,
                    "SourceSampleRate":48000,"SourceBits":24,"ResampleTo":null,
                    "MlpSource":"surcode-batch","ExternalResampled":false,
                    "ExternalRebitded":false,"ParametersChanged":false,"Title":"Track"
                }],
                "Discs":[{"Number":1,"Groups":[{"Number":1,"SampleRate":48000,"Bits":24,
                    "Tracks":[{"MlpPath":"track.mlp","SourcePath":"track.flac",
                        "Title":"Track","ResampleTo":null}]}]}]
            }
        });
        assert_eq!(write_index(request).unwrap(), Value::Bool(true));
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("\"__meta__\""));
        assert!(contents.contains("\"__discs__\""));
        assert!(contents.contains("\"ATS_01_1.AOB\""));
        assert!(contents.contains("\"track.mlp\""));
        let _ = fs::remove_dir_all(root);
    }
}
