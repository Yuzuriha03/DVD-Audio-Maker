# Update log

## v1.3 (2026-11-19)

### 界面优化
*   **弹窗布局**：优化了提示与确认弹窗的顶部标题栏布局。移除了程序名文字，使弹窗更加简洁；同时优化了布局算法，确保副标题在无程序名时能完美垂直居中对齐，提升了提示弹窗的视觉专业度。

### Android 应用修复
*   **轮询与状态同步**：重构了 AppController 的任务轮询逻辑。修正了在日志为空但修订版本号（logRevision）更新时出现的快照替换缺陷，确保即使日志暂时为空，任务进度和状态也能保持实时增量同步。
*   **稳定性增强**：优化了通知状态的同步逻辑，减少冗余更新。

### 国际化支持
*   **翻译完善**：修复了多语言文件（i18n.rs）中 brace 结构不匹配的问题，确保程序在各语言环境下均能稳定运行。修正了日文环境下 "Show in folder" 翻译过长导致的布局溢出问题。

### 开发与打包
*   **构建优化**：修复了 Windows 和 Android 的构建流程中的若干语法兼容性问题，并清理了打包脚本中的文件锁定阻塞，提升了打包成功率与一致性。


## v1.2

### 中文

**新增 Android 版**

- Android 7.0 及以上的 ARM64 手机和平板可安装 APK，在设备上制作 DVD-Audio ISO。与 Windows 共用制作流程，支持 MLP / LPCM 编码、导入已有 MLP、自动分盘、选曲菜单、专辑索引和播放封面。
- 用系统文件夹选择器授权音源和成品目录，无需「所有文件访问」权限。支持直接读取音源的文件提供方无需先复制整个目录；不支持时使用应用内部的临时副本。成品在校验后导出，替换已有 ISO 前还会核对写入的副本；导出失败时会尝试恢复原 ISO，并保留可续作的数据。
- 可用系统选择器打开或保存 JSON 方案、导出单次任务的完整日志。支持五种界面语言和明暗主题。制作进度可显示在通知栏；Android 16 及以上在系统支持并允许通知时可使用 Live Updates。拒绝通知权限不影响应用内进度。
- 可续作或重新制作，也可从「更多设置」删除未完成的进度与应用内部缓存；不会删除已授权的目录或已导出的 ISO。

**Windows 版变化**

- 标准发布格式从 ZIP 改为可直接运行的单个 EXE，仍分别提供 x64 和 Windows on ARM 版本。新版新增可滚动、可调整大小的「关于」窗口，从标题栏打开即可查看作者、版本、项目主页及随包许可证。
- 提示、确认和任务恢复对话框改用与主窗口一致的样式。失败与主动停止会显示不同的提示；发生失败时，保留或删除进度的对话框可显示具体原因。
- 选择 LPCM 时，位深下拉列表只提供 16 位和 24 位；需要 20 位输出请选 MLP。关闭导航菜单会隐藏菜单专用设置，菜单字体的完整设置行仅在菜单和高级设置都开启时显示。窄窗口中的说明文字也改进了标点换行。
- ALAC 曲目经自动修复后，菜单仍按原专辑归类，并读取原目录中的封面。

**升级提示**

- 仍由一次制作操作自动完成音源检查、编码、生成 ISO 和成品验证，无需另外验证。旧的 `verify.cmd` 和开发命令行中的独立 `prepare`、`verify`、`quick-check`、`audit` 已移除（包括 `verify --iso` 单独校验指定镜像的用法）。普通 `build` 仍包含检查和验证。

### English

**New Android app**

- Install the APK on an ARM64 phone or tablet running Android 7.0 or later to create DVD-Audio ISOs on the device. It uses the same authoring workflow as Windows, with MLP / LPCM encoding, import of existing MLP files, automatic disc splitting, track menus, album indexes and covers during playback.
- Grant access to source and output folders through the system picker; all-files access is not required. The app reads sources directly when the provider supports it and otherwise uses a temporary app-private copy. It exports verified ISOs and checks the written copy before replacing an existing ISO. If export fails, it attempts to restore the old ISO and keeps the data needed to resume.
- Use the system picker to open or save JSON profiles and export the full log for a task. The app supports five interface languages and light and dark themes. Progress can appear in system notifications, including Live Updates on Android 16 or later when supported and permitted. In-app progress still works if notifications are denied.
- Resume or start over. More settings lets you discard unfinished progress and clear internal caches without deleting granted folders or exported ISOs.

**Windows changes**

- The standard release now comes as a single executable you can run directly instead of a ZIP, with separate builds for x64 and Windows on ARM as before. The new resizable, scrollable About window opens from the title bar and lists the author, version, project homepage and bundled licenses.
- Messages, confirmations and task recovery dialogs now match the main window. A failure and a user-requested stop show different messages; the dialog that asks whether to keep or discard progress can show the reason for a failure.
- Selecting LPCM now shows only 16-bit and 24-bit options in the bit-depth list; use MLP for 20-bit output. Disabling navigation menus hides menu-only settings. The full menu font rows appear only when menus and Advanced settings are both enabled. Explanatory text wraps punctuation better in narrow windows.
- Automatically repaired ALAC tracks now stay under their original albums in menus and can still use the covers from their original folders.

**Upgrade note**

- A single build still checks sources, encodes audio, creates the ISOs and verifies the output automatically. The old `verify.cmd` wrapper and standalone developer CLI commands `prepare`, `verify`, `quick-check` and `audit` have been removed, including `verify --iso` for checking one existing image. The regular `build` command still includes checks and verification.

### 日本語

**Android 版を追加**

- Android 7.0 以上の ARM64 スマートフォンやタブレットに APK をインストールし、端末上で DVD-Audio ISO を作成できます。Windows 版と共通の制作フローで、MLP / LPCM エンコード、既存 MLP の取り込み、ディスクへの自動振り分け、曲目メニュー、アルバム索引、再生中のカバー表示に対応します。
- 音源と出力先はシステムのフォルダー選択画面で許可します。「すべてのファイルへのアクセス」権限は不要です。直接読み込めない音源はアプリ内部に一時コピーします。検証済みの ISO を書き出し、書き込んだコピーを確認してから既存の ISO を置き換えます。書き出しに失敗した場合は元の ISO の復元を試み、再開に必要なデータを保持します。
- システムの選択画面から JSON プロファイルの保存・読み込みと、タスクごとの全ログの書き出しができます。5 言語とライト／ダークテーマに対応します。制作の進捗は通知に表示でき、Android 16 以上ではシステムが対応し通知が許可されていれば Live Updates を利用できます。通知を許可しなくても、アプリ内で進捗を確認できます。
- 中断した作業を再開するか、最初からやり直せます。設定メニューで未完了の進捗やアプリ内部のキャッシュを削除しても、許可済みのフォルダーや書き出し済みの ISO は削除されません。

**Windows 版の変更**

- 標準の配布形式を ZIP から、直接起動できる単一 EXE に変更しました。x64 版と Windows on ARM 版は引き続き別々に提供します。タイトルバーに新設した「情報」から、スクロールやサイズ変更が可能なウィンドウで、作者、バージョン、プロジェクトのホームページ、同梱ライセンスを確認できます。
- メッセージ、確認、タスク再開のダイアログをメインウィンドウと同じ外観にしました。失敗と利用者による中止では異なるメッセージを表示します。失敗後に進捗を保持または削除するダイアログでは、失敗の原因を確認できます。
- LPCM を選ぶと、ビット深度の選択欄には 16 ビットと 24 ビットのみが表示されます。20 ビットには MLP を使用してください。ナビゲーションメニューを無効にすると専用設定は非表示になります。メニューフォントの設定欄は、メニューと詳細設定の両方が有効な場合にだけ表示されます。横幅の狭いウィンドウでの説明文の句読点の折り返しも改善しました。
- 自動修復した ALAC の曲も、メニューでは元のアルバムに属し、元のフォルダーのカバー画像を使えるようになりました。

**アップグレード時の注意**

- これまでどおり、一度の制作操作で音源チェック、エンコード、ISO の作成、完成品の検証まで自動で行います。旧 `verify.cmd` と、開発用 CLI の単独コマンド `prepare`、`verify`、`quick-check`、`audit` は削除されました。既存の ISO を 1 枚だけ検証する `verify --iso` も対象です。通常の `build` にはチェックと検証が引き続き含まれます。

### 下载 / Download / ダウンロード

[GitHub Releases v1.2](https://github.com/Yuzuriha03/DVD-Audio-Maker/releases/tag/v1.2)

- Windows x64: `DVD-Audio-Maker-v1.2-win-x64.exe`
- Windows ARM64 / WoA: `DVD-Audio-Maker-v1.2-win-arm64.exe`
- Android ARM64: `DVD-Audio-Maker-v1.2-android-arm64.apk`

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
