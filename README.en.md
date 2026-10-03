# DVD-Audio Maker

[简体中文](README.md) | [English](README.en.md)

A Windows x64 application that turns FLAC and ALAC/M4A sources into **DVD-Audio ISOs**, with a bilingual GUI, MLP encoding, disc planning, optional menus and output verification.

The default release is **one Windows x64 GUI EXE without the .NET runtime**. Install [.NET 10 Desktop Runtime for Windows x64](https://dotnet.microsoft.com/download/dotnet/10.0), then run `DVD-Audio-Maker.exe`. The plain .NET Runtime, ASP.NET Core Runtime or .NET Framework 4.x alone is insufficient. Historical directory packages still require complete extraction.

## Quick start

1. Select the source, working and ISO output directories; set the title and capacity.
2. Choose the target sample rate, bit depth and optional menus. Keep each album in its own folder with album, title, track and date tags.
3. Check the sources, then preview the build. Preview prepares sources and encodes MLP but does not create ISOs.
4. Build the discs and verify every track, menu and output ISO.

The release ZIP contains the single-file EXE plus separate instructions, example configuration, licenses and component notices. Only required runtime components and complete CJK fonts are embedded in the EXE, extracted into `%LOCALAPPDATA%/DVD-Audio-Maker/runtime` on first launch, then verified/reused. Media processing and authoring share one FFmpeg source build, with one copy of each DLL. Close all instances before clearing this cache; it is restored automatically. Retain the license notices. `config.env.example` is not loaded automatically; existing `config.env` files remain importable. See [single-file builds](docs/ONEFILE-PUBLISH.md).

## Configuration, language and logs

The GUI replaces manual env editing while retaining `config.env` import and JSON profile loading/saving. Daily settings live in `%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`. The CLI and `--config` remain available.

The upper-right language selector switches the UI and task logs between Chinese and English without changing paths, track tags, settings or encoded data. The initial language follows Windows; switching is disabled while a task runs. GUI and CLI accept `--language en`, `--language zh-CN`, `--language auto` and `DVDA_LANGUAGE`.

Logs default to stage summaries, with detailed output, warning filters, paused display, copy and export options. Tasks can be canceled. Full logs and startup errors are stored in `%LOCALAPPDATA%/DVD-Audio-Maker/logs`.

## Encoding and tools

| Task | Implementation |
|---|---|
| Source reading, conversion, decoding and media verification | In-process x64 media libraries; no FFmpeg/FFprobe EXE |
| MLP encoding | Embedded mlpencoder DLL; no original SurCode or eac3to |
| Covers, text, menu images, fonts and image verification | The GUI and native author each call the tailored image DLL in process |
| Menu encoding/muxing, authoring and ISO creation | Project author with in-process C menu modules and ISO writer |
| Optional M4A/ALAC-to-FLAC organization | In-process FLAC metadata editor handles artwork and tags |

No separate FFmpeg, FFprobe or ImageMagick installation is needed. The GUI replaces imported legacy media paths with built-in components; the developer CLI and reference tests can still use explicit external converters. Images support JPEG/PNG reading and writing plus WebP cover reading, with complete SC/JP/KR font faces. General video, PDF/SVG and other image delegate chains are excluded.

The batch interface supports one to six channels at 44.1, 48, 88.2 and 96 kHz, and mono/stereo at 176.4 and 192 kHz, with 16-, 20- or 24-bit target samples. Input channel layouts are retained. Mixed-rate/depth channel groups are not exposed as GUI settings. Some high-noise material can still exceed available MLP stream limits; it is not made to fit through lossy processing.

Byte-identical MLP requires **identical target PCM, encoding parameters and auxiliary metadata context**. Encoded output is not patched. Identical audio with different historical metadata does not imply identical complete files. See [mlpencoder integration](docs/MLP-ENCODER.en.md) and [current media processing](docs/INPROCESS-MEDIA.en.md).

## Source development

The application uses .NET 10 / C#. The GUI, CLI and native encoding core target Windows x64. Development needs the .NET 10 SDK plus matching native libraries, authoring tools and assets; the Git repository does not contain the complete external tool bundle.

```bat
gui-debug.cmd
cli.cmd config
dotnet build DVD-Audio-Maker.sln -c Debug -p:SelfContained=false
```

VS Code retains F5 profiles for the GUI, CLI and compatibility tests. Routine C# changes reuse verified native components; native maintenance additionally needs Python, MSYS2/MinGW-w64 and development libraries. See [development/debugging](docs/DEVELOPMENT.en.md) and [Windows builds](tools/win-build/README.en.md).

| Directory | Purpose |
|---|---|
| `src/DvdaMaker.Desktop` / `Cli` | GUI and developer command-line entry points |
| `src/DvdaMaker.Configuration` / `Localization` | Settings, profiles and bilingual resources |
| `src/DvdaMaker.Preparation` / `Building` | Source preparation, MLP, disc planning, menus and verification |
| `src/DvdaMaker.Processes` | In-process media/images and remaining external process management |
| `src/DvdaMaker.SurcodeTool/Native` | Pinned mlpencoder sources and x64 DLL |
| `src/DvdaMaker.Formats` / `FontTool` | Stream parsing and shared font utilities |
| `src/DvdaMaker.Toolchain` / `tools/win-build` | Native builds, packaging and regression scripts |
| `tests` / `docs` | Compatibility tests, designs and validation records |
| `build` | Git-ignored local components, packages, fixtures and caches |

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

verify lossless checks every disc, group and track: complete target PCM is compared, and a read-only C parser checks every MLP byte in all AOB segments inside the ISO. Batch-surcode regenerates target PCM using the same SWR, bit-depth conversion and WAV normalization as encoding. Equal length is required by default; SurCode only permits complete zero-valued tail frames shorter than 1 ms. Truncation, nonzero tails and differences fail. Unknown conversion policies for old external MLP cannot pass through similar sample counts. Verification never modifies encoded files.

## Reruns and caches

| Setting | Default | Purpose |
| --- | --- | --- |
| `DVDA_PREPARE_CACHE` | `on` | Source probing and decoding-check cache at `<DVDA_BUILD_DIR>/prepare-cache.json` |
| `DVDA_RESUME` | `on` | Per-disc resumption, recorded at `<DVDA_BUILD_DIR>/publish-staging/resume.json` |
| `DVDA_MLP_JOBS` | `1` | MlpEncoder encoder concurrency, from 1 to 16 jobs |
| `DVDA_KEEP_TMP` | `off` | Keep `<DVDA_BUILD_DIR>/tmp` for diagnostics; disables automatic cleanup |
| `DVDA_KEEP_INTERMEDIATE` | `off` | Keep authoring output and intermediate ISOs; **disables per-disc resumption while enabled** |

- Source caches are reused only when file identity (length, modification time and hashes of the first and last 64 KiB) and normalization parameters match exactly. Tracks that fail verification are never cached.
- Per-disc resumption skips a disc only when its signature matches (source/MLP identity, author/ISO-writer identity, output-affecting settings and menu configuration) and its staged ISO is unchanged. Final publication still commits the entire ISO set and index as one transaction. Failed builds retain `<DVDA_BUILD_DIR>/publish-staging` for the next attempt.
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

### MlpEncoder-core batch encoding (default)

```text
DVDA_MLP_SOURCE="surcode-batch"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
DVDA_MLP_BATCH_TEMP_DIR="D:/dvda-surcode/temp"
DVDA_MLP_BATCH_OUTPUT_DIR="D:/dvda-surcode/output"
DVDA_MLP_METADATA_CONTEXT=""
DVDA_FFMPEG="builtin:media"
DVDA_FFPROBE="builtin:probe"
DVDA_MLP_SURCODE_SAMPLE_RATE="48000"
DVDA_MLP_SURCODE_BITS="24"
```

The `surcode-batch` configuration name is retained for existing jobs. The execution chain is:

`Source audio → in-process media DLLs → integer PCM → mlpencoder MLP core DLL → MLP cache`.

The Windows x64 application embeds a pinned Windows x64 encoder DLL, extracts and verifies it at runtime, then calls it in-process.
It does not start the original SurCode, load its DLLs, write SSF files or patch encoded output bytes.
The old DVDA_MLP_EAC3TO_EXE setting remains readable but is unused. The GUI automatically uses the bundled media libraries, ignoring legacy FFmpeg / FFprobe paths. Resampling and bit-depth reduction retain the validated SWR and 20-bit quantization behavior; eac3to-identical PCM is not promised. Identical target PCM and metadata must still yield byte-identical MLP. See [in-process media](docs/INPROCESS-MEDIA.en.md) for the current design and validation, and [FFmpeg PCM migration](docs/FFMPEG-PCM-MIGRATION.en.md) for the earlier migration.

An empty MLP cache directory defaults to `<DVDA_BUILD_DIR>/mlp`. The former `DVDA_MLP_SOURCE=ffmpeg` value reports an explicit error;
change it to `surcode-batch`. `DVDA_MLP_SURCODE_EXE` has been removed.

A fixed empty auxiliary TLV is used by default, making output reproducible for identical PCM and settings. For byte-for-byte comparison with a historical original file,
supply the track's complete `DVDA_MLP_METADATA_CONTEXT`. Different timestamp metadata
produces a different complete file; identical audio alone does not imply identical bytes. Reference audio payloads are never fed to the encoder.
Old caches without provenance credentials are rebuilt. A failed re-encode does not overwrite an existing valid output.

See [MlpEncoder-core integration](docs/MLP-ENCODER.en.md) for interfaces, precision, channels, caches and reproduction details.

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

The bundled TTC registers separate SC, JP and KR faces through type.xml. The GUI selects regional faces automatically. For custom fonts, specify registered regional names or the corresponding OTF files.

`DVDA_FINAL_DIR` is the final output directory. After a successful build, ISOs are published directly there without an additional copy stage.

## Build the compact package

Prepare the media/image libraries, rebuilt native author and assets following [the build guide](tools/win-build/README.en.md), then run:

```bat
tools\win-build\build-all.cmd ^
  --framework-dependent ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --source "D:\dev\winbuild\src"
```

Replace the example paths with your prepared Windows tools and full asset tree. Output defaults to `tools/win-build/release-framework-dependent/DVD-Audio-Maker` and its sibling ZIP; `--output` selects another location. The standard package contains only the GUI. `--include-cli` adds developer diagnostics. Omitting `--framework-dependent` still produces a self-contained package, with the size cost of bundling .NET.

Packaging reuses existing Windows artifacts rather than invoking the native compiler chain. Generated releases, source audio, caches and ZIPs stay out of Git; distributable archives belong in GitHub Releases.

## Validation and maintenance

```bat
dotnet build DVD-Audio-Maker.sln -c Debug -p:SelfContained=false
tests\DvdaMaker.CompatibilityTests\bin\Debug\net10.0-windows\win-x64\DvdaMaker.CompatibilityTests.exe
```

The migration passes 109/109 compatibility checks, 41 menu-media checks, 32 native-menu checks and 14 full-stream corruption checks. GUI tests cover regular/indexed menus, multiple discs/groups and 44.1 kHz / 20-bit conversion. Source-built menu modules replace the old menu executables. MLP core identity and whole-file byte equality remain; MPEG-2 menu bytes need not match the former encoder.

The validated compact package is under build/release-menu-final; artifacts stay ignored. See the [migration checklist](docs/NO-EXTERNAL-RUNTIME-MIGRATION.en.md) and [validation record](docs/menu-migration-validation.json). Historical baselines remain development references.

## Documentation

- [Development/debugging](docs/DEVELOPMENT.en.md), [native builds/packaging](tools/win-build/README.en.md)
- [MlpEncoder core and byte identity](docs/MLP-ENCODER.en.md), [native-core maintenance](src/DvdaMaker.SurcodeTool/Native/README.md)
- [In-process media](docs/INPROCESS-MEDIA.en.md), [in-process images](docs/INPROCESS-IMAGES.en.md)
- [Minimal media libraries](docs/MINIMAL-FFMPEG.en.md), [shared fonts](docs/SHARED-FONTS.en.md)
- [Author changes](docs/DVDA-AUTHOR-CHANGES.en.md), [historical troubleshooting](docs/TROUBLESHOOTING.en.md)
- [Image validation record](docs/inprocess-images-validation.json), [third-party licensing](docs/LICENSING.en.md)

## License

Repository code is distributed under [GPL-3.0](LICENSE). Third-party sources, tools, fonts and audio remain subject to their respective licenses. Original SurCode is not distributed with this project.
