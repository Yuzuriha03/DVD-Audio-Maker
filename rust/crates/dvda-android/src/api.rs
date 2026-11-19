//! `flutter_rust_bridge` bindings over the real dvda-core application entrypoints.
//!
//! No orchestration or log rules live here: Flutter passes a desktop-compatible
//! profile JSON and the binding calls the shared `dvda_core::gui` task sequence
//! and `dvda_core::log_view` formatting, forwarding progress and cancellation.
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use dvda_core::app::AppOptions;
use dvda_core::gui;
use dvda_native::media::Callbacks;
use flutter_rust_bridge::frb;

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Monotonic counter starting at 1 so `last_revision == 0` always fetches the initial log.
static LOG_REVISION: AtomicU64 = AtomicU64::new(1);
static CANCEL: AtomicBool = AtomicBool::new(false);
static DONE: AtomicU64 = AtomicU64::new(0);
static TOTAL: AtomicU64 = AtomicU64::new(PROGRESS_TOTAL);
static TEXT: Mutex<String> = Mutex::new(String::new());

/// Progress is a single percentage scale on the Flutter side.
const PROGRESS_TOTAL: u64 = 100;
/// Build verification is done, but the SAF export has not delivered yet.
const DELIVERY_PENDING_PROGRESS: u64 = 99;

pub struct ProgressSnapshot {
    pub done: u64,
    pub total: u64,
    pub text: String,
    pub log: String,
    pub log_revision: u64,
}

fn clear_log() {
    if let Ok(mut log) = LOG.lock() {
        log.clear();
        LOG_REVISION.fetch_add(1, Ordering::Release);
    }
}

fn push_log(text: &str) {
    // Raw lines only: classification, hiding and localization are shared in
    // `dvda_core::log_view` and applied by the GUI through `format_log`.
    // The shared noise rules keep decoder chatter out of the 300-line ring
    // without ever dropping real problem lines.
    if dvda_core::log_view::is_diagnostic_noise(text) {
        return;
    }
    if let Ok(mut log) = LOG.lock() {
        log.push(text.to_owned());
        if log.len() > 300 {
            let drain = log.len() - 300;
            log.drain(..drain);
        }
        LOG_REVISION.fetch_add(1, Ordering::Release);
    }
}

fn set_text(text: &str) {
    if let Ok(mut slot) = TEXT.lock() {
        // Keep the complete structured event: truncating it produced invalid
        // JSON that the shared formatter could not localize, so raw [TASK] JSON
        // leaked into the progress label.
        text.clone_into(&mut slot);
    }
}

struct Forward {
    start: u64,
    end: u64,
    last: u64,
    /// App-private source mirror and the user-visible folder it represents.
    source_display: Option<(PathBuf, PathBuf)>,
}

impl Forward {
    const fn stage(start: u64, end: u64) -> Self {
        Self {
            start,
            end,
            last: start,
            source_display: None,
        }
    }

    /// Maps the app-private source mirror back to the folder the user picked.
    /// Windows passes no display key, so its log text is unchanged.
    fn with_source_display(mut self, options: &AppOptions) -> Self {
        let display = options.text("SourceDirectoryDisplay");
        let actual = options.path("SourceDirectory");
        // An empty actual root must never be replaced everywhere in a message.
        if !display.is_empty() && !actual.as_os_str().is_empty() {
            self.source_display = Some((actual, PathBuf::from(display)));
        }
        self
    }
}

impl Callbacks for Forward {
    fn emit(&mut self, _stream: i32, text: &str) {
        let mapped = match &self.source_display {
            Some((actual, display)) => {
                dvda_core::gui::display_product_message(actual, Some(display), text)
            }
            None => text.to_owned(),
        };
        let trimmed = mapped.trim();
        if trimmed.is_empty() {
            return;
        }
        // Only structured events drive the progress line; raw tool output must
        // not overwrite the readable status.
        if trimmed.starts_with("[TASK]") || trimmed.starts_with('{') {
            set_text(trimmed);
        }
        // Structured events also belong to the detailed log; the GUI renders
        // them through the shared formatter, never as raw JSON.
        push_log(trimmed);
    }

    fn cancelled(&mut self) -> bool {
        CANCEL.load(Ordering::Relaxed)
    }

    fn progress(&mut self, completed: u64, total: u64) {
        let total = total.max(1);
        let span = self.end.saturating_sub(self.start);
        let fraction = u128::from(completed.min(total)) * u128::from(span) / u128::from(total);
        let target = self
            .start
            .saturating_add(u64::try_from(fraction).unwrap_or(span));
        let done = target.max(self.last).min(PROGRESS_TOTAL);
        self.last = done;
        DONE.store(done, Ordering::Relaxed);
        TOTAL.store(PROGRESS_TOTAL, Ordering::Relaxed);
    }
}

impl gui::GuiCallbacks for Forward {
    fn product_delivery(&mut self) -> gui::ProductDelivery {
        // The user's final folder lives behind SAF; the shared build sequence
        // publishes into a task-owned transport folder and the Flutter SAF
        // adapter exports it after verification.
        gui::ProductDelivery::PlatformPending
    }

    fn set_progress_range(&mut self, start: u64, end: u64) {
        self.start = start;
        self.end = end;
        self.last = start;
    }

    fn as_callbacks(&mut self) -> &mut dyn Callbacks {
        self
    }
}

fn outcome_with_log(
    ok: bool,
    cancelled: bool,
    message: &str,
    diagnostics: &serde_json::Value,
    log: &str,
    delivery_directory: Option<&std::path::Path>,
) -> String {
    serde_json::json!({
        "Ok": ok,
        "Cancelled": cancelled,
        "Message": message,
        "Diagnostics": diagnostics,
        "Log": log,
        "ProgressDone": DONE.load(Ordering::Relaxed),
        "ProgressTotal": TOTAL.load(Ordering::Relaxed),
        // Present only while the platform still has to export the product;
        // Flutter exports exactly this task-local folder before finishing.
        "FinalDirectory": delivery_directory.map(|path| path.to_string_lossy().into_owned()),
    })
    .to_string()
}

fn map_display_paths(value: &mut serde_json::Value, actual: &Path, display: &Path) {
    match value {
        serde_json::Value::String(text) => {
            *text = dvda_core::gui::display_product_message(actual, Some(display), text);
        }
        serde_json::Value::Array(items) => {
            for item in items {
                map_display_paths(item, actual, display);
            }
        }
        serde_json::Value::Object(fields) => {
            for field in fields.values_mut() {
                map_display_paths(field, actual, display);
            }
        }
        _ => {}
    }
}
/// Clean-install Android font runtime: `<files>/runtime`, derived from the
/// existing app-private `<files>/build` directory.
///
/// The Android shell extracts the packaged `DvdaNotoCJK-Regular.ttc` there. The
/// shared menu runtime path (`DVDA_MENU_NATIVE_DIR`) stays the only resolver;
/// Windows never calls this adapter and keeps its embedded runtime path.
fn android_menu_runtime_directory(build: &Path) -> Option<PathBuf> {
    let files = build.parent()?;
    let runtime = files.join("runtime");
    runtime
        .join("fonts")
        .join("DvdaNotoCJK-Regular.ttc")
        .is_file()
        .then_some(runtime)
}

/// Adds the clean-install runtime folder as a profile property for this task.
///
/// The host already passes the app-private `DVDA_BUILD_DIR`; the packaged TTC
/// therefore lives at its sibling `runtime/fonts/DvdaNotoCJK-Regular.ttc`. The
/// shared menu runtime resolver only sees the ordinary profile property, so no
/// process-global environment or platform font rule is introduced.
fn with_android_menu_runtime(profile_json: String) -> String {
    let Ok(mut profile) = serde_json::from_str::<serde_json::Value>(&profile_json) else {
        return profile_json;
    };
    let Some(build) = profile
        .get("Values")
        .and_then(|values| values.get("DVDA_BUILD_DIR"))
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
    else {
        return profile_json;
    };
    let Some(runtime) = android_menu_runtime_directory(&build) else {
        return profile_json;
    };
    if let Some(values) = profile
        .get_mut("Values")
        .and_then(serde_json::Value::as_object_mut)
    {
        values.insert(
            "DVDA_MENU_NATIVE_DIR".into(),
            serde_json::Value::String(runtime.to_string_lossy().into_owned()),
        );
    }
    serde_json::to_string(&profile).unwrap_or(profile_json)
}
fn temporary_profile_path(profile_json: &str) -> Result<PathBuf, String> {
    let value: serde_json::Value = serde_json::from_str(profile_json)
        .map_err(|error| format!("Cannot parse task profile: {error}"))?;
    let build = value
        .get("Values")
        .and_then(|values| values.get("DVDA_BUILD_DIR"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("Task profile has no app-private DVDA_BUILD_DIR")?;
    let build = PathBuf::from(build);
    if !build.is_absolute() {
        return Err(format!(
            "Task build directory must be absolute: {}",
            build.display()
        ));
    }
    let directory = build.join(".bridge");
    std::fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "Cannot create Android bridge directory {}: {error}",
            directory.display()
        )
    })?;
    Ok(directory.join(format!(
        "profile-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )))
}

#[frb]
pub fn run_profile(profile_json: String, action: String) -> String {
    let profile_json = with_android_menu_runtime(profile_json);
    // Start from empty bridge state. Clearing after the profile is loaded let the
    // GUI poller read the previous run's errors for a moment, which showed a
    // short-lived error flash at task start.
    clear_log();
    if let Ok(mut text) = TEXT.lock() {
        text.clear();
    }
    CANCEL.store(false, Ordering::Relaxed);
    DONE.store(0, Ordering::Relaxed);
    TOTAL.store(PROGRESS_TOTAL, Ordering::Relaxed);
    // A failure before the native pipeline starts must never leave the task
    // panel without a log line.
    push_log(&format!("[bridge] action={action}"));
    let path = match temporary_profile_path(&profile_json) {
        Ok(path) => path,
        Err(error) => {
            push_log(&format!("[bridge] {error}"));
            let tail = LOG
                .lock()
                .map_or_else(|_| String::new(), |log| log.join("\n"));
            return outcome_with_log(false, false, &error, &serde_json::Value::Null, &tail, None);
        }
    };
    let started = std::time::Instant::now();
    let mut delivery_directory: Option<PathBuf> = None;
    let mut source: Option<(PathBuf, PathBuf)> = None;
    let mut cancelled = false;
    let result = (|| -> Result<(bool, String, serde_json::Value), String> {
        std::fs::write(&path, profile_json)
            .map_err(|error| format!("Cannot write task profile {}: {error}", path.display()))?;
        let options = AppOptions::load(Some(&path))?;
        // Android packages its menu font under app-private `files/runtime`;
        // `with_android_menu_runtime` already supplied the shared property.
        let source_display = options.text("SourceDirectoryDisplay");
        let source_actual = options.path("SourceDirectory");
        if !source_display.is_empty() && !source_actual.as_os_str().is_empty() {
            source = Some((source_actual, PathBuf::from(source_display)));
        }
        // The user's product lives in the SAF-selected folder. The task profile
        // carries that user-visible location only for logs; the shared build
        // sequence publishes into its task-owned transport folder internally.
        let final_display = options.text("FinalDirectoryDisplay");
        let final_display = if final_display.is_empty() {
            options.path("FinalDirectory").display().to_string()
        } else {
            final_display
        };
        let source_display = options.text("SourceDirectoryDisplay");
        let source_display = if source_display.is_empty() {
            options.path("SourceDirectory").display().to_string()
        } else {
            source_display
        };
        push_log(&format!(
            "[bridge] build={} src={} final={}",
            options.path("BuildDirectory").display(),
            source_display,
            final_display
        ));
        match action.as_str() {
            "status" => {
                let value = serde_json::json!({
                    "Ok": true,
                    "Unfinished": options.has_unfinished_task(),
                });
                Ok((true, value.to_string(), serde_json::Value::Null))
            }
            "discard" => {
                // The GUI discards progress before binding a task workspace, so
                // the bridge removes the unfinished task directory directly.
                let discarded = options.discard_progress()?;
                Ok((
                    true,
                    serde_json::json!({"Ok": true, "Discarded": discarded}).to_string(),
                    serde_json::Value::Null,
                ))
            }
            "finish_delivery" => {
                // Called only after the SAF adapter exported the task-local
                // product successfully. The shared tail adopts the unfinished
                // task and runs the same finish lifecycle as Windows, so the
                // exported transport copy is removed by the workspace drop
                // instead of any Android-specific cleanup pass.
                let outcome = gui::finish_after_delivery(&options, true);
                // Only a successful shared finish may show 100%; a failed finish
                // keeps the pending-delivery value and the workspace for retry.
                DONE.store(
                    if outcome.finished {
                        PROGRESS_TOTAL
                    } else {
                        DELIVERY_PENDING_PROGRESS
                    },
                    Ordering::Relaxed,
                );
                let message = if outcome.finished {
                    gui::status_text(&options.language, true).to_owned()
                } else {
                    String::new()
                };
                Ok((outcome.succeeded, message, serde_json::Value::Null))
            }
            _ => {
                // Recovery policy stays a GUI decision; the shared sequence owns
                // the prepare -> build -> verify -> finish stages and percentages.
                let options = match action.as_str() {
                    "build_continue" => options.recover_task(false)?,
                    "build_restart" => options.recover_task(true)?,
                    _ => options.clone(),
                };
                let mut forward = Forward::stage(0, PROGRESS_TOTAL).with_source_display(&options);
                let outcome = gui::run_command(&options, &mut forward);
                cancelled = outcome.cancelled;
                if outcome.succeeded {
                    // Pending SAF delivery is not a finished task yet.
                    DONE.store(
                        if outcome.pending_delivery {
                            DELIVERY_PENDING_PROGRESS
                        } else {
                            PROGRESS_TOTAL
                        },
                        Ordering::Relaxed,
                    );
                }
                let diagnostics =
                    serde_json::to_value(&outcome.diagnostics).unwrap_or(serde_json::Value::Null);
                let message = outcome.failure.unwrap_or_else(|| {
                    gui::status_text(&options.language, outcome.succeeded).to_owned()
                });
                delivery_directory = outcome.delivery_directory;
                Ok((outcome.succeeded, message, diagnostics))
            }
        }
    })();
    let elapsed = started.elapsed();
    // Visible stage timing: the Windows client shows the same numbers in its log
    // and both platforms must be comparable when reporting speed.
    push_log(&format!(
        "[bridge] {action} finished in {:.1} s",
        elapsed.as_secs_f64()
    ));
    let _ = std::fs::remove_file(&path);
    let tail = LOG
        .lock()
        .map_or_else(|_| String::new(), |log| log.join("\n"));
    let result = result.map(|(ok, mut message, mut diagnostics)| {
        if let Some((actual, display)) = &source {
            message = dvda_core::gui::display_product_message(actual, Some(display), &message);
            map_display_paths(&mut diagnostics, actual, display);
        }
        (ok, message, diagnostics)
    });
    match result {
        Ok((ok, message, diagnostics)) => outcome_with_log(
            ok,
            cancelled,
            &message,
            &diagnostics,
            &tail,
            delivery_directory.as_deref(),
        ),
        Err(error) => {
            let error = match &source {
                Some((actual, display)) => {
                    dvda_core::gui::display_product_message(actual, Some(display), &error)
                }
                None => error,
            };
            outcome_with_log(
                false,
                cancelled,
                &error,
                &serde_json::Value::Null,
                &tail,
                None,
            )
        }
    }
}

#[frb(sync)]
pub fn cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}

/// One formatted log view. Formatting and classification happen in the
/// shared `dvda_core::log_view` module so every GUI shows the same lines.
pub struct FormattedLog {
    pub text: String,
    pub problem_text: String,
    pub problems: u64,
}

/// One profile validation problem; the GUI localizes `code`.
pub struct ProfileIssue {
    pub key: String,
    pub code: String,
}

/// Localized text for a profile validation issue code.
#[frb(sync)]
#[must_use]
pub fn profile_issue_message(language: String, code: String) -> String {
    dvda_core::profile::issue_message(&language, &code)
}

/// Schema defaults and allowed values shared with the desktop shell.
#[frb(sync)]
#[must_use]
pub fn profile_options() -> String {
    dvda_core::profile::gui_options().to_string()
}

/// Shared capacity rule: combo index plus optional custom byte count.
#[frb(sync)]
#[must_use]
pub fn profile_disc_bytes(capacity: i64, custom_bytes: String) -> i64 {
    let custom = custom_bytes.trim().parse::<i64>().ok();
    dvda_core::profile::disc_bytes(capacity, custom)
}

/// Shared profile validation. LPCM 20-bit is reported, never rewritten.
#[frb(sync)]
#[must_use]
pub fn profile_validate(values_json: String, for_task: bool) -> Vec<ProfileIssue> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&values_json) else {
        return Vec::new();
    };
    let Some(values) = value.as_object() else {
        return Vec::new();
    };
    dvda_core::profile::validate_values(values, for_task)
        .into_iter()
        .map(|issue| ProfileIssue {
            key: issue.key.to_owned(),
            code: issue.code.to_owned(),
        })
        .collect()
}

/// Formats a single raw progress or log line with the shared Rust rules.
#[frb(sync)]
#[must_use]
pub fn format_line(language: String, text: String) -> String {
    dvda_core::log_view::render_line(&language, &text)
}

/// Formats a whole task log for the Flutter GUI and counts problem lines.
#[frb(sync)]
#[must_use]
pub fn format_log(language: String, log: String) -> FormattedLog {
    let formatted = dvda_core::log_view::format_log(&language, &log);
    FormattedLog {
        text: formatted.text,
        problem_text: formatted.problem_text,
        problems: formatted.problems,
    }
}

#[frb(sync)]
#[must_use]
pub fn progress_snapshot(last_revision: u64) -> ProgressSnapshot {
    let current_revision = LOG_REVISION.load(Ordering::Acquire);
    let (log_revision, log) = if current_revision == last_revision {
        (current_revision, String::new())
    } else {
        LOG.lock().map_or_else(
            |_| (current_revision, String::new()),
            |log| (LOG_REVISION.load(Ordering::Relaxed), log.join("\n")),
        )
    };
    ProgressSnapshot {
        done: DONE.load(Ordering::Relaxed),
        total: TOTAL.load(Ordering::Relaxed),
        text: TEXT
            .lock()
            .map_or_else(|_| String::new(), |slot| slot.clone()),
        log,
        log_revision,
    }
}

/// An available package for the installed Android edition.
pub struct AndroidUpdate {
    pub version: String,
    pub download_url: String,
}

#[frb(sync)]
#[must_use]
pub fn update_release_api_url() -> String {
    dvda_core::updates::RELEASE_API_URL.to_owned()
}

/// Validates the fetched release and returns zero or one Android ARM64 package.
///
/// # Errors
/// Returns an error if the version, release payload, or asset URL is invalid.
#[frb(sync)]
pub fn select_update(
    current_version: String,
    release_json: String,
) -> Result<Vec<AndroidUpdate>, String> {
    use dvda_core::updates::{Platform, select_update as select_release};

    select_release(&current_version, &release_json, Platform::AndroidArm64)
        .map(|update| {
            update
                .into_iter()
                .map(|item| AndroidUpdate {
                    version: item.version,
                    download_url: item.download_url,
                })
                .collect()
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bridge progress state is process-global; serialize tests touching it.
    static API_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn bridge_checks_installed_version_and_only_returns_android_assets() {
        const LINK: &str = "https://github.com/Yuzuriha03/DVD-Audio-Maker/releases/download/v1.3/DVD-Audio-Maker-v1.3-android-arm64.apk";
        let metadata = serde_json::json!({
            "tag_name": "v1.3", "draft": false, "prerelease": false,
            "assets": [{
                "name": "DVD-Audio-Maker-v1.3-android-arm64.apk",
                "state": "uploaded", "browser_download_url": LINK,
            }],
        });
        let results = select_update("1.2.0".into(), metadata.to_string()).unwrap();
        assert_eq!(results.len(), 1);
        let result = &results[0];
        assert_eq!(result.version, "v1.3");
        assert_eq!(result.download_url, LINK);
        assert!(
            select_update("1.3.0".into(), metadata.to_string())
                .unwrap()
                .is_empty()
        );
        assert!(select_update("1.2.0".into(), "invalid data".into()).is_err());
    }

    #[test]
    fn bridge_profile_uses_the_supplied_app_private_build_directory() {
        let root = std::env::temp_dir().join(format!(
            "dvda-android-path-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let build = root.join("files").join("build");
        let profile = serde_json::json!({"Values":{"DVDA_BUILD_DIR":build}});
        let path = temporary_profile_path(&profile.to_string()).unwrap();
        assert!(path.starts_with(build.join(".bridge")));
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("json")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bridge_profile_rules_are_shared_and_lpcm_20_is_an_error() {
        assert_eq!(
            profile_disc_bytes(0, String::new()),
            dvda_core::profile::DVD5_BYTES
        );
        assert_eq!(
            profile_disc_bytes(1, String::new()),
            dvda_core::profile::DVD9_BYTES
        );
        assert_eq!(profile_disc_bytes(2, "1234".into()), 1234);
        let values = serde_json::json!({
            "DVDA_MLP_SOURCE": "lpcm",
            "DVDA_MLP_SAMPLE_RATE": "48000",
            "DVDA_MLP_BITS": "20",
        })
        .to_string();
        let issues = profile_validate(values, true);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].key, "DVDA_MLP_BITS");
        assert_eq!(issues[0].code, "invalid_bits");
        let options: serde_json::Value = serde_json::from_str(&profile_options()).unwrap();
        assert_eq!(options["capacity"]["dvd5"], dvda_core::profile::DVD5_BYTES);
    }

    #[test]
    fn progress_snapshot_skips_unchanged_log_and_advances_revision() {
        let _guard = API_TEST_LOCK.lock().unwrap();
        clear_log();
        push_log("[error] first line");
        push_log("[error] second line");

        let first = progress_snapshot(0);
        assert!(first.log_revision > 0);
        assert!(first.log.contains("first line") && first.log.contains("second line"));

        // Polling again with the same revision must skip joining and return an empty delta.
        let unchanged = progress_snapshot(first.log_revision);
        assert_eq!(unchanged.log_revision, first.log_revision);
        assert_eq!(unchanged.log, "");

        // Appending a new line must advance the revision and return the updated log.
        push_log("[error] third line");
        let updated = progress_snapshot(first.log_revision);
        assert!(updated.log_revision > first.log_revision);
        assert!(updated.log.contains("third line"));

        // Starting a new profile run clears the ring and still advances the revision monotonically.
        let _ = run_profile("not-a-profile".into(), "status".into());
        let reset = progress_snapshot(updated.log_revision);
        assert!(reset.log_revision > updated.log_revision);
        assert!(!reset.log.contains("third line"));
    }

    #[test]
    fn format_line_renders_single_structured_event_and_hides_noise() {
        let event = format!(
            "{} {}",
            dvda_core::task_log::PREFIX,
            serde_json::json!({
                "Action": "disc_plan",
                "Current": 1,
                "Total": 2,
                "Name": "",
                "Detail": "",
                "Warning": false
            })
        );
        let rendered = format_line("zh-CN".into(), event);
        assert!(rendered.contains("分盘规划完成"), "{rendered}");
        assert!(!rendered.contains("\"Action\""), "{rendered}");
        assert_eq!(format_line("zh-CN".into(), "out_time_us=99".into()), "");
    }

    #[test]
    fn bridge_progress_text_keeps_the_whole_structured_event() {
        let _guard = API_TEST_LOCK.lock().unwrap();
        let event = format!(
            "{} {}",
            dvda_core::task_log::PREFIX,
            serde_json::json!({
                "Action": "disc_plan",
                "Current": 2,
                "Total": 149,
                "Name": "",
                "Detail": "",
                "Warning": false
            })
        );
        assert!(event.len() > 80);
        set_text(&event);
        let snapshot = progress_snapshot(0);
        assert_eq!(snapshot.text, event);
        let formatted = dvda_core::log_view::format_log("zh-CN", &snapshot.text);
        assert!(
            formatted.text.contains("分盘规划完成"),
            "{}",
            formatted.text
        );
        assert!(!formatted.text.contains("\"Action\""), "{}", formatted.text);
    }

    fn outcome_log(payload: &str) -> (serde_json::Value, String) {
        let value: serde_json::Value = serde_json::from_str(payload).unwrap();
        let log = value["Log"].as_str().unwrap_or_default().to_owned();
        (value, log)
    }

    #[test]
    fn bridge_log_ring_filters_noise_but_keeps_errors_and_structured_events() {
        let _guard = API_TEST_LOCK.lock().unwrap();
        if let Ok(mut log) = LOG.lock() {
            log.clear();
        }
        let mut forward = Forward::stage(0, PROGRESS_TOTAL);
        forward.emit(1, "out_time_us=123");
        forward.emit(1, "[space] C:/tmp");
        forward.emit(2, "[error] decoder failed");
        forward.emit(
            1,
            r#"[TASK] {"Action":"disc_plan","Current":2,"Total":149,"Name":"","Detail":""}"#,
        );
        let snapshot = progress_snapshot(0);
        assert!(!snapshot.log.contains("out_time_us"), "{}", snapshot.log);
        // Capacity warnings stay in the bridge ring; the shared formatter renders
        // them instead of dropping them.
        assert!(snapshot.log.contains("[space]"), "{}", snapshot.log);
        assert!(
            snapshot.log.contains("[error] decoder failed"),
            "{}",
            snapshot.log
        );
        assert!(snapshot.log.contains("disc_plan"), "{}", snapshot.log);
        // The structured event reaches the detailed log but the GUI formatter
        // turns it into readable text, never raw JSON.
        let formatted = format_log("zh-CN".into(), snapshot.log);
        assert!(
            formatted.text.contains("分盘规划完成"),
            "{}",
            formatted.text
        );
        assert!(!formatted.text.contains("\"Action\""), "{}", formatted.text);
    }

    #[test]
    fn bridge_state_resets_before_the_previous_error_can_flash() {
        let _guard = API_TEST_LOCK.lock().unwrap();
        // First run fails before the pipeline starts and leaves an error log.
        let first = run_profile("not-a-profile".into(), "status".into());
        let (first_value, first_log) = outcome_log(&first);
        assert_eq!(first_value["Ok"], false);
        assert!(!first_log.trim().is_empty(), "{first}");
        set_text("[TASK] stale-progress");

        // Second run must start from empty state: no stale error or progress.
        let root = std::env::temp_dir().join(format!(
            "dvda-bridge-reset-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let profile = serde_json::json!({
            "Version": 1,
            "Language": "en",
            "Values": {
                "DVDA_SRC": root.join("src"),
                "DVDA_FINAL_DIR": root.join("final"),
                "DVDA_BUILD_DIR": root.join("build")
            }
        })
        .to_string();
        let second = run_profile(profile.clone(), "status".into());
        let (second_value, second_log) = outcome_log(&second);
        assert_eq!(second_value["Ok"], true, "{second}");
        assert!(second_log.contains("[bridge] action=status"), "{second}");
        assert!(
            !second_log.contains("not-a-profile"),
            "stale log leaked: {second}"
        );
        assert!(!second_log.contains("stale-progress"), "{second}");
        let snapshot = progress_snapshot(0);
        assert!(
            !snapshot.text.contains("stale-progress"),
            "{}",
            snapshot.text
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_build_never_offers_a_delivery_directory() {
        let _guard = API_TEST_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "dvda-android-no-delivery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let profile = serde_json::json!({
            "Version": 1,
            "Language": "en",
            "Values": {
                "DVDA_SRC": root.join("missing-source"),
                "DVDA_FINAL_DIR": "",
                "DVDA_BUILD_DIR": root.join("build"),
            }
        })
        .to_string();
        let payload = run_profile(profile, "build".into());
        let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(value["Ok"], false, "{payload}");
        assert!(value["FinalDirectory"].is_null(), "{payload}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn finish_delivery_removes_the_unfinished_task_through_the_shared_workspace() {
        let _guard = API_TEST_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "dvda-android-finish-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let build = root.join("build");
        let task = build.join("unfinished-task");
        std::fs::create_dir_all(task.join("deliver")).unwrap();
        std::fs::write(task.join(".owner"), b"dvda-task-v1").unwrap();
        std::fs::write(task.join("deliver/disc_1.iso"), b"product").unwrap();
        std::fs::write(
            task.join("task-options.json"),
            serde_json::json!({"values": {}, "profile": {}}).to_string(),
        )
        .unwrap();
        let profile = serde_json::json!({
            "Version": 1,
            "Language": "en",
            "Values": {
                "DVDA_FINAL_DIR": "",
                "DVDA_BUILD_DIR": build,
            }
        })
        .to_string();
        let payload = run_profile(profile, "finish_delivery".into());
        let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(value["Ok"], true, "{payload}");
        assert!(
            !task.exists(),
            "shared finish must remove the task workspace"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn bridge_log_formatting_uses_the_shared_rules() {
        let log = [
            "out_time_us=1",
            "[error] decode failed",
            r#"[TASK] {"Action":"brand_new_stage","Current":1,"Total":2,"Name":"t.flac","Detail":""}"#,
        ]
        .join("\n");
        let formatted = format_log("en".into(), log);
        assert_eq!(formatted.problems, 1);
        assert!(!formatted.text.contains("out_time_us"));
        assert!(
            formatted.text.contains("brand_new_stage 1/2"),
            "{}",
            formatted.text
        );
        assert!(formatted.text.contains("[error] decode failed"));
        assert!(formatted.problem_text.contains("[error] decode failed"));
    }

    #[test]
    fn bridge_unknown_task_actions_are_never_dropped() {
        let raw = r#"[TASK] {"Action":"future_step","Current":3,"Total":7,"Name":"","Detail":""}"#;
        let formatted = format_log("zh-CN".into(), raw.into());
        assert_eq!(formatted.text, "future_step 3/7");
    }

    #[test]
    fn source_mirror_paths_are_replaced_by_the_user_display() {
        let _guard = API_TEST_LOCK.lock().unwrap();
        if let Ok(mut log) = LOG.lock() {
            log.clear();
        }
        let root = std::env::temp_dir().join(format!(
            "dvda-android-source-display-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let build = root.join("files/build");
        let mirror = root.join("files/saf-mirror/key");
        std::fs::create_dir_all(&build).unwrap();
        std::fs::create_dir_all(&mirror).unwrap();
        let profile = root.join("profile.json");
        std::fs::write(
            &profile,
            serde_json::json!({
                "Version": 1,
                "Language": "en",
                "Values": {
                    "DVDA_SRC": mirror,
                    "DVDA_SRC_DISPLAY_DIR": "/sdcard/Music",
                    "DVDA_BUILD_DIR": build,
                    "DVDA_FINAL_DIR": "",
                }
            })
            .to_string(),
        )
        .unwrap();
        let options = AppOptions::load(Some(&profile)).unwrap();
        let mut forward = Forward::stage(0, PROGRESS_TOTAL).with_source_display(&options);
        let source_path = mirror.join("a.flac");
        forward.emit(1, &format!("源文件不存在: {}", source_path.display()));
        let log = LOG
            .lock()
            .map_or_else(|_| String::new(), |log| log.join("\n"));
        // Host Windows uses backslashes in the remaining relative component.
        assert!(log.contains("/sdcard/Music"), "{log}");
        assert!(log.contains("a.flac"), "{log}");
        assert!(!log.contains("saf-mirror"), "{log}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_source_root_never_rewrites_log_text() {
        let _guard = API_TEST_LOCK.lock().unwrap();
        if let Ok(mut log) = LOG.lock() {
            log.clear();
        }
        let root = std::env::temp_dir().join(format!(
            "dvda-android-empty-source-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let build = root.join("files/build");
        std::fs::create_dir_all(&build).unwrap();
        let profile = root.join("profile.json");
        std::fs::write(
            &profile,
            serde_json::json!({
                "Version": 1,
                "Language": "en",
                "Values": {
                    "DVDA_SRC": "",
                    "DVDA_SRC_DISPLAY_DIR": "/sdcard/Music",
                    "DVDA_BUILD_DIR": build,
                    "DVDA_FINAL_DIR": "",
                }
            })
            .to_string(),
        )
        .unwrap();
        let options = AppOptions::load(Some(&profile)).unwrap();
        let mut forward = Forward::stage(0, PROGRESS_TOTAL).with_source_display(&options);
        let message = "SOURCE_MISSING: relative/path.flac";
        forward.emit(1, message);
        let log = LOG
            .lock()
            .map_or_else(|_| String::new(), |log| log.join("\n"));
        assert_eq!(log, message);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bridge_profile_rejects_a_missing_build_directory() {
        let error = temporary_profile_path("{\"Values\":{}}").unwrap_err();
        assert!(error.contains("DVDA_BUILD_DIR"));
    }

    #[test]
    fn clean_install_menu_font_runtime_is_derived_from_build_directory() {
        let _guard = API_TEST_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "dvda-android-font-runtime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let files = root.join("files");
        let build = files.join("build");
        let runtime = files.join("runtime");
        let fonts = runtime.join("fonts");
        std::fs::create_dir_all(&build).unwrap();
        std::fs::create_dir_all(&fonts).unwrap();
        // The Android shell extracts the packaged TTC here before any task.
        std::fs::write(fonts.join("DvdaNotoCJK-Regular.ttc"), b"font").unwrap();
        assert_eq!(
            android_menu_runtime_directory(&build),
            Some(runtime.clone())
        );

        let source = files.join("source");
        std::fs::create_dir_all(&source).unwrap();
        let profile = root.join("profile.json");
        let profile_json = serde_json::json!({
            "Version": 1,
            "Language": "en",
            "Values": {
                "DVDA_SRC": source,
                "DVDA_BUILD_DIR": build,
                "DVDA_FINAL_DIR": files.join("output"),
                // Only menu-enabled builds resolve the explicit runtime path.
                "DVDA_MENU": "on",
            }
        })
        .to_string();
        std::fs::write(&profile, with_android_menu_runtime(profile_json)).unwrap();
        let previous = std::env::var_os("DVDA_MENU_NATIVE_DIR");
        let options = AppOptions::load(Some(&profile)).unwrap();
        assert_eq!(
            options.text("MenuRuntimeDirectory"),
            runtime.to_string_lossy()
        );
        let job = options.build_job(true).unwrap();
        assert_eq!(job.menu_binary_directory, runtime);
        assert_eq!(std::env::var_os("DVDA_MENU_NATIVE_DIR"), previous);
        std::fs::remove_dir_all(root).unwrap();
    }
}
