# Windows x64 GUI release

[简体中文](README.md) | [English](README.en.md)

<!-- RUNTIME_REQUIREMENTS -->

Double-click `DVD-Audio-Maker.exe` in the package root. Standard releases provide only the graphical entry point. Extract and retain the entire directory, including DLLs, JSON files and resources; do not copy the EXE alone.

See [RUNTIME.en.md](RUNTIME.en.md) for this package: the compact framework-dependent package requires .NET 10 Desktop Runtime for Windows x64, while the self-contained package includes its runtime.

## Make a disc

1. Select your audio source, working directory, destination, title and disc capacity.
2. Choose sample rate, bit depth and FFmpeg location in the audio settings; configure menus if needed.
3. Check the sources. Preview prepares sources, encodes MLP and plans discs without creating ISO images.
4. Build the discs, then verify the output. Lossless audio verification samples the first track; it does not check every track.

Tasks support cancellation. Closing during a task first cancels it and waits for cleanup.

## Configuration and language

Edit everyday settings in the GUI. Open/save JSON profiles and import existing config.env files. Without saved settings, first launch reads an adjacent config.env. Import accepts KEY=VALUE only, without executing commands or expanding variables. Use absolute Windows paths.

The language selector supports Chinese and English and saves the selection with the profile. First launch follows the Windows UI language. You can also use `DVD-Audio-Maker.exe --language en` or `--language zh-CN`, and `--config` with an env or JSON file.

## Tools and encoding

The package includes dvda-author, mkisofs, menu tools, ImageMagick and Chinese/Japanese/Korean fonts. The GUI detects their bundled paths. Retain the menu-bin/fonts directory.

FFmpeg, FFprobe and Metaflac must be on PATH or configured with full paths in the GUI. FFmpeg prepares, decodes and verifies audio. MLP encoding uses a native x64 DLL in process and requires neither eac3to nor original SurCode.

Sources include FLAC and M4A/ALAC, with JPG, PNG or WebP artwork. Use one directory per album and supply date, track, album and title tags.

## Logs and troubleshooting

The log panel offers a summary, details, issue filtering, pause, copy and export. Full logs and startup errors are in `%LOCALAPPDATA%/DVD-Audio-Maker/logs`. Configure missing tools in settings. Disc auditing requires build.log from a real build; preview logs cannot replace it.

## Development diagnostics

The source checkout retains CLI and test projects, with `cli.cmd`, `gui-debug.cmd` and VS Code debug configurations. Add `--include-cli` explicitly when packaging a portable developer diagnostic build; that package includes CLI-TOOLS.en.md.

## License and byte identity

Project code follows LICENSE; third-party components are listed in THIRD-PARTY.md. The native MLP core is embedded in an application assembly and verified with SHA-256. Encoded output is not patched. Historical original byte comparisons require identical PCM, settings and explicit metadata context.
