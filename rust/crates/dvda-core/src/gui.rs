//! Shared GUI task orchestration (prepare → build → verify → finish).
//!
//! The Windows desktop shell and the Flutter/FRB Android host both run this
//! sequence, so stage order, progress ranges, scratch cleanup and finish
//! semantics exist once. Shells only adapt callbacks and UI messages.

use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::app::{self, AppOptions};
use crate::preparation::models::Issue;
use dvda_native::media::Callbacks;

/// Prepare-only run (the "check sources" button) spans the whole bar.
pub const PREPARE_ONLY_RANGE: (u64, u64) = (0, 100);
/// Build runs preparation, production and verification in three stages.
pub const BUILD_PREPARE_RANGE: (u64, u64) = (0, 15);
pub const BUILD_RANGE: (u64, u64) = (15, 65);
pub const BUILD_VERIFY_RANGE: (u64, u64) = (65, 99);
/// Standalone verification also must not show 100% before every ISO is checked.
pub const VERIFY_ONLY_RANGE: (u64, u64) = (0, 99);

/// Callbacks plus the progress-range adapter every GUI already owns.
///
/// The shared sequence asks for a stage range and then reports through the
/// ordinary [`Callbacks`] methods; shells keep their own widget updates.
pub trait GuiCallbacks: Callbacks {
    /// Maps subsequent progress reports onto `start..end` percent.
    fn set_progress_range(&mut self, start: u64, end: u64);
    /// Reborrow as the plain callback trait the pipeline expects.
    fn as_callbacks(&mut self) -> &mut dyn Callbacks;
    /// Whether the user product is already in its final location.
    ///
    /// Windows writes the ISOs straight to the user's final directory, so the
    /// default means "delivered". A SAF host returns
    /// [`ProductDelivery::PlatformPending`] and performs its export as the thin
    /// platform I/O callback, then calls [`finish_after_delivery`]; the shared
    /// finish/cleanup order is identical.
    fn product_delivery(&mut self) -> ProductDelivery {
        ProductDelivery::Delivered
    }
}

/// Where the finished product is delivered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProductDelivery {
    /// Product already sits in the user's final directory (Windows/desktop).
    #[default]
    Delivered,
    /// A platform host exports a task-local copy after the build; the shared
    /// finish/cleanup runs once that export reports its real outcome.
    PlatformPending,
}

/// Maps an actual task-file path to the user-visible product location.
///
/// Windows leaves the display override empty, so diagnostics and log paths stay
/// byte-identical there. A SAF host supplies the user's selected folder, so the
/// app-private transport path never appears in user-visible output.
/// Rewrites a pipeline error that embeds the actual task root.
///
/// Windows passes no override and receives the message unchanged.
#[must_use]
pub fn display_product_message(
    actual_root: &Path,
    display: Option<&Path>,
    message: &str,
) -> String {
    let Some(display) = display else {
        return message.to_owned();
    };
    if actual_root.as_os_str().is_empty() {
        return message.to_owned();
    }
    message.replace(
        actual_root.to_string_lossy().as_ref(),
        display.to_string_lossy().as_ref(),
    )
}

#[must_use]
pub fn display_product_path(actual_root: &Path, display: Option<&Path>, path: &Path) -> PathBuf {
    if actual_root.as_os_str().is_empty() {
        return path.to_path_buf();
    }
    display
        .and_then(|display| {
            path.strip_prefix(actual_root)
                .ok()
                .map(|relative| display.join(relative))
        })
        .unwrap_or_else(|| path.to_path_buf())
}

/// One GUI task action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuiCommand {
    /// Prepare sources in a scratch workspace, then discard it.
    Prepare,
    /// Prepare, build, verify and finish the task workspace.
    Build,
    /// Verify the finished task without rebuilding.
    Verify,
}

/// Result of one GUI task action.
#[allow(
    clippy::struct_excessive_bools,
    reason = "Succeeded, cancelled, finished and pending delivery are independent GUI states"
)]
#[derive(Clone, Debug, Default)]
pub struct GuiOutcome {
    pub succeeded: bool,
    /// True when the sequence stopped because the user requested cancellation.
    ///
    /// This is the authoritative distinction between a user stop and a real
    /// task failure; hosts must not infer it from partial/empty failure text.
    pub cancelled: bool,
    /// Pipeline-reported failure text when the sequence stopped early.
    pub failure: Option<String>,
    pub diagnostics: Vec<Value>,
    pub preparation_issues: Vec<Issue>,
    /// Number of failed source-preparation checks, when preparation ran.
    pub preparation_failures: Option<usize>,
    /// True when the task workspace was finished (successful build only).
    pub finished: bool,
    /// True when the build/verify succeeded but the platform must deliver the
    /// product (SAF export) before the shared finish/cleanup runs.
    pub pending_delivery: bool,
    /// Task-local folder holding the product for that platform export.
    pub delivery_directory: Option<std::path::PathBuf>,
}

/// Five-language GUI status text for a finished task.
#[must_use]
pub fn status_text(language: &str, succeeded: bool) -> &'static str {
    let lang = crate::log_view::Lang::from_code(language);
    if succeeded {
        crate::log_view::phrase(lang, "完成", "完成", "Done", "完了", "완료")
    } else {
        crate::log_view::phrase(lang, "失败", "失敗", "Failed", "失敗", "실패")
    }
}

/// Five-language text for a prepare run that finished with source problems.
#[must_use]
pub fn preparation_problem_text(language: &str) -> &'static str {
    crate::log_view::phrase(
        crate::log_view::Lang::from_code(language),
        "音源检查未通过，请查看诊断信息。",
        "音源檢查未通過，請查看診斷資訊。",
        "Source checks did not pass; see the diagnostics.",
        "音源チェックに合格しませんでした。診断情報を確認してください。",
        "음원 검사에 통과하지 못했습니다. 진단 정보를 확인하세요.",
    )
}

fn failed(message: String) -> GuiOutcome {
    GuiOutcome {
        failure: Some(message),
        ..GuiOutcome::default()
    }
}

/// Runs one GUI task action with the shared stage order and progress ranges.
#[must_use]
pub fn run_command(
    base: &AppOptions,
    command: GuiCommand,
    callbacks: &mut dyn GuiCallbacks,
) -> GuiOutcome {
    match command {
        GuiCommand::Prepare => run_prepare_sequence(base, callbacks),
        GuiCommand::Build => run_build_sequence(base, callbacks),
        GuiCommand::Verify => run_verify_sequence(base, callbacks),
    }
}

/// Source check: scratch workspace, prepare, discard. No task is left behind.
#[must_use]
pub fn run_prepare_sequence(base: &AppOptions, callbacks: &mut dyn GuiCallbacks) -> GuiOutcome {
    callbacks.set_progress_range(PREPARE_ONLY_RANGE.0, PREPARE_ONLY_RANGE.1);
    let scratch = match base.scratch_options() {
        Ok(scratch) => scratch,
        Err(error) => return failed(error),
    };
    let outcome = app::run_prepare(&scratch, false, callbacks.as_callbacks());
    // The scratch workspace only exists for this check.
    scratch.discard_task();
    GuiOutcome {
        succeeded: outcome.succeeded(),
        cancelled: callbacks.cancelled() && !outcome.succeeded(),
        failure: outcome.failure.map(|failure| failure.message),
        diagnostics: Vec::new(),
        preparation_issues: outcome
            .data
            .as_ref()
            .map_or_else(Vec::new, |data| data.issues.clone()),
        preparation_failures: outcome.data.as_ref().map(|data| data.failure_count),
        finished: false,
        delivery_directory: None,
        pending_delivery: false,
    }
}

/// Full task: prepare → build → verify → finish, with 0/15/65/100 staging.
#[must_use]
pub fn run_build_sequence(base: &AppOptions, callbacks: &mut dyn GuiCallbacks) -> GuiOutcome {
    let options = match base.task_options() {
        Ok(options) => options,
        Err(error) => return failed(error),
    };
    // A SAF host exports the product after verification. The transport copy is
    // task-owned, so the existing success/failure/cancel workspace lifecycle
    // (the same one Windows uses) is the only thing that deletes it.
    let platform_delivery = matches!(
        callbacks.product_delivery(),
        ProductDelivery::PlatformPending
    );
    let options = if platform_delivery {
        options.bind_task_delivery_directory()
    } else {
        options
    };
    callbacks.set_progress_range(BUILD_PREPARE_RANGE.0, BUILD_PREPARE_RANGE.1);
    let prepared = app::run_prepare(&options, false, callbacks.as_callbacks());
    let preparation_issues = prepared
        .data
        .as_ref()
        .map_or_else(Vec::new, |data| data.issues.clone());
    let preparation_failures = prepared.data.as_ref().map(|data| data.failure_count);
    if !prepared.succeeded() {
        return GuiOutcome {
            succeeded: false,
            cancelled: callbacks.cancelled(),
            failure: prepared.failure.map(|failure| failure.message),
            diagnostics: Vec::new(),
            preparation_issues,
            preparation_failures,
            finished: false,
            delivery_directory: None,
            pending_delivery: false,
        };
    }
    callbacks.set_progress_range(BUILD_RANGE.0, BUILD_RANGE.1);
    let result = match app::run_build(&options, false, callbacks.as_callbacks()) {
        Ok(result) => result,
        Err(error) => {
            return GuiOutcome {
                succeeded: false,
                cancelled: callbacks.cancelled(),
                failure: Some(error),
                diagnostics: Vec::new(),
                preparation_issues,
                preparation_failures,
                finished: false,
                delivery_directory: None,
                pending_delivery: false,
            };
        }
    };
    let mut diagnostics = result.diagnostics;
    let mut succeeded = result.succeeded;
    if succeeded {
        callbacks.set_progress_range(BUILD_VERIFY_RANGE.0, BUILD_VERIFY_RANGE.1);
        match app::run_verify(&options, callbacks.as_callbacks()) {
            Ok(verification) => {
                diagnostics.extend(verification.diagnostics);
                succeeded = verification.succeeded;
            }
            Err(error) => {
                return GuiOutcome {
                    succeeded: false,
                    cancelled: callbacks.cancelled(),
                    failure: Some(error),
                    diagnostics,
                    preparation_issues,
                    preparation_failures,
                    finished: false,
                    delivery_directory: None,
                    pending_delivery: false,
                };
            }
        }
    }
    let delivery_directory =
        (succeeded && platform_delivery).then(|| options.path("FinalDirectory"));
    if succeeded && !platform_delivery {
        // Windows delivery is inherent: 100% only after verification completed.
        callbacks.set_progress_range(99, 100);
        callbacks.progress(100, 100);
        options.finish_task();
    }
    GuiOutcome {
        succeeded,
        cancelled: callbacks.cancelled() && !succeeded,
        failure: None,
        diagnostics,
        preparation_issues,
        preparation_failures,
        finished: succeeded && !platform_delivery,
        pending_delivery: delivery_directory.is_some(),
        delivery_directory,
    }
}

/// Shared finish tail used by both hosts once the product is delivered.
///
/// Windows runs it inside [`run_build_sequence`] (delivery is inherent); a SAF
/// host calls it after its export reports the real outcome. Success adopts the
/// existing unfinished task through the normal recovery path and finishes it,
/// so the task workspace (including the exported transport copy) is removed by
/// the same lifecycle Windows uses. Failure returns without touching anything,
/// exactly like a Windows failure keeps the resumable workspace.
#[must_use]
pub fn finish_after_delivery(base: &AppOptions, delivered: bool) -> GuiOutcome {
    if !delivered {
        return GuiOutcome::default();
    }
    // A retry after the response was lost finds the workspace already gone: the
    // previous finish succeeded, so there is nothing left to clean. A leftover
    // directory with `.finished` is a failed/partial delete and must be retried.
    let task_directory = base.path("BuildDirectory").join("unfinished-task");
    if !base.has_unfinished_task() && !task_directory.exists() {
        return GuiOutcome {
            succeeded: true,
            finished: true,
            ..GuiOutcome::default()
        };
    }
    let options = match base.recover_task(false) {
        Ok(options) => options,
        Err(error) => return failed(error),
    };
    if let Err(error) = options.finish_task_checked() {
        // Keep the failure user-visible and retryable; Drop still retries later.
        return failed(error);
    }
    GuiOutcome {
        succeeded: true,
        finished: true,
        ..GuiOutcome::default()
    }
}

/// Verification of an existing task, spanning the whole progress bar.
#[must_use]
pub fn run_verify_sequence(base: &AppOptions, callbacks: &mut dyn GuiCallbacks) -> GuiOutcome {
    callbacks.set_progress_range(VERIFY_ONLY_RANGE.0, VERIFY_ONLY_RANGE.1);
    match app::run_verify(base, callbacks.as_callbacks()) {
        Ok(verification) => {
            if verification.succeeded {
                // Only the completed verification may show 100%.
                callbacks.set_progress_range(99, 100);
                callbacks.progress(100, 100);
            }
            GuiOutcome {
                succeeded: verification.succeeded,
                cancelled: callbacks.cancelled() && !verification.succeeded,
                failure: None,
                diagnostics: verification.diagnostics,
                preparation_issues: Vec::new(),
                preparation_failures: None,
                finished: false,
                delivery_directory: None,
                pending_delivery: false,
            }
        }
        Err(error) => GuiOutcome {
            cancelled: callbacks.cancelled(),
            failure: Some(error),
            ..GuiOutcome::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Default)]
    struct Recorder {
        ranges: Vec<(u64, u64)>,
    }

    impl Callbacks for Recorder {
        fn emit(&mut self, _stream: i32, _text: &str) {}
        fn cancelled(&mut self) -> bool {
            false
        }
    }

    impl GuiCallbacks for Recorder {
        fn set_progress_range(&mut self, start: u64, end: u64) {
            self.ranges.push((start, end));
        }
        fn as_callbacks(&mut self) -> &mut dyn Callbacks {
            self
        }
    }

    fn empty_profile(label: &str) -> (AppOptions, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "dvda-gui-sequence-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let profile = root.join("profile.json");
        std::fs::write(
            &profile,
            serde_json::to_vec(&json!({
                "Version": 1,
                "Language": "en",
                "Values": {
                    "DVDA_SRC": root.join("missing-source"),
                    "DVDA_FINAL_DIR": root.join("final"),
                    "DVDA_BUILD_DIR": root.join("build"),
                    "DVDA_TITLE": "Sequence",
                }
            }))
            .unwrap(),
        )
        .unwrap();
        (AppOptions::load(Some(&profile)).unwrap(), root)
    }

    #[test]
    fn build_sequence_reports_the_staged_ranges_before_failing_early() {
        let (options, root) = empty_profile("build");
        let mut callbacks = Recorder::default();
        let outcome = run_command(&options, GuiCommand::Build, &mut callbacks);
        assert!(!outcome.finished);
        assert_eq!(callbacks.ranges.first(), Some(&BUILD_PREPARE_RANGE));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn prepare_sequence_uses_the_whole_bar_and_discards_the_scratch_workspace() {
        let (options, root) = empty_profile("prepare");
        let mut callbacks = Recorder::default();
        let outcome = run_command(&options, GuiCommand::Prepare, &mut callbacks);
        assert!(!outcome.finished);
        assert_eq!(callbacks.ranges, vec![PREPARE_ONLY_RANGE]);
        assert!(!root.join("build/unfinished-task").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn task_delivery_is_deleted_by_the_shared_finish_lifecycle() {
        let (base, root) = empty_profile("delivery");
        let task = base.task_options().unwrap();
        let bound = task.bind_task_delivery_directory();
        let delivery = bound.path("FinalDirectory");
        assert!(
            delivery.starts_with(bound.path("BuildDirectory")),
            "{} must live inside the task workspace",
            delivery.display()
        );
        std::fs::create_dir_all(&delivery).unwrap();
        std::fs::write(delivery.join("disc_1.iso"), b"product").unwrap();
        drop(bound);
        drop(task);
        assert!(base.has_unfinished_task());
        let outcome = finish_after_delivery(&base, true);
        assert!(outcome.finished, "{outcome:?}");
        assert!(!base.has_unfinished_task());
        assert!(
            !base.path("BuildDirectory").join("unfinished-task").exists(),
            "finish must remove the whole task directory, not just deliver"
        );
        assert!(
            !delivery.exists(),
            "task workspace must own the transport copy"
        );
        // A retry after a lost response is idempotent: no workspace means the
        // previous finish already completed.
        let retry = finish_after_delivery(&base, true);
        assert!(retry.succeeded && retry.finished, "{retry:?}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_platform_delivery_keeps_the_workspace_like_windows() {
        let (base, root) = empty_profile("delivery-failed");
        let task = base.task_options().unwrap();
        let bound = task.bind_task_delivery_directory();
        let delivery = bound.path("FinalDirectory");
        std::fs::create_dir_all(&delivery).unwrap();
        drop(bound);
        drop(task);
        let outcome = finish_after_delivery(&base, false);
        assert!(!outcome.finished);
        assert!(
            base.has_unfinished_task(),
            "failed export must stay resumable"
        );
        assert!(delivery.exists());
        assert!(base.discard_progress().unwrap());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn failed_task_removal_is_reported_and_retryable() {
        use crate::platform::OpenOptionsExt;
        let (base, root) = empty_profile("delivery-locked");
        let task = base.task_options().unwrap();
        let bound = task.bind_task_delivery_directory();
        let delivery = bound.path("FinalDirectory");
        std::fs::create_dir_all(&delivery).unwrap();
        let locked = delivery.join("disc_1.iso");
        std::fs::write(&locked, b"product").unwrap();
        // Windows file-sharing semantics: an open file without delete sharing
        // makes the shared Workspace Drop fail, which must be reported.
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked)
            .unwrap();
        drop(bound);
        drop(task);
        let task_directory = base.path("BuildDirectory").join("unfinished-task");
        let failed = finish_after_delivery(&base, true);
        assert!(!failed.succeeded, "{failed:?}");
        assert!(task_directory.join(".finished").exists());
        // A second consecutive failure must keep the same retryable marker.
        let failed_again = finish_after_delivery(&base, true);
        assert!(!failed_again.succeeded, "{failed_again:?}");
        assert!(task_directory.join(".finished").exists());
        drop(lock);
        let retry = finish_after_delivery(&base, true);
        assert!(retry.succeeded && retry.finished, "{retry:?}");
        assert!(!base.path("BuildDirectory").join("unfinished-task").exists());
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn user_visible_paths_replace_the_task_root_only_when_requested() {
        let actual = Path::new("C:/private/build/unfinished-task/deliver");
        let path = actual.join("disc_1.iso");
        assert_eq!(
            display_product_path(actual, Some(Path::new("D:/Music/Out")), &path),
            Path::new("D:/Music/Out/disc_1.iso")
        );
        // Windows supplies no display override; its output paths stay
        // byte-identical.
        assert_eq!(display_product_path(actual, None, &path), path);
    }

    #[test]
    fn empty_actual_root_never_rewrites_messages_or_paths() {
        let display = Path::new("/sdcard/Out");
        let message = "SOURCE_MISSING: /data/user/0/x/saf-mirror/key/a.flac";
        assert_eq!(
            display_product_message(Path::new(""), Some(display), message),
            message
        );
        let path = Path::new("/data/user/0/x/saf-mirror/key/a.flac");
        assert_eq!(
            display_product_path(Path::new(""), Some(display), path),
            path
        );
    }
    #[test]
    fn verify_sequence_uses_the_whole_bar() {
        let (options, root) = empty_profile("verify");
        let mut callbacks = Recorder::default();
        let _outcome = run_command(&options, GuiCommand::Verify, &mut callbacks);
        assert_eq!(callbacks.ranges, vec![VERIFY_ONLY_RANGE]);
        let _ = std::fs::remove_dir_all(root);
    }
}
