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
};

#[derive(Clone, Debug)]
pub struct AppOptions {
    pub language: String,
    pub values: Map<String, Value>,
    pub stored_profile_values: Map<String, Value>,
    pub config_path: Option<PathBuf>,
    pub executable_directory: PathBuf,
}

impl AppOptions {
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
        })
    }

    /// Re-evaluate edited settings, including every path derived from the work
    /// folder. Explicit UI values take precedence for this unsaved session.
    pub fn with_profile_values(&self, edits: Map<String, Value>) -> Result<Self, String> {
        let edits = normalize_profile_values(&edits);
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
        Ok(copy)
    }

    pub fn profile_values(&self) -> Map<String, Value> {
        self.stored_profile_values.clone()
    }

    pub fn value(&self, name: &str) -> &Value {
        self.values.get(name).unwrap_or(&Value::Null)
    }
    pub fn text(&self, name: &str) -> String {
        self.value(name).as_str().unwrap_or_default().to_owned()
    }
    pub fn boolean(&self, name: &str) -> bool {
        self.value(name).as_bool().unwrap_or(false)
    }
    pub fn integer(&self, name: &str) -> i64 {
        self.value(name).as_i64().unwrap_or(0)
    }
    pub fn path(&self, name: &str) -> PathBuf {
        resolve_path(&self.executable_directory, &self.text(name))
    }
    pub fn source_directory(&self) -> PathBuf {
        self.path("SourceDirectory")
    }
    pub fn media_library(&self) -> Result<PathBuf, String> {
        locate_file(
            "DVDA_MEDIA_NATIVE_LIBRARY",
            "DVDA_MEDIA_NATIVE_DIR",
            &self.executable_directory,
            &["dvda-media.dll"],
            &["media-native", "menu-bin"],
        )
    }
    pub fn encoder_library(&self) -> Result<PathBuf, String> {
        locate_file(
            "DVDA_ENCODER_LIBRARY",
            "",
            &self.executable_directory,
            &["mlp_encoder.dll"],
            &[],
        )
        .or_else(|_| {
            let development = PathBuf::from("native/mlp-encoder/win-x64/mlp_encoder.dll");
            development
                .canonicalize()
                .map_err(|error| format!("MLP encoder DLL not found: {error}"))
        })
    }
    pub fn image_library(&self) -> Result<PathBuf, String> {
        locate_file(
            "DVDA_IMAGE_NATIVE_LIBRARY",
            "DVDA_IMAGE_NATIVE_DIR",
            &self.executable_directory,
            &["dvda-image.dll"],
            &["image-native"],
        )
    }
    pub fn author_path(&self) -> PathBuf {
        let configured = self.text("DvdaAuthor");
        let path = resolve_path(&self.executable_directory, &configured);
        if path.is_file() {
            return path;
        }
        let flat = self.executable_directory.join(&configured);
        if flat.is_file() {
            return flat;
        }
        let bundled = self.executable_directory.join("menu-bin").join(&configured);
        if bundled.is_file() { bundled } else { path }
    }
    pub fn build_job(&self, dry_run: bool) -> Result<build::Job, String> {
        let source = self.source_directory();
        if source.as_os_str().is_empty() {
            return Err("请先选择音源目录".into());
        }
        let media = self.media_library()?;
        let mlp_source = self.text("MlpSource");
        let encoder = (mlp_source == "surcode-batch")
            .then(|| self.encoder_library())
            .transpose()?;
        let menu_enabled = self.boolean("MenuEnabled");
        let image_library = menu_enabled.then(|| self.image_library()).transpose()?;
        let author = self.author_path();
        let configured_author_source = self.path("AuthorSource");
        let author_source = if configured_author_source.is_dir() {
            configured_author_source
        } else {
            self.executable_directory.join("data")
        };
        let menu_binary_directory = crate::runtime::directory()
            .map(Path::to_owned)
            .unwrap_or_else(|| {
                author_source
                    .parent()
                    .map(Path::to_owned)
                    .unwrap_or_else(|| self.executable_directory.clone())
                    .join("menu-bin")
            });
        Ok(build::Job {
            resume_enabled: self.boolean("ResumeEnabled"),
            diagnostic_album_limit: self
                .value("DiagnosticAlbumLimit")
                .as_i64()
                .map(|v| v as i32),
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
            planned_discs: self.integer("PlannedDiscs") as i32,
            group_track_limit: self.integer("GroupTrackLimit") as i32,
            mlp_sample_rate: self.integer("MlpSurcodeSampleRate") as i32,
            mlp_bits: self.integer("MlpSurcodeBits") as i32,
            mlp_jobs: self.integer("MlpJobs") as i32,
            mlp_metadata_context: self.text("MlpMetadataContext"),
            mlp_batch_temp_directory: self.path("MlpBatchTempDirectory"),
            mlp_batch_output_directory: self.path("MlpBatchOutputDirectory"),
            encoder_identity: "rust-mlp-encoder".into(),
            dvda_author: author.to_string_lossy().into_owned(),
            author_working_directory: author.parent().map(Path::to_owned),
            menu_enabled,
            menu_still_pictures: self.boolean("MenuStillPictures"),
            menu_tracks_per_page: self.integer("MenuTracksPerPage") as i32,
            menu_index_minimum_albums: self.integer("MenuIndexMinimumAlbums") as i32,
            menu_cover_dim: self.integer("MenuCoverDim") as i32,
            menu_font: self.text("MenuFont"),
            menu_font_japanese: self.text("MenuFontJapanese"),
            menu_font_korean: self.text("MenuFontKorean"),
            menu_directory: self.path("MenuDirectory"),
            author_source,
            menu_binary_directory,
            image_library,
            menu_arguments: Vec::new(),
            dry_run,
            keep_temporary: self.boolean("KeepTemporary"),
            keep_intermediate: self.boolean("KeepIntermediate"),
        })
    }
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
            loss_error_seconds: f64::from_bits(self.integer("LossErrorSeconds") as u64),
            loss_warning_seconds: f64::from_bits(self.integer("LossWarningSeconds") as u64),
            force_revalidation,
        })
    }

    pub fn verify_job(&self) -> Result<verify::Job, String> {
        Ok(verify::Job {
            image_library: self.image_library().ok(),
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
            planned_discs: self.integer("PlannedDiscs") as i32,
        })
    }
}

pub fn run_prepare(
    options: &AppOptions,
    force_revalidation: bool,
    callbacks: &mut dyn Callbacks,
) -> preparation::pipeline::Outcome {
    match options.prepare_job(force_revalidation) {
        Ok(job) => preparation::pipeline::execute(job, callbacks),
        Err(error) => preparation::pipeline::Outcome {
            exit_code: None,
            failure: Some(crate::media::Failure::new("InvalidData", &error)),
            data: None,
        },
    }
}

pub fn run_build(
    options: &AppOptions,
    dry_run: bool,
    callbacks: &mut dyn Callbacks,
) -> Result<build::Outcome, String> {
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
    preparation::pipeline::ensure_ready(&options.prepare_job(false)?, &mut callbacks)
        .map_err(|failure| failure.message)?;
    Ok(build::execute(options.build_job(dry_run)?, &mut callbacks))
}

pub fn run_verify(
    options: &AppOptions,
    callbacks: &mut dyn Callbacks,
) -> Result<verify::Outcome, String> {
    Ok(verify::execute(options.verify_job()?, callbacks))
}

pub fn default_profile_path() -> PathBuf {
    let base = env::var_os("LOCALAPPDATA")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .or_else(|| {
            env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(Path::to_owned))
        })
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("DVD-Audio-Maker").join("settings.json")
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
                    .map(|code| std::io::Error::from_raw_os_error(code as i32).to_string())
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
    let mut result = values.clone();
    for (key, value) in &mut result {
        if key.starts_with("DVDA_") {
            match value {
                Value::Bool(v) => *value = Value::String(if *v { "on" } else { "off" }.into()),
                Value::Number(v) => *value = Value::String(v.to_string()),
                _ => {}
            }
        }
    }
    let aliases = [
        ("SourceDirectory", "DVDA_SRC"),
        ("FinalDirectory", "DVDA_FINAL_DIR"),
        ("BuildDirectory", "DVDA_BUILD_DIR"),
        ("Title", "DVDA_TITLE"),
        ("IsoPrefix", "DVDA_ISO_PREFIX"),
        ("MlpSource", "DVDA_MLP_SOURCE"),
        ("MlpExternalDirectory", "DVDA_MLP_EXTERNAL_DIR"),
        ("DvdaAuthor", "DVDA_AUTHOR"),
    ];
    for (effective, legacy) in aliases {
        if !result.contains_key(legacy)
            && let Some(value) = values.get(effective)
        {
            result.insert(legacy.into(), value.clone());
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
fn locate_file(
    primary_env: &str,
    directory_env: &str,
    base: &Path,
    names: &[&str],
    directories: &[&str],
) -> Result<PathBuf, String> {
    if !primary_env.is_empty()
        && let Some(path) = env::var_os(primary_env).map(PathBuf::from)
        && path.is_file()
    {
        return Ok(path);
    }
    if !directory_env.is_empty()
        && let Some(value) = env::var_os(directory_env)
    {
        let path = PathBuf::from(value);
        let candidate = if path.extension().is_some() {
            path
        } else {
            path.join(names[0])
        };
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    for directory in directories {
        for name in names {
            let candidate = base.join(directory).join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(format!("内置组件缺失: {}", names.join(", ")))
}

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
                    .map(|code| std::io::Error::from_raw_os_error(code as i32).to_string())
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
    fn edited_work_folder_recomputes_every_dependent_path() {
        let original = AppOptions::load(None).unwrap();
        let changes = json!({"DVDA_BUILD_DIR":"C:/new-work", "DVDA_MLP_EXTERNAL_DIR":"", "DVDA_MLP_SOURCE":"surcode-batch"}).as_object().unwrap().clone();
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
}
