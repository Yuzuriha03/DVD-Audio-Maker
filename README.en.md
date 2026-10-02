# DVD-Audio Maker

[简体中文](README.md) | [English](README.en.md)


## Interface and log language

Use the language selector at the top right to switch between Chinese and English. The interface and task logs follow the selected language, which is saved with JSON profiles and everyday settings. On first launch, the Windows UI language determines the default. Switching language preserves paths, track tags, configuration values and encoded data. The selector is disabled while a task is running.

Both GUI and CLI accept `--language en`, `--language zh-CN` or `--language auto`. You can also set `DVDA_LANGUAGE`. The command-line option takes precedence over that environment variable; the GUI then falls back to its saved preference and finally the system language. CLI `--shell` / `--shell-all` output stays machine-readable and does not translate configuration values.

```bat
DVD-Audio-Maker.exe --language en
dvda.cmd config --language en
```

## Graphical interface (Windows x64)

In a source checkout, double-click `gui.cmd`. In the self-contained release, open `DVD-Audio-Maker.exe` in the package root. The GUI lets you select folders, configure encoding and menus, check sources, preview disc layouts, build discs and verify output, with cancellation and live logs.

Use "Import env…" to read an existing configuration, or open/save JSON profiles. Everyday settings are saved automatically to `%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`; no manual env editing is required. The CLI and `--config` remain available. MLP encoding calls the embedded x64 DLL directly and does not launch an encoder EXE.

Settings are grouped into "Getting started / Audio encoding / Disc menus / Tools & advanced", with advanced options collapsed initially. Choose DVD5, DVD9 or a custom capacity. The lower pane shows task summaries, the current stage, elapsed time and next steps. Drag the divider to resize the log area.

Switch to detailed output, filter issues, pause live updates, copy the current view or export the full task log. Normal tool progress is not treated as failure; errors and warnings remain visible. Complete task logs are saved automatically under `%LOCALAPPDATA%/DVD-Audio-Maker/logs`, while the window retains only recent entries to limit memory use. Startup failures are logged there too.

Implementation plan and scope: [GUI and DLL plan](docs/GUI-AND-DLL-PLAN.en.md).


A native Windows toolchain for creating standard **DVD-Audio ISO** images from FLAC or ALAC/M4A audio.

The project uses **.NET 10 / C#** for source preparation, MLP management, disc planning, menu generation, ISO publication and output verification. It uses a modified `dvda-author` and a `mkisofs` build supporting `-dvd-audio` underneath.

> The repository includes the MLP encoder source and pinned Windows binaries. It does not include the original SurCode application, audio content or other external tools.

## Features

- Recursively scan FLAC and ALAC/M4A files
- Read album, track title, track number and date tags
- Check decoding completeness, channel counts and audio parameters
- Repair missing END markers in certain Apple ALAC files
- Normalize sample rate and bit depth by album
- Encode with the MLP core or import external MLP files
- Keep albums intact and fill discs sequentially by capacity
- Optional DVD-Audio AMG track menus and ASVS playback artwork
- Publish the complete ISO set transactionally
- Verify ISO, IFO, AOB, PTS, menus, capacity and a first-track lossless sample
- Self-contained Windows x64 release

## Project layout

```text
src/DvdaMaker.Desktop         Windows x64 graphical entry point
src/DvdaMaker.Cli             Command-line entry point
src/DvdaMaker.Configuration   GUI JSON profiles and config.env parsing
src/DvdaMaker.Localization    Chinese/English interface and log resources
src/DvdaMaker.Preparation     Scanning, normalization, decoding checks, ALAC repair
src/DvdaMaker.Building        MLP, disc planning, menus, authoring, publication, verification
src/DvdaMaker.Formats         ISO9660, MLP and MPEG/PTS parsers
src/DvdaMaker.Processes       External process execution
src/DvdaMaker.SurcodeTool     FFmpeg PCM preparation and Windows MLP core
src/DvdaMaker.FontTool        OpenType/TTC font utilities
src/DvdaMaker.Toolchain       Windows release package assembler
tests/                        Compatibility and end-to-end tests
```

Everyday use and release assembly do not require WSL, PowerShell or Bash. Launch the GUI with `gui.cmd` in the source checkout or `DVD-Audio-Maker.exe` in the release package. `build.cmd`, `verify.cmd` and the C# CLI remain available.

## Prerequisites

Running from source requires:

- Windows 10/11 x64
- .NET 10 SDK
- FFmpeg, FFprobe and Metaflac
- Prebuilt Windows `dvda-author-dev.exe` and `mkisofs.exe`
- Menu tools, ImageMagick, fonts and runtime assets when menus are enabled

FFmpeg is used for source conversion, decoding and verification, not MLP encoding.
Batch encoding uses the FFmpeg specified by DVDA_FFMPEG, resolving it through PATH by default. Neither eac3to nor the original SurCode is required.

## CLI and existing env configurations

Edit GUI settings on screen and save them as JSON. The priority below applies only to the CLI. The repository includes an example `config.env` suitable for version control. Use "Import env…" or `--config` to select an existing configuration.

Configuration value priority:

```text
Environment variables > selected configuration file > built-in defaults
```

Configuration files are selected in this order: `--config`, `DVDA_CONFIG`, then `config.env`. Built-in defaults are used when no configuration file is found.

Minimal configuration example:

```text
DVDA_SRC="D:/Music/MyAlbums"
DVDA_FINAL_DIR="D:/DVD_Output"
DVDA_BUILD_DIR="D:/DVD_Output/_work"
DVDA_TITLE="My DVD-Audio"
DVDA_ISO_PREFIX="MyCollection"
DVDA_MAX_DISCS="2"
DVDA_MENU="off"
```

Configuration parsing accepts only `KEY=VALUE`; it does not execute commands or expand variables. Use absolute Windows paths. Both forward and backslashes are accepted.

Check configuration:

```bat
dotnet run --project src\DvdaMaker.Cli -- config
dotnet run --project src\DvdaMaker.Cli -- config --check
```

## Building and verification

```bat
build.cmd --dry-run
build.cmd
verify.cmd
```

Select another configuration:

```bat
build.cmd --dry-run --config "D:\Config\dvda.env"
build.cmd --config "D:\Config\dvda.env"
verify.cmd all --config "D:\Config\dvda.env"
```

You can also invoke the CLI directly:

```bat
dotnet run --project src\DvdaMaker.Cli -- prepare
dotnet run --project src\DvdaMaker.Cli -- plan
dotnet run --project src\DvdaMaker.Cli -- build --dry-run
dotnet run --project src\DvdaMaker.Cli -- build
dotnet run --project src\DvdaMaker.Cli -- verify all
```

`plan` reads the existing `manifest.json` and MLP sizes without acquiring or encoding MLP. Use it for a quick disc-layout preview.
`build --dry-run` performs the full MLP acquisition workflow and writes a separate preview index. It is not a zero-write operation.
`prepare --force` bypasses source verification caches and repeats probing and decoding checks.
`build --no-resume` disables per-disc resumption and rebuilds every disc.

`verify lossless` samples only **disc 1 / group 1 / track 1**. It compares source-decoded PCM with MLP-decoded PCM byte for byte, then extracts the first-track MLP from the completed ISO and compares it with the source MLP. Equal length is required by default. Only SurCode mode permits identical shared content followed by less than 1 ms of complete, zero-valued sample frames. Truncation, nonzero tails and content differences remain errors. This is not a lossless check of every track on every disc.

## Reruns and caches

| Setting | Default | Purpose |
| --- | --- | --- |
| `DVDA_PREPARE_CACHE` | `on` | Source probing and decoding-check cache at `build/prepare-cache.json` |
| `DVDA_RESUME` | `on` | Per-disc resumption, recorded at `build/publish-staging/resume.json` |
| `DVDA_MLP_JOBS` | `1` | MLP encoder concurrency, from 1 to 16 jobs |
| `DVDA_KEEP_TMP` | `off` | Keep `build/tmp` for diagnostics; disables automatic cleanup |
| `DVDA_KEEP_INTERMEDIATE` | `off` | Keep authoring output and intermediate ISOs; **disables per-disc resumption while enabled** |

- Source caches are reused only when file identity (length, modification time and hashes of the first and last 64 KiB) and normalization parameters match exactly. Tracks that fail verification are never cached.
- Per-disc resumption skips a disc only when its signature matches (source/MLP identity, author/mkisofs identity, output-affecting settings and menu configuration) and its staged ISO is unchanged. Final publication still commits the entire ISO set and index as one transaction. Failed builds retain `build/publish-staging` for the next attempt.
- `DVDA_MLP_JOBS` above 1 runs multiple independent in-process DLL encoder states concurrently. Cache credentials still include source identity, encoder identity, encoding parameters and output identity. Performance depends on disk throughput and CPU capacity; the default remains one job.

## MLP sources

### External MLP

```text
DVDA_MLP_SOURCE="external"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
```

The external directory must mirror the source directory layout:

```text
Source: D:/Music/MyAlbums/Album/01 Song.flac
MLP ：D:/Music/MLP/Album/01 Song.mlp
```

### MLP core batch encoding (default)

```text
DVDA_MLP_SOURCE="surcode-batch"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
DVDA_MLP_BATCH_TEMP_DIR="D:/dvda-surcode/temp"
DVDA_MLP_BATCH_OUTPUT_DIR="D:/dvda-surcode/output"
DVDA_MLP_METADATA_CONTEXT=""
DVDA_FFMPEG="C:/Tools/ffmpeg/bin/ffmpeg.exe"
DVDA_MLP_SURCODE_SAMPLE_RATE="48000"
DVDA_MLP_SURCODE_BITS="24"
```

The `surcode-batch` configuration name is retained for existing jobs. The execution chain is:

`Source audio → FFmpeg → integer PCM → MLP core DLL → MLP cache`.

The native x64 host embeds a pinned Windows x64 encoder DLL, extracts and verifies it at runtime, then calls it in-process.
It does not start the original SurCode, load its DLLs, write SSF files or patch encoded output bytes.
The old DVDA_MLP_EAC3TO_EXE setting remains readable but is unused. Set FFmpeg on the GUI's "Audio encoding" page. Resampling and bit-depth reduction use FFmpeg's conversion results and do not promise eac3to-identical PCM; identical target PCM and metadata must still yield byte-identical MLP. See [FFmpeg PCM migration](docs/FFMPEG-PCM-MIGRATION.en.md) for the migration plan and acceptance results.

An empty MLP cache directory defaults to `<build>/mlp`. The former `DVDA_MLP_SOURCE=ffmpeg` value reports an explicit error;
change it to `surcode-batch`. `DVDA_MLP_SURCODE_EXE` has been removed.

A fixed empty auxiliary TLV is used by default, making output reproducible for identical PCM and settings. For byte-for-byte comparison with a historical original file,
supply the track's complete `DVDA_MLP_METADATA_CONTEXT`. Different timestamp metadata
produces a different complete file; identical audio alone does not imply identical bytes. Reference audio payloads are never fed to the encoder.
Old caches without provenance credentials are rebuilt. A failed re-encode does not overwrite an existing valid output.

See [MLP core integration](docs/MLP-ENCODER.en.md) for interfaces, precision, channels, caches and reproduction details.

## Disc planning and menus

- Albums remain intact by default, and discs fill in global track order.
- `DVDA_MAX_DISCS` sets only a disc-count ceiling; it does not control splitting.
- Default DVD-5 capacity is `4,707,319,808` bytes.
- Set `DVDA_DISC_BYTES="8540123136"` for DVD-9.
- Each DVD-Audio audio group supports up to 99 tracks.

Example menu configuration:

```text
DVDA_MENU="on"
DVDA_MENU_TRACKS_PER_PAGE="12"
DVDA_MENU_INDEX_MIN_ALBUMS="4"
DVDA_MENU_STILLPICS="on"
DVDA_MENU_COVER_DIM="35"
```

Do not use a TTC collection directly as a menu font. Use separate SC, JP and KR OTF faces.

`DVDA_FINAL_DIR` is the final output directory. After a successful build, ISOs are published directly there without an additional copy stage.

## Self-contained Windows release

The repository no longer runs Autotools, Make, MSYS2, Bash or WSL during release assembly. Third-party C tools must be built beforehand and collected in a Windows directory.

Default prebuilt directory:

```text
tools\win-build\prebuilt\
```

Build the release package:

```bat
tools\win-build\build-all.cmd
```

Specify third-party tools and the runtime asset tree:

```bat
tools\win-build\build-all.cmd ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --source "D:\dev\winbuild\src"
```

Outputs are located at:

```text
tools\win-build\release\DVD-Audio-Maker\
tools\win-build\release\DVD-Audio-Maker.zip
```

See [`tools/win-build/README.md`](tools/win-build/README.en.md) for detailed requirements.

## Development and tests

```bat
dotnet build DVD-Audio-Maker.sln --configuration Release
dotnet run --project tests\DvdaMaker.CompatibilityTests --configuration Release
```

The current compatibility suite contains 107 tests, including English catalog coverage, path preservation and language persistence.

## Documentation

- [MLP encoding and validation results](docs/MLP-ENCODER.en.md): batch execution, metadata settings and byte-comparison coverage

- [`docs/CSHARP-MIGRATION.md`](docs/CSHARP-MIGRATION.en.md): C# migration status and implementation boundaries
- [`docs/DVDA-AUTHOR-CHANGES.md`](docs/DVDA-AUTHOR-CHANGES.en.md): `dvda-author` changes and supporting evidence
- [`docs/DVDA-AUTHOR-DISABLED.md`](docs/DVDA-AUTHOR-DISABLED.en.md): experiments that are not enabled
- [`docs/TROUBLESHOOTING.md`](docs/TROUBLESHOOTING.en.md): historical issues, diagnostics and fixes
- [`docs/LICENSING.md`](docs/LICENSING.en.md): third-party components and licensing

## License

Repository code is distributed under [`GPL-3.0`](LICENSE). Third-party source, tools, fonts, FFmpeg, `dvda-author`, `dvdauthor`, SurCode and audio content remain subject to their respective licenses.
