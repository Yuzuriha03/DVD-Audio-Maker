//! Shared log-line classification, hiding rules and rendering for every GUI.
//!
//! The Windows desktop shell and the Flutter/FRB Android host both call this
//! module, so one task produces the same log semantics on every platform.
//! Rendering is pure text; widgets, panels and colours stay in the shells.

use serde_json::Value;

use crate::localization;
use crate::task_log;

/// GUI languages. Codes match the profile `Language` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Zh,
    ZhHant,
    En,
    Ja,
    Ko,
}

impl Lang {
    /// Maps a profile/UI language code onto the supported enum.
    #[must_use]
    pub fn from_code(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" | "zh-hans" => Self::Zh,
            "zh-tw" | "zh-hant" | "zh-hk" | "zh-mo" => Self::ZhHant,
            "ja" | "ja-jp" => Self::Ja,
            "ko" | "ko-kr" => Self::Ko,
            _ => Self::En,
        }
    }

    /// Canonical profile language code for localization lookups.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Zh => "zh-CN",
            Self::ZhHant => "zh-TW",
            Self::En => "en",
            Self::Ja => "ja",
            Self::Ko => "ko",
        }
    }
}

/// Picks the localized variant of one fixed message.
#[must_use]
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

/// One `[space]` capacity warning rendered for every GUI.
///
/// The pipeline emits this fixed English wire format; the shared renderer
/// localizes the purpose, volume and both byte counts. Windows and Android
/// therefore show exactly the same sentence.
#[must_use]
pub fn space_message(lang: Lang, text: &str) -> Option<String> {
    let value = text.strip_prefix("[space] ")?;
    let (head, available) = value.rsplit_once(", available ")?;
    let (head, required) = head.rsplit_once(", required ")?;
    let required = required.strip_suffix(" B")?.parse::<u64>().ok()?;
    #[allow(
        clippy::cast_precision_loss,
        reason = "Human-readable GiB retains the existing f64 rounding to two decimals, not exact byte arithmetic"
    )]
    let gib = |bytes: u64| format!("{:.2} GiB", bytes as f64 / 1_073_741_824.0);
    let available = if available == "unknown" {
        phrase(lang, "未知", "未知", "unknown", "不明", "알 수 없음").to_owned()
    } else {
        let bytes = available.strip_suffix(" B")?.parse::<u64>().ok()?;
        gib(bytes)
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
        localization::translate(lang.code(), purpose)
    };
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
            "공간 확인 · {volume} · {purpose}: 예상 필요 {}, 사용 가능 {available}。",
            gib(required)
        ),
    })
}

/// Prefixes that only carry decoder/author chatter and are never shown.
const HIDDEN_NOISE_PREFIXES: &[&str] = &[
    "out_time_us=",
    "Number of samples:",
    "[menu-cover] oversized ",
    "WARN: Button y coordinates are odd for button ",
];

/// Prefixes excluded from the desktop summary pane but kept in the archive.
const HIDDEN_SUMMARY_DETAIL_PREFIXES: &[&str] =
    &["[debug]", "[MLP] 临时目录:", "[MLP] MLP 输出目录:"];

/// Activity-line prefixes that may replace the current task status.
const ACTIVITY_PREFIXES: &[&str] = &[
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

/// Summary prefixes that are meaningful task state, not chatter.
const SUMMARY_PREFIXES: &[&str] = &[
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
    "[bridge]",
];

fn localized_problem(language: &str, text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    for (markers, zh, hant, en, ja, ko) in [
        (
            &["[error]", "[err]", "[fail]", "[fatal]"][..],
            "[错误]",
            "[錯誤]",
            "[error]",
            "[エラー]",
            "[오류]",
        ),
        (
            &["[warn]", "[warning]", "[war]"][..],
            "[警告]",
            "[警告]",
            "[warning]",
            "[警告]",
            "[경고]",
        ),
    ] {
        for marker in markers {
            if lower.starts_with(marker) {
                let rest = text[marker.len()..].trim_start();
                let marker_text = phrase(Lang::from_code(language), zh, hant, en, ja, ko);
                let translated = localization::translate(language, rest);
                if translated.is_empty() {
                    return marker_text.to_owned();
                }
                return format!("{marker_text} {translated}");
            }
        }
    }
    localization::translate(language, text)
}

fn is_metric(text: &str) -> bool {
    text.starts_with("[METRIC] ")
}

/// True for the reproducible decoder/author noise every GUI hides.
#[must_use]
pub fn is_hidden_noise(text: &str) -> bool {
    let text = text.trim();
    HIDDEN_NOISE_PREFIXES
        .iter()
        .any(|prefix| text.starts_with(prefix))
}

/// True for lines that must never enter a GUI/diagnostic log ring.
///
/// Shared by every shell so noise filtering is not duplicated; real problem
/// lines are never classified as noise.
#[must_use]
pub fn is_diagnostic_noise(text: &str) -> bool {
    let text = text.trim();
    is_metric(text) || is_hidden_noise(text)
}

/// True for lines the desktop summary pane drops before its rich messages run
/// (metrics and the repetitive author diagnostics).
#[must_use]
pub fn is_hidden_summary_header(text: &str) -> bool {
    let text = text.trim();
    is_metric(text)
        || text.starts_with("[menu-cover] oversized ")
        || text.starts_with("WARN: Button y coordinates are odd for button ")
}

/// True for lines the desktop summary pane drops after its rich messages ran.
#[must_use]
pub fn is_hidden_summary_detail(text: &str) -> bool {
    let text = text.trim();
    text.is_empty()
        || activity_only(text)
        || HIDDEN_SUMMARY_DETAIL_PREFIXES
            .iter()
            .any(|prefix| text.starts_with(prefix))
}

/// True for lines that must not replace the desktop activity line.
#[must_use]
pub fn is_hidden_activity(text: &str) -> bool {
    let text = text.trim();
    is_metric(text)
        || is_hidden_noise(text)
        || serde_json::from_str::<Value>(text).is_ok_and(|value| value["Kind"] == "Finalized")
}

/// True for lines that belong to the readable task log.
#[must_use]
pub fn is_relevant_line(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty()
        && (SUMMARY_PREFIXES
            .iter()
            .any(|prefix| text.starts_with(prefix))
            || ACTIVITY_PREFIXES
                .iter()
                .any(|prefix| text.starts_with(prefix))
            || text.starts_with('{'))
}

/// True when a line may replace the desktop activity line (task state only).
#[must_use]
pub fn is_activity_relevant(text: &str) -> bool {
    let text = text.trim();
    ACTIVITY_PREFIXES
        .iter()
        .any(|prefix| text.starts_with(prefix))
        || text.starts_with('{')
}

/// True for the summary prefixes that stay readable in the desktop log pane.
#[must_use]
pub fn is_summary_prefix(text: &str) -> bool {
    let text = text.trim();
    SUMMARY_PREFIXES
        .iter()
        .any(|prefix| text.starts_with(prefix))
}

/// True for the multi-field log records the desktop activity line may show.
#[must_use]
pub fn activity_only(text: &str) -> bool {
    text.starts_with("[PCM] ")
        || serde_json::from_str::<Value>(text).is_ok_and(|value| value["Kind"] == "PcmProgress")
}

/// True when a line reports a problem, independent of its source.
#[must_use]
pub fn problem(text: &str) -> bool {
    let text = text.trim();
    if is_metric(text) {
        return false;
    }
    if let Some(event) = task_log::parse(text) {
        return event.warning || event.action == "pcm_failed";
    }
    if text.starts_with('{')
        && let Ok(event) = serde_json::from_str::<Value>(text)
    {
        return match event.get("Kind").and_then(Value::as_str) {
            Some("PcmError") => true,
            Some("Detail") => event
                .get("Value")
                .and_then(Value::as_str)
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
        .any(|prefix| lower.starts_with(prefix))
        || lower
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .any(|word| matches!(word, "error" | "failed" | "fatal" | "cannot"))
        || ["could not", "no such file", "not found"]
            .iter()
            .any(|phrase| lower.contains(phrase))
        || text.contains("失败")
        || text.contains("错误")
}

/// Result of formatting a whole multi-line task log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormattedLog {
    /// Every visible line, already localized and classified.
    pub text: String,
    /// Only the visible problem lines; GUIs use it for their problem filter.
    pub problem_text: String,
    pub problems: u64,
}
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "Keeps the complete five-language action table together"
)]
pub fn task_message(lang: Lang, raw: &str) -> Option<String> {
    let event = task_log::parse(raw)?;
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
        other => {
            // Unknown actions must stay visible: silently dropping them would
            // hide real progress from every GUI.
            let mut unknown = format!("{other} {position}");
            if !n.is_empty() {
                unknown.push_str(" · ");
                unknown.push_str(n);
            }
            if !d.is_empty() {
                unknown.push_str(" · ");
                unknown.push_str(d);
            }
            return Some(unknown);
        }
    };
    Some(if n.is_empty() {
        base
    } else {
        format!("{base} · {n}")
    })
}

fn batch_template(lang: Lang, kind: &str) -> Option<&'static str> {
    match (kind, lang) {
        ("Started", Lang::Zh) => Some("开始处理：{0}"),
        ("Started", Lang::ZhHant) => Some("開始處理：{0}"),
        ("Started", Lang::En) => Some("Processing: {0}"),
        ("Started", Lang::Ja) => Some("処理を開始：{0}"),
        ("Started", Lang::Ko) => Some("처리 중: {0}"),
        ("PcmStarted", Lang::Zh) => Some("正在准备音频：{0}"),
        ("PcmStarted", Lang::ZhHant) => Some("正在準備音訊：{0}"),
        ("PcmStarted", Lang::En) => Some("Preparing audio: {0}"),
        ("PcmStarted", Lang::Ja) => Some("音声を準備中：{0}"),
        ("PcmStarted", Lang::Ko) => Some("오디오 준비 중: {0}"),
        ("PcmProgress", Lang::Zh) => Some("正在准备音频：{0}（{1}%）"),
        ("PcmProgress", Lang::ZhHant) => Some("正在準備音訊：{0}（{1}%）"),
        ("PcmProgress", Lang::En) => Some("Preparing audio: {0} ({1}%)"),
        ("PcmProgress", Lang::Ja) => Some("音声を準備中：{0}（{1}%）"),
        ("PcmProgress", Lang::Ko) => Some("오디오 준비 중: {0} ({1}%)"),
        ("PcmFinished", Lang::Zh) => Some("音频准备完成，开始 MLP 编码：{0}"),
        ("PcmFinished", Lang::ZhHant) => Some("音訊準備完成，開始 MLP 編碼：{0}"),
        ("PcmFinished", Lang::En) => Some("Audio ready. Starting MLP encoding: {0}"),
        ("PcmFinished", Lang::Ja) => Some("音声の準備完了。MLP エンコードを開始：{0}"),
        ("PcmFinished", Lang::Ko) => Some("오디오 준비 완료. MLP 인코딩 시작: {0}"),
        ("Encoded", Lang::Zh) => Some("MLP 编码完成：{0}；{1} Hz / {2} 位 / {3} 声道，{4} 字节"),
        ("Encoded", Lang::ZhHant) => {
            Some("MLP 編碼完成：{0}；{1} Hz / {2} 位 / {3} 聲道，{4} 位元組")
        }
        ("Encoded", Lang::En) => {
            Some("MLP encoding completed: {0}; {1} Hz / {2} bit / {3} channels, {4} bytes")
        }
        ("Encoded", Lang::Ja) => {
            Some("MLP エンコード完了：{0}；{1} Hz / {2} ビット / {3} チャンネル、{4} バイト")
        }
        ("Encoded", Lang::Ko) => {
            Some("MLP 인코딩 완료: {0}; {1} Hz / {2}비트 / {3}채널, {4}바이트")
        }
        ("PcmError", Lang::Zh) => Some("音频转换提示：{0}；{1}"),
        ("PcmError", Lang::ZhHant) => Some("音訊轉換提示：{0}；{1}"),
        ("PcmError", Lang::En) => Some("Audio conversion message: {0}; {1}"),
        ("PcmError", Lang::Ja) => Some("音声変換メッセージ：{0}；{1}"),
        ("PcmError", Lang::Ko) => Some("오디오 변환 메시지: {0}; {1}"),
        _ => None,
    }
}

fn localize_detail_default(language: &str, detail: &str) -> String {
    localization::translate(language, detail)
}

/// Renders a batch (`{"Kind":...}`) media event for the readable log.
///
/// Returns `None` when the line is not a known batch event, so callers keep
/// classifying it. Unknown kinds are classified, never silently dropped.
#[must_use]
pub fn batch_message(language: &str, raw: &str) -> Option<String> {
    batch_message_with(language, raw, localize_detail_default)
}

/// [`batch_message`] with a caller-supplied detail localizer (the desktop shell
/// keeps its richer detail table without duplicating the event templates).
#[must_use]
pub fn batch_message_with(
    language: &str,
    raw: &str,
    localize_detail: fn(&str, &str) -> String,
) -> Option<String> {
    if !raw.starts_with('{') {
        return None;
    }
    let value: Value = serde_json::from_str(raw).ok()?;
    let name = value.get("Name")?.as_str()?;
    let detail = value.get("Value").unwrap_or(&Value::Null);
    let kind = value.get("Kind")?.as_str()?;
    if kind == "Detail" {
        return Some(format!(
            "{name}: {}",
            localize_detail(language, detail.as_str().unwrap_or_default())
        ));
    }
    let lang = Lang::from_code(language);
    let template = batch_template(lang, kind)?;
    let mut result = template.to_owned();
    match kind {
        "Encoded" => {
            for (index, field) in ["Rate", "Bits", "Channels", "Bytes"].iter().enumerate() {
                result = result.replace(
                    &format!("{{{}}}", index + 1),
                    &detail
                        .get(field)
                        .map_or_else(|| "?".into(), Value::to_string),
                );
            }
        }
        "PcmProgress" => {
            result = result.replace(
                "{1}",
                &detail
                    .as_i64()
                    .map_or_else(|| "?".into(), |value| value.clamp(0, 100).to_string()),
            );
        }
        "PcmError" => {
            result = result.replace(
                "{1}",
                &localize_detail(language, detail.as_str().unwrap_or_default()),
            );
        }
        _ => {}
    }
    // Substitute track names last so user data containing braces stays verbatim.
    Some(result.replace("{0}", name))
}

/// Renders one raw log line for the readable task log.
///
/// Returns an empty string when the line is hidden. Unknown `[TASK]` actions are
/// rendered by [`task_message`] instead of disappearing.
#[must_use]
pub fn render_line(language: &str, raw: &str) -> String {
    let text = raw.trim();
    if text.is_empty() || is_metric(text) {
        return String::new();
    }
    if let Some(message) = task_message(Lang::from_code(language), text) {
        return message;
    }
    if let Some(message) = batch_message(language, text) {
        return message;
    }
    if let Some(message) = space_message(Lang::from_code(language), text) {
        return message;
    }
    // A structured event that cannot be parsed (for example a truncated wire
    // line) must never be shown verbatim: hide it instead of leaking raw JSON.
    if text.starts_with('{') || text.starts_with(task_log::PREFIX) {
        return String::new();
    }
    if let Some(index) = text.find(['{', '['])
        && serde_json::from_str::<Value>(&text[index..])
            .is_ok_and(|value| value.is_object() || value.is_array())
    {
        return String::new();
    }
    if is_hidden_noise(text) || is_hidden_summary_header(text) || is_hidden_summary_detail(text) {
        return String::new();
    }
    if problem(text) || is_relevant_line(text) {
        return localized_problem(language, text);
    }
    String::new()
}

/// Formats a whole multi-line log for a GUI and counts problem lines.
#[must_use]
pub fn format_log(language: &str, raw: &str) -> FormattedLog {
    let mut rendered = Vec::new();
    let mut problem_lines = Vec::new();
    let mut problems = 0_u64;
    for line in raw.split('\n') {
        let text = render_line(language, line);
        if text.is_empty() {
            continue;
        }
        if problem(line) {
            problems += 1;
            problem_lines.push(text.clone());
        }
        rendered.push(text);
    }
    FormattedLog {
        text: rendered.join("\n"),
        problem_text: problem_lines.join("\n"),
        problems,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(action: &str, name: &str, detail: &str) -> String {
        task_log::Event {
            action: action.into(),
            current: 2,
            total: 5,
            name: name.into(),
            detail: detail.into(),
            warning: false,
        }
        .pipe(|event| {
            format!(
                "{}{}",
                task_log::PREFIX,
                serde_json::to_string(&event).unwrap()
            )
        })
    }

    trait Pipe: Sized {
        fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
            f(self)
        }
    }
    impl Pipe for task_log::Event {}

    #[test]
    fn known_task_actions_are_localized_for_every_gui_language() {
        let raw = task("author_iso", "", "");
        let expected = [
            ("zh-CN", "打包 ISO，光盘： 2/5"),
            ("zh-TW", "打包 ISO，光碟： 2/5"),
            ("en", "Writing ISO, disc: 2/5"),
            ("ja", "ISO を作成中、ディスク： 2/5"),
            ("ko", "ISO 기록 중, 디스크: 2/5"),
        ];
        for (language, text) in expected {
            assert_eq!(render_line(language, &raw), text, "{language}");
        }
    }

    #[test]
    fn unknown_task_actions_are_rendered_not_dropped() {
        let raw = task("brand_new_stage", "track.flac", "extra detail");
        for language in ["zh-CN", "zh-TW", "en", "ja", "ko", "unknown-code"] {
            let rendered = render_line(language, &raw);
            assert!(
                rendered.contains("brand_new_stage 2/5"),
                "{language}: {rendered}"
            );
            assert!(rendered.contains("track.flac"), "{language}: {rendered}");
            assert!(rendered.contains("extra detail"), "{language}: {rendered}");
        }
    }

    #[test]
    fn truncated_structured_events_never_leak_raw_json() {
        for raw in [
            r#"[TASK] {"Action":"disc_plan","Current":2,"Total":149,"Name":"","Detail":"","Warn"#,
            r#"{"Kind":"Encoded","Name":"track.flac","Value":{"Rate":48000"#,
        ] {
            assert_eq!(render_line("zh-CN", raw), "", "{raw}");
        }
        let formatted = format_log("zh-CN", "out_time_us=1\n{\"Kind\":\"Encoded\"");
        assert_eq!(formatted.text, "");
        assert_eq!(formatted.problems, 0);
    }

    #[test]
    fn decoder_noise_and_metrics_stay_hidden() {
        for hidden in [
            "[METRIC] speed=1",
            "out_time_us=5000000",
            "Number of samples: 48000",
            "[menu-cover] oversized 3",
            "WARN: Button y coordinates are odd for button 1",
            "[debug] noise",
            "[MLP] 临时目录: C:\\tmp",
            "",
        ] {
            assert_eq!(render_line("en", hidden), "", "{hidden}");
        }
    }

    #[test]
    fn structured_payloads_with_a_human_prefix_are_hidden() {
        assert_eq!(
            render_line("zh-CN", "track: {\"Kind\":\"Finalized\",\"Track\":1}"),
            ""
        );
        assert_eq!(render_line("en", "verify: [{\"event\":\"done\"}]"), "");
        assert_eq!(
            render_line("en", "[verify] complete 1 discs 1 tracks ok"),
            "[verify] complete 1 discs 1 tracks ok"
        );
    }

    #[test]
    fn space_checks_render_identically_on_every_gui() {
        let raw = "[space] C:\\: MLP 编码输出, required 1073741824 B, available 2147483648 B";
        for (language, expected) in [
            (
                "zh-CN",
                "空间检查 · C:\\ · MLP 编码输出：预计需要 1.00 GiB，可用 2.00 GiB。",
            ),
            (
                "en",
                "Space check · C:\\ · MLP encoding output: estimated 1.00 GiB, available 2.00 GiB.",
            ),
        ] {
            assert_eq!(render_line(language, raw), expected, "{language}");
        }
        assert_eq!(
            render_line(
                "en",
                "[space] C:\\: author 中间产物与暂存 ISO + 成品 ISO 集合, required 1 B, available unknown"
            ),
            "Space check · C:\\ · Authoring temporary files and finished ISOs: estimated 0.00 GiB, available unknown."
        );
        assert!(!is_diagnostic_noise(raw));
        assert!(space_message(Lang::En, "[space] broken").is_none());
    }
    #[test]
    fn batch_events_use_the_shared_templates() {
        let encoded = r#"{"Kind":"Encoded","Name":"track.flac","Value":{"Rate":48000,"Bits":24,"Channels":2,"Bytes":12345}}"#;
        assert_eq!(
            render_line("en", encoded),
            "MLP encoding completed: track.flac; 48000 Hz / 24 bit / 2 channels, 12345 bytes"
        );
        let progress = r#"{"Kind":"PcmProgress","Name":"track.flac","Value":50}"#;
        assert_eq!(
            render_line("ko", progress),
            "오디오 준비 중: track.flac (50%)"
        );
        let detail = r#"{"Kind":"Detail","Name":"track.flac","Value":"Found 3 audio files"}"#;
        assert!(render_line("en", detail).starts_with("track.flac: "));
    }

    #[test]
    fn batch_detail_localizer_can_be_supplied_by_the_shell() {
        fn upper(_language: &str, detail: &str) -> String {
            detail.to_uppercase()
        }
        let detail = r#"{"Kind":"Detail","Name":"n","Value":"abc"}"#;
        assert_eq!(
            batch_message_with("en", detail, upper).as_deref(),
            Some("n: ABC")
        );
    }

    #[test]
    fn problem_lines_are_detected_from_every_source() {
        assert!(problem("[error] failed to decode"));
        assert!(problem("[警告] something"));
        assert!(problem(r#"{"Kind":"PcmError","Name":"n","Value":"bad"}"#));
        assert!(problem(&task("pcm_failed", "n", "mismatch")));
        assert!(!problem("[info] fine"));
        assert!(!problem("[METRIC] speed=1"));
        assert!(problem("failed to open"));
    }

    #[test]
    fn whole_log_formatting_hides_noise_and_counts_problems() {
        let log = [
            "out_time_us=1",
            "[error] decode failed",
            task("verify_audio_files", "", "").as_str(),
            "[METRIC] speed=2",
            "成品：D:\\out",
        ]
        .join("\n");
        let formatted = format_log("zh-CN", &log);
        assert_eq!(formatted.problems, 1);
        assert!(!formatted.text.contains("out_time_us"));
        assert!(!formatted.text.contains("[METRIC]"));
        assert!(formatted.text.contains("[错误]"));
        assert!(
            formatted.text.contains("开始比对成盘音频与编码文件。"),
            "{}",
            formatted.text
        );
        assert!(formatted.problem_text.contains("[错误]"));
        assert!(!formatted.problem_text.contains("开始比对成盘音频"));
    }

    #[test]
    fn language_codes_fall_back_to_english() {
        assert_eq!(Lang::from_code(""), Lang::En);
        assert_eq!(Lang::from_code("fr"), Lang::En);
        assert_eq!(Lang::from_code("zh-TW"), Lang::ZhHant);
        assert_eq!(Lang::from_code("ja-JP"), Lang::Ja);
    }
}
