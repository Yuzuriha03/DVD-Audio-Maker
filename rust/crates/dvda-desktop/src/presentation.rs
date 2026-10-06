//! Presentation rules retained from TaskLogPresentation without coupling to widgets.
use super::{Lang, localized_detail, localized_log};

pub fn phrase(lang: Lang, zh: &'static str, en: &'static str, ja: &'static str) -> &'static str {
    match lang {
        Lang::Zh => zh,
        Lang::En => en,
        Lang::Ja => ja,
    }
}

pub fn problem(text: &str) -> bool {
    let text = text.trim();
    if text.starts_with('{')
        && let Ok(event) = serde_json::from_str::<serde_json::Value>(text)
    {
        return match event.get("Kind").and_then(serde_json::Value::as_str) {
            Some("PcmError") => true,
            Some("Detail") => event
                .get("Value")
                .and_then(serde_json::Value::as_str)
                .is_some_and(problem),
            _ => false,
        };
    }
    let lower = text.to_ascii_lowercase();
    text.contains("VOLUME_ID_MISMATCH:")
        || text.starts_with(['?'])
        || text.starts_with("!!")
        || [
            "[err]",
            "[error]",
            "[fail]",
            "[fatal]",
            "[war]",
            "[warn]",
            "[warning]",
            "[警告]",
            "[错误]",
            "warning:",
            "[ffmpeg pcm]",
        ]
        .iter()
        .any(|p| lower.starts_with(p))
        || lower
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .any(|word| matches!(word, "error" | "failed" | "fatal" | "cannot"))
        || ["could not", "no such file", "not found"]
            .iter()
            .any(|p| lower.contains(p))
        || text.contains("失败")
        || text.contains("错误")
}

pub fn activity_only(text: &str) -> bool {
    text.starts_with("[PCM] ")
        || serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .is_some_and(|v| v["Kind"] == "PcmProgress")
}

pub fn summary(lang: Lang, text: &str, explicit_problem: bool) -> Option<String> {
    let text = text.trim();
    if text.is_empty()
        || activity_only(text)
        || text.starts_with("[debug]")
        || text.starts_with("[MLP] 临时目录:")
        || text.starts_with("[MLP] MLP 输出目录:")
    {
        return None;
    }
    if text.contains("VOLUME_ID_MISMATCH:") {
        return Some(phrase(lang,"光盘名称与成品卷标不一致。请确认方案对应这份成品；名称含中文时，可改用英文名称重新制作。","The disc name differs from its volume label. Check that this profile belongs to the disc; if needed, rebuild with an English disc name.","ディスク名とボリューム名が一致しません。対応するプロファイルを確認し、必要なら英語のディスク名で作り直してください。").into());
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("directory not recognized")
        || lower.starts_with("[err]") && lower.contains("directory")
    {
        return Some(phrase(lang,"制盘工具报告目录问题。详情保留在详细日志中，请结合最终任务状态和成品验证判断。","The author reported a directory warning. Details are retained in the full log; check the final task status and verify the disc.","作成ツールがフォルダーの警告を報告しました。詳細ログを参照し、最終結果とディスク検証を確認してください。").into());
    }
    if lower.contains("coherence test for iso start sector failed") {
        return Some(phrase(lang,"制盘工具报告扇区位置差异。制作后请运行“验证成品”，详细信息已保留。","The author reported a sector-position difference. Run Verify discs after building; details are retained.","作成ツールがセクター位置の差異を報告しました。作成後にディスクを検証してください。詳細は保存されています。").into());
    }
    if explicit_problem || problem(text) {
        return Some(localized_log(lang, text));
    }
    if text.starts_with("[MLP-FINALIZE] ") {
        return Some(localized_log(lang, text));
    }
    if let Some(value) = text.strip_prefix("[MLP] 提交 ")
        && let Some(count) = value
            .split_whitespace()
            .next()
            .filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
    {
        return Some(
            phrase(
                lang,
                "开始 MLP 编码，共 {0} 首音频。",
                "Starting MLP encoding for {0} tracks.",
                "{0} 曲の MLP エンコードを開始します。",
            )
            .replace("{0}", count),
        );
    }
    if text.starts_with("[MLP DLL]") {
        let fields: Vec<_> = text.split(" / ").collect();
        if fields.len() >= 4 {
            let rate = fields[fields.len() - 3]
                .split_whitespace()
                .next()
                .unwrap_or("?");
            let bits = fields[fields.len() - 2]
                .split_whitespace()
                .next()
                .unwrap_or("?");
            let channels = fields[fields.len() - 1]
                .split_whitespace()
                .next()
                .unwrap_or("?");
            return Some(
                phrase(
                    lang,
                    "音轨编码完成 · {0} Hz · {1} 位 · {2} 声道",
                    "Track encoded · {0} Hz · {1} bit · {2} channels",
                    "エンコード完了 · {0} Hz · {1} ビット · {2} チャンネル",
                )
                .replace("{0}", rate)
                .replace("{1}", bits)
                .replace("{2}", channels),
            );
        }
    }
    if text.starts_with('{') {
        let event: serde_json::Value = serde_json::from_str(text).ok()?;
        if event["Kind"] == "Detail" {
            return summary(lang, event["Value"].as_str().unwrap_or_default(), false);
        }
        return Some(localized_log(lang, text));
    }
    // New Rust progress prefixes replace the old pipeline's console summaries.
    let known = [
        "发现 ",
        "共需重采样 ",
        "[缓存]",
        "[MLP]",
        "[MLP-PROGRESS] ",
        "[MLP DLL]",
        "[LPCM]",
        "[prepare]",
        "[build]",
        "[verify]",
        "[author] disc ",
        "[resume]",
        "[menu]",
        "[续跑]",
        "[恢复]",
        "总计 ",
        "总曲目 ",
        "第 ",
        "容量检查：",
        "=== 分盘结果",
        "成品：",
        "报告已写入:",
        "manifest.json 已生成",
        "[信息]",
    ];
    let translated = localized_detail(lang, text);
    if known.iter().any(|prefix| text.starts_with(prefix))
        || translated != text
        || [
            "ready",
            "ready_hint",
            "loaded",
            "done",
            "failed",
            "cancelled",
            "stopping",
            "started_check",
            "started_build",
            "started_verify",
            "saved",
            "profile_error",
        ]
        .iter()
        .any(|key| {
            [Lang::Zh, Lang::En, Lang::Ja]
                .iter()
                .any(|&source| text.starts_with(super::tr(source, key)))
        })
    {
        Some(localized_log(lang, text))
    } else {
        None
    }
}

pub fn error_advice(lang: Lang, error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("access is denied") || lower.contains("permission denied") || lower.contains("os error 5") || error.contains("拒绝访问") {
        phrase(lang,"请选择有写入权限的输出与工作目录，然后重试。","Choose output and work folders that you can write to, then retry.","書き込み可能な出力フォルダーと作業フォルダーを選び、再試行してください。")
    } else if lower.contains("timed out") || lower.contains("timeout") || error.contains("超时") {
        phrase(lang,"处理超时。请检查音源和工具状态；可导出详细日志排查。","The operation timed out. Check the audio and tools; export the full log for details.","処理がタイムアウトしました。音源とツールを確認し、詳細ログを保存してください。")
    } else if lower.contains("no such file") || lower.contains("not found") || lower.contains("missing") || error.contains("不存在") || error.contains("无法启动") {
        phrase(lang,"请检查音源、文件和工具路径，以及移动硬盘或网络盘是否已连接。","Check the audio, file and tool paths, and that external or network drives are connected.","音源・ファイル・ツールのパス、および外付けドライブやネットワークドライブの接続を確認してください。")
    } else {
        phrase(lang,"查看下方问题提示，调整设置后重试；需要排查时可导出详细日志。","Review the problems below and retry after adjusting settings. Export the full log if needed.","下の問題を確認し、設定を修正して再試行してください。必要に応じて詳細ログを保存できます。")
    }.into()
}

pub fn same_path(first: &std::path::Path, second: &std::path::Path) -> bool {
    fn normalized(path: &std::path::Path) -> String {
        path.canonicalize()
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join(path))
            .to_string_lossy()
            .to_ascii_lowercase()
    }
    normalized(first) == normalized(second)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_old_failure_markers_survive_summary_and_problem_filter() {
        for raw in [
            "[WAR] warning",
            "[ERR] issue",
            "[FAIL] failure",
            "[FATAL] fatal",
            "fatal error: x",
            "could not open",
            "no such file",
            "not found",
            "ERROR x",
            "!! broken",
            "? warning",
        ] {
            assert!(problem(raw), "{raw}");
            for lang in [Lang::Zh, Lang::En, Lang::Ja] {
                assert!(summary(lang, raw, false).is_some());
            }
        }
        for raw in [
            "biterror_count=0",
            "no errors reported",
            "frame=4",
            "{\"Kind\":\"PcmProgress\",\"Name\":\"错误.flac\",\"Value\":50}",
        ] {
            assert!(!problem(raw), "{raw}");
        }
        assert!(problem(
            "{\"Kind\":\"Detail\",\"Value\":\"fatal error: decoder\"}"
        ));
    }
    #[test]
    fn chatter_is_detail_only_and_explanations_are_localized() {
        for raw in [
            "[PCM] 50%",
            "[MLP] 临时目录: C:/work",
            "[MLP] MLP 输出目录: C:/output",
            "[debug] internal",
            "frame=9 speed=1",
        ] {
            assert!(summary(Lang::En, raw, false).is_none(), "{raw}");
        }
        for lang in [Lang::Zh, Lang::En, Lang::Ja] {
            assert!(summary(lang, "[ERR] Directory not recognized", false).is_some());
            assert!(!error_advice(lang, "Access is denied (os error 5)").is_empty());
            assert_ne!(
                error_advice(lang, "timeout"),
                error_advice(lang, "missing source")
            );
        }
    }

    #[test]
    fn mlp_and_verification_progress_are_visible_in_summary() {
        let mlp = "[MLP-PROGRESS] 37/147";
        let verify = "[verify] pcm 4/12 ok: D:/music/track.flac";
        let mismatch = "[verify] pcm 5/12 failed: D:/music/bad.flac";
        let complete = "[verify] complete 2 discs 24 tracks ok";
        assert!(problem(mismatch));
        for lang in [Lang::Zh, Lang::En, Lang::Ja] {
            let mlp_text = summary(lang, mlp, false).unwrap();
            let verify_text = summary(lang, verify, false).unwrap();
            let complete_text = summary(lang, complete, false).unwrap();
            assert!(mlp_text.contains("37/147"), "{mlp_text}");
            assert!(verify_text.contains("4/12"), "{verify_text}");
            assert!(verify_text.contains("D:/music/track.flac"), "{verify_text}");
            let mismatch_text = summary(lang, mismatch, false).unwrap();
            assert!(mismatch_text.contains("5/12"), "{mismatch_text}");
            assert!(
                mismatch_text.contains("D:/music/bad.flac"),
                "{mismatch_text}"
            );
            assert!(complete_text.contains("2") && complete_text.contains("24"));
        }
    }
}
