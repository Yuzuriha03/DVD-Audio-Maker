//! Configuration evaluation shared by the desktop and development entrypoints.
use crate::config::trim;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// Return the production worker count: two workers per detected logical
/// processor, with a minimum of one and an optional work-item cap.
pub fn worker_count(work_items: usize) -> usize {
    let processors = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    processors.saturating_mul(2).max(1).min(work_items.max(1))
}

pub fn combine(left: &str, right: &str) -> String {
    if left.is_empty()
        || right.starts_with(['/', '\\'])
        || (right.as_bytes().get(1) == Some(&b':') && right.as_bytes()[0].is_ascii_alphabetic())
    {
        return right.to_owned();
    }
    if right.is_empty() {
        return left.to_owned();
    }
    if left.ends_with(['/', '\\']) {
        format!("{left}{right}")
    } else {
        format!("{left}\\{right}")
    }
}

pub fn defaults(local_app_data: &str) -> Map<String, Value> {
    let mut result = Map::new();
    for (key, value) in [
        ("DVDA_SRC", ""),
        ("DVDA_FINAL_DIR", ""),
        ("DVDA_TITLE", "DVD-Audio"),
        ("DVDA_ISO_PREFIX", ""),
        ("DVDA_GROUP_TRACK_LIMIT", "99"),
        ("DVDA_DISC_BYTES", ""),
        ("DVDA_MLP_SOURCE", "surcode-batch"),
        ("DVDA_MLP_EXTERNAL_DIR", ""),
        ("DVDA_MLP_METADATA_CONTEXT", ""),
        ("DVDA_MLP_SURCODE_SAMPLE_RATE", "48000"),
        ("DVDA_MLP_SURCODE_BITS", "24"),
        ("DVDA_MENU", "off"),
        ("DVDA_MENU_TRACKS_PER_PAGE", "12"),
        ("DVDA_MENU_INDEX_MIN_ALBUMS", "4"),
        ("DVDA_MENU_STILLPICS", "on"),
        ("DVDA_MENU_COVER_DIM", "35"),
        ("DVDA_MENU_FONT", "Noto-Sans-CJK-SC"),
        ("DVDA_MENU_FONT_JP", ""),
        ("DVDA_MENU_FONT_KR", ""),
        ("DVDA_AUTHOR", "dvda-author-dev.exe"),
        ("DVDA_AUTHOR_SRC", ""),
        ("DVDA_PREPARE_CACHE", "on"),
        ("DVDA_RESUME", "on"),
        ("DVDA_LOSS_ERROR_S", "0.05"),
        ("DVDA_LOSS_WARN_S", "0.005"),
    ] {
        result.insert(key.into(), json!(value));
    }
    result.insert(
        "DVDA_BUILD_DIR".into(),
        json!(combine(
            &combine(local_app_data, "DVD-Audio-Maker"),
            "build"
        )),
    );
    result
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Request {
    pub config_path: Option<String>,
    pub file_values: Map<String, Value>,
    pub environment: Map<String, Value>,
    pub defaults: Map<String, Value>,
    pub positive_sign: String,
    pub negative_sign: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct OptionValue {
    value: Value,
    error: Option<String>,
}
impl OptionValue {
    fn new(value: impl Serialize) -> Self {
        Self {
            value: json!(value),
            error: None,
        }
    }
    fn failure(message: &str) -> Self {
        Self {
            value: Value::Null,
            error: Some(message.to_owned()),
        }
    }
}

impl Request {
    pub fn get(&self, key: &str) -> &str {
        self.get_or(key, "")
    }
    pub fn get_or<'a>(&'a self, key: &str, fallback: &'a str) -> &'a str {
        self.environment
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .or_else(|| {
                self.file_values
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|v| !v.is_empty())
            })
            .or_else(|| self.defaults.get(key).and_then(Value::as_str))
            .unwrap_or(fallback)
    }
    fn integer(&self, key: &str) -> Option<i64> {
        integer(self.get(key), &self.positive_sign, &self.negative_sign)
    }
    fn int(&self, key: &str, fallback: i32) -> i32 {
        if self.get(key).eq_ignore_ascii_case("auto") {
            return fallback;
        }
        self.integer(key)
            .and_then(|v| i32::try_from(v).ok())
            .unwrap_or(fallback)
    }
    fn flag(&self, key: &str, default_on: bool) -> bool {
        let value = trim(self.get_or(key, if default_on { "on" } else { "" })).to_ascii_lowercase();
        if default_on {
            !matches!(value.as_str(), "off" | "0" | "false" | "no")
        } else {
            matches!(value.as_str(), "on" | "1" | "true" | "yes")
        }
    }
    fn source(&self, key: &str) -> &str {
        if self
            .environment
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            "环境变量"
        } else if self
            .file_values
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            self.config_path
                .as_deref()
                .unwrap_or("settings.json")
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or("")
        } else {
            "默认值"
        }
    }
}

fn integer(text: &str, positive: &str, negative: &str) -> Option<i64> {
    let text = trim(text)
        .trim_end_matches('\0')
        .trim_end_matches(|c: char| c == ' ' || ('\t'..='\r').contains(&c));
    let (negative_value, digits) = if !positive.is_empty() && text.starts_with(positive) {
        (false, &text[positive.len()..])
    } else if !negative.is_empty() && text.starts_with(negative) {
        (true, &text[negative.len()..])
    } else if negative == "−" && text.starts_with('-') {
        (true, &text[1..])
    } else {
        (false, text)
    };
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let magnitude = digits.parse::<u64>().ok()?;
    if negative_value {
        i64::try_from(-(magnitude as i128)).ok()
    } else {
        i64::try_from(magnitude).ok()
    }
}

fn real(text: &str, fallback: f64) -> f64 {
    let text = trim(text);
    match text.to_ascii_lowercase().as_str() {
        "nan" | "+nan" | "-nan" => f64::from_bits(0xfff8_0000_0000_0000),
        "infinity" | "+infinity" => f64::INFINITY,
        "-infinity" => f64::NEG_INFINITY,
        "inf" | "+inf" | "-inf" => fallback,
        _ => {
            let numeric = text
                .trim_end_matches('\0')
                .trim_end_matches(|c: char| c == ' ' || ('\t'..='\r').contains(&c));
            if numeric
                .bytes()
                .any(|c| !matches!(c, b'0'..=b'9' | b'.' | b'+' | b'-' | b'e' | b'E'))
            {
                fallback
            } else {
                numeric.parse::<f64>().unwrap_or(fallback)
            }
        }
    }
}

fn prefix(title: &str) -> String {
    let mut result = String::new();
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            result.push(ch);
        } else if !result.ends_with('_') {
            result.push('_');
        }
    }
    let result = result.trim_matches('_');
    if result.is_empty() {
        "DVD_Audio".into()
    } else {
        result.into()
    }
}

pub fn evaluate(request: Request) -> Value {
    let r = &request;
    let mut properties = Map::new();
    let mut put = |name: &str, value: OptionValue| {
        properties.insert(name.into(), json!(value));
    };
    put("ConfigPath", OptionValue::new(&r.config_path));
    for (name, key) in [
        ("SourceDirectory", "DVDA_SRC"),
        ("FinalDirectory", "DVDA_FINAL_DIR"),
        ("BuildDirectory", "DVDA_BUILD_DIR"),
        ("AuthorSource", "DVDA_AUTHOR_SRC"),
    ] {
        put(name, OptionValue::new(r.get(key).trim_end_matches('/')));
    }
    for (name, key) in [("Title", "DVDA_TITLE"), ("DvdaAuthor", "DVDA_AUTHOR")] {
        put(name, OptionValue::new(r.get(key)));
    }
    let iso_prefix = r.get("DVDA_ISO_PREFIX");
    put(
        "IsoPrefix",
        OptionValue::new(if iso_prefix.is_empty() {
            prefix(r.get("DVDA_TITLE"))
        } else {
            iso_prefix.into()
        }),
    );
    let build = r.get("DVDA_BUILD_DIR").trim_end_matches('/');
    for (name, file) in [
        ("ManifestPath", "manifest.json"),
        ("ReportPath", "decode_report.txt"),
        ("OutputRoot", "out"),
        ("TemporaryRoot", "tmp"),
        ("IsoDirectory", "iso"),
        ("MlpDirectory", "mlp"),
        ("MlpIndexPath", "mlp_index.json"),
        ("AlacFixDirectory", "alacfix"),
        ("MenuDirectory", "menu"),
        ("BuildLogPath", "build.log"),
        ("PrepareCachePath", "prepare-cache.json"),
        ("PrepareSnapshotPath", "prepare-snapshot.json"),
    ] {
        put(name, OptionValue::new(combine(build, file)));
    }
    let author = r.get("DVDA_AUTHOR_SRC").replace('\\', "/");
    let author = author.trim_end_matches('/');
    let parent = match author.rfind('/') {
        None => ".",
        Some(0) => "/",
        Some(i) => &author[..i],
    };
    put(
        "MenuBinaryDirectory",
        OptionValue::new(if parent.contains('/') {
            format!("{}/menu-bin", parent.trim_end_matches('/'))
        } else {
            combine(parent, "menu-bin")
        }),
    );
    for (name, key, default_on) in [
        ("PrepareCacheEnabled", "DVDA_PREPARE_CACHE", true),
        ("ResumeEnabled", "DVDA_RESUME", true),
        ("MenuEnabled", "DVDA_MENU", false),
        ("MenuStillPictures", "DVDA_MENU_STILLPICS", false),
        ("KeepTemporary", "DVDA_KEEP_TMP", false),
        ("KeepIntermediate", "DVDA_KEEP_INTERMEDIATE", false),
    ] {
        put(name, OptionValue::new(r.flag(key, default_on)));
    }
    for (name, key, fallback, low, high) in [
        ("GroupTrackLimit", "DVDA_GROUP_TRACK_LIMIT", 99, 1, 99),
        (
            "MlpSurcodeSampleRate",
            "DVDA_MLP_SURCODE_SAMPLE_RATE",
            48000,
            i32::MIN,
            i32::MAX,
        ),
        (
            "MlpSurcodeBits",
            "DVDA_MLP_SURCODE_BITS",
            24,
            i32::MIN,
            i32::MAX,
        ),
        ("MenuTracksPerPage", "DVDA_MENU_TRACKS_PER_PAGE", 12, 1, 32),
        (
            "MenuIndexMinimumAlbums",
            "DVDA_MENU_INDEX_MIN_ALBUMS",
            4,
            0,
            i32::MAX,
        ),
        ("MenuCoverDim", "DVDA_MENU_COVER_DIM", 35, 0, 100),
    ] {
        put(
            name,
            OptionValue::new(r.int(key, fallback).clamp(low, high)),
        );
    }
    put("PlannedDiscs", OptionValue::new(0));
    put(
        "DiscBytes",
        OptionValue::new(
            r.integer("DVDA_DISC_BYTES")
                .filter(|v| *v != 0)
                .unwrap_or(4_707_319_808),
        ),
    );
    put(
        "DiagnosticAlbumLimit",
        OptionValue::new(Some(r.int("DVDA_ALBUM_LIMIT", 0)).filter(|v| *v > 0)),
    );
    let mode = trim(r.get_or("DVDA_TITLE_MODE", "album")).to_ascii_lowercase();
    let mode = if matches!(mode.as_str(), "album" | "one") {
        mode
    } else {
        integer(&mode, &r.positive_sign, &r.negative_sign)
            .filter(|n| *n > 0 && *n <= i32::MAX as i64)
            .map(|n| n.to_string())
            .unwrap_or("album".into())
    };
    put("DiagnosticTitleMode", OptionValue::new(mode));
    for (name, key, fallback) in [
        ("LossErrorSeconds", "DVDA_LOSS_ERROR_S", 0.05),
        ("LossWarningSeconds", "DVDA_LOSS_WARN_S", 0.005),
    ] {
        put(
            name,
            OptionValue::new(real(r.get(key), fallback).to_bits() as i64),
        );
    }
    for (name, key, slash) in [
        ("MlpBatchTempDirectory", "DVDA_MLP_BATCH_TEMP_DIR", true),
        ("MlpBatchOutputDirectory", "DVDA_MLP_BATCH_OUTPUT_DIR", true),
        ("MlpMetadataContext", "DVDA_MLP_METADATA_CONTEXT", false),
        ("MenuFont", "DVDA_MENU_FONT", false),
        ("MenuFontJapanese", "DVDA_MENU_FONT_JP", false),
        ("MenuFontKorean", "DVDA_MENU_FONT_KR", false),
    ] {
        let value = trim(r.get(key));
        put(
            name,
            OptionValue::new(if slash {
                value.trim_end_matches('/')
            } else {
                value
            }),
        );
    }
    let source = trim(r.get("DVDA_MLP_SOURCE")).to_ascii_lowercase();
    let source = match source.as_str() {
        "external" | "surcode" => Ok("external"),
        "surcode-batch" | "batch-surcode" => Ok("surcode-batch"),
        "lpcm" => Ok("lpcm"),
        "ffmpeg" => Err(
            "FFmpeg MLP 编码分支已移除，请将 DVDA_MLP_SOURCE 改为 surcode-batch、lpcm 或 external。",
        ),
        _ => Err("DVDA_MLP_SOURCE 仅支持 surcode-batch、lpcm 或 external（外部文件）。"),
    };
    put(
        "MlpSource",
        match source {
            Ok(v) => OptionValue::new(v),
            Err(e) => OptionValue::failure(e),
        },
    );
    let external = trim(r.get("DVDA_MLP_EXTERNAL_DIR")).trim_end_matches('/');
    put(
        "MlpExternalDirectory",
        if !external.is_empty() {
            OptionValue::new(external)
        } else {
            match source {
                Ok("surcode-batch") => OptionValue::new(combine(build, "mlp")),
                Ok(_) => OptionValue::new(""),
                Err(e) => OptionValue::failure(e),
            }
        },
    );
    let keys: BTreeSet<_> = r
        .defaults
        .keys()
        .chain(r.file_values.keys())
        .cloned()
        .collect();
    let mut keys: Vec<_> = keys.into_iter().collect();
    keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    let all_keys: BTreeSet<_> = keys.iter().chain(r.environment.keys()).collect();
    let raw: Map<String, Value> = all_keys
        .iter()
        .map(|k| ((*k).clone(), json!(r.get(k))))
        .collect();
    let sources: Map<String, Value> = all_keys
        .iter()
        .map(|k| ((*k).clone(), json!(r.source(k))))
        .collect();
    let mut overrides: Vec<_> = r
        .environment
        .iter()
        .filter(|(_, v)| v.as_str().is_some_and(|v| !v.is_empty()))
        .map(|(k, _)| k)
        .collect();
    overrides.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    let missing: Vec<_> = ["DVDA_SRC", "DVDA_FINAL_DIR"]
        .into_iter()
        .filter(|k| r.get(k).is_empty())
        .collect();
    json!({"Properties": properties, "Raw": raw, "Sources": sources, "EffectiveKeys": keys, "Overrides": overrides, "MissingRequired": missing})
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "options.defaults" => Ok(json!(defaults(
            request
                .as_str()
                .ok_or("Expected local application data path")?
        ))),
        "options.evaluate" => Ok(evaluate(
            serde_json::from_value(request).map_err(|e| e.to_string())?,
        )),
        "shell.assignment" => {
            let key = request["Key"].as_str().ok_or("Missing shell key")?;
            let value = request["Value"].as_str().ok_or("Missing shell value")?;
            Ok(json!(format!(
                "{key}='{}'",
                value.replace(char::from(39), "'\\''")
            )))
        }
        _ => Err(format!("Unsupported options operation: {operation}")),
    }
}
