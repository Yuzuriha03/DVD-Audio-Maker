# DVD-Audio Maker

[简体中文](README.md) | [English](README.en.md) | [日本語](README.ja.md)

Create DVD-Audio ISO images from FLAC and ALAC/M4A music, with MLP and LPCM encoding, automatic disc splitting, track menus, album covers and output verification. The application runs on Windows x64 and supports Chinese, English and Japanese interfaces and logs.

The release is a native Windows x64 Rust GUI.

## Install and open

1. Download `DVD-Audio-Maker-v1.0-win-x64.zip` from [GitHub Releases](https://github.com/Yuzuriha03/DVD-Audio-Maker/releases/tag/v1.0).
2. Extract the ZIP to a folder of your choice and double-click `DVD-Audio-Maker.exe`.
3. Required components are embedded in the EXE and extracted automatically to a local cache. Startup checks and repairs damaged cache files. No separate FFmpeg, ImageMagick, eac3to or SurCode installation is required.

The application, instructions in three languages and licenses are together in the extracted folder. Retain `LICENSE`, `THIRD-PARTY.md`, `THIRD-PARTY.en.md`, `NOTICE-Image.txt` and `NOTICE-Menu.txt`.

## Make your first disc

1. **Choose the sources.** Select your music folder; subfolders are included. Keep each album in its own folder and provide album, title, track number and date tags.
2. **Choose the output folder.** Set the ISO destination, disc title and DVD5, DVD9 or custom capacity. Albums stay intact; discs are filled in track order, and a new disc starts when the next album will not fit.
3. **Choose the audio format.** Normally select “MLP encoding”. Choose a sample rate and bit depth suitable for the source and playback device. Reducing either can lose source precision; increasing them does not add detail.
4. **Configure menus.** Optionally enable track menus, album indexes and playback covers. JPG, PNG and WebP covers are supported, and Chinese, Japanese and Korean menu fonts are included.
5. Click **“Check sources”** and resolve any issues. After the check succeeds, click **“Build discs”** to encode audio and create ISOs.
6. When finished, click **“Verify output”**. After verification passes, use “Open output” to find your ISOs.

**Build discs** checks sources automatically and reuses valid preparation snapshots when sources and settings are unchanged. Failed checks block the build.

Tasks can be canceled. Closing the window during a task stops it and waits for cleanup. Canceling does not delete completed output. Burning and playback require software or hardware that supports DVD-Audio; an ISO is not a guarantee of compatibility with ordinary DVD-Video players.

## Audio options

| Mode | Storage | Size and limits | When to choose it |
| --- | --- | --- | --- |
| MLP encoding | Losslessly compressed PCM | Usually smaller; supports 16 / 20 / 24 bits | Save space or prepare high-resolution multichannel audio |
| LPCM encoding | Uncompressed PCM | Larger; currently supports 16 / 24 bits, up to 9.6 Mb/s | Store audio without compression |

**Both preserve the same audio quality when given identical PCM at the same sample rate, bit depth and channel layout.** LPCM does not gain detail by using more space. Lowering the output rate or depth can lose precision; raising it does not create new source detail.

LPCM must also meet the bitrate limit. For example, 96 kHz / 24-bit / 6-channel LPCM requires 13.824 Mb/s and is rejected; choose MLP or reduce the target format. 20-bit output currently requires MLP. LPCM also needs more cache and working space.

- 44.1, 48, 88.2 and 96 kHz support one to six channels; 176.4 and 192 kHz support mono and stereo.
- Target bit depths are 16, 20 and 24 bits. The input channel layout is retained.
- Extremely noisy material may exceed MLP stream limits. If encoding fails, review the track's log and retry with a suitable audio format.
- **Import existing MLP files:** Use this option for MLP files encoded with the original SurCode MLP (surcodemlp.exe). Select their folder and keep album directories and filenames aligned with the corresponding sources. The application uses the existing encoded files directly; it does not re-encode them or launch SurCode MLP. Retain the original audio and encoding settings, including sample rate and bit depth, for source checks and lossless verification. A successful import alone does not verify files with unknown origins or conversion settings.

The working folder stores check results, encoded audio and temporary disc files. Choose a drive with enough free space and keep the folder until verification is complete. Retries compare sources, settings and component identities before reusing encoded audio or completed staged discs. Changes invalidate the corresponding results.

## Save settings and import configurations

Settings and language are saved automatically to `%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`. Save profile writes to the selected JSON profile; Save as creates another profile; Open profile loads it. An imported profile changes only when explicitly saved. Advanced settings can be folded, small windows scroll, and invalid numbers prevent saving or starting a task.

The application reads JSON profiles only. Use “Open profile” to select `settings.example.json` or another JSON profile. Existing `config.env` files are not read.

## Progress and troubleshooting

The log area shows task summaries by default. For troubleshooting, switch to detailed logs or save the full log. You can filter issues, pause log display and copy text. Pausing the log display does not pause the task.

- **The application will not open:** extract the release again and ensure that `DVD-Audio-Maker.exe` is intact and the user cache folder is writable.
- **Source checking fails:** review the named track and reason; check that the file is readable and contains supported lossless audio.
- **Not enough disk space:** both working and output folders need free space. Change the working folder under “More settings” if needed.
- **Building or verification fails:** save the full log and retain the working folder, resolve the problem and retry. An ISO being present does not replace verification.
- **A component is missing or damaged:** restart the application to check and repair embedded components. Extract the release again if the EXE itself is damaged.

Session logs are written automatically to `%LOCALAPPDATA%/DVD-Audio-Maker/logs`, with `build.log` in the work folder. Output verification checks capacity, IFO tracks and timing, menu navigation and images, encoded files against disc audio, and target PCM reconstructed from sources. Verification never modifies encoded files. Preparation reports use the selected language; unknown native diagnostics keep their original text.

## Licenses and requirements

See [LICENSE](LICENSE) for the project license and [third-party notices](tools/win-build/docs/THIRD-PARTY.en.md) for components and fonts. Releases use dedicated user documentation in Chinese, English and Japanese, without this development section.

## Development and builds

Development requires Windows x64, the Rust toolchain and matching native components. Native rebuilds use MSYS2/MinGW-w64 and Python. The dvda-author folder is a partial source mirror; prepare full dependencies as described in the build guide.

```bat
gui-debug.cmd
cli.cmd abi.version
cli.cmd build --dry-run --profile "C:/work/settings.json"
cargo build --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace
```

`dry-run` is a developer debugging option in the source CLI. It prepares audio, encodes MLP and writes a separate index without creating an ISO; it is not a zero-write operation. The GUI offers checking, building and verification. `surcode-batch` remains a compatibility configuration key; legacy `surcode` import values map to `external`, with no separate encoding mode.

- [Development, debugging and CLI](docs/DEVELOPMENT.en.md)
- [C# to Rust migration plan and acceptance criteria (Chinese)](docs/RUST-MIGRATION.md)
- [Rust format processing and byte-level validation](docs/C17-FORMATS.md)
- [Windows builds, native components and packaging](tools/win-build/README.en.md)
- [Single-file packaging design and validation](docs/ONEFILE-PUBLISH.md)
- [MLP integration and validation limits](docs/MLP-ENCODER.en.md)
- [Native migration records](docs/NO-EXTERNAL-RUNTIME-MIGRATION.en.md)
- [Troubleshooting and historical diagnostics](docs/TROUBLESHOOTING.en.md)

Default packaging combines one EXE with embedded components and companion user documentation into `DVD-Audio-Maker-v1.0-win-x64.zip`. Use `--version` for later versions. The standard user package excludes the developer CLI, PDBs, build-provenance JSON and the .NET runtime. Artifacts remain in ignored directories and are distributed only as GitHub Release assets.
