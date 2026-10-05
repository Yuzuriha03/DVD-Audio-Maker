//! File boundaries for portable GUI profiles.
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::fmt;
use std::{
    fs,
    io::{self, Read, Write},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Failure {
    pub kind: &'static str,
    pub code: Option<i32>,
    pub message: Option<String>,
}
impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self {
            kind: "Io",
            code: Some(error.raw_os_error().unwrap_or(87)),
            message: None,
        }
    }
}
impl Failure {
    fn validation(kind: &'static str, message: &str) -> Self {
        Self {
            kind,
            code: None,
            message: Some(message.into()),
        }
    }
    fn json() -> Self {
        Self {
            kind: "Json",
            code: None,
            message: None,
        }
    }
}

pub fn outcome(result: Result<Value, Failure>) -> Value {
    match result {
        Ok(value) => json!({"Value": value, "Failure": null}),
        Err(failure) => json!({"Value": null, "Failure": failure}),
    }
}

/// Match StreamReader BOM detection. Invalid sequences use replacement text.
pub fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xff, 0xfe, 0, 0]) || bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
        let little = bytes[0] == 0xff;
        let mut text: String = bytes[4..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| {
                char::from_u32(if little {
                    u32::from_le_bytes(*b)
                } else {
                    u32::from_be_bytes(*b)
                })
                .unwrap_or(char::REPLACEMENT_CHARACTER)
            })
            .collect();
        if !(bytes.len() - 4).is_multiple_of(4) {
            text.push(char::REPLACEMENT_CHARACTER);
        }
        text
    } else if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little = bytes[0] == 0xff;
        let units = bytes[2..].as_chunks::<2>().0.iter().map(|b| {
            if little {
                u16::from_le_bytes(*b)
            } else {
                u16::from_be_bytes(*b)
            }
        });
        let mut text: String = char::decode_utf16(units)
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect();
        if !(bytes.len() - 2).is_multiple_of(2) {
            text.push(char::REPLACEMENT_CHARACTER);
        }
        text
    } else {
        String::from_utf8_lossy(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
            .into_owned()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct Profile {
    version: i32,
    language: Option<String>,
    values: Option<Map<String, Value>>,
}

impl<'de> Deserialize<'de> for Profile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ProfileVisitor;
        impl<'de> Visitor<'de> for ProfileVisitor {
            type Value = Profile;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a profile object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Profile, M::Error> {
                let mut profile = Profile {
                    version: 1,
                    language: Some("auto".into()),
                    values: Some(Map::new()),
                };
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "Version" => profile.version = map.next_value()?,
                        "Language" => profile.language = map.next_value()?,
                        "Values" => {
                            profile.values = map.next_value::<Option<ProfileValues>>()?.map(|v| v.0)
                        }
                        _ => {
                            map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(profile)
            }
        }
        deserializer.deserialize_map(ProfileVisitor)
    }
}
struct ProfileValues(Map<String, Value>);
impl<'de> Deserialize<'de> for ProfileValues {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValuesVisitor;
        impl<'de> Visitor<'de> for ValuesVisitor {
            type Value = ProfileValues;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("string or null values")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut values = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    let value = map.next_value::<Option<String>>()?;
                    values.insert(key, json!(value));
                }
                Ok(ProfileValues(values))
            }
        }
        deserializer.deserialize_map(ValuesVisitor)
    }
}

pub fn normalize_language(
    language: Option<&str>,
    installed_language: &str,
) -> Result<String, Failure> {
    let language = crate::config::trim(language.unwrap_or("")).to_ascii_lowercase();
    Ok(match language.as_str() {
        "" | "auto" => match installed_language {
            "zh" => "zh-CN",
            "ja" => "ja",
            _ => "en",
        },
        "zh" | "zh-cn" | "zh-hans" | "中文" => "zh-CN",
        "en" | "en-us" | "en-gb" | "english" => "en",
        "ja" | "ja-jp" | "日本語" | "japanese" => "ja",
        _ => {
            return Err(Failure::validation(
                "Argument",
                "Language must be auto, en, zh-CN, or ja.",
            ));
        }
    }
    .into())
}

fn read_profile(path: &str, defaults: &Value) -> Result<Value, Failure> {
    let mut bytes = Vec::new();
    crate::identity::open_read(path)?.read_to_end(&mut bytes)?;
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    check_json_depth(bytes)?;
    let profile: Option<Profile> = serde_json::from_slice(bytes).map_err(|_| Failure::json())?;
    let profile = profile.ok_or_else(|| Failure::validation("InvalidData", "配置方案为空。"))?;
    if profile.version != 1
        || profile
            .values
            .as_ref()
            .is_none_or(|v| v.values().any(Value::is_null))
    {
        return Err(Failure::validation(
            "InvalidData",
            "配置方案版本或内容无效。",
        ));
    }
    normalize_language(profile.language.as_deref(), "en")?;
    let mut values = defaults.as_object().cloned().ok_or_else(Failure::json)?;
    values.extend(profile.values.unwrap_or_default());
    Ok(json!({"Version": 1, "Language": profile.language, "Values": values}))
}

// System.Text.Json's default nesting limit is 64, including ignored properties.
fn check_json_depth(bytes: &[u8]) -> Result<(), Failure> {
    let mut depth = 0i32;
    let mut quoted = false;
    let mut escaped = false;
    for byte in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if *byte == 92 {
                escaped = true;
            } else if *byte == 34 {
                quoted = false;
            }
        } else {
            match byte {
                34 => quoted = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > 64 {
                        return Err(Failure::json());
                    }
                }
                b'}' | b']' => depth -= 1,
                _ => {}
            }
        }
    }
    Ok(())
}

fn save_profile(path: &str, profile: &Value) -> Result<Value, Failure> {
    if path.is_empty() || path.contains(char::from(0)) {
        return Err(Failure::validation("Argument", "Invalid profile path."));
    }
    let path = std::path::absolute(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| Failure::validation("Argument", "Invalid profile parent."))?;
    fs::create_dir_all(parent)?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let token = format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let mut temp_name = path.as_os_str().to_owned();
    temp_name.push(format!(".{token}.tmp"));
    let temporary = std::path::PathBuf::from(temp_name);
    let result = (|| -> Result<Value, Failure> {
        let text = serde_json::to_vec_pretty(profile).map_err(|_| Failure::json())?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&text)?;
        drop(file);
        dvda_native::files::move_file(&temporary, &path, true)?;
        Ok(Value::Null)
    })();
    if temporary.is_file() {
        fs::remove_file(&temporary)?;
    }
    result
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "profile.load" => {
            let path = request["Path"].as_str().ok_or("Missing profile path")?;
            Ok(outcome(read_profile(path, &request["Defaults"])))
        }
        "profile.save" => {
            let path = request["Path"].as_str().ok_or("Missing profile path")?;
            Ok(outcome(save_profile(path, &request["Profile"])))
        }
        _ => Err(format!(
            "Unsupported configuration file operation: {operation}"
        )),
    }
}
