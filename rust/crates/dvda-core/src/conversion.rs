//! Developer ALAC/M4A → FLAC workflow, using the same in-process media library.
use crate::{formats, media, preparation::alac};
use dvda_native::{
    files,
    media::{Callbacks, Operation, OutputFormat, Request},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

pub struct Job {
    pub library: PathBuf,
    pub paths: Vec<PathBuf>,
    pub compression: u32,
    pub jobs: usize, // Deprecated compatibility field; production ignores explicit worker counts.
    pub dry_run: bool,
    pub delete_sources: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ResultRow {
    pub source_path: String,
    pub destination_path: String,
    pub status: String,
    pub error: Option<String>,
    pub repaired_frames: usize,
    pub pcm_md5: Option<String>,
    pub source_tag_count: usize,
    pub destination_tag_count: usize,
    pub has_cover: bool,
    pub cover_exact: bool,
    pub missing_tags: Vec<(String, String)>,
    pub picture: Option<Value>,
    pub source_delete_error: Option<String>,
}
struct Workspace(PathBuf);
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn normalize_tags(raw: &serde_json::Map<String, Value>) -> Vec<(String, String)> {
    let mut entries: Vec<_> = raw.iter().collect();
    entries.sort_by_key(|(k, _)| k.to_uppercase());
    let mut seen = HashSet::new();
    entries
        .into_iter()
        .filter_map(|(key, value)| {
            let value = value.as_str()?;
            if value.trim().is_empty() {
                return None;
            }
            let lower = key.to_ascii_lowercase();
            let key = match lower.as_str() {
                "major_brand" | "minor_brand" | "minor_version" | "compatible_brands"
                | "creation_time" | "encoder" | "vendor_id" | "handler_name" | "language" => {
                    return None;
                }
                "sort_name" => "TITLESORT".into(),
                "sort_album" => "ALBUMSORT".into(),
                "sort_artist" => "ARTISTSORT".into(),
                "sort_album_artist" => "ALBUMARTISTSORT".into(),
                "sort_composer" => "COMPOSERSORT".into(),
                "track" => "TRACKNUMBER".into(),
                "disc" => "DISCNUMBER".into(),
                "album_artist" => "ALBUMARTIST".into(),
                "upc" => "BARCODE".into(),
                _ => key.to_uppercase(),
            };
            let pair = (key, value.to_owned());
            seen.insert(pair.clone()).then_some(pair)
        })
        .collect()
}

fn collect(paths: &[PathBuf], caller: &mut dyn Callbacks) -> Result<Vec<PathBuf>, String> {
    let mut pending = paths.to_vec();
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    while let Some(path) = pending.pop() {
        if caller.cancelled() {
            return Err("Conversion cancelled".into());
        }
        if path.is_dir() {
            let identity = fs::canonicalize(&path).map_err(|e| e.to_string())?;
            if !seen.insert(identity.to_string_lossy().to_uppercase()) {
                continue;
            }
            for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
                pending.push(entry.map_err(|e| e.to_string())?.path());
            }
        } else if path.is_file()
            && path
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("m4a"))
        {
            let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
            if seen.insert(path.to_string_lossy().to_uppercase()) {
                found.push(path);
            }
        }
    }
    found.sort_by_key(|p| p.to_string_lossy().to_uppercase());
    if found.is_empty() {
        return Err("没有可处理的 .m4a 文件。".into());
    }
    Ok(found)
}

pub fn request(
    input: &Path,
    operation: Operation,
    format: OutputFormat,
    output: Option<&Path>,
) -> Request {
    Request {
        operation,
        rate: 0,
        bits: if matches!(format, OutputFormat::Md5 | OutputFormat::S16) {
            16
        } else {
            0
        },
        output_format: format,
        soxr: false,
        compression: 8,
        cover: false,
        input: input.to_string_lossy().into_owned(),
        output: output.map(|p| p.to_string_lossy().into_owned()),
        tags: vec![],
    }
}
struct Capture<'a> {
    caller: &'a mut dyn Callbacks,
    stdout: String,
    stderr: String,
}
impl Callbacks for Capture<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        let buffer = if stream == 1 {
            &mut self.stdout
        } else {
            &mut self.stderr
        };
        buffer.push_str(text);
        buffer.push('\n');
    }
    fn cancelled(&mut self) -> bool {
        self.caller.cancelled()
    }

    fn progress(&mut self, completed: u64, total: u64) {
        self.caller.progress(completed, total);
    }
}
pub fn run_media(
    library: &Path,
    request: Request,
    caller: &mut dyn Callbacks,
) -> Result<String, String> {
    let mut capture = Capture {
        caller,
        stdout: String::new(),
        stderr: String::new(),
    };
    let outcome = media::execute(
        media::Job {
            library: library.to_owned(),
            request,
            replace: false,
            timeout_millis: None,
        },
        &mut capture,
    );
    if let Some(failure) = outcome.failure {
        return Err(failure.message);
    }
    if outcome.exit_code != Some(0) {
        return Err(format!(
            "Media conversion failed: {}",
            capture.stderr.trim()
        ));
    }
    Ok(capture.stdout)
}
pub fn probe(library: &Path, input: &Path, caller: &mut dyn Callbacks) -> Result<Value, String> {
    serde_json::from_str(&run_media(
        library,
        request(input, Operation::Probe, OutputFormat::None, None),
        caller,
    )?)
    .map_err(|e| e.to_string())
}
fn md5(library: &Path, input: &Path, caller: &mut dyn Callbacks) -> Result<String, String> {
    let text = run_media(
        library,
        request(input, Operation::Audio, OutputFormat::Md5, None),
        caller,
    )?;
    text.lines()
        .find_map(|line| line.trim().strip_prefix("MD5="))
        .filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|s| s.to_ascii_lowercase())
        .ok_or("Missing PCM MD5".into())
}
fn one(job: &Job, input: &Path, caller: &mut dyn Callbacks) -> Result<ResultRow, String> {
    let destination = input.with_extension("flac");
    let info = probe(&job.library, input, caller)?;
    let streams = info["streams"].as_array().ok_or("Missing media streams")?;
    let stream = streams
        .iter()
        .find(|v| v["codec_type"] == "audio")
        .ok_or("读不到首音频流编码")?;
    if stream["codec_name"] != "alac" {
        return Err(format!("首音频流不是 ALAC: {}", stream["codec_name"]));
    }
    let raw = info["format"]["tags"]
        .as_object()
        .filter(|v| !v.is_empty())
        .ok_or("读不到标签")?;
    let mut row = ResultRow {
        source_path: input.to_string_lossy().into_owned(),
        destination_path: destination.to_string_lossy().into_owned(),
        status: if job.dry_run { "DRY" } else { "OK" }.into(),
        error: None,
        repaired_frames: 0,
        pcm_md5: None,
        source_tag_count: raw.len(),
        destination_tag_count: 0,
        has_cover: false,
        cover_exact: false,
        missing_tags: vec![],
        picture: None,
        source_delete_error: None,
    };
    if job.dry_run {
        row.repaired_frames = alac::inspect(&job.library, &input.to_string_lossy(), caller)
            .map_err(|e| e.message)?
            .patches
            .len();
        return Ok(row);
    }
    let work = Workspace(media::temporary_path(&destination));
    fs::create_dir(&work.0).map_err(|e| e.to_string())?;
    let repaired = alac::try_repair(&job.library, &input.to_string_lossy(), &work.0, caller)
        .map_err(|e| e.message)?;
    let source = repaired
        .as_ref()
        .map_or(input, |r| Path::new(&r.output_path));
    row.repaired_frames = repaired.as_ref().map_or(0, |r| r.patches.len());
    let tags = normalize_tags(raw);
    row.source_tag_count = tags.len();
    let cover = streams.iter().find(|v| v["codec_type"] == "video");
    row.has_cover = cover.is_some();
    let target = work.0.join("converted.flac");
    let mut request = request(source, Operation::Audio, OutputFormat::Flac, Some(&target));
    request.compression = job.compression;
    request.tags = tags.clone();
    request.cover = row.has_cover;
    run_media(&job.library, request, caller)?;
    let reference = md5(&job.library, source, caller)?;
    let result = md5(&job.library, &target, caller)?;
    if reference != result {
        return Err("PCM MD5 不一致".into());
    }
    // Also compare all valid PCM bits; the historic MD5 output uses s16.
    let bits = stream["bits_per_raw_sample"]
        .as_i64()
        .or_else(|| stream["bits_per_raw_sample"].as_str()?.parse().ok())
        .unwrap_or(24);
    let raw_paths = [work.0.join("source.pcm"), work.0.join("converted.pcm")];
    for (source, raw) in [source, target.as_path()].into_iter().zip(&raw_paths) {
        let mut request = self::request(
            source,
            Operation::Audio,
            if bits <= 16 {
                OutputFormat::S16
            } else {
                OutputFormat::S24
            },
            Some(raw),
        );
        request.bits = if bits <= 16 { 16 } else { 24 };
        run_media(&job.library, request, caller)?;
    }
    let native = dvda_native::NativeFormats::load()?;
    if !native
        .compare_pcm(&raw_paths[0], &raw_paths[1], 1, 0)?
        .matches
    {
        return Err("Converted PCM differs at full precision".into());
    }
    row.pcm_md5 = Some(result);
    let comments = formats::dispatch("flac.comments", json!(target))?;
    let comments = comments.as_array().ok_or("Invalid FLAC comments")?;
    row.destination_tag_count = comments.len();
    row.missing_tags = tags
        .into_iter()
        .filter(|(k, v)| !comments.iter().any(|t| t["Key"] == *k && t["Value"] == *v))
        .collect();
    if let Some(cover) = cover {
        let mime = match cover["codec_name"].as_str().unwrap_or_default() {
            "png" => "image/png",
            "bmp" => "image/bmp",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "tiff" => "image/tiff",
            _ => "image/jpeg",
        };
        let source_cover = work.0.join("cover.bin");
        run_media(
            &job.library,
            self::request(
                source,
                Operation::Cover,
                OutputFormat::None,
                Some(&source_cover),
            ),
            caller,
        )?;
        let mut picture = formats::dispatch("flac.picture", json!(target))?;
        if picture.is_null() {
            return Err("FLAC PICTURE block is missing".into());
        }
        let bytes = fs::read(&source_cover).map_err(|e| e.to_string())?;
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        if !picture["ImageHex"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(&hex))
        {
            return Err("封面字节内容不一致".into());
        }
        picture["Descriptor"]["Type"] = json!(3);
        picture["Description"] = json!("");
        formats::dispatch(
            "flac.replace_picture",
            json!({"Path":target,"ImagePath":source_cover,"Template":picture}),
        )?;
        let descriptor = &picture["Descriptor"];
        if descriptor["MimeType"] != mime
            || ["Width", "Height", "Depth"]
                .iter()
                .any(|k| descriptor[*k].as_i64().unwrap_or(0) <= 0)
            || (mime == "image/jpeg" && descriptor["Depth"] != 24)
        {
            return Err("FLAC PICTURE 描述不合规".into());
        }
        row.cover_exact = true;
        row.picture = Some(descriptor.clone());
    }
    if caller.cancelled() {
        return Err("Conversion cancelled".into());
    }
    files::move_file(&target, &destination, true).map_err(|e| e.to_string())?;
    Ok(row)
}

fn failed(path: &Path, error: String) -> ResultRow {
    ResultRow {
        source_path: path.to_string_lossy().into_owned(),
        destination_path: path.with_extension("flac").to_string_lossy().into_owned(),
        status: "FAIL".into(),
        error: Some(error),
        repaired_frames: 0,
        pcm_md5: None,
        source_tag_count: 0,
        destination_tag_count: 0,
        has_cover: false,
        cover_exact: false,
        missing_tags: vec![],
        picture: None,
        source_delete_error: None,
    }
}
struct Worker<'a>(&'a AtomicBool);
impl Callbacks for Worker<'_> {
    fn emit(&mut self, _: i32, _: &str) {}
    fn cancelled(&mut self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
pub fn execute(job: &Job, caller: &mut dyn Callbacks) -> Result<Vec<ResultRow>, String> {
    if job.compression > 8 {
        return Err("FLAC level must be 0–8".into());
    }
    let paths = collect(&job.paths, caller)?;
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let count = crate::options::worker_count(paths.len());
    let (send, receive) = mpsc::sync_channel(count);
    let mut rows = Vec::with_capacity(paths.len());
    let observed = std::thread::scope(|scope| {
        for _ in 0..count {
            let (paths, next, stop, send) = (&paths, &next, &stop, send.clone());
            scope.spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    let Some(path) = paths.get(next.fetch_add(1, Ordering::Relaxed)) else {
                        break;
                    };
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        one(job, path, &mut Worker(stop))
                    }))
                    .unwrap_or_else(|_| Err("Conversion worker panic".into()))
                    .unwrap_or_else(|error| failed(path, error));
                    if send.send(result).is_err() {
                        break;
                    }
                }
            });
        }
        drop(send);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loop {
                if caller.cancelled() {
                    stop.store(true, Ordering::Release);
                }
                match receive.recv_timeout(Duration::from_millis(20)) {
                    Ok(row) => {
                        caller.emit(
                            if row.status == "FAIL" { 2 } else { 1 },
                            &format!("[{}] {}", row.status, row.source_path),
                        );
                        rows.push(row);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => (),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        }));
        if result.is_err() {
            stop.store(true, Ordering::Release);
            while receive.recv().is_ok() {}
        }
        result
    });
    if observed.is_err() {
        return Err("Conversion callback panic".into());
    }
    if stop.load(Ordering::Acquire) {
        return Err("Conversion cancelled".into());
    }
    rows.sort_by_key(|r| r.source_path.to_uppercase());
    if job.delete_sources && !job.dry_run && rows.iter().all(|r| r.status == "OK") {
        delete_sources(&mut rows, |path| fs::remove_file(path));
    }
    Ok(rows)
}
fn delete_sources(rows: &mut [ResultRow], mut delete: impl FnMut(&str) -> std::io::Result<()>) {
    for row in rows {
        if let Err(error) = delete(&row.source_path) {
            row.source_delete_error = Some(error.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tags_preserve_values_and_delete_failures_do_not_stop_other_sources() {
        let raw = json!({"title":"  A\nB  ","track":"02/10","sort_name":" x ","encoder":"drop","blank":"  ","upc":"123"});
        let tags = normalize_tags(raw.as_object().unwrap());
        assert!(tags.contains(&("TITLE".into(), "  A\nB  ".into())));
        assert!(tags.contains(&("TRACKNUMBER".into(), "02/10".into())));
        assert!(tags.contains(&("TITLESORT".into(), " x ".into())));
        assert_eq!(tags.len(), 4);
        let mut rows = vec![
            failed(Path::new("one.m4a"), "".into()),
            failed(Path::new("two.m4a"), "".into()),
        ];
        let mut calls = 0;
        delete_sources(&mut rows, |_| {
            calls += 1;
            if calls == 1 {
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            } else {
                Ok(())
            }
        });
        assert_eq!(calls, 2);
        assert!(rows[0].source_delete_error.is_some());
        assert!(rows[1].source_delete_error.is_none());
    }
}
