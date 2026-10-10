//! Check authored button/navigation and overlay output before publishing.
use crate::{build::Job, disc::Disc, menu};
use dvda_native::media::Callbacks;
use serde_json::{Value, json};
use std::{fs, path::Path};

fn issue(code: &str, message: String) -> Value {
    json!({"Severity":2,"Code":code,"Message":message})
}
fn numbers(text: &str, prefix: &str, suffix: &str) -> Vec<u32> {
    text.split(prefix)
        .skip(1)
        .filter_map(|tail| {
            let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
            (!digits.is_empty() && tail[digits.len()..].starts_with(suffix))
                .then(|| digits.parse().ok())
                .flatten()
        })
        .collect()
}
pub fn buttons(directory: &Path, total: usize, index: usize) -> Vec<Value> {
    let mut issues = Vec::new();
    let project = match fs::read_to_string(directory.join("xmltemp")) {
        Ok(text) => text,
        Err(error) => return vec![issue("MENU_BUTTON_XML_MISSING", error.to_string())],
    };
    let pages: Vec<_> = project.split("<pgc>").skip(1).collect();
    if pages.len() != total {
        issues.push(issue(
            "MENU_BUTTON_PAGE_MISMATCH",
            format!("菜单跳转 XML 有 {} 页，规划为 {total} 页。", pages.len()),
        ));
    }
    for (page, data) in pages.iter().enumerate() {
        let overlay = match fs::read_to_string(directory.join(format!("spu_xmltemp_{page}.xml"))) {
            Ok(text) => text,
            Err(_) => {
                issues.push(issue(
                    "MENU_BUTTON_OVERLAY_XML_MISSING",
                    format!("第 {} 页缺少按钮位置 XML。", page + 1),
                ));
                continue;
            }
        };
        let jumps = numbers(data, "name=\"button", "\"");
        let positions = numbers(&overlay, "name=\"button", "\"");
        if jumps != positions || jumps.is_empty() {
            issues.push(issue(
                "MENU_BUTTON_MISMATCH",
                format!(
                    "第 {} 页按钮编号不一致：跳转 {jumps:?}，位置 {positions:?}。",
                    page + 1
                ),
            ));
        }
        if index > 0 && page >= index && numbers(data, ">jump menu ", ";").last() != Some(&1) {
            issues.push(issue(
                "MENU_RETURN_BUTTON_MISSING",
                format!(
                    "第 {} 个菜单页缺少末尾的 jump menu 1 返回索引按钮。",
                    page + 1
                ),
            ));
        }
    }
    issues
}
pub fn verify(
    job: &Job,
    disc: &Disc,
    output: &Path,
    temporary: &Path,
    author_arguments: &[String],
    caller: &mut dyn Callbacks,
) -> Result<Vec<Value>, String> {
    let vob = output.join("AUDIO_TS/AUDIO_TS.VOB");
    if fs::metadata(&vob).map_or(true, |m| m.len() == 0) {
        return Ok(vec![issue(
            "MENU_VOB_MISSING",
            format!("未生成菜单文件 {}。", vob.display()),
        )]);
    }
    let plan = menu::plan(json!({"Disc":disc,"Title":job.title,
        "MenuTracksPerPage":job.menu_tracks_per_page,"MenuIndexMinimumAlbums":job.menu_index_minimum_albums}))?;
    let total = plan["TotalPages"]
        .as_u64()
        .ok_or("Invalid menu page count")? as usize;
    let index = plan["IndexPages"]
        .as_u64()
        .ok_or("Invalid menu index page count")? as usize;
    let mut issues = buttons(temporary, total, index);
    issues.extend(still_warnings(output, author_arguments));
    if issues.iter().any(|d| d["Severity"] == 2) {
        return Ok(issues);
    }
    let images = match crate::native_components::images(job.image_library.as_deref()) {
        Ok(images) => images,
        _ => {
            issues.push(json!({"Severity":1,"Code":"MENU_OVERLAY_CHECK_SKIPPED","Message":"内置图像组件缺失，无法完成菜单文字与高亮检查。"}));
            return Ok(issues);
        }
    };
    for page in 0..total {
        if caller.cancelled() {
            return Err("Build cancelled".into());
        }
        let normal = images.read_rgba(&temporary.join(format!("impic{page}.png")));
        let highlighted = images.read_rgba(&temporary.join(format!("hlpic{page}.png")));
        let (Ok((width, height, normal)), Ok((hw, hh, highlighted))) = (normal, highlighted) else {
            issues.push(issue(
                "MENU_OVERLAY_MISSING",
                format!("第 {} 页缺少可读取的文字层或高亮层。", page + 1),
            ));
            continue;
        };
        let alpha = |data: &[u8]| {
            data.as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| pixel[3] as u64)
                .sum::<u64>()
        };
        if (width, height) != (hw, hh) || alpha(&highlighted) <= alpha(&normal) {
            issues.push(issue(
                "MENU_OVERLAY_EMPTY",
                format!("第 {} 页高亮层没有比文字层增加墨迹。", page + 1),
            ));
        }
        if page < index && index > 1 {
            let arrow = normal
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .any(|(i, pixel)| {
                    let y = i / width as usize;
                    (488..560).contains(&y) && pixel[3] > 0
                });
            if !arrow {
                issues.push(issue(
                    "MENU_INDEX_ARROW_MISSING",
                    format!("第 {} 个索引页的翻页箭头区域没有墨迹。", page + 1),
                ));
            }
        }
    }
    Ok(issues)
}

/// Inspect the effective --stillpics list, including developer menu arguments,
/// so disabled/missing source covers do not produce a false warning.
pub fn still_warnings(output: &Path, author_arguments: &[String]) -> Vec<Value> {
    let has_pictures = author_arguments.windows(2).any(|args| {
        args[0] == "--stillpics" && args[1].split(';').any(|path| !path.trim().is_empty())
    });
    let path = output.join("AUDIO_TS/AUDIO_SV.VOB");
    if has_pictures && fs::metadata(&path).map_or(true, |m| m.len() == 0) {
        vec![
            json!({"Severity":1,"Code":"MENU_STILL_VOB_MISSING","Message":format!("配置了播放封面，但未生成有效的静图文件：{}",path.display())}),
        ]
    } else {
        Vec::new()
    }
}
