//! CLI compatibility and complete disc assembly. Audio, tables and filesystem
//! generation stay in Rust; image/video codecs are supplied by the menu backend.
use crate::{Callbacks, amg, aob, asvs, atsi, iso, menu, samg};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug)]
pub struct Request {
    pub groups: Vec<Vec<aob::TrackInput>>,
    pub output: PathBuf,
    pub temporary: PathBuf,
    pub iso: Option<PathBuf>,
    pub volume: Option<String>,
    pub menu: menu::Options,
    pub manager: amg::Options,
    pub downmix: Option<[[f32; 12]; 16]>,
}

const HELP: &str = "Rust DVD-Audio author\nUsage: dvda-author-dev -g track.mlp track.wav [-z track ...] [-g ...] -o disc\n  -D temporary  --iso=image.iso  --iso-volume LABEL\n  --topmenu --background files --screentext text --stillpics files\n  --provider TEXT --autoplay --playlist groups --downmix coefficients\nMLP and integer PCM WAV inputs; 1..=9 groups, 1..=99 tracks per group.";

pub fn parse(arguments: &[String]) -> Result<Request, String> {
    let mut request = Request {
        groups: Vec::new(),
        output: PathBuf::new(),
        temporary: PathBuf::new(),
        iso: None,
        volume: None,
        menu: menu::Options::default(),
        manager: amg::Options::default(),
        downmix: None,
    };
    let mut index = 0;
    let mut boundary = true;
    let mut downmix_tables = Vec::new();
    while index < arguments.len() {
        let argument = &arguments[index];
        let (name, attached) = argument
            .split_once('=')
            .map_or((argument.as_str(), None), |(a, b)| (a, Some(b)));
        let flag = matches!(
            name,
            "-g" | "-z"
                | "-W"
                | "-P0"
                | "-n"
                | "-v"
                | "--verbose"
                | "--topmenu"
                | "--autoplay"
                | "-a"
                | "--loop"
        );
        let mut value = || -> Result<&str, String> {
            if let Some(value) = attached {
                return Ok(value);
            }
            index += 1;
            arguments
                .get(index)
                .map(String::as_str)
                .ok_or_else(|| format!("Missing value for {name}"))
        };
        match name {
            "-g" => {
                request.groups.push(Vec::new());
                boundary = true;
            }
            "-z" => boundary = true,
            "-o" | "--output" => request.output = PathBuf::from(value()?),
            "-D" | "--tempdir" => request.temporary = PathBuf::from(value()?),
            "--iso" => request.iso = Some(PathBuf::from(value()?)),
            "--iso-volume" => request.volume = Some(value()?.to_owned()),
            "--provider" => request.manager.provider = value()?.to_owned(),
            "--autoplay" | "-a" => request.manager.autoplay = true,
            "--playlist" => {
                request.manager.playlist_groups = value()?
                    .split(',')
                    .map(|s| {
                        s.parse::<usize>()
                            .ok()
                            .and_then(|n| n.checked_sub(1))
                            .ok_or("Playlist groups must be positive integers".to_owned())
                    })
                    .collect::<Result<_, _>>()?
            }
            "--downmix" => {
                let values = value()?
                    .split(',')
                    .map(|s| {
                        s.parse::<f32>()
                            .map_err(|_| "Invalid downmix coefficient".to_owned())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let table: [f32; 12] = values
                    .try_into()
                    .map_err(|_| "A downmix table needs 12 coefficients")?;
                if downmix_tables.len() == 16 {
                    return Err("At most 16 downmix tables are supported".into());
                }
                downmix_tables.push(table);
            }
            "--dtable" => {
                let rank = value()?
                    .parse::<u8>()
                    .map_err(|_| "Invalid downmix table rank")?;
                if !(1..=16).contains(&rank) {
                    return Err("Downmix table rank must be in 1..=16".into());
                }
                for track in request.groups.iter_mut().flatten() {
                    track.downmix_rank = rank;
                }
            }
            "--cga" | "-c" => {
                let assignments = value()?
                    .split(',')
                    .map(|s| {
                        s.parse::<u8>()
                            .map_err(|_| "Invalid channel assignment".to_owned())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let group = request
                    .groups
                    .last_mut()
                    .ok_or("Channel assignments require a preceding group")?;
                if assignments.len() != 1 && assignments.len() != group.len() {
                    return Err(
                        "Channel assignments must specify one value or one per group track".into(),
                    );
                }
                for (i, track) in group.iter_mut().enumerate() {
                    track.cga = Some(assignments[if assignments.len() == 1 { 0 } else { i }]);
                }
            }
            "-W" | "-P0" | "-n" | "-v" | "--verbose" => {}
            _ if name.starts_with('-') => {
                let option_value = if flag { attached } else { Some(value()?) };
                if !request.menu.parse_option(name, option_value)? {
                    return Err(format!("Unsupported author option: {name}"));
                }
            }
            _ => {
                let group = request.groups.last_mut().ok_or("Audio input requires -g")?;
                group.push(aob::TrackInput {
                    path: argument.into(),
                    new_title: boundary,
                    cga: None,
                    downmix_rank: 0,
                });
                boundary = false;
            }
        }
        index += 1;
    }
    if request.output.as_os_str().is_empty() {
        return Err("Output directory (-o) is required".into());
    }
    if request.groups.is_empty()
        || request.groups.len() > 9
        || request.groups.iter().any(|g| g.is_empty() || g.len() > 99)
    {
        return Err("Author needs 1..=9 groups with 1..=99 tracks each".into());
    }
    if request.temporary.as_os_str().is_empty() {
        request.temporary = request.output.with_extension("author-temp");
    }
    if !downmix_tables.is_empty() {
        request.downmix = Some(std::array::from_fn(|i| {
            downmix_tables[i % downmix_tables.len()]
        }));
    }
    Ok(request)
}

pub fn run(
    arguments: &[String],
    backend: &mut dyn menu::Backend,
    callbacks: &mut dyn Callbacks,
) -> Result<(), String> {
    if arguments
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        callbacks.emit(1, HELP);
        return Ok(());
    }
    if arguments.iter().any(|arg| arg == "--version") {
        callbacks.emit(1, concat!("dvda-author Rust ", env!("CARGO_PKG_VERSION")));
        return Ok(());
    }
    author(&parse(arguments)?, backend, callbacks)
}

fn io<T>(result: std::io::Result<T>) -> Result<T, String> {
    result.map_err(|e| e.to_string())
}
fn check(callbacks: &mut dyn Callbacks) -> Result<(), String> {
    if callbacks.cancelled() {
        Err("Author cancelled".into())
    } else {
        Ok(())
    }
}

fn rename_directory(
    source: &Path,
    destination: &Path,
    callbacks: &mut dyn Callbacks,
) -> Result<(), String> {
    let start = std::time::Instant::now();
    loop {
        check(callbacks)?;
        match fs::rename(source, destination) {
            Ok(()) => return Ok(()),
            // Windows scanners can briefly open a completed staging directory
            // without FILE_SHARE_DELETE. Retry the atomic rename, preserving
            // cancellation and the original destination on failure.
            Err(error)
                if cfg!(windows)
                    && matches!(error.raw_os_error(), Some(5 | 32 | 33))
                    && start.elapsed() < std::time::Duration::from_secs(1) =>
            {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(error) => {
                return Err(format!(
                    "Cannot publish directory {} to {}: {error}",
                    source.display(),
                    destination.display()
                ));
            }
        }
    }
}
fn pair(directory: &Path, stem: &str, bytes: &[u8]) -> Result<(), String> {
    io(fs::write(directory.join(format!("{stem}.IFO")), bytes))?;
    io(fs::write(directory.join(format!("{stem}.BUP")), bytes))
}
struct Staging(PathBuf, bool);
impl Drop for Staging {
    fn drop(&mut self) {
        if self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
struct StagedIso(PathBuf);
impl Drop for StagedIso {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn resolve(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        io(std::env::current_dir())?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            part => normalized.push(part.as_os_str()),
        }
    }
    let mut missing = Vec::new();
    while !normalized.exists() {
        missing.push(
            normalized
                .file_name()
                .ok_or("Cannot resolve author path")?
                .to_owned(),
        );
        if !normalized.pop() {
            return Err("Cannot resolve author path".into());
        }
    }
    let mut result = io(normalized.canonicalize())?;
    for part in missing.into_iter().rev() {
        result.push(part);
    }
    Ok(result)
}
fn within(path: &Path, directory: &Path) -> bool {
    #[cfg(windows)]
    {
        let key = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
        let path = key(path);
        let directory = key(directory);
        path == directory || path.starts_with(&(directory + "/"))
    }
    #[cfg(not(windows))]
    {
        path.starts_with(directory)
    }
}

pub fn author(
    request: &Request,
    backend: &mut dyn menu::Backend,
    callbacks: &mut dyn Callbacks,
) -> Result<(), String> {
    check(callbacks)?;
    if request.output.file_name().is_none() {
        return Err("Author output needs a directory name".into());
    }
    let output = resolve(&request.output)?;
    let temporary = resolve(&request.temporary)?;
    if within(&temporary, &output) {
        return Err("Author temporary directory must be outside the output tree".into());
    }
    let iso_destination = request.iso.as_deref().map(resolve).transpose()?;
    if let Some(destination) = &iso_destination {
        if destination.is_dir() || within(destination, &output) {
            return Err("ISO destination must be a file outside the output tree".into());
        }
        for track in request.groups.iter().flatten() {
            if resolve(&track.path)? == *destination {
                return Err("ISO destination would overwrite an audio input".into());
            }
        }
    }
    if request.output.exists()
        && (!request.output.is_dir() || io(fs::read_dir(&request.output))?.next().is_some())
    {
        return Err("Author output directory must be empty".into());
    }
    let parent = request
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    io(fs::create_dir_all(parent))?;
    let parent = io(parent.canonicalize())?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut stage = loop {
        let directory = parent.join(format!(
            ".dvda-author-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&directory) {
            Ok(()) => break Staging(directory, true),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    let audio_ts = stage.0.join("AUDIO_TS");
    io(fs::create_dir(&audio_ts))?;
    io(fs::create_dir(stage.0.join("VIDEO_TS")))?;
    io(fs::create_dir_all(&request.temporary))?;
    let mut audio_groups = Vec::new();
    let mut title_starts = Vec::new();
    for (index, group) in request.groups.iter().enumerate() {
        check(callbacks)?;
        callbacks.emit(1, &format!("[INF] Creating ATS_{:02} audio", index + 1));
        let output = aob::author_group(&audio_ts, (index + 1) as u8, group, callbacks)?;
        audio_groups.push(output.tracks);
        title_starts.push(output.title_starts);
    }
    let menu_groups: Vec<_> = request
        .groups
        .iter()
        .map(|g| {
            g.iter()
                .map(|t| menu::Track {
                    name: t
                        .path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                })
                .collect()
        })
        .collect();
    let menu_output = menu::author(
        &audio_ts,
        &request.temporary,
        &menu_groups,
        &request.menu,
        backend,
        callbacks,
    )?;
    if menu_output
        .track_pictures
        .iter()
        .flatten()
        .filter(|p| !p.is_empty())
        .count()
        > 99
    {
        // ASVS has a fixed picture-title directory ending at 0x378. Protect it
        // before ATSI's byte-sized picture-title counters can overflow.
        let mut picture_titles = 0;
        for (index, pictures) in menu_output.track_pictures.iter().enumerate() {
            let mut any = false;
            for (track, p) in pictures.iter().enumerate() {
                if title_starts[index].contains(&track) {
                    picture_titles += usize::from(any);
                    any = false;
                }
                any |= !p.is_empty();
            }
            picture_titles += usize::from(any);
        }
        if picture_titles > 99 {
            return Err("ASVS fixed directory supports at most 99 picture titles".into());
        }
    }
    let mut groups = Vec::new();
    let mut picture_title_rank = 0;
    for (index, audio) in audio_groups.iter().enumerate() {
        let mut titles = Vec::<atsi::Title>::new();
        for (track_index, &track) in audio.iter().enumerate() {
            if title_starts[index].contains(&track_index) {
                titles.push(atsi::Title { tracks: Vec::new() });
            }
            titles
                .last_mut()
                .ok_or("AOB did not report its initial title boundary")?
                .tracks
                .push(atsi::Track {
                    audio: track,
                    downmix_table_rank: request.groups[index][track_index].downmix_rank,
                    pictures: menu_output.track_pictures[index][track_index].clone(),
                });
        }
        let encoded = atsi::encode_checked(
            &titles,
            &atsi::Options {
                downmix: request.downmix,
                still_picture_titles: menu_output.still.as_ref().map(|_| picture_title_rank),
            },
        )?;
        picture_title_rank = encoded.still_picture_titles;
        pair(
            &audio_ts,
            &format!("ATS_{:02}_0", index + 1),
            &encoded.bytes,
        )?;
        groups.push(amg::Group {
            titles,
            atsi_sectors: encoded.sectors,
        });
    }
    let mut layout = amg::Layout::default();
    if let Some(still) = &menu_output.still {
        let encoded = asvs::encode(
            &groups.iter().map(|g| g.titles.clone()).collect::<Vec<_>>(),
            &still.picture_sectors,
            &asvs::Options {
                video_attribute: still.video_attribute,
                palette: still.palette,
                sectors: None,
            },
        )?;
        if encoded.vob_sectors != still.still_vob_sectors {
            return Err("ASVS picture sizes do not match the still VOB".into());
        }
        pair(&audio_ts, "AUDIO_SV", &encoded.bytes)?;
        layout.asvs_sectors = encoded.sectors;
        layout.still_vob_sectors = encoded.vob_sectors;
    }
    let mut options = request.manager.clone();
    if let Some(top) = menu_output.top_menu {
        options.menu = Some(amg::Menu {
            page_sectors: top.page_sectors,
            video_attribute: top.video_attribute,
            duration: top.duration,
            looped: top.looped,
            palette: top.palette,
        });
        layout.top_vob_sectors = top.top_vob_sectors;
    }
    callbacks.emit(1, "[INF] Creating AUDIO_TS.IFO and AUDIO_PP.IFO");
    let manager = amg::encode(&groups, &layout, &options)?;
    pair(&audio_ts, "AUDIO_TS", &manager.bytes)?;
    // Reserve the complete SAMG before planning the filesystem: adding a file
    // afterward would change all physical addresses.
    let track_count: usize = audio_groups.iter().map(Vec::len).sum();
    let samg_sectors = u8::try_from(((16 + 52 * track_count).div_ceil(2048) * 8).max(64))
        .map_err(|_| "SAMG exceeds 255 sectors")?;
    io(fs::write(
        audio_ts.join("AUDIO_PP.IFO"),
        vec![0u8; usize::from(samg_sectors) * 2048],
    ))?;
    let files = io(iso::file_layout(&stage.0))?;
    let start_sector = files
        .iter()
        .find(|(p, _, _)| p == Path::new("AUDIO_TS/AUDIO_PP.IFO"))
        .ok_or("SAMG missing from ISO layout")?
        .1;
    let samg_layout = samg::Layout {
        start_sector,
        samg_sectors,
        amg_sectors: manager.sectors,
        asvs_sectors: layout.asvs_sectors,
        atsi_sectors: groups.iter().map(|g| g.atsi_sectors).collect(),
        still_vob_sectors: layout.still_vob_sectors,
        top_vob_sectors: layout.top_vob_sectors,
        video_link_tracks: options.video_links.len() as u16,
    };
    let (simple, _) = samg::encode(&audio_groups, &samg_layout)?;
    for (index, tracks) in audio_groups.iter().enumerate() {
        let path = PathBuf::from(format!("AUDIO_TS/ATS_{:02}_1.AOB", index + 1));
        let lba = files
            .iter()
            .find(|(p, _, _)| p == &path)
            .ok_or("AOB missing from ISO layout")?
            .1;
        let flat = audio_groups[..index].iter().map(Vec::len).sum::<usize>();
        if u32::from_be_bytes(
            simple[16 + 52 * flat + 40..16 + 52 * flat + 44]
                .try_into()
                .unwrap(),
        ) != lba + tracks[0].first_sector
        {
            return Err("SAMG AOB sector address does not match the ISO writer".into());
        }
    }
    io(fs::write(audio_ts.join("AUDIO_PP.IFO"), simple))?;
    callbacks.emit(
        1,
        "Group Title Track First_Sect Last_Sect First_PTS PTS_length cga",
    );
    for (group, manager) in groups.iter().enumerate() {
        let mut group_track = 0;
        for (title, tracks) in manager.titles.iter().enumerate() {
            for entry in &tracks.tracks {
                group_track += 1;
                let audio = entry.audio;
                callbacks.emit(
                    1,
                    &format!(
                        "{} {:02}/{:02} {} {} {} {} {} {}",
                        group + 1,
                        title + 1,
                        manager.titles.len(),
                        group_track,
                        audio.first_sector,
                        audio.last_sector,
                        audio.first_pts,
                        audio.pts_length,
                        audio.channel_assignment
                    ),
                );
            }
        }
    }
    let staged_iso = if let Some(destination) = &iso_destination {
        let parent = destination
            .parent()
            .ok_or("ISO destination needs a parent directory")?;
        io(fs::create_dir_all(parent))?;
        let path = loop {
            let path = parent.join(format!(
                ".dvda-author-iso-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => break StagedIso(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        };
        callbacks.emit(1, "[INF] Writing ISO image (Rust writer)");
        io(iso::write_with_callbacks(
            &stage.0,
            &path.0,
            request.volume.as_deref(),
            callbacks,
        ))?;
        Some(path)
    } else {
        None
    };
    check(callbacks)?;
    let restore_empty = output.exists();
    if restore_empty {
        io(fs::remove_dir(&output))?;
    }
    if let Err(error) = rename_directory(&stage.0, &output, callbacks) {
        if restore_empty {
            let _ = fs::create_dir(&output);
        }
        return Err(error);
    }
    if let (Some(staged), Some(destination)) = (&staged_iso, &iso_destination)
        && let Err(error) = fs::rename(&staged.0, destination)
    {
        match rename_directory(&output, &stage.0, &mut crate::Silent) {
            Ok(()) => {
                if restore_empty {
                    io(fs::create_dir(&output))?;
                }
            }
            Err(rollback) => {
                return Err(format!(
                    "ISO commit failed: {error}; disc directory rollback failed: {rollback}"
                ));
            }
        }
        return Err(error.to_string());
    }
    stage.1 = false;
    callbacks.emit(1, "[INF] Rust DVD-Audio author complete");
    Ok(())
}

#[cfg(all(test, windows))]
mod publication_tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "dvda-publish-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::create_dir(path.join("stage")).unwrap();
            Self(path)
        }
        fn lock(&self) -> fs::File {
            OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(0x0200_0000) // FILE_FLAG_BACKUP_SEMANTICS
                .open(self.0.join("stage"))
                .unwrap()
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn publication_recovers_after_windows_directory_lock_is_released() {
        let work = Directory::new();
        let lock = work.lock();
        fs::write(work.0.join("stage/payload"), b"complete disc").unwrap();
        assert!(fs::rename(work.0.join("stage"), work.0.join("disc")).is_err());
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(80));
            drop(lock);
        });
        rename_directory(
            &work.0.join("stage"),
            &work.0.join("disc"),
            &mut crate::Silent,
        )
        .unwrap();
        release.join().unwrap();
        assert_eq!(
            fs::read(work.0.join("disc/payload")).unwrap(),
            b"complete disc"
        );
        assert!(!work.0.join("stage").exists());
    }

    #[test]
    fn cancellation_during_windows_directory_lock_keeps_staging_unpublished() {
        struct Cancel(usize);
        impl Callbacks for Cancel {
            fn emit(&mut self, _: i32, _: &str) {}
            fn cancelled(&mut self) -> bool {
                self.0 += 1;
                self.0 > 1
            }
        }
        let work = Directory::new();
        let lock = work.lock();
        let error = rename_directory(&work.0.join("stage"), &work.0.join("disc"), &mut Cancel(0))
            .unwrap_err();
        assert!(error.contains("cancelled"), "{error}");
        assert!(work.0.join("stage").is_dir());
        assert!(!work.0.join("disc").exists());
        drop(lock);
    }
}
