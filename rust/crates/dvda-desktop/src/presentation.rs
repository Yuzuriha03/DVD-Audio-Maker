//! Presentation rules retained from TaskLogPresentation without coupling to widgets.
use super::{Lang, localized_detail, localized_log};

pub fn phrase(
    lang: Lang,
    zh: &'static str,
    hant: &'static str,
    en: &'static str,
    ja: &'static str,
    ko: &'static str,
) -> &'static str {
    match lang {
        Lang::Zh => zh,
        Lang::ZhHant => hant,
        Lang::En => en,
        Lang::Ja => ja,
        Lang::Ko => ko,
    }
}

pub fn problem(text: &str) -> bool {
    let text = text.trim();
    if text.starts_with("[METRIC] ") {
        return false;
    }
    if let Some(event) = dvda_core::task_log::parse(text) {
        return event.warning || event.action == "pcm_failed";
    }
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
            "warn:",
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
    if text.starts_with("[METRIC] ") {
        return None;
    }
    // Repetitive author diagnostics are retained verbatim in the archive.
    // The coordinator emits category summaries, including warning severity.
    if text.starts_with("[menu-cover] oversized ")
        || text.starts_with("WARN: Button y coordinates are odd for button ")
    {
        return None;
    }
    if let Some(message) = task_message(lang, text) {
        return Some(message);
    }
    if let Some(message) = space_message(lang, text) {
        return Some(message);
    }
    if let Some(message) = group_message(lang, text) {
        return Some(message);
    }
    if text.is_empty()
        || activity_only(text)
        || text.starts_with("[debug]")
        || text.starts_with("[MLP] 临时目录:")
        || text.starts_with("[MLP] MLP 输出目录:")
    {
        return None;
    }
    if text.contains("VOLUME_ID_MISMATCH:") {
        return Some(phrase(lang,"光盘名称与成品卷标不一致。请确认方案对应这份成品；名称含中文时，可改用英文名称重新制作。",
"光碟名稱與成品卷標不一致。請確認方案對應這份成品；名稱含中文時，可改用英文名稱重新制作。","The disc name differs from its volume label. Check that this profile belongs to the disc; if needed, rebuild with an English disc name.","ディスク名とボリューム名が一致しません。対応するプロファイルを確認し、必要なら英語のディスク名で作り直してください。", "디스크 이름과 볼륨 레이블이 다릅니다. 이 프로필이 해당 디스크의 것인지 확인하고, 필요하면 영문 디스크 이름으로 다시 만드세요.").into());
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("directory not recognized")
        || lower.starts_with("[err]") && lower.contains("directory")
    {
        return Some(phrase(lang,"制盘工具报告目录问题。详情保留在详细日志中，请结合最终任务状态和成品验证判断。",
"制盤工具報告目錄問題。詳情保留在詳細日志中，請結合最終任務狀態和成品驗證判斷。","The author reported a directory warning. Details are retained in the full log; check the final task status and verify the disc.","作成ツールがフォルダーの警告を報告しました。詳細ログを参照し、最終結果とディスク検証を確認してください。", "제작 도구에서 폴더 경고를 보고했습니다. 전체 로그의 세부 정보와 최종 작업 상태를 확인하고 디스크를 검증하세요.").into());
    }
    if lower.contains("coherence test for iso start sector failed") {
        return Some(phrase(lang,"制盘工具报告扇区位置差异，后续成品校验会继续检查；详细信息已保留。",
"制盤工具報告扇區位置差異，后續成品校驗會繼續檢查；詳細資訊已保留。","The author reported a sector-position difference. Output verification will check it; details are retained.","作成ツールがセクター位置の差異を報告しました。続く出力検証で確認します。詳細は保存されています。", "제작 도구에서 섹터 위치 차이를 보고했습니다. 출력 검증에서 확인하며 세부 정보는 보존됩니다.").into());
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
                "開始 MLP 編碼，共 {0} 首音訊。",
                "Starting MLP encoding for {0} tracks.",
                "{0} 曲の MLP エンコードを開始します。",
                "{0}곡의 MLP 인코딩을 시작합니다.",
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
                    "音軌編碼完成 · {0} Hz · {1} 位 · {2} 聲道",
                    "Track encoded · {0} Hz · {1} bit · {2} channels",
                    "エンコード完了 · {0} Hz · {1} ビット · {2} チャンネル",
                    "트랙 인코딩 완료 · {0} Hz · {1}비트 · {2}채널",
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
        "[MLP-CHECK] ",
        "[MLP DLL]",
        "[menu-cover]",
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
            [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja]
                .iter()
                .any(|&source| text.starts_with(super::tr(source, key)))
        })
    {
        Some(localized_log(lang, text))
    } else {
        None
    }
}

/// Only meaningful task state can replace the activity line. Changing the
/// selected log view must not make decoder chatter the current task state.
pub fn activity(lang: Lang, text: &str) -> Option<String> {
    let text = text.trim();
    if text.starts_with("[METRIC] ") {
        return None;
    }
    if let Some(event) = dvda_core::task_log::parse(text) {
        if matches!(
            event.action.as_str(),
            "oversized_summary" | "button_coordinates" | "button_coordinates_seen"
        ) {
            return None;
        }
        return task_message(lang, text);
    }
    if let Ok(event) = serde_json::from_str::<serde_json::Value>(text)
        && event["Kind"] == "Detail"
    {
        return activity(lang, event["Value"].as_str()?);
    }
    if activity_only(text) {
        return Some(localized_log(lang, text));
    }
    if text.starts_with("[space]")
        || text.starts_with("Number of samples:")
        || text.starts_with("[menu-cover] oversized ")
        || text.starts_with("WARN: Button y coordinates are odd for button ")
    {
        return None;
    }
    if problem(text) {
        return summary(lang, text, true);
    }
    let relevant = [
        "[MLP-CHECK]",
        "[MLP-PROGRESS]",
        "[MLP-FINALIZE]",
        "[menu-cover]",
        "[author]",
        "[verify]",
        "[LPCM]",
        "[续跑]",
        "[恢复]",
        "[TASK]",
    ];
    if relevant.iter().any(|prefix| text.starts_with(prefix)) || text.starts_with('{') {
        return summary(lang, text, false);
    }
    None
}

pub fn task_message(lang: Lang, raw: &str) -> Option<String> {
    let event = dvda_core::task_log::parse(raw)?;
    let c = event.current;
    let t = event.total;
    let n = &event.name;
    let d = &event.detail;
    let position = format!("{c}/{t}");
    let label = |zh, hant, en, ja, ko| phrase(lang, zh, hant, en, ja, ko);
    let base = match event.action.as_str() {
        "source_metadata" => format!(
            "{} {position}",
            label(
                "读取曲目信息：",
                "讀取曲目資訊：",
                "Reading track information:",
                "曲情報を読み込み中：",
                "트랙 정보 읽는 중:"
            )
        ),
        "source_decode" => format!(
            "{} {position}",
            label(
                "检查音源解码：",
                "檢查音源解碼：",
                "Checking source decoding:",
                "音源のデコードを確認中：",
                "음원 디코딩 확인 중:"
            )
        ),
        "source_result" => format!(
            "{} {position}",
            label(
                "整理音源检查结果：",
                "整理音源檢查結果：",
                "Processing source check results:",
                "音源の確認結果を整理中：",
                "음원 검사 결과 정리 중:"
            )
        ),
        "source_checked" => {
            return Some(format!(
                "{} {c}",
                label(
                    "音源检查完成，曲目数：",
                    "音源檢查完成，曲目數：",
                    "Source checks completed; tracks:",
                    "音源の確認完了、曲数：",
                    "음원 검사 완료; 트랙 수:"
                )
            ));
        }
        "disc_plan" => {
            return Some(match lang {
                Lang::Zh => format!("分盘规划完成：{c} 张光盘，共 {t} 首音轨。"),
                Lang::ZhHant => format!("分碟規劃完成：{c} 張光碟，共 {t} 首音軌。"),
                Lang::En => format!("Disc plan ready: {c} discs, {t} tracks."),
                Lang::Ja => format!("ディスク構成を作成しました：{c} 枚、計 {t} 曲。"),
                Lang::Ko => format!("디스크 구성 완료: 디스크 {c}개, 총 {t}곡."),
            });
        }
        "author_disc" => format!(
            "{} {position}",
            label(
                "制作光盘：",
                "制作光碟：",
                "Creating disc:",
                "ディスク作成中：",
                "디스크 제작 중:"
            )
        ),
        "author_audio" => format!(
            "{} {position}",
            label(
                "读取光盘音频：",
                "讀取光碟音訊：",
                "Reading disc audio:",
                "ディスク音声を読み込み中：",
                "디스크 오디오 읽는 중:"
            )
        ),
        "author_layout" => format!(
            "{} {position}",
            label(
                "分析音频布局，光盘：",
                "分析音訊布局，光碟：",
                "Analyzing audio layout, disc:",
                "音声配置を解析中、ディスク：",
                "오디오 배치 분석 중, 디스크:"
            )
        ),
        "author_navigation" => format!(
            "{} {position}",
            label(
                "制作光盘导航：",
                "制作光碟導航：",
                "Creating disc navigation:",
                "ディスクナビゲーションを作成中：",
                "디스크 탐색 정보 만드는 중:"
            )
        ),
        "author_stills" => format!(
            "{} {position}",
            label(
                "整理播放封面，光盘：",
                "整理播放封面，光碟：",
                "Preparing playback covers, disc:",
                "再生用カバーを準備中、ディスク：",
                "재생 표지 준비 중, 디스크:"
            )
        ),
        "author_iso" => format!(
            "{} {position}",
            label(
                "打包 ISO，光盘：",
                "打包 ISO，光碟：",
                "Writing ISO, disc:",
                "ISO を作成中、ディスク：",
                "ISO 기록 중, 디스크:"
            )
        ),
        "author_check_menu" => format!(
            "{} {position}",
            label(
                "检查已制作的菜单，光盘：",
                "檢查已制作的選單，光碟：",
                "Checking authored menus, disc:",
                "作成済みメニューを確認中、ディスク：",
                "제작된 메뉴 확인 중, 디스크:"
            )
        ),
        "publish_start" => {
            return Some(format!(
                "{} {d}",
                label(
                    "保存成品 ISO 到：",
                    "儲存成品 ISO 到：",
                    "Saving finished ISOs to:",
                    "完成 ISO の保存先：",
                    "완성된 ISO 저장 위치:"
                )
            ));
        }
        "published_iso" => {
            return Some(format!(
                "{} {position} · {n}",
                label(
                    "ISO 已保存：",
                    "ISO 已儲存：",
                    "ISO saved:",
                    "ISO を保存しました：",
                    "ISO 저장됨:"
                )
            ));
        }
        "published_directory" => {
            return Some(format!(
                "{} {d}",
                label(
                    "成品目录：",
                    "成品目錄：",
                    "Output folder:",
                    "出力フォルダー：",
                    "출력 폴더:"
                )
            ));
        }
        "verify_timeline" => {
            return Some(
                label(
                    "开始检查光盘音频时间线。",
                    "開始檢查光碟音訊時間線。",
                    "Checking disc audio timelines.",
                    "ディスク音声のタイムラインを確認します。",
                    "디스크 오디오 타임라인을 확인합니다.",
                )
                .into(),
            );
        }
        "verify_timeline_group" => format!(
            "{} {c}",
            label(
                "检查音频时间线，音轨组：",
                "檢查音訊時間線，音軌組：",
                "Checking audio timeline, group:",
                "音声タイムラインを確認中、グループ：",
                "오디오 타임라인 확인 중, 그룹:"
            )
        ),
        "verify_menu_disc" => format!(
            "{} {position}",
            label(
                "验证菜单，光盘：",
                "驗證選單，光碟：",
                "Verifying menus, disc:",
                "メニューを検証中、ディスク：",
                "메뉴 검증 중, 디스크:"
            )
        ),
        "verify_menu_page" => format!(
            "{} {position}",
            label(
                "验证菜单页：",
                "驗證選單頁：",
                "Verifying menu page:",
                "メニューページを検証中：",
                "메뉴 페이지 검증 중:"
            )
        ),
        "verify_audio_files" => {
            return Some(
                label(
                    "开始比对成盘音频与编码文件。",
                    "開始比對成盤音訊與編碼檔案。",
                    "Comparing authored audio with encoded files.",
                    "ディスク内音声とエンコード済みファイルを比較します。",
                    "제작된 오디오와 인코딩 파일을 비교합니다.",
                )
                .into(),
            );
        }
        "verify_audio_group" => format!(
            "{} {position}",
            label(
                "比对成盘音频，音轨组：",
                "比對成盤音訊，音軌組：",
                "Comparing authored audio, group:",
                "ディスク内音声を比較中、グループ：",
                "제작된 오디오 비교 중, 그룹:"
            )
        ),
        "verify_pcm_start" => {
            return Some(match lang {
                Lang::Zh => format!("开始完整音频对照：0/{t} 首，自动 {c} 路并行。"),
                Lang::ZhHant => format!("開始完整音訊對照：0/{t} 首，自動 {c} 路并行。"),
                Lang::En => {
                    format!("Starting full audio comparison: 0/{t} tracks, {c} automatic workers.")
                }
                Lang::Ja => format!("全音声の比較を開始：0/{t} 曲、自動 {c} 並列。"),
                Lang::Ko => format!("전체 오디오 비교 시작: 0/{t}곡, 자동 병렬 작업 {c}개."),
            });
        }
        "pcm_ok" => {
            return Some(format!(
                "{} {position} · {n} · {}",
                label("已校验：", "已校驗：", "Verified:", "検証済み：", "검증됨:"),
                label("一致", "一致", "match", "一致", "일치")
            ));
        }
        "pcm_failed" => {
            return Some(format!(
                "{} {position} · {n} · {} · {d}",
                label("已校验：", "已校驗：", "Verified:", "検証済み：", "검증됨:"),
                label("不一致", "不一致", "mismatch", "不一致", "불일치")
            ));
        }
        "oversized_summary" => {
            return Some(match lang {
                Lang::Zh => format!(
                    "第 {n} 张光盘有 {c} 条播放封面超过 2 MB 的提示；已继续制作，明细见详细日志。"
                ),
                Lang::ZhHant => format!(
                    "第 {n} 張光碟有 {c} 條播放封面超過 2 MB 的提示；已繼續製作，明細見詳細日志。"
                ),
                Lang::En => format!(
                    "Disc {n}: {c} playback-cover size notices (over 2 MB). Creation continued; see the full log."
                ),
                Lang::Ja => format!(
                    "ディスク {n}：再生用カバーが 2 MB を超える通知 {c} 件。作成を続行しました。詳細ログを参照してください。"
                ),
                Lang::Ko => format!(
                    "디스크 {n}: 재생 표지가 2 MB를 초과한다는 알림 {c}개. 제작은 계속됩니다. 전체 로그를 확인하세요."
                ),
            });
        }
        "button_coordinates_seen" => {
            return Some(match lang {
                Lang::Zh => format!(
                    "第 {n} 张光盘：制盘工具报告菜单按钮坐标警告，详细日志已保留，稍后汇总。"
                ),
                Lang::ZhHant => format!(
                    "第 {n} 張光碟：制盤工具報告選單按鈕坐標警告，詳細日志已保留，稍后匯總。"
                ),
                Lang::En => format!(
                    "Disc {n}: the author reported menu button-coordinate warnings. Details are retained; a summary will follow."
                ),
                Lang::Ja => format!(
                    "ディスク {n}：メニューボタン座標の警告があります。詳細を保存し、後ほど集計します。"
                ),
                Lang::Ko => format!(
                    "디스크 {n}: 메뉴 버튼 좌표 경고가 보고되었습니다. 세부 정보는 보존되며 나중에 요약됩니다."
                ),
            });
        }
        "button_coordinates" => {
            return Some(match lang {
                Lang::Zh => format!(
                    "第 {n} 张光盘：菜单按钮坐标警告共 {c} 条，请结合成品校验和播放结果检查；可导出详细日志。"
                ),
                Lang::ZhHant => format!(
                    "第 {n} 張光碟：選單按鈕坐標警告共 {c} 條，請結合成品校驗和播放結果檢查；可匯出詳細日志。"
                ),
                Lang::En => format!(
                    "Disc {n}: {c} menu button-coordinate warnings. Review verification and playback results; export the full log for details."
                ),
                Lang::Ja => format!(
                    "ディスク {n}：メニューボタン座標の警告 {c} 件。検証結果と再生を確認してください。詳細ログを保存できます。"
                ),
                Lang::Ko => format!(
                    "디스크 {n}: 메뉴 버튼 좌표 경고 {c}개. 검증 및 재생 결과를 확인하세요. 전체 로그를 내보낼 수 있습니다."
                ),
            });
        }
        _ => return None,
    };
    Some(if n.is_empty() {
        base
    } else {
        format!("{base} · {n}")
    })
}

fn space_message(lang: Lang, text: &str) -> Option<String> {
    let value = text.strip_prefix("[space] ")?;
    let (head, available) = value.rsplit_once(", available ")?;
    let (head, required) = head.rsplit_once(", required ")?;
    let required = required.strip_suffix(" B")?.parse::<u64>().ok()?;
    let available = if available == "unknown" {
        phrase(lang, "未知", "未知", "unknown", "不明", "알 수 없음").to_owned()
    } else {
        let bytes = available.strip_suffix(" B")?.parse::<u64>().ok()?;
        format!("{:.2} GiB", bytes as f64 / 1073741824.0)
    };
    let (volume, purpose) = head.split_once(": ")?;
    let purpose = if purpose == "MLP 编码输出" {
        phrase(
            lang,
            "MLP 编码输出",
            "MLP 編碼輸出",
            "MLP encoding output",
            "MLP エンコード出力",
            "MLP 인코딩 출력",
        )
        .into()
    } else if purpose == "author 中间产物与暂存 ISO + 成品 ISO 集合" {
        phrase(
            lang,
            "制盘临时文件与成品 ISO",
            "制盤暫存檔案與成品 ISO",
            "Authoring temporary files and finished ISOs",
            "作成用一時ファイルと完成 ISO",
            "제작 임시 파일 및 완성된 ISO",
        )
        .into()
    } else {
        dvda_core::localization::translate(lang.code(), purpose)
    };
    let gib = |bytes: u64| format!("{:.2} GiB", bytes as f64 / 1073741824.0);
    Some(match lang {
        Lang::Zh => format!(
            "空间检查 · {volume} · {purpose}：预计需要 {}，可用 {available}。",
            gib(required)
        ),
        Lang::ZhHant => format!(
            "空間檢查 · {volume} · {purpose}：預計需要 {}，可用 {available}。",
            gib(required)
        ),
        Lang::En => format!(
            "Space check · {volume} · {purpose}: estimated {}, available {available}.",
            gib(required)
        ),
        Lang::Ja => format!(
            "空き容量確認 · {volume} · {purpose}：必要量の目安 {}、使用可能 {available}。",
            gib(required)
        ),
        Lang::Ko => format!(
            "공간 확인 · {volume} · {purpose}: 예상 필요 {}, 사용 가능 {available}.",
            gib(required)
        ),
    })
}

fn group_message(lang: Lang, text: &str) -> Option<String> {
    let (parameters, tracks) = text.strip_prefix("group_")?.split_once(": ")?;
    let (rate, bits) = parameters.split_once('_')?;
    let rate = rate.parse::<u32>().ok()?;
    let bits = bits.parse::<u32>().ok()?;
    let tracks = tracks.strip_suffix(" 首")?.parse::<u32>().ok()?;
    let khz = format!("{} kHz", rate as f64 / 1000.0);
    Some(match lang {
        Lang::Zh => format!("音源分组 · {khz} / {bits} 位：{tracks} 首"),
        Lang::ZhHant => format!("音源分組 · {khz} / {bits} 位：{tracks} 首"),
        Lang::En => format!("Source group · {khz} / {bits} bit: {tracks} tracks"),
        Lang::Ja => format!("音源グループ · {khz} / {bits} ビット：{tracks} 曲"),
        Lang::Ko => format!("음원 그룹 · {khz} / {bits}비트: {tracks}곡"),
    })
}

pub fn error_advice(lang: Lang, error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("access is denied") || lower.contains("permission denied") || lower.contains("os error 5") || error.contains("拒绝访问") {
        phrase(lang,"请选择有写入权限的输出与工作目录，然后重试。",
"請選擇有寫入權限的輸出與工作目錄，然后重試。","Choose output and work folders that you can write to, then retry.","書き込み可能な出力フォルダーと作業フォルダーを選び、再試行してください。", "쓰기 권한이 있는 출력 폴더와 작업 폴더를 선택한 후 다시 시도하세요.")
    } else if lower.contains("timed out") || lower.contains("timeout") || error.contains("超时") {
        phrase(lang,"处理超时。请检查音源和工具状态；可导出详细日志排查。",
"處理超時。請檢查音源和工具狀態；可匯出詳細日志排查。","The operation timed out. Check the audio and tools; export the full log for details.","処理がタイムアウトしました。音源とツールを確認し、詳細ログを保存してください。", "작업 시간이 초과되었습니다. 음원과 도구 상태를 확인하고 전체 로그를 내보내세요.")
    } else if lower.contains("no such file") || lower.contains("not found") || lower.contains("missing") || error.contains("不存在") || error.contains("无法启动") {
        phrase(lang,"请检查音源、文件和工具路径，以及移动硬盘或网络盘是否已连接。",
"請檢查音源、檔案和工具路徑，以及移動硬盤或網絡盤是否已連接。","Check the audio, file and tool paths, and that external or network drives are connected.","音源・ファイル・ツールのパス、および外付けドライブやネットワークドライブの接続を確認してください。", "음원, 파일 및 도구 경로와 외장 또는 네트워크 드라이브의 연결을 확인하세요.")
    } else {
        phrase(lang,"查看下方问题提示，调整设置后重试；需要排查时可导出详细日志。",
"查看下方問題提示，調整設定后重試；需要排查時可匯出詳細日志。","Review the problems below and retry after adjusting settings. Export the full log if needed.","下の問題を確認し、設定を修正して再試行してください。必要に応じて詳細ログを保存できます。", "아래 문제를 확인하고 설정을 조정한 후 다시 시도하세요. 필요하면 전체 로그를 내보내세요.")
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
    use crate::ui_test_guard;
    #[test]
    fn metrics_remain_detail_only_even_with_failure_words() {
        let _ui_test_lock = ui_test_guard();
        let raw = r#"[METRIC] {"event":"finish","stage":"verify","succeeded":false,"temporary_roots":["C:\\failed\\error"],"incomplete_samples":1}"#;
        assert!(!problem(raw));
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja] {
            assert!(summary(lang, raw, false).is_none());
            assert!(summary(lang, raw, true).is_none());
            assert!(activity(lang, raw).is_none());
            assert_eq!(localized_log(lang, raw), raw);
        }
    }

    #[test]
    fn all_old_failure_markers_survive_summary_and_problem_filter() {
        let _ui_test_lock = ui_test_guard();
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
            for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja] {
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
        let _ui_test_lock = ui_test_guard();
        for raw in [
            "[PCM] 50%",
            "[MLP] 临时目录: C:/work",
            "[MLP] MLP 输出目录: C:/output",
            "[debug] internal",
            "frame=9 speed=1",
        ] {
            assert!(summary(Lang::En, raw, false).is_none(), "{raw}");
        }
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja] {
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
        let _ui_test_lock = ui_test_guard();
        let mlp = "[MLP-PROGRESS] 37/147";
        let verify = "[verify] pcm 4/12 ok: D:/music/track.flac";
        let mismatch = "[verify] pcm 5/12 failed: D:/music/bad.flac";
        let complete = "[verify] complete 2 discs 24 tracks ok";
        assert!(problem(mismatch));
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja] {
            let mlp_text = summary(lang, mlp, false).unwrap();
            let verify_text = summary(lang, verify, false).unwrap();
            let complete_text = summary(lang, complete, false).unwrap();
            assert!(mlp_text.contains("37/147"), "{mlp_text}");
            assert_eq!(
                verify_text,
                phrase(
                    lang,
                    "成品音频校验：4/12 首一致：D:/music/track.flac",
                    "成品音訊校驗：4/12 首一致：D:/music/track.flac",
                    "Output audio check: 4/12 tracks match: D:/music/track.flac",
                    "出力音声検証：4/12 曲一致：D:/music/track.flac",
                    "출력 오디오 검사: 4/12곡 일치: D:/music/track.flac"
                )
            );
            let mismatch_text = summary(lang, mismatch, false).unwrap();
            assert_eq!(
                mismatch_text,
                phrase(
                    lang,
                    "成品音频校验：5/12 首不一致：D:/music/bad.flac",
                    "成品音訊校驗：5/12 首不一致：D:/music/bad.flac",
                    "Output audio check: 5/12 tracks differ: D:/music/bad.flac",
                    "出力音声検証：5/12 曲不一致：D:/music/bad.flac",
                    "출력 오디오 검사: 5/12곡 불일치: D:/music/bad.flac"
                )
            );
            assert!(complete_text.contains("2") && complete_text.contains("24"));
        }
    }

    #[test]
    fn pcm_log_status_preserves_legacy_results_and_unknown_messages() {
        let _ui_test_lock = ui_test_guard();
        let path = r"D:\Music\三叩首 (伴奏版).flac";
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja] {
            for result in ["ok", "failed"] {
                assert_eq!(
                    localized_log(lang, &format!("[verify] pcm 147/147 {result}: {path}")),
                    localized_log(lang, &format!("[verify] pcm 147/147 {result} {path}")),
                );
            }
            let unknown = format!("[verify] pcm 147/147 pending: {path}");
            assert_eq!(localized_log(lang, &unknown), unknown);
        }
    }

    #[test]
    fn wrapped_details_cannot_replace_activity_with_chatter() {
        let _ui_test_lock = ui_test_guard();
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja] {
            for raw in [
                "Number of samples: 123456",
                "[space] C:\\: MLP 编码输出, required 1073741824 B, available unknown",
                "[menu-cover] oversized 17",
                "WARN: Button y coordinates are odd for button 1",
                "frame=9 speed=1",
            ] {
                let wrapped = serde_json::json!({"Kind": "Detail", "Value": raw}).to_string();
                let nested = serde_json::json!({"Kind": "Detail", "Value": wrapped}).to_string();
                assert!(activity(lang, raw).is_none(), "{raw}");
                assert!(activity(lang, &wrapped).is_none(), "{wrapped}");
                assert!(activity(lang, &nested).is_none(), "{nested}");
            }
            let diagnostic = "fatal error: decoder";
            let wrapped = serde_json::json!({"Kind": "Detail", "Value": diagnostic}).to_string();
            assert_eq!(activity(lang, &wrapped), activity(lang, diagnostic));
            assert!(activity(lang, &wrapped).is_some());
        }
    }

    #[test]
    fn space_checks_localize_known_and_unknown_capacity() {
        let _ui_test_lock = ui_test_guard();
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja] {
            for available in ["2147483648 B", "unknown"] {
                let raw = format!(
                    "[space] C:\\: MLP 编码输出, required 1073741824 B, available {available}"
                );
                let message = summary(lang, &raw, false).unwrap();
                assert!(message.contains("1.00 GiB"), "{message}");
                assert!(message.contains("C:\\"), "{message}");
                assert!(
                    message.contains(if available == "unknown" {
                        phrase(lang, "未知", "未知", "unknown", "不明", "알 수 없음")
                    } else {
                        "2.00 GiB"
                    }),
                    "{message}"
                );
                assert!(!message.contains("[space]"));
                assert!(activity(lang, &raw).is_none());
            }
            for raw in [
                "[space] C:\\: output, required invalid B, available unknown",
                "[space] C:\\: output, required 1 B, available invalid B",
            ] {
                assert!(space_message(lang, raw).is_none());
            }
        }
    }

    #[test]
    fn structured_workflow_progress_is_localized_without_detail_paths() {
        let _ui_test_lock = ui_test_guard();
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja] {
            for action in [
                "source_metadata",
                "source_decode",
                "source_result",
                "author_disc",
                "author_audio",
                "author_layout",
                "author_navigation",
                "author_stills",
                "author_iso",
                "author_check_menu",
                "verify_menu_disc",
                "verify_menu_page",
                "verify_audio_group",
                "pcm_ok",
                "published_iso",
            ] {
                let raw = format!(
                    "{}{}",
                    dvda_core::task_log::PREFIX,
                    serde_json::json!({
                        "Action": action, "Current": 3, "Total": 12,
                        "Name": "专辑 / 曲名 {0}", "Detail": "C:\\long\\source\\track.flac"
                    })
                );
                let message = summary(lang, &raw, false).unwrap();
                assert!(message.contains("3/12"), "{action}: {message}");
                assert!(message.contains("专辑 / 曲名 {0}"), "{message}");
                assert!(!message.contains("C:\\long"), "{message}");
                assert!(!message.contains("[TASK]"), "{message}");
                assert_eq!(activity(lang, &raw), Some(message));
                assert!(!problem(&raw));
            }
            for action in [
                "oversized_summary",
                "button_coordinates",
                "button_coordinates_seen",
            ] {
                let raw = format!(
                    "{}{}",
                    dvda_core::task_log::PREFIX,
                    serde_json::json!({
                        "Action": action, "Current": 103, "Total": 103,
                        "Name": "2", "Detail": "", "Warning": action != "oversized_summary"
                    })
                );
                assert!(summary(lang, &raw, false).is_some());
                assert!(activity(lang, &raw).is_none());
                assert_eq!(problem(&raw), action != "oversized_summary");
            }
            let raw = format!(
                "{}{}",
                dvda_core::task_log::PREFIX,
                serde_json::json!({
                    "Action": "pcm_failed", "Current": 3, "Total": 12,
                    "Name": "曲名", "Detail": "C:\\source.flac: sample mismatch"
                })
            );
            assert!(problem(&raw));
            assert!(
                summary(lang, &raw, false)
                    .unwrap()
                    .contains("sample mismatch")
            );
            assert!(activity(lang, &raw).unwrap().contains("C:\\source.flac"));
        }
    }

    #[test]
    fn mlp_cache_checks_are_distinct_from_encoding_in_all_languages() {
        let _ui_test_lock = ui_test_guard();
        for lang in [Lang::Zh, Lang::ZhHant, Lang::En, Lang::Ja] {
            for (raw, expected) in [
                ("[MLP-CHECK] start 147", "0/147"),
                ("[MLP-CHECK] progress 37/147 三叩首 {伴奏版}", "37/147"),
                ("[MLP-CHECK] complete 137 10", "137"),
            ] {
                let message = summary(lang, raw, false).unwrap();
                assert!(message.contains(expected), "{message}");
                assert!(!message.contains("[MLP-CHECK]"), "{message}");
                assert!(!problem(raw));
                assert!(!activity_only(raw));
                if raw.contains("三叩首") {
                    assert!(message.contains("三叩首 {伴奏版}"));
                }
            }
            let unknown = "[MLP-CHECK] complete 137 failed";
            assert_eq!(localized_log(lang, unknown), unknown);
        }
    }
}
