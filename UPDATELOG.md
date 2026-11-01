# Update log

## v1.1

### 中文

- 新增繁体中文与韩语界面、核心消息、CLI 语言选择及系统语言检测支持。
- 对齐各语言 DVD-Audio 菜单固定文案和 SC／TC／JP／KR 默认地区字形；保留用户路径、曲名与专辑名的 Unicode 内容。
- 增加无损菜单字体集合优化工具，保留全部字符及四地区字形，并校验轮廓、布局与度量。
- 修复原生 MLP 编码冻结基准回归：将主预测器扰动搜索从 4 次恢复为历史 C 实现的 16 次；116 个冻结用例的长度与原始 SHA256 全部通过。
- 保持发布包 GUI-only 的分发方式。
- 新增 Windows ARM64（WOA）原生发行包：FFmpeg、ImageMagick、MLP 编码与菜单运行时均为 ARM64 原生构建，采用静态 CRT，单 EXE，归档名为 -win-arm64.zip。
- 菜单子画面与导航（XML 解析、VM 编译、VOB 组装、NAV 包）已整体迁移到 Rust：删除 C 菜单 vendor 与静态 vendor 库，发布包内嵌进程内 Rust 菜单运行时，不再依赖独立菜单 DLL。
- 修复 MLP 输入到达时间抖动或回绕被误判为不连续的回归，提升第三方 MLP 输入兼容性。
- 发布验收覆盖进程内 Rust 菜单的跨进程可重复性与 production 制盘（pal-1albums），校验 UDF 载荷与 SPU 调色板／AMG 一致性。

验收说明：本次及以后以经独立 MLP 解码后的完整 PCM 样本数据与编码输入的 SHA256 一致作为无损验收标准，统一样本字节表示并排除容器头。允许编码码流和压缩大小不同，不再要求完整旧 C 严格对照作为发布门槛。格式兼容性检查继续保留；116 个冻结向量及编码哈希用于回归诊断，不应表述为全部输入与旧 C 字节一致。

### English

- Add Traditional Chinese and Korean support across the GUI, core messages, CLI language selection and system-language detection.
- Align localized DVD-Audio menu labels and default SC/TC/JP/KR regional glyphs while preserving Unicode user paths and metadata.
- Add lossless menu-font collection optimization with character, regional-glyph, outline, layout and metric preservation checks.
- Fix the native MLP frozen-vector regression by restoring the primary predictor search from 4 to the historical C budget of 16 perturbations. All 116 frozen cases pass length and raw SHA256 checks.
- Retain GUI-only distribution packaging.
- Add a native Windows on ARM64 (WOA) package: FFmpeg, ImageMagick, the MLP encoder and the menu runtime are built natively for ARM64 with a static CRT, shipped as a single EXE in the -win-arm64.zip archive.
- Move the complete menu subpicture and navigation pipeline (XML parsing, VM compilation, VOB assembly, NAV packs) to Rust: the C menu vendor and static vendor archives are removed, and the package embeds the in-process Rust menu runtime instead of loading separate menu DLLs.
- Fix a regression where MLP arrival-time jitter or wraparound was rejected as discontinuous, improving compatibility with third-party MLP inputs.
- Release acceptance now covers cross-process repeatability of the in-process Rust menu plus production authoring (pal-1albums), checking UDF payloads and SPU palette/AMG consistency.

Acceptance note: lossless acceptance for this and future releases requires matching SHA256 of the complete independently MLP-decoded PCM and encoder input, using the same sample representation and excluding container headers. Encoded bytes and compressed sizes may differ; complete historical C comparison is not a release gate. Format compatibility checks remain applicable. The 116 frozen vectors and encoded hashes are regression diagnostics, not a claim of historical C byte equality for all inputs.

### 日本語

- GUI、コアメッセージ、CLI の言語選択、システム言語の検出に繁体字中国語と韓国語のサポートを追加しました。
- 各言語の DVD-Audio メニューの定型文と SC／TC／JP／KR の既定の地域別字形を統一しました。ユーザーのパス、曲名、アルバム名の Unicode 文字は保持されます。
- 全文字と 4 地域の字形を保持する、ロスレスなメニューフォントコレクション最適化ツールを追加しました。アウトライン、レイアウト、メトリクスの保持も検証します。
- ネイティブ MLP エンコーダーの固定ベンチマークにおけるリグレッションを修正しました。主予測器の摂動探索回数を 4 回から従来の C 実装と同じ 16 回に戻し、116 件すべてで出力長と元の SHA256 の検証に合格しました。
- 配布パッケージは引き続き GUI のみです。
- Windows on ARM64（WOA）ネイティブ配布パッケージを追加しました。FFmpeg、ImageMagick、MLP エンコーダー、メニューランタイムは ARM64 でネイティブにビルドされ、静的 CRT の単一 EXE として -win-arm64.zip に含まれます。
- メニューのサブピクチャとナビゲーション（XML 解析、VM コンパイル、VOB 組み立て、NAV パック）を Rust に完全移行しました。C メニュー vendor と静的 vendor ライブラリを削除し、パッケージにはプロセス内 Rust メニューランタイムを内蔵します（独立したメニュー DLL は不要）。
- MLP 入力の到達時刻の揺らぎやラップアラウンドが不連続と誤判定されるリグレッションを修正し、サードパーティ製 MLP 入力の互換性を改善しました。
- リリース受け入れでは、プロセス内 Rust メニューのプロセス間再現性と production オーサリング（pal-1albums）を検証し、UDF ペイロードと SPU パレット／AMG の整合性を確認します。

受け入れ基準：今回以降のリリースでは、独立した MLP デコーダーで復号した全 PCM サンプルとエンコーダー入力の SHA256 の一致をロスレス性の基準とします。サンプルのバイト表現を統一し、コンテナーヘッダーは除外します。符号化ビットストリームや圧縮後のサイズの違いは許容し、旧 C 実装との完全な厳密比較はリリース条件としません。形式の互換性確認は継続します。116 件の固定テストベクトルと符号化データのハッシュは回帰診断用であり、すべての入力で旧 C 実装とバイト単位で一致することを意味しません。
