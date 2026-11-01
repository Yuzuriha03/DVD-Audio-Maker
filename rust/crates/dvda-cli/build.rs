fn main() {
    let target = std::env::var("TARGET").unwrap();
    assert!(
        matches!(
            target.as_str(),
            "x86_64-pc-windows-msvc" | "aarch64-pc-windows-msvc"
        ),
        "DVD-Audio Maker supports only Windows x64/ARM64 MSVC, got {target}"
    );
    if std::env::var_os("CARGO_FEATURE_DIRECT_BRIDGES").is_some() && !static_ffmpeg() {
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
}

fn static_ffmpeg() -> bool {
    println!("cargo:rerun-if-env-changed=DVDA_FFMPEG_PREFIX");
    let Some(prefix) = std::env::var_os("DVDA_FFMPEG_PREFIX") else {
        return false;
    };
    let manifest = std::path::PathBuf::from(prefix).join("build-manifest.json");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let bytes = std::fs::read(manifest).expect("FFmpeg build manifest required");
    let record: serde_json::Value =
        serde_json::from_slice(&bytes).expect("Invalid FFmpeg manifest");
    record["linkage"] == "static"
}
