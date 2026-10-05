use super::models::Patch;
use crate::{
    audio::Metadata,
    identity,
    media::{Failure, Temporary, temporary_path},
};
use dvda_native::{files, media::Callbacks};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
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
    entries: Vec<(String, Entry)>,
}

pub fn matches(path: &str, expected: &Value) -> bool {
    identity::compute(path).is_ok_and(|current| {
        ["Size", "LastWriteUtcTicks", "HeadHash", "TailHash"]
            .iter()
            .all(|key| !expected[*key].is_null() && current[*key] == expected[*key])
    })
}
pub fn read_json(path: &Path) -> Result<Value, Failure> {
    let mut bytes = Vec::new();
    identity::open_read(&path.to_string_lossy())?.read_to_end(&mut bytes)?;
    serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))
        .map_err(|error| Failure::new("InvalidData", &error.to_string()))
}
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
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Failure> {
    let mut data = serde_json::to_vec_pretty(value)
        .map_err(|error| Failure::new("InvalidData", &error.to_string()))?;
    data.extend_from_slice(b"\r\n");
    write_bytes(path, &data)
}
impl Cache {
    pub fn load(path: &Path, events: &mut dyn Callbacks) -> Self {
        if !path.is_file() {
            return Self::default();
        }
        let loaded = (|| {
            let value = read_json(path)?;
            if value.is_null() {
                return Ok(Self::default());
            }
            let object = value.as_object().ok_or_else(|| {
                Failure::new("InvalidData", "Preparation cache must be an object")
            })?;
            let mut cache = Self::default();
            for (path, value) in object {
                if cache
                    .entries
                    .iter()
                    .any(|(key, _)| files::ordinal_ignore_case(path, key))
                {
                    return Err(Failure::new(
                        "InvalidData",
                        "Duplicate preparation cache path",
                    ));
                }
                let entry = serde_json::from_value(value.clone())
                    .map_err(|error| Failure::new("InvalidData", &error.to_string()))?;
                cache.entries.push((path.clone(), entry));
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
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn find(&self, path: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|(key, entry)| {
                files::ordinal_ignore_case(path, key) && matches(path, &entry.identity)
            })
            .map(|(_, v)| v)
    }
    pub fn record(&mut self, path: String, entry: Entry) {
        if let Some((_, existing)) = self
            .entries
            .iter_mut()
            .find(|(key, _)| files::ordinal_ignore_case(key, &path))
        {
            *existing = entry;
        } else {
            self.entries.push((path, entry));
        }
    }
    pub fn save(&self, path: &Path) -> Result<(), Failure> {
        let entries: Map<_, _> = self
            .entries
            .iter()
            .map(|(key, value)| (key.clone(), json!(value)))
            .collect();
        write_json(path, &entries)
    }
}
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
pub fn normalize(path: &str) -> String {
    if crate::config::trim(path).is_empty() {
        return String::new();
    }
    // Retain the original snapshot's full-path treatment of tool identifiers.
    // The identifiers are fingerprint data and are never executed as paths.
    std::path::absolute(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.trim().into())
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
    for (index, item) in expected.iter().enumerate() {
        if expected[..index].iter().any(|other| {
            files::ordinal_ignore_case(
                other["RelativePath"].as_str().unwrap_or(""),
                item["RelativePath"].as_str().unwrap_or(""),
            )
        }) {
            return Ok((None, "准备快照包含重复的音源路径".into()));
        }
    }
    for source in &sources {
        let found = expected.iter().find(|item| {
            files::ordinal_ignore_case(
                item["RelativePath"].as_str().unwrap_or(""),
                &relative(root, source),
            )
        });
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
