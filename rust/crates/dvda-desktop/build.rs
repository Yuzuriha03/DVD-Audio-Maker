use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    assert_eq!(
        env::var("TARGET").unwrap(),
        "x86_64-pc-windows-msvc",
        "DVD-Audio Maker supports only Windows x64 MSVC"
    );
    if env::var_os("CARGO_FEATURE_DIRECT_BRIDGES").is_some() && !static_ffmpeg() {
        for library in [
            "avformat-63.dll",
            "avcodec-63.dll",
            "avutil-61.dll",
            "swresample-7.dll",
            "swscale-10.dll",
        ] {
            println!("cargo:rustc-link-arg=/DELAYLOAD:{library}");
        }
    }
    println!("cargo:rerun-if-env-changed=DVDA_RUNTIME_ARCHIVE");
    println!("cargo:rerun-if-env-changed=DVDA_PRODUCT_VERSION");
    let generated = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let archive = env::var_os("DVDA_RUNTIME_ARCHIVE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let empty = generated.join("empty-runtime.bin");
            std::fs::write(&empty, []).unwrap();
            empty
        });
    println!("cargo:rerun-if-changed={}", archive.display());
    std::fs::write(
        generated.join("runtime.rs"),
        format!(
            "static EMBEDDED_RUNTIME: &[u8] = include_bytes!({:?});",
            archive
        ),
    )
    .unwrap();
    println!("cargo:rerun-if-changed=app.manifest");
    for name in [
        "RC",
        "PATH",
        "WindowsSdkDir",
        "WindowsSDKVersion",
        "WindowsSdkVerBinPath",
        "ProgramFiles(x86)",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let manifest = manifest_dir.join("app.manifest");
    let icon = manifest_dir.join("../../../assets/dvd-audio-maker.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let script = output.join("desktop.rc");
    let resource = output.join("desktop-resource.res");
    std::fs::write(
        &script,
        format!(
            "1 24 \"{}\"\n1 ICON \"{}\"\n{}",
            manifest.display().to_string().replace('\\', "/"),
            icon.display().to_string().replace('\\', "/"),
            version_resource()
        ),
    )
    .unwrap();
    let compiler = resource_compiler();
    let result = Command::new(&compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg(&script)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "Run Windows SDK resource compiler {}: {error}",
                compiler.display()
            )
        });
    assert!(
        result.status.success(),
        "Compile Windows desktop resources with {}: {}{}",
        compiler.display(),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    // The .res already contains the application manifest as resource 1.
    println!("cargo:rustc-link-arg=/MANIFEST:NO");
    println!("cargo:rustc-link-arg={}", resource.display());
}

fn static_ffmpeg() -> bool {
    println!("cargo:rerun-if-env-changed=DVDA_FFMPEG_PREFIX");
    let Some(prefix) = env::var_os("DVDA_FFMPEG_PREFIX") else {
        return false;
    };
    let manifest = PathBuf::from(prefix).join("build-manifest.json");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let bytes = std::fs::read(manifest).expect("FFmpeg build manifest required");
    let record: serde_json::Value =
        serde_json::from_slice(&bytes).expect("Invalid FFmpeg manifest");
    record["linkage"] == "static"
}

fn compiler_in_path(name: &Path) -> Option<PathBuf> {
    env::var_os("PATH")
        .into_iter()
        .flat_map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
}

fn resource_compiler() -> PathBuf {
    if let Some(configured) = env::var_os("RC") {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return path;
        }
        return compiler_in_path(&path).unwrap_or_else(|| {
            panic!(
                "RC does not name an installed Windows SDK resource compiler: {}",
                path.display()
            )
        });
    }
    if let Some(path) = compiler_in_path(Path::new("rc.exe")) {
        return path;
    }
    if let Some(directory) = env::var_os("WindowsSdkVerBinPath") {
        let path = PathBuf::from(directory).join("x64/rc.exe");
        if path.is_file() {
            return path;
        }
    }
    let mut roots = Vec::new();
    if let Some(directory) = env::var_os("WindowsSdkDir") {
        roots.push(PathBuf::from(directory).join("bin"));
    }
    let program_files =
        env::var_os("ProgramFiles(x86)").unwrap_or_else(|| "C:/Program Files (x86)".into());
    roots.push(PathBuf::from(program_files).join("Windows Kits/10/bin"));
    let mut candidates = Vec::new();
    for root in roots {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let version = entry
                    .file_name()
                    .to_string_lossy()
                    .split('.')
                    .map(str::parse::<u32>)
                    .collect::<Result<Vec<_>, _>>();
                let path = entry.path().join("x64/rc.exe");
                match version {
                    Ok(version) if path.is_file() => candidates.push((version, path)),
                    _ => {}
                }
            }
        }
    }
    candidates.sort_by(|a, b| a.0.cmp(&b.0));
    candidates.pop().map(|(_, path)| path).expect(
        "Windows SDK rc.exe was not found. Install Windows SDK with Visual Studio C++ tools, or set RC to its full path."
    )
}

/// `VS_VERSION_INFO` block so the executable carries company, product and version
/// metadata instead of shipping as an anonymous binary.
///
/// The packaging toolchain forwards the release version (`v1.0`) through
/// `DVDA_PRODUCT_VERSION`; a plain source build falls back to the crate version.
fn version_resource() -> String {
    let configured = env::var("DVDA_PRODUCT_VERSION").unwrap_or_default();
    let configured = configured.trim();
    let raw = if configured.is_empty() {
        env::var("CARGO_PKG_VERSION").unwrap()
    } else {
        configured.to_owned()
    };
    let [major, minor, patch, build] = version_quad(&raw);
    let dotted = format!("{major}.{minor}.{patch}.{build}");
    format!(
        "1 VERSIONINFO\n\
FILEVERSION {major},{minor},{patch},{build}\n\
PRODUCTVERSION {major},{minor},{patch},{build}\n\
FILEFLAGSMASK 0x3fL\n\
FILEFLAGS 0x0L\n\
FILEOS 0x40004L\n\
FILETYPE 0x1L\n\
FILESUBTYPE 0x0L\n\
BEGIN\n\
    BLOCK \"StringFileInfo\"\n\
    BEGIN\n\
        BLOCK \"040904b0\"\n\
        BEGIN\n\
            VALUE \"CompanyName\", \"Yuzuriha03\"\n\
            VALUE \"FileDescription\", \"DVD-Audio Maker\"\n\
            VALUE \"FileVersion\", \"{dotted}\"\n\
            VALUE \"InternalName\", \"DVD-Audio-Maker\"\n\
            VALUE \"LegalCopyright\", \"Copyright (C) Yuzuriha03 and DVD-Audio Maker contributors\"\n\
            VALUE \"OriginalFilename\", \"DVD-Audio-Maker.exe\"\n\
            VALUE \"ProductName\", \"DVD-Audio Maker\"\n\
            VALUE \"ProductVersion\", \"{dotted}\"\n\
        END\n\
    END\n\
    BLOCK \"VarFileInfo\"\n\
    BEGIN\n\
        VALUE \"Translation\", 0x409, 0x4b0\n\
    END\n\
END\n"
    )
}

/// `v1.1.0-rc.1` becomes `[1, 1, 0, 0]`, matching the packaging version validator.
fn version_quad(value: &str) -> [u16; 4] {
    let mut numbers = [0u16; 4];
    let core = value.trim().trim_start_matches(['v', 'V']);
    let core = core.split('-').next().unwrap_or_default();
    for (index, piece) in core.split('.').take(numbers.len()).enumerate() {
        numbers[index] = piece.parse().unwrap_or(0);
    }
    numbers
}
