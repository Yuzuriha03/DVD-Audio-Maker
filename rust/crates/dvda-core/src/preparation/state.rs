use super::models::Patch;
use crate::{
    audio::Metadata,
    identity,
    media::{Failure, Temporary, temporary_path},
};
use dvda_native::{files, media::Callbacks};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    os::windows::fs::OpenOptionsExt,
    path::Path,
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ProbeFacts {
    pub sr: i32,
    pub bits: i32,
    pub ch: i32,
    pub date: String,
    pub track: String,
    pub title: String,
    pub album: String,
    pub dur: f64,
}
impl ProbeFacts {
    #[must_use]
    pub fn metadata(&self, path: &str) -> Metadata {
        Metadata {
            path: path.into(),
            sample_rate: self.sr,
            bits: self.bits,
            channels: self.ch,
            date: self.date.clone(),
            track: self.track.clone(),
            title: self.title.clone(),
            album: self.album.clone(),
            duration: self.dur,
            source_sample_rate: self.sr,
            source_bits: self.bits,
            resample_to: None,
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Validation {
    pub sr: i32,
    pub bits: i32,
    pub resample_to: Option<i32>,
    pub expected: Option<i64>,
    pub decoded: i64,
    pub patches: Option<Vec<Patch>>,
    pub repaired_file: Option<Value>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Entry {
    pub identity: Value,
    pub probe: ProbeFacts,
    pub validation: Validation,
}
#[derive(Default)]
pub struct Cache {
    // Sorted with the same ordinal comparer used for Windows path equality.
    entries: Vec<(String, Entry)>,
}

#[must_use]
pub fn matches(path: &str, expected: &Value) -> bool {
    identity::compute(path).is_ok_and(|current| {
        ["Size", "LastWriteUtcTicks", "HeadHash", "TailHash"]
            .iter()
            .all(|key| !expected[*key].is_null() && current[*key] == expected[*key])
    })
}
/// Read a JSON value, accepting an optional UTF-8 BOM.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or read, or its contents are
/// not valid JSON.
pub fn read_json(path: &Path) -> Result<Value, Failure> {
    let mut bytes = Vec::new();
    identity::open_read(&path.to_string_lossy())?.read_to_end(&mut bytes)?;
    serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))
        .map_err(|error| Failure::new("InvalidData", &error.to_string()))
}
/// Write bytes through a temporary file and replace the destination.
///
/// # Errors
///
/// Returns an error if parent directories or the exclusive temporary file cannot
/// be created, writing fails, or the temporary file cannot replace the destination.
pub fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let temporary = temporary_path(path);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(&temporary)?;
    let mut owned = Temporary(Some(temporary.clone()));
    let written = file.write_all(bytes);
    drop(file);
    written?;
    files::move_file(&temporary, path, true)?;
    owned.0 = None;
    Ok(())
}
/// Serialize pretty JSON with a trailing CRLF and replace the destination.
///
/// # Errors
///
/// Returns an error if serialization fails or [`write_bytes`] cannot create,
/// write, or commit the destination file.
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Failure> {
    let mut data = serde_json::to_vec_pretty(value)
        .map_err(|error| Failure::new("InvalidData", &error.to_string()))?;
    data.extend_from_slice(b"\r\n");
    write_bytes(path, &data)
}
#[derive(Deserialize)]
struct CacheFile {
    #[serde(flatten)]
    entries: BTreeMap<String, Entry>,
}

impl Serialize for Cache {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap as _;
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (key, value) in &self.entries {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl Cache {
    pub fn load(path: &Path, events: &mut dyn Callbacks) -> Self {
        if !path.is_file() {
            return Self::default();
        }
        let loaded = (|| {
            let mut bytes = Vec::new();
            identity::open_read(&path.to_string_lossy())?.read_to_end(&mut bytes)?;
            let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
            let Some(file) = serde_json::from_slice::<Option<CacheFile>>(bytes)
                .map_err(|error| Failure::new("InvalidData", &error.to_string()))?
            else {
                return Ok(Self::default());
            };
            let mut records: Vec<(String, Entry)> = file.entries.into_iter().collect();
            records.sort_unstable_by(|(left, _), (right, _)| {
                files::ordinal_ignore_case_cmp(left, right)
            });
            let mut cache = Self::default();
            for (path, entry) in records {
                if cache
                    .entries
                    .last()
                    .is_some_and(|(key, _)| files::ordinal_ignore_case(&path, key))
                {
                    return Err(Failure::new(
                        "InvalidData",
                        "Duplicate preparation cache path",
                    ));
                }
                cache.entries.push((path, entry));
            }
            Ok(cache)
        })();
        match loaded {
            Ok(cache) => cache,
            Err(error) => {
                events.emit(
                    1,
                    &format!("[警告] 准备缓存无法读取，将全部重新校验: {}", error.message),
                );
                Self::default()
            }
        }
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
    pub fn find(&self, path: &str) -> Option<&Entry> {
        let index = self.index(path).ok()?;
        let entry = &self.entries[index].1;
        matches(path, &entry.identity).then_some(entry)
    }
    pub fn record(&mut self, path: String, entry: Entry) {
        match self.index(&path) {
            Ok(index) => self.entries[index].1 = entry,
            Err(index) => self.entries.insert(index, (path, entry)),
        }
    }
    fn index(&self, path: &str) -> Result<usize, usize> {
        self.entries
            .binary_search_by(|(key, _)| files::ordinal_ignore_case_cmp(key, path))
    }
    /// Persist the current cache entries as JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if JSON serialization or creating, writing, or replacing
    /// the cache file fails.
    pub fn save(&self, path: &Path) -> Result<(), Failure> {
        write_json(path, self)
    }
}
#[must_use]
pub fn reusable(entry: &Entry, track: &Metadata, expected: Option<i64>) -> bool {
    let v = &entry.validation;
    v.decoded > 0
        && v.sr == track.sample_rate
        && v.bits == track.bits
        && v.resample_to == track.resample_to
        && v.expected == expected
        && v.repaired_file.as_ref().is_none_or(|file| {
            file["Path"]
                .as_str()
                .is_some_and(|path| matches(path, file))
        })
}
pub fn record(
    cache: &mut Cache,
    track: &Metadata,
    path: &str,
    expected: Option<i64>,
    decoded: Option<i64>,
    patches: &[Patch],
) {
    let (Ok(identity), Some(decoded)) = (identity::compute(&track.path), decoded) else {
        return;
    };
    cache.record(
        track.path.clone(),
        Entry {
            identity,
            probe: ProbeFacts {
                sr: track.source_sample_rate,
                bits: track.source_bits,
                ch: track.channels,
                date: track.date.clone(),
                track: track.track.clone(),
                title: track.title.clone(),
                album: track.album.clone(),
                dur: track.duration,
            },
            validation: Validation {
                sr: track.sample_rate,
                bits: track.bits,
                resample_to: track.resample_to,
                expected,
                decoded,
                patches: (!patches.is_empty()).then(|| patches.to_vec()),
                repaired_file: if path == track.path {
                    None
                } else {
                    identity::compute(path).ok()
                },
            },
        },
    );
}
/// Enumerate FLAC and M4A sources recursively in UTF-16 path order.
/// A root that is not a directory produces an empty list.
///
/// # Errors
///
/// Returns an error if a directory cannot be canonicalized or enumerated, an
/// entry cannot be read, or a directory link leads back to an ancestor.
pub fn enumerate_sources(root: &Path) -> Result<Vec<String>, Failure> {
    let mut result = Vec::new();
    if !root.is_dir() {
        return Ok(result);
    }
    let mut pending = vec![(root.to_path_buf(), Vec::new())];
    while let Some((path, mut ancestors)) = pending.pop() {
        // Preserve distinct directory aliases. Only ancestor cycles are rejected.
        let canonical = fs::canonicalize(&path)?;
        if ancestors.contains(&canonical) {
            return Err(Failure::new("Io", "Source directory contains a link cycle"));
        }
        ancestors.push(canonical);
        for item in fs::read_dir(path)? {
            let item = item?;
            let path = item.path();
            if path.is_dir() {
                pending.push((path, ancestors.clone()));
            } else if path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("flac") || ext.eq_ignore_ascii_case("m4a")
            }) {
                result.push(path.to_string_lossy().into_owned());
            }
        }
    }
    result.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    Ok(result)
}
#[must_use]
pub fn normalize(path: &str) -> String {
    if crate::config::trim(path).is_empty() {
        return String::new();
    }
    // Retain the original snapshot's full-path treatment of tool identifiers.
    // The identifiers are fingerprint data and are never executed as paths.
    std::path::absolute(path)
        .map_or_else(|_| path.trim().into(), |p| p.to_string_lossy().into_owned())
}
pub(super) fn relative(root: &str, path: &str) -> String {
    let root = normalize(root);
    let path = normalize(path);
    let left: Vec<_> = Path::new(&root).components().collect();
    let right: Vec<_> = Path::new(&path).components().collect();
    let common = left
        .iter()
        .zip(&right)
        .take_while(|(a, b)| {
            files::ordinal_ignore_case(
                &a.as_os_str().to_string_lossy(),
                &b.as_os_str().to_string_lossy(),
            )
        })
        .count();
    if common < 2 {
        return path;
    }
    let mut parts = vec!["..".to_owned(); left.len() - common];
    parts.extend(
        right[common..]
            .iter()
            .map(|part| part.as_os_str().to_string_lossy().into_owned()),
    );
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}
/// # Panics
/// Sorting requires string-valued `RelativePath` fields. Every record is built
/// locally from the string returned by `relative`, preserving this invariant.
#[must_use]
pub fn save_snapshot(
    path: &Path,
    root: &str,
    fingerprint: &str,
    manifest_path: &str,
    sources: &[String],
) -> bool {
    let Ok(manifest) = identity::compute(manifest_path) else {
        return false;
    };
    let mut records = Vec::new();
    for source in sources {
        let Ok(identity) = identity::compute(source) else {
            return false;
        };
        records.push(json!({"RelativePath":relative(root,source),"Identity":identity}));
    }
    records.sort_by(|a, b| {
        a["RelativePath"]
            .as_str()
            .unwrap()
            .encode_utf16()
            .cmp(b["RelativePath"].as_str().unwrap().encode_utf16())
    });
    write_json(path,&json!({"Version":1,"SourceDirectory":normalize(root),"ConfigurationFingerprint":fingerprint,"Sources":records,"Manifest":manifest})).is_ok()
}

pub fn invalidate(path: &Path) {
    if path.is_file() {
        let _ = fs::remove_file(path);
    }
}
/// Check a saved snapshot against configuration, manifest, and source identities.
/// Missing, invalid, or outdated snapshots return `Ok` with no reusable value
/// and an explanation.
///
/// # Errors
///
/// Returns an error if source enumeration fails, including unreadable directories
/// or directory link cycles; see [`enumerate_sources`].
///
/// # Panics
/// Sorting and searching require string-valued `RelativePath` fields. All
/// records are validated before these operations; malformed records return
/// a non-reusable snapshot instead.
pub fn reuse_snapshot(
    path: &Path,
    root: &str,
    fingerprint: &str,
    manifest: &str,
) -> Result<(Option<Value>, String), Failure> {
    let Some(value) = super::snapshot::load(path) else {
        return Ok((None, "未找到可复用的准备快照".into()));
    };
    let reason = if value["Sources"].is_null() || value["Manifest"].is_null() {
        Some("准备快照内容不完整")
    } else if value["Version"] != 1 {
        Some("准备快照版本已变化")
    } else if !files::ordinal_ignore_case(
        value["SourceDirectory"].as_str().unwrap_or(""),
        &normalize(root),
    ) {
        Some("音源目录已变化")
    } else if value["ConfigurationFingerprint"] != fingerprint {
        Some("音源检查设置已变化")
    } else if !matches(manifest, &value["Manifest"]) {
        Some("准备清单已变化或不存在")
    } else {
        None
    };
    if let Some(reason) = reason {
        return Ok((None, reason.into()));
    }
    let Some(expected) = value["Sources"].as_array() else {
        return Ok((None, "准备快照内容不完整".into()));
    };
    // Corrupt null records cannot serve as cache evidence.
    if expected
        .iter()
        .any(|item| item["RelativePath"].as_str().is_none() || item["Identity"].is_null())
    {
        return Ok((None, "准备快照内容不完整".into()));
    }
    let sources = enumerate_sources(Path::new(root))?;
    if sources.len() != expected.len() {
        return Ok((None, "音源文件数量已变化".into()));
    }
    let mut expected: Vec<_> = expected.iter().collect();
    expected.sort_unstable_by(|left, right| {
        files::ordinal_ignore_case_cmp(
            left["RelativePath"].as_str().unwrap(),
            right["RelativePath"].as_str().unwrap(),
        )
    });
    for pair in expected.windows(2) {
        if files::ordinal_ignore_case(
            pair[0]["RelativePath"].as_str().unwrap(),
            pair[1]["RelativePath"].as_str().unwrap(),
        ) {
            return Ok((None, "准备快照包含重复的音源路径".into()));
        }
    }
    for source in &sources {
        let relative = relative(root, source);
        let found = expected
            .binary_search_by(|item| {
                files::ordinal_ignore_case_cmp(item["RelativePath"].as_str().unwrap(), &relative)
            })
            .ok()
            .map(|index| expected[index]);
        if found.is_none_or(|item| !matches(source, &item["Identity"])) {
            return Ok((
                None,
                format!(
                    "音源文件已变化: {}",
                    Path::new(source)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                ),
            ));
        }
    }
    Ok((
        Some(value),
        format!("复用已确认的 {} 个音源检查结果", sources.len()),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;
    use std::path::PathBuf;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "dvda-cache-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
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
    fn cache_reuses_case_aliases_and_rejects_same_size_same_time_changes() {
        let root = Scratch::new();
        let mut cache = Cache::default();
        let mut paths = Vec::new();
        for name in ["z.flac", "a.flac", "\u{4e2d}\u{6587}.flac"] {
            let path = root.0.join(name);
            fs::write(&path, b"original").unwrap();
            let path = path.to_str().unwrap().to_owned();
            cache.record(
                path.clone(),
                Entry {
                    identity: identity::compute(&path).unwrap(),
                    ..Default::default()
                },
            );
            paths.push(path);
        }
        let cache_path = root.0.join("cache.json");
        cache.save(&cache_path).unwrap();
        let mut loaded = Cache::load(&cache_path, &mut Events::default());
        for path in &paths {
            assert!(loaded.find(&path.to_ascii_uppercase()).is_some());
        }
        let path = &paths[0];
        let mut entry = loaded.find(path).unwrap().clone();
        entry.validation.decoded = 48000;
        loaded.record(path.to_ascii_uppercase(), entry);
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded.find(path).unwrap().validation.decoded, 48000);
        let modified = fs::metadata(path).unwrap().modified().unwrap();
        fs::write(path, b"modified").unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert!(loaded.find(path).is_none());
    }

    #[test]
    fn cache_rejects_duplicate_ascii_and_supplementary_case_keys() {
        let root = Scratch::new();
        let path = root.0.join("cache.json");
        for (left, right) in [("A.flac", "a.flac"), ("\u{10400}.flac", "\u{10428}.flac")] {
            let mut values = Map::new();
            values.insert(left.into(), json!(Entry::default()));
            values.insert("unrelated.flac".into(), json!(Entry::default()));
            values.insert(right.into(), json!(Entry::default()));
            write_json(&path, &values).unwrap();
            let mut events = Events::default();
            assert!(Cache::load(&path, &mut events).is_empty());
            assert!(events.0[0].contains("Duplicate preparation cache path"));
        }
        let mut cache = Cache::default();
        cache.record("\u{10400}.flac".into(), Entry::default());
        cache.record("\u{10428}.flac".into(), Entry::default());
        assert_eq!(cache.len(), 1);
        cache.record("Stra\u{df}e.flac".into(), Entry::default());
        cache.record("STRASSE.flac".into(), Entry::default());
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn snapshot_reuse_accepts_unsorted_records_but_rejects_duplicates_and_replacements() {
        let root = Scratch::new();
        let source = root.0.join("sources");
        fs::create_dir(&source).unwrap();
        for name in ["a.flac", "z.flac", "\u{4e2d}\u{6587}.flac"] {
            fs::write(source.join(name), b"source").unwrap();
        }
        let manifest = root.0.join("manifest.json");
        fs::write(&manifest, b"{}").unwrap();
        let snapshot = root.0.join("snapshot.json");
        let sources = enumerate_sources(&source).unwrap();
        assert!(save_snapshot(
            &snapshot,
            source.to_str().unwrap(),
            "fingerprint",
            manifest.to_str().unwrap(),
            &sources
        ));
        let mut value = read_json(&snapshot).unwrap();
        value["Sources"].as_array_mut().unwrap().reverse();
        let records = value["Sources"].as_array_mut().unwrap();
        for record in records.iter_mut() {
            if record["RelativePath"] == "a.flac" {
                record["RelativePath"] = json!("A.FLAC");
            }
        }
        write_json(&snapshot, &value).unwrap();
        let reuse = || {
            reuse_snapshot(
                &snapshot,
                source.to_str().unwrap(),
                "fingerprint",
                manifest.to_str().unwrap(),
            )
            .unwrap()
            .0
            .is_some()
        };
        assert!(reuse());
        let mut duplicate = value.clone();
        duplicate["Sources"][0]["RelativePath"] = duplicate["Sources"][2]["RelativePath"].clone();
        write_json(&snapshot, &duplicate).unwrap();
        assert!(!reuse());
        let mut corrupt = value.clone();
        corrupt["Sources"][0]["Identity"] = Value::Null;
        write_json(&snapshot, &corrupt).unwrap();
        assert!(!reuse());
        write_json(&snapshot, &value).unwrap();
        fs::remove_file(source.join("a.flac")).unwrap();
        fs::write(source.join("replacement.flac"), b"source").unwrap();
        assert!(!reuse());
    }

    fn benchmark_cache(count: usize) -> Cache {
        let mut cache = Cache::default();
        for index in 0..count {
            cache.record(
                format!("D:/library/disc {index:04}/track {index:02}.flac"),
                Entry {
                    identity: json!({
                        "Size": (index as u64) * 16_000_000 + 123_456,
                        "LastWriteUtcTicks": 638_000_000_000_000_000u64 + index as u64,
                        "HeadHash": format!("{index:016x}"),
                        "TailHash": format!("{:016x}", index * 7 + 1),
                    }),
                    probe: ProbeFacts {
                        sr: 96_000,
                        bits: 24,
                        ch: 2,
                        date: "2026-11-08".into(),
                        track: format!("{index}"),
                        title: "Benchmark title".into(),
                        album: format!("Album {index}"),
                        dur: 180.0,
                    },
                    validation: Validation {
                        sr: 96_000,
                        bits: 24,
                        resample_to: None,
                        expected: Some(17_280_000),
                        decoded: 17_280_000,
                        patches: None,
                        repaired_file: None,
                    },
                },
            );
        }
        cache
    }

    fn legacy_save_bytes(cache: &Cache) -> Vec<u8> {
        let entries: Map<_, _> = cache
            .entries
            .iter()
            .map(|(key, value)| (key.clone(), json!(value)))
            .collect();
        let mut data = serde_json::to_vec_pretty(&entries).unwrap();
        data.extend_from_slice(b"\r\n");
        data
    }

    fn legacy_load(path: &Path) -> Cache {
        let value = read_json(path).unwrap();
        assert!(!value.is_null(), "benchmark cache is never null");
        let object = value.as_object().unwrap();
        let mut records: Vec<_> = object.iter().collect();
        records
            .sort_unstable_by(|(left, _), (right, _)| files::ordinal_ignore_case_cmp(left, right));
        let mut cache = Cache::default();
        for (path, value) in records {
            let entry: Entry = serde_json::from_value(value.clone()).unwrap();
            cache.entries.push((path.clone(), entry));
        }
        cache
    }

    fn new_save_bytes(cache: &Cache) -> Vec<u8> {
        let mut data = serde_json::to_vec_pretty(cache).unwrap();
        data.extend_from_slice(b"\r\n");
        data
    }

    fn median_ms(times: &[std::time::Duration]) -> f64 {
        let mut values: Vec<f64> = times
            .iter()
            .map(|time| time.as_secs_f64() * 1000.0)
            .collect();
        values.sort_by(f64::total_cmp);
        let middle = values.len() / 2;
        if values.len().is_multiple_of(2) {
            f64::midpoint(values[middle - 1], values[middle])
        } else {
            values[middle]
        }
    }

    #[test]
    fn direct_cache_serialization_matches_legacy_value_tree() {
        let cache = benchmark_cache(64);
        let legacy = legacy_save_bytes(&cache);
        let direct = new_save_bytes(&cache);
        assert_eq!(legacy, direct);
        let root = Scratch::new();
        let path = root.0.join("cache.json");
        fs::write(&path, &legacy).unwrap();
        let loaded = legacy_load(&path);
        assert_eq!(loaded.entries.len(), cache.entries.len());
        let direct_loaded = Cache::load(&path, &mut Events::default());
        assert_eq!(direct_loaded.entries.len(), cache.entries.len());
        for ((legacy_key, _), (direct_key, _)) in loaded.entries.iter().zip(&direct_loaded.entries)
        {
            assert_eq!(legacy_key, direct_key);
        }
        fs::write(&path, b"null").unwrap();
        assert!(Cache::load(&path, &mut Events::default()).is_empty());
    }

    #[test]
    #[ignore = "deterministic preparation cache throughput benchmark"]
    #[allow(
        clippy::too_many_lines,
        reason = "The opt-in benchmark keeps legacy and direct load/save paired with output validation"
    )]
    fn cache_load_save_throughput() {
        let count = 4_000;
        let cache = benchmark_cache(count);
        let root = Scratch::new();
        let path = root.0.join("cache.json");
        let legacy_bytes = legacy_save_bytes(&cache);
        fs::write(&path, &legacy_bytes).unwrap();
        assert_eq!(new_save_bytes(&cache), legacy_bytes);

        let mut legacy_load_times = Vec::new();
        let mut direct_load_times = Vec::new();
        let mut legacy_save_times = Vec::new();
        let mut direct_save_times = Vec::new();
        for round in 0..6 {
            let legacy_first = round % 2 == 0;
            let time_load = |direct: bool| {
                let started = std::time::Instant::now();
                for _ in 0..10 {
                    let loaded = if direct {
                        Cache::load(&path, &mut Events::default())
                    } else {
                        legacy_load(&path)
                    };
                    assert_eq!(loaded.entries.len(), count);
                }
                started.elapsed()
            };
            let time_save = |direct: bool| {
                let started = std::time::Instant::now();
                for _ in 0..10 {
                    let bytes = if direct {
                        new_save_bytes(&cache)
                    } else {
                        legacy_save_bytes(&cache)
                    };
                    assert_eq!(bytes.len(), legacy_bytes.len());
                }
                started.elapsed()
            };
            if legacy_first {
                legacy_load_times.push(time_load(false));
                direct_load_times.push(time_load(true));
                legacy_save_times.push(time_save(false));
                direct_save_times.push(time_save(true));
            } else {
                direct_load_times.push(time_load(true));
                legacy_load_times.push(time_load(false));
                direct_save_times.push(time_save(true));
                legacy_save_times.push(time_save(false));
            }
        }
        println!(
            "[METRIC] cache_load_legacy_ms={:.4} cache_load_direct_ms={:.4} cache_save_legacy_ms={:.4} cache_save_direct_ms={:.4} entries={count} bytes={}",
            median_ms(&legacy_load_times),
            median_ms(&direct_load_times),
            median_ms(&legacy_save_times),
            median_ms(&direct_save_times),
            legacy_bytes.len()
        );
    }
}
