//! Device-side probe for the shared profile/log APIs the FRB layer exposes.

use serde_json::json;

struct Probe;

impl dvda_core::gui::GuiCallbacks for Probe {
    fn set_progress_range(&mut self, _start: u64, _end: u64) {}

    fn as_callbacks(&mut self) -> &mut dyn dvda_native::media::Callbacks {
        self
    }
}

impl dvda_native::media::Callbacks for Probe {
    fn emit(&mut self, _stream: i32, _text: &str) {}

    fn cancelled(&mut self) -> bool {
        false
    }
}

fn main() {
    println!("probe=start");
    let options = dvda_core::profile::gui_options();
    println!("options_capacity_dvd5={}", options["capacity"]["dvd5"]);
    println!("options_defaults_title={}", options["defaults"]["DVDA_TITLE"]);
    println!("disc_bytes_dvd5={}", dvda_core::profile::disc_bytes(0, None));
    println!("disc_bytes_dvd9={}", dvda_core::profile::disc_bytes(1, None));
    println!("disc_bytes_custom={}", dvda_core::profile::disc_bytes(2, Some(1234)));
    let values = json!({
        "DVDA_MLP_SOURCE": "lpcm",
        "DVDA_MLP_SAMPLE_RATE": "48000",
        "DVDA_MLP_BITS": "20"
    });
    let issues = dvda_core::profile::validate_values(values.as_object().unwrap(), true);
    println!("lpcm20_issue_count={}", issues.len());
    for issue in &issues {
        println!("lpcm20_issue={}:{}", issue.key, issue.code);
    }

    let log = [
        "out_time_us=1234",
        "[METRIC] speed=9",
        "[TASK] {\"Action\":\"author_iso\",\"Current\":2,\"Total\":5,\"Name\":\"Track A\",\"Detail\":\"\"}",
        "[TASK] {\"Action\":\"brand_new_stage\",\"Current\":1,\"Total\":2,\"Name\":\"t.flac\",\"Detail\":\"\"}",
        "[error] decoder failed",
        "\u{6210}\u{54c1}\u{ff1a}/sdcard/out",
    ]
    .join("\n");
    let formatted = dvda_core::log_view::format_log("zh-CN", &log);
    println!("log_text=<<<{}>>>", formatted.text);
    println!("log_problems={}", formatted.problems);
    println!("log_problem_text=<<<{}>>>", formatted.problem_text);
    let digest = dvda_core::hash::hex_digest(formatted.text.as_bytes());
    println!("log_text_sha256={digest}");

    let root = std::env::temp_dir().join(format!("dvda-probe-{}", std::process::id()));
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
                "DVDA_TITLE": "Probe"
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let parsed = dvda_core::app::AppOptions::load(Some(&profile)).unwrap();
    let mut callbacks = Probe;
    let outcome = dvda_core::gui::run_command(
        &parsed,
        dvda_core::gui::GuiCommand::Prepare,
        &mut callbacks,
    );
    println!("prepare_succeeded={}", outcome.succeeded);
    println!("prepare_finished={}", outcome.finished);
    println!("prepare_failure_present={}", outcome.failure.is_some());
    let _ = std::fs::remove_dir_all(&root);
    println!("probe=ok");
}
