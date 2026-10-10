use crate::{build::Job, disc::Disc};
use dvda_native::{images::Images, media::Callbacks};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde_json::{Value, json};

fn is_wide_unit(value: u16) -> bool {
    (0x2e80..=0x9fff).contains(&value) || value >= 0xff00
}
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn from_units(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

pub fn sanitize(request: Value) -> Result<Value, String> {
    let value = request.as_str().ok_or("Expected menu text")?;
    let input = units(value);
    if !input.iter().any(|unit| matches!(*unit, 0x2c | 0x3a | 0x3d)) {
        return Ok(json!({"Value":value,"Changed":false}));
    }
    let wide = input.iter().any(|unit| is_wide_unit(*unit));
    let result: Vec<u16> = input
        .into_iter()
        .map(|unit| match unit {
            0x2c if wide => 0xff0c,
            0x2c => 0x00b7,
            0x3a if wide => 0xff1a,
            0x3a => 0x00b7,
            0x3d if wide => 0xff1d,
            0x3d => 0x00b7,
            other => other,
        })
        .collect();
    Ok(json!({"Value":from_units(&result),"Changed":true}))
}

fn text_pixels(value: &str, font_size: i64) -> i64 {
    let wide = units(value)
        .iter()
        .filter(|unit| is_wide_unit(**unit))
        .count() as f64;
    let length = units(value).len() as f64;
    (wide * font_size as f64 + (length - wide) * font_size as f64 * 0.55) as i64
}
fn round_even(value: f64) -> f64 {
    let lower = value.floor();
    let fraction = value - lower;
    if fraction < 0.5 || (fraction == 0.5 && (lower as i64) % 2 == 0) {
        lower
    } else {
        lower + 1.0
    }
}
pub fn truncate(request: Value) -> Result<Value, String> {
    let value = text(&request, "Value");
    let font_size = request["FontSize"].as_i64().ok_or("Missing FontSize")?;
    let budget = request["Budget"].as_i64().unwrap_or(660);
    if text_pixels(value, font_size) <= budget {
        return Ok(Value::String(value.to_owned()));
    }
    let mut output = units(value);
    while !output.is_empty() && text_pixels(&(from_units(&output) + "~"), font_size) > budget {
        output.pop();
    }
    let result = if output.is_empty() {
        String::new()
    } else {
        format!("{}~", from_units(&output).trim_end())
    };
    Ok(Value::String(result))
}
pub fn font_size(request: Value) -> Result<Value, String> {
    let rows = request.as_i64().ok_or("Expected row count")?;
    let span = rows + 4;
    let label_height = (576 - 56 - 40 - span * 12) / span;
    Ok(json!((label_height + 2).clamp(7, 30)))
}
pub fn font_width(request: Value) -> Result<Value, String> {
    let texts = request.as_array().ok_or("Expected text array")?;
    let mut wide = 0f64;
    let mut narrow = 0f64;
    let mut bytes = 0f64;
    for item in texts {
        let value = item.as_str().ok_or("Expected text string")?;
        for unit in units(value) {
            if is_wide_unit(unit) {
                wide += 1.0;
                bytes += 3.0
            } else {
                narrow += 1.0;
                bytes += 1.0
            }
        }
    }
    if bytes == 0.0 {
        return Ok(json!(5));
    }
    Ok(json!(
        round_even(10.0 * (wide + 0.5 * narrow) / bytes).clamp(1.0, 10.0) as i64
    ))
}
pub fn short_album(request: Value) -> Result<Value, String> {
    let value = text(&request, "Value");
    let max = request["MaximumCharacters"].as_i64().unwrap_or(24).max(0) as usize;
    let index = value
        .char_indices()
        .find(|(_, c)| matches!(c, '(' | '[' | '（' | '【'))
        .map(|(i, _)| i)
        .unwrap_or(value.len());
    let mut output = value[..index]
        .trim()
        .trim_end_matches(['-', '–', '—'])
        .trim()
        .to_owned();
    if output.is_empty() {
        output = value.to_owned();
    }
    output = from_units(&units(&output).into_iter().take(max).collect::<Vec<_>>());
    Ok(Value::String(output))
}
pub fn normalize_path(request: Value) -> Result<Value, String> {
    Ok(Value::String(
        request.as_str().ok_or("Expected path")?.replace('\\', "/"),
    ))
}

pub fn pages(request: Value) -> Result<Value, String> {
    let tracks = request["Tracks"].as_array().ok_or("Missing Tracks")?;
    let albums = request["Albums"].as_array().ok_or("Missing Albums")?;
    let row_cap = request["RowCap"].as_i64().ok_or("Missing RowCap")?;
    if tracks.len() != albums.len() {
        return Err("Track and album lists must have equal lengths".into());
    }
    if row_cap < 1 {
        return Err("RowCap must be positive".into());
    }
    let mut result = Vec::new();
    let mut block_start = 0usize;
    while block_start < tracks.len() {
        let album = albums[block_start].as_str().unwrap_or("");
        let mut block_end = block_start + 1;
        while block_end < albums.len() && albums[block_end].as_str().unwrap_or("") == album {
            block_end += 1;
        }
        let mut page_start = block_start;
        let mut continuation = false;
        while page_start < block_end {
            let page_end = (page_start + row_cap as usize).min(block_end);
            result.push(json!({"Album":album,"StartTrackIndex":page_start,"EndTrackIndex":page_end,"Continuation":continuation,"Tracks":tracks[page_start..page_end].to_vec()}));
            continuation = true;
            page_start = page_end;
        }
        block_start = block_end;
    }
    Ok(Value::Array(result))
}

pub fn plan(request: Value) -> Result<Value, String> {
    let object = request.as_object().ok_or("Expected menu plan request")?;
    let disc = object
        .get("Disc")
        .and_then(Value::as_object)
        .ok_or("Missing disc")?;
    let groups = disc
        .get("Groups")
        .and_then(Value::as_array)
        .ok_or("Missing disc groups")?;
    let tracks: Vec<Value> = groups
        .iter()
        .flat_map(|group| {
            group
                .get("Tracks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .cloned()
        })
        .collect();
    let albums: Vec<String> = tracks.iter().map(album_of).collect();
    let row_cap = object
        .get("MenuTracksPerPage")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .clamp(1, 24) as usize;
    let pages = menu_pages(&tracks, &albums, row_cap)?;
    let maximum_rows = pages
        .iter()
        .map(|page| {
            page.get("EndTrackIndex")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                - page
                    .get("StartTrackIndex")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
        })
        .max()
        .unwrap_or(1)
        .max(1);
    let font_size = font_size(json!(maximum_rows))?.as_i64().unwrap_or(7);
    let mut index_pages = if pages.is_empty() {
        0
    } else {
        pages.len().div_ceil(12)
    };
    let minimum_albums = object
        .get("MenuIndexMinimumAlbums")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if index_pages > 0 && pages.len() < minimum_albums.max(0) as usize {
        index_pages = 0;
    }
    let index_albums: Vec<Value> = (0..index_pages)
        .map(|page| {
            Value::Array(
                pages[page * 12..(page * 12 + 12).min(pages.len())]
                    .iter()
                    .map(|item| item["Album"].clone())
                    .collect(),
            )
        })
        .collect();
    let page_albums: Vec<String> = pages
        .iter()
        .map(|page| page["Album"].as_str().unwrap_or_default().to_owned())
        .collect();
    let labels = album_labels(&page_albums);
    let mut sanitized = Vec::new();
    let mut chunks = Vec::new();
    let mut displayed = Vec::new();
    for page in &index_albums {
        let names: Vec<String> = page
            .as_array()
            .into_iter()
            .flatten()
            .map(|album| {
                labels
                    .get(album.as_str().unwrap_or_default())
                    .cloned()
                    .unwrap_or_default()
            })
            .map(|label| sanitize_record(&label, &mut sanitized))
            .map(|label| truncate_value(&label, font_size, 660))
            .collect();
        chunks.push(format!("选择专辑={}", names.join(",")));
    }
    for page in &pages {
        let raw_album = page["Album"].as_str().unwrap_or_default();
        let continuation = page["Continuation"].as_bool().unwrap_or(false);
        let album = sanitize_record(
            &format!(
                "{}{}",
                labels.get(raw_album).cloned().unwrap_or_default(),
                if continuation { "（续）" } else { "" }
            ),
            &mut sanitized,
        );
        let titles: Vec<String> = page["Tracks"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|track| {
                let title = track["Title"].as_str().unwrap_or_default();
                if title.trim().is_empty() {
                    basename(track["SourcePath"].as_str().unwrap_or_default())
                } else {
                    title.to_owned()
                }
            })
            .map(|title| sanitize_record(&title, &mut sanitized))
            .map(|title| truncate_value(&title, font_size, 660))
            .collect();
        chunks.push(format!("{}={}", album, titles.join(",")));
        displayed.push(album);
        displayed.extend(titles);
    }
    let title = sanitize(Value::String(
        object
            .get("Title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    ))?["Value"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    displayed.push(title.clone());
    let font_width = font_width(json!(displayed))?.as_i64().unwrap_or(5);
    let track_count: i64 = pages
        .iter()
        .map(|page| {
            page.get("EndTrackIndex")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                - page
                    .get("StartTrackIndex")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
        })
        .sum();
    Ok(json!({
        "TotalPages": index_pages + pages.len(),
        "IndexPages": index_pages,
        "MaximumRows": maximum_rows,
        "FontSize": font_size,
        "FontWidth": font_width,
        "ScreenText": format!("{}={}", title, chunks.join(":")),
        "AlbumPages": pages,
        "IndexAlbums": index_albums,
        "SanitizedValues": unique(sanitized),
        "AlbumPageCount": pages.len(),
        "TrackCount": track_count,
    }))
}

const FRAME_WIDTH: &str = "720";
const FRAME_HEIGHT: &str = "576";

/// Build the on-disk menu assets consumed by the project-built author.
/// ImageMagick is reached only through the bundled C ABI; no image process is
/// started and no shell command is constructed here.
pub fn build_assets(
    job: &Job,
    disc: &Disc,
    callbacks: &mut dyn Callbacks,
    diagnostics: &mut Vec<Value>,
) -> Result<Vec<String>, String> {
    let library = job.image_library.as_deref();
    if !crate::native_components::images_available(library) {
        diagnostics.push(json!({"Severity":2,"Code":"IMAGEMAGICK_MISSING","Message":"内置图像组件缺失，请重新解压或修复发布包。"}));
        diagnostics.push(json!({"Severity":2,"Code":"IMAGEMAGICK_IDENTIFY_MISSING","Message":"内置图像组件缺失，无法验证菜单图片尺寸。"}));
    }
    let data_menu = job.author_source.join("menu");
    if !data_menu.is_dir() {
        diagnostics.push(json!({"Severity":2,"Code":"MENU_DATA_MISSING","Message":format!("找不到 dvda-author 菜单素材目录: {}",data_menu.display())}));
    }
    #[cfg(not(feature = "direct-bridges"))]
    for name in ["dvda-menu-spu.dll", "dvda-menu-nav.dll"] {
        if !job.menu_binary_directory.join(name).is_file() {
            diagnostics.push(json!({"Severity":2,"Code":"MENU_LIBRARY_MISSING","Message":format!("内置菜单组件缺失：{name}。请重新解压或修复发布包。" )}));
        }
    }
    if diagnostics.iter().any(|d| d["Severity"] == 2) {
        return Ok(Vec::new());
    }
    let request = json!({
        "Disc": disc,
        "Title": job.title,
        "MenuTracksPerPage": job.menu_tracks_per_page,
        "MenuIndexMinimumAlbums": job.menu_index_minimum_albums,
    });
    let plan = plan(request)?;
    let directory = job.menu_directory.join(format!("disc{}", disc.number));
    if directory.is_dir() {
        fs::remove_dir_all(&directory).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let images = crate::native_components::images(library)
        .map_err(|error| format!("Could not load menu image library: {error}"))?;
    let configured = [
        resolve_font(&job.menu_font, &job.menu_binary_directory),
        resolve_font(&job.menu_font_japanese, &job.menu_binary_directory),
        resolve_font(&job.menu_font_korean, &job.menu_binary_directory),
    ];
    let fonts = crate::menu_fonts::resolve(
        &images,
        &directory,
        [&configured[0], &configured[1], &configured[2]],
        plan["ScreenText"].as_str().unwrap_or_default(),
        callbacks,
    )?;
    diagnostics.extend(fonts.diagnostics);
    if diagnostics.iter().any(|d| d["Severity"] == 2) {
        return Ok(Vec::new());
    }
    let blank = directory.join("blankscreen.png");
    run_image(
        &images,
        vec![
            "magick".into(),
            "-size".into(),
            format!("{FRAME_WIDTH}x{FRAME_HEIGHT}"),
            "xc:none".into(),
            "-depth".into(),
            "8".into(),
            format!("PNG32:{}", blank.display()),
        ],
        callbacks,
    )?;
    require_frame(&images, &blank, callbacks)?;

    let pages = plan
        .get("AlbumPages")
        .and_then(Value::as_array)
        .ok_or("Menu plan did not contain album pages")?;
    let mut backgrounds = Vec::with_capacity(pages.len());
    let mut covers = HashMap::<String, Option<PathBuf>>::new();
    for page in pages {
        let album = page
            .get("Album")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !covers.contains_key(album) {
            covers.insert(album.to_owned(), page_cover(page));
        }
    }
    let mut missing_covers = covers
        .iter()
        .filter(|(_, cover)| cover.is_none())
        .map(|(album, _)| album.clone())
        .collect::<Vec<_>>();
    missing_covers.sort();
    if !missing_covers.is_empty() {
        diagnostics.push(json!({"Severity":1,"Code":"MENU_COVER_MISSING","Message":format!("{} 张专辑没有 cover.jpg/jpeg/png/webp，页面将使用黑色背景: {}{}",missing_covers.len(),missing_covers.iter().take(3).cloned().collect::<Vec<_>>().join("、"),if missing_covers.len()>3 {"…"}else{""})}));
    }
    for (index, page) in pages.iter().enumerate() {
        let album = page
            .get("Album")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let output = directory.join(format!("bg{index}.jpg"));
        let cover = covers.get(album).and_then(Option::as_deref);
        let mut arguments = vec!["magick".into()];
        if let Some(cover) = cover {
            arguments.extend([
                cover.to_string_lossy().into_owned(),
                "-resize".into(),
                format!("{FRAME_WIDTH}x{FRAME_HEIGHT}^"),
                "-gravity".into(),
                "center".into(),
                "-extent".into(),
                format!("{FRAME_WIDTH}x{FRAME_HEIGHT}"),
            ]);
            if job.menu_cover_dim > 0 {
                arguments.extend([
                    "-brightness-contrast".into(),
                    format!("-{}x0", job.menu_cover_dim.clamp(0, 100)),
                ]);
            }
        } else {
            arguments.extend([
                "-size".into(),
                format!("{FRAME_WIDTH}x{FRAME_HEIGHT}"),
                "xc:black".into(),
            ]);
        }
        arguments.extend([
            "-quality".into(),
            "88".into(),
            output.to_string_lossy().into_owned(),
        ]);
        run_image(&images, arguments, callbacks)?;
        require_frame(&images, &output, callbacks)?;
        callbacks.emit(
            1,
            &format!(
                "[menu-cover] background {}/{} {album}",
                index + 1,
                pages.len()
            ),
        );
        backgrounds.push(output);
    }

    let index_pages = plan.get("IndexPages").and_then(Value::as_i64).unwrap_or(0);
    let index_covers = if index_pages > 0 {
        let path = directory.join("index_covers.txt");
        let albums = plan
            .get("IndexAlbums")
            .and_then(Value::as_array)
            .ok_or("Menu plan did not contain index albums")?;
        let mut lines = Vec::new();
        for page in albums {
            for album in page
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if let Some(cover) = covers.get(album).and_then(Option::as_deref) {
                    lines.push(cover.to_string_lossy().into_owned());
                }
            }
        }
        let expected = albums
            .iter()
            .filter_map(Value::as_array)
            .map(Vec::len)
            .sum::<usize>();
        if lines.len() != expected {
            diagnostics.push(json!({"Severity":2,"Code":"MENU_INDEX_COVER_MISSING","Message":format!("一级菜单按位置解释封面清单，不能压缩缺项；缺少封面的专辑: {}{}",missing_covers.iter().take(5).cloned().collect::<Vec<_>>().join("、"),if missing_covers.len()>5 {"…"}else{""})}));
            diagnostics.push(json!({"Severity":2,"Code":"MENU_INDEX_COVER_COUNT_MISMATCH","Message":format!("索引封面清单 {} 项 != 索引格子 {expected} 项。",lines.len())}));
        }
        fs::write(&path, format!("{}\n", lines.join("\n"))).map_err(|error| error.to_string())?;
        Some(path)
    } else {
        None
    };

    let mut still_pictures = Vec::new();
    if job.menu_still_pictures {
        let still_total = covers.values().filter(|cover| cover.is_some()).count();
        let mut still_by_album = HashMap::<String, String>::new();
        for (index, page) in pages.iter().enumerate() {
            let album = page
                .get("Album")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if still_by_album.contains_key(album) {
                continue;
            }
            let Some(cover) = covers.get(album).and_then(Option::as_deref) else {
                still_by_album.insert(album.to_owned(), String::new());
                continue;
            };
            let output = directory.join(format!("still{index}.jpg"));
            let generated = (|| -> Result<(), String> {
                run_image(
                    &images,
                    vec![
                        "magick".into(),
                        cover.to_string_lossy().into_owned(),
                        "-resize".into(),
                        "93.75%x100%!".into(),
                        "-resize".into(),
                        format!("{FRAME_WIDTH}x{FRAME_HEIGHT}"),
                        "-background".into(),
                        "black".into(),
                        "-gravity".into(),
                        "center".into(),
                        "-extent".into(),
                        format!("{FRAME_WIDTH}x{FRAME_HEIGHT}"),
                        "-quality".into(),
                        "92".into(),
                        output.to_string_lossy().into_owned(),
                    ],
                    callbacks,
                )?;
                require_frame(&images, &output, callbacks)
            })();
            if let Err(reason) = generated {
                if callbacks.cancelled() {
                    return Err("Build cancelled".into());
                }
                diagnostics.push(json!({"Severity":1,"Code":"MENU_STILL_FAILED","Message":format!("生成专辑 {album} 的播放静图失败: {reason}")}));
                let _ = fs::remove_file(&output);
                still_by_album.insert(album.to_owned(), String::new());
                continue;
            }
            still_by_album.insert(album.to_owned(), output.to_string_lossy().into_owned());
            let still_completed = still_by_album
                .values()
                .filter(|path| !path.is_empty())
                .count();
            callbacks.emit(
                1,
                &format!("[menu-cover] still {still_completed}/{still_total} {album}"),
            );
        }
        for page in pages {
            let album = page
                .get("Album")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let count = page
                .get("Tracks")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            still_pictures.extend(std::iter::repeat_n(
                still_by_album.get(album).cloned().unwrap_or_default(),
                count,
            ));
        }
    }

    let mut arguments = vec![
        "--topmenu".into(),
        format!("--nmenus={}", plan["TotalPages"].as_i64().unwrap_or(0)),
        "--blankscreen".into(),
        blank.to_string_lossy().into_owned(),
        "--background".into(),
        backgrounds
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(","),
        "--screentext".into(),
        plan["ScreenText"].as_str().unwrap_or_default().into(),
        "--bindir".into(),
        job.menu_binary_directory.to_string_lossy().into_owned(),
        "--datadir".into(),
        job.author_source.to_string_lossy().into_owned(),
    ];
    if index_pages > 0 {
        arguments.extend(["--index-pages".into(), index_pages.to_string()]);
        if let Some(path) = index_covers {
            arguments.extend(["--index-covers".into(), path.to_string_lossy().into_owned()]);
        }
    }
    for (flag, configured) in [
        ("--fontname", &fonts.font),
        ("--fontname-jp", &fonts.japanese),
        ("--fontname-kr", &fonts.korean),
    ] {
        let font = resolve_font(configured, &job.menu_binary_directory);
        if !font.is_empty() {
            arguments.extend([flag.into(), font]);
        }
    }
    arguments.extend([
        "--fontsize".into(),
        plan["FontSize"].as_i64().unwrap_or(7).to_string(),
        "--fontwidth".into(),
        plan["FontWidth"].as_i64().unwrap_or(5).to_string(),
    ]);
    if still_pictures.iter().any(|path| !path.is_empty()) {
        arguments.extend(["--stillpics".into(), still_pictures.join(";")]);
    }
    let expected_pages = plan["TotalPages"].as_u64().unwrap_or(0) as usize;
    if !blank.is_file() {
        diagnostics.push(json!({"Severity":2,"Code":"MENU_BLANKSCREEN_MISSING","Message":"未生成菜单透明底图。"}));
    }
    if backgrounds.len() != pages.len() || backgrounds.iter().any(|path| !path.is_file()) {
        diagnostics.push(json!({"Severity":2,"Code":"MENU_BACKGROUND_MISMATCH","Message":format!("菜单背景图数量或文件不完整: {}/{}。",backgrounds.len(),pages.len())}));
    }
    let tracks = plan["TrackCount"].as_u64().unwrap_or(0) as usize;
    if !still_pictures.is_empty() && still_pictures.len() != tracks {
        diagnostics.push(json!({"Severity":2,"Code":"MENU_STILL_MISMATCH","Message":format!("播放静图列表 {} 项 != 曲目数 {tracks}。",still_pictures.len())}));
    }
    if expected_pages != index_pages as usize + pages.len() {
        diagnostics.push(json!({"Severity":2,"Code":"MENU_PAGE_MISMATCH","Message":"总菜单页数与索引页、专辑页之和不一致。"}));
    }
    let segments = plan["ScreenText"]
        .as_str()
        .unwrap_or_default()
        .split_once('=')
        .map_or(0, |(_, text)| text.split(':').count());
    if segments != expected_pages {
        diagnostics.push(json!({"Severity":2,"Code":"MENU_TEXT_PAGE_MISMATCH","Message":format!("screentext 段数 {segments} != 菜单页数 {expected_pages}。")}));
    }
    Ok(arguments)
}

fn run_image(
    images: &Arc<Images>,
    arguments: Vec<String>,
    callbacks: &mut dyn Callbacks,
) -> Result<(), String> {
    let status = images
        .run(&arguments, callbacks)
        .map_err(|error| error.to_string())?;
    if status == 0 {
        Ok(())
    } else {
        Err(format!("Menu image operation failed with status {status}"))
    }
}

fn require_frame(
    images: &Arc<Images>,
    path: &Path,
    callbacks: &mut dyn Callbacks,
) -> Result<(), String> {
    let mut output = String::new();
    struct Capture<'a> {
        target: &'a mut String,
        parent: &'a mut dyn Callbacks,
    }
    impl Callbacks for Capture<'_> {
        fn emit(&mut self, stream: i32, text: &str) {
            if stream == 1 {
                self.target.push_str(text);
            } else {
                self.parent.emit(stream, text);
            }
        }
        fn cancelled(&mut self) -> bool {
            self.parent.cancelled()
        }

        fn progress(&mut self, completed: u64, total: u64) {
            self.parent.progress(completed, total);
        }
    }
    let mut capture = Capture {
        target: &mut output,
        parent: callbacks,
    };
    let status = images
        .run(
            &[
                "identify".into(),
                "-format".into(),
                "%w %h".into(),
                path.to_string_lossy().into_owned(),
            ],
            &mut capture,
        )
        .map_err(|error| error.to_string())?;
    if status != 0 || output.split_whitespace().collect::<Vec<_>>() != [FRAME_WIDTH, FRAME_HEIGHT] {
        return Err(format!(
            "Menu image has invalid dimensions: {}",
            path.display()
        ));
    }
    Ok(())
}

fn page_cover(page: &Value) -> Option<PathBuf> {
    page.get("Tracks")
        .and_then(Value::as_array)
        .and_then(|tracks| tracks.first())
        .and_then(|track| track.get("SourcePath"))
        .and_then(Value::as_str)
        .and_then(|source| Path::new(source).parent())
        .and_then(find_cover)
}

fn find_cover(directory: &Path) -> Option<PathBuf> {
    ["jpg", "jpeg", "png", "webp"]
        .iter()
        .map(|extension| directory.join(format!("cover.{extension}")))
        .find(|path| path.is_file())
}

fn resolve_font(configured: &str, binary_directory: &Path) -> String {
    let configured = configured.trim();
    if configured.is_empty() {
        return String::new();
    }
    let candidate = PathBuf::from(configured);
    if candidate.is_file() {
        return author_font_path(&candidate);
    }
    // The bundled ImageMagick typemap exposes these names as separate CJK
    // faces. Keep the family name instead of expanding it to a TTC path;
    // dvda-author can then select the registered face without parsing a
    // collection filename in its menu command.
    if configured.starts_with("DVDA-") {
        return configured.to_owned();
    }
    let fonts = binary_directory.join("fonts");
    if let Ok(entries) = fs::read_dir(&fonts) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().is_some_and(|extension| {
                    matches!(
                        extension.to_string_lossy().to_ascii_lowercase().as_str(),
                        "ttc" | "otf" | "ttf"
                    )
                })
                && (path
                    .file_stem()
                    .is_some_and(|stem| stem.to_string_lossy().eq_ignore_ascii_case(configured))
                    || configured.starts_with("DVDA-"))
            {
                return author_font_path(&path);
            }
        }
    }
    configured.to_owned()
}

#[cfg(windows)]
fn author_font_path(path: &Path) -> String {
    use std::{
        ffi::{OsStr, OsString},
        os::windows::ffi::{OsStrExt, OsStringExt},
    };

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetShortPathNameW(long_path: *const u16, short_path: *mut u16, capacity: u32) -> u32;
    }

    let wide: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut capacity = 260u32;
    loop {
        let mut buffer = vec![0u16; capacity as usize];
        // The author passes this value into an unquoted image command. A short
        // path keeps the existing author ABI usable for repositories in paths
        // such as "Visual Studio Code" without adding shell quoting rules.
        let length = unsafe { GetShortPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), capacity) };
        if length == 0 {
            return path.to_string_lossy().into_owned();
        }
        if length < capacity {
            return OsString::from_wide(&buffer[..length as usize])
                .to_string_lossy()
                .into_owned();
        }
        capacity = length.saturating_add(1);
        if capacity > 32 * 1024 {
            return path.to_string_lossy().into_owned();
        }
    }
}

#[cfg(not(windows))]
fn author_font_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn menu_pages(tracks: &[Value], albums: &[String], row_cap: usize) -> Result<Vec<Value>, String> {
    if tracks.len() != albums.len() {
        return Err("Track and album lists must have equal lengths".into());
    }
    let mut result = Vec::new();
    let mut block_start = 0;
    while block_start < tracks.len() {
        let album = &albums[block_start];
        let mut block_end = block_start + 1;
        while block_end < albums.len() && albums[block_end] == *album {
            block_end += 1;
        }
        let mut page_start = block_start;
        let mut continuation = false;
        while page_start < block_end {
            let page_end = (page_start + row_cap).min(block_end);
            result.push(json!({
                "Album":album,
                "StartTrackIndex":page_start,
                "EndTrackIndex":page_end,
                "Continuation":continuation,
                "Tracks":tracks[page_start..page_end].to_vec()
            }));
            continuation = true;
            page_start = page_end;
        }
        block_start = block_end;
    }
    Ok(result)
}

fn album_of(track: &Value) -> String {
    let source = track["SourcePath"]
        .as_str()
        .unwrap_or_default()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_owned();
    let Some(slash) = source.rfind('/') else {
        return track["Album"].as_str().unwrap_or_default().to_owned();
    };
    if slash == 0 {
        return track["Album"].as_str().unwrap_or_default().to_owned();
    }
    let parent = source[..slash].trim_end_matches('/');
    let Some(parent_slash) = parent.rfind('/') else {
        return track["Album"].as_str().unwrap_or_default().to_owned();
    };
    let name = &parent[parent_slash + 1..];
    if name.is_empty() {
        track["Album"].as_str().unwrap_or_default().to_owned()
    } else {
        name.to_owned()
    }
}

fn basename(value: &str) -> String {
    value
        .replace('\\', "/")
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn album_labels(albums: &[String]) -> HashMap<String, String> {
    let distinct: Vec<String> = unique(albums.to_vec());
    let mut short_groups: HashMap<String, Vec<String>> = HashMap::new();
    for album in &distinct {
        short_groups
            .entry(short_album_text(album, 24))
            .or_default()
            .push(album.clone());
    }
    distinct
        .into_iter()
        .map(|album| {
            let short = short_album_text(&album, 24);
            if short_groups.get(&short).map_or(0, Vec::len) == 1 {
                return (album, short);
            }
            let qualifier = bracket_contents(&album)
                .into_iter()
                .find(|value| !is_generic_qualifier(value));
            (
                album,
                qualifier.map_or(short.clone(), |value| format!("{short} [{value}]")),
            )
        })
        .collect()
}

fn short_album_text(value: &str, maximum: usize) -> String {
    let index = value
        .char_indices()
        .find(|(_, character)| matches!(character, '(' | '[' | '（' | '【'))
        .map_or(value.len(), |(index, _)| index);
    let mut output = value[..index]
        .trim()
        .trim_end_matches(['-', '–', '—'])
        .trim()
        .to_owned();
    if output.is_empty() {
        output = value.to_owned();
    }
    from_units(&units(&output).into_iter().take(maximum).collect::<Vec<_>>())
}

fn bracket_contents(value: &str) -> Vec<String> {
    let opening = ['(', '[', '（', '【'];
    let closing = [')', ']', '）', '】'];
    let mut output = Vec::new();
    let mut chars = value.char_indices();
    while let Some((start, character)) = chars.next() {
        let Some(index) = opening.iter().position(|candidate| *candidate == character) else {
            continue;
        };
        let end_character = closing[index];
        if let Some((end, _)) = chars.find(|(_, candidate)| *candidate == end_character) {
            let value = value[start + character.len_utf8()..end].trim();
            output.push(value.to_owned());
        }
    }
    output
}

fn is_generic_qualifier(value: &str) -> bool {
    let trimmed = value.trim_start();
    trimmed.starts_with("游戏《") || trimmed.starts_with("游戏<")
}

fn sanitize_record(value: &str, changed: &mut Vec<String>) -> String {
    let result = sanitize(Value::String(value.to_owned())).unwrap_or(Value::Null);
    let changed_value = result["Changed"].as_bool().unwrap_or(false);
    if changed_value {
        changed.push(value.to_owned());
    }
    result["Value"].as_str().unwrap_or_default().to_owned()
}

fn truncate_value(value: &str, font_size: i64, budget: i64) -> String {
    truncate(json!({"Value":value,"FontSize":font_size,"Budget":budget}))
        .unwrap_or(Value::Null)
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn unique(values: Vec<String>) -> Vec<String> {
    let mut output = Vec::new();
    for value in values {
        if !output.contains(&value) {
            output.push(value);
        }
    }
    output
}

pub fn visual_near_solid(request: Value) -> Result<Value, String> {
    let standard_deviation = request["StandardDeviation"]
        .as_f64()
        .ok_or("Missing StandardDeviation")?;
    let colors = request["Colors"].as_f64().ok_or("Missing Colors")?;
    Ok(json!(colors <= 50.0 || standard_deviation <= 1.0))
}

pub fn visual_background_invalid(request: Value) -> Result<Value, String> {
    let mean = request.as_f64().ok_or("Expected background mean")?;
    Ok(json!(mean > 160.0))
}

pub fn visual_thumbnail_missing(request: Value) -> Result<Value, String> {
    let mean = request.as_f64().ok_or("Expected thumbnail mean")?;
    Ok(json!(mean <= 3.0))
}

pub fn visual_label_missing(request: Value) -> Result<Value, String> {
    let maximum = request["Maximum"].as_f64().ok_or("Missing label maximum")?;
    let mean = request["Mean"].as_f64().ok_or("Missing label mean")?;
    Ok(json!(maximum <= 200.0 || mean > 200.0))
}

pub fn visual_expected_index_cells(request: Value) -> Result<Value, String> {
    let album_count = request["AlbumCount"]
        .as_i64()
        .ok_or("Missing album count")?;
    let page_index = request["PageIndex"].as_i64().ok_or("Missing page index")?;
    let per_page = request["PerPage"]
        .as_i64()
        .ok_or("Missing per-page count")?;
    if per_page < 1 {
        return Err("Per-page count must be positive".into());
    }
    Ok(json!(
        (album_count - page_index * per_page).clamp(0, per_page)
    ))
}

fn finite_number(value: &str) -> Option<f64> {
    // Invariant NumberStyles.Float accepts ASCII numeric whitespace and trailing NULs.
    let parsed = value
        .trim_end_matches('\0')
        .trim_matches(|c: char| c == ' ' || ('\t'..='\r').contains(&c))
        .parse::<f64>()
        .ok()?;
    parsed.is_finite().then_some(parsed)
}

pub fn parse_batch_frame_stats(request: Value) -> Result<Value, String> {
    let output = request.as_str().ok_or("Expected ImageMagick output")?;
    let lines: Vec<&str> = output
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| line.starts_with("F|"))
        .collect();
    if lines.len() != 1 {
        return Ok(Value::Null);
    }
    let fields: Vec<&str> = lines[0].split('|').collect();
    if fields.len() != 4 {
        return Ok(Value::Null);
    }
    let Some(mean) = finite_number(fields[1]) else {
        return Ok(Value::Null);
    };
    let Some(deviation) = finite_number(fields[2]) else {
        return Ok(Value::Null);
    };
    let Some(colors) = finite_number(fields[3]) else {
        return Ok(Value::Null);
    };
    Ok(json!([mean, deviation, colors]))
}

pub fn parse_index_batch(request: Value) -> Result<Value, String> {
    let output = text(&request, "Output");
    let expected = request["Expected"]
        .as_i64()
        .ok_or("Missing expected cell count")?;
    let succeeded = request["Succeeded"]
        .as_bool()
        .ok_or("Missing success flag")?;
    let mut records = HashMap::<(String, i64), Vec<String>>::new();
    let mut duplicates = HashSet::<(String, i64)>::new();
    for line in output
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_end_matches('\r'))
    {
        let mut fields: Vec<String> = line.split('|').map(str::to_owned).collect();
        if fields.len() > 1 {
            fields[1] = fields[1].trim_end_matches('\0').to_owned();
        }
        if fields.len() < 3
            || !matches!(fields[0].as_str(), "B" | "T" | "L")
            || fields[1].is_empty()
            || !fields[1]
                .chars()
                .all(|character| character.is_ascii_digit())
        {
            continue;
        }
        let Ok(index) = fields[1].parse::<i32>().map(i64::from) else {
            continue;
        };
        if !(1..=expected).contains(&index) {
            continue;
        }
        let key = (fields[0].clone(), index);
        if records.insert(key.clone(), fields).is_some() {
            duplicates.insert(key);
        }
    }

    fn valid(
        records: &HashMap<(String, i64), Vec<String>>,
        duplicates: &HashSet<(String, i64)>,
        succeeded: bool,
        kind: &str,
        index: i64,
        count: usize,
    ) -> Option<(f64, f64)> {
        if !succeeded || duplicates.contains(&(kind.to_owned(), index)) {
            return None;
        }
        let fields = records.get(&(kind.to_owned(), index))?;
        if fields.len() != count {
            return None;
        }
        let first = finite_number(&fields[2])?;
        let second = if count == 3 {
            0.0
        } else {
            finite_number(&fields[3])?
        };
        Some((first, second))
    }

    let mut background = Vec::new();
    let mut thumbnail = Vec::new();
    let mut label = Vec::new();
    if expected > 0 {
        for index in 1..=expected {
            match valid(&records, &duplicates, succeeded, "B", index, 3) {
                Some((value, _)) if value <= 160.0 => {}
                _ => background.push(index),
            }
            match valid(&records, &duplicates, succeeded, "T", index, 3) {
                Some((value, _)) if value > 3.0 => {}
                _ => thumbnail.push(index),
            }
            match valid(&records, &duplicates, succeeded, "L", index, 4) {
                Some((maximum, mean)) if maximum > 200.0 && mean <= 200.0 => {}
                _ => label.push(index),
            }
        }
    }
    Ok(json!({
        "Background": background,
        "Thumbnail": thumbnail,
        "Label": label,
    }))
}

pub fn parse_overlay_batch(request: Value) -> Result<Value, String> {
    let output = request["Output"].as_str().ok_or("Missing overlay output")?;
    let succeeded = request["Succeeded"]
        .as_bool()
        .ok_or("Missing success flag")?;
    let needs_arrow = request["NeedsArrow"]
        .as_bool()
        .ok_or("Missing arrow flag")?;
    let mut values = HashMap::new();
    let mut invalid = HashSet::new();
    if succeeded {
        for line in output.split('\n').filter(|line| !line.is_empty()) {
            let parts: Vec<_> = line.trim_end_matches('\r').split('|').collect();
            let kind = parts[0];
            if !matches!(kind, "N" | "H" | "A") {
                continue;
            }
            let value = (parts.len() == 2)
                .then(|| finite_number(parts[1]))
                .flatten();
            match value {
                Some(value) if !values.contains_key(kind) => {
                    values.insert(kind, value);
                }
                _ => {
                    invalid.insert(kind);
                }
            }
        }
    }
    let get = |kind| {
        if invalid.contains(kind) {
            None
        } else {
            values.get(kind).copied()
        }
    };
    Ok(json!({
        "Normal": get("N"),
        "Highlighted": get("H"),
        "Arrow": if needs_arrow { get("A") } else { None },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_frame_statistics_and_rejects_non_finite_values() {
        assert_eq!(
            parse_batch_frame_stats(Value::String("F|1.5|2.5|3\r\n".into())).unwrap(),
            json!([1.5, 2.5, 3.0])
        );
        assert_eq!(
            parse_batch_frame_stats(Value::String("F|0.5|NaN|1234\n".into())).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn classifies_index_batch_records_and_duplicates() {
        let result = parse_index_batch(json!({
            "Output": "B|1|120\nT|1|4\nL|1|240|80\nB|2|120\nT|2|3\nL|2|240|80\nB|2|120\n",
            "Expected": 2,
            "Succeeded": true,
        }))
        .unwrap();
        assert_eq!(
            result,
            json!({
                "Background": [2],
                "Thumbnail": [2],
                "Label": [],
            })
        );
    }
}
