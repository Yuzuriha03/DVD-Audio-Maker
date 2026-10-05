use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=DVDA_RUNTIME_ARCHIVE");
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
                "1 24 \"{}\"\n",
                manifest.display().to_string().replace('\\', "/")
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
        assert!(status.success(), "compile Windows desktop manifest");
        println!("cargo:rustc-link-arg={}", object.display());
    } else {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
