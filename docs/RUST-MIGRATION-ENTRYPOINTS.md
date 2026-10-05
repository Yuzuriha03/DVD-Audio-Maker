# 固定 C# 基线的功能与测试映射

基线 `fb3cbb84788a807758fa6a8f472a9ffdad3115e8`：90 份 C# 源文件、115 个兼容性测试入口。完整源文件、公开/私有成员签名、Rust/C 对应路径见 [机器可读清单](rust-migration-entrypoints-2026-10-05.json)。

本表是逐项映射，不把历史 C# 测试结果冒充本轮执行结果。最新执行记录见 [机器可读验收记录](rust-migration-recheck-fixes-2026-10-05.json)；[初次验收快照](rust-migration-checklist-validation-2026-10-05.json) 保留原结果。一个新回归测试可覆盖多个旧入口；冻结 MLP 116 个样本是另一项计数。被用户排除的行为单独注明。

后续 [独立复核](RUST-MIGRATION-RECHECK-2026-10-05.md) 确认的 R01–R16 已修复，最终验证另补齐 R17，见 [修复验收报告](RUST-MIGRATION-RECHECK-FIXES-2026-10-05.md)。本表仍仅表示源码/测试入口的对应关系，不能把各行的“当前证据”理解为该入口任意输入均已通过。

| 编号 | 旧入口 | 当前证据文件 | 范围说明 |
|---|---|---|---|
| 001 | `RuntimeArchiveTests.RoundtripAndRepair` | [runtime.rs](../rust/crates/dvda-core/src/runtime.rs)、[test-rust-onefile.py](../tools/win-build/test-rust-onefile.py) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 002 | `RuntimeArchiveTests.RejectUnsafePaths` | [runtime.rs](../rust/crates/dvda-core/src/runtime.rs)、[test-rust-onefile.py](../tools/win-build/test-rust-onefile.py) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 003 | `FlacMetadataTests.InProcessEditor` | [flac_metadata.rs](../rust/crates/dvda-core/tests/flac_metadata.rs)、[conversion_native.rs](../rust/crates/dvda-core/tests/conversion_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 004 | `LocalizationTests.CatalogCoverage` | [localization.rs](../rust/crates/dvda-core/src/localization.rs)、[main.rs](../rust/crates/dvda-desktop/src/main.rs)、[test-rust-gui.py](../tools/win-build/test-rust-gui.py) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 005 | `LocalizationTests.JapaneseCoverage` | [localization.rs](../rust/crates/dvda-core/src/localization.rs)、[main.rs](../rust/crates/dvda-desktop/src/main.rs)、[test-rust-gui.py](../tools/win-build/test-rust-gui.py) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 006 | `LpcmTests.FormatAndGrouping` | [lpcm.rs](../rust/crates/dvda-core/src/lpcm.rs)、[disc.rs](../rust/crates/dvda-core/src/disc.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 007 | `LocalizationTests.OpaqueValuesAndCulture` | [localization.rs](../rust/crates/dvda-core/src/localization.rs)、[main.rs](../rust/crates/dvda-desktop/src/main.rs)、[test-rust-gui.py](../tools/win-build/test-rust-gui.py) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 008 | `LocalizationTests.ProfileAndArguments` | [developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs)、[app.rs](../rust/crates/dvda-core/src/app.rs)、[main.rs](../rust/crates/dvda-desktop/src/main.rs)、[test-rust-gui.py](../tools/win-build/test-rust-gui.py) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 009 | `ParseAssignments` | 明确排除 | 用户删除 config.env 解析，不恢复引号/注释/查找旧文件。 |
| 010 | `NormalizeWindowsPaths` | [options_parity.rs](../rust/crates/dvda-core/tests/options_parity.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 保留 JSON Windows 路径与原样值；env 文本转义规则排除。 |
| 011 | `EnvironmentWins` | [options_parity.rs](../rust/crates/dvda-core/tests/options_parity.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 保留开发环境覆盖 JSON 优先级；config.env 文件排除。 |
| 012 | `EmptyEnvironmentDoesNotOverride` | [options_parity.rs](../rust/crates/dvda-core/tests/options_parity.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 013 | `DefaultsAndNumericFallbacks` | [options_parity.rs](../rust/crates/dvda-core/tests/options_parity.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 014 | `DerivedPathsAndNames` | [options_parity.rs](../rust/crates/dvda-core/tests/options_parity.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 015 | `ClampGroupTrackLimit` | [options_parity.rs](../rust/crates/dvda-core/tests/options_parity.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 016 | `NormalizeMlpSource` | [options_parity.rs](../rust/crates/dvda-core/tests/options_parity.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 017 | `GuiConfigurationTests.ProfileRoundtrip` | [developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs)、[app.rs](../rust/crates/dvda-core/src/app.rs)、[main.rs](../rust/crates/dvda-desktop/src/main.rs)、[test-rust-gui.py](../tools/win-build/test-rust-gui.py) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 018 | `GuiConfigurationTests.ImportEnv` | [developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs)、[app.rs](../rust/crates/dvda-core/src/app.rs)、[main.rs](../rust/crates/dvda-desktop/src/main.rs)、[test-rust-gui.py](../tools/win-build/test-rust-gui.py) | 排除旧 env 导入；保留 JSON 未知键、版本校验和方案持久化。 |
| 019 | `GuiConfigurationTests.ExplicitValuesWin` | [developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs)、[app.rs](../rust/crates/dvda-core/src/app.rs)、[main.rs](../rust/crates/dvda-desktop/src/main.rs)、[test-rust-gui.py](../tools/win-build/test-rust-gui.py) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 020 | `GuiPresentationTests.ImportantDiagnosticsSurvive` | [main.rs](../rust/crates/dvda-desktop/src/main.rs)、[presentation.rs](../rust/crates/dvda-desktop/src/presentation.rs)、[process.rs](../rust/crates/dvda-core/src/process.rs)、[test-rust-gui.py](../tools/win-build/test-rust-gui.py) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 021 | `GuiPresentationTests.CompleteLineCapture` | [main.rs](../rust/crates/dvda-desktop/src/main.rs)、[presentation.rs](../rust/crates/dvda-desktop/src/presentation.rs)、[process.rs](../rust/crates/dvda-core/src/process.rs)、[test-rust-gui.py](../tools/win-build/test-rust-gui.py) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 022 | `GuiPresentationTests.ChoiceValuesRemainStable` | [developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs)、[app.rs](../rust/crates/dvda-core/src/app.rs)、[main.rs](../rust/crates/dvda-desktop/src/main.rs)、[test-rust-gui.py](../tools/win-build/test-rust-gui.py) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 023 | `MlpEncoderTests.DllCancellation` | [encoder_native.rs](../rust/crates/dvda-core/tests/encoder_native.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 024 | `() => { FfmpegPcmTests.ConversionContract(); MediaMigrationTests.Run(); }` | [media_native.rs](../rust/crates/dvda-core/tests/media_native.rs)、[audio_native.rs](../rust/crates/dvda-core/tests/audio_native.rs) | 保留目标精度/声道/转换行为；外部 FFmpeg 命令字符串排除。 |
| 025 | `FfmpegPcmTests.FailureAndCancellation` | [media_native.rs](../rust/crates/dvda-core/tests/media_native.rs)、[audio_native.rs](../rust/crates/dvda-core/tests/audio_native.rs) | 保留原生转换失败/取消保护；外部程序 PATH 选择排除。 |
| 026 | `FfmpegPcmTests.LegacySettings` | 明确排除 | 用户删除外部 FFmpeg/eac3to 可执行文件设置与兼容层。 |
| 027 | `MlpEncoderTests.OversizedAccessUnit` | [encoder_native.rs](../rust/crates/dvda-core/tests/encoder_native.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 028 | `MlpEncoderTests.OddPcmTail` | [pcm_streaming.rs](../rust/crates/dvda-core/tests/pcm_streaming.rs) | 保留奇数 PCM 长度/容器封装；不调用 eac3to。 |
| 029 | `BuildSurcodeBatchJob` | [encoder_native.rs](../rust/crates/dvda-core/tests/encoder_native.rs)、[acquisition.rs](../rust/crates/dvda-core/src/acquisition.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 030 | `() => { MlpEncoderTests.Metadata(); EncoderMigrationTests.Run(); }` | [encoder_native.rs](../rust/crates/dvda-core/tests/encoder_native.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 031 | `() => { UpconvertSurcodePcmWav(); PcmMigrationTests.Run(); }` | [pcm_streaming.rs](../rust/crates/dvda-core/tests/pcm_streaming.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 032 | `EscapeShellAssignment` | [options_parity.rs](../rust/crates/dvda-core/tests/options_parity.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 033 | `PreserveLegacyShellKeySet` | [options_parity.rs](../rust/crates/dvda-core/tests/options_parity.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 保留有效派生键/转义/顺序；删除 DVDA_FFMPEG/FFPROBE/MKISOFS/EAC3TO 外部命令键。 |
| 034 | `DescribeConfigurationSources` | [options_parity.rs](../rust/crates/dvda-core/tests/options_parity.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 035 | `StripIsoVersion` | [native_format_parity.rs](../rust/crates/dvda-core/tests/native_format_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 036 | `ParsePts` | [format_parity.rs](../rust/crates/dvda-native/src/format_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 037 | `CalculatePeakBitrate` | [format_parity.rs](../rust/crates/dvda-native/src/format_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 038 | `WalkMlpAccessUnits` | [format_parity.rs](../rust/crates/dvda-native/src/format_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 039 | `MlpChecksumBaseline` | [format_parity.rs](../rust/crates/dvda-native/src/format_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 040 | `ValidateMlpDamageFixtures` | [format_parity.rs](../rust/crates/dvda-native/src/format_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 041 | `MlpStreamingInspection` | [format_parity.rs](../rust/crates/dvda-native/src/format_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 042 | `FormatCommandLine` | [process_native.rs](../rust/crates/dvda-cli/tests/process_native.rs)、[process.rs](../rust/crates/dvda-core/src/process.rs)、[buildlog.rs](../rust/crates/dvda-core/src/buildlog.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 043 | `() => { RunExternalProcess(); ProcessMigrationTests.Run(); }` | [process_native.rs](../rust/crates/dvda-cli/tests/process_native.rs)、[process.rs](../rust/crates/dvda-core/src/process.rs)、[buildlog.rs](../rust/crates/dvda-core/src/buildlog.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 044 | `HandleNonZeroExitCode` | [process_native.rs](../rust/crates/dvda-cli/tests/process_native.rs)、[process.rs](../rust/crates/dvda-core/src/process.rs)、[buildlog.rs](../rust/crates/dvda-core/src/buildlog.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 045 | `NormalizeAlbumParameters` | [preparation_native.rs](../rust/crates/dvda-core/tests/preparation_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 046 | `PrepareNamingHelpers` | [preparation_native.rs](../rust/crates/dvda-core/tests/preparation_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 047 | `() => { ScanDecodeErrors(); AudioMigrationTests.Run(); }` | [audio_native.rs](../rust/crates/dvda-core/tests/audio_native.rs)、[audio.rs](../rust/crates/dvda-core/src/audio.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 048 | `SortBuildTracks` | [manifest.rs](../rust/crates/dvda-core/src/manifest.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 049 | `SplitAlbumsAcrossDiscs` | [disc.rs](../rust/crates/dvda-core/src/disc.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 050 | `KeepAlbumOnOneDisc` | [disc.rs](../rust/crates/dvda-core/src/disc.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 051 | `GroupAtAlbumBoundaries` | [disc.rs](../rust/crates/dvda-core/src/disc.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 052 | `DiagnoseOversizedAlbumGroup` | [disc.rs](../rust/crates/dvda-core/src/disc.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 053 | `MlpEncoderTests.EmbeddedCore` | [encoder_native.rs](../rust/crates/dvda-core/tests/encoder_native.rs)、[test-rust-package.py](../tools/win-build/test-rust-package.py)、[test-rust-onefile.py](../tools/win-build/test-rust-onefile.py) | 保留原生编码库身份/缓存修复；.NET 嵌入资源格式排除。 |
| 054 | `ResolveExternalMlp` | [acquisition.rs](../rust/crates/dvda-core/src/acquisition.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 055 | `WriteMlpIndex` | [index.rs](../rust/crates/dvda-core/src/index.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 056 | `PreserveFormalMlpIndex` | [application_native.rs](../rust/crates/dvda-core/tests/application_native.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 057 | `BuildDvdaAuthorArguments` | [author.rs](../rust/crates/dvda-core/src/author.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 058 | `BuildDiagnosticTitleModes` | [author.rs](../rust/crates/dvda-core/src/author.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 059 | `LimitDiagnosticAlbums` | [disc.rs](../rust/crates/dvda-core/src/disc.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 060 | `BuildInProcessIsoArguments` | [author.rs](../rust/crates/dvda-core/src/author.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 061 | `WriteCompatibleBuildLog` | [buildlog.rs](../rust/crates/dvda-core/src/buildlog.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 062 | `PublishIso` | [publication_parity.rs](../rust/crates/dvda-core/tests/publication_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 063 | `StageIsoWithoutCopy` | [publication_parity.rs](../rust/crates/dvda-core/tests/publication_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 064 | `PublishDiscSetTransactionally` | [publication_parity.rs](../rust/crates/dvda-core/tests/publication_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 065 | `PublishMovedDiscSetTransactionally` | [publication_parity.rs](../rust/crates/dvda-core/tests/publication_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 066 | `ParseAlacMagicCookie` | [preparation_native.rs](../rust/crates/dvda-core/tests/preparation_native.rs)、[native_format_parity.rs](../rust/crates/dvda-core/tests/native_format_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 067 | `PatchAlacEndMarker` | [preparation_native.rs](../rust/crates/dvda-core/tests/preparation_native.rs)、[native_format_parity.rs](../rust/crates/dvda-core/tests/native_format_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 068 | `PatchAlacWithExtraBits` | [preparation_native.rs](../rust/crates/dvda-core/tests/preparation_native.rs)、[native_format_parity.rs](../rust/crates/dvda-core/tests/native_format_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 069 | `PreserveM4aTagValues` | [conversion_native.rs](../rust/crates/dvda-core/tests/conversion_native.rs)、[conversion.rs](../rust/crates/dvda-core/src/conversion.rs)、[flac_metadata.rs](../rust/crates/dvda-core/tests/flac_metadata.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 070 | `M4aDryRunDoesNotWrite` | [conversion_native.rs](../rust/crates/dvda-core/tests/conversion_native.rs)、[conversion.rs](../rust/crates/dvda-core/src/conversion.rs)、[flac_metadata.rs](../rust/crates/dvda-core/tests/flac_metadata.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 071 | `RejectNonAlacM4a` | [conversion_native.rs](../rust/crates/dvda-core/tests/conversion_native.rs)、[conversion.rs](../rust/crates/dvda-core/src/conversion.rs)、[flac_metadata.rs](../rust/crates/dvda-core/tests/flac_metadata.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 072 | `IsolateM4aFileFailures` | [conversion_native.rs](../rust/crates/dvda-core/tests/conversion_native.rs)、[conversion.rs](../rust/crates/dvda-core/src/conversion.rs)、[flac_metadata.rs](../rust/crates/dvda-core/tests/flac_metadata.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 073 | `ParseFlacPictureDescriptor` | [conversion_native.rs](../rust/crates/dvda-core/tests/conversion_native.rs)、[conversion.rs](../rust/crates/dvda-core/src/conversion.rs)、[flac_metadata.rs](../rust/crates/dvda-core/tests/flac_metadata.rs) | 结构化原生 PICTURE 字段取代旧外部文本描述解析，不恢复外部工具。 |
| 074 | `PreserveConversionOnDeleteFailure` | [conversion_native.rs](../rust/crates/dvda-core/tests/conversion_native.rs)、[conversion.rs](../rust/crates/dvda-core/src/conversion.rs)、[flac_metadata.rs](../rust/crates/dvda-core/tests/flac_metadata.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 075 | `AnalyzeAllAobSegments` | [aob.rs](../rust/crates/dvda-core/src/aob.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 076 | `ValidateAobPtsDamageFixtures` | [aob.rs](../rust/crates/dvda-core/src/aob.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 077 | `ParseAnsiAuditLog` | [verification.rs](../rust/crates/dvda-core/src/verification.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 078 | `ParseLatestFormalAuditLog` | [verification.rs](../rust/crates/dvda-core/src/verification.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 079 | `SelectCurrentIsoNames` | [verification.rs](../rust/crates/dvda-core/src/verification.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 080 | `SelectNewestAuditLog` | [verification.rs](../rust/crates/dvda-core/src/verification.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 081 | `DetectExternalMlpAmbiguity` | [acquisition.rs](../rust/crates/dvda-core/src/acquisition.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 082 | `RejectDuplicateMlpIndexKeys` | [verify.rs](../rust/crates/dvda-core/src/verify.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 083 | `BuildDiscEndToEnd` | [application_native.rs](../rust/crates/dvda-core/tests/application_native.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 084 | `StageDiscAndKeepIntermediate` | [application_native.rs](../rust/crates/dvda-core/tests/application_native.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 085 | `PreserveFailedDiscWorkspace` | [application_native.rs](../rust/crates/dvda-core/tests/application_native.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 086 | `LoadMenuConfiguration` | [options_parity.rs](../rust/crates/dvda-core/tests/options_parity.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 087 | `PlanAlbumMenuPages` | [menu_planning_parity.rs](../rust/crates/dvda-core/tests/menu_planning_parity.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 088 | `SanitizeMenuText` | [menu_planning_parity.rs](../rust/crates/dvda-core/tests/menu_planning_parity.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 089 | `BuildMenuAuthorArguments` | [author.rs](../rust/crates/dvda-core/src/author.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 090 | `ResolveMenuFontRules` | [menu_fonts.rs](../rust/crates/dvda-core/src/menu_fonts.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 091 | `ExtractOpenTypeCollectionFaces` | [fonts.rs](../rust/crates/dvda-toolchain/src/fonts.rs)、[font-validation.json](../build/rust-font-migration/font-validation.json) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 092 | `NativeOptimizerTests.Run` | [pe.rs](../rust/crates/dvda-toolchain/src/pe.rs)、[validation.rs](../rust/crates/dvda-toolchain/src/validation.rs)、[main.rs](../rust/crates/dvda-toolchain/src/main.rs)、[test-rust-package.py](../tools/win-build/test-rust-package.py) | 保留普通/延迟 PE 导入与依赖完整性；旧便携二进制包硬编码裁剪规则排除。 |
| 093 | `() => { VerifyMenuVisualThresholds(); ImageMigrationTests.Run(); }` | [menu_planning_parity.rs](../rust/crates/dvda-core/tests/menu_planning_parity.rs)、[images_native.rs](../rust/crates/dvda-core/tests/images_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 094 | `ParseBatchedMenuStatistics` | [menu.rs](../rust/crates/dvda-core/src/menu.rs)、[menu_planning_parity.rs](../rust/crates/dvda-core/tests/menu_planning_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 095 | `BatchMenuProcessCalls` | [application_native.rs](../rust/crates/dvda-core/tests/application_native.rs)、[menu_check.rs](../rust/crates/dvda-core/src/menu_check.rs) | 保留每页批量统计/失败回退；原进程计数替换为进程内图像调用，不再启动 ImageMagick。 |
| 096 | `VerifyMultiIndexPageLayout` | [menu_planning_parity.rs](../rust/crates/dvda-core/tests/menu_planning_parity.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 097 | `ValidateAmgCellChainFixtures` | [menu_verify.rs](../rust/crates/dvda-core/src/menu_verify.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 098 | `ValidateAsvsFixtures` | [menu_verify.rs](../rust/crates/dvda-core/src/menu_verify.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 099 | `BuildScriptBranching` | [test-rust-build-entrypoints.py](../tools/win-build/test-rust-build-entrypoints.py) | 保留 prepare→build、dry-run 开发开关和 JSON profile；旧 --config env 路径排除。 |
| 100 | `BuildScriptStopsAfterPrepareFailure` | [test-rust-build-entrypoints.py](../tools/win-build/test-rust-build-entrypoints.py) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 101 | `ComparePcmFixtures` | [format_parity.rs](../rust/crates/dvda-native/src/format_parity.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 102 | `EstimateDiskSpaceRequirements` | [workflow.rs](../rust/crates/dvda-core/src/workflow.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 103 | `PrepareCacheReuseAndInvalidation` | [preparation_native.rs](../rust/crates/dvda-core/tests/preparation_native.rs)、[snapshot.rs](../rust/crates/dvda-core/src/preparation/snapshot.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 104 | `PrepareCacheSkipsFailedTracks` | [preparation_native.rs](../rust/crates/dvda-core/tests/preparation_native.rs)、[snapshot.rs](../rust/crates/dvda-core/src/preparation/snapshot.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 105 | `PrepareCacheIdentityAndStorage` | [preparation_native.rs](../rust/crates/dvda-core/tests/preparation_native.rs)、[snapshot.rs](../rust/crates/dvda-core/src/preparation/snapshot.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 106 | `PreparationSnapshotChecks` | [preparation_native.rs](../rust/crates/dvda-core/tests/preparation_native.rs)、[snapshot.rs](../rust/crates/dvda-core/src/preparation/snapshot.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 107 | `ScanPtsSectorsStreamingly` | [aob.rs](../rust/crates/dvda-core/src/aob.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 108 | `SharedAobScanPreservesDiagnostics` | [aob.rs](../rust/crates/dvda-core/src/aob.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 109 | `ReuseValidatedSurcodeOutput` | [encoder_native.rs](../rust/crates/dvda-core/tests/encoder_native.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 110 | `MlpEncoderTests.Cache` | [encoder_native.rs](../rust/crates/dvda-core/tests/encoder_native.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 111 | `MlpCacheIndexStorage` | [cache.rs](../rust/crates/dvda-core/src/cache.rs)、[mlp_workflow.rs](../rust/crates/dvda-core/src/mlp_workflow.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 112 | `DiscResumeStoreRules` | [resume.rs](../rust/crates/dvda-core/src/resume.rs)、[signature.rs](../rust/crates/dvda-core/src/signature.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 113 | `DiscSignatureChanges` | [resume.rs](../rust/crates/dvda-core/src/resume.rs)、[signature.rs](../rust/crates/dvda-core/src/signature.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 114 | `BuildPipelineResumesDiscs` | [application_native.rs](../rust/crates/dvda-core/tests/application_native.rs)、[developer_commands.rs](../rust/crates/dvda-cli/tests/developer_commands.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
| 115 | `() => { MlpEncoderTests.Parallel(); BatchMigrationTests.Run(); }` | [encoder_native.rs](../rust/crates/dvda-core/tests/encoder_native.rs)、[application_native.rs](../rust/crates/dvda-core/tests/application_native.rs) | 按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。 |
