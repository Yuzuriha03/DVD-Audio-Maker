//! Typed navigation document model for the AMGM menu authoring path.
//!
//! Mirrors the surviving dvdauthor workspace structures (menus, PGCs, buttons,
//! VOB/cell sources) without the C ownership machinery: every collection is a
//! Vec, every string an owned String and every failure a Result.

use crate::spu::format::VideoFormat;
use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub(crate) struct VideoAttrs {
    pub(crate) format: Option<String>,
    pub(crate) aspect: Option<String>,
    pub(crate) resolution: Option<String>,
    pub(crate) widescreen: Option<String>,
    pub(crate) caption: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AudioAttrs {
    pub(crate) format: Option<String>,
    pub(crate) quant: Option<String>,
    pub(crate) dolby: Option<String>,
    pub(crate) lang: Option<String>,
    pub(crate) channels: Option<String>,
    pub(crate) samplerate: Option<String>,
    pub(crate) content: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SubpictureAttrs {
    pub(crate) lang: Option<String>,
    pub(crate) content: Option<String>,
    pub(crate) streams: Vec<StreamAttrs>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct StreamAttrs {
    pub(crate) mode: Option<String>,
    pub(crate) id: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Cell {
    /// Start timestamp as written in the XML; parsed by the VOB builder.
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
    pub(crate) chapter: bool,
    pub(crate) program: bool,
    /// Still-time in seconds, 255 means infinite.
    pub(crate) pause: u8,
    pub(crate) commands: String,
    pub(crate) compiled: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Vob {
    pub(crate) file: Option<PathBuf>,
    pub(crate) chapters: Option<String>,
    pub(crate) pause: u8,
    pub(crate) cells: Vec<Cell>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Button {
    pub(crate) name: Option<String>,
    pub(crate) up: Option<String>,
    pub(crate) down: Option<String>,
    pub(crate) left: Option<String>,
    pub(crate) right: Option<String>,
    pub(crate) x0: i32,
    pub(crate) y0: i32,
    pub(crate) x1: i32,
    pub(crate) y1: i32,
    pub(crate) commands: String,
    pub(crate) compiled: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Pgc {
    /// Menu entry bitmask: 2 title, 3 root, 4 subtitle, 5 audio, 6 angle, 7 ptt.
    pub(crate) entries: u8,
    pub(crate) palette: Option<PathBuf>,
    pub(crate) pause: u8,
    pub(crate) pre: String,
    pub(crate) post: String,
    pub(crate) pre_compiled: Vec<u8>,
    pub(crate) post_compiled: Vec<u8>,
    pub(crate) buttons: Vec<Button>,
    pub(crate) vobs: Vec<Vob>,
    pub(crate) subpictures: Vec<SubpictureAttrs>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MenuGroup {
    pub(crate) lang: String,
    pub(crate) video: VideoAttrs,
    pub(crate) audio: Vec<AudioAttrs>,
    pub(crate) subpictures: Vec<SubpictureAttrs>,
    pub(crate) pgcs: Vec<Pgc>,
}

#[derive(Clone, Debug)]
pub(crate) struct Document {
    pub(crate) format: VideoFormat,
    pub(crate) jumppad: bool,
    pub(crate) allgprm: bool,
    pub(crate) output_dir: Option<PathBuf>,
    pub(crate) menus: Vec<MenuGroup>,
    pub(crate) fpc: Option<Pgc>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            format: VideoFormat::Pal,
            jumppad: false,
            allgprm: false,
            output_dir: None,
            menus: Vec::new(),
            fpc: None,
        }
    }
}
