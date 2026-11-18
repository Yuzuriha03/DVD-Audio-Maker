use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let target = env::var("TARGET").unwrap();
    assert!(
        matches!(
            target.as_str(),
            "x86_64-pc-windows-msvc" | "aarch64-pc-windows-msvc"
        ),
        "DVD-Audio Maker supports only Windows x64/ARM64 MSVC, got {target}"
    );
    println!("cargo:rerun-if-env-changed=DVDA_RUNTIME_ARCHIVE");
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let product_version = effective_product_version(&manifest_dir);
    println!("cargo:rustc-env=DVDA_DESKTOP_PRODUCT_VERSION={product_version}");
    println!("cargo:rerun-if-env-changed=DVDA_PRODUCT_VERSION");
    let generated = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let archive = env::var_os("DVDA_RUNTIME_ARCHIVE").map_or_else(
        || {
            let empty = generated.join("empty-runtime.bin");
            std::fs::write(&empty, []).unwrap();
            empty
        },
        PathBuf::from,
    );
    println!("cargo:rerun-if-changed={}", archive.display());
    #[allow(
        clippy::unnecessary_debug_formatting,
        reason = "Debug escapes the archive path as a Rust string literal for include_bytes!"
    )]
    std::fs::write(
        generated.join("runtime.rs"),
        format!("static EMBEDDED_RUNTIME: &[u8] = include_bytes!({archive:?});"),
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
    embed_licenses(&manifest_dir, &generated);
    let manifest = embed_manifest(&manifest_dir, &generated, &target);
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let icon = manifest_dir.join("../../../assets/dvd-audio-maker.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let script = output.join("desktop.rc");
    let resource = output.join("desktop-resource.res");
    std::fs::write(
        &script,
        format!(
            "1 24 \"{}\"\n1 ICON \"{}\"\n{}",
            manifest.display().to_string().replace('\\', "/"),
            icon.display().to_string().replace('\\', "/"),
            version_resource(&product_version)
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

/// Writes the application manifest with the architecture of the binary that
/// embeds it. The source manifest is shared; only the processor architecture of
/// the assembly identities may follow the Cargo target.
fn embed_manifest(manifest_dir: &Path, generated: &Path, target: &str) -> PathBuf {
    let architecture = if target.starts_with("aarch64") {
        "arm64"
    } else {
        "amd64"
    };
    let source = manifest_dir.join("app.manifest");
    let text = std::fs::read_to_string(&source).expect("read app.manifest");
    let destination = generated.join("app.manifest");
    std::fs::write(
        &destination,
        text.replace(
            "processorArchitecture=\"amd64\"",
            &format!("processorArchitecture=\"{architecture}\""),
        ),
    )
    .unwrap();
    destination
}

fn embed_licenses(manifest_dir: &Path, generated: &Path) {
    use std::fmt::Write as _;

    let repository = manifest_dir.join("../../..");
    let mut module = String::new();
    for (identifier, relative) in [
        ("PROJECT_LICENSE", "LICENSE"),
        ("FFMPEG_LICENSE", "FFMPEG-LICENSE.txt"),
        ("THIRD_PARTY_ZH", "tools/win-build/docs/THIRD-PARTY.txt"),
        ("THIRD_PARTY_EN", "tools/win-build/docs/THIRD-PARTY.en.txt"),
        ("IMAGE_NOTICES", "NOTICE-Image.txt"),
        ("MENU_NOTICES", "NOTICE-Menu.txt"),
    ] {
        let path = repository.join(relative);
        println!("cargo:rerun-if-changed={}", path.display());
        let escaped = path.display().to_string().replace('\\', "\\\\");
        writeln!(
            module,
            "pub(crate) static {identifier}: &str = include_str!(\"{escaped}\");"
        )
        .unwrap();
    }
    std::fs::write(generated.join("licenses.rs"), module).unwrap();
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

/// Effective product version used by the About page, update checks and the
/// executable's VERSIONINFO resource. Packaged builds provide
/// `DVDA_PRODUCT_VERSION`; development builds read the Android product version so
/// the desktop and Android releases stay on the same product line.
fn effective_product_version(manifest_dir: &Path) -> String {
    if let Ok(configured) = env::var("DVDA_PRODUCT_VERSION") {
        let configured = configured.trim();
        if valid_product_version(configured) {
            return configured.to_owned();
        }
    }
    let pubspec = manifest_dir.join("../../../apps/android/pubspec.yaml");
    println!("cargo:rerun-if-changed={}", pubspec.display());
    if let Ok(text) = std::fs::read_to_string(&pubspec)
        && let Some(line) = text
            .lines()
            .find(|line| line.trim_start().starts_with("version:"))
        && let Some(value) = line.split_once(':').map(|(_, value)| value)
    {
        let version_name = value.trim().split('+').next().unwrap_or_default().trim();
        if valid_product_version(version_name) {
            return version_name.to_owned();
        }
    }
    env::var("CARGO_PKG_VERSION").unwrap()
}

fn valid_product_version(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

/// `VS_VERSION_INFO` block so the executable carries company, product and version
/// metadata instead of shipping as an anonymous binary.
///
/// The packaging toolchain forwards the release version (`v1.0`) through
/// `DVDA_PRODUCT_VERSION`; a plain source build falls back to the crate version.
fn version_resource(raw: &str) -> String {
    let [major, minor, patch, build] = version_quad(raw);
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
