//! JSON cache and resume index persistence.
use dvda_native::{
    files::{ordinal_ignore_case, ordinal_ignore_case_cmp},
    media::Callbacks,
};
use serde_json::{Map, Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Load or save a cache or resume index.
///
/// # Errors
///
/// Returns an error for an unsupported operation or a missing/non-string path.
/// Saving also fails if `Entries` is absent, serialization fails, the parent
/// directory cannot be created, or the index cannot be committed. Loading an
/// unreadable, invalid, or non-object index returns an empty object instead.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Public JSON dispatch preserves the owned-request API used by crate::dispatch"
)]
pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "cache.load" | "resume.load" => load(&request),
        "cache.save" | "resume.save" => save(&request),
        _ => Err(format!("Unsupported cache operation: {operation}")),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MlpCache {
    entries: Map<String, Value>,
    ordered: Vec<String>,
}

impl MlpCache {
    #[cfg(test)]
    fn from_entries(entries: Map<String, Value>) -> Self {
        let mut ordered: Vec<String> = entries.keys().cloned().collect();
        ordered.sort_unstable_by(|left, right| ordinal_ignore_case_cmp(left, right));
        Self { entries, ordered }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn entries(&self) -> &Map<String, Value> {
        &self.entries
    }

    #[must_use]
    pub fn find(&self, path: &Path) -> Option<&Value> {
        let path = path.to_string_lossy();
        let index = self
            .ordered
            .binary_search_by(|key| ordinal_ignore_case_cmp(key, &path))
            .ok()?;
        self.entries.get(&self.ordered[index])
    }

    /// # Panics
    /// Panics if the ordered index and map diverge; they are only mutated together.
    pub fn record(&mut self, path: &Path, value: Value) {
        let path = path.to_string_lossy();
        match self
            .ordered
            .binary_search_by(|key| ordinal_ignore_case_cmp(key, &path))
        {
            Ok(index) => {
                *self
                    .entries
                    .get_mut(&self.ordered[index])
                    .expect("ordered cache key exists") = value;
            }
            Err(index) => {
                self.ordered.insert(index, path.clone().into_owned());
                self.entries.insert(path.into_owned(), value);
            }
        }
    }
}

/// The application cache loader retains the old `MlpCacheIndex` warning and
/// `OrdinalIgnoreCase` dictionary behavior. The developer JSON envelope stays
/// permissive for compatibility with its historical generic load operation.
pub fn load_mlp(path: &Path, events: &mut dyn Callbacks) -> MlpCache {
    if !path.exists() {
        return MlpCache::default();
    }
    let result = (|| -> Result<MlpCache, String> {
        let value = crate::preparation::state::read_json(path).map_err(|error| error.message)?;
        let entries = match value {
            Value::Null => return Ok(MlpCache::default()),
            Value::Object(entries) => entries,
            _ => return Err("MLP cache index must be an object".into()),
        };
        let mut ordered: Vec<(usize, &String)> = entries.keys().enumerate().collect();
        ordered.sort_unstable_by(|(_, left), (_, right)| ordinal_ignore_case_cmp(left, right));
        let duplicate = ordered
            .windows(2)
            .filter(|pair| ordinal_ignore_case(pair[0].1, pair[1].1))
            .map(|pair| pair[0].0.max(pair[1].0))
            .min();
        for (index, entry) in entries.values().enumerate() {
            let _ = <MlpEntryShape as serde::Deserialize>::deserialize(entry)
                .map_err(|error| error.to_string())?;
            if duplicate == Some(index) {
                return Err("MLP cache index contains duplicate paths".into());
            }
        }
        let ordered = ordered.into_iter().map(|(_, key)| key.clone()).collect();
        Ok(MlpCache { entries, ordered })
    })();
    match result {
        Ok(entries) => entries,
        Err(error) => {
            events.emit(
                1,
                &format!("[警告] MLP 缓存索引无法读取，将重新编码: {error}"),
            );
            MlpCache::default()
        }
    }
}

#[derive(Default, serde::Deserialize)]
#[serde(default)]
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
struct IdentityShape {
    path: Option<String>,
    size: i64,
    last_write_utc_ticks: i64,
    head_hash: Option<String>,
    tail_hash: Option<String>,
}

fn load(request: &Value) -> Result<Value, String> {
    let path = text(request, "Path").ok_or("Missing cache path")?;
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

fn save(request: &Value) -> Result<Value, String> {
    let path = text(request, "Path").ok_or("Missing cache path")?;
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
        assert_eq!(load(&json!({"Path":path})).expect("load"), json!({}));
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
        save(&json!({"Path":path, "Entries":entries})).expect("save");
        assert_eq!(load(&json!({"Path":path})).expect("reload"), entries);
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
        assert_eq!(events.0, [] as [std::string::String; 0]);
        let key = Path::new("C:\\Music\\ÉTÉ\\song.MLP");
        let valid = json!({key.to_str().unwrap():{"source":{"Size":1},"output":{"Size":2},"encoder":"same","bits":24,"resample_to":48000,"max_interval":8}});
        save(&json!({"Path":path,"Entries":valid})).unwrap();
        let mut entries = load_mlp(&path, &mut events);
        assert!(
            entries
                .find(Path::new("c:\\music\\été\\SONG.mlp"))
                .is_some()
        );
        entries.record(
            Path::new("c:\\music\\été\\SONG.mlp"),
            json!({"encoder":"new"}),
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries.entries()[key.to_str().unwrap()]["encoder"], "new");
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
        assert_eq!(events.0, [] as [std::string::String; 0]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mlp_loader_preserves_error_precedence_unicode_and_insertion_order() {
        let root = temporary_root();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("cache.json");
        let valid = |key: &str| json!({key:{"source":{"Size":1},"output":{"Size":2},"encoder":"same","bits":24,"resample_to":48000,"max_interval":8}});
        let mut events = Events::default();

        fs::write(
            &path,
            serde_json::to_vec(&json!({
                "C:\\Music\\B.mlp": valid("unused")["unused"],
                "C:\\Music\\A.mlp": valid("unused")["unused"],
                "C:\\Music\\c.mlp": valid("unused")["unused"]
            }))
            .unwrap(),
        )
        .unwrap();
        let loaded = load_mlp(&path, &mut events);
        assert_eq!(
            loaded.entries().keys().collect::<Vec<_>>(),
            [r"C:\Music\B.mlp", r"C:\Music\A.mlp", r"C:\Music\c.mlp"]
        );

        fs::write(
            &path,
            br#"{"C:\\M.mlp":{"bits":24},"c:\\m.MLP":{"bits":24},"later":{"bits":"wrong"}}"#,
        )
        .unwrap();
        assert!(load_mlp(&path, &mut events).is_empty());
        assert!(events.0.last().unwrap().contains("duplicate paths"));

        fs::write(
            &path,
            br#"{"C:\\M.mlp":{"bits":24},"c:\\m.MLP":{"bits":"wrong"}}"#,
        )
        .unwrap();
        assert!(load_mlp(&path, &mut events).is_empty());
        assert!(!events.0.last().unwrap().contains("duplicate paths"));

        fs::write(
            &path,
            serde_json::to_vec(&json!({
                "C:\\𐐀.mlp": valid("unused")["unused"],
                "c:\\𐐨.MLP": valid("unused")["unused"]
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(load_mlp(&path, &mut events).is_empty());
        assert!(events.0.last().unwrap().contains("duplicate paths"));

        fs::write(
            &path,
            serde_json::to_vec(&json!({
                "Straße.mlp": valid("unused")["unused"],
                "STRASSE.mlp": valid("unused")["unused"]
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(load_mlp(&path, &mut events).len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mlp_cache_updates_preserve_original_keys_and_insertion_order() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MlpCache>();
        let root = temporary_root();
        let mut events = Events::default();
        let mut entries = load_mlp(&root.join("missing.json"), &mut events);
        for path in ["C:\\Music\\b.mlp", "C:\\Music\\A.mlp", "C:\\Music\\c.mlp"] {
            entries.record(Path::new(path), json!({"encoder":"old"}));
        }
        entries.record(Path::new("c:\\music\\a.MLP"), json!({"encoder":"new"}));
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries.entries().keys().collect::<Vec<_>>(),
            [r"C:\Music\b.mlp", r"C:\Music\A.mlp", r"C:\Music\c.mlp"]
        );
        assert_eq!(entries.entries()[r"C:\Music\A.mlp"]["encoder"], "new");
        assert!(entries.find(Path::new("c:\\music\\B.MLP")).is_some());
    }

    #[test]
    fn mlp_cache_save_preserves_map_order_without_serializing_index() {
        let root = temporary_root();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("cache.json");
        let mut entries = MlpCache::default();
        for key in ["C:\\Music\\B.mlp", "C:\\Music\\A.mlp", "C:\\Music\\c.mlp"] {
            entries.record(Path::new(key), json!({"encoder":"old"}));
        }
        entries.record(Path::new("c:\\music\\a.MLP"), json!({"encoder":"new"}));
        entries.record(Path::new("C:\\Music\\d.mlp"), json!({"encoder":"added"}));
        save(&json!({"Path":path,"Entries":entries.entries()})).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("ordered"));
        let document: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            document.as_object().unwrap().keys().collect::<Vec<_>>(),
            [
                r"C:\Music\B.mlp",
                r"C:\Music\A.mlp",
                r"C:\Music\c.mlp",
                r"C:\Music\d.mlp"
            ]
        );
        let reloaded = load_mlp(&path, &mut Events::default());
        assert_eq!(reloaded, entries);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "deterministic throughput benchmark"]
    fn mlp_cache_lookup_throughput() {
        use std::hint::black_box;
        use std::time::Instant;

        let mut entries = Map::new();
        let mut paths = Vec::new();
        for index in 0..5000 {
            let path = format!("C:\\Music\\Album-{:03}\\Track-{index:05}.mlp", index / 20);
            paths.push(PathBuf::from(&path));
            entries.insert(path, json!({"encoder":"same","bits":24}));
        }
        let mut entries = MlpCache::from_entries(entries);
        let start = Instant::now();
        for path in &paths {
            assert!(entries.find(black_box(path)).is_some());
        }
        let find_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        for path in paths.iter().take(99) {
            entries.record(black_box(path), json!({"encoder":"updated"}));
        }
        let record_ns = start.elapsed().as_nanos();
        assert_eq!(entries.len(), 5000);
        println!(
            "[METRIC] mlp_cache_find_ns={find_ns} entries={}",
            entries.len()
        );
        println!("[METRIC] mlp_cache_record_ns={record_ns} updates=99");
    }

    #[test]
    #[ignore = "deterministic throughput benchmark"]
    fn mlp_cache_load_throughput() {
        use std::hint::black_box;
        use std::time::Instant;

        let root = temporary_root();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("cache.json");
        let mut entries = Map::new();
        for index in 0..5000 {
            entries.insert(
                format!("C:\\Music\\Album-{:03}\\Track-{index:05}.mlp", index / 20),
                json!({"source":{"Size":1},"output":{"Size":2},"encoder":"same","bits":24,"resample_to":48000,"max_interval":8}),
            );
        }
        fs::write(&path, serde_json::to_vec(&entries).unwrap()).unwrap();
        let mut events = Events::default();
        let start = Instant::now();
        let loaded = load_mlp(black_box(&path), black_box(&mut events));
        let elapsed = start.elapsed().as_nanos();
        assert_eq!(loaded.len(), entries.len());
        assert_eq!(events.0, [] as [std::string::String; 0]);
        println!(
            "[METRIC] mlp_cache_load_ns={elapsed} entries={}",
            loaded.len()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cache_failed_atomic_commit_keeps_prior_file_and_cleans_only_its_temporary() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = temporary_root();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("cache.json");
        save(&json!({"Path":path,"Entries":{"before":1}})).unwrap();
        let before = fs::read(&path).unwrap();
        let foreign = root.join("cache.json.tmp");
        fs::write(&foreign, b"other writer").unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        assert!(save(&json!({"Path":path,"Entries":{"after":2}})).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::read(&foreign).unwrap(), b"other writer");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        drop(lock);
        save(&json!({"Path":path,"Entries":{"after":2}})).unwrap();
        assert_eq!(load(&json!({"Path":path})).unwrap(), json!({"after":2}));
        fs::remove_dir_all(root).unwrap();
    }
}
