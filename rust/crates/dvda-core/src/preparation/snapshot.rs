//! Persistent preparation evidence shared with the original application.
use super::state;
use crate::{audio::Metadata, media::Failure};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fmt,
    io::Read,
    path::{Path, PathBuf},
};

/// Invariant Double.ToString("R") spelling used by historical cache signatures.
pub fn roundtrip(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Infinity"
        } else {
            "Infinity"
        }
        .into();
    }
    // Rust Display chooses the upper decimal at some shortest-decimal ties.
    // The already-pinned JSON formatter chooses the even decimal like the old
    // host; adjust notation only, retaining those exact significant digits.
    let decimal = serde_json::to_string(&value).unwrap();
    let unsigned = decimal.trim_start_matches('-');
    let (mantissa, exponent) = unsigned.split_once('e').unwrap_or((unsigned, "0"));
    let point = mantissa.find('.').unwrap_or(mantissa.len());
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let significant = digits.trim_start_matches('0');
    let sign = if value.is_sign_negative() { "-" } else { "" };
    if significant.is_empty() {
        return format!("{sign}0");
    }
    let exponent: i32 = exponent.parse::<i32>().unwrap() + point as i32
        - (digits.len() - significant.len()) as i32
        - 1;
    let significant = significant.trim_end_matches('0');
    if (-4..17).contains(&exponent) {
        let point = exponent + 1;
        if point <= 0 {
            format!("{sign}0.{}{significant}", "0".repeat((-point) as usize))
        } else if point as usize >= significant.len() {
            format!(
                "{sign}{significant}{}",
                "0".repeat(point as usize - significant.len())
            )
        } else {
            format!(
                "{sign}{}.{}",
                &significant[..point as usize],
                &significant[point as usize..]
            )
        }
    } else {
        let significand = if significant.len() == 1 {
            format!("{sign}{significant}")
        } else {
            format!("{sign}{}.{}", &significant[..1], &significant[1..])
        };
        format!("{significand}E{exponent:+03}")
    }
}

// Stream the property names so duplicate and differently cased properties use
// their last occurrence, matching System.Text.Json's case-insensitive reader.
macro_rules! snapshot_record {
    ($name:ident { $( $field:ident : $type:ty = $json:literal ),* $(,)? }) => {
        #[derive(Debug, Default, Serialize)]
        #[serde(rename_all="PascalCase")]
        pub struct $name { $(pub $field: $type),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self,D::Error> {
                struct Fields;
                impl<'de> Visitor<'de> for Fields {
                    type Value=$name;
                    fn expecting(&self, f:&mut fmt::Formatter)->fmt::Result {f.write_str("a snapshot record")}
                    fn visit_map<M:MapAccess<'de>>(self, mut map:M)->Result<Self::Value,M::Error> {
                        let mut value=$name::default();
                        while let Some(key)=map.next_key::<String>()? {
                            match key.to_ascii_lowercase().as_str() {
                                $($json=>value.$field=map.next_value()?,)*
                                _=>{map.next_value::<serde::de::IgnoredAny>()?;},
                            }
                        }
                        Ok(value)
                    }
                }
                deserializer.deserialize_map(Fields)
            }
        }
    }
}
snapshot_record!(Identity {
    path:Option<String>="path",size:i64="size",last_write_utc_ticks:i64="lastwriteutcticks",
    head_hash:Option<String>="headhash",tail_hash:Option<String>="tailhash",
});
snapshot_record!(Source {relative_path:Option<String>="relativepath",identity:Option<Identity>="identity"});
snapshot_record!(Snapshot {
    version:i32="version",source_directory:Option<String>="sourcedirectory",
    configuration_fingerprint:Option<String>="configurationfingerprint",
    sources:Option<Vec<Option<Source>>>="sources",manifest:Option<Identity>="manifest",
});
pub fn load(path: &Path) -> Option<Value> {
    let mut bytes = Vec::new();
    crate::identity::open_read(&path.to_string_lossy())
        .ok()?
        .read_to_end(&mut bytes)
        .ok()?;
    let text = crate::config_files::decode_text(&bytes);
    let value: Option<Snapshot> = serde_json::from_str(&text).ok()?;
    value.map(|v| json!(v))
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Request {
    path: PathBuf,
    root: String,
    fingerprint: String,
    manifest: String,
    #[serde(default)]
    sources: Vec<String>,
}
pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    if operation == "prepare.normalize_albums" {
        let mut tracks: Vec<Metadata> =
            serde_json::from_value(request).map_err(|e| e.to_string())?;
        super::rules::normalize_albums(&mut tracks);
        return Ok(json!(tracks));
    }
    if operation == "prepare.roundtrip" {
        let bits: Vec<u64> = serde_json::from_value(request).map_err(|e| e.to_string())?;
        return Ok(json!(
            bits.into_iter()
                .map(|bits| roundtrip(f64::from_bits(bits)))
                .collect::<Vec<_>>()
        ));
    }
    let request: Request = serde_json::from_value(request).map_err(|e| e.to_string())?;
    let value:Result<Value,Failure>=match operation {
        "prepare.snapshot_reuse"=>state::reuse_snapshot(&request.path,&request.root,&request.fingerprint,&request.manifest)
            .map(|(snapshot,reason)|json!({"Reused":snapshot.is_some(),"Snapshot":snapshot,"Reason":reason})),
        "prepare.snapshot_save"=>Ok(json!(state::save_snapshot(&request.path,&request.root,&request.fingerprint,&request.manifest,&request.sources))),
        "prepare.enumerate"=>state::enumerate_sources(Path::new(&request.root)).map(|v|json!(v)),
        _=>return Err(format!("Unsupported preparation operation: {operation}")),
    };
    value.map_err(|e| e.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invariant_roundtrip_exponent_boundaries() {
        for (value, expected) in [
            (0., "0"),
            (-0., "-0"),
            (1e-4, "0.0001"),
            (1e-5, "1E-05"),
            (1e16, "10000000000000000"),
            (1e17, "1E+17"),
            (f64::MAX, "1.7976931348623157E+308"),
            (f64::from_bits(1), "5E-324"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
            (f64::from_bits(0x43073f9c2996809a), "817983051190291.2"),
            (f64::from_bits(0x42bc32771e19a310), "31003072403875.062"),
        ] {
            assert_eq!(roundtrip(value), expected);
        }
    }
    #[test]
    fn snapshot_fields_use_last_case_insensitive_property() {
        let value:Snapshot=serde_json::from_str(r#"{"Version":3,"VERSION":2,"Version":1,"sourceDirectory":"中文","Sources":[],"manifest":{"Size":9,"SIZE":7,"Size":0}}"#).unwrap();
        assert_eq!(value.version, 1);
        assert_eq!(value.source_directory.as_deref(), Some("中文"));
        assert_eq!(value.manifest.unwrap().size, 0);
        assert!(serde_json::from_str::<Snapshot>(r#"{"Version":"1"}"#).is_err());
        assert!(serde_json::from_str::<Snapshot>(r#"{"Version":null}"#).is_err());
    }
}
