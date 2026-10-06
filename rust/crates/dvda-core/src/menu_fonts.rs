//! Probe the scripts actually used by a menu through the in-process image ABI.
//! This retains the old representative-rendering test (not a Unicode coverage
//! proof), candidate fallback and optional Japanese/Korean face selection.
use dvda_native::{images::Images, media::Callbacks};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};
const PROBES: [(&str, &str); 6] = [
    ("ASCII", "Ag("),
    ("汉字", "中文"),
    ("平假名", "あいう"),
    ("片假名", "アイウ"),
    ("韩文", "한글"),
    ("CJK标点", "、《》"),
];
const CANDIDATES: [&str; 14] = [
    "DVDA-Noto-Sans-CJK-SC",
    "DVDA-Noto-Sans-CJK-JP",
    "DVDA-Noto-Sans-CJK-KR",
    "Noto-Sans-CJK-SC",
    "Noto-Sans-CJK-JP",
    "Noto-Sans-CJK-KR",
    "Noto-Sans-CJK-TC",
    "NotoSansCJK-Regular",
    "Source-Han-Sans-CN",
    "WenQuanYi-Micro-Hei",
    "WenQuanYi-Zen-Hei",
    "AR-PL-UMing-CN",
    "Droid-Sans-Fallback",
    "DejaVu-Sans",
];
#[derive(Debug)]
pub struct Resolution {
    pub font: String,
    pub japanese: String,
    pub korean: String,
    pub diagnostics: Vec<Value>,
}
fn diagnostic(severity: i32, code: &str, message: String) -> Value {
    json!({"Severity":severity,"Code":code,"Message":message})
}
pub fn needed_scripts(text: &str) -> BTreeSet<String> {
    text.chars()
        .filter_map(|ch| {
            if ch.is_whitespace() {
                return None;
            }
            match ch as u32 {
                0..=0x7f => Some("ASCII"),
                0x3040..=0x309f => Some("平假名"),
                0x30a0..=0x30ff => Some("片假名"),
                0xac00..=0xd7af | 0x1100..=0x11ff => Some("韩文"),
                0x3400..=0x4dbf | 0x4e00..=0x9fff => Some("汉字"),
                0x3000..=0x303f | 0xff00..=0xffef => Some("CJK标点"),
                _ => None,
            }
        })
        .map(str::to_owned)
        .collect()
}
fn normalize_name(font: &str) -> String {
    font.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}
fn font_exists(font: &str, available: &[String]) -> bool {
    if font.trim().is_empty() || font.chars().any(char::is_whitespace) {
        return false;
    }
    if Path::new(font).is_file() {
        return true;
    }
    let target = normalize_name(font);
    available.iter().any(|font| {
        let candidate = normalize_name(font);
        candidate == target
            || !target.is_empty() && (candidate.contains(&target) || target.contains(&candidate))
    })
}
pub fn derive_regional_face(font: &str, tag: &str, exists: impl Fn(&str) -> bool) -> String {
    let candidate = if !font.contains(['/', '\\']) {
        let lower = font.to_ascii_lowercase();
        let Some(start) = lower.find("noto-sans-cjk-") else {
            return String::new();
        };
        let face = start + 14;
        if !["sc", "jp", "kr", "tc", "hk"].contains(&lower.get(face..face + 2).unwrap_or_default())
        {
            return String::new();
        }
        format!(
            "{}{}{}",
            &font[..face],
            tag.to_ascii_uppercase(),
            &font[face + 2..]
        )
    } else {
        let normalized = font.replace('\\', "/");
        let (directory, name) = normalized.rsplit_once('/').unwrap_or(("", &normalized));
        let lower = name.to_ascii_lowercase();
        if !lower.starts_with("notosanscjk")
            || !["sc", "jp", "kr", "tc", "hk"].contains(&lower.get(11..13).unwrap_or_default())
            || !lower.get(13..).unwrap_or_default().starts_with('-')
        {
            return String::new();
        }
        format!("{directory}/{}{tag}{}", &name[..11], &name[13..])
    };
    if exists(&candidate) {
        candidate
    } else {
        String::new()
    }
}
trait Probe {
    fn fonts(&mut self) -> Result<Vec<String>, String>;
    fn ink(&mut self, font: &str, text: &str) -> Result<bool, String>;
}
struct NativeProbe<'a> {
    images: &'a Images,
    directory: &'a Path,
    caller: &'a mut dyn Callbacks,
}
struct Capture<'a> {
    caller: &'a mut dyn Callbacks,
    text: String,
}
impl Callbacks for Capture<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        if stream == 1 {
            self.text.push_str(text);
        } else {
            self.caller.emit(stream, text);
        }
    }
    fn cancelled(&mut self) -> bool {
        self.caller.cancelled()
    }

    fn progress(&mut self, completed: u64, total: u64) {
        self.caller.progress(completed, total);
    }
}
impl NativeProbe<'_> {
    fn run(&mut self, args: &[String]) -> Result<Option<String>, String> {
        if self.caller.cancelled() {
            return Err("Build cancelled".into());
        }
        let mut capture = Capture {
            caller: self.caller,
            text: String::new(),
        };
        let status = self
            .images
            .run(args, &mut capture)
            .map_err(|e| e.to_string())?;
        if capture.cancelled() {
            return Err("Build cancelled".into());
        }
        Ok((status == 0).then_some(capture.text))
    }
}
impl Probe for NativeProbe<'_> {
    fn fonts(&mut self) -> Result<Vec<String>, String> {
        Ok(self
            .run(&["magick".into(), "-list".into(), "font".into()])?
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                line.trim()
                    .strip_prefix("Font:")
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned)
            })
            .collect())
    }
    fn ink(&mut self, font: &str, text: &str) -> Result<bool, String> {
        let path = crate::media::temporary_path(&self.directory.join("font-probe.txt"));
        if let Err(reason) = fs::write(&path, text) {
            let _ = fs::remove_file(&path);
            return Err(reason.to_string());
        }
        let result = self.run(
            &[
                "magick",
                "-size",
                "160x48",
                "xc:none",
                "-font",
                font,
                "-pointsize",
                "20",
                "-fill",
                "white",
                "-annotate",
                "+2+32",
                &format!("@{}", path.to_string_lossy().replace('\\', "/")),
                "-format",
                "%[fx:mean.a]",
                "info:",
            ]
            .map(str::to_owned),
        );
        let cleanup = fs::remove_file(&path).map_err(|e| e.to_string());
        let result = result?;
        cleanup?;
        Ok(result
            .and_then(|value| value.trim().parse::<f64>().ok())
            .is_some_and(|value| value.is_finite() && value > 0.0))
    }
}
fn coverage(
    probe: &mut impl Probe,
    font: &str,
    required: &BTreeSet<String>,
) -> Result<BTreeSet<String>, String> {
    let mut found = BTreeSet::new();
    for (script, text) in PROBES {
        if required.contains(script) && probe.ink(font, text)? {
            found.insert(script.to_owned());
        }
    }
    Ok(found)
}
fn cjk_hint(font: &str) -> bool {
    let font = font.to_ascii_lowercase();
    [
        "cjk", "hei", "ming", "song", "kai", "wqy", "wenquan", "arphic", "ar-pl", "arpl", "droid",
        "han", "zen", "uming", "ukai", "noto",
    ]
    .iter()
    .any(|part| font.contains(part))
}
fn resolve_with(
    probe: &mut impl Probe,
    preferred: &str,
    japanese: &str,
    korean: &str,
    text: &str,
) -> Result<Resolution, String> {
    let required = needed_scripts(text);
    let mut available = probe.fonts()?;
    available.sort_by_key(|font| font.to_ascii_lowercase());
    let mut result = Resolution {
        font: String::new(),
        japanese: String::new(),
        korean: String::new(),
        diagnostics: Vec::new(),
    };
    if preferred.to_ascii_lowercase().ends_with(".ttc") {
        result.diagnostics.push(diagnostic(1,"MENU_FONT_COLLECTION","主字体是 TTC 集合；ImageMagick 按文件路径通常只使用 face 0，Noto Sans CJK 可能因此把中文显示为日文字形。建议使用独立 SC OTF。".into()));
    }
    let mut missing = required.clone();
    if !preferred.is_empty() {
        if font_exists(preferred, &available) {
            missing = required
                .difference(&coverage(probe, preferred, &required)?)
                .cloned()
                .collect();
            if missing.is_empty() {
                result.font = preferred.into();
            } else {
                result.diagnostics.push(diagnostic(
                    1,
                    "MENU_FONT_INCOMPLETE",
                    format!(
                        "字体 {preferred} 实际画不出 {}，正在寻找候选字体。",
                        missing.iter().cloned().collect::<Vec<_>>().join("、")
                    ),
                ));
            }
        } else {
            result.diagnostics.push(diagnostic(
                1,
                "MENU_FONT_UNAVAILABLE",
                format!("字体不可用或名称含空格，正在寻找候选字体: {preferred}"),
            ));
        }
    }
    if result.font.is_empty() && !required.is_empty() {
        let mut seen = BTreeSet::new();
        let mut best_score = None;
        for font in CANDIDATES
            .iter()
            .map(|font| font.to_string())
            .chain(available.iter().cloned())
        {
            if !cjk_hint(&font)
                || !seen.insert(font.to_ascii_lowercase())
                || !font_exists(&font, &available)
            {
                continue;
            }
            let coverage = coverage(probe, &font, &required)?;
            let candidate_missing = required
                .difference(&coverage)
                .cloned()
                .collect::<BTreeSet<_>>();
            if best_score.is_none_or(|score| coverage.len() > score) {
                best_score = Some(coverage.len());
                result.font = font;
                missing = candidate_missing;
            }
            if missing.is_empty() {
                break;
            }
        }
    } else if required.is_empty() {
        result.font = preferred.into();
        missing.clear();
    }
    if result.font.is_empty() {
        result.diagnostics.push(diagnostic(
            2,
            "MENU_FONT_MISSING",
            "找不到能渲染菜单文字的字体；请提供可用的 CJK 字体。".into(),
        ));
    } else if !missing.is_empty() {
        result.diagnostics.push(diagnostic(
            2,
            "MENU_FONT_GLYPHS_MISSING",
            format!(
                "字体 {} 仍无法渲染: {}。",
                result.font,
                missing.iter().cloned().collect::<Vec<_>>().join("、")
            ),
        ));
    }
    for (configured, tag, label, script) in [
        (japanese, "jp", "日文", "平假名"),
        (korean, "kr", "韩文", "韩文"),
    ] {
        let candidate = if configured.is_empty() {
            derive_regional_face(&result.font, tag, |font| font_exists(font, &available))
        } else {
            configured.to_owned()
        };
        if candidate.is_empty() {
            continue;
        }
        if !font_exists(&candidate, &available) {
            result.diagnostics.push(diagnostic(
                1,
                "MENU_REGIONAL_FONT_UNAVAILABLE",
                format!("{label}字体不可用，继续使用主字体: {candidate}"),
            ));
            continue;
        }
        if !coverage(probe, &candidate, &BTreeSet::from([script.to_owned()]))?.contains(script) {
            result.diagnostics.push(diagnostic(
                1,
                "MENU_REGIONAL_FONT_INCOMPLETE",
                format!("{label}字体无法实际渲染{script}，继续使用主字体: {candidate}"),
            ));
            continue;
        }
        if tag == "jp" {
            result.japanese = candidate;
        } else {
            result.korean = candidate;
        }
    }
    Ok(result)
}
pub fn resolve(
    images: &Images,
    directory: &Path,
    preferred: [&str; 3],
    text: &str,
    caller: &mut dyn Callbacks,
) -> Result<Resolution, String> {
    resolve_with(
        &mut NativeProbe {
            images,
            directory,
            caller,
        },
        preferred[0],
        preferred[1],
        preferred[2],
        text,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        fonts: Vec<String>,
        ink: Vec<(String, String)>,
    }
    impl Probe for Fake {
        fn fonts(&mut self) -> Result<Vec<String>, String> {
            Ok(self.fonts.clone())
        }
        fn ink(&mut self, font: &str, text: &str) -> Result<bool, String> {
            Ok(self.ink.contains(&(font.into(), text.into())))
        }
    }
    #[test]
    fn font_and_asset_warnings_are_translated_in_every_language() {
        for message in [
            "字体不可用或名称含空格，正在寻找候选字体: missing family",
            "字体 font.ttf 实际画不出 平假名，正在寻找候选字体。",
            "找不到能渲染菜单文字的字体；请提供可用的 CJK 字体。",
            "字体 font.ttf 仍无法渲染: 平假名。",
            "日文字体不可用，继续使用主字体: missing",
            "韩文字体无法实际渲染韩文，继续使用主字体: missing",
            "1 张专辑没有 cover.jpg/jpeg/png/webp，页面将使用黑色背景: 专辑",
            "一级菜单按位置解释封面清单，不能压缩缺项；缺少封面的专辑: 专辑",
            "索引封面清单 0 项 != 索引格子 1 项。",
            "生成专辑 专辑 的播放静图失败: image failure",
            "[警告] 续跑记录无法读取，将重新出盘: corrupt JSON",
            "内置图像组件缺失，请重新解压或修复发布包。",
            "内置图像组件缺失，无法验证菜单图片尺寸。",
            "内置菜单组件缺失：dvda-menu-nav.dll。请重新解压或修复发布包。",
            "内置图像组件缺失，无法完成菜单文字与高亮检查。",
            "找不到 dvda-author 菜单素材目录: C:\\中文\\menu",
        ] {
            for language in ["en", "ja"] {
                assert_ne!(
                    crate::localization::translate(language, message),
                    message,
                    "{language}: {message}"
                );
            }
        }
    }
    #[test]
    fn scripts_regions_fallback_and_failure_severity_match_old_resolver() {
        assert_eq!(needed_scripts("A 中文 あいう アイウ 한글 、\n").len(), 6);
        assert_eq!(
            derive_regional_face("DVDA-Noto-Sans-CJK-SC", "jp", |_| true),
            "DVDA-Noto-Sans-CJK-JP"
        );
        assert_eq!(
            derive_regional_face("C:\\fonts\\NotoSansCJKsc-Regular.otf", "kr", |_| true),
            "C:/fonts/NotoSansCJKkr-Regular.otf"
        );
        let mut probe = Fake {
            fonts: vec![
                "Noto-Sans-CJK-SC".into(),
                "Noto-Sans-CJK-JP".into(),
                "Noto-Sans-CJK-KR".into(),
            ],
            ink: vec![("Noto-Sans-CJK-SC".into(), "中文".into())],
        };
        let selected = resolve_with(
            &mut probe,
            "missing-font",
            "bad-jp",
            "Noto-Sans-CJK-KR",
            "中文",
        )
        .unwrap();
        assert_eq!(selected.font, "Noto-Sans-CJK-SC");
        assert!(selected.japanese.is_empty());
        assert!(selected.korean.is_empty());
        for code in [
            "MENU_FONT_UNAVAILABLE",
            "MENU_REGIONAL_FONT_UNAVAILABLE",
            "MENU_REGIONAL_FONT_INCOMPLETE",
        ] {
            assert!(
                selected
                    .diagnostics
                    .iter()
                    .any(|v| v["Code"] == code && v["Severity"] == 1),
                "{selected:?}"
            );
        }
        let incomplete = resolve_with(&mut probe, "Noto-Sans-CJK-JP", "", "", "中文").unwrap();
        assert!(
            incomplete
                .diagnostics
                .iter()
                .any(|v| v["Code"] == "MENU_FONT_INCOMPLETE")
        );
        let missing = resolve_with(&mut probe, "Noto-Sans-CJK-SC", "", "", "あ").unwrap();
        assert!(
            missing
                .diagnostics
                .iter()
                .any(|v| v["Code"] == "MENU_FONT_GLYPHS_MISSING" && v["Severity"] == 2)
        );
        probe.fonts.clear();
        let absent = resolve_with(&mut probe, "missing", "", "", "中文").unwrap();
        assert!(
            absent
                .diagnostics
                .iter()
                .any(|v| v["Code"] == "MENU_FONT_MISSING" && v["Severity"] == 2)
        );
    }
}
