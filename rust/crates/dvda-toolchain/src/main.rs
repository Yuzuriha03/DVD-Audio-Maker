//! Small Windows release assembler for the Rust GUI.
//!
//! The release ZIP contains one application EXE and user documentation.
//! Build provenance files, source trees, C# outputs and local profiles never
//! enter the user package.
mod fonts;
mod pe;
mod publication;
mod validation;
mod verifier;

use std::{
    collections::BTreeMap,
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Default, Debug)]
struct Args {
    repository: Option<PathBuf>,
    output: Option<PathBuf>,
    target_directory: Option<PathBuf>,
    media_runtime: Option<PathBuf>,
    image_runtime: Option<PathBuf>,
    image_author: Option<PathBuf>,
    ffmpeg_prefix: Option<PathBuf>,
    source: Option<PathBuf>,
    prebuilt: Option<PathBuf>,
    version: String,
    target: String,
    single_exe: bool,
}

/// Validated release inputs recorded next to every published file.
struct ReleaseSpec<'a> {
    version: &'a str,
    suffix: &'a str,
    target: &'a str,
    single_exe: bool,
    target_directory: &'a Path,
    media: &'a Path,
    image: &'a Path,
    author: &'a Path,
    ffmpeg_prefix: &'a Path,
    prebuilt: &'a Path,
    source: &'a Path,
}

/// Environment facts of the GUI build that affect the published executable.
#[derive(Debug)]
struct BuildEnvironment {
    ffmpeg_prefix: PathBuf,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("[ERROR] {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    run_args(env::args().skip(1).collect())
}

fn run_args(arguments: Vec<String>) -> Result<(), String> {
    let mut args = arguments.into_iter();
    let command = args.next().unwrap_or_else(|| "help".into());
    if command == "help" || command == "--help" || command == "-h" {
        print_usage();
        return Ok(());
    }
    if command == "font" {
        return fonts::run(&args.collect::<Vec<_>>());
    }
    if command == "verify-aob" {
        return verifier::run(&args.collect::<Vec<_>>());
    }
    if command != "package" {
        return Err(format!("Unknown command: {command}"));
    }
    package(parse_args(args)?)
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut options = Args {
        version: "v1.0".into(),
        target: "x86_64-pc-windows-msvc".into(),
        ..Args::default()
    };
    while let Some(argument) = args.next() {
        if argument == "--single-exe" {
            options.single_exe = true;
            continue;
        }
        let value = next_arg(&mut args, &argument)?;
        match argument.as_str() {
            "--repo" => options.repository = Some(value.into()),
            "--output" => options.output = Some(value.into()),
            "--target-dir" => options.target_directory = Some(value.into()),
            "--media-runtime" => options.media_runtime = Some(value.into()),
            "--image-runtime" => options.image_runtime = Some(value.into()),
            "--image-author" => options.image_author = Some(value.into()),
            "--ffmpeg-prefix" => options.ffmpeg_prefix = Some(value.into()),
            "--source" => options.source = Some(value.into()),
            "--prebuilt" => options.prebuilt = Some(value.into()),
            "--magick-work" | "--image-deps" => {
                return Err(format!(
                    "{argument} is retired; Rust images need no ImageMagick build"
                ));
            }
            "--version" => options.version = value,
            "--target" => options.target = value,
            _ => return Err(format!("Unknown option: {argument}")),
        }
    }
    validate_version(&options.version)?;
    validation::target_machine(&options.target)?;
    Ok(options)
}

fn target_label(target: &str) -> &'static str {
    if target == "aarch64-pc-windows-msvc" {
        "Windows ARM64"
    } else {
        "Windows x64"
    }
}

fn validate_version(value: &str) -> Result<(), String> {
    let valid = value.strip_prefix('v').is_some_and(|value| {
        let (numbers, suffix) = value
            .split_once('-')
            .map_or((value, None), |(a, b)| (a, Some(b)));
        let parts: Vec<_> = numbers.split('.').collect();
        (2..=3).contains(&parts.len())
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            && suffix.is_none_or(|s| {
                s.split(['.', '-'])
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric()))
            })
    });
    if valid {
        Ok(())
    } else {
        Err("--version requires a release version such as v1.0 or v1.1.0-rc.1".into())
    }
}

fn next_arg<I: Iterator<Item = String>>(args: &mut I, name: &str) -> Result<String, String> {
    args.next()
        .filter(|value| !value.trim().is_empty() && !value.starts_with("--"))
        .ok_or_else(|| format!("{name} requires a value"))
}

fn input_path(
    repository: &Path,
    explicit: Option<PathBuf>,
    environment: Option<std::ffi::OsString>,
    default: &str,
) -> PathBuf {
    absolutize(
        repository,
        explicit
            .or_else(|| environment.filter(|v| !v.is_empty()).map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from(default)),
    )
}

/// Resolve the one `FFmpeg` prefix used by the GUI release build. Static media is
/// already an authenticated link-time prefix; the shared media directory only
/// carries the runtime DLLs, so it must be told which authenticated shared
/// installation to link against. A supplied prefix never overrides static media.
fn resolve_ffmpeg_prefix(
    media: &Path,
    explicit: Option<PathBuf>,
    inputs: &mut validation::Inputs,
    target: &str,
) -> Result<PathBuf, String> {
    match inputs.ffmpeg_linkage {
        validation::FfmpegLinkage::Static => {
            if let Some(explicit) = explicit {
                let explicit =
                    canonical(&explicit).map_err(|error| format!("--ffmpeg-prefix: {error}"))?;
                if explicit != canonical(media)? {
                    return Err(format!(
                        "--ffmpeg-prefix must not replace the validated static media runtime: {}",
                        explicit.display()
                    ));
                }
            }
            Ok(media.to_path_buf())
        }
        validation::FfmpegLinkage::Shared => {
            let prefix = explicit.ok_or(
                "--ffmpeg-prefix is required when the validated media runtime uses the shared FFmpeg profile",
            )?;
            let manifest = validation::validate_ffmpeg_prefix(
                &prefix,
                target,
                validation::FfmpegLinkage::Shared,
            )?;
            inputs.records.push(manifest);
            Ok(prefix)
        }
    }
}

/// The author manifest authenticates its font; `--prebuilt/fonts` may only stage
/// identical bytes. Menu data conflicts keep their existing check in `copy_file`.
fn verify_staged_font(runtime: &Path, inputs: &validation::Inputs) -> Result<(), String> {
    let staged = runtime.join("fonts/DvdaNotoCJK-Regular.ttc");
    let Some(expected) = inputs.files.get("fonts/DvdaNotoCJK-Regular.ttc") else {
        return Ok(());
    };
    let staged_hash = validation::file_hash(&staged)?;
    let expected_hash = validation::file_hash(expected)?;
    if staged_hash != expected_hash {
        return Err(format!(
            "Staged menu font conflicts with the author-authenticated font: {} differs from {}",
            staged.display(),
            expected.display()
        ));
    }
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "Release validation, staging, and publication remain in their required order"
)]
fn package(options: Args) -> Result<(), String> {
    validate_version(&options.version)?;
    // Release inputs are explicit. A default or environment fallback could mix
    // one architecture's fonts/menu into another release, so require both.
    let prebuilt = options.prebuilt.ok_or(
        "--prebuilt is required and must name the architecture directory that contains fonts/",
    )?;
    let source = options
        .source
        .ok_or("--source is required and must name the data directory that contains menu/")?;
    let repository = options
        .repository
        .or_else(|| env::current_dir().ok())
        .ok_or("Cannot determine repository directory")?;
    let repository = normal_path(canonical(&repository)?);
    let target_machine = validation::target_machine(&options.target)?;
    let arm64 = target_machine == 0xAA64;
    let suffix = if arm64 { "win-arm64" } else { "win-x64" };
    let target_directory = input_path(
        &repository,
        options.target_directory,
        env::var_os("CARGO_TARGET_DIR"),
        "rust/target",
    );
    let target = target_directory.join(&options.target).join("release");
    let output = options.output.map_or_else(
        || repository.join("tools/win-build/release-onefile"),
        |path| absolutize(&repository, path),
    );
    let media = required_dir(
        input_path(
            &repository,
            options.media_runtime,
            None,
            "build/msvc-native/ffmpeg-static/install",
        ),
        "media runtime",
    )?;
    let image = input_path(
        &repository,
        options.image_runtime,
        None,
        "build/image-native",
    );
    let author = required_dir(
        input_path(
            &repository,
            options.image_author,
            None,
            "build/rust-author-production",
        ),
        "author runtime",
    )?;
    let source = absolutize(&repository, source);
    let prebuilt = absolutize(&repository, prebuilt);
    // All provenance and dependency checks precede any changes to release files.
    let mut inputs = validation::validate(&media, &image, &author, &options.target)?;
    // Exactly <prebuilt>/fonts and <source>/menu are the release inputs; no
    // environment variable or alternate layout may substitute silently.
    let fonts = required_dir(prebuilt.join("fonts"), "menu fonts")?;
    let menu = required_dir(source.join("menu"), "menu data")?;
    // The GUI release build never inherits DVDA_FFMPEG_PREFIX. Static media is
    // its own validated prefix; the shared profile needs an explicit,
    // authenticated installation because its runtime directory only carries the
    // DLLs, not the link-time FFmpeg layout.
    let explicit_prefix = options
        .ffmpeg_prefix
        .map(|path| absolutize(&repository, path));
    let ffmpeg_prefix =
        resolve_ffmpeg_prefix(&media, explicit_prefix, &mut inputs, &options.target)?;
    for name in [
        "Cargo.lock",
        "crates/dvda-desktop/Cargo.toml",
        "crates/dvda-core/Cargo.toml",
        "crates/dvda-mlp/Cargo.toml",
    ] {
        let path = repository.join("rust").join(name);
        validation::file_hash(&path)?;
        inputs.records.push(path);
    }
    for name in [
        "assets/dvd-audio-maker.svg",
        "assets/dvd-audio-maker.png",
        "assets/dvd-audio-maker.ico",
    ] {
        let path = repository.join(name);
        validation::file_hash(&path)?;
        inputs.records.push(path);
    }
    let candidate = publication::Candidate::new(&output)?;
    let stage = candidate.root.join("DVD-Audio-Maker");
    let runtime = candidate.root.join("runtime-input");
    fs::create_dir_all(&stage).map_err(io_error)?;
    fs::create_dir_all(&runtime).map_err(io_error)?;
    if !options.single_exe {
        for name in ["README.md", "README.en.md", "README.ja.md", "LICENSE"] {
            let source = if name.starts_with("README") {
                repository.join("tools/win-build/docs").join(name)
            } else {
                repository.join(name)
            };
            copy_file(&source, &stage.join(name))?;
        }
        copy_first_file(
            &[
                repository.join("THIRD-PARTY.txt"),
                repository.join("tools/win-build/docs/THIRD-PARTY.txt"),
            ],
            &stage.join("THIRD-PARTY.txt"),
        )?;
        copy_first_file(
            &[
                repository.join("THIRD-PARTY.en.txt"),
                repository.join("tools/win-build/docs/THIRD-PARTY.en.txt"),
            ],
            &stage.join("THIRD-PARTY.en.txt"),
        )?;
        copy_first_file(
            &[
                repository.join("NOTICE-Image.txt"),
                image.join("NOTICE.txt"),
            ],
            &stage.join("NOTICE-Image.txt"),
        )?;
        copy_first_file(
            &[
                repository.join("NOTICE-Menu.txt"),
                author.join("menu-NOTICE.txt"),
            ],
            &stage.join("NOTICE-Menu.txt"),
        )?;
        copy_first_file(
            &[
                repository.join("FFMPEG-LICENSE.txt"),
                repository.join("tools/win-build/docs/FFMPEG-LICENSE.txt"),
            ],
            &stage.join("FFMPEG-LICENSE.txt"),
        )?;
    }
    for (name, path) in &inputs.files {
        copy_file(path, &runtime.join(name))?;
    }
    fonts::stage_menu_fonts(std::slice::from_ref(&fonts), &runtime.join("fonts"))?;
    // <prebuilt>/fonts must not silently replace a font already authenticated
    // by the author manifest; identical bytes are required.
    verify_staged_font(&runtime, &inputs)?;
    for name in [
        "activeheader",
        "black_NTSC_720x480.jpg",
        "black_NTSC_720x480.png",
        "black_PAL_720x576.jpg",
        "black_PAL_720x576.png",
        "silence.wav",
    ] {
        copy_file(&menu.join(name), &runtime.join("data/menu").join(name))?;
    }
    // Relative font paths keep the archive portable across computers/cache roots.
    let font = runtime.join("fonts/DvdaNotoCJK-Regular.ttc");
    if !font.is_file() {
        return Err("Required shared CJK font collection is missing".into());
    }
    let mut typemap = String::from("<typemap>");
    for (face, language) in [(0, "SC"), (1, "JP"), (2, "KR"), (3, "TC")] {
        for prefix in ["DVDA-", ""] {
            let _ = write!(
                typemap,
                "<type name=\"{prefix}Noto-Sans-CJK-{language}\" family=\"Noto Sans CJK {language}\" format=\"truetype\" style=\"normal\" stretch=\"normal\" weight=\"400\" face=\"{face}\" glyphs=\"fonts/DvdaNotoCJK-Regular.ttc\"/>"
            );
        }
    }
    typemap.push_str("</typemap>");
    fs::write(runtime.join("type.xml"), typemap).map_err(io_error)?;
    validation::validate_staged(&runtime, &options.target)?;
    let embedded = candidate.root.join("runtime.bin");
    fs::write(&embedded, dvda_core::runtime::pack(&runtime)?).map_err(io_error)?;
    let spec = ReleaseSpec {
        version: &options.version,
        suffix,
        target: &options.target,
        single_exe: options.single_exe,
        target_directory: &target_directory,
        media: &media,
        image: &image,
        author: &author,
        ffmpeg_prefix: &ffmpeg_prefix,
        prebuilt: &prebuilt,
        source: &source,
    };
    finish_release(
        &candidate.root,
        &output,
        &inputs,
        &spec,
        || {
            let environment = build_gui(
                &repository,
                &embedded,
                &options.version,
                &options.target,
                &target_directory,
                arm64,
                &ffmpeg_prefix,
            )?;
            copy_file(
                &target.join("dvda-desktop.exe"),
                &stage.join("DVD-Audio-Maker.exe"),
            )?;
            validation::validate_shipped(&stage.join("DVD-Audio-Maker.exe"), &options.target)?;
            Ok(environment)
        },
        compress_zip,
    )?;
    print_release(&spec, &output)
}

/// Report only files this package actually published. A single-EXE release has
/// no release directory; printing one would point at a stale historical folder.
fn print_release(spec: &ReleaseSpec<'_>, output: &Path) -> Result<(), String> {
    if spec.single_exe {
        let exe = output.join(format!(
            "DVD-Audio-Maker-{}-{}.exe",
            spec.version, spec.suffix
        ));
        println!("[OK] Release EXE: {}", exe.display());
        println!(
            "[OK] Release records: {} and {}",
            output.join("MANIFEST.txt").display(),
            output.join("release-build.json").display()
        );
        println!("[OK] EXE SHA-256: {}", validation::file_hash(&exe)?);
        return Ok(());
    }
    println!(
        "[OK] Release directory: {}",
        output.join("DVD-Audio-Maker").display()
    );
    println!(
        "[OK] Release ZIP: {}",
        output
            .join(format!(
                "DVD-Audio-Maker-{}-{}.zip",
                spec.version, spec.suffix
            ))
            .display()
    );
    println!(
        "[OK] EXE SHA-256: {}",
        validation::file_hash(&output.join("DVD-Audio-Maker/DVD-Audio-Maker.exe"))?
    );
    println!(
        "[OK] Hash/provenance records: {}",
        output.join("MANIFEST.txt").display()
    );
    Ok(())
}

fn finish_release(
    candidate: &Path,
    output: &Path,
    inputs: &validation::Inputs,
    spec: &ReleaseSpec<'_>,
    build: impl FnOnce() -> Result<BuildEnvironment, String>,
    compress: impl FnOnce(&Path, &Path) -> Result<(), String>,
) -> Result<(), String> {
    let environment = build()?;
    let stage = candidate.join("DVD-Audio-Maker");
    let runtime = candidate.join("runtime-input");
    if runtime.is_dir() {
        validation::validate_combined(&[&stage, &runtime], spec.target)?;
    } else {
        validation::validate_staged(&stage, spec.target)?;
    }
    if spec.single_exe {
        let exe_name = format!("DVD-Audio-Maker-{}-{}.exe", spec.version, spec.suffix);
        copy_file(
            &stage.join("DVD-Audio-Maker.exe"),
            &candidate.join(&exe_name),
        )?;
        let mut relative_files = vec![PathBuf::from(&exe_name)];
        write_records(candidate, &relative_files, inputs, spec, &environment)?;
        relative_files.push(PathBuf::from("MANIFEST.txt"));
        relative_files.push(PathBuf::from("release-build.json"));
        return publication::commit(candidate, output, &relative_files);
    }
    let archive_name = format!("DVD-Audio-Maker-{}-{}.zip", spec.version, spec.suffix);
    let archive = candidate.join(&archive_name);
    compress(&stage, &archive)?;
    if !archive.is_file() || fs::metadata(&archive).map_err(io_error)?.len() == 0 {
        return Err("ZIP creation produced no archive".into());
    }
    let mut relative_files: Vec<PathBuf> = fs::read_dir(&stage)
        .map_err(io_error)?
        .map(|entry| {
            entry
                .map(|e| PathBuf::from("DVD-Audio-Maker").join(e.file_name()))
                .map_err(io_error)
        })
        .collect::<Result<_, _>>()?;
    relative_files.sort();
    relative_files.push(PathBuf::from(&archive_name));
    write_records(candidate, &relative_files, inputs, spec, &environment)?;
    relative_files.push(PathBuf::from("MANIFEST.txt"));
    relative_files.push(PathBuf::from("release-build.json"));
    publication::commit(candidate, output, &relative_files)
}

fn write_records(
    candidate: &Path,
    files: &[PathBuf],
    inputs: &validation::Inputs,
    spec: &ReleaseSpec<'_>,
    environment: &BuildEnvironment,
) -> Result<(), String> {
    let mut rows = String::new();
    let mut records = serde_json::Map::new();
    for relative in files {
        let path = candidate.join(relative);
        let digest = validation::file_hash(&path)?;
        let name = relative.to_string_lossy().replace('\\', "/");
        rows.push_str(&digest);
        rows.push_str("  ");
        rows.push_str(&name);
        rows.push('\n');
        records.insert(
            name,
            serde_json::json!({"bytes": fs::metadata(path).map_err(io_error)?.len(), "sha256": digest}),
        );
    }
    let sources: Vec<_> = inputs
        .records
        .iter()
        .map(|path| Ok(serde_json::json!({"path": path, "sha256": validation::file_hash(path)?})))
        .collect::<Result<_, String>>()?;
    // Record the exact architecture inputs so an x64/WoA release can be traced
    // back to its media, author, fonts and menu directories.
    let input_record = serde_json::json!({
        "target_directory": spec.target_directory.display().to_string(),
        "media_runtime": spec.media.display().to_string(),
        "image_runtime": spec.image.display().to_string(),
        "image_author": spec.author.display().to_string(),
        "ffmpeg_prefix": spec.ffmpeg_prefix.display().to_string(),
        "prebuilt": spec.prebuilt.display().to_string(),
        "source": spec.source.display().to_string(),
    });
    let environment_record = serde_json::json!({
        "ffmpeg_linkage": inputs.ffmpeg_linkage.as_str(),
        "DVDA_FFMPEG_PREFIX": environment.ffmpeg_prefix.display().to_string(),
        "RUSTFLAGS": serde_json::Value::Null,
    });
    let record = serde_json::json!({
        "version": spec.version,
        "target": target_label(spec.target),
        "target_triple": spec.target,
        "single_exe": spec.single_exe,
        "files": records,
        "component_manifests": sources,
        "inputs": input_record,
        "build_environment": environment_record,
    });
    fs::write(candidate.join("MANIFEST.txt"), rows).map_err(io_error)?;
    fs::write(
        candidate.join("release-build.json"),
        serde_json::to_vec_pretty(&record).map_err(io_error)?,
    )
    .map_err(io_error)
}

/// Start from the Visual Studio environment but drop every inherited build-input
/// override. A stale `DVDA_FFMPEG_PREFIX` could link another architecture or
/// profile, and ambient `RUSTFLAGS` would override the workspace CRT flags.
/// The validated prefix is then set explicitly for static and shared builds.
fn release_build_environment(
    mut environment: BTreeMap<String, String>,
    ffmpeg_prefix: &Path,
) -> BTreeMap<String, String> {
    environment.remove("DVDA_FFMPEG_PREFIX");
    environment.remove("RUSTFLAGS");
    environment.remove("CARGO_ENCODED_RUSTFLAGS");
    environment.insert(
        "DVDA_FFMPEG_PREFIX".into(),
        ffmpeg_prefix.to_string_lossy().into_owned(),
    );
    environment
}

fn build_gui(
    repository: &Path,
    archive: &Path,
    version: &str,
    target: &str,
    target_directory: &Path,
    arm64: bool,
    ffmpeg_prefix: &Path,
) -> Result<BuildEnvironment, String> {
    let environment = msvc_environment(repository, arm64)?;
    let mut environment = release_build_environment(environment, ffmpeg_prefix);
    if let Some(cargo_bin) =
        env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".cargo/bin"))
    {
        let path = environment.get("PATH").cloned().unwrap_or_default();
        environment.insert("PATH".into(), format!("{};{path}", cargo_bin.display()));
    }
    let mut command = Command::new("cargo");
    command
        .args([
            "build",
            "--manifest-path",
            "rust/Cargo.toml",
            "--target",
            target,
            "--release",
            "--offline",
            "-p",
            "dvda-desktop",
            "--features",
            "rust-mlp",
        ])
        .arg("--target-dir")
        .arg(target_directory)
        .current_dir(repository)
        .env_remove("DVDA_FFMPEG_PREFIX")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .envs(&environment)
        .env("DVDA_RUNTIME_ARCHIVE", archive)
        .env("DVDA_PRODUCT_VERSION", version);
    command.env_remove("DVDA_MAGICK_WORK");
    command.env_remove("DVDA_IMAGE_DEPS_DIR");
    let status = command.status().map_err(io_error)?;
    if status.success() {
        Ok(BuildEnvironment {
            ffmpeg_prefix: ffmpeg_prefix.to_path_buf(),
        })
    } else {
        Err("Rust GUI release build failed".into())
    }
}

fn msvc_environment(repository: &Path, arm64: bool) -> Result<BTreeMap<String, String>, String> {
    let mut command = Command::new("python");
    command
        .args(["-X", "utf8"])
        .arg(repository.join("tools/win-build/msvc_env.py"))
        .arg("--json")
        .args(if arm64 {
            vec!["--arch", "arm64"]
        } else {
            Vec::new()
        })
        .current_dir(repository);
    let output = command
        .output()
        .map_err(|error| format!("Initialize Visual Studio MSVC build environment: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Visual Studio MSVC build environment initialization failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Invalid Visual Studio environment response: {error}"))
}

fn compress_zip(stage: &Path, archive: &Path) -> Result<(), String> {
    let script = format!(
        "$ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue'; Compress-Archive -Path '{}' -DestinationPath '{}' -CompressionLevel Optimal -Force",
        stage.join("*").display().to_string().replace('\'', "''"),
        archive.display().to_string().replace('\'', "''")
    );
    let status = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .map_err(io_error)?;
    if status.success() {
        Ok(())
    } else {
        Err("ZIP creation failed".into())
    }
}

fn copy_first_file(sources: &[PathBuf], destination: &Path) -> Result<(), String> {
    for source in sources {
        if source.is_file() {
            return copy_file(source, destination);
        }
    }
    Err(format!(
        "Required release file is missing: {}",
        destination.display()
    ))
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err(format!(
            "Required release file is missing: {}",
            source.display()
        ));
    }
    if destination.is_file() {
        if fs::read(source).map_err(io_error)? == fs::read(destination).map_err(io_error)? {
            return Ok(());
        }
        return Err(format!(
            "Conflicting runtime files: {} and {}",
            source.display(),
            destination.display()
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    fs::copy(source, destination).map_err(io_error)?;
    Ok(())
}

fn required_dir(path: PathBuf, label: &str) -> Result<PathBuf, String> {
    if path.is_dir() {
        Ok(path)
    } else {
        Err(format!("{label} directory is missing: {}", path.display()))
    }
}

fn canonical(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize().map_err(io_error)
}

fn normal_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    let Some(rest) = text.strip_prefix("\\\\?\\") else {
        return path;
    };
    if let Some(unc) = rest.strip_prefix("UNC\\") {
        PathBuf::from(format!("\\\\{unc}"))
    } else {
        PathBuf::from(rest)
    }
}

fn absolutize(repository: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        repository.join(path)
    }
}

fn io_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn print_usage() {
    println!(
        "Usage: dvda-toolchain package [--repo PATH] [--output PATH] [--target-dir PATH] [--media-runtime PATH] [--image-runtime PATH] [--image-author PATH] [--ffmpeg-prefix PATH] --prebuilt PATH --source PATH [--target TRIPLE] [--version v1.0] [--single-exe]\n       dvda-toolchain verify-aob --source FILE [--source FILE ...] --aob FILE [--aob FILE ...] [--lpcm --title-ends 0,1,...]\n       dvda-toolchain font <extract|verify|inspect|pack|verify-collection> ..."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use validation::tests::Fixture;

    #[test]
    fn retired_magick_options_fail_before_package_io() {
        for option in ["--magick-work", "--image-deps"] {
            let error =
                run_args(vec!["package".into(), option.into(), "unused".into()]).unwrap_err();
            assert!(error.contains("is retired"), "{error}");
        }
    }

    #[test]
    fn isolated_target_directory_is_parsed_and_resolved() {
        let options =
            parse_args(vec!["--target-dir".into(), "build/isolated target".into()].into_iter())
                .unwrap();
        let repository = Path::new("C:/repo");
        assert_eq!(
            input_path(
                repository,
                options.target_directory,
                Some("ignored".into()),
                "rust/target"
            ),
            repository.join("build/isolated target")
        );
        assert_eq!(
            input_path(
                repository,
                None,
                Some("build/environment".into()),
                "rust/target"
            ),
            repository.join("build/environment")
        );
        assert_eq!(
            input_path(repository, None, None, "rust/target"),
            repository.join("rust/target")
        );
    }

    #[test]
    fn version_paths_and_missing_values() {
        for version in ["v1.0", "v1.1.0", "v1.1.0-rc.1", "v12.0-beta-2"] {
            assert!(validate_version(version).is_ok());
        }
        for version in [
            "",
            "1.0",
            "v1",
            "v1.0.0.0",
            "v1.0/evil",
            "v1.0\\evil",
            "v1.0:",
            "v1.0-",
            "v1.0-rc..1",
        ] {
            assert!(validate_version(version).is_err());
        }
        for option in ["--source", "--prebuilt", "--version", "--target-dir"] {
            assert!(parse_args(vec![option.into()].into_iter()).is_err());
            assert!(parse_args(vec![option.into(), String::new()].into_iter()).is_err());
            assert!(parse_args(vec![option.into(), "--repo".into()].into_iter()).is_err());
        }
        let repo = Path::new("C:/repo");
        assert_eq!(
            input_path(repo, None, None, "tools/win-build/prebuilt"),
            repo.join("tools/win-build/prebuilt")
        );
        assert_eq!(
            input_path(repo, None, Some("source 中文".into()), "unused"),
            repo.join("source 中文")
        );
        assert_eq!(
            input_path(
                repo,
                Some("chosen space".into()),
                Some("ignored".into()),
                "unused"
            ),
            repo.join("chosen space")
        );
        for option in ["--formats-runtime", "--encoder-runtime"] {
            assert!(parse_args(vec![option.into(), "unused".into()].into_iter()).is_err());
        }
    }

    fn package_args(f: &Fixture, output: &Path) -> Vec<String> {
        let path = |name: &str| f.root.join(name).to_string_lossy().into_owned();
        vec![
            "package".into(),
            "--repo".into(),
            f.root.to_string_lossy().into_owned(),
            "--media-runtime".into(),
            path("media"),
            "--image-runtime".into(),
            path("image"),
            "--image-author".into(),
            path("author"),
            "--prebuilt".into(),
            path("prebuilt"),
            "--source".into(),
            path("source"),
            "--output".into(),
            output.to_string_lossy().into_owned(),
        ]
    }

    fn entry(f: &Fixture) -> Result<(), String> {
        run_args(package_args(f, &f.root.join("output")))
    }

    struct SpecPaths {
        target_directory: PathBuf,
        media: PathBuf,
        image: PathBuf,
        author: PathBuf,
        ffmpeg_prefix: PathBuf,
        prebuilt: PathBuf,
        source: PathBuf,
    }

    impl SpecPaths {
        fn new(f: &Fixture) -> Self {
            let path = |name: &str| f.root.join(name);
            Self {
                target_directory: path("build/target"),
                media: path("media"),
                image: path("image"),
                author: path("author"),
                ffmpeg_prefix: path("media"),
                prebuilt: path("prebuilt"),
                source: path("source"),
            }
        }

        fn spec(
            &self,
            version: &'static str,
            suffix: &'static str,
            target: &'static str,
            single_exe: bool,
        ) -> ReleaseSpec<'_> {
            ReleaseSpec {
                version,
                suffix,
                target,
                single_exe,
                target_directory: &self.target_directory,
                media: &self.media,
                image: &self.image,
                author: &self.author,
                ffmpeg_prefix: &self.ffmpeg_prefix,
                prebuilt: &self.prebuilt,
                source: &self.source,
            }
        }
    }

    #[test]
    fn package_requires_explicit_prebuilt_and_source() {
        let error = run_args(vec!["package".into()]).unwrap_err();
        assert!(error.contains("--prebuilt"), "{error}");
        let error = run_args(vec![
            "package".into(),
            "--prebuilt".into(),
            "prebuilt".into(),
        ])
        .unwrap_err();
        assert!(error.contains("--source"), "{error}");
    }

    #[test]
    fn only_explicit_fonts_and_menu_directories_are_accepted() {
        let f = validation::tests::rust_author_fixture();
        let output = f.root.join("output");
        let prebuilt = f.root.join("prebuilt");
        let source = f.root.join("source");
        // Historical alternate layouts must not be substituted silently.
        fs::create_dir_all(source.join("fonts")).unwrap();
        fs::create_dir_all(prebuilt.join("menu")).unwrap();
        let error = run_args(package_args(&f, &output)).unwrap_err();
        assert!(error.contains("menu fonts"), "{error}");
        assert!(!output.exists(), "release candidate must not be created");
        fs::create_dir_all(prebuilt.join("fonts")).unwrap();
        let error = run_args(package_args(&f, &output)).unwrap_err();
        assert!(error.contains("menu data"), "{error}");
        assert!(!output.exists(), "release candidate must not be created");
    }

    #[test]
    fn shared_media_package_requires_explicit_ffmpeg_prefix() {
        let f = validation::tests::rust_author_fixture();
        let output = f.root.join("output");
        fs::create_dir_all(f.root.join("prebuilt/fonts")).unwrap();
        fs::create_dir_all(f.root.join("source/menu")).unwrap();
        let error = run_args(package_args(&f, &output)).unwrap_err();
        assert!(error.contains("--ffmpeg-prefix"), "{error}");
        assert!(!output.exists(), "release candidate must not be created");
        let mut args = package_args(&f, &output);
        args.push("--ffmpeg-prefix".into());
        args.push(
            f.root
                .join("missing-ffmpeg-prefix")
                .to_string_lossy()
                .into_owned(),
        );
        let error = run_args(args).unwrap_err();
        assert!(error.contains("FFmpeg prefix"), "{error}");
        assert!(!output.exists(), "release candidate must not be created");
    }

    #[test]
    fn ffmpeg_prefix_comes_from_static_media_or_authenticated_shared_input() {
        let f = Fixture::new();
        let media = f.root.join("media");
        fs::create_dir_all(&media).unwrap();
        let mut static_inputs = validation::Inputs {
            files: BTreeMap::new(),
            records: vec![],
            ffmpeg_linkage: validation::FfmpegLinkage::Static,
        };
        assert_eq!(
            resolve_ffmpeg_prefix(&media, None, &mut static_inputs, "x86_64-pc-windows-msvc")
                .unwrap(),
            media
        );
        assert_eq!(
            resolve_ffmpeg_prefix(
                &media,
                Some(media.clone()),
                &mut static_inputs,
                "x86_64-pc-windows-msvc"
            )
            .unwrap(),
            media
        );
        let other = f.root.join("other");
        fs::create_dir_all(&other).unwrap();
        assert!(
            resolve_ffmpeg_prefix(
                &media,
                Some(other),
                &mut static_inputs,
                "x86_64-pc-windows-msvc"
            )
            .unwrap_err()
            .contains("must not replace")
        );
        let prefix = validation::tests::shared_ffmpeg_fixture(&f.root.join("shared-ffmpeg"));
        let mut shared_inputs = validation::Inputs {
            files: BTreeMap::new(),
            records: vec![],
            ffmpeg_linkage: validation::FfmpegLinkage::Shared,
        };
        assert!(
            resolve_ffmpeg_prefix(&media, None, &mut shared_inputs, "x86_64-pc-windows-msvc")
                .unwrap_err()
                .contains("--ffmpeg-prefix")
        );
        assert_eq!(
            resolve_ffmpeg_prefix(
                &media,
                Some(prefix.clone()),
                &mut shared_inputs,
                "x86_64-pc-windows-msvc"
            )
            .unwrap(),
            prefix
        );
        assert_eq!(
            shared_inputs.records,
            vec![prefix.join("build-manifest.json")]
        );
    }

    #[test]
    fn staged_font_must_match_author_authenticated_font() {
        let f = Fixture::new();
        let runtime = f.root.join("runtime");
        fs::create_dir_all(runtime.join("fonts")).unwrap();
        fs::write(runtime.join("fonts/DvdaNotoCJK-Regular.ttc"), b"same font").unwrap();
        let author_font = f.root.join("author-font.ttc");
        fs::write(&author_font, b"same font").unwrap();
        let mut files = BTreeMap::new();
        files.insert("fonts/DvdaNotoCJK-Regular.ttc".into(), author_font.clone());
        let inputs = validation::Inputs {
            files,
            records: vec![],
            ffmpeg_linkage: validation::FfmpegLinkage::Shared,
        };
        verify_staged_font(&runtime, &inputs).unwrap();
        fs::write(&author_font, b"different font").unwrap();
        assert!(
            verify_staged_font(&runtime, &inputs)
                .unwrap_err()
                .contains("conflicts")
        );
        let inputs = validation::Inputs {
            files: BTreeMap::new(),
            records: vec![],
            ffmpeg_linkage: validation::FfmpegLinkage::Shared,
        };
        verify_staged_font(&runtime, &inputs).unwrap();
    }

    #[test]
    fn shared_release_records_the_validated_ffmpeg_prefix() {
        let f = Fixture::new();
        let files = vec![PathBuf::from("app.exe")];
        fs::write(f.root.join("app.exe"), b"exe").unwrap();
        let inputs = f.validate().unwrap();
        assert_eq!(inputs.ffmpeg_linkage, validation::FfmpegLinkage::Shared);
        let paths = SpecPaths::new(&f);
        let spec = paths.spec("v1.0", "win-x64", "x86_64-pc-windows-msvc", false);
        let environment = BuildEnvironment {
            ffmpeg_prefix: paths.media.clone(),
        };
        write_records(&f.root, &files, &inputs, &spec, &environment).unwrap();
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(f.root.join("release-build.json")).unwrap()).unwrap();
        assert_eq!(record["build_environment"]["ffmpeg_linkage"], "shared");
        let prefix = paths.media.display().to_string();
        assert_eq!(record["build_environment"]["DVDA_FFMPEG_PREFIX"], prefix);
        assert_eq!(record["inputs"]["ffmpeg_prefix"], prefix);
        assert!(record["build_environment"]["RUSTFLAGS"].is_null());
    }

    #[test]
    fn package_entry_rejects_both_import_kinds_before_touching_release() {
        for delayed in [false, true] {
            let f = Fixture::new();
            fs::create_dir(f.root.join("output")).unwrap();
            let old = f.root.join("output/DVD-Audio-Maker-v1.0-win-x64.zip");
            fs::write(&old, b"previous ZIP").unwrap();
            f.replace(
                "media",
                "dvda-media.dll",
                &pe::fixture(
                    true,
                    if delayed { &[] } else { &["missing.dll"] },
                    if delayed { &["missing.dll"] } else { &[] },
                ),
            );
            assert!(entry(&f).unwrap_err().contains("Missing native dependency"));
            assert_eq!(fs::read(old).unwrap(), b"previous ZIP");
            assert_eq!(fs::read_dir(f.root.join("output")).unwrap().count(), 1);
        }
        let f = Fixture::new();
        f.replace("image", "dvda-image.dll", b"broken PE");
        assert!(entry(&f).unwrap_err().contains("DOS header"));
        let f = Fixture::new();
        f.edit("media", |d| d["profile"] = "wrong".into());
        assert!(entry(&f).unwrap_err().contains("profile=shared"));
    }

    #[test]
    fn build_copy_and_zip_failure_leave_old_release_and_hashes_untouched() {
        let f = Fixture::new();
        let output = f.root.join("output");
        fs::create_dir_all(output.join("DVD-Audio-Maker")).unwrap();
        let old_names = [
            "DVD-Audio-Maker/DVD-Audio-Maker.exe",
            "DVD-Audio-Maker-v1.0-win-x64.zip",
            "MANIFEST.txt",
        ];
        for name in old_names {
            fs::write(output.join(name), b"known good").unwrap();
        }
        let paths = SpecPaths::new(&f);
        let inputs = validation::Inputs {
            files: std::collections::BTreeMap::default(),
            records: vec![],
            ffmpeg_linkage: validation::FfmpegLinkage::Shared,
        };
        for reason in ["build", "copy", "zip"] {
            let spec = paths.spec("v1.0", "win-x64", "x86_64-pc-windows-msvc", false);
            let candidate = publication::Candidate::new(&output).unwrap();
            let stage = candidate.root.join("DVD-Audio-Maker");
            fs::create_dir(&stage).unwrap();
            fs::write(
                stage.join("DVD-Audio-Maker.exe"),
                pe::fixture(false, &["kernel32.dll"], &[]),
            )
            .unwrap();
            let result = finish_release(
                &candidate.root,
                &output,
                &inputs,
                &spec,
                || match reason {
                    "build" => Err("build failed".into()),
                    "copy" => {
                        copy_file(
                            &candidate.root.join("missing.exe"),
                            &stage.join("DVD-Audio-Maker.exe"),
                        )?;
                        Ok(BuildEnvironment {
                            ffmpeg_prefix: paths.media.clone(),
                        })
                    }
                    _ => Ok(BuildEnvironment {
                        ffmpeg_prefix: paths.media.clone(),
                    }),
                },
                |_, archive| {
                    fs::write(archive, b"partial ZIP").unwrap();
                    Err("compression interrupted".into())
                },
            );
            assert!(result.is_err());
            for name in old_names {
                assert_eq!(fs::read(output.join(name)).unwrap(), b"known good");
            }
        }
    }

    #[test]
    fn release_build_environment_never_inherits_stale_inputs() {
        let mut inherited = BTreeMap::new();
        inherited.insert("PATH".into(), "C:/tools".into());
        inherited.insert("DVDA_FFMPEG_PREFIX".into(), "C:/stale/ffmpeg".into());
        inherited.insert("RUSTFLAGS".into(), "-C target-feature=+avx2".into());
        inherited.insert(
            "CARGO_ENCODED_RUSTFLAGS".into(),
            "-C\0target-feature=+avx2".into(),
        );
        let media = Path::new("C:/repo/build/msvc-native/ffmpeg-static/install");
        let environment = release_build_environment(inherited, media);
        assert_eq!(
            environment.get("PATH").map(String::as_str),
            Some("C:/tools")
        );
        assert_eq!(
            environment.get("DVDA_FFMPEG_PREFIX").map(String::as_str),
            Some("C:/repo/build/msvc-native/ffmpeg-static/install")
        );
        assert!(!environment.contains_key("RUSTFLAGS"));
        assert!(!environment.contains_key("CARGO_ENCODED_RUSTFLAGS"));
    }

    #[expect(
        clippy::too_many_lines,
        reason = "End-to-end publication and record assertions stay together"
    )]
    #[test]
    fn single_exe_publication_writes_only_real_files_with_traceable_inputs() {
        let f = Fixture::new();
        let output = f.root.join("output");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("user-notes.txt"), b"keep").unwrap();
        fs::create_dir(output.join("DVD-Audio-Maker")).unwrap();
        fs::write(output.join("DVD-Audio-Maker/keep.txt"), b"previous release").unwrap();
        let paths = SpecPaths::new(&f);
        let spec = paths.spec("v1.0", "win-x64", "x86_64-pc-windows-msvc", true);
        let inputs = f.validate().unwrap();
        let candidate = publication::Candidate::new(&output).unwrap();
        let stage = candidate.root.join("DVD-Audio-Maker");
        fs::create_dir(&stage).unwrap();
        let executable = pe::fixture(false, &["kernel32.dll"], &[]);
        fs::write(stage.join("DVD-Audio-Maker.exe"), &executable).unwrap();
        finish_release(
            &candidate.root,
            &output,
            &inputs,
            &spec,
            || {
                Ok(BuildEnvironment {
                    ffmpeg_prefix: paths.media.clone(),
                })
            },
            |_, _| panic!("single EXE release must not create a ZIP"),
        )
        .unwrap();
        let exe_name = "DVD-Audio-Maker-v1.0-win-x64.exe";
        assert_eq!(fs::read(output.join(exe_name)).unwrap(), executable);
        assert_eq!(fs::read(output.join("user-notes.txt")).unwrap(), b"keep");
        assert_eq!(
            fs::read(output.join("DVD-Audio-Maker/keep.txt")).unwrap(),
            b"previous release"
        );
        drop(candidate);
        assert!(fs::read_dir(&output).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".package-")
        }));
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("release-build.json")).unwrap()).unwrap();
        assert_eq!(record["version"], "v1.0");
        assert_eq!(record["target"], "Windows x64");
        assert_eq!(record["target_triple"], "x86_64-pc-windows-msvc");
        assert_eq!(record["single_exe"], true);
        let files = record["files"].as_object().unwrap();
        assert_eq!(
            files.keys().map(String::as_str).collect::<Vec<_>>(),
            [exe_name]
        );
        assert_eq!(files[exe_name]["bytes"], executable.len());
        assert_eq!(
            files[exe_name]["sha256"],
            validation::file_hash(&output.join(exe_name)).unwrap()
        );
        assert_eq!(
            record["inputs"]["media_runtime"],
            paths.media.display().to_string()
        );
        assert_eq!(
            record["inputs"]["ffmpeg_prefix"],
            paths.media.display().to_string()
        );
        assert_eq!(
            record["inputs"]["prebuilt"],
            paths.prebuilt.display().to_string()
        );
        assert_eq!(
            record["inputs"]["source"],
            paths.source.display().to_string()
        );
        assert_eq!(
            record["inputs"]["target_directory"],
            paths.target_directory.display().to_string()
        );
        assert_eq!(record["build_environment"]["ffmpeg_linkage"], "shared");
        assert_eq!(
            record["build_environment"]["DVDA_FFMPEG_PREFIX"],
            paths.media.display().to_string()
        );
        assert!(record["build_environment"]["RUSTFLAGS"].is_null());
        assert!(
            record["component_manifests"]
                .as_array()
                .is_some_and(|entries| !entries.is_empty())
        );
        let manifest = fs::read_to_string(output.join("MANIFEST.txt")).unwrap();
        let rows: Vec<_> = manifest
            .lines()
            .map(|line| line.split_once("  ").unwrap())
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].0,
            validation::file_hash(&output.join(exe_name)).unwrap()
        );
        assert_eq!(rows[0].1, exe_name);
        // The single-EXE report must not depend on a release directory.
        print_release(&spec, &output).unwrap();
    }

    #[test]
    fn single_exe_failure_preserves_previous_release() {
        let f = Fixture::new();
        let output = f.root.join("output");
        fs::create_dir(&output).unwrap();
        let old_names = [
            "DVD-Audio-Maker-v1.0-win-x64.exe",
            "MANIFEST.txt",
            "release-build.json",
        ];
        for name in old_names {
            fs::write(output.join(name), b"known good").unwrap();
        }
        let paths = SpecPaths::new(&f);
        let inputs = f.validate().unwrap();
        for reason in ["build", "copy"] {
            let spec = paths.spec("v1.0", "win-x64", "x86_64-pc-windows-msvc", true);
            let candidate = publication::Candidate::new(&output).unwrap();
            let stage = candidate.root.join("DVD-Audio-Maker");
            fs::create_dir(&stage).unwrap();
            if reason == "build" {
                fs::write(
                    stage.join("DVD-Audio-Maker.exe"),
                    pe::fixture(false, &["kernel32.dll"], &[]),
                )
                .unwrap();
            }
            let result = finish_release(
                &candidate.root,
                &output,
                &inputs,
                &spec,
                || match reason {
                    "build" => Err("injected build failure".into()),
                    _ => Ok(BuildEnvironment {
                        ffmpeg_prefix: paths.media.clone(),
                    }),
                },
                |_, _| panic!("single EXE release must not create a ZIP"),
            );
            assert!(result.is_err());
            for name in old_names {
                assert_eq!(fs::read(output.join(name)).unwrap(), b"known good");
            }
        }
    }

    #[test]
    fn generated_hash_records_track_every_final_file() {
        let f = Fixture::new();
        let files = vec![PathBuf::from("release.zip"), PathBuf::from("app.exe")];
        for file in &files {
            fs::write(f.root.join(file), b"first").unwrap();
        }
        let inputs = f.validate().unwrap();
        let paths = SpecPaths::new(&f);
        let spec = paths.spec("v1.0", "win-x64", "x86_64-pc-windows-msvc", false);
        let environment = BuildEnvironment {
            ffmpeg_prefix: paths.media.clone(),
        };
        write_records(&f.root, &files, &inputs, &spec, &environment).unwrap();
        let old = fs::read(f.root.join("MANIFEST.txt")).unwrap();
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(f.root.join("release-build.json")).unwrap()).unwrap();
        for file in &files {
            assert_eq!(
                record["files"][file.to_str().unwrap()]["sha256"],
                validation::file_hash(&f.root.join(file)).unwrap()
            );
        }
        assert_eq!(record["target_triple"], "x86_64-pc-windows-msvc");
        assert_eq!(record["single_exe"], false);
        assert_eq!(
            record["inputs"]["source"],
            paths.source.display().to_string()
        );
        assert_eq!(
            record["build_environment"]["DVDA_FFMPEG_PREFIX"],
            paths.media.display().to_string()
        );
        fs::write(f.root.join("app.exe"), b"other").unwrap();
        write_records(&f.root, &files, &inputs, &spec, &environment).unwrap();
        assert_ne!(old, fs::read(f.root.join("MANIFEST.txt")).unwrap());
    }
}
