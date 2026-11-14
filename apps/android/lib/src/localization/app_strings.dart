/// One of the five UI languages preserved from the classic Android app.
class AppLanguage {
  const AppLanguage(this.code, this.label);

  final String code;
  final String label;

  static const List<AppLanguage> values = <AppLanguage>[
    AppLanguage('zh-CN', '简体中文'),
    AppLanguage('zh-TW', '繁體中文'),
    AppLanguage('en', 'English'),
    AppLanguage('ja', '日本語'),
    AppLanguage('ko', '한국어'),
  ];

  static AppLanguage fromCode(String? code) {
    for (final language in values) {
      if (language.code == code) {
        return language;
      }
    }
    return values.first;
  }
}

/// UI copy for the five supported languages.
///
/// Missing entries fall back to English so a partially translated map never
/// renders a raw key.
class AppStrings {
  const AppStrings._(this._values);

  final Map<String, String> _values;

  static const AppStrings english = AppStrings._(<String, String>{
    'appTitle': 'DVD-Audio Maker',
    'appTagline': 'Author DVD-Audio discs on your phone',
    'tabSetup': 'Setup',
    'tabAudio': 'Audio',
    'tabMenu': 'Menu',
    'tabTask': 'Task',
    'setupSubtitle': 'Sources, output and disc options',
    'audioSubtitle': 'MLP / LPCM encoding and sampling',
    'menuSubtitle': 'Track menu, cover art and video standard',
    'taskSubtitle': 'Progress, logs and disc verification',
    'language': 'Language',
    'openProfile': 'Open profile',
    'saveProfile': 'Save profile',
    'storageAccess': 'Storage access',
    'discardProgress': 'Discard progress',
    'sectionStorage': 'Disc and folders',
    'sectionAdvanced': 'Advanced',
    'sectionEncoding': 'Encoding',
    'sectionDiscMenu': 'Menu and video',
    'sectionStatus': 'Task status',
    'sectionLog': 'Log',
    'sectionActions': 'Actions',
    'sourceDir': 'Source folder',
    'finalDir': 'Output folder',
    'discTitle': 'Disc title',
    'discCapacity': 'Disc capacity',
    'capacityDvd5': 'DVD5 (4.7 GB)',
    'capacityDvd9': 'DVD9 (8.5 GB)',
    'capacityCustom': 'Custom size',
    'customBytes': 'Custom size (bytes)',
    'capacitySummary': 'Capacity {size}',
    'showAdvanced': 'Show advanced settings',
    'moreSettings': 'More settings',
    'advancedSettingsHint':
        'Controls disc grouping, image prefix, menu fonts and other advanced options.',
    'advancedHint':
        'These settings are enough for a first disc. Enable “Show advanced settings” above for work paths, menu fonts and other advanced options.',
    'mlpNote':
        'MLP lossless compression suits most DVD-Audio projects. Sample rate and bit depth follow the selected settings.',
    'lpcmNote':
        'LPCM is faster but uncompressed and uses more space. Check the disc capacity.',
    'workDir': 'Work directory',
    'isoPrefix': 'Image filename prefix',
    'groupTrackLimit': 'Max tracks per group',
    'titleMode': 'Title grouping',
    'titleModeHelp': 'album, one, or a positive integer',
    'albumLimit': 'Album limit (0 = unlimited)',
    'keepTmp': 'Keep temporary files',
    'resumeHint':
        'Interrupted builds can be resumed; you will be asked when a build starts.',
    'browse': 'Browse',
    'audioMode': 'Encoding mode',
    'modeMlp': 'MLP (batch)',
    'modeLpcm': 'LPCM',
    'modeExternal': 'Import MLP',
    'externalDir': 'MLP folder',
    'externalDirHelp': 'Choose the folder containing the MLP files to import.',
    'sampleRate': 'Sample rate',
    'bits': 'Bit depth',
    'dither16': '16-bit output dither',
    'ditherHint': 'Recommended when outputting 16-bit audio.',
    'makeMenu': 'Build track menu',
    'menuStills': 'Show album art in menu',
    'tracksPerPage': 'Tracks per page',
    'coverDim': 'Cover dimming',
    'norm': 'Video standard',
    'normPal': 'PAL',
    'normSecam': 'SECAM',
    'normNtsc': 'NTSC',
    'fontSc': 'Chinese menu font',
    'fontJp': 'Japanese menu font',
    'fontKr': 'Korean menu font',
    'fontHelp': 'Leave blank to use the system font.',
    'actionPrepare': 'Check sources',
    'actionBuild': 'Start build',
    'actionCancel': 'Stop task',
    'actionVerify': 'Verify disc',
    'actionOpenOutput': 'Open folder',
    'actionDiscard': 'Discard progress',
    'actionStatus': 'Status',
    'statusUnfinished': 'An unfinished build is present',
    'statusNoUnfinished': 'No unfinished build',
    'statusReady': 'Ready',
    'statusRunning': 'Running...',
    'statusStopping': 'Stopping...',
    'statusDone': 'Completed',
    'statusFailed': 'Failed',
    'statusCancelled': 'Cancelled',
    'copyLog': 'Copy current view',
    'exportLog': 'Export full log...',
    'logCopied': 'Log copied',
    'logExported': 'Log exported',
    'logExportFailed': 'Could not export log',
    'onlyIssues': 'Problems only',
    'noOutput': 'The output folder does not exist yet.',
    'selectSourceRequired':
        'Select a source folder with the Android file browser first.',
    'selectFinalRequired':
        'Select an output folder with the Android file browser first.',
    'selectExternalRequired':
        'Select the MLP folder with the Android file browser first.',
    'exportFailed':
        'The disc was built, but exporting it to the selected folder failed.',
    'runningConfirm': 'A task is still running. Stop it and exit?',
    'stopAndExit': 'Stop and exit',
    'themeSystem': 'Theme: system',
    'themeLight': 'Theme: light',
    'themeDark': 'Theme: dark',
    'statistics': 'Elapsed {hours}:{minutes}:{seconds} · Notices {problems}',
    'progressLabel': 'Progress',
    'logLabel': 'Log',
    'logEmpty': 'No log yet.',
    'diagnosticsLabel': 'Diagnostics',
    'unfinishedTitle': 'Unfinished build found',
    'unfinishedBody':
        'Resume validates and reuses saved progress. Restart deletes this task intermediate files and starts over. Source audio and finished ISO files are never deleted.',
    'buildContinue': 'Resume',
    'buildRestart': 'Restart',
    'retentionTitle': 'Build unfinished',
    'retentionBody':
        'Keep this progress so you can resume later? Discarding only removes this task intermediate files; source audio and finished ISO files are kept.',
    'keepProgress': 'Keep progress',
    'deleteProgress': 'Delete progress',
    'resultSuccess': 'Done',
    'resultFailure': 'Failed',
    'profileLoaded': 'Profile loaded',
    'profileSaved': 'Profile saved',
    'profileSaveFailed': 'Could not save profile',
    'profileOpenFailed': 'Could not read profile',
    'profileInvalid': 'Invalid profile JSON',
    'storageHint': 'Allow storage access in system settings.',
    'cancel': 'Cancel',
    'tasksIdleHint':
        'Check sources first, then start the build. You can stop it at any time.',
    'runningHint': 'The task runs in the background; the UI stays responsive.',
    'progressSummary': '{done} / {total}',
  });

  static const AppStrings simplifiedChinese = AppStrings._(<String, String>{
    'appTitle': 'DVD-Audio Maker',
    'appTagline': '在手机上制作 DVD-Audio 光盘',
    'tabSetup': '基本设置',
    'tabAudio': '音频编码',
    'tabMenu': '光盘菜单',
    'tabTask': '任务与成品',
    'setupSubtitle': '音源、输出位置与光盘参数',
    'audioSubtitle': 'MLP / LPCM 编码与采样设置',
    'menuSubtitle': '选曲菜单、封面与视频制式',
    'taskSubtitle': '进度、日志与成品验证',
    'language': '语言',
    'openProfile': '打开方案',
    'saveProfile': '保存方案',
    'storageAccess': '存储权限',
    'discardProgress': '删除进度',
    'sectionStorage': '光盘与目录',
    'sectionAdvanced': '高级设置',
    'sectionEncoding': '音频编码',
    'sectionDiscMenu': '菜单与画面',
    'sectionStatus': '任务状态',
    'sectionLog': '运行日志',
    'sectionActions': '操作',
    'sourceDir': '音源文件夹',
    'finalDir': '成品保存位置',
    'discTitle': '光盘名称',
    'discCapacity': '光盘容量',
    'capacityDvd5': 'DVD5 (4.7 GB)',
    'capacityDvd9': 'DVD9 (8.5 GB)',
    'capacityCustom': '自定义容量',
    'customBytes': '自定义容量（字节）',
    'capacitySummary': '容量 {size}',
    'showAdvanced': '显示高级设置',
    'moreSettings': '更多设置',
    'advancedSettingsHint': '控制光盘分组、镜像前缀、菜单字体等进阶选项。',
    'advancedHint': '这些设置已经足够制作第一张光盘。需要光盘分组、菜单字体等高级选项时，请在右上角齿轮中开启“显示高级设置”。',
    'mlpNote': 'MLP 编码会进行无损压缩，适合大多数 DVD-Audio 制作。采样率和位深按右侧设置处理。',
    'lpcmNote': 'LPCM 不压缩音频，速度较快但占用空间更大。请确认光盘容量足够。',
    'workDir': '临时工作目录',
    'isoPrefix': '镜像文件名前缀',
    'groupTrackLimit': '每组最多曲目',
    'titleMode': '标题分组',
    'titleModeHelp': 'album、one 或正整数',
    'albumLimit': '限制专辑数量（0 表示不限）',
    'keepTmp': '保留临时/中间文件',
    'resumeHint': '制作中断后可继续；未完成任务会在开始制作时询问。',
    'browse': '浏览',
    'audioMode': '音频编码方式',
    'modeMlp': 'MLP（批量）',
    'modeLpcm': 'LPCM',
    'modeExternal': '导入 MLP',
    'externalDir': 'MLP 文件目录',
    'externalDirHelp': '选择包含导入 MLP 文件的目录。',
    'sampleRate': '采样率',
    'bits': '位深',
    'dither16': '16-bit 输出抖动',
    'ditherHint': '推荐在 16-bit 输出时启用。',
    'makeMenu': '制作选曲菜单',
    'menuStills': '在菜单中显示专辑封面',
    'tracksPerPage': '每页显示几首',
    'coverDim': '封面暗化程度',
    'norm': '视频制式',
    'fontSc': '中文菜单字体',
    'fontJp': '日文菜单字体',
    'fontKr': '韩文菜单字体',
    'fontHelp': '可留空使用系统默认字体。',
    'actionPrepare': '检查音源',
    'actionBuild': '开始制作',
    'actionCancel': '停止任务',
    'actionVerify': '校验成品',
    'actionOpenOutput': '打开目录',
    'actionDiscard': '删除进度',
    'actionStatus': '状态',
    'statusUnfinished': '存在未完成的制作',
    'statusNoUnfinished': '没有未完成的制作',
    'statusReady': '就绪',
    'statusRunning': '运行中...',
    'statusStopping': '正在停止...',
    'statusDone': '已完成',
    'statusFailed': '失败',
    'statusCancelled': '已取消',
    'copyLog': '复制当前内容',
    'exportLog': '导出详细日志...',
    'logCopied': '已复制日志',
    'logExported': '日志已导出',
    'logExportFailed': '日志导出失败',
    'onlyIssues': '仅显示问题',
    'noOutput': '尚未找到成品路径，请先完成制作。',
    'selectSourceRequired': '请先通过 Android 文件浏览器选择音源文件夹。',
    'selectFinalRequired': '请先通过 Android 文件浏览器选择成品保存文件夹。',
    'selectExternalRequired': '请先通过 Android 文件浏览器选择 MLP 文件夹。',
    'exportFailed': '制作已完成，但导出到所选文件夹失败。',
    'runningConfirm': '任务仍在运行。要停止任务并退出吗？',
    'stopAndExit': '停止并退出',
    'themeSystem': '主题：跟随系统',
    'themeLight': '主题：浅色',
    'themeDark': '主题：深色',
    'statistics': '用时 {hours}:{minutes}:{seconds} · 提醒 {problems}',
    'progressLabel': '进度',
    'logLabel': '运行日志',
    'logEmpty': '尚无日志。',
    'diagnosticsLabel': '诊断信息',
    'unfinishedTitle': '发现未完成的制作',
    'unfinishedBody':
        '继续制作会校验并使用已保存的进度；重新制作会删除该任务的中间文件，从头开始。音源和已输出的 ISO 不会被删除。',
    'buildContinue': '继续制作',
    'buildRestart': '重新制作',
    'retentionTitle': '制作未完成',
    'retentionBody': '是否保留本次进度，以便下次继续制作？删除进度只会删除此任务的中间文件，不会删除音源或已输出的 ISO。',
    'keepProgress': '保留进度',
    'deleteProgress': '删除进度',
    'resultSuccess': '完成',
    'resultFailure': '失败',
    'profileLoaded': '已载入方案',
    'profileSaved': '方案已保存',
    'profileSaveFailed': '方案保存失败',
    'profileOpenFailed': '无法读取方案文件',
    'profileInvalid': '方案 JSON 格式无效',
    'storageHint': '请在系统设置中允许访问存储。',
    'cancel': '取消',
    'tasksIdleHint': '先检查音源，然后开始制作。制作过程可以随时停止。',
    'runningHint': '任务在后台运行，界面保持可响应。',
    'progressSummary': '{done} / {total}',
  });

  static const AppStrings traditionalChinese = AppStrings._(<String, String>{
    'appTitle': 'DVD-Audio Maker',
    'appTagline': '在手機上製作 DVD-Audio 光碟',
    'tabSetup': '基本設定',
    'tabAudio': '音訊編碼',
    'tabMenu': '光碟選單',
    'tabTask': '任務與成品',
    'setupSubtitle': '音源、輸出位置與光碟參數',
    'audioSubtitle': 'MLP / LPCM 編碼與取樣設定',
    'menuSubtitle': '選曲選單、封面與視訊制式',
    'taskSubtitle': '進度、日誌與成品驗證',
    'language': '語言',
    'openProfile': '開啟方案',
    'saveProfile': '儲存方案',
    'storageAccess': '儲存權限',
    'discardProgress': '刪除進度',
    'sectionStorage': '光碟與目錄',
    'sectionAdvanced': '進階設定',
    'sectionEncoding': '音訊編碼',
    'sectionDiscMenu': '選單與畫面',
    'sectionStatus': '任務狀態',
    'sectionLog': '執行日誌',
    'sectionActions': '操作',
    'sourceDir': '音源資料夾',
    'finalDir': '成品儲存位置',
    'discTitle': '光碟名稱',
    'discCapacity': '光碟容量',
    'capacityDvd5': 'DVD5 (4.7 GB)',
    'capacityDvd9': 'DVD9 (8.5 GB)',
    'capacityCustom': '自訂容量',
    'customBytes': '自訂容量（位元組）',
    'capacitySummary': '容量 {size}',
    'showAdvanced': '顯示進階設定',
    'moreSettings': '更多設定',
    'advancedSettingsHint': '控制光碟分組、映像前綴、選單字型等進階選項。',
    'advancedHint': '這些設定已足夠製作第一張光碟。需要光碟分組、選單字型等進階選項時，請在右上角齒輪中開啟「顯示進階設定」。',
    'mlpNote': 'MLP 編碼會進行無損壓縮，適合大多數 DVD-Audio 製作。取樣率和位元深度按所選設定處理。',
    'lpcmNote': 'LPCM 不壓縮音訊，速度較快但占用空間更大。請確認光碟容量足夠。',
    'workDir': '暫存工作目錄',
    'isoPrefix': '映像檔名前綴',
    'groupTrackLimit': '每組最多曲目',
    'titleMode': '標題分組',
    'titleModeHelp': 'album、one 或正整數',
    'albumLimit': '限制專輯數量（0 表示不限）',
    'keepTmp': '保留暫存/中間檔案',
    'resumeHint': '製作中斷後可繼續；未完成任務會在開始製作時詢問。',
    'browse': '瀏覽',
    'audioMode': '音訊編碼方式',
    'modeMlp': 'MLP（批次）',
    'modeLpcm': 'LPCM',
    'modeExternal': '匯入 MLP',
    'externalDir': 'MLP 檔案目錄',
    'externalDirHelp': '選擇包含匯入 MLP 檔案的目錄。',
    'sampleRate': '取樣率',
    'bits': '位元深度',
    'dither16': '16-bit 輸出抖動',
    'ditherHint': '建議在 16-bit 輸出時啟用。',
    'makeMenu': '製作選曲選單',
    'menuStills': '在選單中顯示專輯封面',
    'tracksPerPage': '每頁顯示幾首',
    'coverDim': '封面暗化程度',
    'norm': '視訊制式',
    'fontSc': '中文選單字型',
    'fontJp': '日文選單字型',
    'fontKr': '韓文選單字型',
    'fontHelp': '可留空使用系統預設字型。',
    'actionPrepare': '檢查音源',
    'actionBuild': '開始製作',
    'actionCancel': '停止任務',
    'actionVerify': '校驗成品',
    'actionOpenOutput': '開啟目錄',
    'actionDiscard': '刪除進度',
    'actionStatus': '狀態',
    'statusUnfinished': '存在未完成的製作',
    'statusNoUnfinished': '沒有未完成的製作',
    'statusReady': '就緒',
    'statusRunning': '執行中...',
    'statusStopping': '正在停止...',
    'statusDone': '已完成',
    'statusFailed': '失敗',
    'statusCancelled': '已取消',
    'copyLog': '複製目前內容',
    'exportLog': '匯出詳細日誌...',
    'logCopied': '已複製日誌',
    'logExported': '日誌已匯出',
    'logExportFailed': '日誌匯出失敗',
    'onlyIssues': '僅顯示問題',
    'noOutput': '尚未找到成品路徑，請先完成製作。',
    'selectSourceRequired': '請先透過 Android 檔案瀏覽器選擇音源資料夾。',
    'selectFinalRequired': '請先透過 Android 檔案瀏覽器選擇成品儲存資料夾。',
    'selectExternalRequired': '請先透過 Android 檔案瀏覽器選擇 MLP 資料夾。',
    'exportFailed': '製作已完成，但匯出到所選資料夾失敗。',
    'runningConfirm': '任務仍在執行。要停止任務並退出嗎？',
    'stopAndExit': '停止並退出',
    'themeSystem': '主題：跟隨系統',
    'themeLight': '主題：淺色',
    'themeDark': '主題：深色',
    'statistics': '用時 {hours}:{minutes}:{seconds} · 提醒 {problems}',
    'progressLabel': '進度',
    'logLabel': '執行日誌',
    'logEmpty': '尚無日誌。',
    'diagnosticsLabel': '診斷資訊',
    'unfinishedTitle': '發現未完成的製作',
    'unfinishedBody':
        '繼續製作會驗證並使用已儲存的進度；重新製作會刪除該任務的中間檔案，從頭開始。音源和已輸出的 ISO 不會被刪除。',
    'buildContinue': '繼續製作',
    'buildRestart': '重新製作',
    'retentionTitle': '製作未完成',
    'retentionBody': '是否保留本次進度，以便下次繼續製作？刪除進度只會刪除此任務的中間檔案，不會刪除音源或已輸出的 ISO。',
    'keepProgress': '保留進度',
    'deleteProgress': '刪除進度',
    'resultSuccess': '完成',
    'resultFailure': '失敗',
    'profileLoaded': '已載入方案',
    'profileSaved': '方案已儲存',
    'profileSaveFailed': '方案儲存失敗',
    'profileOpenFailed': '無法讀取方案檔案',
    'profileInvalid': '方案 JSON 格式無效',
    'storageHint': '請在系統設定中允許存取儲存空間。',
    'cancel': '取消',
    'tasksIdleHint': '先檢查音源，然後開始製作。製作過程可以隨時停止。',
    'runningHint': '任務在背景執行，介面保持可回應。',
    'progressSummary': '{done} / {total}',
  });

  static const AppStrings japanese = AppStrings._(<String, String>{
    'appTitle': 'DVD-Audio Maker',
    'appTagline': 'スマートフォンで DVD-Audio を作成',
    'tabSetup': '基本設定',
    'tabAudio': 'オーディオ',
    'tabMenu': 'メニュー',
    'tabTask': 'タスク',
    'setupSubtitle': 'ソース・出力先・ディスク設定',
    'audioSubtitle': 'MLP / LPCM エンコードとサンプリング',
    'menuSubtitle': '選曲メニュー・カバー・映像方式',
    'taskSubtitle': '進捗・ログ・成品検証',
    'language': '言語',
    'openProfile': 'プロファイルを開く',
    'saveProfile': 'プロファイルを保存',
    'storageAccess': 'ストレージ権限',
    'discardProgress': '進捗を削除',
    'sectionStorage': 'ディスクとフォルダー',
    'sectionAdvanced': '詳細設定',
    'sectionEncoding': 'エンコード',
    'sectionDiscMenu': 'メニューと映像',
    'sectionStatus': 'タスク状態',
    'sectionLog': '実行ログ',
    'sectionActions': '操作',
    'sourceDir': 'ソースフォルダー',
    'finalDir': '出力先フォルダー',
    'discTitle': 'ディスク名',
    'discCapacity': 'ディスク容量',
    'capacityDvd5': 'DVD5 (4.7 GB)',
    'capacityDvd9': 'DVD9 (8.5 GB)',
    'capacityCustom': 'カスタム容量',
    'customBytes': 'カスタム容量（バイト）',
    'capacitySummary': '容量 {size}',
    'showAdvanced': '詳細設定を表示',
    'moreSettings': 'その他の設定',
    'advancedSettingsHint': 'ディスクのグループ化、イメージ接頭辞、メニューフォントなどを制御します。',
    'advancedHint':
        'これらの設定だけで最初のディスクを作成できます。作業パスやメニューフォントなどを変更する場合は、上部の「詳細設定を表示」を有効にしてください。',
    'mlpNote':
        'MLP はロスレス圧縮で、ほとんどの DVD-Audio 制作に適しています。サンプルレートとビット深度は選択した設定に従います。',
    'lpcmNote': 'LPCM は高速ですが無圧縮のため容量を多く使用します。ディスク容量を確認してください。',
    'workDir': '作業ディレクトリ',
    'isoPrefix': 'イメージファイル名の接頭辞',
    'groupTrackLimit': 'グループあたりの最大曲数',
    'titleMode': 'タイトルの区切り',
    'titleModeHelp': 'album、one、正の整数',
    'albumLimit': 'アルバム数制限（0 = 無制限）',
    'keepTmp': '一時ファイルを保持',
    'resumeHint': '中断した制作は再開できます。未完了タスクは制作開始時に確認します。',
    'browse': '参照',
    'audioMode': 'エンコード方式',
    'modeMlp': 'MLP（バッチ）',
    'modeLpcm': 'LPCM',
    'modeExternal': 'MLP を取り込む',
    'externalDir': 'MLP フォルダー',
    'externalDirHelp': '取り込む MLP ファイルを含むフォルダーを選択します。',
    'sampleRate': 'サンプルレート',
    'bits': 'ビット深度',
    'dither16': '16-bit 出力ディザ',
    'ditherHint': '16-bit 出力時に有効にすることをおすすめします。',
    'makeMenu': '選曲メニューを作成',
    'menuStills': 'メニューにアルバムアートを表示',
    'tracksPerPage': '1 ページの曲数',
    'coverDim': 'カバーの暗さ',
    'norm': '映像方式',
    'fontSc': '中国語メニューフォント',
    'fontJp': '日本語メニューフォント',
    'fontKr': '韓国語メニューフォント',
    'fontHelp': '空欄の場合はシステムフォントを使用します。',
    'actionPrepare': 'ソースを確認',
    'actionBuild': '制作を開始',
    'actionCancel': 'タスクを停止',
    'actionVerify': '成品を検証',
    'actionOpenOutput': '成品を開く',
    'actionDiscard': '進捗を削除',
    'actionStatus': '状態',
    'statusUnfinished': '未完了の制作があります',
    'statusNoUnfinished': '未完了の制作はありません',
    'statusReady': '準備完了',
    'statusRunning': '実行中...',
    'statusStopping': '停止中...',
    'statusDone': '完了',
    'statusFailed': '失敗',
    'statusCancelled': 'キャンセル',
    'copyLog': '表示をコピー',
    'exportLog': 'ログを保存...',
    'logCopied': 'ログをコピーしました',
    'logExported': 'ログを保存しました',
    'logExportFailed': 'ログを保存できませんでした',
    'onlyIssues': '問題のみ表示',
    'noOutput': '出力フォルダーがまだありません。',
    'selectSourceRequired': 'まず Android のファイルブラウザーでソースフォルダーを選択してください。',
    'selectFinalRequired': 'まず Android のファイルブラウザーで出力フォルダーを選択してください。',
    'selectExternalRequired': 'まず Android のファイルブラウザーで MLP フォルダーを選択してください。',
    'exportFailed': '制作は完了しましたが、選択したフォルダーへのエクスポートに失敗しました。',
    'runningConfirm': 'タスクが実行中です。停止して終了しますか？',
    'stopAndExit': '停止して終了',
    'themeSystem': 'テーマ：システム',
    'themeLight': 'テーマ：ライト',
    'themeDark': 'テーマ：ダーク',
    'statistics': '経過時間 {hours}:{minutes}:{seconds} · 注意 {problems}',
    'progressLabel': '進捗',
    'logLabel': '実行ログ',
    'logEmpty': 'ログはまだありません。',
    'diagnosticsLabel': '診断情報',
    'unfinishedTitle': '未完了の制作があります',
    'unfinishedBody':
        '制作を再開すると保存済みの進捗を検証して使用します。作り直すとこのタスクの中間ファイルを削除して最初からやり直します。音源と出力済み ISO は削除されません。',
    'buildContinue': '制作を再開',
    'buildRestart': '作り直す',
    'retentionTitle': '制作が未完了です',
    'retentionBody':
        '今回の進捗を保持して後で再開しますか？削除するとこのタスクの中間ファイルのみ削除され、音源と出力済み ISO は残ります。',
    'keepProgress': '進捗を保持',
    'deleteProgress': '進捗を削除',
    'resultSuccess': '完了',
    'resultFailure': '失敗',
    'profileLoaded': 'プロファイルを読み込みました',
    'profileSaved': 'プロファイルを保存しました',
    'profileSaveFailed': 'プロファイルを保存できませんでした',
    'profileOpenFailed': 'プロファイルを読み込めません',
    'profileInvalid': 'プロファイル JSON が無効です',
    'storageHint': 'システム設定でストレージへのアクセスを許可してください。',
    'cancel': 'キャンセル',
    'tasksIdleHint': 'まず音源を確認し、制作を開始します。いつでも停止できます。',
    'runningHint': 'タスクはバックグラウンドで実行され、UI は応答し続けます。',
    'progressSummary': '{done} / {total}',
  });

  static const AppStrings korean = AppStrings._(<String, String>{
    'appTitle': 'DVD-Audio Maker',
    'appTagline': '휴대폰에서 DVD-Audio 디스크 제작',
    'tabSetup': '기본 설정',
    'tabAudio': '오디오',
    'tabMenu': '메뉴',
    'tabTask': '작업',
    'setupSubtitle': '소스, 출력 위치 및 디스크 설정',
    'audioSubtitle': 'MLP / LPCM 인코딩 및 샘플링',
    'menuSubtitle': '트랙 메뉴, 커버 및 비디오 규격',
    'taskSubtitle': '진행률, 로그 및 결과 검증',
    'language': '언어',
    'openProfile': '프로필 열기',
    'saveProfile': '프로필 저장',
    'storageAccess': '저장소 권한',
    'discardProgress': '진행 삭제',
    'sectionStorage': '디스크 및 폴더',
    'sectionAdvanced': '고급 설정',
    'sectionEncoding': '인코딩',
    'sectionDiscMenu': '메뉴 및 화면',
    'sectionStatus': '작업 상태',
    'sectionLog': '실행 로그',
    'sectionActions': '작업',
    'sourceDir': '소스 폴더',
    'finalDir': '출력 폴더',
    'discTitle': '디스크 이름',
    'discCapacity': '디스크 용량',
    'capacityDvd5': 'DVD5 (4.7 GB)',
    'capacityDvd9': 'DVD9 (8.5 GB)',
    'capacityCustom': '사용자 지정 용량',
    'customBytes': '사용자 지정 용량(바이트)',
    'capacitySummary': '용량 {size}',
    'showAdvanced': '고급 설정 표시',
    'moreSettings': '추가 설정',
    'advancedSettingsHint': '디스크 그룹, 이미지 접두사, 메뉴 글꼴 등 고급 옵션을 제어합니다.',
    'advancedHint':
        '이 설정만으로 첫 디스크를 만들 수 있습니다. 작업 경로나 메뉴 글꼴 등을 변경하려면 위의 “고급 설정 표시”를 켜세요.',
    'mlpNote':
        'MLP 무손실 압축은 대부분의 DVD-Audio 제작에 적합합니다. 샘플 레이트와 비트 심도는 선택한 설정을 따릅니다.',
    'lpcmNote': 'LPCM은 빠르지만 압축되지 않아 더 많은 공간을 사용합니다. 디스크 용량을 확인하세요.',
    'workDir': '작업 디렉터리',
    'isoPrefix': '이미지 파일 이름 접두사',
    'groupTrackLimit': '그룹당 최대 트랙 수',
    'titleMode': '타이틀 그룹',
    'titleModeHelp': 'album, one 또는 양의 정수',
    'albumLimit': '앨범 수 제한(0 = 무제한)',
    'keepTmp': '임시 파일 유지',
    'resumeHint': '중단된 빌드는 이어서 할 수 있으며, 빌드 시작 시 확인합니다.',
    'browse': '찾아보기',
    'audioMode': '인코딩 방식',
    'modeMlp': 'MLP(배치)',
    'modeLpcm': 'LPCM',
    'modeExternal': 'MLP 가져오기',
    'externalDir': 'MLP 폴더',
    'externalDirHelp': '가져올 MLP 파일이 있는 폴더를 선택하세요.',
    'sampleRate': '샘플 레이트',
    'bits': '비트 심도',
    'dither16': '16-bit 출력 디더',
    'ditherHint': '16-bit 출력 시 사용을 권장합니다.',
    'makeMenu': '트랙 메뉴 만들기',
    'menuStills': '메뉴에 앨범 아트 표시',
    'tracksPerPage': '페이지당 트랙 수',
    'coverDim': '커버 어둡기',
    'norm': '비디오 규격',
    'fontSc': '중국어 메뉴 글꼴',
    'fontJp': '일본어 메뉴 글꼴',
    'fontKr': '한국어 메뉴 글꼴',
    'fontHelp': '비워 두면 시스템 글꼴을 사용합니다.',
    'actionPrepare': '소스 확인',
    'actionBuild': '빌드 시작',
    'actionCancel': '작업 중지',
    'actionVerify': '결과 검증',
    'actionOpenOutput': '결과 열기',
    'actionDiscard': '진행 삭제',
    'actionStatus': '상태',
    'statusUnfinished': '미완료 빌드가 있습니다',
    'statusNoUnfinished': '미완료 빌드가 없습니다',
    'statusReady': '준비됨',
    'statusRunning': '실행 중...',
    'statusStopping': '중지 중...',
    'statusDone': '완료',
    'statusFailed': '실패',
    'statusCancelled': '취소됨',
    'copyLog': '현재 보기 복사',
    'exportLog': '전체 로그 남겨두기...',
    'logCopied': '로그를 복사했습니다',
    'logExported': '로그를 남겼습니다',
    'logExportFailed': '로그를 남기지 못했습니다',
    'onlyIssues': '문제만 표시',
    'noOutput': '출력 폴더가 아직 없습니다.',
    'selectSourceRequired': '먼저 Android 파일 브라우저에서 소스 폴더를 선택하세요.',
    'selectFinalRequired': '먼저 Android 파일 브라우저에서 출력 폴더를 선택하세요.',
    'selectExternalRequired': '먼저 Android 파일 브라우저에서 MLP 폴더를 선택하세요.',
    'exportFailed': '빌드는 완료되었지만 선택한 폴더로 남기지 못했습니다.',
    'runningConfirm': '작업이 진행 중입니다. 중지하고 종료할까요?',
    'stopAndExit': '중지하고 종료',
    'themeSystem': '테마: 시스템',
    'themeLight': '테마: 라이트',
    'themeDark': '테마: 다크',
    'statistics': '경과 시간 {hours}:{minutes}:{seconds} · 알림 {problems}',
    'progressLabel': '진행률',
    'logLabel': '실행 로그',
    'logEmpty': '아직 로그가 없습니다.',
    'diagnosticsLabel': '진단 정보',
    'unfinishedTitle': '미완료 빌드 발견',
    'unfinishedBody':
        '이어서 빌드하면 저장된 진행 상황을 검증해 사용합니다. 다시 만들면 이 작업의 중간 파일을 삭제하고 처음부터 시작합니다. 소스와 출력된 ISO는 삭제되지 않습니다.',
    'buildContinue': '이어서 빌드',
    'buildRestart': '다시 빌드',
    'retentionTitle': '빌드 미완료',
    'retentionBody':
        '이번 진행 상황을 저장해 나중에 이어서 할까요? 삭제하면 이 작업의 중간 파일만 제거되며 소스와 출력된 ISO는 유지됩니다.',
    'keepProgress': '진행 유지',
    'deleteProgress': '진행 삭제',
    'resultSuccess': '완료',
    'resultFailure': '실패',
    'profileLoaded': '프로필을 불러왔습니다',
    'profileSaved': '프로필을 저장했습니다',
    'profileSaveFailed': '프로필을 저장하지 못했습니다',
    'profileOpenFailed': '프로필을 읽을 수 없습니다',
    'profileInvalid': '프로필 JSON이 올바르지 않습니다',
    'storageHint': '시스템 설정에서 저장소 접근을 허용하세요.',
    'cancel': '취소',
    'tasksIdleHint': '먼저 소스를 확인한 뒤 빌드를 시작하세요. 언제든지 중지할 수 있습니다.',
    'runningHint': '작업은 백그라운드에서 실행되며 UI는 계속 응답합니다.',
    'progressSummary': '{done} / {total}',
  });

  static AppStrings forCode(String code) {
    switch (code) {
      case 'zh-TW':
        return traditionalChinese;
      case 'en':
        return english;
      case 'ja':
        return japanese;
      case 'ko':
        return korean;
      default:
        return simplifiedChinese;
    }
  }

  String _text(String key) => _values[key] ?? english._values[key] ?? key;

  String capacitySummary(String size) =>
      _text('capacitySummary').replaceAll('{size}', size);

  String progressSummary(String done, String total) => _text(
    'progressSummary',
  ).replaceAll('{done}', done).replaceAll('{total}', total);

  String statistics(int seconds, int problems) {
    final hours = (seconds ~/ 3600).toString();
    final minutes = ((seconds ~/ 60) % 60).toString().padLeft(2, '0');
    final remainder = (seconds % 60).toString().padLeft(2, '0');
    return _text('statistics')
        .replaceAll('{hours}', hours)
        .replaceAll('{minutes}', minutes)
        .replaceAll('{seconds}', remainder)
        .replaceAll('{problems}', problems.toString());
  }

  String get appTitle => _text('appTitle');
  String get appTagline => _text('appTagline');
  String get tabSetup => _text('tabSetup');
  String get tabAudio => _text('tabAudio');
  String get tabMenu => _text('tabMenu');
  String get tabTask => _text('tabTask');
  String get setupSubtitle => _text('setupSubtitle');
  String get audioSubtitle => _text('audioSubtitle');
  String get menuSubtitle => _text('menuSubtitle');
  String get taskSubtitle => _text('taskSubtitle');
  String get language => _text('language');
  String get openProfile => _text('openProfile');
  String get saveProfile => _text('saveProfile');
  String get storageAccess => _text('storageAccess');
  String get discardProgress => _text('discardProgress');
  String get sectionStorage => _text('sectionStorage');
  String get sectionAdvanced => _text('sectionAdvanced');
  String get sectionEncoding => _text('sectionEncoding');
  String get sectionDiscMenu => _text('sectionDiscMenu');
  String get sectionStatus => _text('sectionStatus');
  String get sectionLog => _text('sectionLog');
  String get sectionActions => _text('sectionActions');
  String get sourceDir => _text('sourceDir');
  String get finalDir => _text('finalDir');
  String get discTitle => _text('discTitle');
  String get discCapacity => _text('discCapacity');
  String get capacityDvd5 => _text('capacityDvd5');
  String get capacityDvd9 => _text('capacityDvd9');
  String get capacityCustom => _text('capacityCustom');
  String get customBytes => _text('customBytes');
  String get showAdvanced => _text('showAdvanced');
  String get moreSettings => _text('moreSettings');
  String get advancedSettingsHint => _text('advancedSettingsHint');
  String get advancedHint => _text('advancedHint');
  String get mlpNote => _text('mlpNote');
  String get lpcmNote => _text('lpcmNote');
  String get workDir => _text('workDir');
  String get isoPrefix => _text('isoPrefix');
  String get groupTrackLimit => _text('groupTrackLimit');
  String get titleMode => _text('titleMode');
  String get titleModeHelp => _text('titleModeHelp');
  String get albumLimit => _text('albumLimit');
  String get keepTmp => _text('keepTmp');
  String get resumeHint => _text('resumeHint');
  String get browse => _text('browse');
  String get audioMode => _text('audioMode');
  String get modeMlp => _text('modeMlp');
  String get modeLpcm => _text('modeLpcm');
  String get modeExternal => _text('modeExternal');
  String get externalDir => _text('externalDir');
  String get externalDirHelp => _text('externalDirHelp');
  String get sampleRate => _text('sampleRate');
  String get bits => _text('bits');
  String get dither16 => _text('dither16');
  String get ditherHint => _text('ditherHint');
  String get makeMenu => _text('makeMenu');
  String get menuStills => _text('menuStills');
  String get tracksPerPage => _text('tracksPerPage');
  String get coverDim => _text('coverDim');
  String get norm => _text('norm');
  String get normPal => _text('normPal');
  String get normSecam => _text('normSecam');
  String get normNtsc => _text('normNtsc');
  String get fontSc => _text('fontSc');
  String get fontJp => _text('fontJp');
  String get fontKr => _text('fontKr');
  String get fontHelp => _text('fontHelp');
  String get actionPrepare => _text('actionPrepare');
  String get actionBuild => _text('actionBuild');
  String get actionCancel => _text('actionCancel');
  String get actionVerify => _text('actionVerify');
  String get actionOpenOutput => _text('actionOpenOutput');
  String get actionDiscard => _text('actionDiscard');
  String get actionStatus => _text('actionStatus');
  String get statusUnfinished => _text('statusUnfinished');
  String get statusNoUnfinished => _text('statusNoUnfinished');
  String get statusReady => _text('statusReady');
  String get statusRunning => _text('statusRunning');
  String get statusStopping => _text('statusStopping');
  String get statusDone => _text('statusDone');
  String get statusFailed => _text('statusFailed');
  String get statusCancelled => _text('statusCancelled');
  String get copyLog => _text('copyLog');
  String get exportLog => _text('exportLog');
  String get logCopied => _text('logCopied');
  String get logExported => _text('logExported');
  String get logExportFailed => _text('logExportFailed');
  String get onlyIssues => _text('onlyIssues');
  String get noOutput => _text('noOutput');
  String get selectSourceRequired => _text('selectSourceRequired');
  String get selectFinalRequired => _text('selectFinalRequired');
  String get selectExternalRequired => _text('selectExternalRequired');
  String get exportFailed => _text('exportFailed');
  String get runningConfirm => _text('runningConfirm');
  String get stopAndExit => _text('stopAndExit');
  String get themeSystem => _text('themeSystem');
  String get themeLight => _text('themeLight');
  String get themeDark => _text('themeDark');
  String get progressLabel => _text('progressLabel');
  String get logLabel => _text('logLabel');
  String get logEmpty => _text('logEmpty');
  String get diagnosticsLabel => _text('diagnosticsLabel');
  String get unfinishedTitle => _text('unfinishedTitle');
  String get unfinishedBody => _text('unfinishedBody');
  String get buildContinue => _text('buildContinue');
  String get buildRestart => _text('buildRestart');
  String get retentionTitle => _text('retentionTitle');
  String get retentionBody => _text('retentionBody');
  String get keepProgress => _text('keepProgress');
  String get deleteProgress => _text('deleteProgress');
  String get resultSuccess => _text('resultSuccess');
  String get resultFailure => _text('resultFailure');
  String get profileLoaded => _text('profileLoaded');
  String get profileSaved => _text('profileSaved');
  String get profileSaveFailed => _text('profileSaveFailed');
  String get profileOpenFailed => _text('profileOpenFailed');
  String get profileInvalid => _text('profileInvalid');
  String get storageHint => _text('storageHint');
  String get cancel => _text('cancel');
  String get tasksIdleHint => _text('tasksIdleHint');
  String get runningHint => _text('runningHint');
}
