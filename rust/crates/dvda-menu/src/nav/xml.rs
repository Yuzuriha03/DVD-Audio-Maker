//! Typed parser for the dvdauthor navigation XML subset used by AMGM menus.
//!
//! The element/attribute tables mirror the original descriptor tables that the C
//! session handed to readxml, but the model is built directly instead of through
//! C callbacks, so the whole document is validated before any model state exists.

use super::model::{
    AudioAttrs, Button, Cell, Document, MenuGroup, Pgc, StreamAttrs, SubpictureAttrs, Vob,
};
use super::vm::{self, VmContext};
use crate::spu::xml::VideoFormat;
use crate::{Token, tokenize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Node {
    Dvdauthor,
    Amgm,
    Fpc,
    Menus,
    Video,
    Audio,
    SubpictureMenus,
    Pgc,
    Pre,
    Post,
    Button,
    Vob,
    Cell,
    SubpicturePgc,
    Stream,
}

#[derive(Default)]
struct Builder {
    document: Document,
    menus: Option<MenuGroup>,
    pgc: Option<Pgc>,
    vob: Option<Vob>,
    button: Option<Button>,
    audio: Option<AudioAttrs>,
    subpicture: Option<SubpictureAttrs>,
    stream: Option<StreamAttrs>,
    body: String,
    current_body: bool,
    seen_amgm: bool,
}

fn element_allowed(parent: Option<Node>, name: &str) -> Option<Node> {
    Some(match (parent, name) {
        (None, "dvdauthor") => Node::Dvdauthor,
        (Some(Node::Dvdauthor), "amgm") => Node::Amgm,
        (Some(Node::Amgm), "fpc") => Node::Fpc,
        (Some(Node::Amgm), "menus") => Node::Menus,
        (Some(Node::Menus), "video") => Node::Video,
        (Some(Node::Menus), "audio") => Node::Audio,
        (Some(Node::Menus), "subpicture") => Node::SubpictureMenus,
        (Some(Node::Menus), "pgc") => Node::Pgc,
        (Some(Node::Pgc), "pre") => Node::Pre,
        (Some(Node::Pgc), "post") => Node::Post,
        (Some(Node::Pgc), "button") => Node::Button,
        (Some(Node::Pgc), "vob") => Node::Vob,
        (Some(Node::Pgc), "subpicture") => Node::SubpicturePgc,
        (Some(Node::Vob), "cell") => Node::Cell,
        (Some(Node::SubpictureMenus), "stream") | (Some(Node::SubpicturePgc), "stream") => {
            Node::Stream
        }
        _ => return None,
    })
}

fn accepts_body(node: Node) -> bool {
    matches!(
        node,
        Node::Fpc | Node::Pre | Node::Post | Node::Button | Node::Cell
    )
}

fn boolean(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "on" | "yes" => Some(true),
        "0" | "off" | "no" => Some(false),
        _ => None,
    }
}

fn parse_pause(value: &str) -> Result<u8, String> {
    if value == "inf" {
        return Ok(255);
    }
    let seconds: u32 = value
        .parse()
        .map_err(|error| format!("bad pause time {value:?}: {error}"))?;
    if seconds > 254 {
        return Err(format!("pause time {seconds} out of range"));
    }
    Ok(seconds as u8)
}

fn parse_unsigned(value: &str, what: &str) -> Result<u32, String> {
    let trimmed = value.trim_start();
    let digits = trimmed.strip_prefix('+').unwrap_or(trimmed);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("bad {what}: {value:?}"));
    }
    digits
        .parse()
        .map_err(|error| format!("bad {what}: {error}"))
}

fn parse_entries(entries: &mut u8, is_vts: bool, value: &str) -> Result<(), String> {
    for name in value.split([',', ' ']).filter(|part| !part.is_empty()) {
        if is_vts {
            if name.eq_ignore_ascii_case("notitle") {
                *entries |= 1;
                continue;
            }
            return Err(format!("unknown title entry {name:?}"));
        }
        let mut matched = None;
        for (index, candidate) in ["", "", "title", "root", "subtitle", "audio", "angle", "ptt"]
            .iter()
            .enumerate()
            .skip(2)
        {
            if name.eq_ignore_ascii_case(candidate) {
                matched = Some(index);
                break;
            }
        }
        let index = matched.ok_or_else(|| format!("unknown menu entry {name:?}"))?;
        let bit = 1u8 << index;
        if *entries & bit != 0 {
            return Err(format!("entry {name:?} defined twice"));
        }
        *entries |= bit;
    }
    Ok(())
}

impl Builder {
    fn attribute(&mut self, node: Node, name: &str, value: &str) -> Result<(), String> {
        match node {
            Node::Dvdauthor => match name {
                "dest" => self.document.output_dir = Some(PathBuf::from(value)),
                "jumppad" => {
                    self.document.jumppad = boolean(value)
                        .ok_or_else(|| format!("invalid boolean for jumppad: {value:?}"))?;
                }
                "allgprm" => {
                    self.document.allgprm = boolean(value)
                        .ok_or_else(|| format!("invalid boolean for allgprm: {value:?}"))?;
                }
                "format" => self.document.format = parse_format(value)?,
                _ => return Err(format!("unsupported dvdauthor attribute {name:?}")),
            },
            Node::Menus => match name {
                "lang" => self.menus_mut()?.lang = value.to_string(),
                _ => return Err(format!("unsupported menus attribute {name:?}")),
            },
            Node::Video => {
                let video = &mut self.menus_mut()?.video;
                match name {
                    "format" => video.format = Some(value.to_string()),
                    "aspect" => video.aspect = Some(value.to_string()),
                    "resolution" => video.resolution = Some(value.to_string()),
                    "widescreen" => video.widescreen = Some(value.to_string()),
                    "caption" => video.caption = Some(value.to_string()),
                    _ => return Err(format!("unsupported video attribute {name:?}")),
                }
            }
            Node::Audio => {
                let audio = self.audio.get_or_insert_with(AudioAttrs::default);
                match name {
                    "format" => audio.format = Some(value.to_string()),
                    "quant" => audio.quant = Some(value.to_string()),
                    "dolby" => audio.dolby = Some(value.to_string()),
                    "lang" => audio.lang = Some(value.to_string()),
                    "channels" => audio.channels = Some(value.to_string()),
                    "samplerate" => audio.samplerate = Some(value.to_string()),
                    "content" => audio.content = Some(value.to_string()),
                    _ => return Err(format!("unsupported audio attribute {name:?}")),
                }
            }
            Node::SubpictureMenus | Node::SubpicturePgc => {
                let subpicture = self.subpicture.get_or_insert_with(SubpictureAttrs::default);
                match name {
                    "lang" => subpicture.lang = Some(value.to_string()),
                    "content" => subpicture.content = Some(value.to_string()),
                    _ => return Err(format!("unsupported subpicture attribute {name:?}")),
                }
            }
            Node::Stream => {
                let stream = self.stream.get_or_insert_with(StreamAttrs::default);
                match name {
                    "mode" => stream.mode = Some(value.to_string()),
                    "id" => stream.id = Some(value.to_string()),
                    _ => return Err(format!("unsupported stream attribute {name:?}")),
                }
            }
            Node::Pgc => {
                let pgc = self.pgc.as_mut().ok_or("pgc attribute outside pgc")?;
                match name {
                    "entry" => parse_entries(&mut pgc.entries, false, value)?,
                    "palette" => pgc.palette = Some(PathBuf::from(value)),
                    "pause" => pgc.pause = parse_pause(value)?,
                    _ => return Err(format!("unsupported pgc attribute {name:?}")),
                }
            }
            Node::Button => {
                let button = self
                    .button
                    .as_mut()
                    .ok_or("button attribute outside button")?;
                match name {
                    "name" => button.name = Some(value.to_string()),
                    "up" => button.up = Some(value.to_string()),
                    "down" => button.down = Some(value.to_string()),
                    "left" => button.left = Some(value.to_string()),
                    "right" => button.right = Some(value.to_string()),
                    "x0" => button.x0 = parse_unsigned(value, "button x0")? as i32,
                    "y0" => button.y0 = parse_unsigned(value, "button y0")? as i32,
                    "x1" => button.x1 = parse_unsigned(value, "button x1")? as i32,
                    "y1" => button.y1 = parse_unsigned(value, "button y1")? as i32,
                    _ => return Err(format!("unsupported button attribute {name:?}")),
                }
            }
            Node::Vob => {
                let vob = self.vob.as_mut().ok_or("vob attribute outside vob")?;
                match name {
                    "file" => vob.file = Some(PathBuf::from(value)),
                    "chapters" => vob.chapters = Some(value.to_string()),
                    "pause" => vob.pause = parse_pause(value)?,
                    _ => return Err(format!("unsupported vob attribute {name:?}")),
                }
            }
            Node::Cell => {
                let cell = self.cell_mut()?;
                match name {
                    "start" => cell.start = Some(value.to_string()),
                    "end" => cell.end = Some(value.to_string()),
                    "chapter" => {
                        cell.chapter = boolean(value).ok_or("invalid boolean for cell chapter")?
                    }
                    "program" => {
                        cell.program = boolean(value).ok_or("invalid boolean for cell program")?
                    }
                    "pause" => cell.pause = parse_pause(value)?,
                    _ => return Err(format!("unsupported cell attribute {name:?}")),
                }
            }
            Node::Amgm | Node::Fpc | Node::Pre | Node::Post => {
                return Err(format!("unsupported attribute {name:?}"));
            }
        }
        Ok(())
    }

    fn menus_mut(&mut self) -> Result<&mut MenuGroup, String> {
        self.menus
            .as_mut()
            .ok_or_else(|| "menus attribute outside menus".to_string())
    }

    fn cell_mut(&mut self) -> Result<&mut Cell, String> {
        self.vob
            .as_mut()
            .and_then(|vob| vob.cells.last_mut())
            .ok_or_else(|| "cell attribute outside cell".to_string())
    }

    fn pgc_mut(&mut self) -> Result<&mut Pgc, String> {
        self.pgc
            .as_mut()
            .ok_or_else(|| "pgc child outside pgc".to_string())
    }

    fn start(&mut self, node: Node) -> Result<(), String> {
        match node {
            Node::Dvdauthor | Node::Amgm => {
                if node == Node::Amgm {
                    if self.seen_amgm {
                        return Err("only one <amgm> is allowed".into());
                    }
                    self.seen_amgm = true;
                }
            }
            Node::Fpc => {
                if self.document.fpc.is_some() {
                    return Err("<fpc> already defined".into());
                }
                self.document.fpc = Some(Pgc::default());
                self.body.clear();
            }
            Node::Menus => {
                if self.menus.is_some() {
                    return Err("nested <menus>".into());
                }
                self.menus = Some(MenuGroup {
                    lang: "en".into(),
                    ..MenuGroup::default()
                });
            }
            Node::Pgc => {
                if self.pgc.is_some() {
                    return Err("nested <pgc>".into());
                }
                self.pgc = Some(Pgc::default());
            }
            Node::Pre | Node::Post => self.body.clear(),
            Node::Button => {
                self.button = Some(Button {
                    x0: -1,
                    y0: -1,
                    x1: -1,
                    y1: -1,
                    ..Button::default()
                });
                self.body.clear();
            }
            Node::Vob => {
                if self.vob.is_some() {
                    return Err("nested <vob>".into());
                }
                self.vob = Some(Vob::default());
            }
            Node::Cell => {
                let vob = self.vob.as_mut().ok_or("<cell> outside <vob>")?;
                vob.cells.push(Cell::default());
                self.body.clear();
            }
            Node::Video
            | Node::Audio
            | Node::SubpictureMenus
            | Node::SubpicturePgc
            | Node::Stream => {}
        }
        Ok(())
    }

    fn end(&mut self, node: Node) -> Result<(), String> {
        match node {
            Node::Fpc => {
                let fpc = self.document.fpc.as_mut().ok_or("missing fpc state")?;
                fpc.pre = std::mem::take(&mut self.body);
            }
            Node::Menus => {
                let group = self.menus.take().ok_or("missing menus state")?;
                if group.pgcs.is_empty() {
                    return Err("<menus> must contain at least one <pgc>".into());
                }
                self.document.menus.push(group);
            }
            Node::Pgc => {
                let pgc = self.pgc.take().ok_or("missing pgc state")?;
                let menus = self.menus.as_mut().ok_or("<pgc> outside <menus>")?;
                menus.pgcs.push(pgc);
            }
            Node::Audio => {
                let audio = self.audio.take().unwrap_or_default();
                let menus = self.menus_mut()?;
                if menus.audio.len() == 8 {
                    return Err("too many audio tracks for one menu group".into());
                }
                menus.audio.push(audio);
            }
            Node::SubpictureMenus => {
                let subpicture = self.subpicture.take().unwrap_or_default();
                let menus = self.menus_mut()?;
                if menus.subpictures.len() == 32 {
                    return Err("too many subpicture streams for one menu group".into());
                }
                menus.subpictures.push(subpicture);
            }
            Node::SubpicturePgc => {
                let subpicture = self.subpicture.take().unwrap_or_default();
                let pgc = self.pgc_mut()?;
                if pgc.subpictures.len() == 32 {
                    return Err("too many subpicture streams for one pgc".into());
                }
                pgc.subpictures.push(subpicture);
            }
            Node::Stream => {
                let stream = self.stream.take().unwrap_or_default();
                self.subpicture
                    .get_or_insert_with(SubpictureAttrs::default)
                    .streams
                    .push(stream);
            }
            Node::Pre => {
                self.pgc_mut()?.pre = std::mem::take(&mut self.body);
            }
            Node::Post => {
                self.pgc_mut()?.post = std::mem::take(&mut self.body);
            }
            Node::Button => {
                let mut button = self.button.take().ok_or("missing button state")?;
                button.commands = std::mem::take(&mut self.body);
                let pgc = self.pgc_mut()?;
                if pgc.buttons.len() == 36 {
                    return Err("limit of 36 buttons per pgc".into());
                }
                if button.name.is_none() {
                    button.name = Some((pgc.buttons.len() + 1).to_string());
                }
                pgc.buttons.push(button);
            }
            Node::Vob => {
                let vob = self.vob.take().ok_or("missing vob state")?;
                if vob.file.is_none() {
                    return Err("<vob> requires a file attribute".into());
                }
                self.pgc_mut()?.vobs.push(vob);
            }
            Node::Cell => {
                let commands = std::mem::take(&mut self.body);
                self.cell_mut()?.commands = commands;
            }
            Node::Dvdauthor | Node::Amgm | Node::Video => {}
        }
        Ok(())
    }

    fn text(&mut self, value: &str) -> Result<(), String> {
        if !self.current_body && !value.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')) {
            return Err("unexpected XML text".into());
        }
        if self.current_body {
            self.body.push_str(value);
        }
        Ok(())
    }

    fn finish(mut self) -> Result<Document, String> {
        for group in &mut self.document.menus {
            let context = VmContext {
                menu_pgc_count: group.pgcs.len() as u16,
            };
            for pgc in &mut group.pgcs {
                pgc.pre_compiled = vm::compile_text(&pgc.pre, context)?;
                pgc.post_compiled = vm::compile_text(&pgc.post, context)?;
                for button in &mut pgc.buttons {
                    button.compiled = vm::compile_text(&button.commands, context)?;
                }
                for vob in &mut pgc.vobs {
                    for cell in &mut vob.cells {
                        cell.compiled = vm::compile_text(&cell.commands, context)?;
                    }
                }
            }
        }
        let fpc_context = VmContext {
            menu_pgc_count: self
                .document
                .menus
                .first()
                .map_or(0, |group| group.pgcs.len() as u16),
        };
        if let Some(fpc) = &mut self.document.fpc {
            fpc.pre_compiled = vm::compile_text(&fpc.pre, fpc_context)?;
        }
        Ok(self.document)
    }
}

fn parse_format(value: &str) -> Result<VideoFormat, String> {
    if value.eq_ignore_ascii_case("pal") {
        Ok(VideoFormat::Pal)
    } else if value.eq_ignore_ascii_case("ntsc") {
        Ok(VideoFormat::Ntsc)
    } else {
        Err(format!("unrecognized video format {value:?}"))
    }
}

pub(crate) fn parse(path: &Path) -> Result<Document, String> {
    let data = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let tokens = tokenize(&data)?;
    let mut builder = Builder::default();
    let mut stack: Vec<Node> = Vec::new();
    for token in tokens {
        match token {
            Token::Start(name, attrs, empty) => {
                let parent = stack.last().copied();
                let node = element_allowed(parent, &name)
                    .ok_or_else(|| format!("unsupported element {name}"))?;
                builder.start(node)?;
                for (key, value) in attrs {
                    builder.attribute(node, &key, &value)?;
                }
                if empty {
                    builder.end(node)?;
                } else {
                    builder.current_body = accepts_body(node);
                    stack.push(node);
                }
            }
            Token::End(_) => {
                let node = stack.pop().ok_or("unmatched end tag")?;
                builder.end(node)?;
                builder.current_body = stack.last().is_some_and(|node| accepts_body(*node));
                builder.body.clear();
            }
            Token::Text(value) => builder.text(&value)?,
        }
    }
    if !stack.is_empty() || !builder.seen_amgm || builder.document.menus.is_empty() {
        return Err("incomplete navigation document".into());
    }
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(xml: &str) -> Result<Document, String> {
        let path = std::env::temp_dir().join(format!("dvda-nav-{}.xml", uuid()));
        std::fs::write(&path, xml).unwrap();
        let result = parse(&path);
        let _ = std::fs::remove_file(path);
        result
    }

    fn uuid() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }

    const NAV: &str = "<dvdauthor jumppad=\"1\"><amgm><menus><video format=\"pal\"/><audio format=\"mp2\" lang=\"en\"/><pgc><pre>g0=1;</pre><button name=\"button01\">jump menu 1;</button><vob file=\"menu.mpg\"/><post>jump menu 1;</post></pgc></menus></amgm></dvdauthor>";

    #[test]
    fn parses_the_acceptance_document() {
        let document = document(NAV).unwrap();
        assert_eq!(document.format, VideoFormat::Pal);
        assert!(document.jumppad);
        assert_eq!(document.menus.len(), 1);
        let menu = &document.menus[0];
        assert_eq!(menu.lang, "en");
        assert_eq!(menu.video.format.as_deref(), Some("pal"));
        assert_eq!(menu.audio.len(), 1);
        assert_eq!(menu.audio[0].format.as_deref(), Some("mp2"));
        assert_eq!(menu.pgcs.len(), 1);
        let pgc = &menu.pgcs[0];
        assert_eq!(
            pgc.pre_compiled,
            [0x71, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00]
        );
        assert_eq!(
            pgc.post_compiled,
            [0x20, 0x04, 0x00, 0x00, 0x00, 0xc0, 0x00, 0x01]
        );
        assert_eq!(pgc.buttons.len(), 1);
        assert_eq!(pgc.buttons[0].name.as_deref(), Some("button01"));
        assert_eq!(
            pgc.buttons[0].compiled,
            [0x20, 0x04, 0x00, 0x00, 0x00, 0xc0, 0x00, 0x01]
        );
        assert_eq!(pgc.vobs.len(), 1);
        assert_eq!(
            pgc.vobs[0].file.as_deref(),
            Some(std::path::Path::new("menu.mpg"))
        );
    }

    #[test]
    fn parses_track_jumps_and_entries() {
        let xml = "<dvdauthor><amgm><menus lang=\"ja\"><pgc entry=\"root\"><button>jump group 2 track 3;</button><vob file=\"a.mpg\"><cell start=\"00:00:01.00\" pause=\"inf\"/></vob></pgc></menus></amgm></dvdauthor>";
        let document = document(xml).unwrap();
        let pgc = &document.menus[0].pgcs[0];
        assert_eq!(pgc.entries, 1 << 3);
        assert_eq!(pgc.buttons[0].name.as_deref(), Some("1"));
        assert_eq!(
            pgc.buttons[0].compiled,
            [0x30, 0x0a, 0x00, 0x02, 0x00, 0x03, 0x00, 0x00]
        );
        assert_eq!(pgc.vobs[0].cells.len(), 1);
        assert_eq!(pgc.vobs[0].cells[0].pause, 255);
        assert_eq!(document.menus[0].lang, "ja");
    }

    #[test]
    fn rejects_unknown_or_malformed_documents() {
        assert!(document("<dvdauthor><amgm/></dvdauthor>").is_err());
        assert!(document("<dvdauthor><amgm><menus><unknown/></menus></amgm></dvdauthor>").is_err());
        assert!(
            document("<dvdauthor><amgm><menus><pgc bogus=\"1\"/></menus></amgm></dvdauthor>")
                .is_err()
        );
        assert!(
            document("<dvdauthor format=\"secam\"><amgm><menus><pgc/></menus></amgm></dvdauthor>")
                .is_err()
        );
        assert!(document("<dvdauthor><amgm><menus><pgc><button>call menu 1;</button></pgc></menus></amgm></dvdauthor>").is_err());
    }
}
