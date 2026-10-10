//! DVD-Audio menu and still-picture authoring orchestration.
//!
//! Layout, text, image arguments, SPU/navigation XML, still-stream repair and
//! sector accounting live here. The caller supplies the existing image,
//! MPEG-2 and menu implementations through `Backend`; no C author is invoked.

use crate::{Callbacks, atsi};
use std::{
    fmt::Write as _,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

const SECTOR: u64 = 2048;
const WIDTH: u16 = 720;
const INDEX_CELLS: usize = 12;
const MAX_ROWS: usize = 30;

/// All calls are synchronous and operate on owned argument vectors or paths.
/// `write_y4m` receives `pal`/`secam`/`ntsc` and a display aspect string; the
/// image bridge maps the norm to its `25`/`30` frame-rate argument. `create_mpg`
/// receives the MPEG aspect code (`1`..`4`). Still encoding ignores `wav`.
pub trait Backend {
    fn image_run(&mut self, args: Vec<String>, callbacks: &mut dyn Callbacks)
    -> Result<(), String>;
    fn write_y4m(
        &mut self,
        input: &Path,
        output: &Path,
        norm: &str,
        aspect: &str,
        callbacks: &mut dyn Callbacks,
    ) -> Result<(), String>;
    #[allow(clippy::too_many_arguments)]
    fn create_mpg(
        &mut self,
        y4m: &Path,
        wav: &Path,
        output: &Path,
        norm: &str,
        aspect: &str,
        still: bool,
        callbacks: &mut dyn Callbacks,
    ) -> Result<(), String>;
    fn subpictures(
        &mut self,
        xml: &Path,
        input: &Path,
        output: &Path,
        callbacks: &mut dyn Callbacks,
    ) -> Result<(), String>;
    /// `output` is the disc root; write `output/AUDIO_TS/AUDIO_TS.VOB`.
    fn navigation(
        &mut self,
        xml: &Path,
        output: &Path,
        callbacks: &mut dyn Callbacks,
    ) -> Result<(), String>;
}

#[derive(Clone, Debug, Default)]
pub struct Track {
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub enabled: bool,
    pub nmenus: usize,
    pub index_pages: usize,
    pub backgrounds: Vec<PathBuf>,
    pub blankscreen: Option<PathBuf>,
    pub screen_text: Option<String>,
    pub index_covers: Option<PathBuf>,
    pub font_name: String,
    pub font_name_jp: Option<String>,
    pub font_name_kr: Option<String>,
    pub font_size: u16,
    pub font_width: u16,
    pub norm: String,
    pub aspect: u8,
    pub duration: [u8; 3],
    pub looped: bool,
    /// Text, highlight, select, background, in AMG order (Y/Cr/Cb).
    pub palette: [u32; 4],
    pub still_pics: Option<String>,
    pub top_menus: Vec<PathBuf>,
    pub top_vob: Option<PathBuf>,
    pub still_vob: Option<PathBuf>,
    pub background_mpgs: Vec<PathBuf>,
    pub soundtrack: Option<PathBuf>,
    pub navigation_xml: Option<PathBuf>,
    pub spu_xmls: Vec<PathBuf>,
    pub images: Vec<PathBuf>,
    pub highlights: Vec<PathBuf>,
    pub selects: Vec<PathBuf>,
    pub data_directory: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: false,
            nmenus: 0,
            index_pages: 0,
            backgrounds: Vec::new(),
            blankscreen: None,
            screen_text: None,
            index_covers: None,
            font_name: if cfg!(windows) {
                let font = PathBuf::from(
                    std::env::var_os("WINDIR").unwrap_or_else(|| "C:/Windows".into()),
                )
                .join("Fonts/arial.ttf");
                if font.is_file() {
                    font.to_string_lossy().replace('\\', "/")
                } else {
                    "Arial".into()
                }
            } else {
                "Courier-Bold".into()
            },
            font_name_jp: None,
            font_name_kr: None,
            font_size: 25,
            font_width: 6,
            norm: "pal".into(),
            aspect: 2,
            duration: [0; 3],
            looped: false,
            palette: [0xeb8080, 0x51f05a, 0x808080, 0x108080],
            still_pics: None,
            top_menus: Vec::new(),
            top_vob: None,
            still_vob: None,
            background_mpgs: Vec::new(),
            soundtrack: None,
            navigation_xml: None,
            spu_xmls: Vec::new(),
            images: Vec::new(),
            highlights: Vec::new(),
            selects: Vec::new(),
            data_directory: None,
        }
    }
}

fn number(value: &str, name: &str, minimum: usize, maximum: usize) -> Result<usize, String> {
    let value = value
        .parse::<usize>()
        .map_err(|_| format!("Invalid --{name}"))?;
    if !(minimum..=maximum).contains(&value) {
        return Err(format!("--{name} must be in {minimum}..={maximum}"));
    }
    Ok(value)
}
fn paths(value: &str) -> Vec<PathBuf> {
    value
        .split(',')
        .filter(|part| !part.is_empty())
        .map(PathBuf::from)
        .collect()
}

impl Options {
    /// Parse a long option without consuming audio/ISO author options.
    pub fn parse_option(&mut self, name: &str, value: Option<&str>) -> Result<bool, String> {
        let name = name.trim_start_matches('-');
        let required = || value.ok_or_else(|| format!("--{name} requires a value"));
        match name {
            "topmenu" => {
                self.enabled = true;
                if let Some(value) = value {
                    self.top_menus = paths(value);
                }
            }
            "nmenus" => self.nmenus = number(required()?, name, 1, 255)?,
            "index-pages" => self.index_pages = number(required()?, name, 0, 254)?,
            "background" => {
                self.backgrounds = paths(required()?);
                self.enabled = true;
            }
            "background-mpg" => {
                self.background_mpgs = paths(required()?);
                self.enabled = true;
            }
            "blankscreen" => {
                self.blankscreen = Some(required()?.into());
                self.enabled = true;
            }
            "screentext" => {
                self.screen_text = Some(required()?.into());
                self.enabled = true;
            }
            "index-covers" => self.index_covers = Some(required()?.into()),
            "fontname" => {
                self.font_name = required()?.into();
                self.enabled = true;
            }
            "fontname-jp" => {
                self.font_name_jp = Some(required()?.into());
                self.enabled = true;
            }
            "fontname-kr" => {
                self.font_name_kr = Some(required()?.into());
                self.enabled = true;
            }
            "fontsize" => {
                self.font_size = number(required()?, name, 7, 30)? as u16;
                self.enabled = true;
            }
            "fontwidth" => {
                self.font_width = number(required()?, name, 1, 10)? as u16;
                self.enabled = true;
            }
            "font" => {
                let values: Vec<_> = required()?.split(',').collect();
                if values.len() != 3 {
                    return Err("--font requires name,size,width".into());
                }
                self.font_name = values[0].into();
                self.font_size = number(values[1], "fontsize", 7, 30)? as u16;
                self.font_width = number(values[2], "fontwidth", 1, 10)? as u16;
                self.enabled = true;
            }
            "norm" => {
                let norm = required()?.to_ascii_lowercase();
                if !matches!(norm.as_str(), "pal" | "secam" | "ntsc") {
                    return Err("--norm requires pal, secam or ntsc".into());
                }
                self.norm = norm;
            }
            "aspect" => self.aspect = number(required()?, name, 1, 4)? as u8,
            "stillpics" => self.still_pics = Some(required()?.into()),
            "topvob" => {
                self.top_vob = Some(required()?.into());
                self.enabled = true;
            }
            "stillvob" => self.still_vob = Some(required()?.into()),
            "duration" => {
                let fields: Vec<_> = required()?.split(':').collect();
                if fields.len() != 3 {
                    return Err("--duration requires hh:mm:ss".into());
                }
                self.duration = [
                    number(fields[0], name, 0, 99)? as u8,
                    number(fields[1], name, 0, 59)? as u8,
                    number(fields[2], name, 0, 59)? as u8,
                ];
            }
            "loop" => {
                self.looped = match value {
                    None | Some("1" | "yes" | "true") => true,
                    Some("0" | "no" | "false") => false,
                    _ => return Err("--loop requires 0 or 1".into()),
                }
            }
            "xml" => {
                self.navigation_xml = Some(required()?.into());
                self.enabled = true;
            }
            "spuxml" => {
                self.spu_xmls.push(required()?.into());
                self.enabled = true;
            }
            "image" => {
                self.images = paths(required()?);
                self.enabled = true;
            }
            "highlight" => {
                self.highlights = paths(required()?);
                self.enabled = true;
            }
            "select" => {
                self.selects = paths(required()?);
                self.enabled = true;
            }
            "soundtracks" => {
                let value = required()?;
                // The in-process media backend encodes one soundtrack for each
                // menu page; a single WAV is reused when fewer are specified.
                if value.contains(':') && !Path::new(value).is_file() {
                    return Err("--soundtracks requires a WAV path".into());
                }
                self.soundtrack = Some(value.into());
            }
            "topmenu-palette" => {
                let values = required()?
                    .split(':')
                    .map(|value| u32::from_str_radix(value.trim_start_matches("0x"), 16))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| "Invalid --topmenu-palette")?;
                if !(3..=4).contains(&values.len()) || values.iter().any(|value| *value > 0xffffff)
                {
                    return Err("--topmenu-palette requires text:highlight:select[:background] YCrCb values".into());
                }
                self.palette[..values.len()].copy_from_slice(&values);
            }
            "datadir" => self.data_directory = Some(required()?.into()),
            // Image/media/menu implementations are linked into the Rust
            // runtime. Keep the old asset-builder CLI compatible.
            "bindir" => {
                let _ = required()?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub fn height(&self) -> u16 {
        if self.norm == "ntsc" { 480 } else { 576 }
    }
    fn aspect_name(&self) -> &'static str {
        ["1:1", "4:3", "16:9", "2.21:1"][usize::from(self.aspect - 1)]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Both numbers are one-based, independent of menu page boundaries.
    Track {
        group: usize,
        track: usize,
    },
    Menu(usize),
}
#[derive(Clone, Debug)]
pub struct Button {
    pub label: String,
    pub target: Target,
    pub rectangle: [u16; 4],
    pub baseline: [u16; 2],
    pub thumbnail: bool,
}
#[derive(Clone, Debug)]
pub struct Page {
    pub title: String,
    pub subtitle: String,
    pub index: bool,
    pub height: u16,
    pub buttons: Vec<Button>,
}
#[derive(Clone, Debug)]
pub struct TopMenu {
    pub pages: Vec<Page>,
    pub page_sectors: Vec<u32>,
    pub top_vob_sectors: u32,
    pub video_attribute: u8,
    pub duration: [u8; 3],
    pub looped: bool,
    pub palette: [u32; 4],
}
#[derive(Clone, Debug)]
pub struct Still {
    pub picture_sectors: Vec<u32>,
    pub still_vob_sectors: u32,
    pub video_attribute: u8,
    /// Background, text, highlight, select (ASVS order).
    pub palette: [u32; 4],
}
#[derive(Clone, Debug, Default)]
pub struct Output {
    pub top_menu: Option<TopMenu>,
    pub still: Option<Still>,
    /// Group -> track -> pictures. Empty tracks keep the previous picture.
    pub track_pictures: Vec<Vec<Vec<atsi::StillPicture>>>,
}

fn even(value: u16) -> u16 {
    value & !1
}
fn y(row: usize, rows: usize, height: u16) -> u16 {
    let label_height = (i32::from(height) - 56 - 40 - rows as i32 * 12) / rows as i32;
    (56 + row as i32 * (label_height + 12) + label_height / 2).max(0) as u16
}
fn row_button(label: String, target: Target, row: usize, count: usize, height: u16) -> Button {
    let rows = count + 4;
    let delta = even((height - 60) / (rows as u16 * 2));
    Button {
        label,
        target,
        rectangle: [
            33,
            even(y(row + 1, rows, height).saturating_sub(delta)),
            708,
            even(y(row + 2, rows, height).saturating_sub(delta)),
        ],
        baseline: [44, even(y(row + 1, rows, height))],
        thumbnail: false,
    }
}
fn scaled_y(value: u16, height: u16) -> u16 {
    even((u32::from(value) * u32::from(height) / 576) as u16)
}
fn index_rectangle(cell: usize, height: u16) -> [u16; 4] {
    let x0 = (cell % 4) as u16 * 180 + 5;
    let y0 = scaled_y(60, height) + (cell / 4) as u16 * scaled_y(140, height) + 5;
    [x0, y0, x0 + 170, y0 + scaled_y(140, height) - 10]
}
fn index_arrow(label: &str, target: usize, next: bool, options: &Options) -> Button {
    let x0 = if next { 520 } else { 40 };
    let y0 = scaled_y(496, options.height());
    let y1 = scaled_y(552, options.height());
    let text_width = options.font_width * options.font_size * label.len() as u16 / 10;
    Button {
        label: label.into(),
        target: Target::Menu(target),
        rectangle: [x0, y0, x0 + 160, y1],
        baseline: [
            x0 + 160u16.saturating_sub(text_width) / 2,
            even((y0 + y1 + options.font_size) / 2),
        ],
        thumbnail: false,
    }
}
fn valid_xml(value: &str) -> Result<(), String> {
    if value.chars().all(
        |c| matches!(c as u32, 9 | 10 | 13 | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff),
    ) {
        Ok(())
    } else {
        Err("Menu text or path contains an invalid XML character".into())
    }
}
fn xml(value: &str) -> Result<String, String> {
    valid_xml(value)?;
    Ok(value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;"))
}
fn path_text(path: &Path) -> Result<String, String> {
    let value = path.to_str().ok_or("Menu path is not Unicode")?;
    valid_xml(value)?;
    // MinGW's vendor filesystem helpers mix '/' with their root argument and
    // cannot use std::fs::canonicalize's extended Windows path syntax. Magick
    // also treats backslashes as escapes in font and annotation options.
    let value = if let Some(unc) = value.strip_prefix("\\\\?\\UNC\\") {
        format!("//{unc}")
    } else {
        value.strip_prefix("\\\\?\\").unwrap_or(value).to_owned()
    };
    Ok(value.replace('\\', "/"))
}
fn check(callbacks: &mut dyn Callbacks) -> Result<(), String> {
    if callbacks.cancelled() {
        Err("Authoring cancelled".into())
    } else {
        Ok(())
    }
}

/// Build all page targets and coordinates together so rendered masks and both
/// XML documents use precisely the same button ordering and rectangles.
pub fn plan(groups: &[Vec<Track>], options: &Options) -> Result<Vec<Page>, String> {
    if groups.is_empty() || groups.len() > 9 || groups.iter().any(Vec::is_empty) {
        return Err("Menus require 1..=9 nonempty audio groups".into());
    }
    if options.aspect == 0
        || options.aspect > 4
        || options.font_size < 7
        || options.font_size > 30
        || options.font_width == 0
        || options.font_width > 10
    {
        return Err("Invalid menu font or aspect settings".into());
    }
    let all_tracks: Vec<_> = groups
        .iter()
        .enumerate()
        .flat_map(|(group, tracks)| {
            tracks.iter().enumerate().map(move |(track, value)| {
                (
                    value,
                    Target::Track {
                        group: group + 1,
                        track: track + 1,
                    },
                )
            })
        })
        .collect();
    let mut title = String::from("Album");
    let mut segments = Vec::<(String, Vec<String>)>::new();
    if let Some(text) = &options.screen_text {
        valid_xml(text)?;
        if let Some((header, body)) = text.split_once('=') {
            if !header.is_empty() {
                title = header.into();
            }
            for segment in body.split(':') {
                let (subtitle, tracks) = segment.split_once('=').unwrap_or((segment, ""));
                segments.push((
                    subtitle.into(),
                    if tracks.is_empty() {
                        Vec::new()
                    } else {
                        tracks.split(',').map(str::to_owned).collect()
                    },
                ));
            }
        } else if !text.is_empty() {
            title = text.clone();
        }
    }
    if segments.is_empty()
        || segments
            .iter()
            .skip(options.index_pages)
            .map(|(_, tracks)| tracks.len())
            .sum::<usize>()
            != all_tracks.len()
    {
        if options.index_pages > 0 {
            return Err(
                "Index menu screen text must describe every album page and audio track".into(),
            );
        }
        segments.clear();
        let pages = if options.nmenus > 0 {
            options.nmenus
        } else {
            groups
                .iter()
                .map(|tracks| tracks.len().div_ceil(MAX_ROWS))
                .sum()
        };
        if pages == 0 || pages > all_tracks.len() || pages > 255 {
            return Err("Menu page count cannot represent the audio tracks".into());
        }
        if options.nmenus == 0 {
            for (group, tracks) in groups.iter().enumerate() {
                for chunk in tracks.chunks(MAX_ROWS) {
                    segments.push((
                        format!("GROUP {}", group + 1),
                        chunk
                            .iter()
                            .enumerate()
                            .map(|(index, track)| {
                                if track.name.is_empty() {
                                    format!("Track {}", index + 1)
                                } else {
                                    track.name.clone()
                                }
                            })
                            .collect(),
                    ));
                }
            }
        } else {
            let per_page = all_tracks.len().div_ceil(pages);
            for (page, chunk) in all_tracks.chunks(per_page).enumerate() {
                segments.push((
                    format!("Page {}", page + 1),
                    chunk.iter().map(|(track, _)| track.name.clone()).collect(),
                ));
            }
            // Balanced partitioning always gives exactly the requested pages.
            if segments.len() != pages {
                segments.clear();
                let mut at = 0;
                for page in 0..pages {
                    let count =
                        all_tracks.len() / pages + usize::from(page < all_tracks.len() % pages);
                    segments.push((
                        format!("Page {}", page + 1),
                        all_tracks[at..at + count]
                            .iter()
                            .map(|(track, _)| track.name.clone())
                            .collect(),
                    ));
                    at += count;
                }
            }
        }
    }
    let count = segments.len();
    if count == 0
        || count > 255
        || options.index_pages >= count
        || (options.nmenus > 0 && options.nmenus != count)
    {
        return Err("Screen text page count does not match --nmenus/--index-pages".into());
    }
    if options.index_pages > 0
        && options.index_pages != (count - options.index_pages).div_ceil(INDEX_CELLS)
    {
        return Err("Index page count must fit all album pages in 12-cell grids".into());
    }
    let mut pages = Vec::with_capacity(count);
    let mut audio_track = 0;
    for (page, (subtitle, labels)) in segments.into_iter().enumerate() {
        let index = page < options.index_pages;
        let max_rows = if index {
            INDEX_CELLS
        } else {
            MAX_ROWS - usize::from(options.index_pages > 0)
        };
        if labels.is_empty() || labels.len() > max_rows {
            return Err(format!(
                "Menu page {} requires 1..={max_rows} entries",
                page + 1
            ));
        }
        let rows = labels.len();
        let mut buttons = Vec::new();
        for (row, label) in labels.into_iter().enumerate() {
            valid_xml(&label)?;
            if index {
                let target = options.index_pages + page * INDEX_CELLS + row + 1;
                if target > count {
                    return Err("Index cell has no corresponding album page".into());
                }
                let rectangle = index_rectangle(row, options.height());
                buttons.push(Button {
                    label,
                    target: Target::Menu(target),
                    rectangle,
                    baseline: [0; 2],
                    thumbnail: true,
                });
            } else {
                let (track, target) = &all_tracks[audio_track];
                let label = if label.is_empty() {
                    track.name.clone()
                } else {
                    label
                };
                buttons.push(row_button(
                    label,
                    target.clone(),
                    row,
                    rows,
                    options.height(),
                ));
                audio_track += 1;
            }
        }
        let next = if index {
            page + 1 < options.index_pages
        } else {
            page + 1 < count
        };
        let previous = if index {
            page > 0
        } else {
            page > options.index_pages
        };
        if next || previous {
            let label = if next { "Next" } else { "Previous" };
            let target = if next { page + 2 } else { page };
            buttons.push(if index {
                index_arrow(label, target, next, options)
            } else {
                row_button(
                    label.into(),
                    Target::Menu(target),
                    rows,
                    rows,
                    options.height(),
                )
            });
        }
        if next && previous {
            buttons.push(if index {
                index_arrow("Previous", page, false, options)
            } else {
                row_button(
                    "Previous".into(),
                    Target::Menu(page),
                    rows + 1,
                    rows,
                    options.height(),
                )
            });
        }
        if !index && options.index_pages > 0 {
            buttons.push(row_button(
                "Menu".into(),
                Target::Menu(1),
                rows + 2,
                rows,
                options.height(),
            ));
        }
        if buttons.len() > 36
            || buttons.iter().any(|button| {
                let [x0, y0, x1, y1] = button.rectangle;
                x0 >= x1 || y0 >= y1 || x1 >= WIDTH || y1 >= options.height()
            })
        {
            return Err(format!(
                "Menu page {} has invalid button coordinates",
                page + 1
            ));
        }
        pages.push(Page {
            title: title.clone(),
            subtitle,
            index,
            height: options.height(),
            buttons,
        });
    }
    if audio_track != all_tracks.len() {
        return Err("Not all audio tracks have menu buttons".into());
    }
    Ok(pages)
}

pub fn subpicture_xml(
    page: &Page,
    image: &Path,
    highlight: &Path,
    select: &Path,
) -> Result<String, String> {
    let mut document = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<subpictures format=\"{}\">\n  <stream>\n    <spu highlight=\"{}\" force=\"yes\" start=\"00:00:00.00\" select=\"{}\" image=\"{}\">\n",
        if page.height == 480 { "NTSC" } else { "PAL" },
        xml(&path_text(highlight)?)?,
        xml(&path_text(select)?)?,
        xml(&path_text(image)?)?
    );
    for (index, button) in page.buttons.iter().enumerate() {
        let [x0, y0, x1, y1] = button.rectangle;
        writeln!(
            document,
            "      <button x0=\"{x0}\" y0=\"{y0}\" x1=\"{x1}\" name=\"button{:02}\" y1=\"{y1}\"/>",
            index + 1
        )
        .unwrap();
    }
    document.push_str("    </spu>\n  </stream>\n</subpictures>\n");
    Ok(document)
}
pub fn navigation_xml(
    pages: &[Page],
    menus: &[PathBuf],
    options: &Options,
) -> Result<String, String> {
    if pages.len() != menus.len() {
        return Err("Menu XML pages and streams differ in length".into());
    }
    let mut document = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<dvdauthor jumppad=\"1\">\n <amgm>\n  <menus>\n   <video format=\"{}\" />\n   <audio format=\"mp2\" lang=\"en\" />\n",
        if options.norm == "ntsc" {
            "ntsc"
        } else {
            "pal"
        }
    );
    for (page, menu) in pages.iter().zip(menus) {
        document.push_str("   <pgc>\n");
        for (index, button) in page.buttons.iter().enumerate() {
            let command = match button.target {
                Target::Track { group, track } => format!("jump group {group} track {track};"),
                Target::Menu(menu) => format!("jump menu {menu};"),
            };
            writeln!(
                document,
                "    <button name=\"button{:02}\">{command}</button>",
                index + 1
            )
            .unwrap();
        }
        writeln!(
            document,
            "    <vob pause=\"inf\" file=\"{}\"/>\n   </pgc>",
            xml(&path_text(menu)?)?
        )
        .unwrap();
    }
    document.push_str("  </menus>\n </amgm>\n</dvdauthor>\n");
    Ok(document)
}

fn font_for<'a>(text: &str, options: &'a Options) -> &'a str {
    if text
        .chars()
        .any(|c| matches!(c as u32, 0xac00..=0xd7a3 | 0x1100..=0x11ff))
        && let Some(font) = &options.font_name_kr
    {
        return font;
    }
    if text.chars().any(|c| matches!(c as u32, 0x3040..=0x30ff))
        && let Some(font) = &options.font_name_jp
    {
        return font;
    }
    &options.font_name
}
fn literal_text(text: &str) -> String {
    // ImageMagick expands image properties and @file syntax inside annotation
    // strings. Treat author labels as text even when they contain those forms.
    text.replace('\\', "\\\\")
        .replace('%', "%%")
        .replace('@', "\\@")
}
fn text_args(
    args: &mut Vec<String>,
    text: &str,
    x: u16,
    y: u16,
    size: u16,
    color: &str,
    options: &Options,
) {
    args.extend([
        "-stroke".into(),
        "none".into(),
        "-fill".into(),
        color.into(),
        "-font".into(),
        font_for(text, options).replace('\\', "/"),
        "-pointsize".into(),
        size.to_string(),
        "-gravity".into(),
        "NorthWest".into(),
        "-annotate".into(),
        format!("+{x}+{y}"),
        literal_text(text),
    ]);
}
fn baseline_text(
    args: &mut Vec<String>,
    text: &str,
    x: u16,
    baseline: u16,
    size: u16,
    color: &str,
    options: &Options,
) {
    // NorthWest annotation uses a top coordinate rather than MVG's baseline.
    text_args(
        args,
        text,
        x,
        baseline.saturating_sub(size),
        size,
        color,
        options,
    );
}
fn caption_size(label: &str, options: &Options) -> u16 {
    let units: f32 = label
        .chars()
        .map(|c| if (c as u32) >= 0x2e80 { 1.0 } else { 0.55 })
        .sum();
    let maximum = (17 * options.height() / 576).max(9);
    (if units > 0.0 {
        (170.0 / units) as u16
    } else {
        maximum
    })
    .clamp(9, maximum)
}

fn run_image(
    backend: &mut dyn Backend,
    args: Vec<String>,
    callbacks: &mut dyn Callbacks,
) -> Result<(), String> {
    check(callbacks)?;
    backend.image_run(args, callbacks)?;
    check(callbacks)
}
fn make_index_background(
    page: &Page,
    rank: usize,
    covers: &[PathBuf],
    output: &Path,
    options: &Options,
    backend: &mut dyn Backend,
    callbacks: &mut dyn Callbacks,
) -> Result<(), String> {
    let height = options.height();
    let mut args: Vec<String> = ["magick", "+antialias", "-size"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    args.extend([
        format!("720x{height}"),
        "(".into(),
        "xc:rgb(62,107,138)".into(),
        "-sparse-color".into(),
        "bilinear".into(),
        format!("0,0 rgb(62,107,138) 719,{} rgb(34,22,48)", height - 1),
        "-stroke".into(),
        "none".into(),
        "-fill".into(),
        "rgba(255,255,255,0.14)".into(),
    ]);
    let mut grid = String::new();
    for col in 1..4 {
        write!(
            grid,
            "rectangle {},0 {},{} ",
            col * 180,
            col * 180,
            height - 1
        )
        .unwrap();
    }
    for row in 0..=3 {
        let at = scaled_y(60, height) + row * scaled_y(140, height);
        write!(grid, "rectangle 0,{at} 719,{at} ").unwrap();
    }
    args.extend([
        "-draw".into(),
        grid,
        "(".into(),
        "-size".into(),
        format!("720x{height}"),
        "radial-gradient:#ffffff-#8090a0".into(),
        ")".into(),
        "-compose".into(),
        "multiply".into(),
        "-composite".into(),
        "-compose".into(),
        "over".into(),
        "-colorspace".into(),
        "sRGB".into(),
        "-type".into(),
        "TrueColor".into(),
        ")".into(),
    ]);
    let thumb = scaled_y(100, height).max(50);
    let label_height = scaled_y(28, height).max(20);
    let mut borders = String::new();
    for (cell, button) in page
        .buttons
        .iter()
        .filter(|button| button.thumbnail)
        .enumerate()
    {
        let [x0, y0, _, _] = button.rectangle;
        let tx = x0 + (170 - thumb) / 2;
        if let Some(cover) = covers.get(rank * INDEX_CELLS + cell) {
            args.extend([
                "(".into(),
                path_text(cover)?,
                "-resize".into(),
                format!("{thumb}x{thumb}^"),
                "-gravity".into(),
                "center".into(),
                "-extent".into(),
                format!("{thumb}x{thumb}"),
                "-repage".into(),
                format!("+{tx}+{y0}"),
                ")".into(),
            ]);
        }
        args.extend([
            "(".into(),
            "-background".into(),
            "none".into(),
            "-stroke".into(),
            "none".into(),
            "-fill".into(),
            "white".into(),
            "-font".into(),
            font_for(&button.label, options).replace('\\', "/"),
            "-pointsize".into(),
            caption_size(&button.label, options).to_string(),
            "-size".into(),
            format!("170x{label_height}"),
            "-gravity".into(),
            "center".into(),
            format!("caption:{}", literal_text(&button.label)),
            "-repage".into(),
            format!("+{x0}+{}", y0 + thumb + 2),
            ")".into(),
        ]);
        write!(
            borders,
            "rectangle {tx},{y0} {},{} ",
            tx + thumb - 1,
            y0 + thumb - 1
        )
        .unwrap();
    }
    args.extend([
        "-flatten".into(),
        "-stroke".into(),
        "rgb(42,42,42)".into(),
        "-strokewidth".into(),
        "2".into(),
        "-fill".into(),
        "none".into(),
        "-draw".into(),
        borders,
        "-colorspace".into(),
        "sRGB".into(),
        "-type".into(),
        "TrueColor".into(),
        "-quality".into(),
        "92".into(),
        path_text(output)?,
    ]);
    run_image(backend, args, callbacks)
}

#[allow(clippy::too_many_arguments)]
fn make_masks(
    page: &Page,
    blank: &Path,
    image: &Path,
    highlight: &Path,
    select: &Path,
    options: &Options,
    backend: &mut dyn Backend,
    callbacks: &mut dyn Callbacks,
) -> Result<(), String> {
    let mut image_args = vec!["magick".into(), path_text(blank)?, "+antialias".into()];
    let mut highlight_args = image_args.clone();
    for args in [&mut image_args, &mut highlight_args] {
        baseline_text(args, &page.title, 46, 50, 25, "black", options);
        baseline_text(args, &page.title, 44, 48, 25, "white", options);
        if !page.index {
            let rows = page
                .buttons
                .iter()
                .filter(|button| matches!(button.target, Target::Track { .. }))
                .count();
            let baseline = even(y(0, rows + 4, options.height()));
            baseline_text(
                args,
                &page.subtitle,
                46,
                baseline + 2,
                options.font_size * 4 / 5,
                "black",
                options,
            );
            baseline_text(
                args,
                &page.subtitle,
                44,
                baseline,
                options.font_size * 4 / 5,
                "white",
                options,
            );
        }
    }
    for button in &page.buttons {
        if button.thumbnail {
            let [x0, y0, x1, y1] = button.rectangle;
            highlight_args.extend([
                "-fill".into(),
                "none".into(),
                "-stroke".into(),
                "red".into(),
                "-strokewidth".into(),
                "6".into(),
                "-draw".into(),
                format!("rectangle {x0},{y0} {x1},{y1}"),
            ]);
        } else {
            let [x, baseline] = button.baseline;
            let size = options.font_size.min(
                button.rectangle[3]
                    .saturating_sub(button.rectangle[1])
                    .saturating_sub(3)
                    .max(7),
            );
            for args in [&mut image_args, &mut highlight_args] {
                baseline_text(
                    args,
                    &button.label,
                    x + 2,
                    baseline + 2,
                    size,
                    "black",
                    options,
                );
                baseline_text(args, &button.label, x, baseline, size, "white", options);
            }
            let ax = x.saturating_sub(11);
            let ay = baseline.saturating_sub(size / 2);
            highlight_args.extend([
                "-stroke".into(),
                "none".into(),
                "-fill".into(),
                "red".into(),
                "-draw".into(),
                format!(
                    "polygon {ax},{ay} {},{} {},{}",
                    ax + 7,
                    ay.saturating_sub(6),
                    ax + 7,
                    ay + 6
                ),
            ]);
        }
    }
    image_args.extend([
        "-depth".into(),
        "8".into(),
        format!("PNG32:{}", path_text(image)?),
    ]);
    highlight_args.extend([
        "-depth".into(),
        "8".into(),
        format!("PNG32:{}", path_text(highlight)?),
    ]);
    run_image(backend, image_args, callbacks)?;
    run_image(backend, highlight_args, callbacks)?;
    // The three layers share the same white glyphs and black shadows. Only
    // the highlighted layer adds red motifs, staying within four SPU tuples.
    fs::copy(image, select).map_err(|error| error.to_string())?;
    Ok(())
}

fn choose(paths: &[PathBuf], rank: usize) -> Option<&Path> {
    paths
        .get(rank)
        .or_else(|| paths.last())
        .map(PathBuf::as_path)
}
#[allow(clippy::too_many_arguments)]
fn encode_picture(
    input: &Path,
    y4m: &Path,
    output: &Path,
    wav: &Path,
    still: bool,
    options: &Options,
    backend: &mut dyn Backend,
    callbacks: &mut dyn Callbacks,
) -> Result<(), String> {
    check(callbacks)?;
    backend.write_y4m(input, y4m, &options.norm, options.aspect_name(), callbacks)?;
    check(callbacks)?;
    let result = backend.create_mpg(
        y4m,
        wav,
        output,
        &options.norm,
        &options.aspect.to_string(),
        still,
        callbacks,
    );
    let _ = fs::remove_file(y4m);
    result?;
    check(callbacks)
}
fn sector_count(path: &Path) -> Result<u32, String> {
    let bytes = fs::metadata(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .len();
    if bytes == 0 || !bytes.is_multiple_of(SECTOR) {
        return Err(format!(
            "{} is not a complete 2048-byte-sector VOB",
            path.display()
        ));
    }
    u32::try_from(bytes / SECTOR).map_err(|_| "VOB sector count overflow".into())
}
fn silence(output: &Path, duration: [u8; 3], callbacks: &mut dyn Callbacks) -> Result<(), String> {
    let seconds =
        (u64::from(duration[0]) * 3600 + u64::from(duration[1]) * 60 + u64::from(duration[2]))
            .max(1);
    let bytes = seconds
        .checked_mul(48_000 * 4)
        .filter(|bytes| *bytes <= u64::from(u32::MAX) - 36)
        .ok_or("Menu soundtrack duration exceeds WAV capacity")? as u32;
    let mut file = File::create(output).map_err(|error| error.to_string())?;
    file.write_all(b"RIFF")
        .and_then(|()| file.write_all(&(36 + bytes).to_le_bytes()))
        .and_then(|()| file.write_all(b"WAVEfmt \x10\0\0\0\x01\0\x02\0"))
        .and_then(|()| file.write_all(&48_000u32.to_le_bytes()))
        .and_then(|()| file.write_all(&192_000u32.to_le_bytes()))
        .and_then(|()| file.write_all(b"\x04\0\x10\0data"))
        .and_then(|()| file.write_all(&bytes.to_le_bytes()))
        .map_err(|error| error.to_string())?;
    let zeroes = [0; 64 * 1024];
    let mut remaining = bytes as usize;
    while remaining > 0 {
        check(callbacks)?;
        let count = remaining.min(zeroes.len());
        file.write_all(&zeroes[..count])
            .map_err(|error| error.to_string())?;
        remaining -= count;
    }
    Ok(())
}

/// Inspect the produced stream rather than assuming every backend made PAL.
pub fn video_attribute(path: &Path, norm: &str) -> Result<u8, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(65_536)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    for position in 0..bytes.len().saturating_sub(11) {
        if bytes[position..position + 4] != [0, 0, 1, 0xb3] {
            continue;
        }
        let data = &bytes[position + 4..];
        let height = (u16::from(data[1] & 15) << 8) | u16::from(data[2]);
        let ntsc = match height {
            480 | 240 => true,
            576 | 288 => false,
            _ => !matches!(data[3] & 15, 3 | 6),
        };
        return Ok(if ntsc { 0x43 } else { 0x53 });
    }
    Ok(if norm == "ntsc" { 0x43 } else { 0x53 })
}

/// Match the C author's three still-stream repairs, before measuring sectors.
/// All edits preserve byte count except moving a trailing program-end code
/// into its own sector. Backends already producing that form remain unchanged.
pub fn repair_still(path: &Path) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    let size = file.metadata().map_err(|error| error.to_string())?.len();
    if size < SECTOR || !size.is_multiple_of(SECTOR) {
        return Err("Still VOB is not sector aligned".into());
    }
    file.seek(SeekFrom::End(-4))
        .map_err(|error| error.to_string())?;
    let mut tail = [0; 4];
    file.read_exact(&mut tail)
        .map_err(|error| error.to_string())?;
    if tail == [0, 0, 1, 0xb9] {
        file.seek(SeekFrom::End(-4))
            .and_then(|_| file.write_all(&[255; 4]))
            .map_err(|error| error.to_string())?;
        let mut sector = [255; SECTOR as usize];
        sector[..4].copy_from_slice(&tail);
        file.seek(SeekFrom::End(0))
            .and_then(|_| file.write_all(&sector))
            .map_err(|error| error.to_string())?;
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut nav = [0; SECTOR as usize];
    file.read_exact(&mut nav)
        .map_err(|error| error.to_string())?;
    if nav[0x400..0x404] == [0, 0, 1, 0xbf] {
        nav[14..20].copy_from_slice(&[0, 0, 1, 0xbb, 0, 15]);
        nav[20..35].copy_from_slice(&[
            0x80, 0xc4, 0xe1, 0, 0x61, 0x7f, 0xb9, 0xe0, 0xe8, 0xbd, 0xe0, 0x34, 0xbf, 0xe0, 1,
        ]);
        nav[35..41].copy_from_slice(&[0, 0, 1, 0xbf, 2, 0xe9]);
        nav[41..786].fill(0);
        nav[41] = 2;
        nav[46] = 0x8c;
        nav[47] = 0xa0;
        nav[48..56].fill(255);
        nav[786..792].copy_from_slice(&[0, 0, 1, 0xbe, 4, 0xe8]);
        nav[792..].fill(255);
        file.seek(SeekFrom::Start(0))
            .and_then(|_| file.write_all(&nav))
            .map_err(|error| error.to_string())?;
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    (&mut file)
        .take(65_536)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    let mut sequence = None;
    let mut picture = None;
    for at in 0..bytes.len().saturating_sub(10) {
        if bytes[at..at + 4] != [0, 0, 1, 0xb5] {
            continue;
        }
        match bytes[at + 4] >> 4 {
            1 if sequence.is_none() => sequence = Some(at),
            8 if picture.is_none() => picture = Some(at),
            _ => (),
        }
        if sequence.is_some() && picture.is_some() {
            break;
        }
    }
    if let (Some(sequence), Some(picture)) = (sequence, picture)
        && (bytes[picture + 6] >> 6) & 3 == 3
        && bytes[picture + 7] & 0x40 != 0
        && bytes[picture + 8] & 0x80 != 0
        && bytes[sequence + 5] & 8 == 0
    {
        file.seek(SeekFrom::Start((sequence + 5) as u64))
            .and_then(|_| file.write_all(&[bytes[sequence + 5] | 8]))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn picture_paths(
    groups: &[Vec<Track>],
    options: &Options,
) -> Result<Vec<Vec<Vec<PathBuf>>>, String> {
    let count: usize = groups.iter().map(Vec::len).sum();
    let mut flat = vec![Vec::new(); count];
    if let Some(value) = &options.still_pics {
        if Path::new(value).is_dir() {
            let mut names = fs::read_dir(value)
                .map_err(|error| error.to_string())?
                .map(|entry| {
                    let entry = entry.map_err(|error| error.to_string())?;
                    let kind = entry.file_type().map_err(|error| error.to_string())?;
                    if !kind.is_file() {
                        return Err(
                            "Still-picture directory must contain regular image files only".into(),
                        );
                    }
                    Ok(entry.path())
                })
                .collect::<Result<Vec<_>, String>>()?;
            names.sort();
            if names.len() != count {
                return Err(
                    "Still-picture directory must contain one image per audio track".into(),
                );
            }
            for (track, path) in flat.iter_mut().zip(names) {
                track.push(path);
            }
        } else {
            let entries: Vec<_> = value.split(';').collect();
            if entries.len() != count {
                return Err(format!(
                    "--stillpics describes {} tracks; audio has {count}",
                    entries.len()
                ));
            }
            for (pictures, entry) in flat.iter_mut().zip(entries) {
                *pictures = entry
                    .split(',')
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
                    .collect();
                if pictures.len() > 99 {
                    return Err("A track supports at most 99 still pictures".into());
                }
            }
        }
    }
    let mut flat = flat.into_iter();
    Ok(groups
        .iter()
        .map(|tracks| tracks.iter().map(|_| flat.next().unwrap()).collect())
        .collect())
}
fn concatenate(
    paths: &[PathBuf],
    output: &Path,
    callbacks: &mut dyn Callbacks,
) -> Result<(), String> {
    let mut file = File::create(output).map_err(|error| error.to_string())?;
    let mut buffer = [0; 64 * 1024];
    for path in paths {
        let mut input = File::open(path).map_err(|error| error.to_string())?;
        loop {
            check(callbacks)?;
            let count = input.read(&mut buffer).map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            file.write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
        }
    }
    file.flush().map_err(|error| error.to_string())
}

/// Produce AUDIO_TS.VOB / AUDIO_SV.VOB plus metadata for AMG, ASVS and ATSI.
pub fn author(
    audio_ts: &Path,
    temporary: &Path,
    groups: &[Vec<Track>],
    options: &Options,
    backend: &mut dyn Backend,
    callbacks: &mut dyn Callbacks,
) -> Result<Output, String> {
    check(callbacks)?;
    let pictures = picture_paths(groups, options)?;
    let mut result = Output {
        track_pictures: pictures
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|pictures| {
                        pictures
                            .iter()
                            .map(|_| atsi::StillPicture::default())
                            .collect()
                    })
                    .collect()
            })
            .collect(),
        ..Output::default()
    };
    if !options.enabled
        && pictures.iter().flatten().all(Vec::is_empty)
        && options.still_vob.is_none()
    {
        return Ok(result);
    }
    fs::create_dir_all(audio_ts).map_err(|error| error.to_string())?;
    fs::create_dir_all(temporary).map_err(|error| error.to_string())?;
    let temporary = PathBuf::from(path_text(
        &temporary
            .canonicalize()
            .map_err(|error| error.to_string())?,
    )?);
    let audio_ts = PathBuf::from(path_text(
        &audio_ts.canonicalize().map_err(|error| error.to_string())?,
    )?);
    let wav = if let Some(path) = &options.soundtrack {
        path.clone()
    } else {
        let path = temporary.join("menu-silence.wav");
        silence(&path, options.duration, callbacks)?;
        path
    };
    if options.enabled {
        let pages = plan(groups, options)?;
        callbacks.emit(
            1,
            &format!("[menu] Authoring {} menu page(s)\n", pages.len()),
        );
        let blank = temporary.join("menu-blank.png");
        if let Some(input) = &options.blankscreen {
            run_image(
                backend,
                vec![
                    "magick".into(),
                    path_text(input)?,
                    "-resize".into(),
                    format!("720x{}!", options.height()),
                    "-depth".into(),
                    "8".into(),
                    format!("PNG32:{}", path_text(&blank)?),
                ],
                callbacks,
            )?;
        } else {
            run_image(
                backend,
                vec![
                    "magick".into(),
                    "-size".into(),
                    format!("720x{}", options.height()),
                    "xc:none".into(),
                    "-depth".into(),
                    "8".into(),
                    format!("PNG32:{}", path_text(&blank)?),
                ],
                callbacks,
            )?;
        }
        let covers = if let Some(path) = &options.index_covers {
            fs::read_to_string(path)
                .map_err(|error| format!("Index cover list {}: {error}", path.display()))?
                .lines()
                .filter(|line| !line.is_empty())
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut menus = Vec::new();
        let mut page_sectors = Vec::new();
        for (rank, page) in pages.iter().enumerate() {
            check(callbacks)?;
            let menu = if let Some(path) = choose(&options.top_menus, rank) {
                path.to_path_buf()
            } else {
                let background = if let Some(path) = choose(&options.background_mpgs, rank) {
                    path.to_path_buf()
                } else {
                    let picture = temporary.join(format!("menu-background-{rank}.jpg"));
                    if page.index {
                        make_index_background(
                            page, rank, &covers, &picture, options, backend, callbacks,
                        )?;
                    } else {
                        let content_rank =
                            if options.backgrounds.len() == pages.len() - options.index_pages {
                                rank - options.index_pages
                            } else {
                                rank
                            };
                        let mut args = vec!["magick".into()];
                        if let Some(input) = choose(&options.backgrounds, content_rank) {
                            args.push(path_text(input)?);
                        } else {
                            args.extend([
                                "-size".into(),
                                format!("720x{}", options.height()),
                                "xc:black".into(),
                            ]);
                        }
                        args.extend([
                            "-resize".into(),
                            format!("720x{}!", options.height()),
                            "-colorspace".into(),
                            "sRGB".into(),
                            "-quality".into(),
                            "92".into(),
                            path_text(&picture)?,
                        ]);
                        run_image(backend, args, callbacks)?;
                    }
                    let path = temporary.join(format!("menu-background-{rank}.mpg"));
                    encode_picture(
                        &picture,
                        &temporary.join(format!("menu-{rank}.y4m")),
                        &path,
                        &wav,
                        false,
                        options,
                        backend,
                        callbacks,
                    )?;
                    path
                };
                let image = temporary.join(format!("impic{rank}.png"));
                let highlight = temporary.join(format!("hlpic{rank}.png"));
                let select = temporary.join(format!("selpic{rank}.png"));
                make_masks(
                    page, &blank, &image, &highlight, &select, options, backend, callbacks,
                )?;
                for (paths, target) in [
                    (&options.images, &image),
                    (&options.highlights, &highlight),
                    (&options.selects, &select),
                ] {
                    if let Some(input) = choose(paths, rank) {
                        fs::copy(input, target).map_err(|error| error.to_string())?;
                    }
                }
                let spu = if let Some(path) = choose(&options.spu_xmls, rank) {
                    path.to_path_buf()
                } else {
                    let path = temporary.join(format!("spu_xmltemp_{rank}.xml"));
                    fs::write(&path, subpicture_xml(page, &image, &highlight, &select)?)
                        .map_err(|error| error.to_string())?;
                    path
                };
                let path = temporary.join(format!("menu-top-{rank}.vob"));
                backend.subpictures(&spu, &background, &path, callbacks)?;
                check(callbacks)?;
                path
            };
            page_sectors.push(sector_count(&menu)?);
            menus.push(menu);
            callbacks.progress((rank + 1) as u64, pages.len() as u64);
        }
        let output = audio_ts.join("AUDIO_TS.VOB");
        if let Some(input) = &options.top_vob {
            fs::copy(input, &output).map_err(|error| error.to_string())?;
        } else {
            let navigation = if let Some(path) = &options.navigation_xml {
                path.clone()
            } else {
                let path = temporary.join("xmltemp");
                fs::write(&path, navigation_xml(&pages, &menus, options)?)
                    .map_err(|error| error.to_string())?;
                path
            };
            backend.navigation(
                &navigation,
                audio_ts.parent().ok_or("AUDIO_TS requires a disc root")?,
                callbacks,
            )?;
        }
        check(callbacks)?;
        result.top_menu = Some(TopMenu {
            top_vob_sectors: sector_count(&output)?,
            video_attribute: video_attribute(&output, &options.norm)?,
            pages,
            page_sectors,
            duration: options.duration,
            looped: options.looped,
            palette: options.palette,
        });
    }
    let flat: Vec<_> = pictures.iter().flatten().flatten().collect();
    if !flat.is_empty() || options.still_vob.is_some() {
        callbacks.emit(
            1,
            &format!("[menu] Authoring {} still picture(s)\n", flat.len()),
        );
        let output = audio_ts.join("AUDIO_SV.VOB");
        let mut sizes = Vec::new();
        if let Some(input) = &options.still_vob {
            if flat.is_empty() {
                return Err(
                    "--stillvob requires --stillpics to describe per-track picture counts".into(),
                );
            }
            let total = sector_count(input)?;
            if !(total as usize).is_multiple_of(flat.len()) {
                return Err(
                    "Imported still VOB cannot be split into equal-sized picture extents".into(),
                );
            }
            sizes.resize(flat.len(), total / flat.len() as u32);
            fs::copy(input, &output).map_err(|error| error.to_string())?;
        } else {
            let mut streams = Vec::new();
            for (rank, input) in flat.iter().enumerate() {
                check(callbacks)?;
                let picture = temporary.join(format!("still-picture-{rank}.jpg"));
                run_image(
                    backend,
                    vec![
                        "magick".into(),
                        path_text(input)?,
                        "-resize".into(),
                        format!("720x{}!", options.height()),
                        "-colorspace".into(),
                        "sRGB".into(),
                        "-quality".into(),
                        "92".into(),
                        path_text(&picture)?,
                    ],
                    callbacks,
                )?;
                let stream = temporary.join(format!("still-picture-{rank}.mpg"));
                encode_picture(
                    &picture,
                    &temporary.join(format!("still-{rank}.y4m")),
                    &stream,
                    &wav,
                    true,
                    options,
                    backend,
                    callbacks,
                )?;
                repair_still(&stream)?;
                let size = sector_count(&stream)?;
                if size > 1024 {
                    callbacks.emit(2, "[menu] Still image exceeds the 2 MiB picture limit\n");
                }
                sizes.push(size);
                streams.push(stream);
                callbacks.progress((rank + 1) as u64, flat.len() as u64);
            }
            concatenate(&streams, &output, callbacks)?;
        }
        check(callbacks)?;
        result.still = Some(Still {
            still_vob_sectors: sector_count(&output)?,
            picture_sectors: sizes,
            video_attribute: video_attribute(&output, &options.norm)?,
            palette: [
                options.palette[3],
                options.palette[0],
                options.palette[1],
                options.palette[2],
            ],
        });
    }
    Ok(result)
}
