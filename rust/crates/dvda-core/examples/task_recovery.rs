use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use dvda_core::app::{AppOptions, run_build};
use dvda_native::media::Callbacks;

struct CancelAfterDisc {
    count: AtomicUsize,
    final_dir: PathBuf,
    cancel: bool,
}

impl Callbacks for CancelAfterDisc {
    fn emit(&mut self, _stream: i32, _text: &str) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }
    fn cancelled(&mut self) -> bool {
        self.cancel
            && std::fs::read_dir(&self.final_dir).is_ok_and(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|entry| entry.file_name().to_string_lossy().ends_with(".iso"))
            })
    }
}

fn list(directory: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory).map_or_default(|entries| {
        entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect()
    });
    names.sort();
    names
}

fn main() {
    let profile = std::env::args()
        .nth(1)
        .expect("usage: task_recovery <profile.json>");
    let base = AppOptions::load(Some(std::path::Path::new(&profile))).expect("profile");
    let final_text = base.path("FinalDirectory").to_string_lossy().into_owned();
    println!("unfinished before: {}", base.has_unfinished_task());

    let mut cancelling = CancelAfterDisc {
        count: AtomicUsize::new(0),
        final_dir: PathBuf::from(&final_text),
        cancel: true,
    };
    match run_build(&base, false, &mut cancelling) {
        Ok(outcome) => println!(
            "cancelled run succeeded={} diagnostics={}",
            outcome.succeeded,
            outcome.diagnostics.len()
        ),
        Err(error) => println!("cancelled run error={error}"),
    }
    println!("final after cancel: {:?}", list(&final_text));

    let after = AppOptions::load(Some(std::path::Path::new(&profile))).expect("profile");
    println!("unfinished after cancel: {}", after.has_unfinished_task());

    let continued = after.recover_task(false).expect("recover task");
    println!(
        "recovered(continue), unfinished={}",
        continued.has_unfinished_task()
    );
    let mut normal = CancelAfterDisc {
        count: AtomicUsize::new(0),
        final_dir: PathBuf::from(&final_text),
        cancel: false,
    };
    match run_build(&continued, false, &mut normal) {
        Ok(outcome) => println!(
            "continue run succeeded={} diagnostics={}",
            outcome.succeeded,
            outcome.diagnostics.len()
        ),
        Err(error) => println!("continue run error={error}"),
    }
    println!("final after continue: {:?}", list(&final_text));

    let again = AppOptions::load(Some(std::path::Path::new(&profile))).expect("profile");
    println!("unfinished after continue: {}", again.has_unfinished_task());
}
