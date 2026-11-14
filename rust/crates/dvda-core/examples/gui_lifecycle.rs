use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use dvda_core::app::{self, AppOptions};
use dvda_native::media::Callbacks;
use serde_json::json;

struct Counting {
    count: AtomicUsize,
    final_dir: PathBuf,
    cancel_after_iso: bool,
}

impl Callbacks for Counting {
    fn emit(&mut self, _stream: i32, _text: &str) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }
    fn cancelled(&mut self) -> bool {
        self.cancel_after_iso
            && std::fs::read_dir(&self.final_dir).is_ok_and(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|entry| entry.file_name().to_string_lossy().ends_with(".iso"))
            })
    }
}

fn lifecycle(
    options: &AppOptions,
    callbacks: &mut dyn Callbacks,
    finish: bool,
) -> Result<bool, String> {
    let prepared = app::run_prepare(options, false, callbacks);
    if prepared.failure.is_some() || prepared.exit_code != Some(0) {
        return Err("prepare failed".into());
    }
    let result = app::run_build(options, false, callbacks)?;
    let mut ok = result.succeeded;
    if ok {
        ok = app::run_verify(options, callbacks)?.succeeded;
    }
    if ok && finish {
        options.finish_task();
    }
    Ok(ok)
}

fn with_paths(base: &AppOptions, build: &str, final_dir: &str) -> AppOptions {
    let mut edits = serde_json::Map::new();
    edits.insert("DVDA_BUILD_DIR".into(), json!(build));
    edits.insert("DVDA_FINAL_DIR".into(), json!(final_dir));
    base.with_profile_values(edits).expect("options")
}

#[allow(
    clippy::too_many_lines,
    reason = "Lifecycle example keeps its end-to-end phases sequential"
)]
fn main() {
    let profile = std::env::args()
        .nth(1)
        .expect("usage: gui_lifecycle <profile.json>");
    let base = AppOptions::load(Some(std::path::Path::new(&profile))).expect("profile");
    let _ = std::fs::remove_dir_all("/tmp/lifecycle-phase1");
    let _ = std::fs::remove_dir_all("/tmp/lifecycle-phase2");
    let _ = std::fs::remove_dir_all("/tmp/lifecycle-phase3");

    let phase1 = with_paths(
        &base,
        "/tmp/lifecycle-phase1/build",
        "/tmp/lifecycle-phase1/final",
    );
    let mut normal = Counting {
        count: AtomicUsize::new(0),
        final_dir: PathBuf::from("/tmp/lifecycle-phase1/final"),
        cancel_after_iso: false,
    };
    println!(
        "fresh lifecycle ok={:?}",
        lifecycle(&phase1.task_options().expect("task"), &mut normal, true)
    );
    let after1 = with_paths(
        &base,
        "/tmp/lifecycle-phase1/build",
        "/tmp/lifecycle-phase1/final",
    );
    println!("unfinished after fresh: {}", after1.has_unfinished_task());

    let phase2 = with_paths(
        &base,
        "/tmp/lifecycle-phase2/build",
        "/tmp/lifecycle-phase2/final",
    );
    let scratch = phase2.scratch_options().expect("scratch");
    let mut quiet = Counting {
        count: AtomicUsize::new(0),
        final_dir: PathBuf::from("/tmp/lifecycle-phase2/final"),
        cancel_after_iso: false,
    };
    let prepared = app::run_prepare(&scratch, true, &mut quiet);
    println!(
        "scratch prepare failure={}",
        prepared.failure.map_or_default(|failure| failure.message)
    );
    scratch.discard_task();
    let after2 = with_paths(
        &base,
        "/tmp/lifecycle-phase2/build",
        "/tmp/lifecycle-phase2/final",
    );
    println!(
        "unfinished after scratch check: {}",
        after2.has_unfinished_task()
    );

    let phase3 = with_paths(
        &base,
        "/tmp/lifecycle-phase3/build",
        "/tmp/lifecycle-phase3/final",
    );
    let mut stopping = Counting {
        count: AtomicUsize::new(0),
        final_dir: PathBuf::from("/tmp/lifecycle-phase3/final"),
        cancel_after_iso: true,
    };
    let _ = lifecycle(&phase3.task_options().expect("task3"), &mut stopping, false);
    let after3 = with_paths(
        &base,
        "/tmp/lifecycle-phase3/build",
        "/tmp/lifecycle-phase3/final",
    );
    println!("unfinished after cancel: {}", after3.has_unfinished_task());

    let resumed = after3.recover_task(false).expect("recover");
    let mut finishing = Counting {
        count: AtomicUsize::new(0),
        final_dir: PathBuf::from("/tmp/lifecycle-phase3/final"),
        cancel_after_iso: false,
    };
    println!(
        "continue lifecycle ok={:?}",
        lifecycle(&resumed, &mut finishing, true)
    );
    let done = with_paths(
        &base,
        "/tmp/lifecycle-phase3/build",
        "/tmp/lifecycle-phase3/final",
    );
    println!(
        "unfinished after finished continue: {}",
        done.has_unfinished_task()
    );
    println!(
        "phase3 final: {:?}",
        std::fs::read_dir("/tmp/lifecycle-phase3/final").map_or_default(|entries| entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>())
    );
}
