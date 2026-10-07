//! Complete Rust-owned preparation, encoding, authoring and publication flow.
//!
//! The only process this module launches is the project-built author. Media,
//! image and MLP work stays in the in-process native libraries.
use crate::{
    author, disc, index, lpcm, manifest, menu, mlp_import, mlp_workflow, process, publication,
};
use dvda_native::media::Callbacks;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    #[serde(default)]
    pub resume_enabled: bool,
    #[serde(default)]
    pub diagnostic_album_limit: Option<i32>,
    pub media_library: PathBuf,
    pub encoder_library: Option<PathBuf>,
    pub manifest_path: PathBuf,
    pub source_root: String,
    pub mlp_source: String,
    pub mlp_external_directory: String,
    pub build_directory: PathBuf,
    pub output_root: PathBuf,
    pub temporary_root: PathBuf,
    pub iso_directory: PathBuf,
    pub final_directory: PathBuf,
    pub mlp_index_path: PathBuf,
    pub build_log_path: PathBuf,
    pub title: String,
    pub iso_prefix: String,
    pub diagnostic_title_mode: String,
    pub disc_bytes: i64,
    pub group_track_limit: i32,
    pub mlp_sample_rate: i32,
    pub mlp_bits: i32,
    pub mlp_metadata_context: String,
    #[serde(default)]
    pub mlp_batch_temp_directory: PathBuf,
    #[serde(default)]
    pub mlp_batch_output_directory: PathBuf,
    pub encoder_identity: String,
    pub dvda_author: String,
    pub author_working_directory: Option<PathBuf>,
    #[serde(default)]
    pub menu_enabled: bool,
    #[serde(default)]
    pub menu_still_pictures: bool,
    #[serde(default)]
    pub menu_tracks_per_page: i32,
    #[serde(default)]
    pub menu_index_minimum_albums: i32,
    #[serde(default)]
    pub menu_cover_dim: i32,
    #[serde(default)]
    pub menu_font: String,
    #[serde(default)]
    pub menu_font_japanese: String,
    #[serde(default)]
    pub menu_font_korean: String,
    #[serde(default)]
    pub menu_directory: PathBuf,
    #[serde(default)]
    pub author_source: PathBuf,
    #[serde(default)]
    pub menu_binary_directory: PathBuf,
    #[serde(default)]
    pub image_library: Option<PathBuf>,
    #[serde(default)]
    pub menu_arguments: Vec<String>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub keep_temporary: bool,
    #[serde(default)]
    pub keep_intermediate: bool,
}

struct AuthorCallbacks<'a> {
    parent: &'a mut dyn Callbacks,
    disc: u64,
    total: u64,
    oversized: u64,
    coordinate_warnings: u64,
    phase: &'static str,
}

impl Callbacks for AuthorCallbacks<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        if let Some(number) = oversized_still_picture_number(text) {
            self.oversized += 1;
            self.parent
                .emit(1, &format!("[menu-cover] oversized {number}"));
        } else {
            self.parent.emit(stream, text);
            if text
                .trim()
                .starts_with("WARN: Button y coordinates are odd for button ")
            {
                self.coordinate_warnings += 1;
                if self.coordinate_warnings == 1 {
                    crate::task_log::warning(
                        self.parent,
                        "button_coordinates_seen",
                        1,
                        &self.disc.to_string(),
                    );
                }
            }
            if let Some((phase, name)) = author_phase(text) {
                // Raw author evidence remains unchanged in the detailed log.
                // Stage events are additional UI information, never fake progress.
                if phase != self.phase || !name.is_empty() {
                    self.phase = phase;
                    crate::task_log::emit(self.parent, phase, self.disc, self.total, name, text);
                }
            }
        }
    }

    fn cancelled(&mut self) -> bool {
        self.parent.cancelled()
    }

    fn progress(&mut self, completed: u64, total: u64) {
        self.parent.progress(completed, total);
    }
}

impl AuthorCallbacks<'_> {
    fn finish(&mut self) {
        let disc = format!("{}", self.disc);
        if self.oversized > 0 {
            crate::task_log::emit(
                self.parent,
                "oversized_summary",
                self.oversized,
                self.total,
                &disc,
                "",
            );
        }
        if self.coordinate_warnings > 0 {
            crate::task_log::warning(
                self.parent,
                "button_coordinates",
                self.coordinate_warnings,
                &disc,
            );
        }
    }
}

fn author_phase(line: &str) -> Option<(&'static str, &str)> {
    let line = line.trim().strip_prefix("[INF]")?.trim();
    if let Some(path) = line.strip_prefix("Auditing MLP file ") {
        Some((
            "author_audio",
            path.rsplit(['/', '\\']).next().unwrap_or(path),
        ))
    } else if line.starts_with("Searching MLP layout for file ") {
        Some(("author_layout", ""))
    } else if line.starts_with("Creating ISO with the in-process C ISO writer") {
        Some(("author_iso", ""))
    } else if line.starts_with("Creating ASVS") {
        Some(("author_stills", ""))
    } else if line.starts_with("Creating ") {
        Some(("author_navigation", ""))
    } else {
        None
    }
}

fn oversized_still_picture_number(line: &str) -> Option<u32> {
    let trimmed = line.trim();
    let line = trimmed.strip_prefix("[ERR]").unwrap_or(trimmed).trim();
    let number = line.strip_prefix("Exceeding stillpic buffer limit (2 MB) at pict #")?;
    number.strip_suffix('.').unwrap_or(number).parse().ok()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Outcome {
    pub succeeded: bool,
    pub plan: Option<Value>,
    pub tracks: Vec<Value>,
    pub cache_hits: i32,
    pub cache_rebuilt: i32,
    pub published: Vec<String>,
    pub diagnostics: Vec<Value>,
    pub index_path: Option<String>,
}

pub fn execute(job: Job, caller: &mut dyn Callbacks) -> Outcome {
    match run(job, caller) {
        Ok(result) => result,
        Err(message) => Outcome {
            succeeded: false,
            plan: None,
            tracks: Vec::new(),
            cache_hits: 0,
            cache_rebuilt: 0,
            published: Vec::new(),
            diagnostics: vec![error("BUILD_FAILED", message)],
            index_path: None,
        },
    }
}

fn run(job: Job, caller: &mut dyn Callbacks) -> Result<Outcome, String> {
    validate(&job)?;
    for directory in [
        &job.build_directory,
        &job.output_root,
        &job.temporary_root,
        &job.iso_directory,
        &job.final_directory,
    ] {
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }
    let initial = manifest::read(
        &job.manifest_path.to_string_lossy(),
        &job.mlp_external_directory,
    )?;
    let initial: Vec<disc::Track> = serde_json::from_value(disc::limit_albums(json!({
        "Tracks":initial,"AlbumLimit":job.diagnostic_album_limit
    }))?)
    .map_err(|e| e.to_string())?;
    if initial.is_empty() {
        return Err("manifest.json contains no tracks".into());
    }
    caller.emit(1, &format!("[build] {} tracks loaded", initial.len()));
    let mut diagnostics = crate::disk_space::check(&job, &initial, None, caller)?;
    let source_root = if job.source_root.is_empty() {
        job.manifest_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_string_lossy()
            .into_owned()
    } else {
        job.source_root.clone()
    };
    let tracks_value = serde_json::to_value(&initial).map_err(|error| error.to_string())?;
    caller.progress(0, 100);
    let acquisition = {
        let mut progress = dvda_native::media::ProgressScope::new(caller, 0, 65);
        acquire(&job, &source_root, tracks_value, &mut progress)?
    };
    let tracks: Vec<disc::Track> = serde_json::from_value(Value::Array(acquisition.tracks.clone()))
        .map_err(|error| format!("Invalid acquired tracks: {error}"))?;
    diagnostics.extend(acquisition.diagnostics);
    let plan = disc::plan(tracks.clone(), job.disc_bytes, job.group_track_limit)?;
    diagnostics.extend(serde_json::to_value(&plan.diagnostics).unwrap_or_default_array());
    let plan_value = serde_json::to_value(&plan).map_err(|error| error.to_string())?;
    crate::task_log::emit(
        caller,
        "disc_plan",
        plan.discs.len() as u64,
        tracks.len() as u64,
        "",
        "",
    );
    diagnostics.extend(crate::disk_space::check(
        &job,
        &tracks,
        Some(&plan.discs),
        caller,
    )?);
    let index_target = if job.dry_run {
        dry_run_path(&job.mlp_index_path)
    } else {
        pending_path(&job.mlp_index_path)
    };
    // Every early error/cancellation after creating the pending index must
    // remove it. A successfully published index has already been moved away.
    let _pending_cleanup = (!job.dry_run).then(|| PendingIndexCleanup(index_target.clone()));
    write_index(&job, &index_target, &plan_value, job.dry_run)?;
    if plan.has_errors || diagnostics.iter().any(is_error) {
        remove_if_file(&index_target);
        return Ok(Outcome {
            succeeded: false,
            plan: Some(plan_value),
            tracks: acquisition.tracks,
            cache_hits: acquisition.cache_hits,
            cache_rebuilt: acquisition.cache_rebuilt,
            published: Vec::new(),
            diagnostics,
            index_path: None,
        });
    }
    if job.dry_run {
        return Ok(Outcome {
            succeeded: true,
            plan: Some(plan_value),
            tracks: acquisition.tracks,
            cache_hits: acquisition.cache_hits,
            cache_rebuilt: acquisition.cache_rebuilt,
            published: Vec::new(),
            diagnostics,
            index_path: Some(index_target.to_string_lossy().into_owned()),
        });
    }
    let pending_index = index_target;
    let staging_root = job.build_directory.join("publish-staging");
    let can_resume = job.resume_enabled && !job.keep_intermediate;
    let staging = if can_resume {
        fs::create_dir_all(&staging_root).map_err(|e| e.to_string())?;
        staging_root
    } else {
        independent_staging(&staging_root)?
    };
    let mut resume = can_resume.then(|| crate::resume::Store::load(&staging, caller));
    let mut staged = Vec::new();
    caller.progress(65, 100);
    for (disc_index, disc) in plan.discs.iter().enumerate() {
        if caller.cancelled() {
            remove_if_file(&pending_index);
            return Err("Build cancelled".into());
        }
        let tag = format!("disc{}", disc.number);
        let output = job.output_root.join(&tag);
        let temporary = job.temporary_root.join(&tag);
        let iso = job.iso_directory.join(format!("{tag}.iso"));
        let staged_iso = staging.join(job.iso_name(disc.number));
        let signature = resume
            .as_ref()
            .map(|_| crate::resume::disc_signature(&job, disc))
            .transpose()?;
        if let Some(store) = resume.as_ref()
            && let Some(signature) = signature.as_deref()
            && store.matches(disc.number, signature, &staged_iso)
        {
            caller.emit(
                1,
                &format!("[续跑] 复用第 {} 张已完成的光盘。", disc.number),
            );
            // Re-emit the exact original author evidence into this formal build
            // section. Never manufacture track rows from the index or ISO.
            let audit_log = store.audit_log(disc.number).unwrap();
            if job.menu_enabled {
                update_still_expectation(
                    &pending_index,
                    disc.number,
                    crate::verification::still_picture_count(audit_log),
                )?;
            }
            let mut callbacks = AuthorCallbacks {
                parent: caller,
                disc: disc.number as u64,
                total: plan.discs.len() as u64,
                oversized: 0,
                coordinate_warnings: 0,
                phase: "",
            };
            for line in audit_log.lines() {
                callbacks.emit(1, line);
            }
            callbacks.finish();
            staged.push((staged_iso, job.iso_name(disc.number)));
            caller.progress(65 + ((disc_index + 1) * 35 / plan.discs.len()) as u64, 100);
            continue;
        }
        if let Some(store) = resume.as_mut() {
            store.discard(disc.number);
        }
        if staged_iso.exists() {
            fs::remove_file(&staged_iso).map_err(|e| e.to_string())?;
        }
        let menu_arguments = if job.menu_enabled {
            match menu::build_assets(&job, disc, caller, &mut diagnostics) {
                Ok(arguments) => arguments,
                Err(reason) => {
                    diagnostics.push(error("MENU_ASSET_FAILED", reason));
                    break;
                }
            }
        } else {
            Vec::new()
        };
        if diagnostics.iter().any(is_error) {
            break;
        }
        reset_directory(&output)?;
        reset_directory(&temporary)?;
        remove_file(&iso).map_err(|e| format!("清理文件失败 {}: {e}", iso.display()))?;
        let args = author::dispatch(
            "author.build_args",
            json!({
                "Disc":disc,
                "OutputDirectory":output,
                "TemporaryDirectory":temporary,
                "TitleMode":job.diagnostic_title_mode,
                "MenuArguments":if menu_arguments.is_empty() { job.menu_arguments.clone() } else { menu_arguments },
                "IsoPath":iso,
                "IsoVolume":format!("{} {}",job.title,disc.number)
            }),
        )?;
        let arguments: Vec<String> =
            serde_json::from_value(args).map_err(|error| error.to_string())?;
        caller.emit(1, &format!("[author] disc {}", disc.number));
        crate::task_log::emit(
            caller,
            "author_disc",
            disc.number as u64,
            plan.discs.len() as u64,
            "",
            "",
        );
        let author_command = format!(
            "+ \"{}\" {}",
            job.dvda_author,
            arguments
                .iter()
                .map(|arg| if arg.contains(char::is_whitespace) {
                    format!("\"{arg}\"")
                } else {
                    arg.clone()
                })
                .collect::<Vec<_>>()
                .join(" ")
        );
        if job.menu_enabled {
            update_still_expectation(
                &pending_index,
                disc.number,
                crate::verification::still_picture_count(&author_command),
            )?;
        }
        caller.emit(1, &author_command);
        let arguments_for_menu = arguments.clone();
        let result = {
            let mut author_callbacks = AuthorCallbacks {
                parent: caller,
                disc: disc.number as u64,
                total: plan.discs.len() as u64,
                oversized: 0,
                coordinate_warnings: 0,
                phase: "",
            };
            let result = process::execute(
                process::Job {
                    file_name: job.dvda_author.clone(),
                    arguments,
                    working_directory: job
                        .author_working_directory
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned()),
                    environment: Vec::new(),
                    output_code_page: 65001,
                    error_code_page: 65001,
                    capture_output: true,
                    capture_error: true,
                    timeout_millis: None,
                },
                &mut author_callbacks,
            );
            author_callbacks.finish();
            result
        };
        if result.failure.is_some() || result.exit_code != Some(0) {
            let detail = result
                .failure
                .as_ref()
                .map(|failure| format!("{}: {}", failure.kind, failure.message))
                .or_else(|| result.exit_code.map(|code| format!("exit code {code}")))
                .unwrap_or_else(|| "unknown process failure".into());
            caller.emit(
                2,
                &format!(
                    "[author] disc {} failed ({detail}){}",
                    disc.number,
                    result
                        .standard_error
                        .lines()
                        .rev()
                        .take(4)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .map(|line| format!("\n{line}"))
                        .collect::<String>()
                ),
            );
            diagnostics.push(error(
                "DVDA_AUTHOR_FAILED",
                format!("author failed for disc {} ({detail})", disc.number),
            ));
            break;
        }
        if !output.join("AUDIO_TS").is_dir() {
            diagnostics.push(error(
                "AUDIO_TS_MISSING",
                format!("author did not create AUDIO_TS for disc {}", disc.number),
            ));
            break;
        }
        if !iso.is_file() || fs::metadata(&iso).map(|m| m.len()).unwrap_or(0) == 0 {
            diagnostics.push(error(
                "ISO_WRITER_FAILED",
                format!("author did not create ISO for disc {}", disc.number),
            ));
            break;
        }
        if let Err(message) = validate_iso(&iso, job.disc_bytes) {
            diagnostics.push(error("ISO_VALIDATION_FAILED", message));
            break;
        }
        if job.menu_enabled {
            crate::task_log::emit(
                caller,
                "author_check_menu",
                disc.number as u64,
                plan.discs.len() as u64,
                "",
                "",
            );
            if let Err(message) = crate::menu_verify::verify_iso_navigation(&iso) {
                diagnostics.push(error("ISO_NAVIGATION_INVALID", message));
                break;
            }
            diagnostics.extend(crate::menu_check::verify(
                &job,
                disc,
                &output,
                &temporary,
                &arguments_for_menu,
                caller,
            )?);
            if diagnostics.iter().any(is_error) {
                break;
            }
        }
        if caller.cancelled() {
            return Err("Build cancelled".into());
        }
        let operation = if job.keep_intermediate {
            "publish.copy"
        } else {
            "publish.stage"
        };
        let staged_result = if job.keep_intermediate {
            publication::dispatch(operation, json!({"Source":iso,"Destination":staged_iso}))?
        } else {
            publication::dispatch(
                operation,
                json!({"Source":iso,"Directory":staging,"Name":job.iso_name(disc.number)}),
            )?
        };
        if !staged_result["Failure"].is_null() {
            return Err(staged_result["Failure"].to_string());
        }
        let audit_log = format!(
            "{author_command}\n{}\n{}",
            result.standard_output, result.standard_error
        );
        if let Some(store) = resume.as_mut()
            && let Some(signature) = signature.as_deref()
            && let Err(reason) = store.record(disc.number, signature, &staged_iso, &audit_log)
        {
            let message = format!("[警告] 续跑记录写入失败: {reason}");
            caller.emit(2, &message);
            diagnostics.push(warning("RESUME_WRITE_FAILED", message));
        }
        staged.push((staged_iso, job.iso_name(disc.number)));
        caller.progress(65 + ((disc_index + 1) * 35 / plan.discs.len()) as u64, 100);
        caller.emit(1, &format!("[build] staged disc {}", disc.number));
        if !job.keep_temporary {
            cleanup_directory(&temporary, &mut diagnostics, "TEMP_CLEANUP_FAILED");
        }
        if !job.keep_intermediate {
            cleanup_file(&iso, &mut diagnostics, "ISO_CLEANUP_FAILED");
            cleanup_directory(&output, &mut diagnostics, "OUTPUT_CLEANUP_FAILED");
        }
    }
    if diagnostics.iter().any(is_error) || staged.len() != plan.discs.len() {
        remove_if_file(&pending_index);
        return Ok(Outcome {
            succeeded: false,
            plan: Some(plan_value),
            tracks: acquisition.tracks,
            cache_hits: acquisition.cache_hits,
            cache_rebuilt: acquisition.cache_rebuilt,
            published: Vec::new(),
            diagnostics,
            index_path: None,
        });
    }
    let files: Vec<Value> = staged
        .iter()
        .map(|(source, name)| json!({"SourcePath":source,"FileName":name}))
        .collect();
    if caller.cancelled() {
        return Err("Build cancelled".into());
    }
    crate::task_log::emit(
        caller,
        "publish_start",
        0,
        files.len() as u64,
        "",
        &job.final_directory.to_string_lossy(),
    );
    let publication = publication::dispatch(
        "publish.set",
        json!({
            "IsoFiles":files,
            "FinalDirectory":job.final_directory,
            "PendingIndexPath":pending_index,
            "FormalIndexPath":job.mlp_index_path,
            "MoveStagedIsos":!job.keep_intermediate
        }),
    )?;
    let failure = publication
        .get("Failure")
        .filter(|value| !value.is_null())
        .map(|value| value.to_string());
    if let Some(failure) = failure {
        diagnostics.push(error("FINAL_PUBLICATION_FAILED", failure));
    }
    let published: Vec<String> = publication
        .get("Value")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect();
    let succeeded = !diagnostics.iter().any(is_error) && published.len() == staged.len();
    if succeeded {
        for (index, path) in published.iter().enumerate() {
            crate::task_log::emit(
                caller,
                "published_iso",
                (index + 1) as u64,
                published.len() as u64,
                Path::new(path)
                    .file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap_or_default(),
                path,
            );
        }
        crate::task_log::emit(
            caller,
            "published_directory",
            published.len() as u64,
            published.len() as u64,
            "",
            &job.final_directory.to_string_lossy(),
        );
    }
    if succeeded && !job.keep_intermediate {
        for (path, _) in &staged {
            cleanup_file(path, &mut diagnostics, "ISO_CLEANUP_FAILED");
        }
    }
    if succeeded && !can_resume {
        cleanup_directory(&staging, &mut diagnostics, "TEMP_CLEANUP_FAILED");
    }
    Ok(Outcome {
        succeeded,
        plan: Some(plan_value),
        tracks: acquisition.tracks,
        cache_hits: acquisition.cache_hits,
        cache_rebuilt: acquisition.cache_rebuilt,
        published,
        diagnostics,
        index_path: succeeded.then(|| job.mlp_index_path.to_string_lossy().into_owned()),
    })
}

fn validate_iso(iso: &Path, disc_bytes: i64) -> Result<(), String> {
    let size = fs::metadata(iso).map_err(|e| e.to_string())?.len();
    if disc_bytes <= 0 || size > disc_bytes as u64 {
        return Err(format!(
            "{} is {size} bytes, above the configured disc capacity",
            iso.display()
        ));
    }
    crate::formats::verify_dvd_audio_filesystem(iso)?;
    let files = crate::formats::iso_list_directory(iso, "AUDIO_TS")?;
    if !files
        .iter()
        .any(|entry| entry.name.eq_ignore_ascii_case("AUDIO_TS.IFO"))
        || !files
            .iter()
            .any(|entry| entry.name.to_ascii_uppercase().ends_with(".AOB"))
    {
        return Err(format!("ISO 缺少必要的 AUDIO_TS 文件：{}", iso.display()));
    }
    Ok(())
}

struct Acquisition {
    tracks: Vec<Value>,
    cache_hits: i32,
    cache_rebuilt: i32,
    diagnostics: Vec<Value>,
}

fn acquire(
    job: &Job,
    source_root: &str,
    tracks: Value,
    caller: &mut dyn Callbacks,
) -> Result<Acquisition, String> {
    match job.mlp_source.as_str() {
        "surcode-batch" => {
            let encoder = job
                .encoder_library
                .clone()
                .ok_or("MLP encoder library is required")?;
            let result = mlp_workflow::execute(
                mlp_workflow::Job {
                    media_library: job.media_library.clone(),
                    encoder_library: encoder,
                    source_root: source_root.into(),
                    output_root: PathBuf::from(&job.mlp_external_directory),
                    cache_path: PathBuf::from(&job.mlp_external_directory).join("mlp-cache.json"),
                    temporary_directory: if job.mlp_batch_temp_directory.as_os_str().is_empty() {
                        job.build_directory.join("surcode-batch/temp")
                    } else {
                        job.mlp_batch_temp_directory.clone()
                    },
                    stage_directory: if job.mlp_batch_output_directory.as_os_str().is_empty() {
                        job.build_directory.join("surcode-batch/output")
                    } else {
                        job.mlp_batch_output_directory.clone()
                    },
                    sample_rate: job.mlp_sample_rate,
                    bits: job.mlp_bits,
                    jobs: 0,
                    metadata_context: job.mlp_metadata_context.clone(),
                    encoder_identity: job.encoder_identity.clone(),
                    tracks: tracks.as_array().cloned().unwrap_or_default(),
                },
                caller,
            )?;
            Ok(Acquisition {
                tracks: result.tracks,
                cache_hits: result.cache_hits,
                cache_rebuilt: result.cache_rebuilt,
                diagnostics: result.diagnostics,
            })
        }
        "external" => {
            let result = mlp_import::execute(
                mlp_import::Job {
                    media_library: job.media_library.clone(),
                    external_root: job.mlp_external_directory.clone(),
                    source_root: source_root.into(),
                    tracks: tracks.as_array().cloned().unwrap_or_default(),
                },
                caller,
            )?;
            Ok(Acquisition {
                tracks: result.tracks,
                cache_hits: result.cache_hits,
                cache_rebuilt: result.cache_rebuilt,
                diagnostics: result.diagnostics,
            })
        }
        "lpcm" => {
            let result = lpcm::execute(
                lpcm::PrepareJob {
                    media_library: job.media_library.clone(),
                    build_directory: job.build_directory.clone(),
                    cache_path: job.build_directory.join("lpcm/lpcm-cache.json"),
                    rate: job.mlp_sample_rate,
                    bits: job.mlp_bits,
                    identity: job.encoder_identity.clone(),
                    tracks: tracks.as_array().cloned().unwrap_or_default(),
                },
                caller,
            )?;
            let value = serde_json::to_value(result).map_err(|error| error.to_string())?;
            Ok(Acquisition {
                tracks: value["Tracks"].as_array().cloned().unwrap_or_default(),
                cache_hits: value["CacheHits"].as_i64().unwrap_or(0) as i32,
                cache_rebuilt: value["CacheRebuilt"].as_i64().unwrap_or(0) as i32,
                diagnostics: value["Diagnostics"].as_array().cloned().unwrap_or_default(),
            })
        }
        value => Err(format!("Unsupported MLP source: {value}")),
    }
}

fn validate(job: &Job) -> Result<(), String> {
    if !job.media_library.is_file() {
        return Err(format!(
            "Media library is missing: {}",
            job.media_library.display()
        ));
    }
    if job.mlp_source == "surcode-batch"
        && !job
            .encoder_library
            .as_ref()
            .is_some_and(|path| path.is_file())
    {
        return Err("MLP encoder library is missing".into());
    }
    if job.dvda_author.trim().is_empty() {
        return Err("dvda-author path is empty".into());
    }
    if !Path::new(&job.dvda_author).is_file() {
        return Err(format!(
            "dvda-author executable is missing: {}",
            job.dvda_author
        ));
    }
    Ok(())
}

fn write_index(job: &Job, path: &Path, plan: &Value, dry_run: bool) -> Result<(), String> {
    index::dispatch(
        "build.write_index",
        json!({
            "Path":path,
            "Plan":plan,
            "BuildDirectory":job.build_directory,
            "BuildLogPath":job.build_log_path,
            "DryRun":dry_run,
            "MlpSource":job.mlp_source,
            "MlpExternalDirectory":job.mlp_external_directory,
            "DiagnosticTitleMode":job.diagnostic_title_mode,
            "Title":job.title,
            "IsoPrefix":job.iso_prefix,
            "MenuEnabled":job.menu_enabled,
            "MenuTracksPerPage":job.menu_tracks_per_page,
            "MenuIndexMinimumAlbums":job.menu_index_minimum_albums,
            "MenuStillPictures":job.menu_still_pictures,
            "Generated":generated()
        }),
    )?;
    Ok(())
}

fn pending_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(".pending");
    value.into()
}
fn dry_run_path(path: &Path) -> PathBuf {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let name = if extension.is_empty() {
        format!("{stem}-dryrun")
    } else {
        format!("{stem}-dryrun.{extension}")
    };
    path.with_file_name(name)
}
fn reset_directory(path: &Path) -> Result<(), String> {
    remove_directory(path).map_err(|error| format!("清理目录失败 {}: {error}", path.display()))?;
    fs::create_dir_all(path).map_err(|error| error.to_string())
}
fn remove_directory(path: &Path) -> std::io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
fn remove_file(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
fn cleanup_directory(path: &Path, diagnostics: &mut Vec<Value>, code: &str) {
    if let Err(error) = remove_directory(path) {
        diagnostics.push(warning(
            code,
            format!("清理目录失败 {}: {error}", path.display()),
        ));
    }
}
fn cleanup_file(path: &Path, diagnostics: &mut Vec<Value>, code: &str) {
    if let Err(error) = remove_file(path) {
        diagnostics.push(warning(
            code,
            format!("清理文件失败 {}: {error}", path.display()),
        ));
    }
}
fn remove_if_file(path: &Path) {
    if path.is_file() {
        let _ = fs::remove_file(path);
    }
}
struct PendingIndexCleanup(PathBuf);
impl Drop for PendingIndexCleanup {
    fn drop(&mut self) {
        remove_if_file(&self.0);
    }
}
fn independent_staging(root: &Path) -> Result<PathBuf, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fs::create_dir_all(root).map_err(|error| error.to_string())?;
    loop {
        let path = root.join(format!(
            "run-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
}
fn generated() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs().to_string())
        .unwrap_or_default()
}
fn error(code: &str, message: impl Into<String>) -> Value {
    json!({"Severity":2,"Code":code,"Message":message.into()})
}
fn warning(code: &str, message: impl Into<String>) -> Value {
    json!({"Severity":1,"Code":code,"Message":message.into()})
}
fn is_error(value: &Value) -> bool {
    value.get("Severity").and_then(Value::as_i64) == Some(2)
}

trait DefaultArray {
    fn unwrap_or_default_array(self) -> Vec<Value>;
}
impl DefaultArray for Result<Value, serde_json::Error> {
    fn unwrap_or_default_array(self) -> Vec<Value> {
        self.ok()
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default()
    }
}

impl Job {
    pub(crate) fn iso_name(&self, index: usize) -> String {
        format!("{}_{}.iso", self.iso_prefix, index)
    }
}

fn update_still_expectation(index: &Path, disc: usize, count: usize) -> Result<(), String> {
    let mut value = crate::preparation::state::read_json(index).map_err(|e| e.message)?;
    if let Some(entry) = value["__discs__"].as_array_mut().and_then(|entries| {
        entries
            .iter_mut()
            .find(|entry| entry["disc"].as_u64() == Some(disc as u64))
    }) && !entry["menu"].is_null()
    {
        entry["menu"]["stills"] = json!(count);
    }
    crate::preparation::state::write_json(index, &value).map_err(|e| e.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::os::windows::fs::OpenOptionsExt;

    struct RecordedCallbacks(RefCell<Vec<(i32, String)>>);

    impl Callbacks for RecordedCallbacks {
        fn emit(&mut self, stream: i32, text: &str) {
            self.0.borrow_mut().push((stream, text.to_owned()));
        }

        fn cancelled(&mut self) -> bool {
            false
        }

        fn progress(&mut self, _: u64, _: u64) {}
    }

    #[test]
    fn only_the_known_oversized_still_warning_becomes_informational() {
        let mut recorded = RecordedCallbacks(RefCell::new(Vec::new()));
        {
            let mut callbacks = AuthorCallbacks {
                parent: &mut recorded,
                disc: 1,
                total: 1,
                oversized: 0,
                coordinate_warnings: 0,
                phase: "",
            };
            callbacks.emit(
                2,
                "[ERR]  Exceeding stillpic buffer limit (2 MB) at pict #17.",
            );
            callbacks.emit(2, "[ERR]  Image encoder failed");
        }
        assert_eq!(
            *recorded.0.borrow(),
            [
                (1, "[menu-cover] oversized 17".into()),
                (2, "[ERR]  Image encoder failed".into())
            ]
        );
    }

    #[test]
    fn locked_iso_cleanup_warns_and_preserves_file_until_unlocked() {
        let directory = std::env::temp_dir().join(format!(
            "dvda-cleanup-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let file = directory.join("中文.iso");
        fs::write(&file, b"verified ISO artifact").unwrap();
        let locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&file)
            .unwrap();
        let mut diagnostics = Vec::new();
        cleanup_file(&file, &mut diagnostics, "ISO_CLEANUP_FAILED");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0]["Code"], "ISO_CLEANUP_FAILED");
        assert_eq!(diagnostics[0]["Severity"], 1);
        assert_eq!(fs::read(&file).unwrap(), b"verified ISO artifact");
        for (language, prefix) in [
            ("zh", "清理文件失败"),
            ("en", "Could not remove file"),
            ("ja", "ファイルを削除できません"),
        ] {
            let translated = crate::localization::translate(
                language,
                diagnostics[0]["Message"].as_str().unwrap(),
            );
            assert!(translated.starts_with(prefix), "{language}: {translated}");
            assert!(translated.contains(&*file.to_string_lossy()));
        }
        drop(locked);
        diagnostics.clear();
        cleanup_file(&file, &mut diagnostics, "ISO_CLEANUP_FAILED");
        cleanup_file(&file, &mut diagnostics, "ISO_CLEANUP_FAILED");
        assert!(diagnostics.is_empty());
        assert!(!file.exists());
        fs::remove_dir(directory).unwrap();
    }
}
