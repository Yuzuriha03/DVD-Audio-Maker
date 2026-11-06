use super::{
    alac,
    models::{self, ManifestGroup, ManifestTrack},
    rules, state,
};
use crate::{audio, media::Failure};
use dvda_native::media::Callbacks;
use serde::{Deserialize, Serialize};
use serde_json::{Map, json};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    #[serde(default = "report_language")]
    pub language: String,
    pub library: PathBuf,
    pub source_directory: String,
    pub manifest_path: PathBuf,
    pub report_path: PathBuf,
    pub prepare_cache_path: PathBuf,
    pub prepare_snapshot_path: PathBuf,
    pub alac_fix_directory: PathBuf,
    pub mlp_source: String,
    pub prepare_cache_enabled: bool,
    pub loss_error_seconds: f64,
    pub loss_warning_seconds: f64,
    pub force_revalidation: bool,
    #[serde(default)]
    pub production: Option<Production>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Production {
    pub rate: i32,
    pub bits: i32,
    pub dither: bool,
    pub directory: PathBuf,
    pub signature: String,
    pub stream_mlp: bool,
}
fn report_language() -> String {
    "zh-CN".into()
}
impl Job {
    /// # Errors
    /// Returns `InvalidData` if the media-library encoding identity cannot be
    /// resolved for the preparation fingerprint.
    pub fn fingerprint(&self) -> Result<String, Failure> {
        let values = [
            state::normalize(&self.source_directory),
            crate::signature::encoding_identity("prepare-v1", &self.library, None, "")
                .map_err(|e| Failure::new("InvalidData", &e))?,
            state::normalize(&self.alac_fix_directory.to_string_lossy()),
            self.mlp_source.clone(),
            if self.prepare_cache_enabled {
                "cache:on".into()
            } else {
                "cache:off".into()
            },
            super::snapshot::roundtrip(self.loss_error_seconds),
            super::snapshot::roundtrip(self.loss_warning_seconds),
        ];
        Ok(crate::hash::hex_digest(values.join("\n").as_bytes()).to_uppercase())
    }
    fn audio(&self, path: &str, resample_to: Option<i32>) -> audio::Job {
        audio::Job {
            library: self.library.clone(),
            input: path.into(),
            operation: audio::Operation::Metadata,
            resample_to,
            decode_output: None,
        }
    }
    fn decode_audio(&self, path: &str, resample_to: Option<i32>) -> audio::Job {
        let mut job = self.audio(path, resample_to);
        if let Some(production) = &self.production {
            let output = crate::media::temporary_path(&production.directory.join("decoded.wav"));
            let mlp_output = production.stream_mlp.then(|| output.with_extension("mlp"));
            job.decode_output = Some(audio::DecodeOutput {
                normalized: output.with_extension("normalized.wav"),
                output,
                rate: production.rate,
                bits: production.bits,
                dither: production.dither,
                mlp_output,
            });
        }
        job
    }
}
struct Quiet<'a>(&'a mut dyn Callbacks);
impl Callbacks for Quiet<'_> {
    fn emit(&mut self, _: i32, _: &str) {}
    fn cancelled(&mut self) -> bool {
        self.0.cancelled()
    }

    fn progress(&mut self, completed: u64, total: u64) {
        self.0.progress(completed, total);
    }
}
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Outcome {
    pub exit_code: Option<i32>,
    pub failure: Option<Failure>,
    pub data: Option<models::Result>,
}
impl Outcome {
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.exit_code == Some(0)
            && self.failure.is_none()
            && self
                .data
                .as_ref()
                .is_some_and(|data| data.failure_count == 0)
    }
}

/// The build entrypoint must never consume a stale manifest after preparation
/// fails. Reuse only a snapshot whose sources, settings and manifest still match.
///
/// # Errors
/// Returns an error on cancellation, fingerprint/snapshot or reused-manifest
/// read failure, any failure propagated by [`run`], or preparation results
/// containing failed source checks.
pub fn ensure_ready(job: &Job, events: &mut dyn Callbacks) -> Result<(), Failure> {
    alac::check_cancel(events)?;
    if !job.force_revalidation {
        let (snapshot, reason) = state::reuse_snapshot(
            &job.prepare_snapshot_path,
            &job.source_directory,
            &job.fingerprint()?,
            &job.manifest_path.to_string_lossy(),
        )?;
        if snapshot.is_some() && repaired_targets_valid(job, events)? {
            events.emit(1, &reason);
            alac::check_cancel(events)?;
            return Ok(());
        }
    }
    let prepared = run(job, events)?;
    // Channel validation issues are not all emitted by the per-track decoder.
    for issue in &prepared.issues {
        events.emit(
            if issue.level == "FAIL" { 2 } else { 1 },
            &format!("[{}] {}: {}", issue.level, issue.title, issue.reason),
        );
    }
    if prepared.failure_count > 0 {
        return Err(Failure::new(
            "InvalidData",
            "音源检查失败，请修复问题后再制作。",
        ));
    }
    alac::check_cancel(events)?;
    Ok(())
}

fn repaired_targets_valid(job: &Job, events: &mut dyn Callbacks) -> Result<bool, Failure> {
    let manifest = state::read_json(&job.manifest_path)?;
    let mut repaired = Vec::new();
    for group in manifest.as_object().into_iter().flat_map(|o| o.values()) {
        for track in group["files"].as_array().into_iter().flatten() {
            if let Some(original) = track["orig_src"].as_str() {
                repaired.push((
                    original,
                    track["src"].as_str().unwrap_or_default(),
                    track["source_identity"].clone(),
                ));
            }
        }
    }
    if repaired.is_empty() {
        return Ok(true);
    }
    let cache = state::Cache::load(&job.prepare_cache_path, events);
    for (original, actual, evidence) in repaired {
        alac::check_cancel(events)?;
        let cached = cache
            .find(original)
            .and_then(|e| e.validation.repaired_file.as_ref())
            .is_some_and(|identity| state::matches(actual, identity));
        let manifested = crate::identity::compute(actual)
            .is_ok_and(|current| crate::identity::equivalent(&evidence, &current));
        if !cached && !manifested {
            events.emit(1, "[快照] 修复音源已改变，将重新检查。");
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn execute(job: Job, events: &mut dyn Callbacks) -> Outcome {
    match run(&job, events) {
        Ok(data) => Outcome {
            exit_code: Some(0),
            failure: None,
            data: Some(data),
        },
        Err(failure) => Outcome {
            exit_code: None,
            failure: Some(failure),
            data: None,
        },
    }
}
/// # Errors
/// Returns an error on cancellation, production-directory creation or old
/// manifest removal failure, fingerprint/source enumeration failure, metadata
/// or decode job failure/missing results, ALAC repair failure, invalid basename,
/// or report/manifest write failure. Source validation issues remain in the
/// returned result; cache writes only emit warnings and snapshot-save failures
/// do not fail preparation.
pub fn run(job: &Job, events: &mut dyn Callbacks) -> Result<models::Result, Failure> {
    crate::metrics::measure(
        "prepare",
        vec![job.alac_fix_directory.clone()],
        0,
        events,
        |_, events| run_measured(job, events),
        Result::is_ok,
    )
}

fn run_measured(job: &Job, events: &mut dyn Callbacks) -> Result<models::Result, Failure> {
    alac::check_cancel(events)?;
    if let Some(production) = &job.production {
        fs::create_dir_all(&production.directory)?;
    }
    let start = Instant::now();
    let fingerprint = job.fingerprint()?;
    let evidence_path = job.prepare_cache_path.with_extension("identity.json");
    let cache_valid = state::read_json(&evidence_path).is_ok_and(|value| value == fingerprint);
    let mut cache = job.prepare_cache_enabled.then(|| {
        if cache_valid {
            state::Cache::load(&job.prepare_cache_path, events)
        } else {
            state::Cache::default()
        }
    });
    let reuse = cache.is_some() && !job.force_revalidation;
    let mut cached = HashMap::new();
    if job.manifest_path.is_file() {
        fs::remove_file(&job.manifest_path)?;
        events.emit(
            1,
            &format!("[清理] 旧 manifest 已移除: {}", job.manifest_path.display()),
        );
    }
    // Once preparation starts, a failed or cancelled run cannot advertise stale evidence.
    state::invalidate(&job.prepare_snapshot_path);
    let sources = state::enumerate_sources(Path::new(&job.source_directory))?;
    events.emit(1, &format!("发现 {} 个音频文件 (FLAC/M4A)", sources.len()));
    let progress_total = (sources.len() as u64).saturating_mul(2).max(1);
    events.progress(0, progress_total);
    let mut tracks = Vec::with_capacity(sources.len());
    let mut metadata_jobs = Vec::new();
    for path in &sources {
        alac::check_cancel(events)?;
        if let Some(entry) = cache.as_ref().filter(|_| reuse).and_then(|c| c.find(path)) {
            cached.insert(path.clone(), entry.clone());
        } else {
            metadata_jobs.push(job.audio(path, None));
        }
    }
    crate::task_log::emit(
        events,
        "source_metadata",
        0,
        metadata_jobs.len() as u64,
        "",
        "",
    );
    events.progress(cached.len() as u64, progress_total);
    let probed = super::audio_decode_parallel::inspect_jobs(
        &metadata_jobs,
        events,
        "source_metadata",
        cached.len() as u64,
        progress_total,
        None,
        audio::read_metadata,
    )?;
    let mut probed: HashMap<_, _> = probed
        .into_iter()
        .map(|track| (track.path.clone(), track))
        .collect();
    for path in &sources {
        tracks.push(if let Some(entry) = cached.get(path) {
            entry.probe.metadata(path)
        } else {
            probed
                .remove(path)
                .ok_or_else(|| Failure::new("InvalidOperation", "Missing audio metadata result"))?
        });
    }
    crate::task_log::emit(
        events,
        "source_metadata",
        metadata_jobs.len() as u64,
        metadata_jobs.len() as u64,
        "",
        "",
    );
    events.emit(
        1,
        &format!(
            "[缓存] 复用元数据 {} 首 / 重新探测 {} 首",
            cached.len(),
            sources.len() - cached.len()
        ),
    );
    rules::normalize_albums(&mut tracks);
    for track in tracks.iter().filter(|t| t.resample_to.is_some()) {
        events.emit(
            1,
            &format!(
                "  [重采样] {}: {}/{} -> {}/{}",
                track.title,
                track.source_sample_rate,
                track.source_bits,
                track.sample_rate,
                track.bits
            ),
        );
    }
    events.emit(
        1,
        &format!(
            "共需重采样 {} 首",
            tracks.iter().filter(|t| t.resample_to.is_some()).count()
        ),
    );
    let mut issues =
        rules::validate_channels(&tracks, &job.source_directory, job.mlp_source == "lpcm");
    let mut manifest = Map::new();
    let mut repairs = Vec::new();
    let mut checked = 0;
    let mut reused = 0;
    let decode_jobs: Vec<_> = tracks
        .iter()
        .filter(|track| {
            !cached
                .get(&track.path)
                .is_some_and(|entry| state::reusable(entry, track, rules::expected_samples(track)))
        })
        .map(|track| job.decode_audio(&track.path, track.resample_to))
        .collect();
    let duration_by_path: HashMap<_, _> = tracks
        .iter()
        .map(|track| (track.path.as_str(), track.duration))
        .collect();
    let decode_durations: Vec<_> = decode_jobs
        .iter()
        .map(|job| duration_by_path[job.input.as_str()])
        .collect();
    crate::task_log::emit(events, "source_decode", 0, decode_jobs.len() as u64, "", "");
    let decoded = super::audio_decode_parallel::inspect_jobs(
        &decode_jobs,
        events,
        "source_decode",
        (sources.len() + sources.len() - decode_jobs.len()) as u64,
        progress_total,
        Some(&decode_durations),
        audio::check_decode,
    )?;
    let decode_count = decoded.len();
    let mut decoded: HashMap<_, _> = decode_jobs
        .iter()
        .zip(decoded)
        .map(|(job, result)| (job.input.clone(), result))
        .collect();
    let mut decode_outputs: HashMap<_, _> = decode_jobs
        .iter()
        .filter_map(|job| {
            job.decode_output
                .as_ref()
                .map(|output| (job.input.clone(), output.clone()))
        })
        .collect();
    for ((rate, bits), group) in rules::ordered_groups(&tracks) {
        let name = format!("group_{rate}_{bits}");
        let mut files = Vec::with_capacity(group.len());
        for (index, track) in group.into_iter().enumerate() {
            alac::check_cancel(events)?;
            checked += 1;
            crate::task_log::emit(
                events,
                "source_result",
                checked as u64,
                sources.len() as u64,
                &track.title,
                &track.path,
            );
            let mut source = track.path.clone();
            let mut original = None;
            let mut detail = None;
            let mut patches = Vec::new();
            let expected = rules::expected_samples(track);
            let decode_rate = job
                .production
                .as_ref()
                .map_or(track.sample_rate, |production| production.rate);
            let decode_expected = rules::expected_samples_at(track, decode_rate);
            let mut prepared_pcm = None;
            let mut prepared_mlp = None;
            if let Some(entry) = cached
                .get(&track.path)
                .filter(|entry| state::reusable(entry, track, expected))
            {
                reused += 1;
                if let Some(file) = &entry.validation.repaired_file {
                    original = Some(track.path.clone());
                    source = file["Path"].as_str().unwrap().into();
                    patches = entry.validation.patches.clone().unwrap_or_default();
                    detail = Some(patches.iter().map(rules::repair_detail).collect());
                    repairs.push(models::Repair {
                        original_path: track.path.clone(),
                        repaired_path: source.clone(),
                        patches: patches.clone(),
                    });
                    events.emit(
                        1,
                        &format!(
                            "  ++ [缓存复用] {}: 已校验的 ALAC 修复结果（{} 处）",
                            track.title,
                            patches.len()
                        ),
                    );
                }
            } else {
                let mut check = decoded.remove(&source).ok_or_else(|| {
                    Failure::new("InvalidOperation", "Missing audio decode result")
                })?;
                if (check.error_count > 0
                    || (check.samples.is_some()
                        && decode_expected.is_some()
                        && check.samples != decode_expected))
                    && let Some(repair) = alac::try_repair(
                        &job.library,
                        &source,
                        &job.alac_fix_directory,
                        &mut Quiet(events),
                    )?
                {
                    let repaired_job = job.decode_audio(&repair.output_path, track.resample_to);
                    if let Some(output) = &repaired_job.decode_output {
                        decode_outputs.insert(repair.output_path.clone(), output.clone());
                    }
                    let repaired = audio::check_decode(&repaired_job, &mut Quiet(events))?;
                    if repaired.error_count == 0
                        && repaired.samples.is_some()
                        && (decode_expected.is_none() || repaired.samples == decode_expected)
                    {
                        original = Some(source);
                        source = repair.output_path;
                        check = repaired;
                        patches = repair.patches;
                        detail = Some(patches.iter().map(rules::repair_detail).collect());
                        repairs.push(models::Repair {
                            original_path: original.clone().unwrap(),
                            repaired_path: source.clone(),
                            patches: patches.clone(),
                        });
                        events.emit(
                            1,
                            &format!(
                                "  ++ [已修复] {}: ALAC END 标记 {} 处，解码采样数已达标",
                                track.title,
                                patches.len()
                            ),
                        );
                    } else {
                        let _ = fs::remove_file(&repair.output_path);
                    }
                }
                if check.error_count == 0
                    && check.samples.is_some()
                    && let Some(output) = decode_outputs.get(&source)
                {
                    let path = output.prepared_path();
                    if path.is_file() {
                        prepared_pcm = Some(path);
                    }
                    if let Some(path) = output.mlp_output.as_ref().filter(|path| path.is_file()) {
                        prepared_mlp = Some(path.clone());
                    }
                }
                if let Some(issue) = rules::evaluate(
                    track,
                    &source,
                    &check,
                    rules::DecodeTarget {
                        samples: decode_expected,
                        sample_rate: decode_rate,
                    },
                    patches.len(),
                    job.loss_error_seconds,
                    job.loss_warning_seconds,
                ) {
                    events.emit(
                        1,
                        &format!(
                            "  {} [{}] {}: {}",
                            if issue.level == "FAIL" { "!!" } else { " ?" },
                            issue.level,
                            track.title,
                            issue.reason
                        ),
                    );
                    issues.push(issue);
                } else if prepared_pcm.is_none()
                    && prepared_mlp.is_none()
                    && let Some(cache) = &mut cache
                {
                    state::record(cache, track, &source, expected, check.samples, &patches);
                }
            }
            let basename = crate::config::safe_basename(json!(track.path))
                .map_err(|e| Failure::new("InvalidData", &e))?;
            let duration = if track.duration.abs() < 1e16 {
                (track.duration * 1e6).round_ties_even() / 1e6
            } else {
                track.duration
            };
            let source_identity = crate::identity::compute(&source).ok();
            let prepared_pcm_identity = prepared_pcm
                .as_ref()
                .and_then(|path| crate::identity::compute(&path.to_string_lossy()).ok());
            let prepared_pcm_path = prepared_pcm_identity
                .is_some()
                .then(|| prepared_pcm.map(|path| path.to_string_lossy().into_owned()))
                .flatten();
            let prepared_pcm_signature = prepared_pcm_path.as_ref().and_then(|_| {
                job.production
                    .as_ref()
                    .map(|production| production.signature.clone())
            });
            let prepared_mlp_identity = prepared_mlp
                .as_ref()
                .and_then(|path| crate::identity::compute(&path.to_string_lossy()).ok());
            let prepared_mlp_path = prepared_mlp_identity
                .as_ref()
                .and(prepared_mlp)
                .map(|path| path.to_string_lossy().into_owned());
            let prepared_mlp_signature = prepared_mlp_path.as_ref().and_then(|_| {
                job.production
                    .as_ref()
                    .map(|production| production.signature.clone())
            });
            files.push(ManifestTrack {
                n: index + 1,
                src: source,
                name: format!("{name}/{:04}__{}", index + 1, basename.as_str().unwrap()),
                title: track.title.clone(),
                date: track.date.clone(),
                track: track.track.clone(),
                album: track.album.clone(),
                dur: duration,
                resample_to: track.resample_to,
                repaired: patches.len(),
                repair_detail: detail,
                orig_src: original,
                source_sample_rate: track.source_sample_rate,
                source_bits: track.source_bits,
                channels: track.channels,
                source_identity,
                prepared_pcm_path,
                prepared_pcm_identity,
                prepared_pcm_signature,
                prepared_mlp_path,
                prepared_mlp_identity,
                prepared_mlp_signature,
            });
            events.progress(
                sources.len() as u64 + checked.max(decode_count) as u64,
                progress_total,
            );
        }
        events.emit(1, &format!("{name}: {} 首", files.len()));
        manifest.insert(
            name,
            json!(ManifestGroup {
                sr: rate,
                bits,
                count: files.len(),
                files
            }),
        );
    }
    events.progress(progress_total, progress_total);
    crate::task_log::emit(
        events,
        "source_checked",
        checked as u64,
        sources.len() as u64,
        "",
        "",
    );
    let failure_count = issues.iter().filter(|i| i.level == "FAIL").count();
    let warning_count = issues.iter().filter(|i| i.level == "WARN").count();
    let result = models::Result {
        manifest,
        issues,
        checked_tracks: checked,
        repairs,
        failure_count,
        warning_count,
    };
    events.emit(
        1,
        &format!(
            "[缓存] 复用已校验结果 {reused} 首 / 重新校验 {} 首",
            checked - reused
        ),
    );
    alac::check_cancel(events)?;
    if let Some(cache) = cache {
        match cache
            .save(&job.prepare_cache_path)
            .and_then(|()| state::write_json(&evidence_path, &fingerprint))
        {
            Ok(()) => events.emit(
                1,
                &format!(
                    "[缓存] 已更新 {} 条音源校验记录 -> {}",
                    cache.len(),
                    job.prepare_cache_path.display()
                ),
            ),
            Err(error) => events.emit(1, &format!("[警告] 准备缓存写入失败: {}", error.message)),
        }
    }
    state::write_bytes(
        &job.report_path,
        localized_report(&result, &job.language).as_bytes(),
    )?;
    events.emit(1, &format!("报告已写入: {}", job.report_path.display()));
    alac::check_cancel(events)?;
    if result.failure_count == 0 {
        state::write_json(&job.manifest_path, &result.manifest)?;
        events.emit(
            1,
            &format!("manifest.json 已生成 -> {}", job.manifest_path.display()),
        );
        events.emit(1, &format!("总计 {} 首", result.checked_tracks));
        if state::save_snapshot(
            &job.prepare_snapshot_path,
            &job.source_directory,
            &fingerprint,
            &job.manifest_path.to_string_lossy(),
            &sources,
        ) {
            events.emit(
                1,
                &format!(
                    "[快照] 已保存音源指纹 -> {}",
                    job.prepare_snapshot_path.display()
                ),
            );
        }
    }
    events.emit(
        1,
        &format!(
            "[耗时] prepare 总计: {:.3} 秒",
            start.elapsed().as_secs_f64()
        ),
    );
    Ok(result)
}
#[must_use]
pub fn localized_report(result: &models::Result, language: &str) -> String {
    let original = report(result);
    if matches!(language, "zh" | "zh-CN") {
        return original;
    }
    original
        .lines()
        .map(|line| {
            let translated = crate::localization::translate(language, line);
            if translated != line {
                return translated;
            }
            let text = line.trim_start();
            format!(
                "{}{}",
                &line[..line.len() - text.len()],
                crate::localization::translate(language, text)
            )
        })
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n"
}

#[must_use]
pub fn report(result: &models::Result) -> String {
    let mut lines = vec![
        "音源校验报告".into(),
        "=".repeat(68),
        format!(
            "已校验 {} 首；失败 {} 首，警告 {} 首",
            result.checked_tracks, result.failure_count, result.warning_count
        ),
        String::new(),
    ];
    if result.issues.is_empty() {
        lines.push("全部通过：无解码错误，解码采样数与源声明一致，组内参数一致。".into());
    }
    for level in ["FAIL", "WARN"] {
        for issue in result.issues.iter().filter(|i| i.level == level) {
            lines.push(format!("[{}] {}", issue.level, issue.title));
            lines.push(format!("    原因: {}", issue.reason));
            lines.push(format!("    源文件: {}", issue.path));
            lines.extend(issue.detail.iter().map(|detail| format!("    {detail}")));
            lines.push(String::new());
        }
    }
    if !result.repairs.is_empty() {
        lines.extend([
            String::new(),
            "-".repeat(68),
            "ALAC END 标记修复记录".into(),
            "-".repeat(68),
            "说明：仅补写 Apple ALAC 未压缩帧尾缺失的 3 位 END 标记，原文件不修改。".into(),
            String::new(),
        ]);
        for repair in &result.repairs {
            lines.push(format!(
                "[已修复 {} 帧] {}",
                repair.patches.len(),
                Path::new(&repair.original_path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ));
            lines.push(format!("    原文件: {}", repair.original_path));
            lines.push(format!("    修复后: {}", repair.repaired_path));
            lines.extend(
                repair
                    .patches
                    .iter()
                    .map(|patch| format!("    {}", rules::repair_detail(patch))),
            );
            lines.push(String::new());
        }
    }
    lines.join("\r\n") + "\r\n"
}

#[cfg(test)]
mod report_tests {
    use super::*;
    #[test]
    fn reports_translate_reasons_without_changing_paths_or_titles() {
        let result = models::Result {
            manifest: Map::new(),
            checked_tracks: 1,
            failure_count: 1,
            warning_count: 0,
            repairs: Vec::new(),
            issues: vec![models::Issue {
                level: "FAIL".into(),
                title: "标题-日本語🎵".into(),
                path: "D:/音源/不能改名.flac".into(),
                reason: "音源检查失败，请修复问题后再制作。".into(),
                detail: vec!["Native decoder unknown diagnostic 1234".into()],
            }],
        };
        assert_eq!(localized_report(&result, "zh-CN"), report(&result));
        for lang in ["en", "ja"] {
            let translated = localized_report(&result, lang);
            assert!(!translated.contains("音源校验报告"));
            assert!(
                !translated.contains("音源检查失败，请修复问题后再制作。"),
                "{translated}"
            );
            assert!(translated.contains("标题-日本語🎵"));
            assert!(translated.contains("D:/音源/不能改名.flac"));
            assert!(translated.contains("Native decoder unknown diagnostic 1234"));
        }
    }

    #[test]
    fn repaired_snapshot_accepts_manifest_identity_without_decode_cache() {
        struct Silent;
        impl Callbacks for Silent {
            fn emit(&mut self, _: i32, _: &str) {}
            fn cancelled(&mut self) -> bool {
                false
            }
        }
        let root = std::env::temp_dir().join(format!(
            "dvda-repaired-evidence-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let original = root.join("original.m4a");
        let repaired = root.join("repaired.m4a");
        fs::write(&original, b"original").unwrap();
        fs::write(&repaired, b"repaired").unwrap();
        let identity = crate::identity::compute(&repaired.to_string_lossy()).unwrap();
        state::write_json(
            &root.join("manifest.json"),
            &json!({"group":{"files":[{"orig_src":original,"src":repaired,"source_identity":identity}]}}),
        )
        .unwrap();
        let job = Job {
            language: "zh-CN".into(),
            library: PathBuf::new(),
            source_directory: root.to_string_lossy().into_owned(),
            manifest_path: root.join("manifest.json"),
            report_path: root.join("report.txt"),
            prepare_cache_path: root.join("missing-cache.json"),
            prepare_snapshot_path: root.join("snapshot.json"),
            alac_fix_directory: root.clone(),
            mlp_source: "mlp-batch".into(),
            prepare_cache_enabled: true,
            loss_error_seconds: 0.05,
            loss_warning_seconds: 0.005,
            force_revalidation: false,
            production: None,
        };
        assert!(repaired_targets_valid(&job, &mut Silent).unwrap());
        fs::write(&repaired, b"changed").unwrap();
        assert!(!repaired_targets_valid(&job, &mut Silent).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
}
