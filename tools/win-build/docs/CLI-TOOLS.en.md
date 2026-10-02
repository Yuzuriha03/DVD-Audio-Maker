# Developer CLI diagnostics

This document applies only to a developer diagnostic package built with `--include-cli`. Standard user releases do not include `dvda.exe` or `dvda.cmd`. Use `cli.cmd` in the source checkout. See RUNTIME.en.md: framework-dependent packages require .NET 10 Desktop Runtime x64.

[简体中文](README.md) | [English](README.en.md)


## Interface and log language

Use the language selector at the top right to switch between Chinese and English. The interface and task logs follow the selected language, which is saved with JSON profiles and everyday settings. On first launch, the Windows UI language determines the default. Switching language preserves paths, track tags, configuration values and encoded data. The selector is disabled while a task is running.

Both GUI and CLI accept `--language en`, `--language zh-CN` or `--language auto`. You can also set `DVDA_LANGUAGE`. The command-line option takes precedence over that environment variable; the GUI then falls back to its saved preference and finally the system language. CLI `--shell` / `--shell-all` output stays machine-readable and does not translate configuration values.

```bat
DVD-Audio-Maker.exe --language en
dvda.cmd config --language en
```

Double-click `DVD-Audio-Maker.exe` in the package root, or run `dvda.cmd` without arguments, to open the GUI. Command arguments continue to invoke the CLI.

GUI and CLI share application dependencies; the CLI is `dvda.exe` in the package root. Extract the entire package and retain its DLLs, JSON files and resource directories alongside the executables. Do not copy an EXE alone. See RUNTIME.en.md for .NET installation requirements.

Edit settings in the interface; they are saved automatically in your user directory. "Import env…" reads existing configurations, and "Open profile / Save profile" handles JSON profiles. Without saved GUI settings, first launch reads an adjacent config.env. MLP uses a native x64 in-process DLL, with no encoder EXE. FFmpeg conversion/decoding and disc-authoring tools remain external programs.

"Preview layout" prepares sources and encodes MLP without creating ISOs. "Build discs" completes authoring. "Verify output" retains first-track sampling for lossless PCM checks. Tasks can be canceled and logs exported.

Use "More settings" for less common options. Choose DVD5, DVD9 or custom capacity; sample rates and encoding methods have readable labels.

Logs show a task summary by default. Switch to detailed logs, filter issues, pause live updates, copy content or export the complete task log. Drag the divider to resize the log area. Full logs are saved under `%LOCALAPPDATA%/DVD-Audio-Maker/logs` and retain external-tool diagnostics. The window keeps only recent entries.

# DVD-Audio Maker for Windows

This document ships with the self-contained Windows x64 release.

GUI and CLI use the embedded native x64 MLP DLL. The package also includes `dvda-author`, `mkisofs`, menu tools, ImageMagick and Chinese/Japanese/Korean fonts. WSL, Bash, MSYS2, PowerShell and Python are not needed; see RUNTIME.en.md for .NET requirements.

## External dependencies

FFmpeg, FFprobe and Metaflac must be on `PATH` or configured with full paths in `config.env`.

FFmpeg performs conversion, decoding and verification. Batch encoding uses the embedded MLP DLL and requires neither eac3to nor original SurCode.

## Existing configurations and CLI

Use "Import env…" for existing configurations, or edit everyday settings directly in the GUI. For CLI use, you can edit `config.env` in the package root:

```text
DVDA_SRC="D:/Music/MyAlbums"
DVDA_FINAL_DIR="D:/DVD_Output"
DVDA_BUILD_DIR="D:/DVD_Output/_work"
DVDA_TITLE="My DVD-Audio Collection"
DVDA_ISO_PREFIX="MyCollection"
DVDA_MAX_DISCS="2"
DVDA_MLP_SOURCE="surcode-batch"
DVDA_MLP_EXTERNAL_DIR=""
DVDA_FFMPEG="C:/Tools/ffmpeg/bin/ffmpeg.exe"
DVDA_MENU="on"
```

CLI configuration priority:

```text
Environment variables > selected configuration file > built-in defaults
```

Select a file with `--config` or `DVDA_CONFIG`; the default is `config.env`. Parsing accepts only `KEY=VALUE`, with no command execution or variable expansion. Use absolute Windows paths.

GUI or `dvda.cmd` sets tool and menu-font paths according to package layout; manual configuration is normally unnecessary.

## Build ISO images

```bat
dvda.cmd config --check
dvda.cmd prepare
dvda.cmd build --dry-run
dvda.cmd build
dvda.cmd verify all
```

## Source audio

FLAC, M4A/ALAC and JPG, PNG or WebP artwork are supported. Use one subdirectory per album and supply `date`, `track`, `album` and `title` tags.

## MLP source

Default MLP core:

```text
DVDA_MLP_SOURCE="surcode-batch"
DVDA_MLP_EXTERNAL_DIR=""
DVDA_FFMPEG="C:/Tools/ffmpeg/bin/ffmpeg.exe"
```

External MLP:

```text
DVDA_MLP_SOURCE="external"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
```

MLP core batch encoding (retaining existing setting names):

```text
DVDA_MLP_SOURCE="surcode-batch"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
DVDA_MLP_BATCH_TEMP_DIR="D:/dvda-surcode/temp"
DVDA_MLP_BATCH_OUTPUT_DIR="D:/dvda-surcode/output"
DVDA_MLP_METADATA_CONTEXT=""
DVDA_FFMPEG="C:/Tools/ffmpeg/bin/ffmpeg.exe"
```

## Verification modes

```bat
dvda.cmd verify quick
dvda.cmd verify capacity
dvda.cmd verify audit
dvda.cmd verify menu
dvda.cmd verify timeline
dvda.cmd verify lossless
dvda.cmd verify all
```

## Troubleshooting

### External program not found

Set full paths in `config.env`:

```text
DVDA_FFMPEG="D:/Tools/ffmpeg/bin/ffmpeg.exe"
DVDA_FFPROBE="D:/Tools/ffmpeg/bin/ffprobe.exe"
DVDA_METAFLAC="D:/Tools/flac/metaflac.exe"
```

### Missing menu text or wrong glyphs

Do not move or delete `menu-bin\fonts`. The launcher automatically configures separate SC, JP and KR font paths.

### `verify audit` reports missing logs

`audit` requires the `build.log` from an actual build. A preview log cannot replace it.

## License

Project code is distributed under the included `LICENSE`. Third-party components retain their own licenses; see `THIRD-PARTY.md`.

The MLP core is embedded and SHA-256-verified. Encoded output is not patched. Auxiliary metadata is fixed to empty by default.
Historical original byte comparisons require matching PCM, settings and explicit metadata context. Empty context does not impersonate historical timestamps.
