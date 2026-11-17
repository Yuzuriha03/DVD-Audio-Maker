use std::{env, path::PathBuf};
fn main() {
    let target = env::var("TARGET").expect("Cargo must provide TARGET to the bridge build script");
    if target == "aarch64-linux-android" {
        android_build();
        return;
    }
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
    use std::{fs, process::Command};
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
    let code = layout_code(&run.stdout);
    fs::write(out.join("ffmpeg-layout.rs"), code).unwrap();
}

fn layout_code(stdout: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut code = String::new();
    for line in String::from_utf8(stdout.to_vec()).unwrap().lines() {
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
    code
}

fn generate_layout_gnu(include: &std::path::Path) -> Vec<u8> {
    use std::process::Command;
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let cc = env::var_os("DVDA_HOST_CC").unwrap_or_else(|| {
        if cfg!(windows) {
            "cl.exe".into()
        } else {
            "cc".into()
        }
    });
    let exe = out.join(if cfg!(windows) {
        "layout-probe-host.exe"
    } else {
        "layout-probe-host"
    });
    let mut probe = Command::new(&cc);
    if cfg!(windows) {
        probe
            .args(["/nologo", "/TC", "/std:c17"])
            .arg(format!("/I{}", include.display()))
            .arg("layout-probe.c")
            .arg(format!("/Fe:{}", exe.display()))
            .arg(format!(
                "/Fo:{}",
                out.join("layout-probe-host.obj").display()
            ));
    } else {
        probe
            .args(["-std=c17", "-O0", "-w"])
            .arg(format!("-I{}", include.display()))
            .arg("layout-probe.c")
            .arg("-o")
            .arg(&exe);
    }
    let status = probe
        .status()
        .expect("cannot compile the host ABI layout probe");
    assert!(status.success(), "host ABI layout probe compilation failed");
    println!("cargo:rerun-if-changed=layout-probe.c");
    let run = Command::new(&exe)
        .output()
        .expect("cannot run the host ABI layout probe");
    assert!(run.status.success(), "host ABI layout probe failed");
    std::fs::write(out.join("ffmpeg-layout.rs"), layout_code(&run.stdout)).unwrap();
    run.stdout
}

fn android_build() {
    println!("cargo:rerun-if-env-changed=DVDA_FFMPEG_PREFIX");
    println!("cargo:rerun-if-env-changed=DVDA_HOST_CC");
    println!("cargo:rerun-if-env-changed=CC_aarch64_linux_android");
    println!("cargo:rerun-if-env-changed=ANDROID_NDK_ROOT");
    println!("cargo:rerun-if-env-changed=ANDROID_NDK_HOME");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    if env::var_os("CARGO_FEATURE_MEDIA").is_none() {
        return;
    }
    let prefix = PathBuf::from(
        env::var_os("DVDA_FFMPEG_PREFIX")
            .expect("DVDA_FFMPEG_PREFIX must name the Android FFmpeg installation"),
    );
    let lib_directory = prefix.join("lib");
    println!("cargo:rustc-link-search=native={}", lib_directory.display());
    for name in [
        "avformat",
        "avcodec",
        "avutil",
        "swresample",
        "swscale",
        "soxr",
    ] {
        let archive = lib_directory.join(format!("lib{name}.a"));
        assert!(
            archive.is_file(),
            "Android FFmpeg static library is missing: {}",
            archive.display()
        );
        println!("cargo:rerun-if-changed={}", archive.display());
        println!("cargo:rustc-link-lib=static={name}");
    }
    for name in ["m", "dl", "z"] {
        println!("cargo:rustc-link-lib={name}");
    }
    let include = prefix.join("include");
    let probe = generate_layout_gnu(&include);
    verify_android_abi(&include, &probe);
    std::fs::write(
        out.join("ffmpeg-linkage.rs"),
        "pub const STATIC_LINKED: bool = true;\n",
    )
    .unwrap();
}

const TYPE_ALIASES: [(&str, &str); 10] = [
    ("Format", "AVFormatContext"),
    ("Stream", "AVStream"),
    ("Parameters", "AVCodecParameters"),
    ("Context", "AVCodecContext"),
    ("Frame", "AVFrame"),
    ("Packet", "AVPacket"),
    ("Layout", "AVChannelLayout"),
    ("Io", "AVIOContext"),
    ("Codec", "AVCodec"),
    ("OutputFormat", "AVOutputFormat"),
];

/// Target compiler for the Android ABI checks.
///
/// The host probe generates offsets with the Windows x64 ABI. Android ARM64 is
/// LP64 while the Windows host is LLP64, so records containing unsigned long
/// differ in size; the target compiler supplies the authoritative layout.
fn android_target_compiler() -> (std::ffi::OsString, Vec<std::ffi::OsString>) {
    use std::ffi::OsString;
    use std::path::Path;
    let compiler = env::var_os("CC_aarch64_linux_android").or_else(|| {
        let ndk = env::var_os("ANDROID_NDK_ROOT").or_else(|| env::var_os("ANDROID_NDK_HOME"))?;
        let host = if cfg!(windows) {
            "windows-x86_64"
        } else if cfg!(target_os = "macos") {
            "darwin-x86_64"
        } else {
            "linux-x86_64"
        };
        let name = if cfg!(windows) {
            "aarch64-linux-android24-clang.cmd"
        } else {
            "aarch64-linux-android24-clang"
        };
        let candidate = PathBuf::from(ndk)
            .join("toolchains/llvm/prebuilt")
            .join(host)
            .join("bin")
            .join(name);
        candidate.is_file().then_some(candidate.into_os_string())
    });
    let Some(compiler) = compiler else {
        panic!(
            "Android ABI verification requires the NDK target compiler; set CC_aarch64_linux_android or ANDROID_NDK_ROOT"
        );
    };
    let wrapper = cfg!(windows)
        && Path::new(&compiler).extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
        });
    if wrapper {
        let directory = Path::new(&compiler)
            .parent()
            .unwrap_or_else(|| Path::new("."));
        (
            directory.join("clang.exe").into_os_string(),
            vec![OsString::from("--target=aarch64-linux-android24")],
        )
    } else {
        (compiler, Vec::new())
    }
}

fn android_command(include: &std::path::Path) -> std::process::Command {
    let (program, prefix) = android_target_compiler();
    let mut command = std::process::Command::new(program);
    command
        .args(prefix)
        .arg("-std=c17")
        .arg(format!("-I{}", include.display()));
    command
}

fn tool_output_text(output: &std::process::Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

fn tool_output_diagnostics(output: &std::process::Output) -> String {
    let text = tool_output_text(output);
    let mut lines: Vec<String> = text
        .lines()
        .map(|line| line.chars().take(400).collect())
        .collect();
    if lines.len() > 30 {
        lines.drain(..lines.len() - 30);
    }
    lines.join("\n")
}

fn android_record_layouts(
    include: &std::path::Path,
    directory: &std::path::Path,
) -> std::collections::BTreeMap<String, (u64, u64)> {
    use std::fmt::Write as _;
    let mut source =
        String::from("#include <libavcodec/avcodec.h>\n#include <libavformat/avformat.h>\n");
    for (index, (_, ctype)) in TYPE_ALIASES.iter().enumerate() {
        writeln!(source, "struct {ctype} android_probe_{index};").unwrap();
    }
    writeln!(source, "int main(void){{return 0;}}").unwrap();
    let source_path = directory.join("android-layout-probe.c");
    let object_path = directory.join("android-layout-probe.o");
    std::fs::write(&source_path, source).unwrap();
    let output = android_command(include)
        .args(["-Xclang", "-fdump-record-layouts", "-c"])
        .arg(&source_path)
        .arg("-o")
        .arg(&object_path)
        .output()
        .expect("cannot run the Android ABI layout probe");
    assert!(
        output.status.success(),
        "Android ABI layout probe failed:\n{}",
        tool_output_diagnostics(&output)
    );
    let mut layouts = std::collections::BTreeMap::new();
    let mut current = None::<String>;
    for line in tool_output_text(&output).lines() {
        if let Some((_, name)) = line.split_once("| struct ") {
            let name = name.trim();
            if !name.is_empty() && !name.contains(' ') {
                current = Some(name.to_owned());
            }
        }
        if let Some(rest) = line.trim().strip_prefix("| [sizeof=")
            && let Some(name) = current.take()
        {
            let rest = rest.trim_end_matches(']');
            if let Some((size, align)) = rest.split_once(", align=")
                && let (Ok(size), Ok(align)) = (size.parse(), align.parse())
            {
                layouts.insert(name, (size, align));
            }
        }
    }
    layouts
}

/// Cross-checks host-generated ABI constants against the Android target ABI and
/// corrects opaque record sizes that differ between LLP64 and LP64.
fn verify_android_abi(include: &std::path::Path, probe: &[u8]) {
    use std::fmt::Write as _;
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let text = String::from_utf8_lossy(probe).into_owned();
    let mut host_sizes = std::collections::BTreeMap::new();
    let mut asserts = String::from(
        "#include <stddef.h>\n#include <libavcodec/avcodec.h>\n#include <libavformat/avformat.h>\n",
    );
    for (alias, ctype) in TYPE_ALIASES {
        writeln!(asserts, "typedef {ctype} {alias};").unwrap();
    }
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        match fields.as_slice() {
            ["T", alias, size] => {
                host_sizes.insert((*alias).to_owned(), size.parse::<u64>().unwrap());
            }
            ["F", alias, field, offset] => {
                writeln!(
                    asserts,
                    "_Static_assert(offsetof({alias}, {field}) == {offset}, \"{alias}.{field}\");"
                )
                .unwrap();
            }
            ["K", name, value] => {
                writeln!(asserts, "_Static_assert({name} == {value}, \"{name}\");").unwrap();
            }
            [] => {}
            _ => panic!("unexpected ABI probe record: {line}"),
        }
    }
    assert!(
        host_sizes.len() == TYPE_ALIASES.len(),
        "host ABI probe did not report every record size"
    );
    let layouts = android_record_layouts(include, &out);
    let layout_path = out.join("ffmpeg-layout.rs");
    let mut generated = std::fs::read_to_string(&layout_path).unwrap();
    for (alias, ctype) in TYPE_ALIASES {
        let host = host_sizes[alias];
        let (size, align) = layouts.get(ctype).copied().unwrap_or_else(|| {
            panic!("Android ABI layout probe did not report {ctype}");
        });
        if host == size && align == 8 {
            continue;
        }
        let old = format!("#[repr(C,align(8))] pub struct {alias} {{ _opaque:[u8;{host}] }}");
        let corrected =
            format!("#[repr(C,align({align}))] pub struct {alias} {{ _opaque:[u8;{size}] }}");
        assert!(
            generated.contains(&old),
            "generated layout record for {alias} was not found"
        );
        generated = generated.replace(&old, &corrected);
        println!(
            "Android ABI: {ctype} is {size} bytes on the target but {host} in the host probe; corrected"
        );
    }
    std::fs::write(&layout_path, generated).unwrap();
    let assert_path = out.join("android-abi-asserts.c");
    let object_path = out.join("android-abi-asserts.o");
    std::fs::write(&assert_path, asserts).unwrap();
    let output = android_command(include)
        .arg("-c")
        .arg(&assert_path)
        .arg("-o")
        .arg(&object_path)
        .output()
        .expect("cannot compile the Android ABI assertions");
    assert!(
        output.status.success(),
        "Android target ABI check failed:\n{}",
        tool_output_diagnostics(&output)
    );
}
