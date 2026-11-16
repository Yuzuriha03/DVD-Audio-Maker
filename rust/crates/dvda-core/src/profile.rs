//! Shared GUI profile contract: editable keys, allowed values and validation.
//!
//! Both GUIs compose the same profile JSON. Capacity bytes, allowed sample
//! rates, bit depths and the LPCM 20-bit rejection live here once; the shells
//! only hold form/widget state and localize the returned issue codes.

use serde_json::{Map, Value, json};

/// DVD5 capacity in bytes.
pub const DVD5_BYTES: i64 = 4_707_319_808;
/// DVD9 capacity in bytes.
pub const DVD9_BYTES: i64 = 8_540_123_136;
pub const SAMPLE_RATES: &[i64] = &[44_100, 48_000, 88_200, 96_000, 176_400, 192_000];
pub const MLP_BITS: &[i64] = &[16, 20, 24];
pub const LPCM_BITS: &[i64] = &[16, 24];
pub const MENU_NORMS: &[&str] = &["pal", "secam", "ntsc"];
pub const AUDIO_MODES: &[&str] = &["mlp-batch", "lpcm", "external"];
pub const LANGUAGES: &[&str] = &["zh-CN", "zh-TW", "en", "ja", "ko"];

/// Capacity combo indices shared by every GUI.
pub const CAPACITY_DVD5: i64 = 0;
pub const CAPACITY_DVD9: i64 = 1;
pub const CAPACITY_CUSTOM: i64 = 2;

/// Disk capacity from a combo index and the optional custom byte count.
#[must_use]
pub fn disc_bytes(capacity: i64, custom_bytes: Option<i64>) -> i64 {
    match capacity {
        CAPACITY_DVD9 => DVD9_BYTES,
        CAPACITY_CUSTOM => custom_bytes
            .filter(|value| *value > 0)
            .unwrap_or(DVD5_BYTES),
        _ => DVD5_BYTES,
    }
}

/// Combo index for a stored capacity value.
#[must_use]
pub fn capacity_index(disc_bytes: i64) -> i64 {
    match disc_bytes {
        DVD9_BYTES => CAPACITY_DVD9,
        DVD5_BYTES => CAPACITY_DVD5,
        value if value <= 0 => CAPACITY_DVD5,
        _ => CAPACITY_CUSTOM,
    }
}

/// Editable profile defaults both GUIs start from.
#[must_use]
pub fn gui_defaults() -> Map<String, Value> {
    let mut defaults = crate::options::defaults("");
    // Contract: the evaluator defaults are the source for user data paths (all
    // folders stay empty here so Android can require an explicit SAF grant),
    // while the GUI-controlled option values below are intentionally overridden
    // for the FRB/Android host defaults. Insert (not or_insert) after the
    // documentation above so these values actually take effect.
    for (key, value) in [
        ("DVDA_TITLE_MODE", "album"),
        ("DVDA_ALBUM_LIMIT", "0"),
        ("DVDA_DITHER_16", "on"),
        ("DVDA_KEEP_TMP", "off"),
        ("DVDA_KEEP_INTERMEDIATE", "off"),
        ("DVDA_RESUME", "on"),
    ] {
        defaults.insert(key.to_owned(), Value::String(value.to_owned()));
    }
    defaults
}

/// Localized, user-facing text for a profile validation issue code.
///
/// Uses the one shared localization catalog (Chinese source -> all languages)
/// so a GUI never shows a raw code such as `(invalid_bits)`.
#[must_use]
pub fn issue_message(language: &str, code: &str) -> String {
    let source = match code {
        "invalid_bits" => "MLP 位深必须为 16、20 或 24；LPCM 位深必须为 16 或 24。",
        "invalid_sample_rate" => "MLP 目标采样率无效。",
        _ => return code.to_owned(),
    };
    crate::localization::translate(language, source)
}

/// One JSON payload describing schema defaults and allowed values for a GUI.
#[must_use]
pub fn gui_options() -> Value {
    json!({
        "defaults": gui_defaults(),
        "sampleRates": SAMPLE_RATES,
        "mlpBits": MLP_BITS,
        "lpcmBits": LPCM_BITS,
        "menuNorms": MENU_NORMS,
        "audioModes": AUDIO_MODES,
        "languages": LANGUAGES,
        "capacity": {
            "dvd5": DVD5_BYTES,
            "dvd9": DVD9_BYTES,
            "dvd5Index": CAPACITY_DVD5,
            "dvd9Index": CAPACITY_DVD9,
            "customIndex": CAPACITY_CUSTOM,
        },
    })
}

/// One profile validation problem; GUIs localize `code` themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileIssue {
    /// Profile key that owns the problem, e.g. `DVDA_MLP_BITS`.
    pub key: &'static str,
    /// Stable code the shells translate, e.g. `invalid_bits`.
    pub code: &'static str,
}

/// Validates the audio rate/bit-depth pair. The LPCM rules match the desktop
/// shell exactly: LPCM accepts 16/24 only, so 20-bit LPCM is an error and is
/// never silently rewritten.
#[must_use]
pub fn validate_audio(rate: i64, bits: i64, lpcm: bool) -> Option<ProfileIssue> {
    if !SAMPLE_RATES.contains(&rate) {
        return Some(ProfileIssue {
            key: "DVDA_MLP_SAMPLE_RATE",
            code: "invalid_sample_rate",
        });
    }
    if !MLP_BITS.contains(&bits) || (lpcm && !LPCM_BITS.contains(&bits)) {
        return Some(ProfileIssue {
            key: "DVDA_MLP_BITS",
            code: "invalid_bits",
        });
    }
    None
}

fn integer(values: &Map<String, Value>, key: &str) -> Option<i64> {
    let value = values.get(key)?;
    value.as_i64().or_else(|| {
        value
            .as_str()
            .and_then(|text| text.trim().parse::<i64>().ok())
    })
}

/// Validates a composed profile `Values` object for a task run.
///
/// Mirrors the desktop control validation: rate and bit depth are checked when
/// a task is about to start, and LPCM rejects 20-bit input with `invalid_bits`.
#[must_use]
pub fn validate_values(values: &Map<String, Value>, for_task: bool) -> Vec<ProfileIssue> {
    if !for_task {
        return Vec::new();
    }
    let lpcm = values
        .get("DVDA_MLP_SOURCE")
        .and_then(Value::as_str)
        .is_some_and(|mode| mode == "lpcm");
    let rate = integer(values, "DVDA_MLP_SAMPLE_RATE");
    let bits = integer(values, "DVDA_MLP_BITS");
    match (rate, bits) {
        (Some(rate), Some(bits)) => validate_audio(rate, bits, lpcm).into_iter().collect(),
        (None, _) | (_, None) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_mapping_round_trips() {
        assert_eq!(disc_bytes(CAPACITY_DVD5, None), DVD5_BYTES);
        assert_eq!(disc_bytes(CAPACITY_DVD9, None), DVD9_BYTES);
        assert_eq!(disc_bytes(CAPACITY_CUSTOM, Some(1_234_567)), 1_234_567);
        assert_eq!(disc_bytes(CAPACITY_CUSTOM, Some(0)), DVD5_BYTES);
        assert_eq!(disc_bytes(CAPACITY_CUSTOM, None), DVD5_BYTES);
        assert_eq!(capacity_index(DVD5_BYTES), CAPACITY_DVD5);
        assert_eq!(capacity_index(DVD9_BYTES), CAPACITY_DVD9);
        assert_eq!(capacity_index(1_234_567), CAPACITY_CUSTOM);
        assert_eq!(capacity_index(0), CAPACITY_DVD5);
    }

    #[test]
    fn audio_validation_matches_the_desktop_rules() {
        assert!(validate_audio(48_000, 24, false).is_none());
        assert!(validate_audio(44_100, 20, false).is_none());
        assert_eq!(
            validate_audio(48_000, 20, true).map(|issue| issue.code),
            Some("invalid_bits")
        );
        assert_eq!(
            validate_audio(32_000, 24, false).map(|issue| issue.code),
            Some("invalid_sample_rate")
        );
        assert_eq!(
            validate_audio(48_000, 32, false).map(|issue| issue.code),
            Some("invalid_bits")
        );
    }

    #[test]
    fn values_validation_only_runs_for_a_task() {
        let values = json!({
            "DVDA_MLP_SOURCE": "lpcm",
            "DVDA_MLP_SAMPLE_RATE": "48000",
            "DVDA_MLP_BITS": "20",
        });
        let map = values.as_object().unwrap().clone();
        assert_eq!(validate_values(&map, false).len(), 0);
        let issues = validate_values(&map, true);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, "DVDA_MLP_BITS");
        assert_eq!(issues[0].code, "invalid_bits");
    }

    #[test]
    fn gui_defaults_override_gui_options_but_keep_folders_empty() {
        let defaults = gui_defaults();
        // Both GUIs must serialize the platform-neutral default; the shared
        // language/font resolver in `app::default_menu_font` picks the real font.
        assert_eq!(defaults["DVDA_MENU_FONT"], "");
        assert_eq!(defaults["DVDA_RESUME"], "on");
        assert_eq!(defaults["DVDA_MENU_FONT_JP"], "");
        assert_eq!(defaults["DVDA_SRC"], "");
        assert_eq!(defaults["DVDA_FINAL_DIR"], "");
        let build_dir = defaults["DVDA_BUILD_DIR"].as_str().unwrap_or_default();
        assert!(build_dir.contains("DVD-Audio-Maker"), "{build_dir}");
        assert!(build_dir.ends_with("build"), "{build_dir}");
    }

    #[test]
    fn issue_messages_are_localized_in_all_five_languages() {
        for language in ["zh-CN", "zh-TW", "en", "ja", "ko"] {
            let bits = issue_message(language, "invalid_bits");
            assert!(bits.contains("16"), "{language}: {bits}");
            assert!(!bits.contains("invalid_bits"), "{language}: {bits}");
            let rate = issue_message(language, "invalid_sample_rate");
            assert!(!rate.contains("invalid_sample_rate"), "{language}: {rate}");
        }
        assert_eq!(
            issue_message("en", "invalid_bits"),
            "MLP bit depth must be 16, 20, or 24; LPCM bit depth must be 16 or 24."
        );
        assert_eq!(
            issue_message("zh-CN", "invalid_sample_rate"),
            "MLP 目标采样率无效。"
        );
    }

    #[test]
    fn gui_options_expose_defaults_and_allowed_values() {
        let options = gui_options();
        assert_eq!(options["capacity"]["dvd5"], DVD5_BYTES);
        assert_eq!(options["sampleRates"][0], 44_100);
        assert_eq!(options["lpcmBits"][1], 24);
        assert_eq!(options["defaults"]["DVDA_TITLE"], "DVD-Audio");
        assert_eq!(options["defaults"]["DVDA_GROUP_TRACK_LIMIT"], "99");
    }
}
