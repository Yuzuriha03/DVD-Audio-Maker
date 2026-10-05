//! JSON cache and resume index persistence.
use dvda_native::{files::ordinal_ignore_case, media::Callbacks};
use serde_json::{Map, Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "cache.load" | "resume.load" => load(request),
        "cache.save" | "resume.save" => save(request),
        _ => Err(format!("Unsupported cache operation: {operation}")),
    }
}

/// The application cache loader retains the old MlpCacheIndex warning and
/// OrdinalIgnoreCase dictionary behavior. The developer JSON envelope stays
/// permissive for compatibility with its historical generic load operation.
pub fn load_mlp(path: &Path, events: &mut dyn Callbacks) -> Map<String, Value> {
    if !path.exists() {
        return Map::new();
    }
    let result = (|| -> Result<Map<String, Value>, String> {
        let value = crate::preparation::state::read_json(path).map_err(|error| error.message)?;
        if value.is_null() {
            return Ok(Map::new());
        }
        let entries = value
            .as_object()
            .ok_or("MLP cache index must be an object")?;
        let mut result = Map::new();
        for (path, entry) in entries {
            let _ = serde_json::from_value::<MlpEntryShape>(entry.clone())
                .map_err(|e| e.to_string())?;
            if result.keys().any(|key| ordinal_ignore_case(path, key)) {
                return Err("MLP cache index contains duplicate paths".into());
            }
            result.insert(path.clone(), entry.clone());
        }
        Ok(result)
    })();
    match result {
        Ok(entries) => entries,
        Err(error) => {
            events.emit(
                1,
                &format!("[警告] MLP 缓存索引无法读取，将重新编码: {error}"),
            );
            Map::new()
        }
    }
}

#[derive(Default, serde::Deserialize)]
#[serde(default)]
#[allow(dead_code)]
struct MlpEntryShape {
    source: Option<IdentityShape>,
    output: Option<IdentityShape>,
    encoder: String,
    bits: i32,
    resample_to: Option<i32>,
    max_interval: i32,
}
#[derive(Default, serde::Deserialize)]
#[serde(default, rename_all = "PascalCase")]
#[allow(dead_code)]
struct IdentityShape {
    path: Option<String>,
    size: i64,
    last_write_utc_ticks: i64,
    head_hash: Option<String>,
    tail_hash: Option<String>,
}

pub fn find_mlp<'a>(entries: &'a Map<String, Value>, path: &Path) -> Option<&'a Value> {
    entries
        .iter()
        .find(|(key, _)| ordinal_ignore_case(key, &path.to_string_lossy()))
        .map(|(_, value)| value)
}

pub fn record_mlp(entries: &mut Map<String, Value>, path: &Path, value: Value) {
    if let Some((_, entry)) = entries
        .iter_mut()
        .find(|(key, _)| ordinal_ignore_case(key, &path.to_string_lossy()))
    {
        *entry = value;
    } else {
        entries.insert(path.to_string_lossy().into_owned(), value);
    }
}

fn load(request: Value) -> Result<Value, String> {
    let path = text(&request, "Path").ok_or("Missing cache path")?;
    let entries = match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) if value.is_object() => value,
            _ => json!({}),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(_) => json!({}),
    };
    Ok(entries)
}

fn save(request: Value) -> Result<Value, String> {
    let path = text(&request, "Path").ok_or("Missing cache path")?;
    let entries = request.get("Entries").ok_or("Missing cache entries")?;
    let text = serde_json::to_string_pretty(entries)
        .map_err(|error| format!("Serialize cache entries: {error}"))?
        + "\n";
    let path = PathBuf::from(path);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Create cache directory {}: {error}", parent.display()))?;
    }
    crate::preparation::state::write_bytes(&path, text.as_bytes())
        .map_err(|error| format!("Commit cache {}: {}", path.display(), error.message))?;
    Ok(Value::Null)
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .or_else(|| value.get(key.to_ascii_lowercase()))
        .and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "dvda-cache-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn load_missing_and_save_round_trip() {
        let root = temporary_root();
        let path = root.join("nested").join("cache.json");
        assert_eq!(load(json!({"Path":path})).expect("load"), json!({}));
        let entries = json!({
            "C:/song.mlp": {
                "source": {"Path":"C:/song.flac", "Size": 1},
                "output": {"Path":"C:/song.mlp", "Size": 2},
                "encoder": "encoder|1",
                "bits": 24,
                "resample_to": null,
                "max_interval": 8
            }
        });
        save(json!({"Path":path, "Entries":entries})).expect("save");
        assert_eq!(load(json!({"Path":path})).expect("reload"), entries);
        let _ = fs::remove_dir_all(root);
    }

    #[derive(Default)]
    struct Events(Vec<String>);
    impl Callbacks for Events {
        fn emit(&mut self, _: i32, text: &str) {
            self.0.push(text.into());
        }
        fn cancelled(&mut self) -> bool {
            false
        }
    }

    #[test]
    fn mlp_loader_warns_on_corruption_and_preserves_windows_case_dictionary_semantics() {
        let root = temporary_root();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("cache.json");
        let mut events = Events::default();
        assert!(load_mlp(&path, &mut events).is_empty());
        assert!(events.0.is_empty());
        let key = Path::new("C:\\Music\\ÉTÉ\\song.MLP");
        let valid = json!({key.to_str().unwrap():{"source":{"Size":1},"output":{"Size":2},"encoder":"same","bits":24,"resample_to":48000,"max_interval":8}});
        save(json!({"Path":path,"Entries":valid})).unwrap();
        let mut entries = load_mlp(&path, &mut events);
        assert!(find_mlp(&entries, Path::new("c:\\music\\été\\SONG.mlp")).is_some());
        record_mlp(
            &mut entries,
            Path::new("c:\\music\\été\\SONG.mlp"),
            json!({"encoder":"new"}),
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[key.to_str().unwrap()]["encoder"], "new");
        let mut bom = vec![0xef, 0xbb, 0xbf];
        bom.extend(serde_json::to_vec(&valid).unwrap());
        fs::write(&path, bom).unwrap();
        assert_eq!(load_mlp(&path, &mut events).len(), 1);
        for malformed in [
            "{",
            "[]",
            "{\"a\":null}",
            "{\"a\":{\"bits\":\"wrong\"}}",
            "{\"a\":{\"source\":{\"Size\":\"wrong\"}}}",
            "{\"C:\\\\M.mlp\":{},\"c:\\\\m.MLP\":{}}",
        ] {
            fs::write(&path, malformed).unwrap();
            events.0.clear();
            assert!(load_mlp(&path, &mut events).is_empty());
            assert_eq!(events.0.len(), 1);
            let warning = &events.0[0];
            assert!(warning.starts_with("[警告] MLP 缓存索引无法读取"));
            for language in ["en", "ja"] {
                assert_ne!(crate::localization::translate(language, warning), *warning);
            }
        }
        fs::write(&path, "null").unwrap();
        events.0.clear();
        assert!(load_mlp(&path, &mut events).is_empty());
        assert!(events.0.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cache_failed_atomic_commit_keeps_prior_file_and_cleans_only_its_temporary() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = temporary_root();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("cache.json");
        save(json!({"Path":path,"Entries":{"before":1}})).unwrap();
        let before = fs::read(&path).unwrap();
        let foreign = root.join("cache.json.tmp");
        fs::write(&foreign, b"other writer").unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        assert!(save(json!({"Path":path,"Entries":{"after":2}})).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::read(&foreign).unwrap(), b"other writer");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        drop(lock);
        save(json!({"Path":path,"Entries":{"after":2}})).unwrap();
        assert_eq!(load(json!({"Path":path})).unwrap(), json!({"after":2}));
        fs::remove_dir_all(root).unwrap();
    }
}
