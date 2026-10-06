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
}
fn report_language() -> String {
    "zh-CN".into()
}
impl Job {
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
        }
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
                repaired.push((original, track["src"].as_str().unwrap_or_default()));
            }
        }
    }
    if repaired.is_empty() {
        return Ok(true);
    }
    let cache = state::Cache::load(&job.prepare_cache_path, events);
    for (original, actual) in repaired {
        alac::check_cancel(events)?;
        if !cache
            .find(original)
            .and_then(|e| e.validation.repaired_file.as_ref())
            .is_some_and(|identity| state::matches(actual, identity))
        {
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
pub fn run(job: &Job, events: &mut dyn Callbacks) -> Result<models::Result, Failure> {
    alac::check_cancel(events)?;
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
    for (index, path) in sources.iter().enumerate() {
        alac::check_cancel(events)?;
        if let Some(entry) = cache.as_ref().filter(|_| reuse).and_then(|c| c.find(path)) {
            tracks.push(entry.probe.metadata(path));
            cached.insert(path.clone(), entry.clone());
        } else {
            tracks.push(audio::read_metadata(
                &job.audio(path, None),
                &mut Quiet(events),
            )?);
        }
        events.progress(index as u64 + 1, progress_total);
    }
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
    for ((rate, bits), group) in rules::ordered_groups(&tracks) {
        let name = format!("group_{rate}_{bits}");
        let mut files = Vec::with_capacity(group.len());
        for (index, track) in group.into_iter().enumerate() {
            alac::check_cancel(events)?;
            checked += 1;
            let mut source = track.path.clone();
            let mut original = None;
            let mut detail = None;
            let mut patches = Vec::new();
            let expected = rules::expected_samples(track);
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
                let mut check = audio::check_decode(
                    &job.audio(&source, track.resample_to),
                    &mut Quiet(events),
                )?;
                if (check.error_count > 0
                    || (check.samples.is_some() && expected.is_some() && check.samples != expected))
                    && let Some(repair) = alac::try_repair(
                        &job.library,
                        &source,
                        &job.alac_fix_directory,
                        &mut Quiet(events),
                    )?
                {
                    let repaired = audio::check_decode(
                        &job.audio(&repair.output_path, track.resample_to),
                        &mut Quiet(events),
                    )?;
                    if repaired.error_count == 0
                        && repaired.samples.is_some()
                        && (expected.is_none() || repaired.samples == expected)
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
                if let Some(issue) = rules::evaluate(
                    track,
                    &source,
                    &check,
                    expected,
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
                } else if let Some(cache) = &mut cache {
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
            });
            events.progress(sources.len() as u64 + checked as u64, progress_total);
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
            .and_then(|_| state::write_json(&evidence_path, &fingerprint))
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
}
