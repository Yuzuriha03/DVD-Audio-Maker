/// Localized rendering for [TASK] workflow events.
///
/// Mirrors the Windows client: structured task events are localized and raw
/// decoder output is not shown, so both platforms read the same way.
library;

const Map<String, Map<String, String>> _labels = <String, Map<String, String>>{
  'source_metadata': <String, String>{
    'zh-CN': '读取曲目信息：',
    'zh-TW': '讀取曲目資訊：',
    'en': 'Reading track information:',
    'ja': '曲情報を読み込み中：',
    'ko': '트랙 정보 읽는 중:',
  },
  'source_decode': <String, String>{
    'zh-CN': '检查音源解码：',
    'zh-TW': '檢查音源解碼：',
    'en': 'Checking source decoding:',
    'ja': '音源のデコードを確認中：',
    'ko': '음원 디코딩 확인 중:',
  },
  'source_result': <String, String>{
    'zh-CN': '整理音源检查结果：',
    'zh-TW': '整理音源檢查結果：',
    'en': 'Processing source check results:',
    'ja': '音源の確認結果を整理中：',
    'ko': '음원 검사 결과 정리 중:',
  },
  'author_disc': <String, String>{
    'zh-CN': '制作光盘：',
    'zh-TW': '制作光碟：',
    'en': 'Creating disc:',
    'ja': 'ディスク作成中：',
    'ko': '디스크 제작 중:',
  },
  'author_audio': <String, String>{
    'zh-CN': '读取光盘音频：',
    'zh-TW': '讀取光碟音訊：',
    'en': 'Reading disc audio:',
    'ja': 'ディスク音声を読み込み中：',
    'ko': '디스크 오디오 읽는 중:',
  },
  'author_layout': <String, String>{
    'zh-CN': '分析音频布局，光盘：',
    'zh-TW': '分析音訊布局，光碟：',
    'en': 'Analyzing audio layout, disc:',
    'ja': '音声配置を解析中、ディスク：',
    'ko': '오디오 배치 분석 중, 디스크:',
  },
  'author_navigation': <String, String>{
    'zh-CN': '制作光盘导航：',
    'zh-TW': '制作光碟導航：',
    'en': 'Creating disc navigation:',
    'ja': 'ディスクナビゲーションを作成中：',
    'ko': '디스크 탐색 정보 만드는 중:',
  },
  'author_stills': <String, String>{
    'zh-CN': '整理播放封面，光盘：',
    'zh-TW': '整理播放封面，光碟：',
    'en': 'Preparing playback covers, disc:',
    'ja': '再生用カバーを準備中、ディスク：',
    'ko': '재생 표지 준비 중, 디스크:',
  },
  'author_iso': <String, String>{
    'zh-CN': '打包 ISO，光盘：',
    'zh-TW': '打包 ISO，光碟：',
    'en': 'Writing ISO, disc:',
    'ja': 'ISO を作成中、ディスク：',
    'ko': 'ISO 기록 중, 디스크:',
  },
  'author_check_menu': <String, String>{
    'zh-CN': '检查已制作的菜单，光盘：',
    'zh-TW': '檢查已制作的選單，光碟：',
    'en': 'Checking authored menus, disc:',
    'ja': '作成済みメニューを確認中、ディスク：',
    'ko': '제작된 메뉴 확인 중, 디스크:',
  },
  'verify_timeline_group': <String, String>{
    'zh-CN': '检查音频时间线，音轨组：',
    'zh-TW': '檢查音訊時間線，音軌組：',
    'en': 'Checking audio timeline, group:',
    'ja': '音声タイムラインを確認中、グループ：',
    'ko': '오디오 타임라인 확인 중, 그룹:',
  },
  'verify_menu_disc': <String, String>{
    'zh-CN': '验证菜单，光盘：',
    'zh-TW': '驗證選單，光碟：',
    'en': 'Verifying menus, disc:',
    'ja': 'メニューを検証中、ディスク：',
    'ko': '메뉴 검증 중, 디스크:',
  },
  'verify_menu_page': <String, String>{
    'zh-CN': '验证菜单页：',
    'zh-TW': '驗證選單頁：',
    'en': 'Verifying menu page:',
    'ja': 'メニューページを検証中：',
    'ko': '메뉴 페이지 검증 중:',
  },
  'verify_audio_group': <String, String>{
    'zh-CN': '比对成盘音频，音轨组：',
    'zh-TW': '比對成盤音訊，音軌組：',
    'en': 'Comparing authored audio, group:',
    'ja': 'ディスク内音声を比較中、グループ：',
    'ko': '제작된 오디오 비교 중, 그룹:',
  },
  'source_checked': <String, String>{
    'zh-CN': '音源检查完成，曲目数：',
    'zh-TW': '音源檢查完成，曲目數：',
    'en': 'Source checks completed; tracks:',
    'ja': '音源の確認完了、曲数：',
    'ko': '음원 검사 완료; 트랙 수:',
  },
  'publish_start': <String, String>{
    'zh-CN': '保存成品 ISO 到：',
    'zh-TW': '儲存成品 ISO 到：',
    'en': 'Saving finished ISOs to:',
    'ja': '完成 ISO の保存先：',
    'ko': '완성된 ISO 저장 위치:',
  },
  'published_iso': <String, String>{
    'zh-CN': 'ISO 已保存：',
    'zh-TW': 'ISO 已儲存：',
    'en': 'ISO saved:',
    'ja': 'ISO を保存しました：',
    'ko': 'ISO 저장됨:',
  },
  'published_directory': <String, String>{
    'zh-CN': '成品目录：',
    'zh-TW': '成品目錄：',
    'en': 'Output folder:',
    'ja': '出力フォルダー：',
    'ko': '출력 폴더:',
  },
  'pcm_ok': <String, String>{
    'zh-CN': '已校验：',
    'zh-TW': '已校驗：',
    'en': 'Verified:',
    'ja': '検証済み：',
    'ko': '검증됨:',
  },
  'pcm_failed': <String, String>{
    'zh-CN': '已校验：',
    'zh-TW': '已校驗：',
    'en': 'Verified:',
    'ja': '検証済み：',
    'ko': '검증됨:',
  },
};

const Map<String, Map<String, String>>
_sentences = <String, Map<String, String>>{
  'disc_plan': <String, String>{
    'zh-CN': '分盘规划完成：{c} 张光盘，共 {t} 首音轨。',
    'zh-TW': '分碟規劃完成：{c} 張光碟，共 {t} 首音軌。',
    'en': 'Disc plan ready: {c} discs, {t} tracks.',
    'ja': 'ディスク構成を作成しました：{c} 枚、計 {t} 曲。',
    'ko': '디스크 구성 완료: 디스크 {c}개, 총 {t}곡.',
  },
  'verify_timeline': <String, String>{
    'zh-CN': '开始检查光盘音频时间线。',
    'zh-TW': '開始檢查光碟音訊時間線。',
    'en': 'Checking disc audio timelines.',
    'ja': 'ディスク音声のタイムラインを確認します。',
    'ko': '디스크 오디오 타임라인을 확인합니다.',
  },
  'verify_audio_files': <String, String>{
    'zh-CN': '开始比对成盘音频与编码文件。',
    'zh-TW': '開始比對成盤音訊與編碼檔案。',
    'en': 'Comparing authored audio with encoded files.',
    'ja': 'ディスク内音声とエンコード済みファイルを比較します。',
    'ko': '제작된 오디오와 인코딩 파일을 비교합니다.',
  },
  'verify_pcm_start': <String, String>{
    'zh-CN': '开始完整音频对照：0/{t} 首，自动 {c} 路并行。',
    'zh-TW': '開始完整音訊對照：0/{t} 首，自動 {c} 路并行。',
    'en':
        'Starting full audio comparison: 0/{t} tracks, {c} automatic workers.',
    'ja': '全音声の比較を開始：0/{t} 曲、自動 {c} 並列。',
    'ko': '전체 오디오 비교 시작: 0/{t}곡, 자동 병렬 작업 {c}개.',
  },
};

const Map<String, Map<String, String>> _states = <String, Map<String, String>>{
  'match': <String, String>{
    'zh-CN': '一致',
    'zh-TW': '一致',
    'en': 'match',
    'ja': '一致',
    'ko': '일치',
  },
  'mismatch': <String, String>{
    'zh-CN': '不一致',
    'zh-TW': '不一致',
    'en': 'mismatch',
    'ja': '不一致',
    'ko': '불일치',
  },
};

String _pick(
  Map<String, Map<String, String>>? table,
  String key,
  String language,
) {
  final entry = table?[key];
  if (entry == null) {
    return key;
  }
  return entry[language] ?? entry['en'] ?? key;
}

/// Returns the readable text for one \`[TASK] \` line, or null when the event
/// only exists to drive progress and must not be shown verbatim.
String? formatTaskEvent(Map<String, Object?> event, String language) {
  final action = event['Action'];
  if (action is! String) {
    return null;
  }
  final current = event['Current'];
  final total = event['Total'];
  final name = event['Name'];
  final detail = event['Detail'];
  final position = '${current ?? 0}/${total ?? 0}';
  final suffix = name is String && name.isNotEmpty ? ' · $name' : '';
  final sentence = _sentences[action];
  if (sentence != null) {
    final template = _pick(_sentences, action, language);
    return template
        .replaceAll('{c}', '${current ?? 0}')
        .replaceAll('{t}', '${total ?? 0}');
  }
  switch (action) {
    case 'source_checked':
      return '${_pick(_labels, action, language)} ${current ?? 0}';
    case 'publish_start':
    case 'published_directory':
      return '${_pick(_labels, action, language)} ${detail ?? ''}';
    case 'published_iso':
      return '${_pick(_labels, action, language)} $position$suffix';
    case 'pcm_ok':
      return '${_pick(_labels, action, language)} $position$suffix · ${_pick(_states, 'match', language)}';
    case 'pcm_failed':
      final reason = detail is String && detail.isNotEmpty ? ' · $detail' : '';
      return '${_pick(_labels, action, language)} $position$suffix · ${_pick(_states, 'mismatch', language)}$reason';
    case 'oversized_summary':
    case 'button_coordinates':
    case 'button_coordinates_seen':
      return null;
    default:
      final label = _labels[action];
      if (label == null) {
        return null;
      }
      return '${_pick(_labels, action, language)} $position$suffix';
  }
}
