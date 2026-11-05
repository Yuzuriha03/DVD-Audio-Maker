use std::{env, path::PathBuf};
fn main() {
    let target = env::var("TARGET").expect("Cargo must provide TARGET to the bridge build script");
    if !matches!(
        target.as_str(),
        "x86_64-pc-windows-msvc" | "aarch64-pc-windows-msvc"
    ) {
        panic!("MSVC-only dvda-bridges requires a Windows MSVC target, got {target}");
    }
    println!("cargo:rerun-if-env-changed=DVDA_FFMPEG_PREFIX");
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
            record["target"] == target && record["toolchain"] == "msvc",
            "media bridge requires FFmpeg libraries compiled with MSVC for {target}; rebuild with tools/win-build/build-msvc-ffmpeg.py"
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
            require_msvc_library(&lib_directory, "soxr");
            println!(
                "cargo:rerun-if-changed={}",
                lib_directory.join("soxr.lib").display()
            );
            println!("cargo:rustc-link-lib=static=soxr");
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
                        .arg(if target == "aarch64-pc-windows-msvc" {
                            "/machine:arm64"
                        } else {
                            "/machine:x64"
                        })
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
    assert!(
        candidates.iter().any(|path| path.is_file()),
        "MSVC-only image bridge requires a .lib archive for {name} in {}; GNU .a archives are not link-compatible",
        directory.display()
    );
}

fn hex_literal(value: u64) -> String {
    let digits = format!("{value:08x}");
    format!("0x{}_{}", &digits[..4], &digits[4..])
}

fn generate_layout(prefix: &std::path::Path) {
    use std::{fmt::Write as _, fs, process::Command};
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let target = env::var("TARGET").unwrap();
    // ARM64 Windows follows the same LLP64 data layout as x64 (8-byte pointers,
    // 8-byte alignment for i64/double), so probe the ABI with a host x64 cl.exe
    // and run the probe locally; the generated offsets are valid for ARM64 too.
    let mut exe = out.join("layout-probe.exe");
    let mut cc = env::var_os("CC").unwrap_or_else(|| "cl.exe".into());
    if target == "aarch64-pc-windows-msvc" {
        let configured = env::var_os("DVDA_HOST_CC");
        let derived = std::path::PathBuf::from(&cc)
            .to_string_lossy()
            .replace("\\ARM64\\", "\\x64\\")
            .replace("\\arm64\\", "\\x64\\")
            .replace("/ARM64/", "/x64/")
            .replace("/arm64/", "/x64/");
        let host: std::ffi::OsString = configured.unwrap_or_else(|| derived.into());
        assert!(
            std::path::Path::new(&host).is_file()
                || std::process::Command::new(&host).arg("/?").output().is_ok(),
            "ARM64 cross layout probe needs host x64 cl.exe; set DVDA_HOST_CC to <VC>/bin/Hostx64/x64/cl.exe"
        );
        cc = host;
        exe = out.join("layout-probe-host.exe");
    }
    let mut probe = Command::new(cc);
    probe
        .args(["/nologo", "/TC", "/std:c17"])
        .arg(format!("/I{}", prefix.join("include").display()))
        .arg("layout-probe.c")
        .arg(format!("/Fe:{}", exe.display()))
        .arg(format!("/Fo:{}", out.join("layout-probe.obj").display()));
    if target == "aarch64-pc-windows-msvc" {
        // The ARM64 VsDevCmd environment points INCLUDE/LIB at ARM64 CRT and
        // SDK libraries; the host x64 probe must use the x64 equivalents.
        for name in ["INCLUDE", "LIB"] {
            if let Some(value) = env::var_os(name) {
                let value = value
                    .to_string_lossy()
                    .replace("ARM64", "x64")
                    .replace("arm64", "x64");
                probe.env(name, value);
            }
        }
    }
    assert!(
        probe.status().unwrap().success(),
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
                let _ = writeln!(
                    code,
                    "#[repr(C,align(8))] pub struct {} {{ _opaque:[u8;{}] }}",
                    p[1], p[2]
                );
            }
            "K" => {
                let value = hex_literal(u64::from(p[2].parse::<u32>().unwrap()));
                let _ = writeln!(code, "pub const {}:i32={value}u32.cast_signed();", p[1]);
            }
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
                let offset = hex_literal(p[3].parse::<u64>().unwrap());
                let _ = writeln!(
                    code,
                    "impl {} {{ /// # Safety\n/// The receiver must refer to a live `FFmpeg` object from the probed ABI.\n#[must_use]\npub unsafe fn {}(&self)->{} {{unsafe{{std::ptr::read_unaligned(std::ptr::from_ref(self).cast::<u8>().add({offset}).cast())}}}} /// # Safety\n/// The receiver must be exclusively accessible and use the probed ABI.\n#[must_use]\npub unsafe fn {}_ptr(&mut self)->*mut {} {{unsafe{{std::ptr::from_mut(self).cast::<u8>().add({offset}).cast()}}}} }}",
                    p[1], p[2], ty, p[2], ty
                );
            }
            _ => panic!("unknown ABI record"),
        }
    }
    fs::write(out.join("ffmpeg-layout.rs"), code).unwrap();
}
