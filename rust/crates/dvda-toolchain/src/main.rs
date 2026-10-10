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
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Default, Debug)]
struct Args {
    repository: Option<PathBuf>,
    output: Option<PathBuf>,
    media_runtime: Option<PathBuf>,
    image_runtime: Option<PathBuf>,
    /// Explicit legacy encoder oracle; it is never embedded in linked releases.
    encoder_runtime: Option<PathBuf>,
    image_author: Option<PathBuf>,
    source: Option<PathBuf>,
    prebuilt: Option<PathBuf>,
    version: String,
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
        ..Args::default()
    };
    while let Some(argument) = args.next() {
        let value = next_arg(&mut args, &argument)?;
        match argument.as_str() {
            "--repo" => options.repository = Some(value.into()),
            "--output" => options.output = Some(value.into()),
            "--media-runtime" => options.media_runtime = Some(value.into()),
            "--image-runtime" => options.image_runtime = Some(value.into()),
            "--encoder-runtime" => options.encoder_runtime = Some(value.into()),
            "--image-author" => options.image_author = Some(value.into()),
            "--source" => options.source = Some(value.into()),
            "--prebuilt" => options.prebuilt = Some(value.into()),
            "--version" => options.version = value,
            _ => return Err(format!("Unknown option: {argument}")),
        }
    }
    validate_version(&options.version)?;
    Ok(options)
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

fn package(options: Args) -> Result<(), String> {
    validate_version(&options.version)?;
    let repository = options
        .repository
        .or_else(|| env::current_dir().ok())
        .ok_or("Cannot determine repository directory")?;
    let repository = normal_path(canonical(&repository)?);
    let target = repository.join("rust/target/x86_64-pc-windows-gnu/release");
    let output = options
        .output
        .map(|path| absolutize(&repository, path))
        .unwrap_or_else(|| repository.join("tools/win-build/release-onefile"));
    let media = required_dir(
        input_path(
            &repository,
            options.media_runtime,
            None,
            "build/media-native-shared",
        ),
        "media runtime",
    )?;
    let image = required_dir(
        input_path(
            &repository,
            options.image_runtime,
            None,
            "build/image-native",
        ),
        "image runtime",
    )?;
    let author = required_dir(
        input_path(
            &repository,
            options.image_author,
            None,
            "build/rust-author-current",
        ),
        "author runtime",
    )?;
    let source = input_path(
        &repository,
        options.source,
        env::var_os("DVDA_SRC_TREE"),
        "tools/dvda-author-mlp8",
    );
    let prebuilt = input_path(
        &repository,
        options.prebuilt,
        env::var_os("DVDA_PREBUILT_DIR"),
        "tools/win-build/prebuilt",
    );
    // All provenance and dependency checks precede any changes to release files.
    let mut inputs = validation::validate(&media, &image, &author)?;
    // Optional encoder artifacts are authenticated as oracles, never shipped.
    if let Some(encoder) = options.encoder_runtime {
        let encoder = required_dir(absolutize(&repository, encoder), "encoder oracle")?;
        inputs
            .records
            .extend(validation::validate_encoder(&encoder)?.records);
    }
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
    let candidate = publication::Candidate::new(&output)?;
    let stage = candidate.root.join("DVD-Audio-Maker");
    let runtime = candidate.root.join("runtime-input");
    fs::create_dir_all(&stage).map_err(io_error)?;
    fs::create_dir_all(&runtime).map_err(io_error)?;
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
            repository.join("THIRD-PARTY.md"),
            repository.join("tools/win-build/docs/THIRD-PARTY.md"),
        ],
        &stage.join("THIRD-PARTY.md"),
    )?;
    copy_first_file(
        &[
            repository.join("THIRD-PARTY.en.md"),
            repository.join("tools/win-build/docs/THIRD-PARTY.en.md"),
        ],
        &stage.join("THIRD-PARTY.en.md"),
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

    for (name, path) in &inputs.files {
        copy_file(path, &runtime.join(name))?;
    }
    for name in ["colors.xml", "policy.xml"] {
        copy_file(&image.join(name), &runtime.join(name))?;
    }
    let font_sources = [
        prebuilt.join("fonts"),
        prebuilt.join("menu-bin/fonts"),
        source.join("fonts"),
        source.join("menu/fonts"),
    ];
    fonts::stage_menu_fonts(&font_sources, &runtime.join("fonts"))?;
    let data_menu = [source.join("menu"), prebuilt.join("menu")]
        .into_iter()
        .find(|path| path.is_dir());
    if let Some(data_menu) = data_menu {
        for name in [
            "activeheader",
            "black_NTSC_720x480.jpg",
            "black_NTSC_720x480.png",
            "black_PAL_720x576.jpg",
            "black_PAL_720x576.png",
            "silence.wav",
        ] {
            copy_file(&data_menu.join(name), &runtime.join("data/menu").join(name))?;
        }
    } else {
        return Err(format!(
            "Required menu data is missing: {} or {}; supply --source or --prebuilt",
            source.join("menu").display(),
            prebuilt.join("menu").display()
        ));
    }
    // Relative font paths keep the archive portable across computers/cache roots.
    let font = runtime.join("fonts/DvdaNotoCJK-Regular.ttc");
    if !font.is_file() {
        return Err("Required shared CJK font collection is missing".into());
    }
    let mut typemap = String::from("<typemap>");
    for (face, language) in [(0, "SC"), (1, "JP"), (2, "KR")] {
        for prefix in ["DVDA-", ""] {
            typemap.push_str(&format!("<type name=\"{prefix}Noto-Sans-CJK-{language}\" family=\"Noto Sans CJK {language}\" format=\"truetype\" style=\"normal\" stretch=\"normal\" weight=\"400\" face=\"{face}\" glyphs=\"fonts/DvdaNotoCJK-Regular.ttc\"/>"));
        }
    }
    typemap.push_str("</typemap>");
    fs::write(runtime.join("type.xml"), typemap).map_err(io_error)?;
    validation::validate_staged(&runtime)?;
    let embedded = candidate.root.join("runtime.bin");
    fs::write(&embedded, dvda_core::runtime::pack(&runtime)?).map_err(io_error)?;
    let archive_name = format!("DVD-Audio-Maker-{}-win-x64.zip", options.version);
    finish_release(
        &candidate.root,
        &output,
        &inputs,
        &options.version,
        || {
            build_gui(&repository, &embedded, &options.version)?;
            copy_file(
                &target.join("dvda-desktop.exe"),
                &stage.join("DVD-Audio-Maker.exe"),
            )
        },
        compress_zip,
    )?;
    println!(
        "[OK] Release directory: {}",
        output.join("DVD-Audio-Maker").display()
    );
    println!("[OK] Release ZIP: {}", output.join(archive_name).display());
    println!(
        "[OK] EXE SHA-256: {}",
        validation::file_hash(&output.join("DVD-Audio-Maker/DVD-Audio-Maker.exe"))?
    );
    println!(
        "[OK] Hash/provenance records (outside ZIP): {}",
        output.join("MANIFEST.txt").display()
    );
    Ok(())
}

fn finish_release(
    candidate: &Path,
    output: &Path,
    inputs: &validation::Inputs,
    version: &str,
    build: impl FnOnce() -> Result<(), String>,
    compress: impl FnOnce(&Path, &Path) -> Result<(), String>,
) -> Result<(), String> {
    build()?;
    let stage = candidate.join("DVD-Audio-Maker");
    let runtime = candidate.join("runtime-input");
    if runtime.is_dir() {
        validation::validate_combined(&[&stage, &runtime])?;
    } else {
        validation::validate_staged(&stage)?;
    }
    let archive_name = format!("DVD-Audio-Maker-{version}-win-x64.zip");
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
    write_records(candidate, &relative_files, inputs, version)?;
    relative_files.push(PathBuf::from("MANIFEST.txt"));
    relative_files.push(PathBuf::from("release-build.json"));
    publication::commit(candidate, output, &relative_files)
}

fn write_records(
    candidate: &Path,
    files: &[PathBuf],
    inputs: &validation::Inputs,
    version: &str,
) -> Result<(), String> {
    let mut rows = String::new();
    let mut records = serde_json::Map::new();
    for relative in files {
        let path = candidate.join(relative);
        let digest = validation::file_hash(&path)?;
        let name = relative.to_string_lossy().replace('\\', "/");
        rows.push_str(&format!("{digest}  {name}\n"));
        records.insert(name,serde_json::json!({"bytes":fs::metadata(path).map_err(io_error)?.len(),"sha256":digest}));
    }
    let sources: Vec<_> = inputs
        .records
        .iter()
        .map(|path| Ok(serde_json::json!({"path":path,"sha256":validation::file_hash(path)?})))
        .collect::<Result<_, String>>()?;
    fs::write(candidate.join("MANIFEST.txt"), rows).map_err(io_error)?;
    fs::write(candidate.join("release-build.json"),serde_json::to_vec_pretty(&serde_json::json!({"version":version,"target":"Windows x64","files":records,"component_manifests":sources})).map_err(io_error)?).map_err(io_error)
}

fn build_gui(repository: &Path, archive: &Path, version: &str) -> Result<(), String> {
    let status = Command::new("cargo")
        .args([
            "build",
            "--manifest-path",
            "rust/Cargo.toml",
            "--target",
            "x86_64-pc-windows-gnu",
            "--release",
            "--offline",
            "-p",
            "dvda-desktop",
            "--features",
            "direct-bridges,rust-mlp",
        ])
        .current_dir(repository)
        .env("DVDA_RUNTIME_ARCHIVE", archive)
        .env("DVDA_PRODUCT_VERSION", version)
        .status()
        .map_err(io_error)?;
    if status.success() {
        Ok(())
    } else {
        Err("Rust GUI release build failed".into())
    }
}

fn compress_zip(stage: &Path, archive: &Path) -> Result<(), String> {
    let script = format!(
        "$ErrorActionPreference = 'Stop'; Compress-Archive -Path '{}' -DestinationPath '{}' -CompressionLevel Optimal -Force",
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
        "Usage: dvda-toolchain package [--repo PATH] [--output PATH] [--media-runtime PATH] [--image-runtime PATH] [--encoder-runtime PATH] [--image-author PATH] [--source PATH] [--prebuilt PATH] [--version v1.0]\n       dvda-toolchain verify-aob --source FILE [--source FILE ...] --aob FILE [--aob FILE ...] [--lpcm --title-ends 0,1,...]\n       dvda-toolchain font <extract|verify|inspect|pack|verify-collection> ..."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use validation::tests::Fixture;

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
        for option in ["--source", "--prebuilt", "--version", "--encoder-runtime"] {
            assert!(parse_args(vec![option.into()].into_iter()).is_err());
            assert!(parse_args(vec![option.into(), String::new()].into_iter()).is_err());
            assert!(parse_args(vec![option.into(), "--repo".into()].into_iter()).is_err());
        }
        let repo = Path::new("C:/repo");
        assert_eq!(
            input_path(repo, None, None, "tools/dvda-author-mlp8"),
            repo.join("tools/dvda-author-mlp8")
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
        assert!(parse_args(vec!["--formats-runtime".into(), "unused".into()].into_iter()).is_err());
    }

    fn entry(f: &Fixture) -> Result<(), String> {
        run_args(vec![
            "package".into(),
            "--repo".into(),
            f.root.to_string_lossy().into_owned(),
            "--media-runtime".into(),
            "media".into(),
            "--image-runtime".into(),
            "image".into(),
            "--image-author".into(),
            "author".into(),
            "--encoder-runtime".into(),
            "encoder".into(),
            "--output".into(),
            "output".into(),
        ])
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
                pe::fixture(
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
        f.replace("image", "dvda-image.dll", b"broken PE".to_vec());
        assert!(entry(&f).unwrap_err().contains("DOS header"));
        let f = Fixture::new();
        f.edit("media", |d| d["profile"] = "wrong".into());
        assert!(entry(&f).unwrap_err().contains("profile=shared"));
    }

    #[test]
    fn package_entry_rejects_encoder_before_touching_release() {
        for failure in ["legacy", "manifest", "tamper", "import", "delay_import"] {
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
            match failure {
                "legacy" => f.edit("encoder", |record| record["implementation"] = "c".into()),
                "manifest" => {
                    fs::remove_file(f.root.join("encoder/encoder-build.json")).unwrap();
                }
                "tamper" => {
                    fs::write(f.root.join("encoder/mlp_encoder.dll"), b"tampered").unwrap();
                }
                _ => f.replace(
                    "encoder",
                    "mlp_encoder.dll",
                    pe::fixture(
                        true,
                        if failure == "import" {
                            &["missing.dll"]
                        } else {
                            &[]
                        },
                        if failure == "delay_import" {
                            &["missing.dll"]
                        } else {
                            &[]
                        },
                    ),
                ),
            }
            assert!(entry(&f).is_err(), "{failure}");
            for name in old_names {
                assert_eq!(fs::read(output.join(name)).unwrap(), b"known good");
            }
            assert_eq!(fs::read_dir(&output).unwrap().count(), 3);
        }
    }

    #[test]
    fn package_entry_rejects_menu_before_touching_release() {
        for failure in ["legacy", "missing", "unauthenticated", "tamper", "mismatch"] {
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
            match failure {
                "legacy" => f.edit_menu(|record| record["adapter"] = "c".into()),
                "missing" => fs::remove_file(f.root.join("author/menu-build.json")).unwrap(),
                "unauthenticated" => f.edit("author", |record| {
                    record["source_inputs"] = serde_json::Value::Null
                }),
                "tamper" => fs::write(f.root.join("author/menu-build.json"), b"tampered").unwrap(),
                _ => f.edit_menu(|record| {
                    record["files"]["dvda-menu-spu.dll"]["sha256"] = "0".repeat(64).into()
                }),
            }
            assert!(entry(&f).is_err(), "{failure}");
            for name in old_names {
                assert_eq!(fs::read(output.join(name)).unwrap(), b"known good");
            }
            assert_eq!(fs::read_dir(&output).unwrap().count(), 3);
        }
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
        let inputs = validation::Inputs {
            files: Default::default(),
            records: vec![],
        };
        for reason in ["build", "copy", "zip"] {
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
                "v1.0",
                || match reason {
                    "build" => Err("build failed".into()),
                    "copy" => copy_file(
                        &candidate.root.join("missing.exe"),
                        &stage.join("DVD-Audio-Maker.exe"),
                    ),
                    _ => Ok(()),
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
    fn generated_hash_records_track_every_final_file() {
        let f = Fixture::new();
        let files = vec![PathBuf::from("release.zip"), PathBuf::from("app.exe")];
        for file in &files {
            fs::write(f.root.join(file), b"first").unwrap();
        }
        let inputs = f.validate().unwrap();
        write_records(&f.root, &files, &inputs, "v1.0").unwrap();
        let old = fs::read(f.root.join("MANIFEST.txt")).unwrap();
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(f.root.join("release-build.json")).unwrap()).unwrap();
        for file in &files {
            assert_eq!(
                record["files"][file.to_str().unwrap()]["sha256"],
                validation::file_hash(&f.root.join(file)).unwrap()
            );
        }
        fs::write(f.root.join("app.exe"), b"other").unwrap();
        write_records(&f.root, &files, &inputs, "v1.0").unwrap();
        assert_ne!(old, fs::read(f.root.join("MANIFEST.txt")).unwrap());
    }
}
