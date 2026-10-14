use std::{env, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DVDA_PRODUCT_VERSION");
    println!("cargo:rerun-if-env-changed=RUSTC");
    let version = env::var("DVDA_PRODUCT_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").unwrap());
    let output = Command::new(env::var_os("RUSTC").expect("Cargo provides RUSTC"))
        .arg("--version")
        .output()
        .expect("Read the actual Rust compiler version");
    assert!(output.status.success(), "rustc --version failed");
    let compiler = String::from_utf8(output.stdout).expect("rustc version is UTF-8");
    let target = env::var("TARGET").expect("Cargo provides TARGET");
    for (key, value) in [
        (
            "DVDA_ENCODER_ID",
            format!("DVD-Audio-Maker {} / Rust MLP", version.trim()),
        ),
        ("DVDA_ENCODER_COMPILER", compiler.trim().to_owned()),
        ("DVDA_ENCODER_TARGET", target),
    ] {
        assert!(
            !value.contains(['\r', '\n', '\0']) && value.len() < 1024,
            "Invalid build identity"
        );
        println!("cargo:rustc-env={key}={value}");
    }
}
