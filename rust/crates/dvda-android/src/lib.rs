//! Thin JNI binding over the real dvda-core application entrypoints.
//!
//! No orchestration logic lives here: Kotlin passes a desktop-compatible
//! profile JSON and the binding calls AppOptions/run_prepare/run_build/
//! run_verify, forwarding progress and cancellation.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
static LANGUAGE: Mutex<String> = Mutex::new(String::new());

fn push_log(text: &str) {
    let language = LANGUAGE
        .lock()
        .map(|slot| slot.clone())
        .unwrap_or_else(|_| String::new());
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

use dvda_core::app::{self, AppOptions};
use dvda_native::media::Callbacks;
use jni::objects::{JClass, JString};
use jni::sys::{jint, jstring};
use jni::JNIEnv;

static CANCEL: AtomicBool = AtomicBool::new(false);
static DONE: AtomicU64 = AtomicU64::new(0);
static TOTAL: AtomicU64 = AtomicU64::new(0);
static TEXT: Mutex<String> = Mutex::new(String::new());

fn set_text(text: &str) {
    if let Ok(mut slot) = TEXT.lock() {
        *slot = text.chars().take(80).collect();
    }
}

struct Forward;
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
        DONE.store(completed, Ordering::Relaxed);
        TOTAL.store(total.max(1), Ordering::Relaxed);
    }
}

fn text(env: &mut JNIEnv, value: &JString) -> String {
    env.get_string(value)
        .map(|value| value.into())
        .unwrap_or_default()
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

#[no_mangle]
pub extern "system" fn Java_com_dvda_maker_NativeEngine_runProfile(
    mut env: JNIEnv,
    _class: JClass,
    profile_json: JString,
    action: JString,
) -> jstring {
    CANCEL.store(false, Ordering::Relaxed);
    DONE.store(0, Ordering::Relaxed);
    TOTAL.store(1, Ordering::Relaxed);
    let profile = text(&mut env, &profile_json);
    let action = text(&mut env, &action);
    let path = std::env::temp_dir().join(format!(
        "dvda-profile-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = (|| -> Result<(bool, String, serde_json::Value), String> {
        std::fs::write(&path, profile).map_err(|error| error.to_string())?;
        let options = AppOptions::load(Some(&path))?;
        if let Ok(mut language) = LANGUAGE.lock() {
            *language = options.language.clone();
        }
        if let Ok(mut log) = LOG.lock() {
            log.clear();
        }
        let mut forward = Forward;
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
                let scratch = options.scratch_options()?;
                let outcome = app::run_prepare(&scratch, true, &mut forward);
                scratch.discard_task();
                let ok = outcome.failure.is_none() && outcome.exit_code == Some(0);
                let message = outcome
                    .failure
                    .map_or_else(|| "prepare ok".to_string(), |failure| failure.message);
                Ok((ok, message, serde_json::Value::Null))
            }
            "verify" => {
                let outcome = app::run_verify(&options, &mut forward)?;
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
                let prepared = app::run_prepare(&options, false, &mut forward);
                if prepared.failure.is_some() || prepared.exit_code != Some(0) {
                    let message = prepared
                        .failure
                        .map_or_else(|| "prepare failed".to_string(), |failure| failure.message);
                    return Ok((false, message, serde_json::Value::Null));
                }
                let result = app::run_build(&options, false, &mut forward)?;
                let mut diagnostics = result.diagnostics.clone();
                let mut ok = result.succeeded;
                if ok {
                    match app::run_verify(&options, &mut forward) {
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
                }
                let diagnostics =
                    serde_json::to_value(&diagnostics).unwrap_or(serde_json::Value::Null);
                Ok((ok, "build".into(), diagnostics))
            }
        }
    })();
    let _ = std::fs::remove_file(&path);
    let tail = LOG.lock().map(|log| log.join("\n")).unwrap_or_default();
    let json = match result {
        Ok((ok, message, diagnostics)) => outcome_with_log(ok, message, diagnostics, &tail),
        Err(error) => outcome_with_log(false, error, serde_json::Value::Null, &tail),
    };
    env.new_string(json)
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_com_dvda_maker_NativeEngine_logText(env: JNIEnv, _: JClass) -> jstring {
    let text = LOG.lock().map(|log| log.join("\n")).unwrap_or_default();
    env.new_string(text)
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_com_dvda_maker_NativeEngine_cancel(_: JNIEnv, _: JClass) {
    CANCEL.store(true, Ordering::Relaxed);
}

#[no_mangle]
pub extern "system" fn Java_com_dvda_maker_NativeEngine_progressDone(_: JNIEnv, _: JClass) -> jint {
    jint::try_from(DONE.load(Ordering::Relaxed)).unwrap_or(jint::MAX)
}

#[no_mangle]
pub extern "system" fn Java_com_dvda_maker_NativeEngine_progressTotal(
    _: JNIEnv,
    _: JClass,
) -> jint {
    jint::try_from(TOTAL.load(Ordering::Relaxed)).unwrap_or(jint::MAX)
}

#[no_mangle]
pub extern "system" fn Java_com_dvda_maker_NativeEngine_progressText(
    env: JNIEnv,
    _: JClass,
) -> jstring {
    let value = TEXT.lock().map(|slot| slot.clone()).unwrap_or_default();
    env.new_string(value)
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
