//! Validate provenance, architecture and the complete DLL closure before staging.
use crate::pe;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct Inputs {
    pub files: BTreeMap<String, PathBuf>,
    pub records: Vec<PathBuf>,
}

pub fn validate(
    media: &Path,
    image: &Path,
    author: &Path,
    formats: &Path,
) -> Result<Inputs, String> {
    let mut inputs = Inputs {
        files: BTreeMap::new(),
        records: Vec::new(),
    };
    let (media_record, media_files) = component(
        media,
        "media-build.json",
        "files",
        &[
            "dvda-media.dll",
            "avcodec-63.dll",
            "avformat-63.dll",
            "avutil-61.dll",
            "swresample-7.dll",
            "swscale-10.dll",
        ],
        false,
        false,
    )?;
    if media_record["profile"] != "shared" {
        return Err("Onefile media runtime must use profile=shared".into());
    }
    let (image_record, image_files) = component(
        image,
        "image-build.json",
        "files",
        &["dvda-image.dll"],
        true,
        false,
    )?;
    let _ = image_record;
    let (formats_record, formats_files) = component(
        formats,
        "formats-build.json",
        "files",
        &["dvda-formats.dll"],
        true,
        false,
    )?;
    if formats_record["profile"] != "c17-formats-runtime" {
        return Err("Invalid C17 formats build profile".into());
    }
    let (author_record, mut author_files) = component(
        author,
        "author-build.json",
        "runtime_files",
        &[
            "avcodec-63.dll",
            "avformat-63.dll",
            "avutil-61.dll",
            "dvda-menu-spu.dll",
            "dvda-menu-nav.dll",
            "dvda-disc-verify.dll",
        ],
        false,
        true,
    )?;
    if author_record["ffmpeg_linkage"] != "shared-source-built-shared-profile"
        || author_record["ffmpeg_profile"] != "build-minimal-ffmpeg.py:shared"
        || author_record["menu_linkage"] != "in-process-source-built"
    {
        return Err("Author must use the shared source-built FFmpeg profile and in-process source-built menu libraries".into());
    }
    let author_exe = author.join("dvda-author-dev.exe");
    let imports = validate_binary(
        &author_exe,
        &author_record["files"]["dvda-author-dev.exe"],
        false,
    )?;
    closure("dvda-author-dev.exe", &imports, &author_files, true)?;
    author_files.insert("dvda-author-dev.exe".into(), author_exe);
    for files in [media_files, image_files, formats_files, author_files] {
        for (name, path) in files {
            if let Some(existing) = inputs.files.get(&name) {
                if fs::read(existing).map_err(err)? != fs::read(&path).map_err(err)? {
                    return Err(format!(
                        "Shared native component differs between consumers: {name}"
                    ));
                }
            } else {
                inputs.files.insert(name, path);
            }
        }
    }
    for (dir, name) in [
        (media, "media-build.json"),
        (image, "image-build.json"),
        (author, "author-build.json"),
        (formats, "formats-build.json"),
    ] {
        inputs.records.push(dir.join(name));
    }
    Ok(inputs)
}

fn component(
    directory: &Path,
    manifest: &str,
    field: &str,
    required: &[&str],
    single: bool,
    api_sets: bool,
) -> Result<(Value, BTreeMap<String, PathBuf>), String> {
    let path = directory.join(manifest);
    let record: Value = serde_json::from_slice(
        &fs::read(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?,
    )
    .map_err(|e| format!("Invalid {}: {e}", path.display()))?;
    let metadata = record[field]
        .as_object()
        .ok_or_else(|| format!("{manifest}: missing {field}"))?;
    let mut files = BTreeMap::new();
    for name in metadata.keys() {
        if !pe::valid_name(name, "dll") {
            return Err(format!("{manifest}: invalid library name {name}"));
        }
        if files
            .insert(name.to_ascii_lowercase(), directory.join(name))
            .is_some()
        {
            return Err(format!("{manifest}: duplicate library name {name}"));
        }
    }
    for name in required {
        if !files.contains_key(*name) {
            return Err(format!("{manifest}: missing required library {name}"));
        }
    }
    if single && files.len() != 1 {
        return Err(format!(
            "{manifest}: expected exactly one self-contained DLL"
        ));
    }
    let actual: BTreeSet<_> = fs::read_dir(directory)
        .map_err(err)?
        .map(|entry| {
            let entry = entry.map_err(err)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "Invalid native file name")?;
            Ok((name.to_ascii_lowercase(), entry.file_type().map_err(err)?))
        })
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .filter_map(|(name, kind)| (name.ends_with(".dll") && kind.is_file()).then_some(name))
        .collect();
    if actual != files.keys().cloned().collect() {
        return Err(format!(
            "{manifest}: actual DLL files differ from the manifest"
        ));
    }
    for (name, metadata) in metadata {
        let imports = validate_binary(&directory.join(name), metadata, true)?;
        closure(name, &imports, &files, api_sets)?;
    }
    Ok((record, files))
}

pub fn validate_binary(
    path: &Path,
    metadata: &Value,
    dll: bool,
) -> Result<BTreeSet<String>, String> {
    let bytes = fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let expected = metadata["sha256"]
        .as_str()
        .ok_or_else(|| format!("Missing SHA-256 record: {}", path.display()))?;
    if metadata["bytes"].as_u64() != Some(bytes.len() as u64)
        || !hash(&bytes).eq_ignore_ascii_case(expected)
    {
        return Err(format!(
            "Native component checksum mismatch: {}",
            path.display()
        ));
    }
    pe::imports(&bytes, dll).map_err(|e| format!("{}: {e}", path.display()))
}

fn closure(
    name: &str,
    imports: &BTreeSet<String>,
    files: &BTreeMap<String, PathBuf>,
    api_sets: bool,
) -> Result<(), String> {
    for import in imports {
        if !files.contains_key(import) && !system_dependency(import, api_sets) {
            return Err(format!("Missing native dependency of {name}: {import}"));
        }
    }
    Ok(())
}

pub fn validate_staged(directory: &Path) -> Result<(), String> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(directory).map_err(err)? {
        let path = entry.map_err(err)?.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        if path.is_file() && (name.ends_with(".dll") || name.ends_with(".exe")) {
            files.insert(name, path);
        }
    }
    for (name, path) in &files {
        let imports = pe::imports(&fs::read(path).map_err(err)?, name.ends_with(".dll"))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        closure(name, &imports, &files, true)?;
    }
    Ok(())
}

fn system_dependency(name: &str, api_sets: bool) -> bool {
    if api_sets && (name.starts_with("api-ms-win-") || name.starts_with("ext-ms-win-")) {
        return true;
    }
    if matches!(
        name,
        "kernel32.dll"
            | "ntdll.dll"
            | "msvcrt.dll"
            | "ucrtbase.dll"
            | "advapi32.dll"
            | "bcrypt.dll"
            | "shell32.dll"
            | "user32.dll"
            | "ws2_32.dll"
            | "shlwapi.dll"
            | "xmllite.dll"
            | "ole32.dll"
            | "gdi32.dll"
            | "comdlg32.dll"
            | "comctl32.dll"
            | "oleaut32.dll"
            | "winmm.dll"
            | "version.dll"
            | "secur32.dll"
            | "crypt32.dll"
            | "dwmapi.dll"
            | "uxtheme.dll"
            | "imm32.dll"
    ) {
        return true;
    }
    // Preserve the original rule for other Windows-provided libraries. Never
    // search PATH or the working directory, which could hide a missing DLL.
    std::env::var_os("SystemRoot")
        .is_some_and(|root| PathBuf::from(root).join("System32").join(name).is_file())
}

pub fn hash(bytes: &[u8]) -> String {
    dvda_core::hash::sha256(bytes)
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect()
}
pub fn file_hash(path: &Path) -> Result<String, String> {
    Ok(hash(&fs::read(path).map_err(err)?))
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    pub struct Fixture {
        pub root: PathBuf,
    }
    impl Fixture {
        pub fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "dvda-package-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            for (dir, manifest, names) in [
                (
                    "media",
                    "media-build.json",
                    vec![
                        "dvda-media.dll",
                        "avcodec-63.dll",
                        "avformat-63.dll",
                        "avutil-61.dll",
                        "swresample-7.dll",
                        "swscale-10.dll",
                    ],
                ),
                ("image", "image-build.json", vec!["dvda-image.dll"]),
                ("formats", "formats-build.json", vec!["dvda-formats.dll"]),
                (
                    "author",
                    "author-build.json",
                    vec![
                        "avcodec-63.dll",
                        "avformat-63.dll",
                        "avutil-61.dll",
                        "dvda-menu-spu.dll",
                        "dvda-menu-nav.dll",
                        "dvda-disc-verify.dll",
                    ],
                ),
            ] {
                let folder = root.join(dir);
                fs::create_dir(&folder).unwrap();
                let mut files = serde_json::Map::new();
                for name in names {
                    let b = pe::fixture(true, &["kernel32.dll"], &[]);
                    fs::write(folder.join(name), &b).unwrap();
                    files.insert(
                        name.into(),
                        serde_json::json!({"bytes":b.len(),"sha256":hash(&b)}),
                    );
                }
                let mut record = serde_json::json!({"profile":if dir=="formats" {"c17-formats-runtime"}else{"shared"},"files":files});
                if dir == "author" {
                    record["runtime_files"] = record["files"].take();
                    let b = pe::fixture(false, &["avcodec-63.dll"], &["dvda-menu-spu.dll"]);
                    fs::write(folder.join("dvda-author-dev.exe"), &b).unwrap();
                    record["files"] = serde_json::json!({"dvda-author-dev.exe":{"bytes":b.len(),"sha256":hash(&b)}});
                    record["ffmpeg_linkage"] = "shared-source-built-shared-profile".into();
                    record["ffmpeg_profile"] = "build-minimal-ffmpeg.py:shared".into();
                    record["menu_linkage"] = "in-process-source-built".into();
                }
                fs::write(folder.join(manifest), serde_json::to_vec(&record).unwrap()).unwrap();
            }
            Self { root }
        }
        pub fn validate(&self) -> Result<Inputs, String> {
            validate(
                &self.root.join("media"),
                &self.root.join("image"),
                &self.root.join("author"),
                &self.root.join("formats"),
            )
        }
        pub fn edit(&self, dir: &str, change: impl FnOnce(&mut Value)) {
            let path = self.root.join(dir).join(format!("{dir}-build.json"));
            let mut d: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            change(&mut d);
            fs::write(path, serde_json::to_vec(&d).unwrap()).unwrap();
        }
        pub fn replace(&self, dir: &str, name: &str, bytes: Vec<u8>) {
            fs::write(self.root.join(dir).join(name), &bytes).unwrap();
            self.edit(dir, |d| {
                d[if dir == "author" {
                    "runtime_files"
                } else {
                    "files"
                }][name] = serde_json::json!({"bytes":bytes.len(),"sha256":hash(&bytes)})
            });
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn manifest_integrity_and_exact_inventory() {
        let f = Fixture::new();
        assert_eq!(f.validate().unwrap().files.len(), 12);
        f.edit("media", |d| {
            d["files"].as_object_mut().unwrap().remove("dvda-media.dll");
        });
        assert!(f.validate().unwrap_err().contains("missing required"));
        let f = Fixture::new();
        fs::remove_file(f.root.join("media/dvda-media.dll")).unwrap();
        assert!(f.validate().is_err());
        let f = Fixture::new();
        let path = f.root.join("media/dvda-media.dll");
        let mut b = fs::read(&path).unwrap();
        b[900] ^= 1;
        fs::write(path, b).unwrap();
        assert!(f.validate().unwrap_err().contains("checksum"));
        let f = Fixture::new();
        fs::write(f.root.join("media/extra.dll"), b"x").unwrap();
        assert!(f.validate().is_err());
        for name in [
            "../escape.dll",
            "C:escape.dll",
            "a\\b.dll",
            "NUL.dll",
            "dvda-media.DLL",
        ] {
            let f = Fixture::new();
            f.edit("media", |d| {
                d["files"][name] = d["files"]["dvda-media.dll"].clone()
            });
            assert!(f.validate().is_err());
        }
    }
    #[test]
    fn component_architecture_dependencies_and_shared_profile() {
        for dir in ["media", "image", "formats", "author"] {
            let name = match dir {
                "media" => "dvda-media.dll",
                "image" => "dvda-image.dll",
                "formats" => "dvda-formats.dll",
                _ => "dvda-disc-verify.dll",
            };
            for delayed in [false, true] {
                let f = Fixture::new();
                f.replace(
                    dir,
                    name,
                    pe::fixture(
                        true,
                        if delayed { &[] } else { &["missing.dll"] },
                        if delayed { &["missing.dll"] } else { &[] },
                    ),
                );
                assert!(
                    f.validate()
                        .unwrap_err()
                        .contains("Missing native dependency")
                );
            }
            let f = Fixture::new();
            let mut b = pe::fixture(true, &[], &[]);
            b[132..134].copy_from_slice(&0x14cu16.to_le_bytes());
            f.replace(dir, name, b);
            assert!(f.validate().unwrap_err().contains("x64"));
            let f = Fixture::new();
            let mut b = pe::fixture(true, &[], &[]);
            b[264 + 14 * 8] = 1;
            f.replace(dir, name, b);
            assert!(f.validate().unwrap_err().contains("Managed"));
            let f = Fixture::new();
            fs::remove_file(f.root.join(dir).join(format!("{dir}-build.json"))).unwrap();
            assert!(f.validate().is_err());
        }
        let f = Fixture::new();
        f.edit("media", |d| d["profile"] = "media".into());
        assert!(f.validate().unwrap_err().contains("profile=shared"));
        for field in ["ffmpeg_profile", "ffmpeg_linkage", "menu_linkage"] {
            let f = Fixture::new();
            f.edit("author", |d| d[field] = "wrong".into());
            assert!(f.validate().is_err());
        }
        for name in [
            "dvda-menu-spu.dll",
            "dvda-menu-nav.dll",
            "dvda-disc-verify.dll",
        ] {
            let f = Fixture::new();
            f.edit("author", |d| {
                d["runtime_files"].as_object_mut().unwrap().remove(name);
            });
            assert!(f.validate().is_err());
        }
        let f = Fixture::new();
        let mut b = pe::fixture(true, &["kernel32.dll"], &[]);
        b[900] = 1;
        f.replace("author", "avcodec-63.dll", b);
        assert!(
            f.validate()
                .unwrap_err()
                .contains("differs between consumers")
        );
        let f = Fixture::new();
        f.replace(
            "author",
            "dvda-menu-spu.dll",
            pe::fixture(true, &["api-ms-win-core-file-l1-1-0.dll"], &[]),
        );
        assert!(f.validate().is_ok());
        let f = Fixture::new();
        fs::write(f.root.join("image/extra.dll"), b"x").unwrap();
        assert!(f.validate().is_err());
    }
}
