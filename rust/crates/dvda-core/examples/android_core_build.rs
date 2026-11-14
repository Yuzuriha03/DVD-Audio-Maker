use dvda_core::app::{AppOptions, run_build};
use dvda_native::media::Callbacks;
use serde_json::json;

struct Console;
impl Callbacks for Console {
    fn emit(&mut self, stream: i32, text: &str) {
        println!("[{stream}] {text}");
    }
    fn cancelled(&mut self) -> bool {
        false
    }
}

fn main() {
    let profile_path = std::env::args().nth(1).map_or_else(
        || {
            let data = std::path::Path::new("/tmp/coretest");
            std::fs::create_dir_all(data).expect("data dir");
            let profile = json!({
                "Version": 1,
                "Language": "zh-CN",
                "Values": {
                    "DVDA_SRC": "/tmp/music_one",
                    "DVDA_FINAL_DIR": "/tmp/coretest/final",
                    "DVDA_BUILD_DIR": "/tmp/coretest/build",
                    "DVDA_TITLE": "AndroidCore",
                    "DVDA_MENU": "on",
                    "DVDA_MENU_FONT": "/system/fonts/NotoSansCJK-Regular.ttc",
                    "DVDA_MLP_SOURCE": "mlp-batch",
                    "DVDA_MENU_TRACKS_PER_PAGE": "12",
                    "DVDA_MENU_INDEX_MIN_ALBUMS": "4",
                    "DVDA_MLP_SAMPLE_RATE": "48000",
                    "DVDA_MLP_BITS": "24"
                }
            });
            let path = data.join("profile.json");
            std::fs::write(&path, serde_json::to_vec_pretty(&profile).unwrap()).expect("profile");
            path
        },
        std::path::PathBuf::from,
    );
    let options = AppOptions::load(Some(&profile_path)).expect("load options");
    println!(
        "source={:?} final={:?} build={:?} menu={:?} mlp={:?}",
        options.values.get("SourceDirectory"),
        options.values.get("FinalDirectory"),
        options.values.get("BuildDirectory"),
        options.values.get("MenuEnabled"),
        options.values.get("MlpSource"),
    );
    match run_build(&options, false, &mut Console) {
        Ok(outcome) => {
            println!(
                "BUILD ok={} diagnostics={}",
                outcome.succeeded,
                outcome.diagnostics.len()
            );
            for diagnostic in &outcome.diagnostics {
                println!("DIAG {diagnostic}");
            }
        }
        Err(error) => println!("BUILD ERROR: {error}"),
    }
}
