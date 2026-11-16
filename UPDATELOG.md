# Update log

## v1.2

### 中文

**Android 版（新增 · ARM64 APK）**

- 新增 Android 手机和平板应用（Android 7.0 及以上、ARM64）。音源检查、MLP/LPCM 编码、选曲菜单与专辑索引、自动分盘、成品验证沿用与 Windows 相同的制作流程和默认值。
- 通过系统文件选择器授权音源目录和成品目录，不申请“所有文件访问”权限；音源在应用私有空间内处理，完成后只导出本次制作的成品 ISO 到你选择的目录，其他文件保持不动。
- 支持简体中文、繁体中文、英文、日文、韩文五种界面语言与明/暗主题；常用设置放在主界面右上角，光盘分组、菜单字体等选项收在“高级设置”中。
- 支持保存/打开 JSON 方案和断点续制：失败或取消后可选择继续未完成的制作或重新制作，也可保留或删除进度；制作成功并通过验证后会清理中间文件。
- 导出与验证采用与桌面版相同的成功/失败/取消规则：导出先写临时文件并校验后再替换旧成品，失败时保留原有 ISO 和可恢复的未完成任务，不会把未完成结果报告为成功；导出、验证和“停止任务”都会及时响应取消。
- 构建期间在通知栏显示进度；Android 16 及以上使用系统 Live Updates 进度样式，较旧系统使用普通进度通知。Android 13 及以上会在需要时请求通知权限，拒绝权限不影响应用内进度显示。
- 支持 LTPO 自适应刷新率：应用向系统声明屏幕峰值刷新率，由系统按场景无缝调整。
- Android 的 MLP 编码使用与 Windows 相同的编码实现，不再走平台专用的慢速回退路径。
- 日志支持“仅显示问题”、复制和通过系统选择器导出；状态区显示运行时长与问题数量，日志文案与 Windows 版一致。
- “关于”页提供作者、项目主页、应用信息和随包许可证（项目许可证、FFmpeg、第三方说明、图像/菜单声明），Flutter 依赖许可也可在应用内查看。
- 已修复：首次选择文件夹后可能丢失选择、重新选择源目录后仍使用旧内容、Android 上“检查音源”因系统目录权限失败且日志为空、删除进度无效等问题。现在任务开始前会校验并同步所选目录，日志会先记录实际使用的路径。
- 使用注意：需要 Android 7.0 及以上的 ARM64 设备；请通过系统选择器授权需要访问的目录并保持授权有效；导出较大的 ISO 前请确认目标目录空间充足。设置页的“删除进度”会清理本应用的缓存和未完成任务，但不删除你授权的目录或已导出的 ISO。当前 APK 为本地安装签名，未上架应用商店。

**Windows 版（x64 与 WoA · 单 EXE）**

- Windows 发布改为单 EXE：下载对应架构的 EXE 后直接运行，无需解压。许可证、FFmpeg LGPL 与第三方/图像/菜单声明已内嵌到“关于”窗口，不再需要随包保存外部许可文件。
- “关于”入口移到标题栏右上角统一按钮组，图标的尺寸、悬停、焦点和主题行为均与相邻按钮一致；关于窗口改为可滚动的独立页面，可查看作者、项目主页、版本和全部随包许可证，窗口支持拖动位置及左右、上下和四角缩放，滚动条只在内容溢出时显示，切换语言/主题后内容立即刷新，重复打开会复用同一个窗口。
- 应用内提示、确认和选择弹窗统一为与主窗口一致的标题栏和关闭按钮，长按钮行可自动换行；键盘操作支持 Enter 触发默认按钮、Esc 取消，键盘聚焦时显示焦点框。
- 菜单设置更清晰：菜单开关同时控制导航菜单的生成；关闭菜单后会隐藏每页曲目数、菜单视频制式、封面暗化和菜单字体等仅与菜单有关的设置。标签明确区分“菜单背景封面暗化”和“播放曲目时显示专辑封面”。
- 音频设置中切换到 LPCM 时目标位深只提供 16 位和 24 位；原有的 20 位 LPCM 设置会回落到 24 位，20 位请使用 MLP。x64 与 WoA 均以单 EXE 发布，既有制作行为保持不变。

**共同说明**

- Android 与 Windows 使用相同的 profile 键、默认值、菜单/分盘规则、日志事件和进度阶段；界面交互按各自平台习惯呈现。
- 制作完成后建议先执行成品验证，再交由支持 DVD-Audio 的播放器或刻录软件使用。普通 DVD-Video 播放器不保证兼容。

### English

**Android edition (new · ARM64 APK)**

- New Android phone and tablet app (Android 7.0+ on ARM64). Source checks, MLP/LPCM encoding, track menus and album indexes, automatic disc splitting and output verification follow the same authoring workflow and defaults as Windows.
- Choose source and output folders through the system file picker; the app does not request "all files access". Sources are processed in app-private storage, and only the finished ISOs from this task are exported to the folder you chose; other files in that folder are left untouched.
- Five UI languages (Simplified Chinese, Traditional Chinese, English, Japanese, Korean) and light/dark themes. Common settings are at the top right; disc grouping, menu fonts and similar options live under Advanced settings.
- Save/open JSON profiles and resume interrupted jobs: after a failure or cancellation you can continue or restart, and keep or discard progress. Intermediate files are cleaned up after a successful build and verification.
- Export and verification follow the same success/failure/cancellation rules as the desktop app: export writes a temporary file and verifies it before replacing an existing ISO, and a failure keeps the old ISO and the recoverable unfinished task instead of reporting success. Export, verification and Stop task respond to cancellation.
- Build progress is shown in the notification shade; Android 16+ uses the system Live Updates progress style and older versions use a regular progress notification. Android 13+ may ask for notification permission; declining it does not affect in-app progress.
- LTPO adaptive refresh: the app advertises the panel's peak refresh rate and lets the system switch rates seamlessly.
- Android MLP encoding now uses the same encoder implementation as Windows instead of a slower platform fallback path.
- Logs support "Problems only", copy and export through the system picker; the status area shows elapsed time and issue count, and log text follows the Windows wording.
- The About page shows the author, project home, app information and bundled licenses (project, FFmpeg, third-party, image/menu notices); Flutter dependency licenses can also be viewed in the app.
- Fixed: a first folder selection could be dropped, an old source mirror could be reused after reselecting a folder, "Check sources" could fail with an empty log because of system-directory permissions, and Delete progress could do nothing. Chosen folders are now validated and synced before a task starts, and logs record the resolved paths first.
- Notes: requires Android 7.0+ on ARM64; grant folders through the system picker and keep the grants valid; check free space in the destination before exporting large ISOs. Delete progress removes the app's caches and unfinished tasks but never deletes granted folders or exported ISOs. The APK is locally signed for direct installation and is not distributed through an app store.

**Windows edition (x64 and WoA · single EXE)**

- Windows now ships as a single EXE: run the EXE for your architecture directly, with no extraction. The license, FFmpeg LGPL notice and third-party/image/menu statements are embedded in the About window; no external license files need to be kept beside the program.
- The About entry now sits in the title bar's top-right button group and matches the neighbouring buttons in size, hover, focus and theme behaviour. About is a scrollable page showing the author, project home, version and bundled licenses; it supports moving, left/right edge and corner resizing, shows scrollbars only when content overflows, refreshes immediately when language or theme changes, and reuses one window when opened again.
- In-app message, confirmation and choice popups now share the main window's title bar and close button styling, wrap long button rows, use Enter for the default button and Esc to cancel, and show a focus ring for keyboard focus.
- Menu settings are clearer: the menu switch controls navigation menu generation; turning menus off hides menu-only options (tracks per page, menu video standard, cover dimming, menu fonts). Labels now distinguish menu-background dimming from the cover shown during playback.
- In audio settings, LPCM offers only 16-bit and 24-bit targets; an existing 20-bit LPCM setting falls back to 24-bit, and 20-bit output uses MLP. Both x64 and WoA ship as single EXEs, and existing authoring behavior is unchanged.

**Shared notes**

- Android and Windows use the same profile keys, defaults, menu/disc-splitting rules, log events and progress stages; each platform presents the UI in its own idiom.
- After building, verify the output before using it in a DVD-Audio compatible player or burner. Ordinary DVD-Video players are not guaranteed to play it.

### 日本語

**Android 版（新規 · ARM64 APK）**

- Android スマートフォン／タブレット向けアプリを追加しました（Android 7.0 以上、ARM64）。音源チェック、MLP/LPCM エンコード、曲目メニューとアルバム索引、自動分盤、完成品の検証は Windows 版と同じ制作フローと既定値を使用します。
- 音源フォルダーと出力フォルダーはシステムのファイルピッカーで許可します。「すべてのファイルへのアクセス」権限は要求しません。音源はアプリのプライベート領域で処理し、完成した ISO だけを選択したフォルダーへ書き出します。フォルダー内のほかのファイルは変更しません。
- 5 言語（簡体字中国語・繁体字中国語・英語・日本語・韓国語）と明/暗テーマに対応。よく使う設定は画面右上に、ディスク分割やメニュー用フォントなどは「詳細設定」にまとめています。
- JSON プロファイルの保存/読み込みと中断からの再開に対応。失敗・キャンセル後は「続きから作成」か「最初から作成」を選べ、進捗の保持・削除も選択できます。作成と検証の成功後は中間ファイルを削除します。
- 書き出しと検証はデスクトップ版と同じ成功/失敗/キャンセル規則です。書き出しは一時ファイルへ書き、検証してから既存の ISO を置き換えます。失敗時は元の ISO と再開可能な未完了タスクを保持し、未完成の結果を成功として表示しません。書き出し、検証、「タスクを停止」はキャンセルに応答します。
- 作成中は通知領域に進捗を表示します。Android 16 以上ではシステムの Live Updates 進捗スタイル、それ以前は通常の進捗通知を使用します。Android 13 以上では必要に応じて通知権限を要求しますが、拒否してもアプリ内の進捗表示には影響しません。
- LTPO 可変リフレッシュレートに対応：アプリがパネルのピークリフレッシュレートをシステムへ通知し、システムが場面に応じてシームレスに切り替えます。
- Android の MLP エンコードは Windows と同じエンコーダー実装を使用し、遅いプラットフォーム向けフォールバック経路を使わなくなりました。
- ログは「問題のみ表示」、コピー、システムの選択画面からの書き出しに対応。状態欄には実行時間と問題数を表示し、ログ文言は Windows 版と揃えています。
- 「情報」ページに作者、プロジェクトページ、アプリ情報、同梱ライセンス（プロジェクト、FFmpeg、第三者、画像/メニュー声明）を表示し、Flutter 依存パッケージのライセンスもアプリ内で確認できます。
- 修正内容：最初のフォルダー選択が失われることがある、音源フォルダーを選び直しても古い内容が使われる、「音源を検査」がシステムディレクトリ権限で必ず失敗しログが空になる、「進捗を削除」が効かない、といった問題を修正しました。タスク開始前に選択フォルダーを検証・同期し、最初に実際の使用パスをログへ記録します。
- 注意：Android 7.0 以上の ARM64 端末が必要です。アクセスするフォルダーはシステムの選択画面で許可し、その許可を有効に保ってください。大きな ISO を書き出す前に保存先の空き容量を確認してください。「進捗を削除」はアプリのキャッシュと未完了タスクを削除しますが、許可したフォルダーや書き出し済み ISO は削除しません。現在の APK はローカル署名で、アプリストアでは配布していません。

**Windows 版（x64 / WoA · 単一 EXE）**

- Windows は単一 EXE 配布になりました。アーキテクチャに合う EXE をダウンロードしてそのまま起動でき、解凍は不要です。ライセンス、FFmpeg の LGPL 表示、第三者・画像・メニューの声明は「情報」画面に内蔵され、外部のライセンスファイルを同梱する必要はありません。
- 「情報」入口をタイトルバー右上の統一ボタングループへ移動し、隣接ボタンと同じサイズ・ホバー・フォーカス・テーマ動作にしました。情報画面はスクロール可能な独立ページで、作者、プロジェクトページ、バージョン、同梱ライセンスを表示します。左右・上下の辺および四隅からリサイズでき、内容があふれた場合のみスクロールバーを表示します。言語/テーマの変更は即時反映され、再度開いても同じウィンドウを再利用します。
- アプリ内の通知・確認・選択ダイアログは、メインウィンドウと同じタイトルバーと閉じるボタンの体裁に統一しました。長いボタン行は折り返し、Enter で既定ボタン、Esc でキャンセル、キーボードフォーカス時はフォーカス枠を表示します。
- メニュー設定を整理しました。メニューのスイッチがナビゲーションメニューの生成を制御し、メニューを閉じるとページごとの曲数、メニュー映像方式、カバー画像の暗さ、メニュー用フォントなどメニュー専用の設定を隠します。ラベルは「メニュー背景のカバー画像の暗さ」と「再生中のカバー表示」を区別します。
- 音声設定で LPCM を選ぶとビット深度は 16 bit と 24 bit のみになります。既存の 20 bit LPCM 設定は 24 bit に切り替わり、20 bit は MLP を使用します。x64 と WoA はどちらも単一 EXE で配布し、既存の制作挙動は変更しません。

**共通の注意**

- Android と Windows は同じプロファイルキー、既定値、メニュー/分盤ルール、ログイベント、進捗段階を使用します。画面操作は各プラットフォームの作法に合わせています。
- 作成後は「完成品を検証」を実行してから、DVD-Audio 対応のプレーヤーや書き込みソフトで使用してください。通常の DVD-Video プレーヤーでの再生は保証されません。

### 下载 / Download / ダウンロード

- Windows（Intel/AMD，x64，单 EXE）：`DVD-Audio-Maker-v1.2-win-x64.exe`
- Windows（ARM64 / WoA，单 EXE）：`DVD-Audio-Maker-v1.2-win-arm64.exe`
- Android（ARM64，APK）：`DVD-Audio-Maker-v1.2-android-arm64.apk`

## v1.1

### 中文

- 新增繁体中文与韩语界面，支持系统语言检测与命令行语言选择。
- DVD-Audio 菜单文案与字形按语言本地化，默认使用简体中文、繁体中文、日文、韩文对应字形；用户路径、曲名与专辑名中的 Unicode 字符完整保留。
- 菜单字体集合做了无损优化：保留全部字符、四种地区字形、轮廓与排版，文件更小但显示效果不变。
- 改进 MLP 无损编码的兼容性与稳定性：修复部分第三方 MLP 文件被误判为不连续的问题，并使编码结果更接近历史版本的音质表现。
- 修复多页菜单在部分 DVD-Audio 项目中的生成问题。
- 菜单子画面与导航功能已完整内置，发布包保持 GUI-only 单文件形式，无需安装额外菜单组件。
- 新增 Windows ARM64（WOA）原生版本：x64 与 ARM64 均提供单 EXE、静态 CRT，无需另外安装运行库。

### English

- Added Traditional Chinese and Korean UI, with system-language detection and command-line language selection.
- DVD-Audio menu labels and glyphs are localized per language, defaulting to the appropriate Simplified Chinese, Traditional Chinese, Japanese or Korean regional glyphs; Unicode paths, track titles and album names are fully preserved.
- The menu font collection is now losslessly optimized: all characters, four regional glyph sets, outlines and layout are retained, with a smaller package and unchanged rendering.
- Improved MLP lossless encoding compatibility and stability: fixed cases where some third-party MLP files were wrongly rejected as discontinuous, and brought encoded results closer to historical quality.
- Fixed multi-page menu generation for some DVD-Audio projects.
- Menu subpictures and navigation are now fully built in; the release remains a GUI-only single-file package with no extra menu components to install.
- Added a native Windows on ARM64 (WOA) build: x64 and ARM64 both ship as single EXEs with the static CRT and need no additional runtime installation.

### 日本語

- 繁体字中国語と韓国語の UI を追加しました。システム言語の自動検出とコマンドラインでの言語選択に対応します。
- DVD-Audio メニューの文言と字形を言語ごとにローカライズし、簡体字中国語・繁体字中国語・日本語・韓国語の地域字形を既定で使用します。ユーザーのパス、曲名、アルバム名の Unicode 文字はそのまま保持されます。
- メニュー用フォントコレクションを可逆的に最適化しました。すべての文字、4 地域の字形、輪郭、レイアウトを保持したまま、ファイルサイズを削減し、表示は変わりません。
- MLP 可逆圧縮の互換性と安定性を改善しました。一部のサードパーティ製 MLP ファイルが不連続と誤判定される問題を修正し、従来の音質に近づけました。
- 一部の DVD-Audio プロジェクトで複数ページメニューが生成できない問題を修正しました。
- メニューのサブピクチャとナビゲーションを完全に内蔵しました。配布パッケージは引き続き GUI のみの単一ファイル形式で、追加のメニューコンポーネントは不要です。
- Windows on ARM64（WOA）ネイティブ版を追加しました。x64 と ARM64 の両方が単一 EXE・静的 CRT で、追加のランタイムインストールは不要です。
