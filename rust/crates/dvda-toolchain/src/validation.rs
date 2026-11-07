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

pub fn target_machine(target: &str) -> Result<u16, String> {
    match target {
        "x86_64-pc-windows-msvc" => Ok(0x8664),
        "aarch64-pc-windows-msvc" => Ok(0xAA64),
        other => Err(format!("Unsupported Windows target: {other}")),
    }
}

fn imports_for(bytes: &[u8], dll: bool, target: &str) -> Result<BTreeSet<String>, String> {
    pe::imports_with_machine(bytes, dll, target_machine(target)?)
}

fn read_json(path: &Path) -> Result<Value, String> {
    let mut bytes = fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes.drain(..3);
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid {}: {e}", path.display()))
}

pub fn validate(media: &Path, image: &Path, author: &Path, target: &str) -> Result<Inputs, String> {
    target_machine(target)?;
    let author_record = read_json(&author.join("author-build.json"))?;
    if author_record["implementation"] == "rust"
        && author_record["ffmpeg_linkage"] == "static-source-built"
    {
        return validate_static_media(media, author, &author_record, target);
    }
    if author_record["implementation"] == "rust" {
        return validate_shared_media(media, author, &author_record, target);
    }
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
        true,
        target,
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
        true,
        target,
    )?;
    let _ = image_record;
    let (author_record, author_files) = component(
        author,
        "author-build.json",
        "runtime_files",
        &["avcodec-63.dll", "avformat-63.dll", "avutil-61.dll"],
        false,
        true,
        target,
    )?;
    if author_record["ffmpeg_linkage"] != "shared-source-built-shared-profile"
        || author_record["ffmpeg_profile"] != "build-msvc-ffmpeg.py:shared"
        || author_record["menu_linkage"] != "in-process-rust"
    {
        return Err("Author must use the shared source-built FFmpeg profile and in-process source-built menu libraries".into());
    }
    let mut author_files = author_files;
    // Legacy manifests may record the old verifier. Authenticate it in its original
    // directory, but check the executable against the DLLs actually shipped.
    author_files.remove("dvda-disc-verify.dll");
    for (name, path) in &author_files {
        let imports = imports_for(&fs::read(path).map_err(err)?, true, target)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        closure(name, &imports, &author_files, true)?;
    }
    for files in [media_files, image_files, author_files] {
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
    ] {
        inputs.records.push(dir.join(name));
    }
    Ok(inputs)
}

fn validate_shared_media(
    media: &Path,
    author: &Path,
    author_record: &Value,
    target: &str,
) -> Result<Inputs, String> {
    let (media_record, mut media_files) = component(
        media,
        "media-build.json",
        "files",
        &[
            "avcodec-63.dll",
            "avformat-63.dll",
            "avutil-61.dll",
            "swresample-7.dll",
            "swscale-10.dll",
        ],
        false,
        true,
        target,
    )?;
    if media_record["profile"] != "shared" {
        return Err("Onefile media runtime must use profile=shared".into());
    }
    media_files.remove("dvda-media.dll");
    let mut linked = validate_rust_author(author, author_record, target)?;
    for (name, path) in media_files {
        if let Some(existing) = linked.files.get(&name) {
            if fs::read(existing).map_err(err)? != fs::read(&path).map_err(err)? {
                return Err(format!(
                    "Shared native component differs between consumers: {name}"
                ));
            }
        } else {
            linked.files.insert(name, path);
        }
    }
    linked.records.push(media.join("media-build.json"));
    Ok(linked)
}

fn validate_static_media(
    media: &Path,
    author: &Path,
    author_record: &Value,
    target: &str,
) -> Result<Inputs, String> {
    let manifest = media.join("build-manifest.json");
    let native = read_json(&manifest)?;
    if native["target"] != target
        || native["toolchain"] != "msvc"
        || native["profile"] != "static"
        || native["linkage"] != "static"
        || native["crt"] != "static"
    {
        return Err("Static FFmpeg requires the authenticated MSVC static profile".into());
    }
    if author_record["ffmpeg_manifest"]["sha256"].as_str() != Some(file_hash(&manifest)?.as_str()) {
        return Err("Author and package FFmpeg manifests differ".into());
    }
    for name in [
        "avcodec",
        "avformat",
        "avutil",
        "swscale",
        "swresample",
        "soxr",
    ] {
        let filename = format!("{name}.lib");
        let path = media.join("lib").join(&filename);
        let bytes = fs::read(&path).map_err(err)?;
        let entry = &native["files"][&filename];
        if !bytes.starts_with(b"!<arch>\n")
            || entry["sha256"].as_str() != Some(hash(&bytes).as_str())
            || entry["bytes"].as_u64() != Some(bytes.len() as u64)
        {
            return Err(format!(
                "Static FFmpeg archive checksum mismatch: {filename}"
            ));
        }
    }
    let mut inputs = validate_rust_author(author, author_record, target)?;
    inputs.records.push(manifest);
    Ok(inputs)
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep ordered author provenance and inventory checks together"
)]
fn validate_rust_author(author: &Path, record: &Value, target: &str) -> Result<Inputs, String> {
    let static_ffmpeg = record["ffmpeg_linkage"] == "static-source-built"
        && record["ffmpeg_profile"] == "build-msvc-ffmpeg.py:static";
    if record["schema_version"] != 1
        || record["target"] != target
        || record["toolchain"] != "msvc"
        || record["project_c_author_compiled"] != false
        || record["menu_linkage"] != "in-process-rust"
        || (!static_ffmpeg
            && (record["ffmpeg_linkage"] != "shared-source-built-shared-profile"
                || record["ffmpeg_profile"] != "build-msvc-ffmpeg.py:shared"))
        || record["author_library"]["implementation"] != "rust"
        || record["author_library"]["available"] != true
        || record["author_library"]["entry"] != "dvda_core::author_runtime::execute"
        || record["author_library"]["serialization"] != "request-owned"
        || record["iso_writer"]["implementation"] != "project-owned-rust"
        || record["iso_writer"]["linkage"] != "embedded-rust-author"
    {
        return Err("Rust author requires its complete Rust library and retained shared FFmpeg/static menu dependencies".into());
    }
    let inputs = record["source_inputs"]
        .as_object()
        .ok_or("Rust author is missing source provenance")?;
    for path in [
        "rust/crates/dvda-author/src/command.rs",
        "rust/crates/dvda-author/src/aob.rs",
        "rust/crates/dvda-author/src/amg.rs",
        "rust/crates/dvda-author/src/asvs.rs",
        "rust/crates/dvda-author/src/atsi.rs",
        "rust/crates/dvda-author/src/iso.rs",
        "rust/crates/dvda-author/src/samg.rs",
        "rust/crates/dvda-author/src/menu.rs",
        "rust/crates/dvda-core/src/author_runtime.rs",
        "rust/crates/dvda-cli/src/author_main.rs",
        "rust/crates/dvda-menu/src/session.rs",
        "rust/crates/dvda-menu/src/resources.rs",
        "rust/crates/dvda-menu/src/direct.rs",
        "rust/crates/dvda-menu/src/lib.rs",
        "rust/crates/dvda-image/src/lib.rs",
        "rust/crates/dvda-image/src/fonts.rs",
    ] {
        let digest = inputs
            .get(path)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Rust author source is missing: {path}"))?;
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("Invalid Rust author source checksum: {path}"));
        }
    }
    let required: &[&str] = if static_ffmpeg {
        &[]
    } else {
        &[
            "avcodec-63.dll",
            "avformat-63.dll",
            "avutil-61.dll",
            "swresample-7.dll",
            "swscale-10.dll",
        ]
    };
    let (_, mut files) = component(
        author,
        "author-build.json",
        "runtime_files",
        required,
        false,
        true,
        target,
    )?;
    if files
        .keys()
        .any(|name| name.starts_with("dvda-") || name == "mlp_encoder.dll")
    {
        return Err("Rust author runtime contains a migrated project-owned DLL".into());
    }
    // The static-import check moved to the shipped executable (`validate_shipped`).
    if static_ffmpeg && !files.is_empty() {
        return Err("Static author must not depend on bundled DLLs".into());
    }
    // Authenticate the standalone acceptance executable without packaging it:
    // the GUI invokes the Rust author library inside its own process.
    let assets = record["runtime_assets"]
        .as_object()
        .ok_or("Rust author is missing runtime asset inventory")?;
    for required in [
        "fonts/DvdaNotoCJK-Regular.ttc",
        "data/menu/activeheader",
        "data/menu/black_NTSC_720x480.jpg",
        "data/menu/black_NTSC_720x480.png",
        "data/menu/black_PAL_720x576.jpg",
        "data/menu/black_PAL_720x576.png",
        "data/menu/silence.wav",
    ] {
        if !assets.contains_key(required) {
            return Err(format!("Rust author runtime asset is missing: {required}"));
        }
    }
    for (name, metadata) in assets {
        if name.contains('\\')
            || name
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == ".." || part.contains(':'))
        {
            return Err(format!("Invalid Rust author asset path: {name}"));
        }
        let path = author.join(name);
        let bytes = fs::read(&path).map_err(err)?;
        if metadata["bytes"].as_u64() != Some(bytes.len() as u64)
            || metadata["sha256"].as_str() != Some(hash(&bytes).as_str())
        {
            return Err(format!("Rust author asset checksum mismatch: {name}"));
        }
        files.insert(name.clone(), path);
    }
    Ok(Inputs {
        files,
        records: vec![author.join("author-build.json")],
    })
}

fn component(
    directory: &Path,
    manifest: &str,
    field: &str,
    required: &[&str],
    single: bool,
    api_sets: bool,
    target: &str,
) -> Result<(Value, BTreeMap<String, PathBuf>), String> {
    let path = directory.join(manifest);
    let record = read_json(&path)?;
    let metadata = record[field]
        .as_object()
        .ok_or_else(|| format!("{manifest}: missing {field}"))?;
    let mut libraries = serde_json::Map::new();
    for (name, entry) in metadata {
        if record["implementation"] == "project-owned-rust" && !name.as_bytes().ends_with(b".dll") {
            if name.contains(['/', '\\']) || name == "." || name == ".." {
                return Err(format!("{manifest}: invalid artifact name {name}"));
            }
            let path = directory.join(name);
            let bytes = fs::read(&path).map_err(err)?;
            if entry["bytes"].as_u64() != Some(bytes.len() as u64)
                || entry["sha256"]
                    .as_str()
                    .is_none_or(|digest| !hash(&bytes).eq_ignore_ascii_case(digest))
            {
                return Err(format!("Artifact checksum mismatch: {name}"));
            }
            if name.as_bytes().ends_with(b".exe") {
                validate_binary(&path, entry, false, target)?;
            }
        } else {
            libraries.insert(name.clone(), entry.clone());
        }
    }
    let metadata = &libraries;
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
        .filter_map(|(name, kind)| {
            (name.as_bytes().ends_with(b".dll") && kind.is_file()).then_some(name)
        })
        .collect();
    if actual != files.keys().cloned().collect() {
        return Err(format!(
            "{manifest}: actual DLL files differ from the manifest"
        ));
    }
    for (name, metadata) in metadata {
        let imports = validate_binary(&directory.join(name), metadata, true, target)?;
        closure(name, &imports, &files, api_sets)?;
    }
    Ok((record, files))
}

pub fn validate_binary(
    path: &Path,
    metadata: &Value,
    dll: bool,
    target: &str,
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
    imports_for(&bytes, dll, target).map_err(|e| format!("{}: {e}", path.display()))
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

pub fn validate_staged(directory: &Path, target: &str) -> Result<(), String> {
    validate_combined(&[directory], target)
}

pub fn validate_combined(directories: &[&Path], target: &str) -> Result<(), String> {
    target_machine(target)?;
    let mut files = BTreeMap::new();
    for directory in directories {
        for entry in fs::read_dir(directory).map_err(err)? {
            let path = entry.map_err(err)?.path();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            if path.is_file()
                && (name.as_bytes().ends_with(b".dll") || name.as_bytes().ends_with(b".exe"))
                && let Some(existing) = files.insert(name, path.clone())
                && fs::read(existing).map_err(err)? != fs::read(&path).map_err(err)?
            {
                return Err("Conflicting combined runtime binaries".into());
            }
        }
    }
    for (name, path) in &files {
        let imports = imports_for(
            &fs::read(path).map_err(err)?,
            name.as_bytes().ends_with(b".dll"),
            target,
        )
        .map_err(|e| format!("{}: {e}", path.display()))?;
        closure(name, &imports, &files, true)?;
    }
    Ok(())
}

fn system_dependency(name: &str, api_sets: bool) -> bool {
    if api_sets && (name.starts_with("api-ms-win-") || name.starts_with("ext-ms-win-")) {
        return true;
    }
    // FFmpeg is built as a shared MSVC DLL. Its Microsoft CRT must be copied
    // into the embedded runtime even when this machine has the same DLL in
    // System32; otherwise a clean Windows installation cannot start it.
    if name.starts_with("vcruntime140")
        || name.starts_with("msvcp140")
        || name.starts_with("concrt140")
    {
        return false;
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

/// The shipped GUI must be self-contained: every import is a Windows system DLL.
/// The Rust author, the media and image libraries, and the menu archives are linked in.
pub fn validate_shipped(executable: &Path, target: &str) -> Result<(), String> {
    let bytes = fs::read(executable).map_err(err)?;
    let imports = imports_for(&bytes, false, target)
        .map_err(|error| format!("{}: {error}", executable.display()))?;
    if let Some(name) = imports.iter().find(|name| !system_dependency(name, true)) {
        return Err(format!("Shipped executable imports a bundled DLL: {name}"));
    }
    Ok(())
}

pub fn hash(bytes: &[u8]) -> String {
    dvda_core::hash::sha256(bytes)
        .iter()
        .fold(String::new(), |mut text, value| {
            use std::fmt::Write as _;
            let _ = write!(text, "{value:02x}");
            text
        })
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

    fn rust_author_fixture() -> Fixture {
        let fixture = Fixture::new();
        fixture.edit("image", |record| {
            record["implementation"] = "project-owned-rust".into();
        });
        let author = fixture.root.join("author");
        let mut libraries = serde_json::Map::new();
        for name in [
            "avcodec-63.dll",
            "avformat-63.dll",
            "avutil-61.dll",
            "swresample-7.dll",
            "swscale-10.dll",
        ] {
            let bytes = pe::fixture(true, &["kernel32.dll"], &[]);
            fs::write(author.join(name), &bytes).unwrap();
            libraries.insert(
                name.into(),
                serde_json::json!({"bytes":bytes.len(),"sha256":hash(&bytes)}),
            );
        }
        let mut sources = serde_json::Map::new();
        for path in [
            "rust/crates/dvda-author/src/command.rs",
            "rust/crates/dvda-author/src/aob.rs",
            "rust/crates/dvda-author/src/amg.rs",
            "rust/crates/dvda-author/src/asvs.rs",
            "rust/crates/dvda-author/src/atsi.rs",
            "rust/crates/dvda-author/src/iso.rs",
            "rust/crates/dvda-author/src/samg.rs",
            "rust/crates/dvda-author/src/menu.rs",
            "rust/crates/dvda-core/src/author_runtime.rs",
            "rust/crates/dvda-cli/src/author_main.rs",
            "rust/crates/dvda-menu/src/session.rs",
            "rust/crates/dvda-menu/src/resources.rs",
            "rust/crates/dvda-menu/src/direct.rs",
            "rust/crates/dvda-menu/src/lib.rs",
            "rust/crates/dvda-image/src/lib.rs",
            "rust/crates/dvda-image/src/fonts.rs",
        ] {
            sources.insert(path.into(), hash(b"Rust source").into());
        }
        let mut assets = serde_json::Map::new();
        for name in [
            "fonts/DvdaNotoCJK-Regular.ttc",
            "data/menu/activeheader",
            "data/menu/black_NTSC_720x480.jpg",
            "data/menu/black_NTSC_720x480.png",
            "data/menu/black_PAL_720x576.jpg",
            "data/menu/black_PAL_720x576.png",
            "data/menu/silence.wav",
        ] {
            let path = author.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"asset").unwrap();
            assets.insert(
                name.into(),
                serde_json::json!({"bytes":5,"sha256":hash(b"asset")}),
            );
        }
        fixture.edit("author", |record| {
            record["schema_version"]=1.into();record["implementation"]="rust".into();record["target"]="x86_64-pc-windows-msvc".into();record["toolchain"]="msvc".into();
            record["project_c_author_compiled"]=false.into();record["menu_linkage"]="in-process-rust".into();
            record["author_library"]=serde_json::json!({"implementation":"rust","available":true,"entry":"dvda_core::author_runtime::execute","serialization":"request-owned"});
            record["iso_writer"]=serde_json::json!({"implementation":"project-owned-rust","linkage":"embedded-rust-author"});
            record["source_inputs"]=sources.into();record["runtime_assets"]=assets.into();record["runtime_files"]=libraries.into();
            record["files"]=serde_json::json!({});
        });
        fixture
    }

    #[test]
    fn static_package_checks_archives_and_rejects_dll_imports() {
        let fixture = rust_author_fixture();
        let media = fixture.root.join("media");
        fs::create_dir(media.join("lib")).unwrap();
        let mut archives = serde_json::Map::new();
        for name in [
            "avcodec",
            "avformat",
            "avutil",
            "swresample",
            "swscale",
            "soxr",
        ] {
            let name = format!("{name}.lib");
            let bytes = b"!<arch>\nfixture";
            fs::write(media.join("lib").join(&name), bytes).unwrap();
            archives.insert(
                name,
                serde_json::json!({"bytes":bytes.len(),"sha256":hash(bytes)}),
            );
        }
        let manifest = serde_json::to_vec(&serde_json::json!({
            "target":"x86_64-pc-windows-msvc","toolchain":"msvc","profile":"static", "linkage":"static","crt":"static","files":archives
        })).unwrap();
        fs::write(media.join("build-manifest.json"), &manifest).unwrap();
        for path in fs::read_dir(fixture.root.join("author")).unwrap() {
            let path = path.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "dll") {
                fs::remove_file(path).unwrap();
            }
        }
        fixture.edit("author", |record| {
            record["ffmpeg_linkage"] = "static-source-built".into();
            record["ffmpeg_profile"] = "build-msvc-ffmpeg.py:static".into();
            record["ffmpeg_manifest"] = serde_json::json!({"sha256":hash(&manifest)});
            record["runtime_files"] = serde_json::json!({});
            record["files"] = serde_json::json!({});
        });
        fs::remove_dir_all(fixture.root.join("image")).unwrap();
        assert!(
            fixture
                .validate()
                .unwrap()
                .files
                .keys()
                .all(|name| !name.as_bytes().ends_with(b".dll"))
        );
        fs::write(media.join("lib/avcodec.lib"), b"tampered").unwrap();
        assert!(fixture.validate().unwrap_err().contains("archive checksum"));
    }

    #[test]
    fn complete_rust_author_authenticates_sources_assets_and_delay_import_closure() {
        let fixture = rust_author_fixture();
        let inputs = fixture.validate().unwrap();
        assert!(inputs.files.contains_key("fonts/DvdaNotoCJK-Regular.ttc"));
        assert!(inputs.files.contains_key("data/menu/silence.wav"));
        assert!(!inputs.files.contains_key("dvda-author-dev.exe"));
        assert!(
            !inputs
                .files
                .keys()
                .any(|name| name.starts_with("dvda-menu-"))
        );
        fs::write(
            fixture.root.join("author/data/menu/activeheader"),
            b"tampered",
        )
        .unwrap();
        assert!(fixture.validate().unwrap_err().contains("asset checksum"));
    }

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
                ("encoder", "encoder-build.json", vec!["mlp_encoder.dll"]),
                (
                    "author",
                    "author-build.json",
                    vec!["avcodec-63.dll", "avformat-63.dll", "avutil-61.dll"],
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
                let mut record = serde_json::json!({"profile":"shared","files":files});
                if dir == "encoder" {
                    record["implementation"] = "rust".into();
                    record["abi_version"] = 1.into();
                }
                if dir == "author" {
                    record["runtime_files"] = record["files"].take();
                    record["files"] = serde_json::json!({});
                    record["ffmpeg_linkage"] = "shared-source-built-shared-profile".into();
                    record["ffmpeg_profile"] = "build-msvc-ffmpeg.py:shared".into();
                    record["menu_linkage"] = "in-process-rust".into();
                    record["source_inputs"] = serde_json::json!({});
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
                "x86_64-pc-windows-msvc",
            )
        }
        pub fn edit(&self, dir: &str, change: impl FnOnce(&mut Value)) {
            let path = self.root.join(dir).join(format!("{dir}-build.json"));
            let mut d: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            change(&mut d);
            fs::write(path, serde_json::to_vec(&d).unwrap()).unwrap();
        }
        pub fn replace(&self, dir: &str, name: &str, bytes: &[u8]) {
            fs::write(self.root.join(dir).join(name), bytes).unwrap();
            self.edit(dir, |d| {
                d[if dir == "author" {
                    "runtime_files"
                } else {
                    "files"
                }][name] = serde_json::json!({"bytes":bytes.len(),"sha256":hash(bytes)});
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
        assert_eq!(f.validate().unwrap().files.len(), 7);
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
                d["files"][name] = d["files"]["dvda-media.dll"].clone();
            });
            assert!(f.validate().is_err());
        }
    }
}
