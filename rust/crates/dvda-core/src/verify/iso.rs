use super::CHUNK_SIZE;
use crate::formats;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) fn find_isos(directory: &Path, prefix: &str) -> Result<Vec<PathBuf>, String> {
    if !directory.is_dir() {
        return Err(format!(
            "Final directory is missing: {}",
            directory.display()
        ));
    }
    let mut paths = fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && matches_iso_name(path, prefix))
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| iso_number(path).unwrap_or(i32::MAX));
    Ok(paths)
}

fn matches_iso_name(path: &Path, prefix: &str) -> bool {
    if prefix.trim().is_empty() {
        return path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("iso"));
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    let prefix = prefix.to_ascii_lowercase();
    lower.starts_with(&(prefix.clone() + "_"))
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("iso"))
        && lower[prefix.len() + 1..lower.len() - 4]
            .parse::<i32>()
            .is_ok()
}

pub(super) fn iso_number(path: &Path) -> Option<i32> {
    path.file_stem()?
        .to_string_lossy()
        .rsplit('_')
        .next()?
        .parse()
        .ok()
}

pub(crate) fn read_iso_file(iso: &Path, name: &str) -> Result<Vec<u8>, String> {
    let mut reader = formats::iso_file_chunks(iso, name, CHUNK_SIZE)?;
    let mut data = Vec::new();
    while let Some(chunk) = reader.next_chunk()? {
        if data.len() + chunk.len() > 16 * 1024 * 1024 {
            return Err("IFO exceeds size limit".into());
        }
        data.extend(chunk);
    }
    Ok(data)
}

pub(super) fn aob_chunk_total(isos: &[PathBuf], metadata: &formats::IsoMetadataCache) -> u64 {
    let mut total = 0u64;
    for iso in isos {
        let Ok(entries) = metadata.list_directory(iso, "AUDIO_TS") else {
            continue;
        };
        total = total.saturating_add(
            entries
                .iter()
                .filter(|entry| parse_aob(&entry.name).is_some())
                .map(|entry| u64::from(entry.size).div_ceil(CHUNK_SIZE as u64))
                .sum::<u64>(),
        );
    }
    total.max(1)
}

pub(super) fn aob_names(
    iso: &Path,
    group: i64,
    metadata: &formats::IsoMetadataCache,
) -> Result<Vec<String>, String> {
    let entries = metadata.list_directory(iso, "AUDIO_TS")?;
    let mut names = entries
        .into_iter()
        .filter_map(|entry| {
            let (actual_group, segment) = parse_aob(&entry.name)?;
            (actual_group == group).then_some((segment, entry.name))
        })
        .collect::<Vec<_>>();
    names.sort_by_key(|(segment, _)| *segment);
    Ok(names.into_iter().map(|(_, name)| name).collect())
}

pub(super) fn chunks<'a>(
    iso: &'a Path,
    names: &'a [String],
) -> Box<dyn Iterator<Item = Result<Vec<u8>, String>> + 'a> {
    let mut readers = Vec::new();
    for name in names {
        match formats::iso_file_chunks(iso, &format!("AUDIO_TS/{name}"), CHUNK_SIZE) {
            Ok(reader) => readers.push(reader),
            Err(error_text) => return Box::new(std::iter::once(Err(error_text))),
        }
    }
    let mut index = 0usize;
    Box::new(std::iter::from_fn(move || {
        loop {
            let reader = readers.get_mut(index)?;
            match reader.next_chunk() {
                Ok(Some(data)) => return Some(Ok(data)),
                Ok(None) => index += 1,
                Err(error_text) => return Some(Err(error_text)),
            }
        }
    }))
}

pub(super) fn parse_aob(name: &str) -> Option<(i64, i64)> {
    let lower = name.to_ascii_lowercase();
    let parts: Vec<_> = lower
        .strip_prefix("ats_")?
        .strip_suffix(".aob")?
        .split('_')
        .collect();
    if parts.len() != 2 {
        return None;
    }
    Some((parts[0].parse().ok()?, parts[1].parse().ok()?))
}
pub(super) fn is_aob(name: &str) -> bool {
    parse_aob(name).is_some()
}
pub(super) fn parse_ifo(name: &str) -> Option<i64> {
    name.to_ascii_lowercase()
        .strip_prefix("ats_")?
        .strip_suffix("_0.ifo")?
        .parse()
        .ok()
}
pub(super) fn is_ifo(name: &str) -> bool {
    parse_ifo(name).is_some()
}
