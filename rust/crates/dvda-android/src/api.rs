//! flutter_rust_bridge bindings over the real dvda-core application entrypoints.
//!
//! No orchestration logic lives here: Flutter passes a desktop-compatible
//! profile JSON and the binding calls AppOptions/run_prepare/run_build/
//! run_verify, forwarding progress and cancellation.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use dvda_core::app::{self, AppOptions};
use dvda_native::media::Callbacks;
use flutter_rust_bridge::frb;

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
static LANGUAGE: Mutex<String> = Mutex::new(String::new());
static CANCEL: AtomicBool = AtomicBool::new(false);
static DONE: AtomicU64 = AtomicU64::new(0);
static TOTAL: AtomicU64 = AtomicU64::new(PROGRESS_TOTAL);
static TEXT: Mutex<String> = Mutex::new(String::new());

/// Progress is a single percentage scale on the Flutter side.
const PROGRESS_TOTAL: u64 = 100;

pub struct ProgressSnapshot {
    pub done: u64,
    pub total: u64,
    pub text: String,
    pub log: String,
}

fn push_log(text: &str) {
    let language = LANGUAGE.lock().map(|slot| slot.clone()).unwrap_or_default();
    let translated = if language.is_empty() {
        text.to_owned()
    } else {
        dvda_core::localization::translate(&language, text)
    };
    if let Ok(mut log) = LOG.lock() {
        log.push(translated);
        if log.len() > 300 {
            let drain = log.len() - 300;
            log.drain(..drain);
        }
    }
}

fn set_text(text: &str) {
    if let Ok(mut slot) = TEXT.lock() {
        *slot = text.chars().take(80).collect();
    }
}

struct Forward {
    start: u64,
    end: u64,
    last: u64,
}

impl Forward {
    const fn stage(start: u64, end: u64) -> Self {
        Self {
            start,
            end,
            last: start,
        }
    }
}

impl Callbacks for Forward {
    fn emit(&mut self, _stream: i32, text: &str) {
        let trimmed = text.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("[METRIC]") {
            set_text(trimmed);
            if !trimmed.starts_with("[TASK]") {
                push_log(trimmed);
            }
        }
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

fn outcome_with_log(
    ok: bool,
    message: String,
    diagnostics: serde_json::Value,
    log: &str,
) -> String {
    serde_json::json!({
        "Ok": ok,
        "Message": message,
        "Diagnostics": diagnostics,
        "Log": log,
        "ProgressDone": DONE.load(Ordering::Relaxed),
        "ProgressTotal": TOTAL.load(Ordering::Relaxed),
    })
    .to_string()
}

#[frb]
pub fn run_profile(profile_json: String, action: String) -> String {
    CANCEL.store(false, Ordering::Relaxed);
    DONE.store(0, Ordering::Relaxed);
    TOTAL.store(PROGRESS_TOTAL, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "dvda-profile-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = (|| -> Result<(bool, String, serde_json::Value), String> {
        std::fs::write(&path, profile_json).map_err(|error| error.to_string())?;
        let options = AppOptions::load(Some(&path))?;
        if let Ok(mut language) = LANGUAGE.lock() {
            *language = options.language.clone();
        }
        if let Ok(mut log) = LOG.lock() {
            log.clear();
        }
        match action.as_str() {
            "status" => {
                let value = serde_json::json!({
                    "Ok": true,
                    "Unfinished": options.has_unfinished_task(),
                });
                Ok((true, value.to_string(), serde_json::Value::Null))
            }
            "discard" => {
                options.discard_task();
                Ok((true, "discarded".to_string(), serde_json::Value::Null))
            }
            "prepare" => {
                // Desktop checks sources in a scratch workspace so no unfinished
                // task is left behind.
                let mut forward = Forward::stage(0, PROGRESS_TOTAL);
                let scratch = options.scratch_options()?;
                let outcome = app::run_prepare(&scratch, true, &mut forward);
                scratch.discard_task();
                let ok = outcome.failure.is_none() && outcome.exit_code == Some(0);
                if ok {
                    DONE.store(PROGRESS_TOTAL, Ordering::Relaxed);
                }
                let message = outcome
                    .failure
                    .map_or_else(|| "prepare ok".to_string(), |failure| failure.message);
                Ok((ok, message, serde_json::Value::Null))
            }
            "verify" => {
                let mut forward = Forward::stage(0, PROGRESS_TOTAL);
                let outcome = app::run_verify(&options, &mut forward)?;
                if outcome.succeeded {
                    DONE.store(PROGRESS_TOTAL, Ordering::Relaxed);
                }
                let diagnostics =
                    serde_json::to_value(&outcome.diagnostics).unwrap_or(serde_json::Value::Null);
                Ok((outcome.succeeded, "verify".into(), diagnostics))
            }
            _ => {
                // Desktop build lifecycle: prepare -> build -> verify -> finish.
                let options = match action.as_str() {
                    "build_continue" => options.recover_task(false)?,
                    "build_restart" => options.recover_task(true)?,
                    _ => options.task_options()?,
                };
                let mut prepare_forward = Forward::stage(0, 15);
                let prepared = app::run_prepare(&options, false, &mut prepare_forward);
                if prepared.failure.is_some() || prepared.exit_code != Some(0) {
                    let message = prepared
                        .failure
                        .map_or_else(|| "prepare failed".to_string(), |failure| failure.message);
                    return Ok((false, message, serde_json::Value::Null));
                }
                let mut build_forward = Forward::stage(15, 65);
                let result = app::run_build(&options, false, &mut build_forward)?;
                let mut diagnostics = result.diagnostics.clone();
                let mut ok = result.succeeded;
                if ok {
                    let mut verify_forward = Forward::stage(65, PROGRESS_TOTAL);
                    match app::run_verify(&options, &mut verify_forward) {
                        Ok(verification) => {
                            diagnostics.extend(verification.diagnostics.clone());
                            ok = verification.succeeded;
                        }
                        Err(error) => {
                            return Ok((false, error, serde_json::Value::Null));
                        }
                    }
                }
                if ok {
                    options.finish_task();
                    DONE.store(PROGRESS_TOTAL, Ordering::Relaxed);
                }
                let diagnostics =
                    serde_json::to_value(&diagnostics).unwrap_or(serde_json::Value::Null);
                Ok((ok, "build".into(), diagnostics))
            }
        }
    })();
    let _ = std::fs::remove_file(&path);
    let tail = LOG.lock().map(|log| log.join("\n")).unwrap_or_default();
    match result {
        Ok((ok, message, diagnostics)) => outcome_with_log(ok, message, diagnostics, &tail),
        Err(error) => outcome_with_log(false, error, serde_json::Value::Null, &tail),
    }
}

#[frb(sync)]
pub fn cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}

#[frb(sync)]
pub fn progress_snapshot() -> ProgressSnapshot {
    ProgressSnapshot {
        done: DONE.load(Ordering::Relaxed),
        total: TOTAL.load(Ordering::Relaxed),
        text: TEXT.lock().map(|slot| slot.clone()).unwrap_or_default(),
        log: LOG.lock().map(|log| log.join("\n")).unwrap_or_default(),
    }
}
