# Update log

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
