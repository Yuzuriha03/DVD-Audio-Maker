fn main() {
    let target = std::env::var("TARGET").unwrap();
    assert!(
        matches!(
            target.as_str(),
            "x86_64-pc-windows-msvc" | "aarch64-pc-windows-msvc"
        ),
        "DVD-Audio Maker supports only Windows x64/ARM64 MSVC, got {target}"
    );
}
