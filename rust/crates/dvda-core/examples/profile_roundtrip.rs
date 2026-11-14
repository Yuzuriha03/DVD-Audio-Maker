use dvda_core::app::{self, AppOptions};

fn main() {
    let source = std::env::args()
        .nth(1)
        .expect("usage: profile_roundtrip <profile.json>");
    let options = AppOptions::load(Some(std::path::Path::new(&source))).expect("load profile");
    println!(
        "loaded language={} title={:?} disc_bytes={:?} menu={:?} mode={:?} rate={:?} bits={:?} font={:?}",
        options.language,
        options.values.get("Title"),
        options.values.get("DiscBytes"),
        options.values.get("MenuEnabled"),
        options.values.get("MlpSource"),
        options.values.get("MlpSampleRate"),
        options.values.get("MlpBits"),
        options.values.get("MenuFont"),
    );
    let out = std::env::temp_dir().join("dvda-roundtrip.json");
    app::save_profile(&out, &options.profile_values(), &options.language).expect("save profile");
    let reloaded = AppOptions::load(Some(&out)).expect("reload saved profile");
    let keys = [
        "SourceDirectory",
        "FinalDirectory",
        "Title",
        "DiscBytes",
        "MlpSource",
        "MlpSampleRate",
        "MlpBits",
        "MenuEnabled",
        "MenuTracksPerPage",
        "MenuFont",
        "GroupTrackLimit",
        "KeepTemporary",
        "ResumeEnabled",
    ];
    let mut mismatches = 0;
    for key in keys {
        if options.values.get(key) != reloaded.values.get(key) {
            mismatches += 1;
            println!(
                "MISMATCH {key}: {:?} -> {:?}",
                options.values.get(key),
                reloaded.values.get(key)
            );
        }
    }
    println!("roundtrip mismatches={mismatches}");
    let saved = std::fs::read_to_string(&out).unwrap_or_default();
    println!("saved profile: {}", saved.get(..200).unwrap_or(&saved));
}
