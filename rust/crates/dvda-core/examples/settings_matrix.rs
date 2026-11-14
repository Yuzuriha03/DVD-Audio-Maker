use dvda_core::app::{self, AppOptions};
use dvda_core::verify::Mode;
use dvda_native::media::Callbacks;
use serde_json::{Map, Value, json};

struct Quiet;
impl Callbacks for Quiet {
    fn emit(&mut self, _stream: i32, _text: &str) {}
    fn cancelled(&mut self) -> bool {
        false
    }
}

fn edits(pairs: &[(&str, &str)]) -> Map<String, Value> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), json!(value)))
        .collect()
}

fn codes(diagnostics: &[Value]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|item| {
            format!(
                "{}/{}",
                item["Severity"].as_i64().unwrap_or(0),
                item["Code"].as_str().unwrap_or("")
            )
        })
        .collect()
}

fn build(
    base: &AppOptions,
    name: &str,
    mut overrides: Map<String, Value>,
) -> (bool, Vec<String>, u64) {
    let build_dir = format!("/tmp/matrix/{name}/build");
    let final_dir = format!("/tmp/matrix/{name}/final");
    overrides.insert("DVDA_BUILD_DIR".into(), json!(build_dir));
    overrides.insert("DVDA_FINAL_DIR".into(), json!(final_dir));
    overrides.insert(
        "DVDA_MENU_FONT".into(),
        json!("/system/fonts/NotoSansCJK-Regular.ttc"),
    );
    let options = base.with_profile_values(overrides).expect("options");
    println!(
        "  resolved {name} tracks={:?} index={:?} title_mode={:?}",
        options.value("MenuTracksPerPage"),
        options.value("MenuIndexMinimumAlbums"),
        options.value("DiagnosticTitleMode")
    );
    let task = options.task_options().expect("task");
    let mut quiet = Quiet;
    let prepared = app::run_prepare(&task, false, &mut quiet);
    if prepared.failure.is_some() || prepared.exit_code != Some(0) {
        return (false, vec!["prepare".into()], 0);
    }
    let result = app::run_build(&task, false, &mut quiet).expect("build");
    let mut issues = codes(&result.diagnostics);
    let mut ok = result.succeeded;
    if ok {
        let verification = app::run_verify(&task, &mut quiet).expect("verify");
        issues.extend(codes(&verification.diagnostics));
        ok = verification.succeeded;
        if name == "baseline" {
            for (label, mode) in [
                ("capacity", Mode::Capacity),
                ("timeline", Mode::Timeline),
                ("menu", Mode::Menu),
                ("lossless", Mode::Lossless),
            ] {
                match app::run_verify_mode(&task, mode, None, &mut quiet) {
                    Ok(outcome) => println!(
                        "  mode {label}: ok={} issues={:?}",
                        outcome.succeeded,
                        codes(&outcome.diagnostics)
                    ),
                    Err(error) => println!("  mode {label}: error={error}"),
                }
            }
        }
    }
    if let Some(log) = task.value("BuildLogPath").as_str() {
        let _ = std::fs::copy(log, format!("/tmp/matrix/{name}/build.log"));
    }
    task.finish_task();
    let iso = std::fs::read_dir(&final_dir).map_or(0, |entries| {
        entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".iso"))
            .map(|entry| entry.metadata().map_or(0, |meta| meta.len()))
            .sum()
    });
    (ok, issues, iso)
}

fn main() {
    let profile = std::env::args()
        .nth(1)
        .expect("usage: settings_matrix <profile.json>");
    let base = AppOptions::load(Some(std::path::Path::new(&profile))).expect("profile");
    let _ = std::fs::remove_dir_all("/tmp/matrix");
    let variants: Vec<(&str, Map<String, Value>)> = vec![
        ("baseline", edits(&[])),
        ("title_one", edits(&[("DVDA_TITLE_MODE", "one")])),
        ("title_two", edits(&[("DVDA_TITLE_MODE", "2")])),
        ("tracks6", edits(&[("DVDA_MENU_TRACKS_PER_PAGE", "6")])),
        ("index9", edits(&[("DVDA_MENU_INDEX_MIN_ALBUMS", "9")])),
        ("stills_off", edits(&[("DVDA_MENU_STILLPICS", "off")])),
        ("ntsc", edits(&[("DVDA_MENU_NORM", "ntsc")])),
        ("album_limit1", edits(&[("DVDA_ALBUM_LIMIT", "1")])),
        ("dvd9", edits(&[("DVDA_DISC_BYTES", "8540123136")])),
        (
            "long_tracks12",
            edits(&[
                ("DVDA_SRC", "/tmp/matrix_lib"),
                ("DVDA_MENU_TRACKS_PER_PAGE", "12"),
            ]),
        ),
        (
            "long_tracks6",
            edits(&[
                ("DVDA_SRC", "/tmp/matrix_lib"),
                ("DVDA_MENU_TRACKS_PER_PAGE", "6"),
            ]),
        ),
        (
            "long_title_one",
            edits(&[("DVDA_SRC", "/tmp/matrix_lib"), ("DVDA_TITLE_MODE", "one")]),
        ),
        (
            "long_title_6",
            edits(&[("DVDA_SRC", "/tmp/matrix_lib"), ("DVDA_TITLE_MODE", "6")]),
        ),
        (
            "five_index4",
            edits(&[
                ("DVDA_SRC", "/tmp/matrix_lib2"),
                ("DVDA_MENU_INDEX_MIN_ALBUMS", "4"),
            ]),
        ),
        (
            "five_index9",
            edits(&[
                ("DVDA_SRC", "/tmp/matrix_lib2"),
                ("DVDA_MENU_INDEX_MIN_ALBUMS", "9"),
            ]),
        ),
        (
            "five_t2_index4",
            edits(&[
                ("DVDA_SRC", "/tmp/matrix_lib2"),
                ("DVDA_MENU_TRACKS_PER_PAGE", "2"),
                ("DVDA_MENU_INDEX_MIN_ALBUMS", "4"),
            ]),
        ),
        (
            "five_t2_index9",
            edits(&[
                ("DVDA_SRC", "/tmp/matrix_lib2"),
                ("DVDA_MENU_TRACKS_PER_PAGE", "2"),
                ("DVDA_MENU_INDEX_MIN_ALBUMS", "9"),
            ]),
        ),
    ];
    for (name, overrides) in variants {
        let (ok, issues, iso) = build(&base, name, overrides);
        println!("{name}: ok={ok} iso_bytes={iso} issues={issues:?}");
    }
}
