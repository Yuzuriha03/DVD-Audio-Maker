use std::{env, path::PathBuf, process::Command};

fn main() {
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
    println!("cargo:rerun-if-env-changed=WINDRES");
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("app.manifest");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") {
        let compiler = env::var_os("WINDRES")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let msys = PathBuf::from("C:/msys64/mingw64/bin/windres.exe");
                if msys.is_file() {
                    msys
                } else {
                    PathBuf::from("windres")
                }
            });
        let script = output.join("desktop.rc");
        let object = output.join("desktop-resource.o");
        std::fs::write(
            &script,
            format!(
                "1 24 \"{}\"\n{}",
                manifest.display().to_string().replace('\\', "/"),
                version_resource()
            ),
        )
        .unwrap();
        let status = Command::new(compiler)
            .args(["-i"])
            .arg(&script)
            .args(["-o"])
            .arg(&object)
            .args(["-O", "coff"])
            .status()
            .expect("run GNU windres from the MSYS2 toolchain");
        assert!(status.success(), "compile Windows desktop resources");
        println!("cargo:rustc-link-arg={}", object.display());
    } else {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
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
