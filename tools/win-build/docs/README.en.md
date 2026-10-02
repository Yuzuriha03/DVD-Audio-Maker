# DVD-Audio Maker — Windows x64

[简体中文](README.md) | [English](README.en.md)

<!-- RUNTIME_REQUIREMENTS -->

Create DVD-Audio ISOs from FLAC and ALAC/M4A sources, with a Chinese/English GUI, MLP encoding, automatic disc planning, optional menus and output verification.

Extract the entire package and double-click `DVD-Audio-Maker.exe`. The standard package provides the GUI only. Retain every DLL, JSON file, language resource and the `media-native`, `image-native`, `menu-bin` and `data` directories. Do not copy only the main EXE. Also see [RUNTIME.en.md](RUNTIME.en.md).

## Make a disc

1. Select source, working and output directories; set the title, capacity and disc-count limit.
2. Choose the target sample rate and bit depth, and optionally enable track menus, album indexes and playback covers.
3. Check the sources, then preview. Preview prepares sources and encodes MLP, using working-directory space, but does not create an ISO.
4. Build the discs, then verify the output. Completed ISOs appear in the configured output directory.

Keep each album in its own folder with album, title, track and date tags. JPEG, PNG and WebP covers are supported; Chinese/Japanese/Korean menu fonts are bundled. Tasks can be canceled. Closing the window cancels the task and waits for cleanup.

## Configuration and language

Open/save JSON profiles or import an existing `config.env`. On first launch without saved settings, the bundled configuration is read. Configuration parsing reads key/value pairs without running commands or expanding variables; use absolute Windows paths.

The upper-right selector switches between Chinese and English, with the choice saved in the profile. Language changes do not alter paths, audio tags or encoded data. You may also use `--language en`, `--language zh-CN` or `--config` with an env/JSON file.

Daily settings: `%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`.

## Bundled components

Source conversion, probing, decoding and media verification use bundled x64 media libraries. Images, fonts and menu drawing use the in-process image library. No FFmpeg, FFprobe or ImageMagick installation is needed, and these operations do not launch their command-line programs. MLP uses the embedded MLP core without original SurCode or eac3to.

Authoring, menu-video encoding/muxing and ISO creation still use bundled tools, so retain menu-bin. The optional M4A/ALAC-to-FLAC organization feature needs Metaflac for tags and artwork; configure its path or place it in menu-bin/on PATH when using that feature.

Legacy FFmpeg/FFprobe settings remain importable; the GUI automatically selects built-in components. Complete SC/JP/KR font faces are included without a separate font installation.

## Verification scope

Output verification checks disc structures, capacity, timing and menus. Lossless audio verification samples disc 1, group 1, track 1; it does not verify every track. Auditing requires build.log from an actual completed build; a preview log is insufficient.

Encoded MLP is not patched. Comparing complete historical original files requires matching target PCM, settings and auxiliary metadata context. Identical audio alone does not guarantee identical complete files.

## Logs and common issues

Logs default to task summaries, with detailed output, warning filters, paused display, copy and export options. Full task logs and startup errors are stored in `%LOCALAPPDATA%/DVD-Audio-Maker/logs`.

- Cannot start: check that the .NET 10 Desktop Runtime x64 required by this package is installed.
- Missing DLL, font or menu tool: extract the complete package again instead of mixing versions.
- Build failure: export the complete log and retain the corresponding working directory for investigation.
- Large working directory: previews and encoding generate caches; removing them means repeating the relevant steps.

## License

See [LICENSE](LICENSE) and [THIRD-PARTY.en.md](THIRD-PARTY.en.md). Retain bundled licenses and component notices.
