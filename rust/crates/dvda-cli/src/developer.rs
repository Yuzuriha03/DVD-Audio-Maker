//! Human-oriented developer commands; JSON operation dispatch remains available.
use dvda_core::{
    app::AppOptions,
    audio, conversion, formats,
    preparation::{alac, state},
};
use dvda_native::media::Callbacks;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

static CANCELLED: AtomicBool = AtomicBool::new(false);
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}
unsafe extern "system" fn control(event: u32) -> i32 {
    if event <= 1 {
        CANCELLED.store(true, Ordering::Release);
        1
    } else {
        0
    }
}
struct Events(String);
impl Callbacks for Events {
    fn emit(&mut self, stream: i32, text: &str) {
        let text = dvda_core::localization::translate(&self.0, text);
        if stream == 2 {
            eprintln!("{text}");
        } else {
            println!("{text}");
        }
    }
    fn cancelled(&mut self) -> bool {
        CANCELLED.load(Ordering::Acquire)
    }
}
pub fn accepts(args: &[String]) -> bool {
    args.first().is_none_or(|s| {
        matches!(
            s.as_str(),
            "config"
                | "prepare"
                | "plan"
                | "build"
                | "verify"
                | "quick-check"
                | "audit"
                | "convert"
                | "m4a2flac"
                | "alac"
                | "iso"
                | "mlp"
                | "aob-pts"
                | "help"
        ) || s.starts_with("--")
    })
}
pub fn run(args: &[String]) -> i32 {
    // SAFETY: handler accesses only a static atomic and has the Win32 callback ABI.
    unsafe {
        SetConsoleCtrlHandler(Some(control), 1);
    }
    match execute(args) {
        Ok(code) => code,
        Err((code, error)) => {
            eprintln!("[ERROR] {error}");
            code
        }
    }
}
type Result<T> = std::result::Result<T, (i32, String)>;
fn failure(error: impl ToString) -> (i32, String) {
    (1, error.to_string())
}
fn usage(error: impl ToString) -> (i32, String) {
    (2, error.to_string())
}
fn print(value: impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value).map_err(failure)?);
    Ok(())
}
fn take(args: &mut Vec<String>, name: &str) -> Result<Option<String>> {
    let Some(index) = args.iter().position(|a| a == name) else {
        return Ok(None);
    };
    if args.iter().filter(|a| *a == name).count() != 1 {
        return Err(usage(format!("Duplicate {name}")));
    }
    if args.get(index + 1).is_none_or(|v| v.starts_with("--")) {
        return Err(usage(format!("{name} requires a value")));
    }
    let value = args.remove(index + 1);
    args.remove(index);
    Ok(Some(value))
}
fn flag(args: &mut Vec<String>, name: &str) -> Result<bool> {
    let count = args.iter().filter(|a| *a == name).count();
    if count > 1 {
        return Err(usage(format!("Duplicate {name}")));
    }
    args.retain(|a| a != name);
    Ok(count == 1)
}
fn empty(args: &[String]) -> Result<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(usage(format!("Unexpected argument: {}", args[0])))
    }
}
fn execute(arguments: &[String]) -> Result<i32> {
    let mut args = arguments.to_vec();
    let profile = take(&mut args, "--profile")?;
    let config = take(&mut args, "--config")?;
    if profile.is_some() && config.is_some() {
        return Err(usage("Specify only one JSON profile"));
    }
    let language = take(&mut args, "--language")?.or_else(|| std::env::var("DVDA_LANGUAGE").ok());
    let command = if args.first().is_some_and(|a| !a.starts_with('-')) {
        args.remove(0)
    } else {
        "config".into()
    };
    if command == "help" || args.iter().any(|a| matches!(a.as_str(), "--help" | "-h")) {
        println!(
            "Usage: dvda-cli [config|prepare|plan|build|verify|quick-check|audit|convert|aob-pts|mlp|alac|iso] [--profile settings.json] [--language zh-CN|en|ja]\n  build [--dry-run] [--no-resume]\n  convert PATH... [--in-place] [--dry-run] [--level 0..8] [--jobs N]\n  mlp (--check|--align) [-o DIR] [-q] FILE...\n  alac check INPUT | alac repair INPUT [OUTPUT]\n  iso list ISO [INNER] | iso extract ISO INNER OUTPUT\n  quick-check|audit [--iso-dir DIR] [--manifest FILE] [--log FILE]"
        );
        return Ok(0);
    }
    let mut options =
        AppOptions::load(profile.or(config).as_deref().map(Path::new)).map_err(usage)?;
    if let Some(language) = language {
        options.language = dvda_core::config_files::normalize_language(
            Some(&language),
            options.language.split('-').next().unwrap_or("en"),
        )
        .map_err(|_| usage("Unsupported language"))?;
    }
    let mut events = Events(options.language.clone());
    match command.as_str() {
        "config" => configuration(&options, &mut args),
        "prepare" => {
            let force = flag(&mut args, "--force")?;
            empty(&args)?;
            let result = dvda_core::app::run_prepare(&options, force, &mut events);
            let code = i32::from(!result.succeeded());
            print(result)?;
            Ok(code)
        }
        "build" => {
            let dry = flag(&mut args, "--dry-run")?;
            if flag(&mut args, "--no-resume")? {
                options = options
                    .with_profile_values(json!({"DVDA_RESUME":"off"}).as_object().unwrap().clone())
                    .map_err(failure)?;
            }
            empty(&args)?;
            let result = dvda_core::app::run_build(&options, dry, &mut events).map_err(failure)?;
            let code = i32::from(!result.succeeded);
            print(result)?;
            Ok(code)
        }
        "plan" => {
            empty(&args)?;
            plan(&options)
        }
        "verify" | "quick-check" | "audit" => {
            verification(&command, &options, &mut args, &mut events)
        }
        "convert" | "m4a2flac" => {
            let dry_run = flag(&mut args, "--dry-run")?;
            let delete_sources = flag(&mut args, "--in-place")?;
            let compression = take(&mut args, "--level")?
                .map(|v| v.parse::<u32>().map_err(usage))
                .transpose()?
                .unwrap_or(8);
            let jobs = take(&mut args, "--jobs")?
                .map(|v| v.parse::<usize>().map_err(usage))
                .transpose()?
                .unwrap_or_else(|| {
                    std::thread::available_parallelism()
                        .map_or(1, usize::from)
                        .min(4)
                });
            if args.is_empty()
                || args.iter().any(|a| a.starts_with('-'))
                || compression > 8
                || jobs == 0
            {
                return Err(usage(
                    "convert requires input paths, level 0–8 and positive jobs",
                ));
            }
            let result = conversion::execute(
                &conversion::Job {
                    library: options.media_library().map_err(failure)?,
                    paths: args.into_iter().map(PathBuf::from).collect(),
                    compression,
                    jobs,
                    dry_run,
                    delete_sources,
                },
                &mut events,
            )
            .map_err(failure)?;
            let code = i32::from(result.iter().any(|r| r.status == "FAIL"));
            print(result)?;
            Ok(code)
        }
        "mlp" => mlp(&mut args),
        "iso" => iso(&args),
        "alac" => alac_command(&options, &args, &mut events),
        "aob-pts" => {
            if args.is_empty() || args.iter().any(|a| a.starts_with('-')) {
                return Err(usage("aob-pts FILE..."));
            }
            let mut code = 0;
            for path in args {
                let result =
                    dvda_core::dispatch("aob.scan_file", json!({"Path":path})).map_err(failure)?;
                if result["Scan"].is_null() {
                    print(json!({"Path":path,"Error":result["ErrorCode"]}))?;
                    code = 1;
                    continue;
                }
                let values: Vec<i64> = result["Scan"]["Values"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(Value::as_i64)
                    .collect();
                let stats = dvda_core::aob::pts_statistics(&values);
                code |= i32::from(!stats.issue_codes.is_empty());
                print(json!({"Path":path,"Scan":result["Scan"],"Statistics":stats}))?;
            }
            Ok(code)
        }
        _ => Err(usage(format!("Unknown command: {command}"))),
    }
}
fn configuration(options: &AppOptions, args: &mut Vec<String>) -> Result<i32> {
    let check = flag(args, "--check")?;
    let shell = flag(args, "--shell")?;
    let all = flag(args, "--shell-all")?;
    empty(args)?;
    if u32::from(check) + u32::from(shell) + u32::from(all) > 1 {
        return Err(usage("Choose one config output mode"));
    }
    if check {
        let missing: Vec<_> = [
            ("DVDA_SRC", "SourceDirectory"),
            ("DVDA_FINAL_DIR", "FinalDirectory"),
        ]
        .into_iter()
        .filter(|(_, property)| options.text(property).is_empty())
        .map(|(key, _)| key)
        .collect();
        if !missing.is_empty() {
            eprintln!(
                "[ERROR] Set {} in the JSON profile (--profile settings.json).",
                missing.join(", ")
            );
        }
        return Ok(if missing.is_empty() { 0 } else { 2 });
    }
    if shell || all {
        // Export evaluated values, including derived paths and clamped numbers.
        // Removed executable overrides deliberately have no shell compatibility key.
        let base = [
            ("DVDA_SRC", "SourceDirectory"),
            ("DVDA_FINAL_DIR", "FinalDirectory"),
            ("DVDA_BUILD_DIR", "BuildDirectory"),
            ("DVDA_TITLE", "Title"),
            ("DVDA_ISO_PREFIX", "IsoPrefix"),
            ("DVDA_MANIFEST", "ManifestPath"),
            ("DVDA_REPORT", "ReportPath"),
            ("DVDA_OUT_ROOT", "OutputRoot"),
            ("DVDA_TMP_ROOT", "TemporaryRoot"),
            ("DVDA_ISO_DIR", "IsoDirectory"),
            ("DVDA_MLP_DIR", "MlpDirectory"),
            ("DVDA_MLP_INDEX", "MlpIndexPath"),
            ("DVDA_ALAC_FIX_DIR", "AlacFixDirectory"),
            ("DVDA_BUILD_LOG", "BuildLogPath"),
            ("DVDA_AUTHOR", "DvdaAuthor"),
            ("DVDA_AUTHOR_SRC", "AuthorSource"),
            ("DVDA_PLANNED_DISCS", "PlannedDiscs"),
            ("DVDA_GROUP_TRACK_LIMIT", "GroupTrackLimit"),
            ("DVDA_DISC_BYTES", "DiscBytes"),
            ("DVDA_MLP_SOURCE", "MlpSource"),
            ("DVDA_MLP_EXTERNAL_DIR", "MlpExternalDirectory"),
            ("DVDA_LOSS_ERROR_S", "LossErrorSeconds"),
            ("DVDA_LOSS_WARN_S", "LossWarningSeconds"),
        ];
        let extra = [
            ("DVDA_MLP_BATCH_TEMP_DIR", "MlpBatchTempDirectory"),
            ("DVDA_MLP_BATCH_OUTPUT_DIR", "MlpBatchOutputDirectory"),
            ("DVDA_MLP_METADATA_CONTEXT", "MlpMetadataContext"),
            ("DVDA_MLP_SURCODE_SAMPLE_RATE", "MlpSurcodeSampleRate"),
            ("DVDA_MLP_SURCODE_BITS", "MlpSurcodeBits"),
            ("DVDA_MLP_JOBS", "MlpJobs"),
            ("DVDA_MENU_DIR", "MenuDirectory"),
            ("DVDA_MENU_BINDIR", "MenuBinaryDirectory"),
            ("DVDA_MENU", "MenuEnabled"),
            ("DVDA_MENU_TRACKS_PER_PAGE", "MenuTracksPerPage"),
            ("DVDA_MENU_INDEX_MIN_ALBUMS", "MenuIndexMinimumAlbums"),
            ("DVDA_MENU_STILLPICS", "MenuStillPictures"),
            ("DVDA_MENU_COVER_DIM", "MenuCoverDim"),
            ("DVDA_MENU_FONT", "MenuFont"),
            ("DVDA_MENU_FONT_JP", "MenuFontJapanese"),
            ("DVDA_MENU_FONT_KR", "MenuFontKorean"),
        ];
        for (key, property) in
            base.into_iter()
                .chain(extra.into_iter().take(if all { extra.len() } else { 0 }))
        {
            let value = if property.starts_with("Loss") {
                f64::from_bits(options.integer(property) as u64).to_string()
            } else if let Some(value) = options.value(property).as_bool() {
                if value { "on" } else { "off" }.into()
            } else {
                options
                    .value(property)
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| options.value(property).to_string())
            };
            println!(
                "{}",
                dvda_core::dispatch("shell.assignment", json!({"Key":key,"Value":value}))
                    .map_err(failure)?
                    .as_str()
                    .unwrap_or_default()
            );
        }
    } else {
        let evaluated = dvda_core::options::evaluate(dvda_core::options::Request {
            config_path: options
                .config_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            file_values: options.profile_values(),
            environment: std::env::vars()
                .filter(|(k, _)| k.starts_with("DVDA_"))
                .map(|(k, v)| (k, json!(v)))
                .collect(),
            defaults: dvda_core::options::defaults(
                &std::env::var("LOCALAPPDATA").unwrap_or_default(),
            ),
            positive_sign: "+".into(),
            negative_sign: "-".into(),
        });
        print(
            json!({"Profile":options.config_path,"Language":options.language,"Properties":options.values,"StoredValues":options.profile_values(),"Sources":evaluated["Sources"],"EffectiveKeys":evaluated["EffectiveKeys"],"Overrides":evaluated["Overrides"],"MissingRequired":evaluated["MissingRequired"]}),
        )?;
    }
    Ok(0)
}
fn plan(options: &AppOptions) -> Result<i32> {
    let initial = dvda_core::manifest::read(
        &options.path("ManifestPath").to_string_lossy(),
        &options.text("MlpDirectory"),
    )
    .map_err(failure)?;
    let value = dvda_core::disc::limit_albums(
        json!({"Tracks":initial,"AlbumLimit":options.value("DiagnosticAlbumLimit")}),
    )
    .map_err(failure)?;
    let mut tracks: Vec<dvda_core::disc::Track> = serde_json::from_value(value).map_err(failure)?;
    if options.text("MlpSource") == "lpcm" {
        for track in &mut tracks {
            let cache =
                dvda_core::lpcm::cache_path(&options.path("BuildDirectory"), &track.source_path);
            let layout = Path::new(&cache)
                .is_file()
                .then(|| formats::read_wav_layout(&cache))
                .transpose()
                .map_err(failure)?;
            let rate = options.integer("MlpSurcodeSampleRate") as i32;
            let bits = options.integer("MlpSurcodeBits") as i32;
            if let Some(wave) = &layout
                && dvda_core::lpcm::layout_code(
                    wave.sample_rate as i64,
                    wave.valid_bits as i64,
                    wave.channels as i64,
                    wave.channel_mask as i64,
                ) != 0
            {
                return Err(failure("Invalid cached LPCM layout"));
            }
            track.mlp_source = "lpcm".into();
            track.sample_rate = rate;
            track.bits = bits;
            track.mlp_size = if layout
                .as_ref()
                .is_some_and(|w| w.sample_rate == rate && w.valid_bits == bits)
            {
                fs::metadata(&cache).map_err(failure)?.len() as i64
            } else {
                0
            };
            track.channels = layout.as_ref().map(|w| w.channels);
            track.channel_mask = layout.as_ref().map(|w| w.channel_mask);
            track.mlp_path = cache;
        }
    }
    let result = dvda_core::disc::plan(
        tracks,
        options.integer("DiscBytes"),
        options.integer("PlannedDiscs") as i32,
        options.integer("GroupTrackLimit") as i32,
    )
    .map_err(failure)?;
    let code = i32::from(result.has_errors);
    print(result)?;
    Ok(code)
}
fn verification(
    command: &str,
    options: &AppOptions,
    args: &mut Vec<String>,
    events: &mut dyn Callbacks,
) -> Result<i32> {
    let mode = if command == "verify" {
        if args.first().is_some_and(|v| !v.starts_with('-')) {
            args.remove(0)
        } else {
            "all".into()
        }
    } else {
        command.into()
    };
    if mode == "config" {
        return configuration(options, args);
    }
    if matches!(mode.as_str(), "quick" | "quick-check" | "audit") {
        let directory = take(args, "--iso-dir")?;
        let manifest = take(args, "--manifest")?;
        let log = take(args, "--log")?;
        empty(args)?;
        if let Some(path) = directory.as_ref().filter(|path| !Path::new(path).is_dir()) {
            return Err(usage(format!("[错误] ISO 目录不存在: {path}")));
        }
        if let Some(path) = manifest.as_ref().filter(|path| !Path::new(path).is_file()) {
            return Err(usage(format!("[错误] manifest 不存在: {path}")));
        }
        if let Some(path) = log.as_ref().filter(|path| !Path::new(path).is_file()) {
            return Err(usage(format!("[错误] 构建日志不存在: {path}")));
        }
        let result = dvda_core::verify::inspect(
            dvda_core::verify::Inspection {
                final_directory: directory
                    .map(PathBuf::from)
                    .unwrap_or_else(|| options.path("FinalDirectory")),
                iso_prefix: options.text("IsoPrefix"),
                manifest_path: manifest
                    .map(PathBuf::from)
                    .or_else(|| Some(options.path("ManifestPath"))),
                build_log_path: log
                    .clone()
                    .map(PathBuf::from)
                    .or_else(|| Some(options.path("BuildLogPath"))),
                allow_log_fallback: log.is_none(),
                audit: mode == "audit",
            },
            events,
        );
        return print_outcome(result);
    }
    let iso = take(args, "--iso")?.map(PathBuf::from);
    empty(args)?;
    use dvda_core::verify::{Job, Mode};
    let mode = match mode.as_str() {
        "all" => Mode::All,
        "capacity" => Mode::Capacity,
        "timeline" => Mode::Timeline,
        "menu" => Mode::Menu,
        "lossless" => Mode::Lossless,
        _ => return Err(usage(format!("Unsupported verification mode: {mode}"))),
    };
    let job = if matches!(mode, Mode::All | Mode::Lossless)
        || (mode == Mode::Menu && (options.boolean("MenuEnabled") || iso.is_some()))
    {
        options.verify_job().map_err(failure)?
    } else {
        Job {
            image_library: None,
            media_library: PathBuf::new(),
            work_directory: options.path("BuildDirectory"),
            final_directory: options.path("FinalDirectory"),
            iso_prefix: options.text("IsoPrefix"),
            title: Some(options.text("Title")),
            menu_enabled: options.boolean("MenuEnabled"),
            index_path: options.path("MlpIndexPath"),
            manifest_path: Some(options.path("ManifestPath")),
            build_log_path: Some(options.path("BuildLogPath")),
            allow_log_fallback: true,
            disc_bytes: options.integer("DiscBytes"),
            planned_discs: options.integer("PlannedDiscs") as i32,
        }
    };
    print_outcome(dvda_core::verify::execute_mode(job, mode, iso, events))
}
fn print_outcome(result: dvda_core::verify::Outcome) -> Result<i32> {
    let missing_material = |d: &Value| {
        d["Unavailable"] == true
            || matches!(
                d["Code"].as_str(),
                Some(
                    "BUILD_LOG_MISSING"
                        | "TRACK_TABLE_MISSING"
                        | "DVDA_COMMAND_MISSING"
                        | "DISC_LOG_MAPPING_MISSING"
                        | "DISC_TRACK_MAPPING_MISMATCH"
                )
            )
    };
    let unavailable = result.diagnostics.iter().any(missing_material);
    let damaged = result
        .diagnostics
        .iter()
        .any(|d| d["Severity"] == 2 && !missing_material(d));
    let code = if result.succeeded {
        0
    } else if unavailable && !damaged {
        2
    } else {
        1
    };
    print(result)?;
    Ok(code)
}
fn iso(args: &[String]) -> Result<i32> {
    let Some((mode, rest)) = args.split_first() else {
        return Err(usage("iso list|extract ISO ..."));
    };
    match (mode.as_str(), rest) {
        ("list", [path]) | ("list", [path, _]) => {
            let value = dvda_core::dispatch(
                "iso.all_paths",
                json!({"Path":path,"InnerPath":rest.get(1).cloned().unwrap_or_default()}),
            )
            .map_err(failure)?;
            print(value)?;
            Ok(0)
        }
        ("extract", [path, inner, destination]) => {
            let result = dvda_core::dispatch(
                "iso.extract",
                json!({"Path":path,"InnerPath":inner,"Destination":destination}),
            )
            .map_err(failure)?;
            print(&result)?;
            Ok(i32::from(result["Extracted"] != true))
        }
        _ => Err(usage("iso list ISO [INNER] | iso extract ISO INNER OUTPUT")),
    }
}
fn mlp(args: &mut Vec<String>) -> Result<i32> {
    let check = flag(args, "--check")?;
    let align = flag(args, "--align")?;
    let quiet = flag(args, "-q")? | flag(args, "--quiet")?;
    let short = take(args, "-o")?;
    let long = take(args, "--outdir")?;
    if short.is_some() && long.is_some() {
        return Err(usage("Duplicate output directory"));
    }
    let output = short.or(long);
    if check == align
        || args.is_empty()
        || args.iter().any(|a| a.starts_with('-'))
        || (check && output.is_some())
    {
        return Err(usage("mlp (--check|--align) [-o DIR] [-q] FILE..."));
    }
    let native = dvda_native::NativeFormats::load().map_err(failure)?;
    let mut code = 0;
    for path in args {
        let result = (|| -> std::result::Result<(), String> {
            if check {
                let value = native.inspect_file(Path::new(path))?;
                code |= i32::from(!value.is_valid);
                println!("{}", json!({"Path":path,"Inspection":value}));
            } else {
                let input = fs::read(&path).map_err(|e| e.to_string())?;
                let result = native.align(&input)?;
                let target = output
                    .as_ref()
                    .map(|o| Path::new(o).join(Path::new(path).file_name().unwrap()))
                    .unwrap_or_else(|| PathBuf::from(&path));
                if result.data != input {
                    state::write_bytes(&target, &result.data).map_err(|e| e.message)?;
                }
                if !quiet {
                    println!(
                        "{}",
                        json!({"Path":path,"Output":target,"Changed":result.data!=input,"AddedEndOfStream":result.inserted_end_of_stream})
                    );
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("[FAIL] {path}: {error}");
            code = 1;
        }
    }
    Ok(code)
}
fn alac_command(options: &AppOptions, args: &[String], events: &mut dyn Callbacks) -> Result<i32> {
    if args.len() < 2
        || !matches!(args[0].as_str(), "check" | "repair")
        || (args[0] == "check" && args.len() != 2)
        || args.len() > 3
    {
        return Err(usage("alac check INPUT | alac repair INPUT [OUTPUT]"));
    }
    let source = Path::new(&args[1]);
    let library = options.media_library().map_err(failure)?;
    let inspection = alac::inspect(&library, &args[1], events).map_err(|e| failure(e.message))?;
    print(&inspection)?;
    if args[0] == "check" {
        return Ok(i32::from(
            !inspection.patches.is_empty() || !decode_ok(&library, source, events)?,
        ));
    }
    let target = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("{}.fixed.m4a", args[1])));
    if dvda_native::files::ordinal_ignore_case(
        &std::path::absolute(source)
            .map_err(failure)?
            .to_string_lossy(),
        &std::path::absolute(&target)
            .map_err(failure)?
            .to_string_lossy(),
    ) {
        return Err(usage("ALAC repair output must differ from source"));
    }
    let work = std::env::temp_dir().join(format!(
        "dvda-cli-alac-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir(&work).map_err(failure)?;
    let result = (|| {
        let repaired =
            alac::try_repair(&library, &args[1], &work, events).map_err(|e| failure(e.message))?;
        let repaired = repaired
            .as_ref()
            .map_or(source, |r| Path::new(&r.output_path));
        if !decode_ok(&library, repaired, events)? {
            return Err(failure("ALAC repair did not restore declared samples"));
        }
        state::write_bytes(&target, &fs::read(repaired).map_err(failure)?)
            .map_err(|e| failure(e.message))?;
        print(json!({"Output":target,"RepairedFrames":inspection.patches.len()}))?;
        Ok(0)
    })();
    let _ = fs::remove_dir_all(&work);
    result
}
fn decode_ok(library: &Path, path: &Path, events: &mut dyn Callbacks) -> Result<bool> {
    let probe = conversion::probe(library, path, events).map_err(failure)?;
    let stream = probe["streams"]
        .as_array()
        .and_then(|v| v.iter().find(|v| v["codec_type"] == "audio"))
        .ok_or_else(|| failure("Missing audio stream"))?;
    let expected = (|| {
        let duration = stream["duration_ts"].as_i64()?;
        let rate = stream["sample_rate"]
            .as_i64()
            .or_else(|| stream["sample_rate"].as_str()?.parse().ok())?;
        let (a, b) = stream["time_base"].as_str()?.split_once('/')?;
        duration
            .checked_mul(a.parse::<i64>().ok()?)?
            .checked_mul(rate)?
            .checked_div(b.parse::<i64>().ok()?)
    })();
    let decoded = audio::check_decode(
        &audio::Job {
            library: library.to_owned(),
            input: path.to_string_lossy().into_owned(),
            operation: audio::Operation::Decode,
            resample_to: None,
        },
        events,
    )
    .map_err(|e| failure(e.message))?;
    Ok(decoded.exit_code == 0
        && decoded.error_count == 0
        && expected.is_some()
        && decoded.samples == expected)
}
