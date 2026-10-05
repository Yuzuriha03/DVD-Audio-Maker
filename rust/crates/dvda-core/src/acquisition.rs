//! Existing MLP import and batch-output path rules.
use dvda_native::files::{ordinal_ignore_case, ordinal_ignore_case_cmp};
use serde_json::{Map, Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "build.mlp_lookup" => lookup(request),
        "build.mlp_resolve" => resolve(request),
        "build.mlp_destination" => destination(request),
        _ => Err(format!("Unsupported acquisition operation: {operation}")),
    }
}

fn lookup(request: Value) -> Result<Value, String> {
    let root = text(&request, "Root").ok_or("Missing MLP root")?;
    let mut paths = Vec::new();
    collect(&PathBuf::from(root), &mut paths)?;
    let mut named_paths: Vec<_> = paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            (name, path)
        })
        .collect();
    // Stable sorting retains the original spelling of the first encountered
    // name while grouping every Windows ordinal case variant together.
    named_paths.sort_by(|(left, _), (right, _)| ordinal_ignore_case_cmp(left, right));
    let mut entries = Map::new();
    let mut ambiguous_names = Vec::new();
    for group in named_paths.chunk_by(|(left, _), (right, _)| ordinal_ignore_case(left, right)) {
        let (name, path) = &group[0];
        if group.len() > 1 {
            ambiguous_names.push(name.clone());
        } else {
            entries.insert(
                name.clone(),
                Value::String(path.to_string_lossy().into_owned()),
            );
        }
    }
    Ok(json!({"Index":entries,"AmbiguousNames":ambiguous_names}))
}

fn resolve(request: Value) -> Result<Value, String> {
    let source = PathBuf::from(text(&request, "SourcePath").ok_or("Missing source path")?);
    let source_root = PathBuf::from(text(&request, "SourceRoot").ok_or("Missing source root")?);
    let external_root =
        PathBuf::from(text(&request, "ExternalRoot").ok_or("Missing external root")?);
    let relative = relative_or_file_name(&source, &source_root);
    let mut mirror = external_root.join(relative);
    mirror.set_extension("mlp");
    if mirror.is_file() {
        return Ok(Value::String(mirror.to_string_lossy().into_owned()));
    }
    let stem = source.file_stem().map(|value| value.to_string_lossy());
    let index = request
        .get("BasenameIndex")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let result = stem.and_then(|stem| {
        index
            .iter()
            .find(|(key, _)| ordinal_ignore_case(key, stem.as_ref()))
            .map(|(_, value)| value.clone())
    });
    Ok(result.unwrap_or(Value::Null))
}

fn destination(request: Value) -> Result<Value, String> {
    let source = PathBuf::from(text(&request, "SourcePath").ok_or("Missing source path")?);
    let source_root = PathBuf::from(text(&request, "SourceRoot").ok_or("Missing source root")?);
    let output_root = PathBuf::from(text(&request, "OutputRoot").ok_or("Missing output root")?);
    let relative = relative_or_file_name(&source, &source_root);
    let mut destination = output_root.join(relative);
    destination.set_extension("mlp");
    Ok(Value::String(destination.to_string_lossy().into_owned()))
}

fn relative_or_file_name(source: &Path, root: &Path) -> PathBuf {
    let mut remaining = source.components();
    let within_root = !root.as_os_str().is_empty()
        && root.components().all(|expected| {
            remaining.next().is_some_and(|actual| {
                ordinal_ignore_case(
                    &actual.as_os_str().to_string_lossy(),
                    &expected.as_os_str().to_string_lossy(),
                )
            })
        });
    if within_root && !remaining.as_path().as_os_str().is_empty() {
        return remaining.collect();
    }
    source.file_name().map(PathBuf::from).unwrap_or_default()
}

fn collect(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(|error| format!("{}: {error}", root.display()))? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            collect(&path, output)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("mlp"))
        {
            output.push(path);
        }
    }
    Ok(())
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .or_else(|| value.get(key.to_ascii_lowercase()))
        .and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn finds_mirror_before_basename_and_reports_duplicates() {
        let root = std::env::temp_dir().join(format!(
            "dvda-acquisition-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source_root = root.join("source");
        let external = root.join("external");
        fs::create_dir_all(source_root.join("album")).unwrap();
        fs::create_dir_all(external.join("album")).unwrap();
        fs::write(external.join("album/track.mlp"), [1]).unwrap();
        fs::create_dir_all(external.join("a")).unwrap();
        fs::create_dir_all(external.join("b")).unwrap();
        fs::write(external.join("a/same.mlp"), [1]).unwrap();
        fs::write(external.join("b/SAME.MLP"), [2]).unwrap();
        let lookup = lookup(json!({"Root":external})).unwrap();
        assert_eq!(lookup["AmbiguousNames"], json!(["same"]));
        assert!(lookup["Index"].as_object().unwrap().contains_key("track"));
        assert_eq!(
            resolve(json!({
                "SourcePath":source_root.join("album/track.flac"),
                "SourceRoot":source_root,
                "ExternalRoot":external,
                "BasenameIndex":lookup["Index"]
            }))
            .unwrap(),
            Value::String(
                external
                    .join("album")
                    .join("track.mlp")
                    .to_string_lossy()
                    .into_owned()
            )
        );
        assert_eq!(
            resolve(json!({
                "SourcePath":source_root.join("other/TRACK.flac"),
                "SourceRoot":source_root,
                "ExternalRoot":root.join("missing"),
                "BasenameIndex":{"Track": external.join("album").join("track.mlp")}
            }))
            .unwrap(),
            Value::String(
                external
                    .join("album")
                    .join("track.mlp")
                    .to_string_lossy()
                    .into_owned()
            )
        );
        let _ = fs::remove_dir_all(root);
    }
}
