# Update log

## v1.2

### 中文

**新增 Android 版**

- DVD-Audio Maker 现在可以在 Android 手机和平板上使用了。完整的制作流程——音源检查、MLP/LPCM 编码、选曲菜单、专辑索引、自动分盘、成品验证——与 Windows 版完全一致。
- 界面按 Android 习惯全新设计（Material 风格），支持简体中文、繁体中文、英文、日文、韩文五种界面语言，可切换明/暗主题。
- 目录通过系统文件选择器授权，不申请“所有文件访问”权限；音源只在应用内处理，制作完成的 ISO 导出到你指定的文件夹。
- 支持断点续制、批量队列、详细日志与日志导出。
- 安装包约 12.3 MB，要求 Android 7.0 及以上版本的 ARM64 设备。当前为本地安装签名，未上架应用商店，下载 APK 后可直接安装。

**Windows 版**

- 本次无功能更新，x64 与 ARM64 维持 v1.1 的一切：免安装单 EXE、静态运行库、菜单与字体全部内置。

### English

**Android edition**

- DVD-Audio Maker now runs on Android phones and tablets. The full authoring pipeline — source check, MLP/LPCM encoding, track menus, album index, automatic disc splitting and final verification — is identical to the Windows version.
- The interface is redesigned for Android following Material conventions, with five UI languages (Simplified Chinese, Traditional Chinese, English, Japanese, Korean) and light/dark themes.
- Folders are granted through the system file picker; the app never requests “all files access”. Sources are processed inside the app, and finished ISOs are exported to the folder you choose.
- Supports resuming interrupted jobs, batch queue, detailed logs and log export.
- The APK is about 12.3 MB and requires Android 7.0 or later on an ARM64 device. It is signed for direct installation and is not distributed through any app store.

**Windows**

- No changes in this release. Both x64 and ARM64 remain portable single EXEs with everything built in, exactly as in v1.1.

### 日本語

**Android 版**

- DVD-Audio Maker が Android スマートフォンとタブレットに対応しました。音源チェック、MLP/LPCM エンコード、トラックメニュー、アルバムインデックス、自動分盤、完成後の検証まで、Windows 版と同じ制作フローです。
- Android の習慣に合わせて Material デザインで作り直したインターフェース。中国語（簡体字・繁体字）、英語、日本語、韓国語の 5 か国語と、明/暗テーマの切り替えに対応しています。
- フォルダーはシステムのファイルピッカーで許可する方式のため、「すべてのファイルへのアクセス」権限は不要です。音源はアプリ内で処理され、完成した ISO は指定したフォルダーへ書き出します。
- 中断した制作の再開、バッチキュー、詳細ログとログ出力に対応しています。
- APK は約 12.3 MB。Android 7.0 以降の ARM64 端末が必要です。現在は直接インストール用の署名で、ストアには公開していません。

**Windows 版**

- 今回の変更はありません。x64 / ARM64 とも v1.1 と同じく、インストール不要の単一 EXE で、必要なものはすべて内蔵されています。

### 下载 / Download / ダウンロード

- Windows（Intel/AMD）：`DVD-Audio-Maker-v1.2-win-x64.zip`
- Windows（ARM 设备）：`DVD-Audio-Maker-v1.2-win-arm64.zip`
- Android（ARM64）：`DVD-Audio-Maker-v1.2-android-arm64.apk`

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
