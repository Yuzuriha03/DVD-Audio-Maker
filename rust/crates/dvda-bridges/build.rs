use std::{env, path::PathBuf};
fn main() {
    println!("cargo:rerun-if-env-changed=DVDA_FFMPEG_PREFIX");
    println!("cargo:rerun-if-env-changed=DVDA_MAGICK_WORK");
    println!("cargo:rerun-if-env-changed=DLLTOOL");
    if env::var_os("CARGO_FEATURE_MEDIA").is_some() {
        let prefix = PathBuf::from(
            env::var_os("DVDA_FFMPEG_PREFIX")
                .expect("DVDA_FFMPEG_PREFIX must name the retained FFmpeg installation"),
        );
        generate_layout(&prefix);
        let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
        let dlltool = env::var_os("DLLTOOL").unwrap_or_else(|| "dlltool".into());
        for (lib, version) in [
            ("avformat", 63),
            ("avcodec", 63),
            ("avutil", 61),
            ("swresample", 7),
            ("swscale", 10),
        ] {
            let definition = prefix.join("lib").join(format!("{lib}-{version}.def"));
            println!("cargo:rerun-if-changed={}", definition.display());
            let archive = out.join(format!("libdvda_delay_{lib}.a"));
            assert!(
                std::process::Command::new(&dlltool)
                    .current_dir(&out)
                    .arg("--dllname")
                    .arg(format!("{lib}-{version}.dll"))
                    .arg("--input-def")
                    .arg(&definition)
                    .arg("--output-delaylib")
                    .arg(archive.file_name().unwrap())
                    .status()
                    .expect("cannot generate FFmpeg delay import library")
                    .success(),
                "FFmpeg delay import generation failed"
            );
            println!("cargo:rustc-link-lib=static=dvda_delay_{lib}");
        }
        println!("cargo:rustc-link-search=native={}", out.display());
        println!("cargo:rustc-link-lib=delayimp");
    }
    if env::var_os("CARGO_FEATURE_IMAGE").is_some() {
        let work = PathBuf::from(
            env::var_os("DVDA_MAGICK_WORK")
                .expect("DVDA_MAGICK_WORK must name the ImageMagick build directory"),
        );
        generate_image_layout(&work);
        for (dir, name) in [
            ("compile/MagickWand/.libs", "MagickWand-7.Q16HDRI"),
            ("compile/MagickCore/.libs", "MagickCore-7.Q16HDRI"),
            (".", "freetype-minimal"),
        ] {
            println!(
                "cargo:rustc-link-search=native={}",
                work.join(dir).display()
            );
            println!("cargo:rustc-link-lib=static={name}");
        }
        let msys =
            PathBuf::from(env::var_os("DVDA_MSYS_ROOT").unwrap_or_else(|| "C:\\msys64".into()));
        println!(
            "cargo:rustc-link-search=native={}",
            msys.join("mingw64/lib").display()
        );
        for name in ["jpeg", "png16", "webpdecoder", "webpmux", "z"] {
            println!("cargo:rustc-link-lib=static={name}");
        }
        for name in ["shell32", "gdi32", "ws2_32", "advapi32"] {
            println!("cargo:rustc-link-lib={name}");
        }
        println!("cargo:rustc-link-search=native={}", work.display());
        println!("cargo:rustc-link-lib=static=ucrt-jump");
    }
}
fn generate_layout(prefix: &std::path::Path) {
    use std::{fs, process::Command};
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let exe = out.join("layout-probe.exe");
    let cc = env::var_os("CC").unwrap_or_else(|| "gcc".into());
    assert!(
        Command::new(cc)
            .args(["layout-probe.c", "-o"])
            .arg(&exe)
            .arg(format!("-I{}", prefix.join("include").display()))
            .status()
            .unwrap()
            .success()
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
    let cc = env::var_os("CC").unwrap_or_else(|| "gcc".into());
    assert!(
        Command::new(cc)
            .args([
                "image-layout-probe.c",
                "-DMAGICKCORE_QUANTUM_DEPTH=16",
                "-DMAGICKCORE_HDRI_ENABLE=1",
                "-o"
            ])
            .arg(&exe)
            .arg(format!("-I{}", work.join("compile").display()))
            .arg(format!("-I{}", work.join("ImageMagick-7.0.8-47").display()))
            .status()
            .unwrap()
            .success()
    );
    println!("cargo:rerun-if-changed=image-layout-probe.c");
    let run = Command::new(exe).output().unwrap();
    assert!(run.status.success());
    fs::write(out.join("image-layout.rs"), run.stdout).unwrap();
}
