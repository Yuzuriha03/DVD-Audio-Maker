use std::{env, path::PathBuf};
fn main() {
    let target = env::var("TARGET").expect("Cargo must provide TARGET to the bridge build script");
    if target != "x86_64-pc-windows-msvc" {
        panic!("MSVC-only dvda-bridges requires TARGET=x86_64-pc-windows-msvc, got {target}");
    }
    println!("cargo:rerun-if-env-changed=DVDA_FFMPEG_PREFIX");
    println!("cargo:rerun-if-env-changed=DVDA_MAGICK_WORK");
    println!("cargo:rerun-if-env-changed=DVDA_IMAGE_DEPS_DIR");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=LIB");
    println!("cargo:rerun-if-env-changed=DVDA_LIB_TOOL");

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    if env::var_os("CARGO_FEATURE_MEDIA").is_some() {
        let prefix = PathBuf::from(env::var_os("DVDA_FFMPEG_PREFIX").expect(
            "DVDA_FFMPEG_PREFIX must name the retained FFmpeg installation when the media feature is enabled",
        ));
        let manifest = prefix.join("build-manifest.json");
        println!("cargo:rerun-if-changed={}", manifest.display());
        let record: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&manifest).expect("MSVC FFmpeg build-manifest.json is required"),
        )
        .expect("invalid FFmpeg build-manifest.json");
        assert!(
            record["target"] == "x86_64-pc-windows-msvc" && record["toolchain"] == "msvc",
            "MSVC-only media bridge requires FFmpeg libraries compiled with MSVC; rebuild with tools/win-build/build-msvc-ffmpeg.py"
        );
        generate_layout(&prefix);
        std::fs::write(
            out.join("ffmpeg-linkage.rs"),
            format!(
                "pub const STATIC_LINKED: bool = {};\n",
                record["linkage"] == "static"
            ),
        )
        .unwrap();
        let libraries = ["avformat", "avcodec", "avutil", "swresample", "swscale"];
        if record["linkage"] == "static" {
            assert!(
                record["profile"] == "static" && record["crt"] == "static",
                "Static FFmpeg requires the complete profile and static CRT"
            );
            let lib_directory = prefix.join("lib");
            println!("cargo:rustc-link-search=native={}", lib_directory.display());
            for name in libraries {
                require_msvc_library(&lib_directory, name);
                println!(
                    "cargo:rerun-if-changed={}",
                    lib_directory.join(format!("{name}.lib")).display()
                );
                println!("cargo:rustc-link-lib=static={name}");
            }
            for name in ["soxr", "zlib"] {
                require_msvc_library(&lib_directory, name);
                println!(
                    "cargo:rerun-if-changed={}",
                    lib_directory.join(format!("{name}.lib")).display()
                );
                println!("cargo:rustc-link-lib=static={name}");
            }
            for name in ["ole32", "user32", "bcrypt", "advapi32", "shell32", "ws2_32"] {
                println!("cargo:rustc-link-lib={name}");
            }
        } else {
            let libtool = msvc_lib_tool();
            for (lib, version) in [
                ("avformat", 63),
                ("avcodec", 63),
                ("avutil", 61),
                ("swresample", 7),
                ("swscale", 10),
            ] {
                let definition = prefix.join("lib").join(format!("{lib}-{version}.def"));
                println!("cargo:rerun-if-changed={}", definition.display());
                let archive = out.join(format!("dvda_delay_{lib}.lib"));
                assert!(
                    std::process::Command::new(&libtool)
                        .arg("/nologo")
                        .arg(format!("/def:{}", definition.display()))
                        .arg(format!("/name:{lib}-{version}.dll"))
                        .arg("/machine:x64")
                        .arg(format!("/out:{}", archive.display()))
                        .status()
                        .expect("cannot generate MSVC FFmpeg import library with LIB")
                        .success(),
                    "MSVC FFmpeg import library generation failed"
                );
                println!("cargo:rustc-link-lib=static=dvda_delay_{lib}");
            }
            println!("cargo:rustc-link-search=native={}", out.display());
            println!("cargo:rustc-link-lib=delayimp");
        }
    }
    if env::var_os("CARGO_FEATURE_IMAGE").is_some() {
        let work = PathBuf::from(env::var_os("DVDA_MAGICK_WORK").expect(
            "DVDA_MAGICK_WORK must name the ImageMagick build directory when the image feature is enabled",
        ));
        generate_image_layout(&work);
        for (dir, name) in [
            ("compile/MagickWand/.libs", "MagickWand-7.Q16HDRI"),
            ("compile/MagickCore/.libs", "MagickCore-7.Q16HDRI"),
            (".", "freetype-minimal"),
        ] {
            require_msvc_library(&work.join(dir), name);
            println!(
                "cargo:rustc-link-search=native={}",
                work.join(dir).display()
            );
            println!("cargo:rustc-link-lib=static={name}");
        }
        let image_deps = env::var_os("DVDA_IMAGE_DEPS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| work.join("lib"));
        println!("cargo:rustc-link-search=native={}", image_deps.display());
        for name in ["jpeg", "png16", "webpdecoder", "webpmux", "z"] {
            require_msvc_library(&image_deps, name);
            println!("cargo:rustc-link-lib=static={name}");
        }
        for name in ["shell32", "gdi32", "user32", "ws2_32", "advapi32"] {
            println!("cargo:rustc-link-lib={name}");
        }
    }
}

fn msvc_lib_tool() -> std::ffi::OsString {
    env::var_os("DVDA_LIB_TOOL")
        .or_else(|| {
            // Native Tools prompts use LIB for the system library search path.
            // Accept a LIB executable override without treating that path list as a command.
            env::var_os("LIB").filter(|value| {
                std::path::Path::new(value)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
            })
        })
        .unwrap_or_else(|| "lib.exe".into())
}

fn require_msvc_library(directory: &std::path::Path, name: &str) {
    let candidates = [
        directory.join(format!("{name}.lib")),
        directory.join(format!("lib{name}.lib")),
    ];
    if !candidates.iter().any(|path| path.is_file()) {
        panic!(
            "MSVC-only image bridge requires a .lib archive for {name} in {}; GNU .a archives are not link-compatible",
            directory.display()
        );
    }
}

fn generate_layout(prefix: &std::path::Path) {
    use std::{fs, process::Command};
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let exe = out.join("layout-probe.exe");
    let cc = env::var_os("CC").unwrap_or_else(|| "cl.exe".into());
    assert!(
        Command::new(cc)
            .args(["/nologo", "/TC", "/std:c17"])
            .arg(format!("/I{}", prefix.join("include").display()))
            .arg("layout-probe.c")
            .arg(format!("/Fe:{}", exe.display()))
            .arg(format!("/Fo:{}", out.join("layout-probe.obj").display()))
            .status()
            .unwrap()
            .success(),
        "MSVC FFmpeg layout probe compilation failed; set CC to cl.exe-compatible compiler"
    );
    println!("cargo:rerun-if-changed=layout-probe.c");
    let run = Command::new(exe).output().unwrap();
    assert!(run.status.success());
    let mut code = String::new();
    for line in String::from_utf8(run.stdout).unwrap().lines() {
        let p: Vec<_> = line.split_whitespace().collect();
        match p[0] {
            "T" => {
                code += &format!(
                    "#[repr(C,align(8))] pub struct {} {{ _opaque:[u8;{}] }}\n",
                    p[1], p[2]
                )
            }
            "K" => code += &format!("pub const {}:i32={}u32 as i32;\n", p[1], p[2]),
            "F" => {
                let ty = match p[2] {
                    "streams" => "*mut *mut Stream",
                    "nb_streams" | "packet_size" => "u32",
                    "duration" if p[1] == "Format" => "i64",
                    "metadata" | "priv_data" => "*mut c_void",
                    "interrupt_callback" => "Interrupt",
                    "oformat" => "*mut OutputFormat",
                    "pb" => "*mut Io",
                    "codecpar" => "*mut Parameters",
                    "ch_layout" => "Layout",
                    "time_base" | "framerate" | "sample_aspect_ratio" => "Rational",
                    "attached_pic" => "Packet",
                    "frame_num" | "pts" | "dts" | "duration" | "pos" | "bit_rate"
                    | "rc_min_rate" | "rc_max_rate" => "i64",
                    "data" if p[1] == "Frame" => "[*mut u8;8]",
                    "data" => "*mut u8",
                    "extended_data" => "*mut *mut u8",
                    "linesize" => "[i32;8]",
                    _ => "i32",
                };
                code += &format!(
                    "impl {} {{ /// # Safety\n/// The receiver must refer to a live FFmpeg object from the probed ABI.\npub unsafe fn {}(&self)->{} {{unsafe{{std::ptr::read_unaligned((self as *const Self as *const u8).add({}).cast())}}}} /// # Safety\n/// The receiver must be exclusively accessible and use the probed ABI.\npub unsafe fn {}_ptr(&mut self)->*mut {} {{unsafe{{(self as *mut Self as *mut u8).add({}).cast()}}}} }}\n",
                    p[1], p[2], ty, p[3], p[2], ty, p[3]
                );
            }
            _ => panic!("unknown ABI record"),
        }
    }
    fs::write(out.join("ffmpeg-layout.rs"), code).unwrap();
}
fn generate_image_layout(work: &std::path::Path) {
    use std::{fs, process::Command};
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let exe = out.join("image-layout-probe.exe");
    let cc = env::var_os("CC").unwrap_or_else(|| "cl.exe".into());
    assert!(
        Command::new(cc)
            .args([
                "image-layout-probe.c",
                "/nologo",
                "/TP",
                "/std:c++17",
                "/DMAGICKCORE_QUANTUM_DEPTH=16",
                "/DMAGICKCORE_HDRI_ENABLE=1",
            ])
            .arg(format!("/I{}", work.join("compile").display()))
            .arg(format!("/I{}", work.join("include").display()))
            .arg(format!("/I{}", work.join("ImageMagick").display()))
            .arg(format!("/I{}", work.join("ImageMagick-7.0.8-47").display()))
            .arg(format!("/Fe:{}", exe.display()))
            .arg(format!(
                "/Fo:{}",
                out.join("image-layout-probe.obj").display()
            ))
            .status()
            .unwrap()
            .success(),
        "MSVC ImageMagick layout probe compilation failed; set CC to cl.exe-compatible compiler"
    );
    println!("cargo:rerun-if-changed=image-layout-probe.c");
    let run = Command::new(exe).output().unwrap();
    assert!(run.status.success());
    fs::write(out.join("image-layout.rs"), run.stdout).unwrap();
}
