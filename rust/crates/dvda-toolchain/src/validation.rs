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

fn read_json(path: &Path) -> Result<Value, String> {
    let mut bytes = fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes.drain(..3);
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid {}: {e}", path.display()))
}

pub fn validate(media: &Path, image: &Path, author: &Path) -> Result<Inputs, String> {
    let author_record = read_json(&author.join("author-build.json"))?;
    if author_record["implementation"] == "rust"
        || author_record["menu_linkage"] == "direct-static-vendor"
    {
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
            false,
        )?;
        if media_record["profile"] != "shared" {
            return Err("Onefile media runtime must use profile=shared".into());
        }
        media_files.remove("dvda-media.dll");
        let (image_record, _) = component(
            image,
            "image-build.json",
            "files",
            &["dvda-image.dll"],
            true,
            true,
        )?;
        if image_record["implementation"] != "project-owned-rust" {
            return Err("Image provenance must identify the Rust implementation".into());
        }
        let mut linked = if author_record["implementation"] == "rust" {
            validate_rust_author(author, &author_record)?
        } else {
            validate_linked(author, &author_record)?
        };
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
        linked.records.push(image.join("image-build.json"));
        return Ok(linked);
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
    let (author_record, author_files) = component(
        author,
        "author-build.json",
        "runtime_files",
        &[
            "avcodec-63.dll",
            "avformat-63.dll",
            "avutil-61.dll",
            "dvda-menu-spu.dll",
            "dvda-menu-nav.dll",
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
    let menu_path = author.join("menu-build.json");
    let menu_bytes =
        fs::read(&menu_path).map_err(|e| format!("Cannot read {}: {e}", menu_path.display()))?;
    let expected = author_record["source_inputs"]["menu-runtime/menu-build.json"]
        .as_str()
        .ok_or("Author is missing authenticated menu provenance")?;
    if !hash(&menu_bytes).eq_ignore_ascii_case(expected) {
        return Err("Author menu manifest checksum mismatch".into());
    }
    let menu_record: Value = serde_json::from_slice(&menu_bytes)
        .map_err(|e| format!("Invalid {}: {e}", menu_path.display()))?;
    if menu_record["adapter"] != "rust" {
        return Err("Menu libraries must use the source-built Rust adapter".into());
    }
    let menu_files = menu_record["files"]
        .as_object()
        .ok_or("Menu manifest is missing its DLL inventory")?;
    if menu_files.len() != 2
        || !["dvda-menu-spu.dll", "dvda-menu-nav.dll"]
            .iter()
            .all(|name| menu_files.contains_key(*name))
    {
        return Err("Menu manifest must contain exactly the SPU and navigation DLLs".into());
    }
    for name in ["dvda-menu-spu.dll", "dvda-menu-nav.dll"] {
        validate_binary(&author.join(name), &menu_record["files"][name], true)?;
    }
    let mut author_files = author_files;
    // Legacy manifests may record the old verifier. Authenticate it in its original
    // directory, but check the executable against the DLLs actually shipped.
    author_files.remove("dvda-disc-verify.dll");
    for (name, path) in &author_files {
        let imports = pe::imports(&fs::read(path).map_err(err)?, true)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        closure(name, &imports, &author_files, true)?;
    }
    let author_exe = author.join("dvda-author-dev.exe");
    let imports = validate_binary(
        &author_exe,
        &author_record["files"]["dvda-author-dev.exe"],
        false,
    )?;
    closure("dvda-author-dev.exe", &imports, &author_files, true)?;
    author_files.insert("dvda-author-dev.exe".into(), author_exe);
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
        (author, "menu-build.json"),
    ] {
        inputs.records.push(dir.join(name));
    }
    Ok(inputs)
}

fn validate_rust_author(author: &Path, record: &Value) -> Result<Inputs, String> {
    if record["schema_version"] != 1
        || record["target"] != "x86_64-pc-windows-gnu"
        || record["project_c_author_compiled"] != false
        || record["menu_linkage"] != "direct-static-vendor"
        || record["ffmpeg_linkage"] != "shared-source-built-shared-profile"
        || record["ffmpeg_profile"] != "build-minimal-ffmpeg.py:shared"
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
    ] {
        let digest = inputs
            .get(path)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Rust author source is missing: {path}"))?;
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("Invalid Rust author source checksum: {path}"));
        }
    }
    let menu_path = author.join("menu-build.json");
    let menu_bytes = fs::read(&menu_path).map_err(err)?;
    if inputs
        .get("menu-runtime/menu-build.json")
        .and_then(Value::as_str)
        != Some(hash(&menu_bytes).as_str())
    {
        return Err("Rust author menu provenance checksum mismatch".into());
    }
    let menu = read_json(&menu_path)?;
    if menu["adapter"] != "rust" || menu["linkage"] != "static-vendor" {
        return Err("Rust author must embed the source-built Rust menu adapter".into());
    }
    if menu["session"]["implementation"] != "rust"
        || menu["session"]["boundary"] != "c-setjmp-varargs"
        || menu["state_reset"]["implementation"] != "rust"
    {
        return Err("Rust author must use the migrated Rust menu session and state reset".into());
    }
    for path in [
        "rust/crates/dvda-menu/src/session.rs",
        "rust/crates/dvda-menu/src/resources.rs",
        "rust/crates/dvda-menu/src/direct.rs",
        "rust/crates/dvda-menu/src/lib.rs",
    ] {
        if menu["rust_inputs"][path].as_str() != inputs.get(path).and_then(Value::as_str) {
            return Err(format!("Rust author/menu source checksum mismatch: {path}"));
        }
    }
    for name in ["libdvda_menu_spu_vendor.a", "libdvda_menu_nav_vendor.a"] {
        if inputs
            .get(&format!("menu-runtime/{name}"))
            .and_then(Value::as_str)
            != menu["files"][name]["sha256"].as_str()
            || menu["files"][name]["sha256"].as_str().is_none()
        {
            return Err(format!(
                "Rust author embedded menu archive checksum mismatch: {name}"
            ));
        }
    }
    let (_, mut files) = component(
        author,
        "author-build.json",
        "runtime_files",
        &[
            "avcodec-63.dll",
            "avformat-63.dll",
            "avutil-61.dll",
            "swresample-7.dll",
            "swscale-10.dll",
        ],
        false,
        true,
    )?;
    if files
        .keys()
        .any(|name| name.starts_with("dvda-") || name == "mlp_encoder.dll")
    {
        return Err("Rust author runtime contains a migrated project-owned DLL".into());
    }
    let exe = author.join("dvda-author-dev.exe");
    let imports = validate_binary(&exe, &record["files"]["dvda-author-dev.exe"], false)?;
    closure("dvda-author-dev.exe", &imports, &files, true)?;
    // Authenticate the standalone acceptance executable without packaging it:
    // the GUI invokes the Rust author library inside its own process.
    let assets = record["runtime_assets"]
        .as_object()
        .ok_or("Rust author is missing runtime asset inventory")?;
    for required in [
        "policy.xml",
        "colors.xml",
        "type.xml",
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
        records: vec![author.join("author-build.json"), menu_path],
    })
}

fn validate_linked(author: &Path, record: &Value) -> Result<Inputs, String> {
    if record["ffmpeg_linkage"] != "shared-source-built-shared-profile"
        || record["ffmpeg_profile"] != "build-minimal-ffmpeg.py:shared"
    {
        return Err("Linked author requires the authenticated shared FFmpeg profile".into());
    }
    let mut records = vec![author.join("author-build.json")];
    let mut authenticated = Vec::new();
    for (name, key) in [
        ("author-bridge-build.json", "rust/author-bridge-build.json"),
        ("menu-build.json", "menu-runtime/menu-build.json"),
    ] {
        let path = author.join(name);
        let bytes = fs::read(&path).map_err(err)?;
        if record["source_inputs"][key].as_str() != Some(hash(&bytes).as_str()) {
            return Err(format!("Linked author provenance mismatch: {name}"));
        }
        authenticated.push(read_json(&path)?);
        records.push(path);
    }
    let bridge = &authenticated[0];
    let menu = &authenticated[1];
    let features: BTreeSet<_> = bridge["features"]
        .as_array()
        .ok_or("Missing bridge features")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    if bridge["schema_version"] != 1
        || bridge["implementation"] != "project-owned-rust"
        || bridge["component"] != "author"
        || bridge["target"] != "x86_64-pc-windows-gnu"
        || features != BTreeSet::from(["media", "image", "menu"])
        || menu["adapter"] != "rust"
        || menu["linkage"] != "static-vendor"
    {
        return Err("Author must statically link Rust media, image and menu".into());
    }
    for name in ["libdvda_menu_spu_vendor.a", "libdvda_menu_nav_vendor.a"] {
        let digest = menu["files"][name]["sha256"]
            .as_str()
            .ok_or("Missing menu archive hash")?;
        let matches: Vec<_> = bridge["source_inputs"]
            .as_object()
            .ok_or("Missing bridge inputs")?
            .iter()
            .filter(|(path, _)| path.replace('\\', "/").rsplit('/').next() == Some(name))
            .map(|(_, value)| value.as_str())
            .collect();
        if matches != vec![Some(digest)] {
            return Err(format!("Embedded menu archive provenance mismatch: {name}"));
        }
    }
    let (_, mut files) = component(
        author,
        "author-build.json",
        "runtime_files",
        &[
            "avcodec-63.dll",
            "avformat-63.dll",
            "avutil-61.dll",
            "swresample-7.dll",
            "swscale-10.dll",
        ],
        false,
        true,
    )?;
    if files
        .keys()
        .any(|name| name.starts_with("dvda-") || name == "mlp_encoder.dll")
    {
        return Err("Linked runtime contains a migrated project-owned DLL".into());
    }
    let exe = author.join("dvda-author-dev.exe");
    let imports = validate_binary(&exe, &record["files"]["dvda-author-dev.exe"], false)?;
    closure("dvda-author-dev.exe", &imports, &files, true)?;
    files.insert("dvda-author-dev.exe".into(), exe);
    Ok(Inputs { files, records })
}

pub fn validate_encoder(directory: &Path) -> Result<Inputs, String> {
    let (record, files) = component(
        directory,
        "encoder-build.json",
        "files",
        &["mlp_encoder.dll"],
        true,
        true,
    )?;
    if record["implementation"] != "rust" || record["abi_version"].as_u64() != Some(1) {
        return Err(
            "Encoder must use the source-built Rust implementation with ABI version 1".into(),
        );
    }
    Ok(Inputs {
        files,
        records: vec![directory.join("encoder-build.json")],
    })
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
    let record = read_json(&path)?;
    let metadata = record[field]
        .as_object()
        .ok_or_else(|| format!("{manifest}: missing {field}"))?;
    let mut libraries = serde_json::Map::new();
    for (name, entry) in metadata {
        if record["implementation"] == "project-owned-rust" && !name.ends_with(".dll") {
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
            if name.ends_with(".exe") {
                validate_binary(&path, entry, false)?;
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
    validate_combined(&[directory])
}

pub fn validate_combined(directories: &[&Path]) -> Result<(), String> {
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
                && (name.ends_with(".dll") || name.ends_with(".exe"))
                && let Some(existing) = files.insert(name, path.clone())
                && fs::read(existing).map_err(err)? != fs::read(&path).map_err(err)?
            {
                return Err("Conflicting combined runtime binaries".into());
            }
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

    fn rust_author_fixture() -> Fixture {
        let fixture = Fixture::new();
        fixture.edit("image", |record| {
            record["implementation"] = "project-owned-rust".into()
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
        for name in ["dvda-menu-spu.dll", "dvda-menu-nav.dll"] {
            fs::remove_file(author.join(name)).unwrap();
        }
        let bytes = pe::fixture(false, &["avcodec-63.dll"], &["avformat-63.dll"]);
        fs::write(author.join("dvda-author-dev.exe"), &bytes).unwrap();
        let digest = hash(b"retained static menu archive");
        let rust_inputs: serde_json::Map<String, Value> = [
            "rust/crates/dvda-menu/src/session.rs",
            "rust/crates/dvda-menu/src/resources.rs",
            "rust/crates/dvda-menu/src/direct.rs",
            "rust/crates/dvda-menu/src/lib.rs",
        ]
        .into_iter()
        .map(|path| (path.into(), hash(b"Rust source").into()))
        .collect();
        let menu = serde_json::json!({"adapter":"rust","linkage":"static-vendor",
            "session":{"implementation":"rust","boundary":"c-setjmp-varargs"},
            "state_reset":{"implementation":"rust"},"rust_inputs":rust_inputs,"files":{
            "libdvda_menu_spu_vendor.a":{"sha256":digest},"libdvda_menu_nav_vendor.a":{"sha256":digest}
        }});
        let menu_bytes = serde_json::to_vec(&menu).unwrap();
        fs::write(author.join("menu-build.json"), &menu_bytes).unwrap();
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
        ] {
            sources.insert(path.into(), hash(b"Rust source").into());
        }
        sources.insert(
            "menu-runtime/menu-build.json".into(),
            hash(&menu_bytes).into(),
        );
        for name in ["libdvda_menu_spu_vendor.a", "libdvda_menu_nav_vendor.a"] {
            sources.insert(format!("menu-runtime/{name}"), digest.clone().into());
        }
        let mut assets = serde_json::Map::new();
        for name in [
            "policy.xml",
            "colors.xml",
            "type.xml",
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
            record["schema_version"]=1.into();record["implementation"]="rust".into();record["target"]="x86_64-pc-windows-gnu".into();
            record["project_c_author_compiled"]=false.into();record["menu_linkage"]="direct-static-vendor".into();
            record["author_library"]=serde_json::json!({"implementation":"rust","available":true,"entry":"dvda_core::author_runtime::execute","serialization":"request-owned"});
            record["iso_writer"]=serde_json::json!({"implementation":"project-owned-rust","linkage":"embedded-rust-author"});
            record["source_inputs"]=sources.into();record["runtime_assets"]=assets.into();record["runtime_files"]=libraries.into();
            record["files"]=serde_json::json!({"dvda-author-dev.exe":{"bytes":bytes.len(),"sha256":hash(&bytes)}});
        });
        fixture
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
        fs::write(fixture.root.join("author/policy.xml"), b"tampered").unwrap();
        assert!(fixture.validate().unwrap_err().contains("asset checksum"));
    }

    #[test]
    fn rust_author_rejects_incomplete_migration_or_unauthenticated_menu() {
        let fixture = rust_author_fixture();
        fixture.edit("author", |record| {
            record["iso_writer"]["linkage"] = "compiled-c".into()
        });
        assert!(
            fixture
                .validate()
                .unwrap_err()
                .contains("complete Rust library")
        );
        let fixture = rust_author_fixture();
        fixture.edit("author", |record| {
            record["project_c_author_compiled"] = true.into()
        });
        assert!(
            fixture
                .validate()
                .unwrap_err()
                .contains("complete Rust library")
        );
        let fixture = rust_author_fixture();
        fixture.edit("author", |record| {
            record["source_inputs"]
                .as_object_mut()
                .unwrap()
                .remove("rust/crates/dvda-author/src/command.rs");
        });
        assert!(
            fixture
                .validate()
                .unwrap_err()
                .contains("source is missing")
        );
        let fixture = rust_author_fixture();
        fixture.edit("author", |record| {
            record["source_inputs"]["menu-runtime/libdvda_menu_spu_vendor.a"] =
                "0".repeat(64).into()
        });
        assert!(
            fixture
                .validate()
                .unwrap_err()
                .contains("embedded menu archive")
        );
    }
    #[test]
    fn rust_author_rejects_legacy_session_reset_and_stale_menu_rust_sources() {
        for component in ["session", "state_reset"] {
            let fixture = rust_author_fixture();
            fixture.edit_menu(|menu| menu[component]["implementation"] = "c".into());
            assert!(
                fixture
                    .validate()
                    .unwrap_err()
                    .contains("migrated Rust menu"),
                "{component}"
            );
        }
        let fixture = rust_author_fixture();
        fixture.edit_menu(|menu| {
            menu["session"]["boundary"] = "c-session".into();
        });
        assert!(
            fixture
                .validate()
                .unwrap_err()
                .contains("migrated Rust menu")
        );
        let fixture = rust_author_fixture();
        fixture.edit_menu(|menu| {
            menu["rust_inputs"]["rust/crates/dvda-menu/src/session.rs"] = "0".repeat(64).into();
        });
        assert!(
            fixture
                .validate()
                .unwrap_err()
                .contains("source checksum mismatch")
        );
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
                    vec![
                        "avcodec-63.dll",
                        "avformat-63.dll",
                        "avutil-61.dll",
                        "dvda-menu-spu.dll",
                        "dvda-menu-nav.dll",
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
                let mut record = serde_json::json!({"profile":"shared","files":files});
                if dir == "encoder" {
                    record["implementation"] = "rust".into();
                    record["abi_version"] = 1.into();
                }
                if dir == "author" {
                    record["runtime_files"] = record["files"].take();
                    let b = pe::fixture(false, &["avcodec-63.dll"], &["dvda-menu-spu.dll"]);
                    fs::write(folder.join("dvda-author-dev.exe"), &b).unwrap();
                    record["files"] = serde_json::json!({"dvda-author-dev.exe":{"bytes":b.len(),"sha256":hash(&b)}});
                    record["ffmpeg_linkage"] = "shared-source-built-shared-profile".into();
                    record["ffmpeg_profile"] = "build-minimal-ffmpeg.py:shared".into();
                    record["menu_linkage"] = "in-process-source-built".into();
                    let menu_record = serde_json::json!({
                        "adapter": "rust",
                        "files": {
                            "dvda-menu-spu.dll": record["runtime_files"]["dvda-menu-spu.dll"],
                            "dvda-menu-nav.dll": record["runtime_files"]["dvda-menu-nav.dll"],
                        }
                    });
                    let menu_bytes = serde_json::to_vec(&menu_record).unwrap();
                    record["source_inputs"] = serde_json::json!({
                        "menu-runtime/menu-build.json": hash(&menu_bytes)
                    });
                    fs::write(folder.join("menu-build.json"), menu_bytes).unwrap();
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
            )
        }
        pub fn edit(&self, dir: &str, change: impl FnOnce(&mut Value)) {
            let path = self.root.join(dir).join(format!("{dir}-build.json"));
            let mut d: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            change(&mut d);
            fs::write(path, serde_json::to_vec(&d).unwrap()).unwrap();
        }
        pub fn edit_menu(&self, change: impl FnOnce(&mut Value)) {
            let path = self.root.join("author/menu-build.json");
            let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            change(&mut record);
            let bytes = serde_json::to_vec(&record).unwrap();
            fs::write(path, &bytes).unwrap();
            self.edit("author", |d| {
                d["source_inputs"]["menu-runtime/menu-build.json"] = hash(&bytes).into();
            });
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
            if dir == "author" && name.starts_with("dvda-menu-") {
                self.edit_menu(|record| {
                    record["files"][name] =
                        serde_json::json!({"bytes":bytes.len(),"sha256":hash(&bytes)});
                });
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn encoder_requires_rust_abi_integrity_and_closed_inventory() {
        let f = Fixture::new();
        let directory = f.root.join("encoder");
        assert_eq!(validate_encoder(&directory).unwrap().files.len(), 1);
        for (field, value) in [
            ("implementation", serde_json::json!("c")),
            ("abi_version", serde_json::json!(2)),
        ] {
            f.edit("encoder", |d| d[field] = value);
            assert!(
                validate_encoder(&directory)
                    .unwrap_err()
                    .contains("ABI version 1")
            );
            f.edit("encoder", |d| {
                d["implementation"] = "rust".into();
                d["abi_version"] = 1.into();
            });
        }
        fs::write(directory.join("mlp_encoder.dll"), b"tampered").unwrap();
        assert!(
            validate_encoder(&directory)
                .unwrap_err()
                .contains("checksum")
        );
        f.replace(
            "encoder",
            "mlp_encoder.dll",
            pe::fixture(true, &["missing.dll"], &[]),
        );
        assert!(
            validate_encoder(&directory)
                .unwrap_err()
                .contains("Missing native dependency")
        );
        f.replace(
            "encoder",
            "mlp_encoder.dll",
            pe::fixture(true, &[], &["missing.dll"]),
        );
        assert!(
            validate_encoder(&directory)
                .unwrap_err()
                .contains("Missing native dependency")
        );
        f.replace("encoder", "mlp_encoder.dll", pe::fixture(true, &[], &[]));
        fs::write(directory.join("extra.dll"), pe::fixture(true, &[], &[])).unwrap();
        assert!(
            validate_encoder(&directory)
                .unwrap_err()
                .contains("actual DLL files")
        );
        fs::remove_file(directory.join("extra.dll")).unwrap();
        fs::remove_file(directory.join("encoder-build.json")).unwrap();
        assert!(
            validate_encoder(&directory)
                .unwrap_err()
                .contains("Cannot read")
        );
    }

    #[test]
    fn encoder_rejects_missing_malformed_and_non_x64_dlls() {
        let f = Fixture::new();
        let directory = f.root.join("encoder");
        fs::remove_file(directory.join("mlp_encoder.dll")).unwrap();
        assert!(validate_encoder(&directory).is_err());

        let mut x86 = pe::fixture(true, &[], &[]);
        x86[132..134].copy_from_slice(&0x14cu16.to_le_bytes());
        for bytes in [b"not a PE".to_vec(), x86] {
            let f = Fixture::new();
            f.replace("encoder", "mlp_encoder.dll", bytes);
            assert!(validate_encoder(&f.root.join("encoder")).is_err());
        }

        let f = Fixture::new();
        f.edit("encoder", |record| {
            record["files"]["extra.dll"] = record["files"]["mlp_encoder.dll"].clone();
        });
        assert!(validate_encoder(&f.root.join("encoder")).is_err());
    }

    #[test]
    fn menu_requires_authenticated_rust_provenance_and_matching_inventory() {
        let f = Fixture::new();
        assert!(
            f.validate()
                .unwrap()
                .records
                .contains(&f.root.join("author/menu-build.json"))
        );
        f.edit("author", |record| {
            record["source_inputs"]
                .as_object_mut()
                .unwrap()
                .remove("menu-runtime/menu-build.json");
        });
        assert!(
            f.validate()
                .unwrap_err()
                .contains("authenticated menu provenance")
        );

        let f = Fixture::new();
        fs::write(f.root.join("author/menu-build.json"), b"tampered").unwrap();
        assert!(f.validate().unwrap_err().contains("manifest checksum"));

        let f = Fixture::new();
        fs::remove_file(f.root.join("author/menu-build.json")).unwrap();
        assert!(f.validate().unwrap_err().contains("Cannot read"));

        let f = Fixture::new();
        f.edit_menu(|record| record["adapter"] = "c".into());
        assert!(f.validate().unwrap_err().contains("Rust adapter"));

        for name in ["dvda-menu-spu.dll", "dvda-menu-nav.dll"] {
            let f = Fixture::new();
            f.edit_menu(|record| record["files"][name]["sha256"] = "0".repeat(64).into());
            assert!(f.validate().unwrap_err().contains("checksum"));
            let f = Fixture::new();
            f.edit_menu(|record| {
                record["files"].as_object_mut().unwrap().remove(name);
            });
            assert!(f.validate().unwrap_err().contains("exactly"));
        }
        let f = Fixture::new();
        f.edit_menu(|record| {
            record["files"]["extra.dll"] = record["files"]["dvda-menu-spu.dll"].clone()
        });
        assert!(f.validate().unwrap_err().contains("exactly"));
        let f = Fixture::new();
        f.edit_menu(|record| record["files"] = Value::Null);
        assert!(f.validate().unwrap_err().contains("DLL inventory"));

        let f = Fixture::new();
        fs::write(f.root.join("author/menu-build.json"), b"not JSON").unwrap();
        f.edit("author", |record| {
            record["source_inputs"]["menu-runtime/menu-build.json"] = hash(b"not JSON").into();
        });
        assert!(f.validate().unwrap_err().contains("Invalid"));
    }

    #[test]
    fn legacy_verifier_is_authenticated_but_never_shipped_or_used_as_import_closure() {
        let f = Fixture::new();
        let verifier = pe::fixture(true, &["kernel32.dll"], &[]);
        fs::write(f.root.join("author/dvda-disc-verify.dll"), &verifier).unwrap();
        f.edit("author", |record| {
            record["runtime_files"]["dvda-disc-verify.dll"] =
                serde_json::json!({"bytes":verifier.len(), "sha256":hash(&verifier)});
        });
        let files = f.validate().unwrap().files;
        assert!(!files.contains_key("dvda-disc-verify.dll"));
        f.replace(
            "author",
            "dvda-menu-nav.dll",
            pe::fixture(true, &["dvda-disc-verify.dll"], &[]),
        );
        assert!(
            f.validate()
                .unwrap_err()
                .contains("Missing native dependency")
        );
        f.replace("author", "dvda-menu-nav.dll", pe::fixture(true, &[], &[]));
        fs::write(f.root.join("author/dvda-disc-verify.dll"), b"tampered").unwrap();
        assert!(f.validate().unwrap_err().contains("checksum"));
    }

    #[test]
    fn author_executable_must_not_import_unshipped_legacy_verifier() {
        let f = Fixture::new();
        let executable = pe::fixture(false, &["dvda-disc-verify.dll"], &[]);
        fs::write(f.root.join("author/dvda-author-dev.exe"), &executable).unwrap();
        f.edit("author", |record| {
            record["files"]["dvda-author-dev.exe"] =
                serde_json::json!({"bytes":executable.len(),"sha256":hash(&executable)});
        });
        assert!(
            f.validate()
                .unwrap_err()
                .contains("Missing native dependency")
        );
    }

    #[test]
    fn manifest_integrity_and_exact_inventory() {
        let f = Fixture::new();
        assert_eq!(f.validate().unwrap().files.len(), 10);
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
        for dir in ["media", "image", "author"] {
            let name = match dir {
                "media" => "dvda-media.dll",
                "image" => "dvda-image.dll",
                _ => "dvda-menu-nav.dll",
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
        for name in ["dvda-menu-spu.dll", "dvda-menu-nav.dll"] {
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
