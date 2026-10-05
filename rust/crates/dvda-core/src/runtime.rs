//! Required runtime files embedded in the GUI; user documents remain beside it.
use crate::{hash, preparation::state};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    os::windows::fs::OpenOptionsExt,
    path::{Component, Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
const MAGIC: &[u8; 8] = b"DVDRUN01";
const MAX_FILE: usize = 128 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
struct Entry {
    path: String,
    sha256: String,
    size: usize,
    offset: usize,
    length: usize,
}

pub fn directory() -> Option<&'static Path> {
    DIRECTORY.get().map(PathBuf::as_path)
}
fn relative(path: &str) -> Result<PathBuf, String> {
    if path.is_empty() || path.contains(['\\', ':', '\0']) {
        return Err("Invalid runtime path".into());
    }
    let parsed = PathBuf::from(path);
    if !parsed
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err("Invalid runtime path".into());
    }
    Ok(parsed)
}
pub fn pack(root: &Path) -> Result<Vec<u8>, String> {
    fn collect(root: &Path, folder: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(folder)? {
            let entry = entry?;
            if entry.file_type()?.is_symlink() {
                return Err(io::Error::other("Runtime symlink is not supported"));
            }
            if entry.file_type()?.is_dir() {
                collect(root, &entry.path(), files)?;
            } else {
                files.push(entry.path().strip_prefix(root).unwrap().to_owned());
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    collect(root, root, &mut files).map_err(|e| e.to_string())?;
    files.sort();
    let mut entries = Vec::new();
    let mut payload = Vec::new();
    for path in files {
        let data = fs::read(root.join(&path)).map_err(|e| e.to_string())?;
        if data.is_empty() || data.len() > MAX_FILE {
            return Err(format!("Invalid runtime file size: {}", path.display()));
        }
        let compressed = dvda_native::compression::compress(&data).map_err(|e| e.to_string())?;
        entries.push(Entry {
            path: path.to_string_lossy().replace('\\', "/"),
            sha256: hash::hex_digest(&data),
            size: data.len(),
            offset: payload.len(),
            length: compressed.len(),
        });
        payload.extend(compressed);
    }
    let metadata = serde_json::to_vec(&entries).map_err(|e| e.to_string())?;
    let mut output = MAGIC.to_vec();
    output.extend((metadata.len() as u32).to_le_bytes());
    output.extend(metadata);
    output.extend(payload);
    Ok(output)
}

fn safe_target(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let mut target = root.to_owned();
    // Reject directory junctions and symlinks, including existing leaf files.
    for part in relative.components() {
        target.push(part.as_os_str());
        if let Ok(metadata) = fs::symlink_metadata(&target) {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("Runtime contains a reparse point".into());
            }
        }
    }
    Ok(target)
}
pub fn extract(archive: &[u8], cache: &Path) -> Result<PathBuf, String> {
    if archive.get(..8) != Some(MAGIC.as_slice()) {
        return Err("Invalid runtime archive header".into());
    }
    let length = u32::from_le_bytes(
        archive
            .get(8..12)
            .ok_or("Truncated runtime archive")?
            .try_into()
            .unwrap(),
    ) as usize;
    let metadata = archive
        .get(12..12 + length)
        .ok_or("Truncated runtime metadata")?;
    let entries: Vec<Entry> = serde_json::from_slice(metadata).map_err(|e| e.to_string())?;
    if entries.is_empty() {
        return Err("Empty runtime archive".into());
    }
    let payload = &archive[12 + length..];
    let digest = hash::hex_digest(archive);
    fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let root = safe_target(cache, Path::new(&digest))?;
    let lock = cache.join(format!("{digest}.lock"));
    let started = Instant::now();
    let _lock = loop {
        match fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .share_mode(0)
            .open(&lock)
        {
            Ok(file) => break file,
            Err(error)
                if error.raw_os_error() == Some(32)
                    && started.elapsed() < Duration::from_secs(120) =>
            {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(error) => return Err(format!("Cannot lock runtime cache: {error}")),
        }
    };
    let mut names = std::collections::HashSet::new();
    for entry in entries {
        let path = relative(&entry.path)?;
        if !names.insert(entry.path.to_lowercase()) || entry.size == 0 || entry.size > MAX_FILE {
            return Err("Invalid runtime archive entry".into());
        }
        let target = safe_target(&root, &path)?;
        let end = entry
            .offset
            .checked_add(entry.length)
            .ok_or("Runtime offset overflow")?;
        let compressed = payload
            .get(entry.offset..end)
            .ok_or("Truncated runtime payload")?;
        let intact = fs::metadata(&target).is_ok_and(|m| m.len() == entry.size as u64)
            && fs::File::open(&target)
                .and_then(hash::reader_digest)
                .is_ok_and(|hash| hash == entry.sha256);
        if intact {
            continue;
        }
        let data = dvda_native::compression::decompress(compressed, entry.size)
            .map_err(|e| e.to_string())?;
        if hash::hex_digest(&data) != entry.sha256 {
            return Err(format!("Runtime checksum mismatch: {}", entry.path));
        }
        state::write_bytes(&target, &data).map_err(|e| e.message)?;
    }
    Ok(root)
}

/// Initialize embedded components.
///
/// # Safety
/// Call once from main before any other thread or native component is started.
pub unsafe fn initialize(archive: &[u8]) -> Result<(), String> {
    if archive.is_empty() {
        return Ok(());
    }
    let cache = crate::app::default_profile_path()
        .parent()
        .ok_or("Missing settings directory")?
        .join("runtime");
    let root = extract(archive, &cache)?;
    // SAFETY: this startup-only call precedes all threads and native library loading.
    unsafe {
        for key in [
            "DVDA_MEDIA_NATIVE_DIR",
            "DVDA_IMAGE_NATIVE_DIR",
            "DVDA_FORMATS_NATIVE_DIR",
            "DVDA_MENU_NATIVE_DIR",
        ] {
            std::env::set_var(key, &root);
        }
        std::env::set_var("DVDA_ENCODER_LIBRARY", root.join("mlp_encoder.dll"));
        std::env::set_var(
            "DVDA_DISC_VERIFY_LIBRARY",
            root.join("dvda-disc-verify.dll"),
        );
    }
    DIRECTORY
        .set(root)
        .map_err(|_| "Runtime already initialized".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_reuses_repairs_and_serializes_concurrent_startup() {
        let root = std::env::temp_dir().join(format!(
            "dvda-runtime-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let input = root.join("input");
        fs::create_dir_all(input.join("data")).unwrap();
        let data = vec![42u8; 32768];
        fs::write(input.join("codec.dll"), &data).unwrap();
        fs::write(input.join("data/config.xml"), b"config").unwrap();
        let archive = pack(&input).unwrap();
        let cache = root.join("cache");
        let output = extract(&archive, &cache).unwrap();
        let time = fs::metadata(output.join("codec.dll"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(extract(&archive, &cache).unwrap(), output);
        assert_eq!(
            fs::metadata(output.join("codec.dll"))
                .unwrap()
                .modified()
                .unwrap(),
            time
        );
        fs::write(output.join("codec.dll"), vec![43; data.len()]).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(output.join("codec.dll"))
            .unwrap()
            .set_modified(time)
            .unwrap();
        std::thread::scope(|scope| {
            for _ in 0..3 {
                scope.spawn(|| extract(&archive, &cache).unwrap());
            }
        });
        assert_eq!(fs::read(output.join("codec.dll")).unwrap(), data);
        assert!(extract(&archive[..archive.len() - 1], &cache).is_err());
        for path in [
            "../evil",
            "C:/evil",
            "/evil",
            "data/../../evil",
            "data\\evil",
        ] {
            assert!(relative(path).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
}
