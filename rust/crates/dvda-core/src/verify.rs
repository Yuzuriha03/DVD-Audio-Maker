//! Rust-owned verification of published DVD-Audio images.
//!
//! Verification uses the repository's ISO reader, AOB timestamp parser and
//! C17 `dvda-disc-verify.dll`. It deliberately has no media-process fallback.
use crate::{
    aob, formats,
    verification::{self, DiscEvidence, GroupEvidence},
};
use dvda_native::{disc_verify::NativeDiscVerifier, media::Callbacks};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

const SECTOR_SIZE: usize = 2048;
const CHUNK_SIZE: usize = 128 * 1024;
fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    pub image_library: Option<PathBuf>,
    pub media_library: PathBuf,
    pub work_directory: PathBuf,
    pub final_directory: PathBuf,
    pub iso_prefix: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default = "default_true")]
    pub menu_enabled: bool,
    pub index_path: PathBuf,
    pub manifest_path: Option<PathBuf>,
    pub build_log_path: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub allow_log_fallback: bool,
    pub disc_bytes: i64,
    pub max_discs: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Outcome {
    pub succeeded: bool,
    pub diagnostics: Vec<Value>,
    pub iso_count: i32,
    pub track_count: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    All,
    Capacity,
    Timeline,
    Menu,
    Lossless,
}

pub fn execute(job: Job, caller: &mut dyn Callbacks) -> Outcome {
    execute_mode(job, Mode::All, None, caller)
}
pub fn execute_mode(
    job: Job,
    mode: Mode,
    explicit_iso: Option<PathBuf>,
    caller: &mut dyn Callbacks,
) -> Outcome {
    match run(job, mode, explicit_iso, caller) {
        Ok(value) => value,
        Err(message) => Outcome {
            succeeded: false,
            diagnostics: vec![error("VERIFY_FAILED", message)],
            iso_count: 0,
            track_count: 0,
        },
    }
}

fn run(
    job: Job,
    mode: Mode,
    explicit_iso: Option<PathBuf>,
    caller: &mut dyn Callbacks,
) -> Result<Outcome, String> {
    check_cancel(caller)?;
    if mode == Mode::Menu && !job.menu_enabled && explicit_iso.is_none() {
        caller.emit(1, "[跳过] 菜单已关闭，成品按预期没有菜单。");
        return Ok(Outcome {
            succeeded: true,
            diagnostics: Vec::new(),
            iso_count: 0,
            track_count: 0,
        });
    }
    let mut diagnostics = Vec::new();
    let needs_index = matches!(mode, Mode::All | Mode::Lossless)
        || (mode == Mode::Menu && explicit_iso.is_none());
    let mut index = if needs_index {
        match read_index(&job.index_path) {
            Ok(index) => Some(index),
            Err(issue) => {
                diagnostics.push(issue);
                None
            }
        }
    } else if matches!(mode, Mode::Timeline | Mode::Menu) {
        // An optional index can add consistency checks, but cannot prevent
        // inspecting the actual ISO timeline or an explicitly selected menu.
        read_index(&job.index_path).ok()
    } else {
        None
    };
    let selected = explicit_iso.is_some();
    let isos = if let Some(iso) = explicit_iso {
        if !iso.is_file() {
            return Err(format!("找不到成品 ISO：{}", iso.display()));
        }
        if let Some(index) = index.as_mut() {
            index["__discs__"].as_array_mut().unwrap().retain(|disc| {
                iso.file_name().is_some_and(|name| {
                    name.to_string_lossy()
                        .eq_ignore_ascii_case(&text(disc, "iso"))
                })
            });
        }
        vec![iso]
    } else {
        find_isos(&job.final_directory, &job.iso_prefix)?
    };
    if needs_index
        && index
            .as_ref()
            .is_some_and(|value| index_track_count(value) == 0)
    {
        diagnostics.push(unavailable(
            "LOSSLESS_SOURCE_MISSING",
            "正式索引不包含可校验的音轨。",
        ));
        index = None;
    }
    let tracks = index.as_ref().map_or(0, index_track_count);
    if matches!(mode, Mode::All | Mode::Capacity) {
        capacity(
            &job,
            &isos,
            index.as_ref().map_or(1, |_| tracks),
            &mut diagnostics,
        );
    } else if isos.is_empty() {
        diagnostics.push(error("NO_ISO", "No published ISO images were found"));
    }
    if let Some(index) = &index {
        for disc in index["__discs__"].as_array().unwrap() {
            let expected = text(disc, "iso");
            if !isos.iter().any(|iso| {
                iso.file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(&expected))
            }) {
                diagnostics.push(error("ISO_MISSING", format!("找不到成品 ISO：{expected}")));
            }
        }
    }
    let mut actual_tracks = tracks;
    if matches!(mode, Mode::All | Mode::Timeline) {
        let evidence = timeline(&isos, index.as_ref(), &mut diagnostics, caller, true, true)?;
        // The preparation manifest describes the entire selected release. An
        // explicit single-ISO developer request has no complete-release total.
        if mode == Mode::All && !selected {
            diagnostics.extend(verification::manifest_issues(
                job.manifest_path.as_deref(),
                &evidence,
            ));
        }
        if mode == Mode::All {
            diagnostics.extend(verification::log_issues(
                job.build_log_path.as_deref(),
                job.allow_log_fallback,
                &evidence,
            ));
        }
        actual_tracks = evidence.iter().map(|disc| disc.track_count() as i32).sum();
    }
    if mode == Mode::All || (mode == Mode::Menu && (index.is_some() || selected)) {
        for iso in &isos {
            check_cancel(caller)?;
            let expected = index
                .as_ref()
                .and_then(|index| index["__discs__"].as_array())
                .and_then(|discs| {
                    discs.iter().find(|disc| {
                        iso.file_name().is_some_and(|name| {
                            name.to_string_lossy()
                                .eq_ignore_ascii_case(&text(disc, "iso"))
                        })
                    })
                })
                .map(|disc| &disc["menu"])
                .filter(|value| !value.is_null());
            let has_menu = match formats::iso_list_directory(iso, "AUDIO_TS") {
                Ok(entries) => entries
                    .iter()
                    .any(|entry| entry.name.eq_ignore_ascii_case("AUDIO_TS.VOB")),
                Err(reason) => {
                    diagnostics.push(error("ISO_READ_FAILED", reason));
                    continue;
                }
            };
            if expected.is_some() || has_menu || mode == Mode::Menu {
                match crate::menu_verify::verify(iso, expected, &job, caller) {
                    Ok(issues) => diagnostics.extend(issues),
                    Err(reason) => {
                        let code = reason.split(':').next().unwrap_or_default();
                        let code = if code.starts_with("ASVS_")
                            || code.starts_with("AMG_")
                            || code == "MENU_FILE_MISSING"
                        {
                            code
                        } else {
                            "MENU_VERIFICATION_FAILED"
                        };
                        diagnostics.push(error(code, reason.clone()));
                    }
                }
            }
        }
    }
    if matches!(mode, Mode::All | Mode::Lossless)
        && let Some(index) = index.as_ref()
        && let Err(reason) = lossless(&job, &isos, index, &mut diagnostics, caller)
    {
        diagnostics.push(error("LOSSLESS_FAILED", reason));
    }
    Ok(Outcome {
        succeeded: !diagnostics.iter().any(is_error),
        diagnostics,
        iso_count: isos.len() as i32,
        track_count: actual_tracks,
    })
}

fn read_index(path: &Path) -> Result<Value, Value> {
    if !path.is_file() {
        return Err(unavailable(
            "MLP_INDEX_MISSING",
            format!("找不到正式索引: {}", path.display()),
        ));
    }
    let invalid = |reason| {
        unavailable(
            "MLP_INDEX_INVALID",
            format!("无法读取 mlp_index.json: {reason}"),
        )
    };
    let value = crate::preparation::state::read_json(path).map_err(|e| invalid(e.message))?;
    if value["__meta__"]["dry_run"].as_bool() == Some(true) {
        return Err(unavailable(
            "MLP_INDEX_DRY_RUN",
            "当前索引来自 dry-run，不能作为现有成品 ISO 的校验依据",
        ));
    }
    if !value.get("__discs__").is_some_and(Value::is_array) {
        return Err(unavailable(
            "MLP_INDEX_INVALID",
            "mlp_index.json 缺少 __discs__",
        ));
    }
    if value["__discs__"].as_array().is_some_and(|discs| {
        discs.is_empty()
            || discs.iter().any(|disc| {
                disc["groups"].as_array().is_some_and(|groups| {
                    groups
                        .iter()
                        .all(|group| group["tracks"].as_array().is_some_and(Vec::is_empty))
                })
            })
    }) {
        return Err(unavailable(
            "LOSSLESS_SOURCE_MISSING",
            "正式索引不包含可校验的音轨。",
        ));
    }
    resolve_index(value).map_err(invalid)
}

fn resolve_index(mut value: Value) -> Result<Value, String> {
    if value["__meta__"]["dry_run"].as_bool() == Some(true) {
        return Err("预演索引不能用于验证正式成品。".into());
    }
    let entries = value.as_object().ok_or("Invalid track index")?.clone();
    let mut names = std::collections::HashSet::new();
    for disc in value["__discs__"]
        .as_array_mut()
        .ok_or("Missing disc index")?
    {
        let name = text(disc, "iso");
        if name.is_empty() || name.contains(['/', '\\']) || !names.insert(name.to_ascii_lowercase())
        {
            return Err("Invalid or duplicate ISO name in index".into());
        }
        let groups = disc["groups"]
            .as_array_mut()
            .filter(|v| !v.is_empty())
            .ok_or("Missing group index")?;
        let mut numbers = std::collections::HashSet::new();
        for group in groups {
            let number = number(group, "group")
                .filter(|v| (1..=9).contains(v))
                .ok_or("Invalid group number in index")?;
            if !numbers.insert(number) {
                return Err("Duplicate group number in index".into());
            }
            if group["tracks"].as_array().is_none_or(Vec::is_empty) {
                return Err("Empty track group in index".into());
            }
            for track in group["tracks"]
                .as_array_mut()
                .ok_or("Missing track index")?
            {
                let path = text(track, "mlp");
                let details = entries
                    .get(&path)
                    .or_else(|| {
                        entries
                            .iter()
                            .find(|(key, _)| dvda_native::files::ordinal_ignore_case(key, &path))
                            .map(|(_, v)| v)
                    })
                    .and_then(Value::as_object)
                    .ok_or_else(|| format!("曲目索引缺少编码参数：{path}"))?;
                let source = text(track, "src");
                if !details
                    .get("src")
                    .and_then(Value::as_str)
                    .is_some_and(|value| dvda_native::files::ordinal_ignore_case(value, &source))
                {
                    return Err(format!("曲目索引的音源路径不一致：{path}"));
                }
                let mode = details
                    .get("mlp_source")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !matches!(mode, "surcode-batch" | "lpcm" | "external" | "surcode") {
                    return Err(format!("曲目索引的编码方式无效：{path}"));
                }
                let target = track.as_object_mut().ok_or("Invalid track index")?;
                target.extend(details.clone());
                target.insert("mlp".into(), json!(path));
            }
        }
    }
    Ok(value)
}

fn check_cancel(caller: &mut dyn Callbacks) -> Result<(), String> {
    if caller.cancelled() {
        Err("成品验证已取消。".into())
    } else {
        Ok(())
    }
}

fn find_isos(directory: &Path, prefix: &str) -> Result<Vec<PathBuf>, String> {
    if !directory.is_dir() {
        return Err(format!(
            "Final directory is missing: {}",
            directory.display()
        ));
    }
    let mut paths = fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && matches_iso_name(path, prefix))
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| iso_number(path).unwrap_or(i32::MAX));
    Ok(paths)
}

fn matches_iso_name(path: &Path, prefix: &str) -> bool {
    if prefix.trim().is_empty() {
        return path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("iso"));
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    let prefix = prefix.to_ascii_lowercase();
    lower.starts_with(&(prefix.clone() + "_"))
        && lower.ends_with(".iso")
        && lower[prefix.len() + 1..lower.len() - 4]
            .parse::<i32>()
            .is_ok()
}

fn iso_number(path: &Path) -> Option<i32> {
    path.file_stem()?
        .to_string_lossy()
        .rsplit('_')
        .next()?
        .parse()
        .ok()
}

fn capacity(job: &Job, isos: &[PathBuf], expected_tracks: i32, diagnostics: &mut Vec<Value>) {
    if isos.is_empty() {
        diagnostics.push(error("NO_ISO", "No published ISO images were found"));
        return;
    }
    if job.max_discs > 0 && isos.len() as i32 > job.max_discs {
        diagnostics.push(error(
            "DISC_COUNT_EXCEEDED",
            format!(
                "{} ISO images exceed the configured limit {}",
                isos.len(),
                job.max_discs
            ),
        ));
    }
    for iso in isos {
        match fs::metadata(iso) {
            Ok(metadata) => {
                if job.disc_bytes > 0 && metadata.len() > job.disc_bytes as u64 {
                    diagnostics.push(error(
                        "ISO_CAPACITY_EXCEEDED",
                        format!(
                            "{} is {} bytes, above the configured disc capacity",
                            iso.display(),
                            metadata.len()
                        ),
                    ));
                }
                match formats::iso_list_directory(iso, "") {
                    Ok(entries)
                        if !entries.iter().any(|entry| {
                            entry.is_directory && entry.name.eq_ignore_ascii_case("AUDIO_TS")
                        }) =>
                    {
                        diagnostics.push(error("AUDIO_TS_MISSING", "ISO 根目录缺少 AUDIO_TS"))
                    }
                    Err(reason) => diagnostics.push(error("ISO_READ_FAILED", reason)),
                    _ => {}
                }
                if let (Some(title), Some(number)) = (&job.title, iso_number(iso)) {
                    match formats::dispatch("iso.info", json!(iso)) {
                        Ok(info) => {
                            let actual = info["VolumeIdentifier"].as_str().unwrap_or_default();
                            let expected = format!("{title} {number}");
                            if actual.trim() != expected.trim() {
                                diagnostics.push(error(
                                    "VOLUME_ID_MISMATCH",
                                    format!("卷标 {actual} != 期望 {expected}"),
                                ));
                            }
                        }
                        Err(reason) => diagnostics.push(error("ISO_READ_FAILED", reason)),
                    }
                }
                match formats::iso_list_directory(iso, "AUDIO_TS") {
                    Ok(entries) => {
                        let aob = entries.iter().filter(|entry| is_aob(&entry.name)).count();
                        let ifo = entries.iter().filter(|entry| is_ifo(&entry.name)).count();
                        if aob == 0 {
                            diagnostics.push(error(
                                "AOB_MISSING",
                                format!("{} has no AOB files", iso.display()),
                            ));
                        }
                        if ifo == 0 {
                            diagnostics.push(error(
                                "AUDIO_IFO_MISSING",
                                format!("{} has no group IFO files", iso.display()),
                            ));
                        }
                    }
                    Err(error_text) => diagnostics.push(error("ISO_READ_FAILED", error_text)),
                }
            }
            Err(message) => diagnostics.push(error(
                "ISO_READ_FAILED",
                format!("{}: {message}", iso.display()),
            )),
        }
    }
    if expected_tracks == 0 {
        diagnostics.push(error(
            "INDEX_TRACKS_MISSING",
            "The formal index contains no tracks",
        ));
    }
}

/// Standalone developer quick-check/audit needs only ISO files and optional
/// manifest/log evidence, just like the original C# developer command.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Inspection {
    pub final_directory: PathBuf,
    pub iso_prefix: String,
    pub manifest_path: Option<PathBuf>,
    pub build_log_path: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub allow_log_fallback: bool,
    pub audit: bool,
}
pub fn inspect(job: Inspection, caller: &mut dyn Callbacks) -> Outcome {
    let mut run = || -> Result<Outcome, String> {
        check_cancel(caller)?;
        let isos = find_isos(&job.final_directory, &job.iso_prefix)?;
        let mut diagnostics = Vec::new();
        if isos.is_empty() {
            diagnostics.push(error("NO_ISO", "No published ISO images were found"));
        }
        let evidence = timeline(&isos, None, &mut diagnostics, caller, job.audit, false)?;
        diagnostics.extend(verification::manifest_issues(
            job.manifest_path.as_deref(),
            &evidence,
        ));
        if job.audit {
            diagnostics.extend(verification::log_issues(
                job.build_log_path.as_deref(),
                job.allow_log_fallback,
                &evidence,
            ));
        } else if let Some(path) = job.build_log_path.as_ref().filter(|path| path.exists()) {
            let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
            if verification::has_padding_failure(&text) {
                diagnostics.push(error("PES_PADDING_FAILED", "构建日志包含 pack 补齐失败"));
            }
        }
        Ok(Outcome {
            succeeded: !diagnostics.iter().any(is_error),
            diagnostics,
            iso_count: isos.len() as i32,
            track_count: evidence.iter().map(|disc| disc.track_count() as i32).sum(),
        })
    };
    match run() {
        Ok(value) => value,
        Err(message) => Outcome {
            succeeded: false,
            diagnostics: vec![error("VERIFY_FAILED", message)],
            iso_count: 0,
            track_count: 0,
        },
    }
}
fn timeline(
    isos: &[PathBuf],
    index: Option<&Value>,
    diagnostics: &mut Vec<Value>,
    caller: &mut dyn Callbacks,
    audit: bool,
    statistics: bool,
) -> Result<Vec<DiscEvidence>, String> {
    let discs = index.and_then(|value| value["__discs__"].as_array());
    let mut observations = Vec::new();
    for iso in isos {
        check_cancel(caller)?;
        let mut evidence = DiscEvidence {
            path: iso.clone(),
            groups: Default::default(),
        };
        let disc = discs.and_then(|discs| {
            discs.iter().find(|value| {
                text(value, "iso").eq_ignore_ascii_case(
                    iso.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default(),
                )
            })
        });
        if index.is_some() && disc.is_none() {
            diagnostics.push(error(
                "INDEX_ISO_MISSING",
                format!("No index entry for {}", iso.display()),
            ));
        }
        let groups = disc.and_then(|disc| disc["groups"].as_array());
        let entries = match formats::iso_list_directory(iso, "AUDIO_TS") {
            Ok(entries) => entries,
            Err(reason) => {
                diagnostics.push(error("ISO_READ_FAILED", reason));
                observations.push(evidence);
                continue;
            }
        };
        let actual: std::collections::HashSet<_> = entries
            .iter()
            .filter_map(|entry| parse_ifo(&entry.name))
            .collect();
        if actual.is_empty() {
            diagnostics.push(error(
                "AUDIO_IFO_MISSING",
                format!("{} has no group IFO files", iso.display()),
            ));
        }
        if let Some(groups) = groups {
            let declared: std::collections::HashSet<_> = groups
                .iter()
                .filter_map(|group| number(group, "group"))
                .collect();
            let aob_groups: std::collections::HashSet<_> = entries
                .iter()
                .filter_map(|entry| parse_aob(&entry.name).map(|(group, _)| group))
                .collect();
            if declared != actual || declared != aob_groups {
                diagnostics.push(error(
                    "GROUP_INDEX_MISMATCH",
                    format!("{}: ISO groups differ from index", iso.display()),
                ));
            }
        }
        let mut actual: Vec<_> = actual.into_iter().collect();
        actual.sort();
        for number in actual {
            check_cancel(caller)?;
            let parsed = match read_iso_file(iso, &format!("AUDIO_TS/ATS_{number:02}_0.IFO"))
                .and_then(|data| crate::ifo::parse(&data, number as i32))
            {
                Ok(parsed) => parsed,
                Err(reason) => {
                    diagnostics.push(error(
                        "IFO_INVALID",
                        format!("{} group {number}: {reason}", iso.display()),
                    ));
                    continue;
                }
            };
            let context = format!("{} group {number} (IFO)", iso.display());
            for issue in &parsed.issues {
                diagnostics.push(error(issue, format!("{} group {number}", iso.display())));
            }
            if let Some(group) = groups.and_then(|groups| {
                groups
                    .iter()
                    .find(|group| self::number(group, "group") == Some(number))
            }) {
                let expected = group["tracks"].as_array().map_or(0, Vec::len);
                if parsed.rows.len() != expected {
                    diagnostics.push(error(
                        "TRACK_COUNT_MISMATCH",
                        format!(
                            "{} group {number}: IFO {}, index {expected}",
                            iso.display(),
                            parsed.rows.len()
                        ),
                    ));
                }
            }
            if parsed.maximum_still > 0 {
                match read_iso_file(iso, "AUDIO_TS/AUDIO_SV.IFO") {
                    Ok(still) => {
                        let count = still
                            .get(12..14)
                            .map(|bytes| u16::from_be_bytes(bytes.try_into().unwrap()));
                        if count.is_none_or(|count| u16::from(parsed.maximum_still) > count) {
                            diagnostics.push(error(
                                "STILL_REFERENCE_OUT_OF_RANGE",
                                format!("{} group {number}", iso.display()),
                            ));
                        }
                    }
                    Err(reason) => diagnostics.push(error("STILL_REFERENCE_OUT_OF_RANGE", reason)),
                }
            }
            let mut aobs: Vec<_> = entries
                .iter()
                .filter(|entry| parse_aob(&entry.name).is_some_and(|(group, _)| group == number))
                .collect();
            aobs.sort_by_key(|entry| parse_aob(&entry.name).unwrap().1);
            let sector_count = aobs
                .iter()
                .map(|entry| i64::from(entry.size) / SECTOR_SIZE as i64)
                .sum();
            let mut observation = GroupEvidence {
                rows: parsed.rows,
                sectors: sector_count,
                missing_sector: -1,
                ..Default::default()
            };
            if audit {
                diagnostics.extend(verification::sector_relations(
                    &observation.rows,
                    sector_count,
                    &context,
                ));
            }
            if aobs.is_empty() {
                diagnostics.push(error(
                    "AOB_MISSING",
                    format!("{} group {number} has no AOB", iso.display()),
                ));
                evidence.groups.insert(number as i32, observation);
                continue;
            }
            if !audit {
                let mut input = fs::File::open(iso).map_err(|e| e.to_string())?;
                for row in &observation.rows {
                    let mut first = i64::from(row.first);
                    let mut header = [0u8; 4];
                    let mut found = false;
                    for entry in &aobs {
                        let sectors = u64::from(entry.size).div_ceil(SECTOR_SIZE as u64) as i64;
                        if first >= 0 && first < sectors {
                            if entry.is_multi_extent {
                                return Err("Multi-extent AOB is unsupported".into());
                            }
                            let offset = (u64::from(entry.logical_block_address)
                                + u64::from(entry.extended_attribute_blocks)
                                + first as u64)
                                * SECTOR_SIZE as u64;
                            input
                                .seek(SeekFrom::Start(offset))
                                .map_err(|e| e.to_string())?;
                            found = input.read_exact(&mut header).is_ok();
                            break;
                        }
                        first -= sectors;
                    }
                    if !found || header != [0, 0, 1, 0xba] {
                        diagnostics.push(error(
                            "TRACK_NOT_PACK",
                            format!("{} group {number} sector {}", iso.display(), row.first),
                        ));
                    }
                }
                evidence.groups.insert(number as i32, observation);
                continue;
            }
            caller.emit(
                1,
                &format!("[verify] timeline {} group {}", iso.display(), number),
            );
            let names: Vec<_> = aobs.iter().map(|entry| entry.name.clone()).collect();
            let mut state = aob::AuditState::default();
            let mut timestamps = Vec::new();
            let mut scanned_sectors = 0i32;
            let starts: std::collections::HashSet<_> =
                observation.rows.iter().map(|row| row.first).collect();
            let mut failed = false;
            for chunk in chunks(iso, &names) {
                check_cancel(caller)?;
                match chunk {
                    Ok(data) => {
                        if !data.len().is_multiple_of(SECTOR_SIZE) {
                            return Err("AOB data is not sector aligned".into());
                        }
                        for sector in data.as_chunks::<SECTOR_SIZE>().0 {
                            if starts.contains(&scanned_sectors) && sector[..4] != [0, 0, 1, 0xba] {
                                diagnostics.push(error(
                                    "TRACK_NOT_PACK",
                                    format!(
                                        "{} group {number} sector {}",
                                        iso.display(),
                                        scanned_sectors
                                    ),
                                ));
                            }
                            if statistics {
                                let timestamp = aob::sector_pts(sector, true)?;
                                if timestamp >= 0 {
                                    timestamps.push(timestamp);
                                }
                            }
                            scanned_sectors = scanned_sectors
                                .checked_add(1)
                                .ok_or("AOB sector count overflow")?;
                            let already_missing = state.missing_sector >= 0;
                            let drop = state.observe(sector)?;
                            if drop >= 0 {
                                observation.drops.push(drop);
                            }
                            if !already_missing && state.missing_sector >= 0 {
                                diagnostics.push(error(
                                    "PTS_MISSING",
                                    format!(
                                        "{} group {} contains a sector without PTS",
                                        iso.display(),
                                        number
                                    ),
                                ));
                                // AuditState deliberately stops at the first
                                // missing timestamp. Statistics still require
                                // every valid timestamp in the complete group.
                            }
                        }
                    }
                    Err(reason) => {
                        diagnostics.push(error("AOB_READ_FAILED", reason));
                        failed = true;
                    }
                }
                if failed {
                    break;
                }
            }
            if statistics && !failed {
                let stats = aob::pts_statistics(&timestamps);
                for code in &stats.issue_codes {
                    let message = match code.as_str() {
                        "PTS_TOO_FEW" => {
                            format!("有效 PTS 只有 {} 个，时间轴缺失", timestamps.len())
                        }
                        "PTS_NOT_ADVANCING" => "首末 PTS 相同，时间轴没有推进".into(),
                        "PTS_ABNORMAL_RATIO" => format!(
                            "异常步长 {}/{}，占比 {:.3}% > 1%",
                            stats.abnormal_steps,
                            stats.steps.len(),
                            stats.abnormal_ratio
                        ),
                        _ => unreachable!("PTS statistics returned an unknown diagnostic"),
                    };
                    let mut issue = error(code, message);
                    issue["Iso"] = json!(iso);
                    issue["Group"] = json!(number);
                    diagnostics.push(issue);
                }
            }
            observation.scanned_sectors = scanned_sectors;
            observation.missing_sector = state.missing_sector;
            observation.scan_complete = !failed && state.missing_sector < 0;
            if !failed && state.missing_sector < 0 {
                for row in &observation.rows {
                    if row.first >= state.sector_count || row.last >= state.sector_count {
                        diagnostics.push(error(
                            "CELL_OUT_OF_RANGE",
                            format!("{} group {number}", iso.display()),
                        ));
                    }
                }
                let issues = aob::dispatch(
                    "aob.audit_diagnostics",
                    json!({"Drops":observation.drops,"MissingSector":state.missing_sector,"SectorCount":state.sector_count,"Rows":observation.rows}),
                )?;
                for item in issues.as_array().ok_or("Invalid PTS audit")? {
                    diagnostics.push(error(
                        item["Code"].as_str().unwrap_or("PTS_INVALID"),
                        format!(
                            "{} group {} has an invalid PTS timeline",
                            iso.display(),
                            number
                        ),
                    ));
                }
            }
            evidence.groups.insert(number as i32, observation);
        }
        observations.push(evidence);
    }
    Ok(observations)
}

pub(crate) fn read_iso_file(iso: &Path, name: &str) -> Result<Vec<u8>, String> {
    let mut reader = formats::iso_file_chunks(iso, name, CHUNK_SIZE)?;
    let mut data = Vec::new();
    while let Some(chunk) = reader.next_chunk()? {
        if data.len() + chunk.len() > 16 * 1024 * 1024 {
            return Err("IFO exceeds size limit".into());
        }
        data.extend(chunk);
    }
    Ok(data)
}

fn lossless(
    job: &Job,
    isos: &[PathBuf],
    index: &Value,
    diagnostics: &mut Vec<Value>,
    caller: &mut dyn Callbacks,
) -> Result<(), String> {
    let verifier = NativeDiscVerifier::load()?;
    let discs = index
        .get("__discs__")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for iso in isos {
        check_cancel(caller)?;
        let Some(disc) = discs.iter().find(|value| {
            text(value, "iso").eq_ignore_ascii_case(
                iso.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default(),
            )
        }) else {
            continue;
        };
        for group in disc
            .get("groups")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let number = number(group, "group").unwrap_or(0);
            let tracks = group
                .get("tracks")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let names = aob_names(iso, number)?;
            let all_lpcm = tracks
                .iter()
                .all(|track| text(track, "mlp_source") == "lpcm");
            let all_mlp = tracks
                .iter()
                .all(|track| text(track, "mlp_source") != "lpcm");
            let sources = tracks
                .iter()
                .map(|track| PathBuf::from(text(track, "mlp")))
                .collect::<Vec<_>>();
            if sources.iter().any(|path| !path.is_file()) {
                diagnostics.push(error(
                    "SOURCE_MISSING",
                    format!(
                        "{} group {} has a missing source track",
                        iso.display(),
                        number
                    ),
                ));
                continue;
            }
            caller.emit(
                1,
                &format!("[verify] lossless {} group {}", iso.display(), number),
            );
            let reader = chunks(iso, &names).map(|chunk| {
                check_cancel(caller)?;
                chunk
            });
            let result = if all_lpcm {
                let ends = tracks
                    .iter()
                    .map(|track| {
                        let mlp = text(track, "mlp");
                        track
                            .get("lpcm_title_end")
                            .and_then(Value::as_bool)
                            .map(u8::from)
                            .ok_or_else(|| format!("Missing LPCM title boundary for {mlp}"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                verifier.verify_lpcm(&sources, &ends, reader).map(|_| ())
            } else if all_mlp {
                verifier.verify_mlp(&sources, reader).map(|_| ())
            } else {
                Err("A group mixes LPCM and MLP tracks".into())
            };
            check_cancel(caller)?;
            if let Err(error_text) = result {
                diagnostics.push(error(
                    "ISO_AUDIO_MISMATCH",
                    format!("{} group {}: {error_text}", iso.display(), number),
                ));
            }
            for track in &tracks {
                check_cancel(caller)?;
                match crate::verify_audio::track(
                    &job.media_library,
                    &job.work_directory,
                    track,
                    caller,
                ) {
                    Ok(()) => caller.emit(
                        1,
                        &format!("[校验] {}：完整目标 PCM 一致。", text(track, "src")),
                    ),
                    Err(reason) => {
                        check_cancel(caller)?;
                        diagnostics.push(error(
                            "TRACK_PCM_MISMATCH",
                            format!("{}：{reason}", text(track, "src")),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn aob_names(iso: &Path, group: i64) -> Result<Vec<String>, String> {
    let entries = formats::iso_list_directory(iso, "AUDIO_TS")?;
    let mut names = entries
        .into_iter()
        .filter_map(|entry| {
            let (actual_group, segment) = parse_aob(&entry.name)?;
            (actual_group == group).then_some((segment, entry.name))
        })
        .collect::<Vec<_>>();
    names.sort_by_key(|(segment, _)| *segment);
    Ok(names.into_iter().map(|(_, name)| name).collect())
}

fn chunks<'a>(
    iso: &'a Path,
    names: &'a [String],
) -> Box<dyn Iterator<Item = Result<Vec<u8>, String>> + 'a> {
    let mut readers = Vec::new();
    for name in names {
        match formats::iso_file_chunks(iso, &format!("AUDIO_TS/{name}"), CHUNK_SIZE) {
            Ok(reader) => readers.push(reader),
            Err(error_text) => return Box::new(std::iter::once(Err(error_text))),
        }
    }
    let mut index = 0usize;
    Box::new(std::iter::from_fn(move || {
        loop {
            let reader = readers.get_mut(index)?;
            match reader.next_chunk() {
                Ok(Some(data)) => return Some(Ok(data)),
                Ok(None) => index += 1,
                Err(error_text) => return Some(Err(error_text)),
            }
        }
    }))
}

fn parse_aob(name: &str) -> Option<(i64, i64)> {
    let lower = name.to_ascii_lowercase();
    let parts: Vec<_> = lower
        .strip_prefix("ats_")?
        .strip_suffix(".aob")?
        .split('_')
        .collect();
    if parts.len() != 2 {
        return None;
    }
    Some((parts[0].parse().ok()?, parts[1].parse().ok()?))
}
fn is_aob(name: &str) -> bool {
    parse_aob(name).is_some()
}
fn parse_ifo(name: &str) -> Option<i64> {
    name.to_ascii_lowercase()
        .strip_prefix("ats_")?
        .strip_suffix("_0.ifo")?
        .parse()
        .ok()
}
fn is_ifo(name: &str) -> bool {
    parse_ifo(name).is_some()
}
fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .or_else(|| value.get(key.to_ascii_lowercase()))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
fn number(value: &Value, key: &str) -> Option<i64> {
    value
        .get(key)
        .or_else(|| value.get(key.to_ascii_lowercase()))
        .and_then(Value::as_i64)
}
fn index_track_count(index: &Value) -> i32 {
    index
        .get("__discs__")
        .and_then(Value::as_array)
        .map(|discs| {
            discs
                .iter()
                .flat_map(|disc| {
                    disc.get("groups")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                })
                .flat_map(|group| {
                    group
                        .get("tracks")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                })
                .count() as i32
        })
        .unwrap_or(0)
}
fn error(code: &str, message: impl Into<String>) -> Value {
    json!({"Severity":2,"Code":code,"Message":message.into()})
}
fn unavailable(code: &str, message: impl Into<String>) -> Value {
    json!({"Severity":2,"Code":code,"Message":message.into(),"Unavailable":true})
}
fn is_error(value: &Value) -> bool {
    value.get("Severity").and_then(Value::as_i64) == Some(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_empty_groups_duplicate_discs_and_ambiguous_groups() {
        let valid = json!({"x.mlp":{"src":"x.flac","mlp_source":"surcode-batch"},
            "__discs__":[{"iso":"disc_1.iso","groups":[{"group":1,"tracks":[{"mlp":"x.mlp","src":"x.flac"}]}]}]});
        assert!(resolve_index(valid.clone()).is_ok());
        let mut bad = valid.clone();
        bad["__discs__"][0]["groups"][0]["tracks"] = json!([]);
        assert!(resolve_index(bad).is_err());
        let mut bad = valid.clone();
        bad["__discs__"]
            .as_array_mut()
            .unwrap()
            .push(valid["__discs__"][0].clone());
        assert!(resolve_index(bad).is_err());
        let mut bad = valid.clone();
        bad["__discs__"][0]["groups"]
            .as_array_mut()
            .unwrap()
            .push(valid["__discs__"][0]["groups"][0].clone());
        assert!(resolve_index(bad).is_err());
    }
}
