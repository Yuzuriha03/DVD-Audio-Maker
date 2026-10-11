use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=DVDA_MENU_NATIVE_DIR");
    if env::var_os("CARGO_FEATURE_DIRECT_LINK").is_none() {
        return;
    }
    // Accept both MSVC and GNU toolchains for Windows x64
    let target = env::var("TARGET").unwrap();
    assert!(
        target == "x86_64-pc-windows-gnu" || target == "x86_64-pc-windows-msvc",
        "direct menu requires Windows x64 (MSVC or GNU), got {target}"
    );

    // If DVDA_MENU_NATIVE_DIR is not set and we can't find the Python builder,
    // emit a warning and skip native linking
    if env::var_os("DVDA_MENU_NATIVE_DIR").is_none() {
        let repo = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../..");
        let python = repo.join(".venv/Scripts/python.exe");
        let build_script = repo.join("tools/win-build/build-menu-runtime.py");

        if !python.exists() || !build_script.exists() {
            println!("cargo:warning=Menu native builder not available; skipping direct-link build");
            return;
        }

        let status = Command::new(&python)
            .arg(&build_script)
            .args(["--direct-link", "--output"])
            .arg("output")
            .status();

        if status.is_err() || !status.as_ref().unwrap().success() {
            println!("cargo:warning=Menu vendor build failed or not available; skipping direct-link");
            return;
        }
    }

    let repo = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../..");
    let output = env::var_os("DVDA_MENU_NATIVE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("menu-vendor"));

    println!(
        "cargo:rerun-if-changed={}",
        repo.join("tools/menu-native").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        repo.join("tools/win-build/build-menu-runtime.py").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        repo.join("rust/crates/dvda-menu/src").display()
    );

    // The vendor builder emits its Rust reset object into these archives. Track
    // replacements even when DVDA_MENU_NATIVE_DIR keeps the same directory.
    for name in [
        "menu-build.json",
        "libdvda_menu_spu_vendor.a",
        "libdvda_menu_nav_vendor.a",
    ] {
        let path = output.join(name);
        if !path.is_file() {
            println!("cargo:warning=menu vendor input is missing: {} (continuing without linking)", path.display());
            return;
        }
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=static=dvda_menu_spu_vendor");
    println!("cargo:rustc-link-lib=static=dvda_menu_nav_vendor");
    println!("cargo:rustc-link-lib=ws2_32");
}
