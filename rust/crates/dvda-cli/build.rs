fn main() {
    assert_eq!(
        std::env::var("TARGET").unwrap(),
        "x86_64-pc-windows-msvc",
        "DVD-Audio Maker supports only Windows x64 MSVC"
    );
    if std::env::var_os("CARGO_FEATURE_DIRECT_BRIDGES").is_some() {
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
