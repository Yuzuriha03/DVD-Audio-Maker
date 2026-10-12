//! Compare preparation cache and snapshot costs on the same synthetic sources.
use dvda_core::{identity, preparation::state};
use dvda_native::media::Callbacks;
use serde_json::json;
use std::{fs, path::PathBuf, time::Instant};

struct Events;
impl Callbacks for Events {
    fn emit(&mut self, _: i32, _: &str) {}
    fn cancelled(&mut self) -> bool {
        false
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("Expected a new benchmark directory")?);
    let count: usize = args
        .next()
        .ok_or("Expected a track count")?
        .to_str()
        .ok_or("Invalid track count")?
        .parse()?;
    fs::create_dir(&root)?;
    let root = fs::canonicalize(root)?;
    let source = root.join("sources");
    fs::create_dir(&source)?;
    let mut entries = Vec::new();
    let mut cache = state::Cache::default();
    for index in 0..count {
        let path = source.join(format!("track-{index:05}.flac"));
        fs::write(&path, index.to_le_bytes())?;
        let path = path.to_string_lossy().into_owned();
        let entry = state::Entry {
            identity: identity::compute(&path)?,
            ..Default::default()
        };
        cache.record(path.clone(), entry.clone());
        entries.push((path, entry));
    }
    let cache_path = root.join("cache.json");
    cache.save(&cache_path).map_err(|error| error.message)?;
    let manifest = root.join("manifest.json");
    fs::write(&manifest, b"{}")?;
    let snapshot = root.join("snapshot.json");
    let sources: Vec<_> = entries.iter().map(|(path, _)| path.clone()).collect();
    assert!(state::save_snapshot(
        &snapshot,
        &source.to_string_lossy(),
        "benchmark-v1",
        &manifest.to_string_lossy(),
        &sources,
    ));
    let mut runs = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        let cache = state::Cache::load(&cache_path, &mut Events);
        assert_eq!(cache.len(), count);
        let load = start.elapsed().as_secs_f64();
        let start = Instant::now();
        for (path, _) in entries.iter().rev() {
            assert!(cache.find(path).is_some());
        }
        let find = start.elapsed().as_secs_f64();
        let start = Instant::now();
        let mut fresh = state::Cache::default();
        for (path, entry) in entries.iter().rev() {
            fresh.record(path.clone(), entry.clone());
        }
        let record = start.elapsed().as_secs_f64();
        assert_eq!(fresh.len(), count);
        let start = Instant::now();
        assert!(
            state::reuse_snapshot(
                &snapshot,
                &source.to_string_lossy(),
                "benchmark-v1",
                &manifest.to_string_lossy(),
            )
            .map_err(|error| error.message)?
            .0
            .is_some()
        );
        runs.push(json!({
            "cache_load_seconds": load,
            "cache_find_seconds": find,
            "cache_record_seconds": record,
            "snapshot_reuse_seconds": start.elapsed().as_secs_f64(),
        }));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"tracks": count, "runs": runs}))?
    );
    Ok(())
}
