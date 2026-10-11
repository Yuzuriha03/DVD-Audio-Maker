use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=DVDA_MENU_NATIVE_DIR");
    if env::var_os("CARGO_FEATURE_DIRECT_LINK").is_none() {
        return;
    }
    assert_eq!(
        env::var("TARGET").unwrap(),
        "x86_64-pc-windows-msvc",
        "MSVC-only direct menu requires x86_64-pc-windows-msvc"
    );
    if env::var_os("DVDA_MENU_NATIVE_DIR").is_none() {
        let repo = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../..");
        let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("menu-vendor");
        let python = repo.join(".venv/Scripts/python.exe");
        let status = Command::new(&python)
            .arg(repo.join("tools/win-build/build-menu-runtime.py"))
            .args(["--direct-link", "--output"])
            .arg(&output)
            .status()
            .expect("run menu vendor builder");
        assert!(status.success(), "menu vendor build failed");
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
    let manifest = output.join("menu-build.json");
    let metadata = fs::read_to_string(&manifest).unwrap_or_else(|error| {
        panic!(
            "MSVC-only menu vendor manifest is missing or unreadable: {} ({error})",
            manifest.display()
        )
    });
    let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap_or_else(|error| {
        panic!(
            "MSVC-only menu vendor manifest is invalid JSON: {} ({error})",
            manifest.display()
        )
    });
    if metadata["target"] != "x86_64-pc-windows-msvc" || metadata["toolchain"] != "msvc" {
        panic!(
            "MSVC-only menu vendor requires menu-build.json target=x86_64-pc-windows-msvc and toolchain=msvc; GNU/MinGW archives are not link-compatible: {}",
            manifest.display()
        );
    }
    for name in [
        "menu-build.json",
        "dvda_menu_spu_vendor.lib",
        "dvda_menu_nav_vendor.lib",
    ] {
        let path = output.join(name);
        assert!(
            path.is_file(),
            "MSVC-only menu vendor input is missing (expected .lib, not GNU .a): {}",
            path.display()
        );
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=static=dvda_menu_spu_vendor");
    println!("cargo:rustc-link-lib=static=dvda_menu_nav_vendor");
    println!("cargo:rustc-link-lib=ws2_32");
}
