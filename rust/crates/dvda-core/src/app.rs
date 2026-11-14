//! User-facing Rust application orchestration shared by the CLI and desktop UI.
//! Settings are stored as a versioned JSON profile. Environment variables are
//! retained only as explicit developer/automation overrides; `config.env` is
//! never discovered or parsed.
use crate::{build, config_files, options, preparation, verify};
use dvda_native::media::Callbacks;
use serde_json::{Map, Value, json};
use std::{
    env,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct AppOptions {
    pub language: String,
    pub values: Map<String, Value>,
    pub stored_profile_values: Map<String, Value>,
    pub config_path: Option<PathBuf>,
    pub executable_directory: PathBuf,
    workspace: Option<std::sync::Arc<crate::workspace::Workspace>>,
}

fn default_menu_font<'a>(language: &str, configured: &'a str) -> &'a str {
    if configured.trim().is_empty() {
        match crate::config_files::normalize_language(Some(language), "zh-CN")
            .as_deref()
            .unwrap_or("zh-CN")
        {
            "zh-TW" => "Noto-Sans-CJK-TC",
            "ko" => "Noto-Sans-CJK-KR",
            "ja" => "Noto-Sans-CJK-JP",
            _ => "Noto-Sans-CJK-SC",
        }
    } else {
        configured
    }
}

impl AppOptions {
    /// Load application options and normalize the selected profile and runtime paths.
    ///
    /// # Errors
    /// Returns an error for unreadable or invalid profiles, invalid path overrides, or unavailable runtime components.
    pub fn load(config_path: Option<&Path>) -> Result<Self, String> {
        let executable_directory = crate::runtime::directory()
            .map(Path::to_owned)
            .or_else(|| {
                env::current_exe()
                    .ok()
                    .and_then(|path| path.parent().map(Path::to_owned))
            })
            .unwrap_or_else(|| PathBuf::from("."));
        let selected = config_path
            .map(Path::to_owned)
            .or_else(|| Some(default_profile_path()))
            .filter(|path| path.is_file());
        if let Some(path) = config_path.filter(|path| !path.is_file()) {
            return Err(format!("Profile does not exist: {}", path.display()));
        }
        let local_app_data = env::var("LOCALAPPDATA")
            .or_else(|_| env::var("HOME"))
            .unwrap_or_else(|_| executable_directory.to_string_lossy().into_owned());
        let file_values = selected
            .as_deref()
            .map(|path| read_values(path, &local_app_data))
            .transpose()?
            .unwrap_or_default();
        let environment: Map<String, Value> = env::vars()
            .filter(|(key, _)| key.starts_with("DVDA_"))
            .map(|(key, value)| (key, Value::String(value)))
            .collect();
        let defaults = options::defaults(&local_app_data);
        // Keep a complete baseline when the user saves a profile for the first
        // time. An existing profile remains sparse so unknown plugin keys and
        // the user's original representation are preserved verbatim.
        let stored_profile_values = if file_values.is_empty() {
            defaults.clone()
        } else {
            file_values.clone()
        };
        let evaluated = evaluate(selected.as_deref(), file_values, environment, defaults);
        let properties = evaluated
            .get("Properties")
            .and_then(Value::as_object)
            .ok_or("Rust option evaluator returned no properties")?;
        let mut values = Map::new();
        for (name, item) in properties {
            if let Some(error) = item.get("Error").and_then(Value::as_str) {
                return Err(error.to_owned());
            }
            values.insert(
                name.clone(),
                item.get("Value").cloned().unwrap_or(Value::Null),
            );
        }
        let language = selected
            .as_deref()
            .and_then(|p| preparation::state::read_json(p).ok())
            .and_then(|v| v["Language"].as_str().map(str::to_owned))
            .unwrap_or_else(|| "zh-CN".into());
        Ok(Self {
            language,
            values,
            stored_profile_values,
            config_path: selected,
            executable_directory,
            workspace: None,
        })
    }

    /// Re-evaluate edited settings, including every path derived from the work
    /// folder. Explicit UI values take precedence for this unsaved session.
    /// # Errors
    /// Returns an error if normalized profile properties are not an object or
    /// option evaluation reports an invalid value.
    pub fn with_profile_values(&self, edits: Map<String, Value>) -> Result<Self, String> {
        let edits = normalize_owned_profile_values(edits);
        let mut copy = self.clone();
        copy.stored_profile_values.extend(edits.clone());
        let mut overrides: Map<String, Value> = env::vars()
            .filter(|(key, _)| key.starts_with("DVDA_"))
            .map(|(key, value)| (key, Value::String(value)))
            .collect();
        overrides.extend(edits);
        let local = env::var("LOCALAPPDATA").unwrap_or_default();
        let evaluated = evaluate(
            self.config_path.as_deref(),
            copy.stored_profile_values.clone(),
            overrides,
            options::defaults(&local),
        );
        copy.values.clear();
        for (name, item) in evaluated["Properties"]
            .as_object()
            .ok_or("Rust option evaluator returned no properties")?
        {
            if let Some(error) = item.get("Error").and_then(Value::as_str) {
                return Err(error.to_owned());
            }
            copy.values.insert(
                name.clone(),
                item.get("Value").cloned().unwrap_or(Value::Null),
            );
        }
        if let Some(workspace) = copy.workspace.take() {
            return Ok(copy.bind_workspace(workspace));
        }
        Ok(copy)
    }

    #[must_use]
    pub fn has_unfinished_task(&self) -> bool {
        crate::workspace::Workspace::unfinished(&self.path("BuildDirectory"))
    }

    /// # Errors
    /// Returns an error if the task workspace cannot be created, its saved
    /// options cannot be read or parsed, the saved values/profile are not
    /// objects, or the current task options cannot be saved.
    pub fn recover_task(&self, restart: bool) -> Result<Self, String> {
        let workspace = std::sync::Arc::new(crate::workspace::Workspace::create_task(
            &self.path("BuildDirectory"),
            true,
            Some(restart),
        )?);
        let saved = workspace.path().join("task-options.json");
        let original = if !restart && saved.is_file() {
            let value: Value =
                serde_json::from_slice(&std::fs::read(&saved).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("Cannot read original task parameters: {e}"))?;
            let mut original = self.clone();
            original.values = value["values"]
                .as_object()
                .cloned()
                .ok_or("Invalid task parameters")?;
            original.stored_profile_values = value["profile"]
                .as_object()
                .cloned()
                .ok_or("Invalid task profile")?;
            original
        } else {
            self.save_task_parameters(&saved)?;
            self.clone()
        };
        let mut copy = original.bind_workspace(workspace);
        // Reuse saved progress only within this unfinished task, including crash recovery.
        copy.values.insert("ResumeEnabled".into(), json!(true));
        copy.values
            .insert("PrepareCacheEnabled".into(), json!(true));
        Ok(copy)
    }

    /// Share one owned workspace across preparation, production and verification.
    /// # Errors
    /// Returns an error if a new task workspace cannot be created or its
    /// options cannot be serialized and written. An already bound task is reused.
    pub fn task_options(&self) -> Result<Self, String> {
        if self.workspace.is_some() {
            return Ok(self.clone());
        }
        let workspace = std::sync::Arc::new(crate::workspace::Workspace::create_task(
            &self.path("BuildDirectory"),
            true,
            None,
        )?);
        self.save_task_parameters(&workspace.path().join("task-options.json"))?;
        Ok(self.clone().bind_workspace(workspace))
    }

    fn save_task_parameters(&self, path: &Path) -> Result<(), String> {
        let data = serde_json::to_vec(
            &json!({"values": self.values, "profile": self.stored_profile_values}),
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(path, data).map_err(|e| e.to_string())
    }

    fn bind_workspace(mut self, workspace: std::sync::Arc<crate::workspace::Workspace>) -> Self {
        let copy = &mut self;
        let root = workspace.path();
        for (name, file) in [
            ("BuildDirectory", ""),
            ("ManifestPath", "manifest.json"),
            ("ReportPath", "decode_report.txt"),
            ("OutputRoot", "out"),
            ("TemporaryRoot", "tmp"),
            ("IsoDirectory", "iso"),
            ("MlpDirectory", "mlp"),
            ("MlpIndexPath", "mlp_index.json"),
            ("AlacFixDirectory", "alacfix"),
            ("MenuDirectory", "menu"),
            ("BuildLogPath", "build.log"),
            ("PrepareCachePath", "prepare-cache.json"),
            ("PrepareSnapshotPath", "prepare-snapshot.json"),
            ("MlpBatchTempDirectory", "mlp-batch-temp"),
            ("MlpBatchOutputDirectory", "mlp-batch-output"),
        ] {
            copy.values.insert(name.into(), json!(root.join(file)));
        }
        if copy.text("MlpSource") == "mlp-batch" {
            copy.values
                .insert("MlpExternalDirectory".into(), json!(root.join("mlp")));
        }
        copy.values.insert("ResumeEnabled".into(), json!(true));
        copy.values
            .insert("PrepareCacheEnabled".into(), json!(true));
        copy.values.insert("KeepTemporary".into(), json!(false));
        copy.values.insert("KeepIntermediate".into(), json!(false));
        copy.workspace = Some(workspace);
        self
    }

    /// # Errors
    /// Returns an error if the temporary task workspace cannot be created.
    pub fn scratch_options(&self) -> Result<Self, String> {
        let workspace =
            std::sync::Arc::new(crate::workspace::Workspace::create(Path::new(""), false)?);
        Ok(self.clone().bind_workspace(workspace))
    }

    pub fn discard_task(&self) {
        if let Some(workspace) = &self.workspace {
            workspace.discard();
        }
    }

    pub fn finish_task(&self) {
        if let Some(workspace) = &self.workspace {
            workspace.finish();
        }
    }

    #[must_use]
    pub fn profile_values(&self) -> Map<String, Value> {
        let mut values = self.stored_profile_values.clone();
        values.remove("DVDA_PREPARE_CACHE");
        values
    }

    #[must_use]
    pub fn value(&self, name: &str) -> &Value {
        self.values.get(name).unwrap_or(&Value::Null)
    }
    #[must_use]
    pub fn text(&self, name: &str) -> String {
        self.value(name).as_str().unwrap_or_default().to_owned()
    }
    #[must_use]
    pub fn boolean(&self, name: &str) -> bool {
        self.value(name).as_bool().unwrap_or(false)
    }
    #[must_use]
    pub fn integer(&self, name: &str) -> i64 {
        self.value(name).as_i64().unwrap_or(0)
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "Public values retain the historical low-i32 bit pattern even for out-of-range caller mutations"
    )]
    fn optional_integer32(&self, name: &str) -> Option<i32> {
        self.value(name).as_i64().map(|value| value as i32)
    }

    fn integer32(&self, name: &str) -> i32 {
        self.optional_integer32(name).unwrap_or(0)
    }

    #[must_use]
    pub fn path(&self, name: &str) -> PathBuf {
        resolve_path(&self.executable_directory, &self.text(name))
    }
    #[must_use]
    pub fn source_directory(&self) -> PathBuf {
        self.path("SourceDirectory")
    }
    /// # Errors
    /// Returns an error if the current executable path cannot be determined.
    pub fn media_library(&self) -> Result<PathBuf, String> {
        env::current_exe().map_err(|error| error.to_string())
    }
    /// # Errors
    /// Returns an error if the current executable path cannot be determined.
    pub fn encoder_library(&self) -> Result<PathBuf, String> {
        env::current_exe().map_err(|error| error.to_string())
    }
    /// # Errors
    /// Returns an error if the current executable path cannot be determined.
    pub fn image_library(&self) -> Result<PathBuf, String> {
        env::current_exe().map_err(|error| error.to_string())
    }
    /// # Errors
    /// Returns an error if the source directory is empty, the executable path
    /// cannot be determined, or an explicit menu runtime directory is invalid.
    pub fn build_job(&self, dry_run: bool) -> Result<build::Job, String> {
        let source = self.source_directory();
        if source.as_os_str().is_empty() {
            return Err("请先选择音源目录".into());
        }
        let media = self.media_library()?;
        let mlp_source = self.text("MlpSource");
        let encoder = (mlp_source == "mlp-batch")
            .then(|| self.encoder_library())
            .transpose()?;
        let menu_enabled = self.boolean("MenuEnabled");
        let image_library = None;
        let configured_author_source = self.path("AuthorSource");
        let author_source = if configured_author_source.is_dir() {
            configured_author_source
        } else if let Some(runtime_data) = crate::runtime::directory()
            .map(|directory| directory.join("data"))
            .filter(|directory| directory.is_dir())
        {
            // Release packages keep the menu assets in the extracted runtime;
            // no user-visible configuration should be required for them.
            runtime_data
        } else {
            self.executable_directory.join("data")
        };
        let menu_binary_directory = menu_runtime_directory(
            menu_enabled
                .then(|| env::var_os("DVDA_MENU_NATIVE_DIR").map(PathBuf::from))
                .flatten(),
            crate::runtime::directory(),
            &author_source,
            &self.executable_directory,
        )?;
        let menu_font = default_menu_font(&self.language, &self.text("MenuFont")).to_owned();
        let menu_font_japanese = self.text("MenuFontJapanese");
        let menu_font_korean = self.text("MenuFontKorean");
        Ok(build::Job {
            menu_language: self.language.clone(),
            resume_enabled: self.boolean("ResumeEnabled"),
            diagnostic_album_limit: self.optional_integer32("DiagnosticAlbumLimit"),
            media_library: media,
            encoder_library: encoder,
            manifest_path: self.path("ManifestPath"),
            source_root: source.to_string_lossy().into_owned(),
            mlp_source,
            mlp_external_directory: self.text("MlpExternalDirectory"),
            build_directory: self.path("BuildDirectory"),
            output_root: self.path("OutputRoot"),
            temporary_root: self.path("TemporaryRoot"),
            iso_directory: self.path("IsoDirectory"),
            final_directory: self.path("FinalDirectory"),
            mlp_index_path: self.path("MlpIndexPath"),
            build_log_path: self.path("BuildLogPath"),
            title: self.text("Title"),
            iso_prefix: self.text("IsoPrefix"),
            diagnostic_title_mode: self.text("DiagnosticTitleMode"),
            disc_bytes: self.integer("DiscBytes"),
            group_track_limit: self.integer32("GroupTrackLimit"),
            mlp_sample_rate: self.integer32("MlpSampleRate"),
            mlp_bits: self.integer32("MlpBits"),
            dither: self.boolean("Dither16"),
            mlp_metadata_context: self.text("MlpMetadataContext"),
            mlp_batch_temp_directory: self.path("MlpBatchTempDirectory"),
            mlp_batch_output_directory: self.path("MlpBatchOutputDirectory"),
            encoder_identity: "rust-mlp-encoder".into(),
            menu_enabled,
            menu_still_pictures: self.boolean("MenuStillPictures"),
            menu_tracks_per_page: self.integer32("MenuTracksPerPage"),
            menu_index_minimum_albums: self.integer32("MenuIndexMinimumAlbums"),
            menu_cover_dim: self.integer32("MenuCoverDim"),
            menu_norm: self.text("MenuNorm"),
            menu_font,
            menu_font_japanese,
            menu_font_korean,
            menu_directory: self.path("MenuDirectory"),
            author_source,
            menu_binary_directory,
            image_library,
            menu_arguments: Vec::new(),
            dry_run,
            keep_temporary: self.boolean("KeepTemporary"),
            keep_intermediate: self.boolean("KeepIntermediate"),
            image_cache: Arc::new(dvda_image::cache::ImageCache::default()),
        })
    }
    /// # Errors
    /// Returns an error if the current executable path cannot be determined.
    pub fn prepare_job(
        &self,
        force_revalidation: bool,
    ) -> Result<preparation::pipeline::Job, String> {
        Ok(preparation::pipeline::Job {
            language: self.language.clone(),
            library: self.media_library()?,
            source_directory: self.source_directory().to_string_lossy().into_owned(),
            manifest_path: self.path("ManifestPath"),
            report_path: self.path("ReportPath"),
            prepare_cache_path: self.path("PrepareCachePath"),
            prepare_snapshot_path: self.path("PrepareSnapshotPath"),
            alac_fix_directory: self.path("AlacFixDirectory"),
            mlp_source: self.text("MlpSource"),
            prepare_cache_enabled: self.boolean("PrepareCacheEnabled"),
            loss_error_seconds: f64::from_bits(self.integer("LossErrorSeconds").cast_unsigned()),
            loss_warning_seconds: f64::from_bits(
                self.integer("LossWarningSeconds").cast_unsigned(),
            ),
            force_revalidation,
            production: None,
        })
    }

    /// # Errors
    /// Returns an error if the current executable path cannot be determined.
    pub fn verify_job(&self) -> Result<verify::Job, String> {
        Ok(verify::Job {
            image_library: None,
            media_library: self.media_library()?,
            work_directory: self.path("BuildDirectory").join("verify-tmp"),
            final_directory: self.path("FinalDirectory"),
            iso_prefix: self.text("IsoPrefix"),
            title: Some(self.text("Title")),
            menu_enabled: self.boolean("MenuEnabled"),
            index_path: self.path("MlpIndexPath"),
            manifest_path: Some(self.path("ManifestPath")),
            build_log_path: Some(self.path("BuildLogPath")),
            allow_log_fallback: true,
            disc_bytes: self.integer("DiscBytes"),
        })
    }
}

pub fn run_prepare(
    options: &AppOptions,
    force_revalidation: bool,
    callbacks: &mut dyn Callbacks,
) -> preparation::pipeline::Outcome {
    let _identity_scope = crate::signature::TaskIdentityScope::new();
    let options = match options.task_options() {
        Ok(options) => options,
        Err(error) => {
            return preparation::pipeline::Outcome {
                exit_code: None,
                failure: Some(crate::media::Failure::new("InvalidData", &error)),
                data: None,
            };
        }
    };
    match options.prepare_job(force_revalidation) {
        Ok(job) => preparation::pipeline::execute(job, callbacks),
        Err(error) => preparation::pipeline::Outcome {
            exit_code: None,
            failure: Some(crate::media::Failure::new("InvalidData", &error)),
            data: None,
        },
    }
}

/// # Errors
/// Returns an error if task options or the build log cannot be created, job
/// configuration or the PCM signature cannot be obtained, or source preparation
/// fails. Build and verification execution failures are reported in the outcome.
pub fn run_build(
    options: &AppOptions,
    dry_run: bool,
    callbacks: &mut dyn Callbacks,
) -> Result<build::Outcome, String> {
    let _identity_scope = crate::signature::TaskIdentityScope::new();
    let standalone = options.workspace.is_none();
    let options = options.task_options()?;
    let log = if dry_run {
        options.path("BuildDirectory").join("build-dryrun.log")
    } else {
        options.path("BuildLogPath")
    };
    let mut callbacks = crate::buildlog::Writer::open(&log, callbacks)?;
    callbacks.emit(
        1,
        &format!(
            "[Rust build]{} {}",
            if dry_run { " [DRY-RUN]" } else { "" },
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ),
    );
    let mut preparation = options.prepare_job(false)?;
    if !dry_run && matches!(options.text("MlpSource").as_str(), "mlp-batch" | "lpcm") {
        let rate = options.integer32("MlpSampleRate");
        let bits = options.integer32("MlpBits");
        let dither = options.boolean("Dither16");
        let media = options.media_library()?;
        preparation.production = Some(preparation::pipeline::Production {
            rate,
            bits,
            dither,
            directory: options.path("BuildDirectory").join("production-pcm"),
            signature: crate::audio::production_pcm_signature(&media, rate, bits, dither)?,
            // The streaming MLP encoder supports live metadata refresh only on
            // x86_64; other targets take the decode-then-batch path.
            stream_mlp: options.text("MlpSource") == "mlp-batch"
                && crate::config::trim(&options.text("MlpMetadataContext")).is_empty()
                && cfg!(feature = "rust-mlp")
                && cfg!(target_arch = "x86_64"),
        });
    }
    preparation::pipeline::ensure_ready(&preparation, &mut callbacks)
        .map_err(|failure| failure.message)?;
    let mut result = build::execute(options.build_job(dry_run)?, &mut callbacks);
    if standalone && result.succeeded && !dry_run {
        let verification = verify::execute(options.verify_job()?, &mut callbacks);
        result.succeeded = verification.succeeded;
        result.diagnostics.extend(verification.diagnostics);
        if result.succeeded {
            options.finish_task();
        }
    }
    Ok(result)
}

/// # Errors
/// Returns the setup and source-reference reconstruction errors from
/// [`run_verify_mode`]. Verification failures are reported in the outcome.
pub fn run_verify(
    options: &AppOptions,
    callbacks: &mut dyn Callbacks,
) -> Result<verify::Outcome, String> {
    run_verify_mode(options, verify::Mode::All, None, callbacks)
}

/// # Errors
/// Returns an error if workspace creation, job configuration, or source
/// preparation fails, or reconstructed verification references cannot be built,
/// located, read, or parsed. Verification failures are reported in the outcome.
pub fn run_verify_mode(
    options: &AppOptions,
    mode: verify::Mode,
    iso: Option<PathBuf>,
    callbacks: &mut dyn Callbacks,
) -> Result<verify::Outcome, String> {
    let _identity_scope = crate::signature::TaskIdentityScope::new();
    if options.workspace.is_some() {
        return Ok(verify::execute_mode(
            options.verify_job()?,
            mode,
            iso,
            callbacks,
        ));
    }
    let workspace = std::sync::Arc::new(crate::workspace::Workspace::create(Path::new(""), false)?);
    let options = options.clone().bind_workspace(workspace);
    if matches!(mode, verify::Mode::Capacity | verify::Mode::Timeline)
        || (mode == verify::Mode::Menu && iso.is_some())
    {
        return Ok(verify::execute_mode(
            options.verify_job()?,
            mode,
            iso,
            callbacks,
        ));
    }
    preparation::pipeline::ensure_ready(&options.prepare_job(false)?, callbacks)
        .map_err(|failure| failure.message)?;
    let planned = build::execute(options.build_job(true)?, callbacks);
    if !planned.succeeded {
        return Err("Cannot reconstruct verification references from the selected sources".into());
    }
    let path = planned
        .index_path
        .ok_or("Missing reconstructed verification index")?;
    let index = preparation::state::read_json(Path::new(&path)).map_err(|e| e.message)?;
    Ok(verify::execute_source_reference(
        options.verify_job()?,
        mode,
        iso,
        index,
        callbacks,
    ))
}

#[must_use]
pub fn default_profile_path() -> PathBuf {
    let base = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_owned))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("settings.json")
}

fn read_values(path: &Path, local_app_data: &str) -> Result<Map<String, Value>, String> {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        return Err("Only JSON profiles are supported".into());
    }
    let defaults = options::defaults(local_app_data);
    let result = config_files::dispatch("profile.load", json!({"Path":path,"Defaults":defaults}))?;
    if let Some(failure) = result.get("Failure").filter(|failure| !failure.is_null()) {
        let detail = failure
            .get("Message")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                failure
                    .get("Code")
                    .and_then(Value::as_i64)
                    .and_then(|code| i32::try_from(code).ok())
                    .map(|code| std::io::Error::from_raw_os_error(code).to_string())
            })
            .unwrap_or_else(|| "Invalid JSON profile".into());
        return Err(format!("Cannot load profile {}: {detail}", path.display()));
    }
    let value = result
        .get("Value")
        .filter(|value| !value.is_null())
        .and_then(|value| value.get("Values"))
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{} is not a valid Rust profile", path.display()))?;
    Ok(normalize_profile_values(value))
}

fn normalize_profile_values(values: &Map<String, Value>) -> Map<String, Value> {
    normalize_owned_profile_values(values.clone())
}

fn normalize_owned_profile_values(mut result: Map<String, Value>) -> Map<String, Value> {
    for key in [
        "DVDA_PLANNED_DISCS",
        "PlannedDiscs",
        "DVDA_MLP_JOBS",
        "MlpJobs",
    ] {
        result.remove(key);
    }
    for (key, value) in &mut result {
        if key.starts_with("DVDA_") {
            match value {
                Value::Bool(v) => *value = Value::String(if *v { "on" } else { "off" }.into()),
                Value::Number(v) => *value = Value::String(v.to_string()),
                _ => {}
            }
        }
    }
    result
}

fn evaluate(
    selected: Option<&Path>,
    file_values: Map<String, Value>,
    environment: Map<String, Value>,
    defaults: Map<String, Value>,
) -> Value {
    options::evaluate(options::Request {
        config_path: selected.map(|path| path.to_string_lossy().into_owned()),
        file_values,
        environment,
        defaults,
        positive_sign: "+".into(),
        negative_sign: "-".into(),
    })
}
fn resolve_path(base: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() || value.is_empty() {
        path
    } else {
        env::current_dir()
            .unwrap_or_else(|_| base.to_owned())
            .join(path)
    }
}
fn menu_runtime_directory(
    explicit: Option<PathBuf>,
    embedded: Option<&Path>,
    author_source: &Path,
    executable_directory: &Path,
) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        if path.as_os_str().is_empty() || !path.is_dir() {
            return Err(format!(
                "DVDA_MENU_NATIVE_DIR: directory not found: {}",
                path.display()
            ));
        }
        return Ok(path);
    }
    Ok(embedded.map_or_else(
        || {
            author_source
                .parent()
                .unwrap_or(executable_directory)
                .join("menu-bin")
        },
        Path::to_owned,
    ))
}

/// # Errors
/// Returns an error if the path has no `.json` extension, profile dispatch
/// fails, or the profile writer reports a filesystem failure.
pub fn save_profile(
    path: &Path,
    values: &Map<String, Value>,
    language: &str,
) -> Result<(), String> {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        return Err("Profiles must use a .json extension".into());
    }
    let profile = json!({"Version":1,"Language":language,"Values":values});
    let result = config_files::dispatch(
        "profile.save",
        json!({"Path":path.to_string_lossy(),"Profile":profile}),
    )?;
    if let Some(failure) = result.get("Failure").filter(|f| !f.is_null()) {
        let detail = failure
            .get("Message")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                failure
                    .get("Code")
                    .and_then(Value::as_i64)
                    .and_then(|code| i32::try_from(code).ok())
                    .map(|code| std::io::Error::from_raw_os_error(code).to_string())
            })
            .unwrap_or_else(|| "Cannot write profile".into());
        return Err(format!("Cannot save profile {}: {detail}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn menu_defaults_follow_language_and_keep_explicit_fonts() {
        for (language, expected) in [
            ("zh-TW", "Noto-Sans-CJK-TC"),
            ("zh-Hant", "Noto-Sans-CJK-TC"),
            ("繁體中文", "Noto-Sans-CJK-TC"),
            ("zh-CN", "Noto-Sans-CJK-SC"),
            ("ja", "Noto-Sans-CJK-JP"),
            ("ko", "Noto-Sans-CJK-KR"),
            ("ko-KR", "Noto-Sans-CJK-KR"),
            ("en", "Noto-Sans-CJK-SC"),
        ] {
            assert_eq!(default_menu_font(language, ""), expected);
            assert_eq!(default_menu_font(language, "  "), expected);
            for explicit in ["Noto-Sans-CJK-SC", "Custom-Font", r"C:\字型\我的字體.otf"] {
                assert_eq!(default_menu_font(language, explicit), explicit);
            }
        }
        let defaults = options::defaults(".");
        for key in ["DVDA_MENU_FONT", "DVDA_MENU_FONT_JP", "DVDA_MENU_FONT_KR"] {
            assert_eq!(defaults[key], "");
        }
    }

    #[test]
    fn menu_runtime_selection_prefers_explicit_then_embedded_then_sibling() {
        let root = env::temp_dir().join(format!(
            "dvda-menu-selection-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("source");
        let embedded = root.join("embedded");
        let select = |explicit, bundled| menu_runtime_directory(explicit, bundled, &source, &root);
        assert_eq!(select(Some(root.clone()), Some(&embedded)).unwrap(), root);
        assert_eq!(select(None, Some(&embedded)).unwrap(), embedded);
        assert_eq!(select(None, None).unwrap(), root.join("menu-bin"));
        assert!(
            select(Some(root.join("missing")), Some(&embedded))
                .unwrap_err()
                .contains("DVDA_MENU_NATIVE_DIR")
        );
        assert!(select(Some(PathBuf::new()), Some(&embedded)).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn edited_work_folder_recomputes_every_dependent_path() {
        let original = AppOptions::load(None).unwrap();
        let changes = json!({"DVDA_BUILD_DIR":"C:/new-work", "DVDA_MLP_EXTERNAL_DIR":"", "DVDA_MLP_SOURCE":"mlp-batch"}).as_object().unwrap().clone();
        let edited = original.with_profile_values(changes).unwrap();
        for name in [
            "ManifestPath",
            "ReportPath",
            "OutputRoot",
            "TemporaryRoot",
            "IsoDirectory",
            "MlpDirectory",
            "MlpIndexPath",
            "AlacFixDirectory",
            "MenuDirectory",
            "BuildLogPath",
            "PrepareCachePath",
            "PrepareSnapshotPath",
            "MlpExternalDirectory",
        ] {
            assert!(
                edited.text(name).starts_with("C:/new-work\\"),
                "{name}: {}",
                edited.text(name)
            );
        }
    }

    #[test]
    fn saving_to_a_directory_reports_failure() {
        let token = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("dvda-unwritable-{token}.json"));
        std::fs::create_dir(&path).unwrap();
        assert!(save_profile(&path, &Map::new(), "ja").is_err());
        std::fs::remove_dir(&path).unwrap();
    }

    #[test]
    fn profile_normalization_does_not_expand_retired_aliases() {
        let values = json!({
            "SourceDirectory": "old-source",
            "FinalDirectory": "old-output",
            "BuildDirectory": "old-work",
            "Title": "old-title",
            "IsoPrefix": "old-prefix",
            "MlpSource": "old-mode",
            "MlpExternalDirectory": "old-mlp"
        });
        assert_eq!(
            normalize_profile_values(values.as_object().unwrap()),
            values.as_object().unwrap().clone()
        );
    }

    #[test]
    fn json_profile_round_trip_uses_settings_path() {
        let token = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = env::temp_dir()
            .join(format!("dvda-profile-{token}"))
            .join("settings.json");
        let mut values = Map::new();
        values.insert("DVDA_SRC".into(), Value::String("C:/Music/Album".into()));
        values.insert(
            "DVDA_FINAL_DIR".into(),
            Value::String("C:/DVD/Output".into()),
        );
        values.insert("DVDA_TITLE".into(), Value::String("Profile test".into()));
        values.insert(
            "CUSTOM_PLUGIN_SETTING".into(),
            Value::String("keep-me".into()),
        );
        save_profile(&path, &values, "en").expect("save profile");

        let loaded = AppOptions::load(Some(&path)).expect("load profile");
        assert_eq!(loaded.text("SourceDirectory"), "C:/Music/Album");
        assert_eq!(loaded.text("FinalDirectory"), "C:/DVD/Output");
        assert_eq!(loaded.text("Title"), "Profile test");
        assert_eq!(loaded.profile_values()["CUSTOM_PLUGIN_SETTING"], "keep-me");
        assert_eq!(loaded.config_path.as_deref(), Some(path.as_path()));
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(path.parent().expect("profile parent"));
    }

    #[test]
    fn non_json_profile_is_rejected() {
        let path = env::temp_dir().join("dvda-profile-not-json.env");
        std::fs::write(&path, b"DVDA_SRC=C:/invalid").expect("write fixture");
        let error = AppOptions::load(Some(&path)).expect_err("env profile must be rejected");
        assert!(error.contains("Only JSON profiles"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn exported_profile_omits_retired_prepare_cache_setting() {
        let options = AppOptions {
            language: "en".into(),
            values: Map::new(),
            stored_profile_values: serde_json::from_value(json!({
                "DVDA_PREPARE_CACHE": "on",
                "DVDA_TITLE": "My Disc"
            }))
            .unwrap(),
            config_path: None,
            executable_directory: PathBuf::new(),
            workspace: None,
        };
        assert_eq!(
            options.profile_values(),
            serde_json::from_value::<Map<String, Value>>(json!({"DVDA_TITLE": "My Disc"})).unwrap()
        );
        assert!(!options::defaults("").contains_key("DVDA_PREPARE_CACHE"));
    }

    #[test]
    fn public_values_preserve_low_i32_conversion_without_panicking() {
        let mut options = AppOptions {
            language: "en".into(),
            values: Map::new(),
            stored_profile_values: Map::new(),
            config_path: None,
            executable_directory: PathBuf::new(),
            workspace: None,
        };
        options
            .values
            .insert("GroupTrackLimit".into(), json!(4_294_967_395_i64));
        assert_eq!(options.integer32("GroupTrackLimit"), 99);
        options.values.insert("MlpBits".into(), json!(-1));
        assert_eq!(options.integer32("MlpBits"), -1);
        assert_eq!(options.optional_integer32("Missing"), None);
    }
}
