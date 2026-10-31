//! Typed parser for the spumux XML document used by the menu authoring path.
//!
//! Reuses the hardened XML tokenizer from the crate root (no DTD, no entity
//! expansion beyond built-ins, UTF-8 only) and maps the tokens onto the same
//! element/attribute descriptor table the C session used to hand to readxml.

use super::color::{Color, parse_hex};
use super::image::{Button, SubPicture};
use crate::{Token, tokenize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum VideoFormat {
    Pal,
    Ntsc,
}

#[derive(Debug)]
pub(crate) struct Document {
    pub(crate) format: VideoFormat,
    pub(crate) spus: Vec<SubPicture>,
}

impl Default for Document {
    fn default() -> Self {
        Self { format: VideoFormat::Pal, spus: Vec::new() }
    }
}

const SUBPICTURES: u32 = 1;
const STREAM: u32 = 2;
const SPU: u32 = 3;
const NO_SUB: u32 = 4;

fn element_table(name: &str, parent: u32) -> Option<u32> {
    match (name, parent) {
        ("subpictures", 0) => Some(SUBPICTURES),
        ("stream", SUBPICTURES) => Some(STREAM),
        ("spu", STREAM) => Some(SPU),
        ("button" | "action", SPU) => Some(NO_SUB),
        _ => None,
    }
}

pub(crate) fn parse(path: &Path) -> Result<Document, String> {
    let data = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let tokens = tokenize(&data)?;
    let mut document = Document::default();
    let mut state = 0u32;
    let mut stack: Vec<(u32, u32)> = Vec::new();
    let mut current: Option<SubPicture> = None;
    let mut had_stream = false;
    for token in tokens {
        match token {
            Token::Start(name, attrs, empty) => {
                let Some(new_state) = element_table(&name, state) else {
                    return Err(format!("unsupported element {name}"));
                };
                match name.as_str() {
                    "stream" => {
                        if had_stream {
                            return Err("only one stream is currently allowed".into());
                        }
                        had_stream = true;
                    }
                    "spu" => current = Some(SubPicture::new()),
                    "button" | "action" => {
                        let spu = current.as_mut().ok_or("button outside spu")?;
                        spu.buttons.push(Button::new(name == "action"));
                    }
                    _ => {}
                }
                for (key, value) in attrs {
                    apply_attribute(current.as_mut(), &name, &key, &value, &mut document)?;
                }
                if empty {
                    if name == "spu" {
                        finish_spu(&mut document, &mut current)?;
                    }
                } else {
                    stack.push((new_state, state));
                    state = new_state;
                }
            }
            Token::End(name) => {
                let Some((new_state, parent)) = stack.pop() else {
                    return Err("unmatched end tag".into());
                };
                state = parent;
                let _ = new_state;
                if name == "spu" {
                    finish_spu(&mut document, &mut current)?;
                }
            }
            Token::Text(value) => {
                if !value.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')) {
                    return Err("unexpected XML text".into());
                }
            }
        }
    }
    Ok(document)
}

fn finish_spu(document: &mut Document, current: &mut Option<SubPicture>) -> Result<(), String> {
    let Some(mut spu) = current.take() else {
        return Err("missing spu state".into());
    };
    if spu.sd == 0 {
        spu.sd = -1;
    } else if spu.sd <= spu.spts {
        return Ok(());
    } else {
        spu.sd -= spu.spts;
    }
    document.spus.push(spu);
    Ok(())
}

fn apply_attribute(
    current: Option<&mut SubPicture>,
    element: &str,
    name: &str,
    value: &str,
    document: &mut Document,
) -> Result<(), String> {
    let allowed = match element {
        "subpictures" => matches!(name, "format"),
        "spu" => matches!(
            name,
            "image"
                | "highlight"
                | "select"
                | "start"
                | "end"
                | "transparent"
                | "autooutline"
                | "outlinewidth"
                | "autoorder"
                | "force"
                | "xoffset"
                | "yoffset"
        ),
        "button" | "action" => matches!(
            name,
            "name" | "up" | "down" | "left" | "right" | "x0" | "y0" | "x1" | "y1"
        ),
        _ => false,
    };
    if !allowed {
        return Err(format!("unsupported attribute {name} on {element}"));
    }
    match element {
        "subpictures" => {
            if value.eq_ignore_ascii_case("ntsc") {
                document.format = VideoFormat::Ntsc;
            } else if value.eq_ignore_ascii_case("pal") {
                document.format = VideoFormat::Pal;
            } else {
                return Err(format!("unrecognized video format {value:?}"));
            }
        }
        "spu" => {
            let spu = current.ok_or("spu attribute outside spu")?;
            match name {
                "image" => spu.img.fname = Some(PathBuf::from(value)),
                "highlight" => spu.hlt.fname = Some(PathBuf::from(value)),
                "select" => spu.sel.fname = Some(PathBuf::from(value)),
                "start" => spu.spts = parse_time(value) as i32,
                "end" => spu.sd = parse_time(value) as i32,
                "outlinewidth" => spu.outlinewidth = parse_unsigned(value, "spu outlinewidth")?,
                "xoffset" => spu.x0 = parse_unsigned(value, "spu xoffset")?,
                "yoffset" => spu.y0 = parse_unsigned(value, "spu yoffset")?,
                "force" => spu.forced = parse_boolean(value).ok_or("invalid boolean for spu force")?,
                "transparent" => spu.transparentc = parse_color(value)?,
                "autooutline" => {
                    if value != "infer" {
                        return Err(format!("unknown autooutline type {value}"));
                    }
                    spu.autooutline = true;
                }
                "autoorder" => match value {
                    "rows" => spu.autoorder = false,
                    "columns" => spu.autoorder = true,
                    _ => return Err(format!("unknown autoorder type {value}")),
                },
                _ => unreachable!(),
            }
        }
        "button" | "action" => {
            let spu = current.ok_or("button attribute outside spu")?;
            let button = spu.buttons.last_mut().ok_or("button attribute without button")?;
            match name {
                "name" => button.name = Some(value.to_string()),
                "up" => button.up = Some(value.to_string()),
                "down" => button.down = Some(value.to_string()),
                "left" => button.left = Some(value.to_string()),
                "right" => button.right = Some(value.to_string()),
                "x0" => button.r.x0 = parse_unsigned(value, "button x0")? as i32,
                "y0" => button.r.y0 = parse_unsigned(value, "button y0")? as i32,
                "x1" => button.r.x1 = parse_unsigned(value, "button x1")? as i32,
                "y1" => button.r.y1 = parse_unsigned(value, "button y1")? as i32,
                _ => unreachable!(),
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn parse_color(value: &str) -> Result<Color, String> {
    // The menu authoring path never emits named colours; fail loudly rather
    // than silently changing the palette.
    parse_hex(value).map_err(|error| format!("unsupported colour {value:?}: {error}"))
}

fn parse_boolean(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "on" | "yes" => Some(true),
        "0" | "off" | "no" => Some(false),
        _ => None,
    }
}

fn parse_unsigned(text: &str, what: &str) -> Result<u32, String> {
    let trimmed = text.trim_start();
    let (negative, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("bad number for {what}: {text:?}"));
    }
    let value: u64 = digits.parse().map_err(|error| format!("bad number for {what}: {error}"))?;
    Ok(if negative { (value as u32).wrapping_neg() } else { value as u32 })
}

fn parse_time(text: &str) -> u32 {
    let mut fraction = false;
    let mut total: i64 = 0;
    let mut last: i64 = 0;
    let mut scale: i64 = 1;
    for byte in text.bytes() {
        if byte.is_ascii_digit() {
            if scale < 10000 {
                last = last * 10 + i64::from(byte - b'0');
                scale *= 10;
            }
        } else if byte == b':' {
            total = total * 60 + last;
            last = 0;
            scale = 1;
        } else if byte == b'.' || byte == b',' {
            total = total * 60 + last;
            last = 0;
            scale = 1;
            fraction = true;
        }
    }
    let value = if fraction {
        total * 90000 + 90000 * last / scale
    } else {
        (total * 60 + last) * 90000
    };
    value as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_match_historical_parser() {
        assert_eq!(parse_time("0"), 0);
        assert_eq!(parse_time("1"), 90_000);
        assert_eq!(parse_time("00:00:00.00"), 0);
        assert_eq!(parse_time("00:01:02.50"), (60 + 2) * 90_000 + 45_000);
        assert_eq!(parse_time("1:02"), 62 * 90_000);
    }

    #[test]
    fn unsigned_rejects_junk() {
        assert_eq!(parse_unsigned("42", "x").unwrap(), 42);
        assert_eq!(parse_unsigned("-1", "x").unwrap(), u32::MAX);
        assert!(parse_unsigned("4x", "x").is_err());
        assert!(parse_unsigned("", "x").is_err());
    }

    #[test]
    fn boolean_forms() {
        assert_eq!(parse_boolean("YES"), Some(true));
        assert_eq!(parse_boolean("off"), Some(false));
        assert_eq!(parse_boolean("maybe"), None);
    }
}
