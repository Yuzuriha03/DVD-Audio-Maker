//! Pure verification-input parsing shared by the build audit workflow.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct TrackRow {
    pub group: i32,
    pub title: i32,
    pub track: i32,
    pub first: i32,
    pub last: i32,
    pub pts: i64,
    pub length: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DiscCommand {
    pub disc_tag: String,
    pub group_count: i32,
    pub rows: Vec<TrackRow>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AuditLogData {
    pub rows: Vec<TrackRow>,
    pub commands: Vec<DiscCommand>,
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "verify.parse_audit_log" => {
            let text = request.as_str().ok_or("Expected build log text")?;
            serde_json::to_value(parse_audit_log(text)).map_err(|error| error.to_string())
        }
        "verify.matches_iso_name" => matches_iso_name(request),
        _ => Err(format!("Unsupported verification operation: {operation}")),
    }
}

pub fn parse_audit_log(text: &str) -> AuditLogData {
    let stripped = strip_ansi(text);
    let text = select_latest_formal_section(&stripped);
    let mut commands = Vec::new();
    let mut current: Option<DiscCommand> = None;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some((group_count, output)) = parse_author_command(line) {
            if let Some(command) = current.take() {
                commands.push(command);
            }
            current = Some(DiscCommand {
                disc_tag: basename(&output),
                group_count,
                rows: Vec::new(),
            });
            continue;
        }
        if let (Some(command), Some(row)) = (current.as_mut(), parse_track_row(line)) {
            command.rows.push(row);
        }
    }
    if let Some(command) = current {
        commands.push(command);
    }
    let rows = commands
        .iter()
        .flat_map(|command| command.rows.iter().cloned())
        .collect();
    AuditLogData { rows, commands }
}

pub fn strip_ansi(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b && index + 1 < bytes.len() && bytes[index + 1] == b'[' {
            index += 2;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if byte.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        let Some(character) = text[index..].chars().next() else {
            break;
        };
        output.push(character);
        index += character.len_utf8();
    }
    output
}

fn select_latest_formal_section(text: &str) -> &str {
    let mut start = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line
            .strip_suffix('\n')
            .unwrap_or(line)
            .strip_suffix('\r')
            .unwrap_or(line.strip_suffix('\n').unwrap_or(line));
        if ["[Rust build]", "[C# build]"].iter().any(|header| {
            trimmed.starts_with(header) && !trimmed.starts_with(&format!("{header} [DRY-RUN]"))
        }) {
            start = Some(offset);
        }
        offset += line.len();
    }
    start.map_or(text, |index| &text[index..])
}

fn parse_author_command(line: &str) -> Option<(i32, String)> {
    if !line.starts_with("+ ") {
        return None;
    }
    let group_count = count_option(line, "-g");
    let output = option_value(line, "-o")?;
    if group_count == 0 {
        return None;
    }
    Some((group_count, output))
}

fn count_option(line: &str, option: &str) -> i32 {
    let bytes = line.as_bytes();
    let mut count = 0;
    let mut offset = 0;
    while let Some(relative) = line[offset..].find(option) {
        let index = offset + relative;
        let before = index == 0 || bytes[index - 1].is_ascii_whitespace();
        let after = index + option.len() == bytes.len()
            || bytes[index + option.len()].is_ascii_whitespace();
        if before && after {
            count += 1;
        }
        offset = index + option.len();
        if offset >= line.len() {
            break;
        }
    }
    count
}

fn option_value(line: &str, option: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut offset = 0;
    while let Some(relative) = line[offset..].find(option) {
        let index = offset + relative;
        let before = index == 0 || bytes[index - 1].is_ascii_whitespace();
        let after = index + option.len() == bytes.len()
            || bytes[index + option.len()].is_ascii_whitespace();
        if before && after {
            let mut value_start = index + option.len();
            while value_start < bytes.len() && bytes[value_start].is_ascii_whitespace() {
                value_start += 1;
            }
            if value_start >= bytes.len() {
                return None;
            }
            let quote = bytes[value_start];
            if quote == b'"' || quote == b'\'' {
                let value_end = line[value_start + 1..].find(quote as char)? + value_start + 1;
                return Some(line[value_start + 1..value_end].to_owned());
            }
            let value_end = line[value_start..]
                .find(char::is_whitespace)
                .map_or(line.len(), |relative| value_start + relative);
            return Some(line[value_start..value_end].to_owned());
        }
        offset = index + option.len();
        if offset >= line.len() {
            break;
        }
    }
    None
}

fn parse_track_row(line: &str) -> Option<TrackRow> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() != 8 {
        return None;
    }
    let title: Vec<_> = fields[1].split('/').collect();
    if title.len() != 2
        || title
            .iter()
            .chain(
                fields
                    .iter()
                    .enumerate()
                    .filter_map(|(i, f)| (i != 1).then_some(f)),
            )
            .any(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some(TrackRow {
        group: fields[0].parse().ok()?,
        title: title[0].parse().ok()?,
        track: fields[2].parse().ok()?,
        first: fields[3].parse().ok()?,
        last: fields[4].parse().ok()?,
        pts: fields[5].parse().ok()?,
        length: fields[6].parse().ok()?,
    })
}

fn basename(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn matches_iso_name(request: Value) -> Result<Value, String> {
    let object = request.as_object().ok_or("Expected ISO name request")?;
    let path = object
        .get("Path")
        .and_then(Value::as_str)
        .ok_or("Missing ISO path")?;
    let prefix = object
        .get("IsoPrefix")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if prefix.trim().is_empty() {
        return Ok(json!(true));
    }
    let name = basename(path);
    let lower_name = name.to_ascii_lowercase();
    let lower_prefix = prefix.to_ascii_lowercase();
    if !lower_name.starts_with(&lower_prefix) || !lower_name.ends_with(".iso") {
        return Ok(json!(false));
    }
    let suffix = &name[prefix.len()..name.len() - 4];
    Ok(json!(
        !suffix.is_empty()
            && suffix[1..]
                .chars()
                .all(|character| character.is_ascii_digit())
            && suffix.starts_with('_')
    ))
}

/// The actual IFO rows and the single streaming AOB observation are shared by
/// index, manifest and build-log audits. Log auditing never rereads AOB payloads.
#[derive(Debug, Default)]
pub struct GroupEvidence {
    pub rows: Vec<TrackRow>,
    pub sectors: i64,
    pub drops: Vec<i32>,
    pub missing_sector: i32,
    pub scanned_sectors: i32,
    pub scan_complete: bool,
}
#[derive(Debug)]
pub struct DiscEvidence {
    pub path: PathBuf,
    pub groups: BTreeMap<i32, GroupEvidence>,
}
impl DiscEvidence {
    pub fn track_count(&self) -> usize {
        self.groups.values().map(|group| group.rows.len()).sum()
    }
}
fn issue(code: &str, message: impl Into<String>) -> Value {
    json!({"Severity":2,"Code":code,"Message":message.into()})
}
fn unavailable(code: &str, message: &str) -> Value {
    json!({"Severity":2,"Code":code,"Message":message,"Unavailable":true})
}
pub fn manifest_issues(path: Option<&Path>, discs: &[DiscEvidence]) -> Vec<Value> {
    let Some(path) = path.filter(|path| path.exists()) else {
        return Vec::new();
    };
    let read = || -> Result<usize, String> {
        let data: Value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let object = data.as_object().ok_or("Invalid manifest object")?;
        let mut count = 0usize;
        for album in object.values() {
            let album = album.as_object().ok_or("Invalid manifest album")?;
            if let Some(files) = album.get("files") {
                count = count
                    .checked_add(files.as_array().ok_or("Invalid manifest files")?.len())
                    .ok_or("Manifest track count overflow")?;
            }
        }
        Ok(count)
    };
    match read() {
        Ok(expected) => {
            let actual: usize = discs.iter().map(DiscEvidence::track_count).sum();
            if actual == expected {
                Vec::new()
            } else {
                vec![issue(
                    "TRACK_COUNT_MISMATCH",
                    format!("全部 ISO 的 IFO 声明轨数 {actual} != manifest 曲目数 {expected}"),
                )]
            }
        }
        Err(reason) => vec![issue(
            "MANIFEST_INVALID",
            format!("准备清单无法读取：{}：{reason}", path.display()),
        )],
    }
}
pub fn select_latest_build_log(requested: &Path, allow_fallback: bool) -> Option<PathBuf> {
    let mut candidates = vec![requested.to_path_buf()];
    if allow_fallback && let Some(parent) = requested.parent() {
        candidates.extend(
            ["build.log", "rebuild-final.log", "finalrebuild.log"]
                .iter()
                .map(|name| parent.join(name)),
        );
    }
    let mut seen = std::collections::HashSet::new();
    let mut existing = candidates
        .into_iter()
        .filter(|path| path.is_file() && seen.insert(path.to_string_lossy().to_lowercase()))
        .collect::<Vec<_>>();
    // A stable sort retains the explicit path first when timestamps tie.
    existing
        .sort_by_key(|path| std::cmp::Reverse(fs::metadata(path).and_then(|m| m.modified()).ok()));
    existing.into_iter().next()
}
pub fn has_padding_failure(text: &str) -> bool {
    strip_ansi(text)
        .to_ascii_lowercase()
        .contains("pes_padding length must be higher")
}
fn missing_material(log: &AuditLogData) -> Option<Value> {
    // Preserve the old ordering. The parser associates every row with an author
    // command, so DVDA_COMMAND_MISSING is defensive and unreachable for its output.
    if log.rows.is_empty() {
        Some(unavailable(
            "TRACK_TABLE_MISSING",
            "构建日志中未解析到 dvda-author 轨道表",
        ))
    } else if log.commands.is_empty() {
        Some(unavailable(
            "DVDA_COMMAND_MISSING",
            "构建日志中未解析到 dvda-author 命令行",
        ))
    } else {
        None
    }
}
fn trailing_number(value: &str) -> Option<u64> {
    let digits: String = value
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.parse().ok()
}
fn mapped_command<'a>(
    log: &'a AuditLogData,
    iso: &Path,
    position: usize,
) -> Option<&'a DiscCommand> {
    let number = iso
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.rsplit_once('_'))
        .and_then(|(_, s)| s.parse::<u64>().ok());
    number
        .and_then(|number| {
            log.commands
                .iter()
                .find(|command| trailing_number(&command.disc_tag) == Some(number))
        })
        .or_else(|| log.commands.get(position))
}
pub fn sector_relations(rows: &[TrackRow], sectors: i64, context: &str) -> Vec<Value> {
    let mut ordered = rows.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|row| row.first);
    let mut issues = Vec::new();
    for pair in ordered.windows(2) {
        if i64::from(pair[1].first) != i64::from(pair[0].last) + 1 {
            issues.push(issue(
                "TRACK_GAP",
                format!("{context}：轨道表存在扇区断点或重叠。"),
            ));
        }
    }
    if let Some(last) = rows.iter().map(|row| i64::from(row.last)).max()
        && sectors != last + 1
    {
        issues.push(issue(
            "AOB_SECTOR_MISMATCH",
            format!("{context}：AOB 扇区 {sectors} != 轨道表 {}", last + 1),
        ));
    }
    issues
}
pub fn log_issues(
    requested: Option<&Path>,
    allow_fallback: bool,
    discs: &[DiscEvidence],
) -> Vec<Value> {
    let Some(path) = requested.and_then(|path| select_latest_build_log(path, allow_fallback))
    else {
        return vec![unavailable(
            "BUILD_LOG_MISSING",
            "未找到可用于审计的构建日志",
        )];
    };
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(reason) => {
            return vec![unavailable(
                "BUILD_LOG_UNREADABLE",
                &format!("构建日志无法读取：{}：{reason}", path.display()),
            )];
        }
    };
    let mut issues = Vec::new();
    if has_padding_failure(&text) {
        issues.push(issue("PES_PADDING_FAILED", "构建日志包含 pack 补齐失败"));
    }
    let log = parse_audit_log(&text);
    if let Some(issue) = missing_material(&log) {
        issues.push(issue);
        return issues;
    }
    for (position, disc) in discs.iter().enumerate() {
        let Some(command) = mapped_command(&log, &disc.path, position) else {
            issues.push(unavailable(
                "DISC_LOG_MAPPING_MISSING",
                "无法把 ISO 映射到构建日志中的 dvda-author 命令",
            ));
            continue;
        };
        if command.rows.len() != disc.track_count() {
            issues.push(unavailable(
                "DISC_TRACK_MAPPING_MISMATCH",
                &format!(
                    "日志映射 {} 轨 != ISO IFO 声明 {} 轨",
                    command.rows.len(),
                    disc.track_count()
                ),
            ));
            continue;
        }
        let mut groups = BTreeMap::<i32, Vec<TrackRow>>::new();
        for row in &command.rows {
            groups.entry(row.group).or_default().push(row.clone());
        }
        for (number, mut rows) in groups {
            rows.sort_by_key(|row| row.first);
            let context = format!("{} group {number} (build log)", disc.path.display());
            let evidence = disc.groups.get(&number);
            issues.extend(sector_relations(
                &rows,
                evidence.map_or(0, |group| group.sectors),
                &context,
            ));
            if let Some(evidence) = evidence {
                let mut expected = evidence.rows.clone();
                expected.sort_by_key(|row| row.first);
                if rows
                    .iter()
                    .map(|r| (r.group, r.title, r.first, r.last))
                    .collect::<Vec<_>>()
                    != expected
                        .iter()
                        .map(|r| (r.group, r.title, r.first, r.last))
                        .collect::<Vec<_>>()
                {
                    issues.push(issue(
                        "DISC_TRACK_LAYOUT_MISMATCH",
                        format!("{context}：日志与 IFO 的轨道位置不一致。"),
                    ));
                }
                if evidence.scan_complete || evidence.missing_sector >= 0 {
                    match crate::aob::dispatch(
                        "aob.audit_diagnostics",
                        json!({"Drops":evidence.drops,"MissingSector":evidence.missing_sector,"SectorCount":evidence.scanned_sectors,"Rows":rows}),
                    ) {
                        Ok(Value::Array(diagnostics)) => {
                            for diagnostic in diagnostics {
                                issues.push(issue(
                                    diagnostic["Code"].as_str().unwrap_or("PTS_INVALID"),
                                    format!("{context}：PTS 时间轴无效。"),
                                ));
                            }
                        }
                        _ => issues
                            .push(issue("PTS_INVALID", format!("{context}：PTS 时间轴无效。"))),
                    }
                }
            }
        }
    }
    issues
}

pub fn still_picture_count(log: &str) -> usize {
    log.lines()
        .find(|line| parse_author_command(line).is_some())
        .and_then(|line| option_value(line, "--stillpics"))
        .map_or(0, |paths| {
            paths
                .split(';')
                .filter(|path| !path.trim().is_empty())
                .count()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_last_formal_section_and_ansi_rows() {
        let text = concat!(
            "[Rust build] 2026-09-28 10:00:00\n",
            "+ dvda-author -g old.mlp -o /work/output/disc1\n",
            "1  1/1  1  0  9  0  9000  0\n",
            "[Rust build] [DRY-RUN] 2026-09-28 11:00:00\n",
            "[Rust build] 2026-09-29 12:00:00\n",
            "+ dvda-author -g one.mlp two.mlp -g three.mlp -o \"/work/output/disc1\"\n",
            "\u{1b}[32m1  1/2  1  0  19  0  9000  0\u{1b}[0m\n",
            "1  1/2  2  20  39  9000  9000  0\n",
        );
        let parsed = parse_audit_log(text);
        assert_eq!(parsed.commands.len(), 1);
        assert_eq!(parsed.commands[0].disc_tag, "disc1");
        assert_eq!(parsed.commands[0].group_count, 2);
        assert_eq!(parsed.rows.len(), 2);
        assert_eq!(parsed.rows[1].first, 20);
    }

    fn row(title: i32, first: i32, last: i32) -> TrackRow {
        TrackRow {
            group: 1,
            title,
            track: title,
            first,
            last,
            pts: 0,
            length: 9000,
        }
    }
    #[test]
    fn audit_material_order_mapping_and_sector_relations() {
        let parsed = parse_audit_log("1 1/1 1 0 9 0 9000 0\n");
        assert_eq!(
            missing_material(&parsed).unwrap()["Code"],
            "TRACK_TABLE_MISSING"
        );
        assert_eq!(
            missing_material(&AuditLogData {
                rows: vec![row(1, 0, 9)],
                commands: vec![]
            })
            .unwrap()["Code"],
            "DVDA_COMMAND_MISSING"
        );
        let log = parse_audit_log(
            "+ author -g x -o '/中文/光盘 10'\n1 1/1 1 0 9 0 9000 0\n+ author -g y -o C:\\work\\disc2\n1 1/1 1 0 19 0 9000 0\n",
        );
        assert_eq!(
            mapped_command(&log, Path::new("release_2.iso"), 0)
                .unwrap()
                .disc_tag,
            "disc2"
        );
        assert_eq!(
            mapped_command(&log, Path::new("release_10.iso"), 1)
                .unwrap()
                .disc_tag,
            "光盘 10"
        );
        assert_eq!(
            mapped_command(&log, Path::new("named.iso"), 1)
                .unwrap()
                .disc_tag,
            "disc2"
        );
        assert!(mapped_command(&log, Path::new("release_3.iso"), 2).is_none());
        assert!(sector_relations(&[row(1, 0, 9), row(2, 10, 19)], 20, "x").is_empty());
        for first in [9, 11] {
            assert_eq!(
                sector_relations(&[row(1, 0, 9), row(2, first, 19)], 20, "x")[0]["Code"],
                "TRACK_GAP"
            );
        }
        for sectors in [19, 21] {
            assert!(
                sector_relations(&[row(1, 0, 9), row(2, 10, 19)], sectors, "x")
                    .iter()
                    .any(|v| v["Code"] == "AOB_SECTOR_MISMATCH")
            );
        }
    }
    #[test]
    fn historical_headers_strict_rows_and_padding_are_preserved() {
        let text = "[C# build] old\n+ author -g old -o disc1\n1 1/1 1 0 9 0 9000 0\n[Rust build] new\n+ author -g new -o disc2\n1 1/1 1 0 19 0 9000 0\n[Rust build] [DRY-RUN] later\n";
        let log = parse_audit_log(text);
        assert_eq!(log.commands.len(), 1);
        assert_eq!(log.commands[0].disc_tag, "disc2");
        for line in [
            "1 1 1 0 9 0 9000 0",
            "1 1/1/1 1 0 9 0 9000 0",
            "1 1/x 1 0 9 0 9000 0",
            "1 1/1 1 -1 9 0 9000 0",
            "1 1/1 1 0 9 0 9000 junk",
        ] {
            assert!(parse_track_row(line).is_none(), "{line}");
        }
        for text in [
            "pes_padding length must be higher",
            "PeS_PaDdInG LeNgTh MuSt Be HiGhEr",
            "\u{1b}[31mpes_padding length must be higher\u{1b}[0m",
        ] {
            assert!(has_padding_failure(text));
        }
        assert!(!has_padding_failure("Encoding succeeded"));
    }

    #[test]
    fn restored_user_audit_messages_have_all_three_languages() {
        for message in [
            "全部 ISO 的 IFO 声明轨数 2 != manifest 曲目数 3",
            "准备清单无法读取：C:\\中文\\manifest.json：Invalid manifest files",
            "构建日志无法读取：C:\\中文\\build.log：read failure",
            "未找到可用于审计的构建日志",
            "构建日志中未解析到 dvda-author 轨道表",
            "构建日志中未解析到 dvda-author 命令行",
            "无法把 ISO 映射到构建日志中的 dvda-author 命令",
            "日志映射 1 轨 != ISO IFO 声明 2 轨",
            "构建日志包含 pack 补齐失败",
            "C:\\中文\\disc.iso：轨道表存在扇区断点或重叠。",
            "C:\\中文\\disc.iso：AOB 扇区 10 != 轨道表 11",
            "C:\\中文\\disc.iso：日志与 IFO 的轨道位置不一致。",
            "C:\\中文\\disc.iso：PTS 时间轴无效。",
            "配置了播放封面，但未生成有效的静图文件：C:\\中文\\AUDIO_SV.VOB",
            "ASVS_TOO_SHORT",
            "MENU_FILE_MISSING: AUDIO_SV.VOB",
        ] {
            for language in ["zh", "en", "ja"] {
                let translated = crate::localization::translate(language, message);
                assert!(!translated.is_empty());
                if language != "zh" {
                    assert_ne!(translated, message, "{language}: {message}");
                }
                if message.contains("C:\\中文\\") {
                    assert!(translated.contains("C:\\中文\\"), "{translated}");
                }
            }
        }
    }

    #[test]
    fn matches_current_iso_name_case_insensitively() {
        assert_eq!(
            matches_iso_name(json!({
                "Path":"Wuthering_Waves_Singles_EPs_2.ISO",
                "IsoPrefix":"Wuthering_Waves_Singles_EPs"
            }))
            .unwrap(),
            json!(true)
        );
        assert_eq!(
            matches_iso_name(json!({
                "Path":"Wuthering_Waves_Singles_EPs_SurCode_2.iso",
                "IsoPrefix":"Wuthering_Waves_Singles_EPs"
            }))
            .unwrap(),
            json!(false)
        );
    }
}
