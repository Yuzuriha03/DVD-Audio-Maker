use std::path::{Path, PathBuf};

use dvda_core::app::{self, AppOptions};
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

fn hashes(dir: &str) -> Vec<String> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "iso")
                && let Ok(data) = std::fs::read(&path)
            {
                let digest = dvda_core::hash::hex_digest(&data);
                result.push(digest.chars().take(12).collect());
            }
        }
    }
    result
}

fn run_variant(
    base: &AppOptions,
    name: &str,
    overrides: Map<String, Value>,
) -> (bool, Vec<String>, Vec<String>) {
    let build_dir = format!("/tmp/codec/{name}/build");
    let final_dir = format!("/tmp/codec/{name}/final");
    let mut overrides = overrides;
    overrides.insert("DVDA_BUILD_DIR".into(), json!(build_dir));
    overrides.insert("DVDA_FINAL_DIR".into(), json!(final_dir));
    overrides.insert("DVDA_MENU".into(), json!("off"));
    let options = base.with_profile_values(overrides).expect("options");
    println!(
        "  {name}: source={:?} bits={:?} rate={:?} dither={:?} external={:?}",
        options.value("MlpSource"),
        options.value("MlpBits"),
        options.value("MlpSampleRate"),
        options.value("Dither16"),
        options.value("MlpExternalDirectory")
    );
    let task = match options.task_options() {
        Ok(task) => task,
        Err(error) => return (false, vec![error], Vec::new()),
    };
    let mut quiet = Quiet;
    let prepared = app::run_prepare(&task, false, &mut quiet);
    if prepared.failure.is_some() || prepared.exit_code != Some(0) {
        let message = prepared.failure.map_or_default(|failure| failure.message);
        return (false, vec![message], Vec::new());
    }
    let result = match app::run_build(&task, false, &mut quiet) {
        Ok(result) => result,
        Err(error) => return (false, vec![format!("build: {error}")], Vec::new()),
    };
    let mut issues = codes(&result.diagnostics);
    let mut ok = result.succeeded;
    if ok {
        match app::run_verify(&task, &mut quiet) {
            Ok(verification) => {
                issues.extend(codes(&verification.diagnostics));
                ok = verification.succeeded;
            }
            Err(error) => {
                issues.push(format!("verify: {error}"));
                ok = false;
            }
        }
    }
    let digests = hashes(&final_dir);
    if name == "external_seed" {
        // Copy the encoded MLP into an external tree mirroring the source layout.
        fn collect(dir: &Path, found: &mut Vec<PathBuf>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        collect(&path, found);
                    } else if path.extension().is_some_and(|ext| ext == "mlp") {
                        found.push(path);
                    }
                }
            }
        }
        let mlp_dir = task
            .value("MlpExternalDirectory")
            .as_str()
            .unwrap_or("")
            .to_owned();
        let mut found = Vec::new();
        collect(Path::new(&mlp_dir), &mut found);
        println!("  seed mlp files: {}", found.len());
        if let Some(first) = found.first() {
            let target = Path::new("/tmp/extmlp/单独专辑/01 - 单曲.mlp");
            let _ = std::fs::create_dir_all(target.parent().unwrap());
            let _ = std::fs::copy(first, target);
            println!("  seeded external mlp <- {}", first.display());
        }
    }
    task.finish_task();
    (ok, issues, digests)
}

fn main() {
    let profile = std::env::args()
        .nth(1)
        .expect("usage: codec_matrix <profile.json>");
    let base = AppOptions::load(Some(std::path::Path::new(&profile))).expect("profile");
    let _ = std::fs::remove_dir_all("/tmp/codec");
    let _ = std::fs::remove_dir_all("/tmp/extmlp");

    let variants: Vec<(&str, Map<String, Value>)> = vec![
        (
            "lpcm_16",
            edits(&[("DVDA_MLP_SOURCE", "lpcm"), ("DVDA_MLP_BITS", "16")]),
        ),
        (
            "lpcm_24",
            edits(&[("DVDA_MLP_SOURCE", "lpcm"), ("DVDA_MLP_BITS", "24")]),
        ),
        (
            "lpcm_16_nd",
            edits(&[
                ("DVDA_MLP_SOURCE", "lpcm"),
                ("DVDA_MLP_BITS", "16"),
                ("DVDA_DITHER_16", "off"),
            ]),
        ),
        (
            "lpcm_20",
            edits(&[("DVDA_MLP_SOURCE", "lpcm"), ("DVDA_MLP_BITS", "20")]),
        ),
        (
            "mlp_16_dither_on",
            edits(&[("DVDA_MLP_BITS", "16"), ("DVDA_DITHER_16", "on")]),
        ),
        (
            "mlp_16_dither_off",
            edits(&[("DVDA_MLP_BITS", "16"), ("DVDA_DITHER_16", "off")]),
        ),
        ("mlp_20", edits(&[("DVDA_MLP_BITS", "20")])),
        ("rate_96000", edits(&[("DVDA_MLP_SAMPLE_RATE", "96000")])),
        ("rate_44100", edits(&[("DVDA_MLP_SAMPLE_RATE", "44100")])),
        ("external_seed", edits(&[])),
    ];
    let mut results: std::collections::BTreeMap<String, (bool, Vec<String>, Vec<String>)> =
        std::collections::BTreeMap::new();
    for (name, overrides) in variants {
        let outcome = run_variant(&base, name, overrides);
        results.insert(name.to_owned(), outcome);
    }
    let external = run_variant(
        &base,
        "external_mlp",
        edits(&[
            ("DVDA_MLP_SOURCE", "external"),
            ("DVDA_MLP_EXTERNAL_DIR", "/tmp/extmlp"),
        ]),
    );
    results.insert("external_mlp".to_owned(), external);
    for (name, (ok, issues, digests)) in results {
        println!("{name}: ok={ok} issues={issues:?} iso_hash={digests:?}");
    }
    let _ = PathBuf::from("");
}
