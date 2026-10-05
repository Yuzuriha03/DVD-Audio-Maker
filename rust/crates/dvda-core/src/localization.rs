//! Presentation-only translations; captured paths and encoder inputs stay unchanged.
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

enum Part {
    Literal(String),
    Argument(usize),
}

struct Entry {
    source: String,
    chinese: String,
    english: String,
    japanese: String,
    pattern: Vec<Part>,
    parameters: usize,
    nested: Vec<usize>,
}

fn parts(text: &str) -> Vec<Part> {
    let mut result = Vec::new();
    let mut literal = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        if rest.starts_with("{{") || rest.starts_with("}}") {
            literal.push(rest.chars().next().unwrap());
            rest = &rest[2..];
            continue;
        }
        if rest.starts_with('{')
            && let Some(end) = rest.find('}')
            && let Ok(index) = rest[1..end].parse::<usize>()
        {
            if !literal.is_empty() {
                result.push(Part::Literal(std::mem::take(&mut literal)));
            }
            result.push(Part::Argument(index));
            rest = &rest[end + 1..];
            continue;
        }
        let ch = rest.chars().next().unwrap();
        literal.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    if !literal.is_empty() {
        result.push(Part::Literal(literal));
    }
    result
}

fn render(template: &str, args: &[String]) -> String {
    parts(template)
        .into_iter()
        .map(|part| match part {
            Part::Literal(value) => value,
            Part::Argument(index) => args.get(index).cloned().unwrap_or_default(),
        })
        .collect()
}

fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let en: Vec<Value> = serde_json::from_str(include_str!("../locales/messages.en.json"))
            .expect("embedded English catalog");
        let ja: Value = serde_json::from_str(include_str!("../locales/messages.ja.json"))
            .expect("embedded Japanese catalog");
        let mut entries = en
            .into_iter()
            .map(|v| {
                let source = v["Source"].as_str().unwrap().to_owned();
                Entry {
                    chinese: source.clone(),
                    english: v["English"].as_str().unwrap().to_owned(),
                    japanese: ja[&source]
                        .as_str()
                        .expect("Japanese catalog coverage")
                        .to_owned(),
                    parameters: v["Parameters"].as_u64().unwrap_or(0) as usize,
                    nested: v["Nested"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|x| x.as_u64().map(|i| i as usize))
                        .collect(),
                    pattern: parts(&source),
                    source,
                }
            })
            .collect::<Vec<_>>();
        let additional: Vec<Value> =
            serde_json::from_str(include_str!("../locales/messages.rust.json"))
                .expect("embedded Rust message catalog");
        for v in additional {
            let source = v[0].as_str().unwrap().to_owned();
            let pattern = parts(&source);
            let parameters = pattern
                .iter()
                .filter_map(|p| match p {
                    Part::Argument(i) => Some(i + 1),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            entries.retain(|e| e.source != source);
            entries.push(Entry {
                source,
                pattern,
                parameters,
                chinese: v[1].as_str().unwrap().into(),
                english: v[2].as_str().unwrap().into(),
                japanese: v[3].as_str().unwrap().into(),
                nested: v
                    .get(4)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|i| i.as_u64().unwrap() as usize)
                    .collect(),
            });
        }
        entries.sort_by_key(|e| {
            std::cmp::Reverse(
                e.pattern
                    .iter()
                    .map(|p| match p {
                        Part::Literal(s) => s.len(),
                        _ => 0,
                    })
                    .sum::<usize>(),
            )
        });
        entries
    })
}

fn capture<'a>(
    pattern: &[Part],
    text: &'a str,
    args: &mut [Option<&'a str>],
    budget: &mut usize,
) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    let Some((first, rest)) = pattern.split_first() else {
        return text.is_empty();
    };
    match first {
        Part::Literal(s) => text
            .strip_prefix(s)
            .is_some_and(|tail| capture(rest, tail, args, budget)),
        Part::Argument(index) => {
            if *index >= args.len() {
                return false;
            }
            if let Some(value) = args[*index] {
                return text
                    .strip_prefix(value)
                    .is_some_and(|tail| capture(rest, tail, args, budget));
            }
            let ends: Vec<usize> = match rest.first() {
                None => vec![text.len()],
                Some(Part::Literal(literal)) if !literal.is_empty() => {
                    text.match_indices(literal).map(|(i, _)| i).collect()
                }
                _ => text
                    .char_indices()
                    .map(|(i, _)| i)
                    .chain(std::iter::once(text.len()))
                    .collect(),
            };
            for end in ends {
                let saved = args.to_vec();
                args[*index] = Some(&text[..end]);
                if capture(rest, &text[end..], args, budget) {
                    return true;
                }
                args.copy_from_slice(&saved);
            }
            false
        }
    }
}

pub fn translate(language: &str, text: &str) -> String {
    translate_inner(language, text, 0)
}

// Channel validation joins multiple numeric counts with semicolons. Match the
// complete generated grammar before generic placeholders can consume another
// item as part of an argument. Arbitrary user paths/titles are never split.
fn channel_counts(language: &str, text: &str) -> Option<String> {
    let counts: Option<Vec<_>> = text
        .split("; ")
        .map(|item| {
            let (channels, tracks) = item.strip_suffix(" 首")?.split_once(" 声道 × ")?;
            let digits =
                |value: &str| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
            (digits(channels) && digits(tracks)).then_some((channels, tracks))
        })
        .collect();
    Some(
        counts?
            .into_iter()
            .map(|(channels, tracks)| match language {
                "zh" | "zh-CN" => format!("{channels} 声道 × {tracks} 首"),
                "ja" => format!("{channels} チャンネル × {tracks} 曲"),
                _ => format!("{channels} channels × {tracks} tracks"),
            })
            .collect::<Vec<_>>()
            .join("; "),
    )
}

fn translate_inner(language: &str, text: &str, depth: usize) -> String {
    if depth > 6 || text.len() > 32768 {
        return text.into();
    }
    if let Some(translated) = channel_counts(language, text) {
        return translated;
    }
    static EXACT: OnceLock<HashMap<String, usize>> = OnceLock::new();
    let exact = EXACT.get_or_init(|| {
        entries()
            .iter()
            .enumerate()
            .filter(|(_, e)| e.parameters == 0)
            .map(|(i, e)| (render(&e.source, &[]), i))
            .collect()
    });
    let target = |e: &'static Entry| match language {
        "ja" => &e.japanese,
        "zh" | "zh-CN" => &e.chinese,
        _ => &e.english,
    };
    if let Some(&index) = exact.get(text) {
        return render(target(&entries()[index]), &[]);
    }
    for e in entries().iter().filter(|e| e.parameters > 0) {
        if let Some(Part::Literal(prefix)) = e.pattern.first()
            && !text.starts_with(prefix)
        {
            continue;
        }
        let mut args = vec![None; e.parameters];
        if capture(&e.pattern, text, &mut args, &mut 4096) {
            let args = args
                .into_iter()
                .enumerate()
                .map(|(i, v)| {
                    let value = v.unwrap_or_default();
                    if e.nested.contains(&i) {
                        translate_inner(language, value, depth + 1)
                    } else {
                        value.into()
                    }
                })
                .collect::<Vec<_>>();
            return render(target(e), &args);
        }
    }
    if text.contains('\n') {
        return text
            .split('\n')
            .map(|line| translate_inner(language, line, depth + 1))
            .collect::<Vec<_>>()
            .join("\n");
    }
    text.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joined_channel_counts_translate_every_item_without_splitting_user_values() {
        let input = "1 声道 × 1 首; 2 声道 × 3 首; 6 声道 × 12 首";
        assert_eq!(
            translate("en", input),
            "1 channels × 1 tracks; 2 channels × 3 tracks; 6 channels × 12 tracks"
        );
        assert_eq!(
            translate("ja", input),
            "1 チャンネル × 1 曲; 2 チャンネル × 3 曲; 6 チャンネル × 12 曲"
        );
        assert_eq!(translate("zh-CN", input), input);
        for user_value in [
            r"D:\音乐\1 声道 × 1 首; 2 声道 × 3 首.flac",
            "音轨; 标题",
            "声道 × 1 首",
        ] {
            assert!(channel_counts("en", user_value).is_none());
        }
    }

    #[test]
    fn all_catalog_entries_preserve_parameters_in_both_languages() {
        assert!(entries().len() >= 980);
        for e in entries() {
            let args = (0..e.parameters)
                .map(|i| format!("音源🎵_ARG_{i}"))
                .collect::<Vec<_>>();
            let input = render(&e.source, &args);
            let mut captured = vec![None; e.parameters];
            assert!(
                capture(&e.pattern, &input, &mut captured, &mut 4096),
                "{}",
                e.source
            );
            // Adjacent placeholders cannot be separated uniquely from formatted text.
            // Require a lossless reconstruction and preservation in both translations.
            let captured = captured
                .into_iter()
                .map(|v| v.unwrap_or_default().to_owned())
                .collect::<Vec<_>>();
            assert_eq!(render(&e.source, &captured), input, "{}", e.source);
            for template in [&e.chinese, &e.english, &e.japanese] {
                assert!(!template.is_empty());
                let output = render(template, &captured);
                for arg in &args {
                    assert!(output.contains(arg), "{} -> {}", e.source, output);
                }
            }
        }
    }

    #[test]
    fn paths_and_unrecognized_diagnostics_stay_intact() {
        let raw = "报告已写入: D:\\音乐\\失败.flac";
        let en = translate("en", raw);
        assert_ne!(en, raw);
        assert!(en.contains("D:\\音乐\\失败.flac"));
        assert_eq!(translate("zh-CN", raw), raw);
        let diagnostic = "decoder 0x6f: unknown failure at offset 98765";
        assert_eq!(translate("ja", diagnostic), diagnostic);
        assert_eq!(translate("ja", "停止任务"), "停止");
    }
}
