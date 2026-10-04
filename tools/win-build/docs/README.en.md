# DVD-Audio Maker

[简体中文](README.md) | [English](README.en.md) | [日本語](README.ja.md)

Create DVD-Audio ISO images from FLAC and ALAC/M4A music, with MLP and LPCM encoding, automatic disc splitting, track menus, album covers and output verification. The application runs on Windows x64 and supports Chinese, English and Japanese interfaces and logs.

<!-- RUNTIME_REQUIREMENTS -->

## Install and open

1. Download `DVD-Audio-Maker-v1.0-win-x64.zip`.
2. Extract the ZIP to a folder of your choice and double-click `DVD-Audio-Maker.exe`.
3. Allow a moment for the first launch to prepare its components. No separate FFmpeg, ImageMagick, eac3to or SurCode installation is required.

The application, instructions, example configuration and licenses are together in the extracted folder. Retain `LICENSE`, `THIRD-PARTY.md`, `THIRD-PARTY.en.md`, `NOTICE-Image.txt` and `NOTICE-Menu.txt`.

## Make your first disc

1. **Choose the sources.** Select your music folder; subfolders are included. Keep each album in its own folder and provide album, title, track number and date tags.
2. **Choose the output folder.** Set the ISO destination, disc title and DVD5, DVD9 or custom capacity. Set the maximum disc count to 0 for no limit; discs are split according to capacity.
3. **Choose the audio format.** Normally select “MLP encoding”. Choose a sample rate and bit depth suitable for the source and playback device. Reducing either can lose source precision; increasing them does not add detail.
4. **Configure menus.** Optionally enable track menus, album indexes and playback covers. JPG, PNG and WebP covers are supported, and Chinese, Japanese and Korean menu fonts are included.
5. Click **“Check sources”** and resolve any issues, then **“Build discs”**. Building also checks the sources automatically before encoding and creating ISOs.
6. When finished, click **“Verify output”**. After verification passes, use “Open output” to find your ISOs.

If you just checked the sources and the files, settings and preparation manifest are unchanged, **Build discs** reuses that result instead of probing and decoding everything again. Any source-file change triggers a fresh check.

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

The working folder stores check results, encoded audio and temporary disc files. Choose a drive with enough free space. Valid caches are reused and unfinished builds can resume. Keep the working folder until output verification is complete.

## Save settings and import configurations

Edit settings in the interface. Use “Save profile” to store a JSON profile and “Open profile” to reuse it later. Everyday settings are saved in `%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`.

Use “Import env…” to read an existing `config.env`. If there are no saved everyday settings, the application also looks for config.env beside the EXE or in the current working directory. Import does not rewrite the original file. A legacy SurCode import setting is mapped to generic MLP import; check the source and MLP folder paths afterward.

The included `config.env.example` is optional and is not loaded automatically. There is no need to edit it to get started. Switch between Chinese, English and Japanese at the top right; the preference is saved with your profile.

## Progress and troubleshooting

The log area shows task summaries by default. For troubleshooting, switch to detailed logs or save the full log. You can filter issues, pause log display and copy text. Pausing the log display does not pause the task.

- **The application will not open:** install the Windows x64 version of .NET 10 Desktop Runtime and try again.
- **Source checking fails:** review the named track and reason; check that the file is readable and contains supported lossless audio.
- **Not enough disk space:** both working and output folders need free space. Change the working folder under “More settings” if needed.
- **Building or verification fails:** save the full log and retain the working folder, resolve the problem and retry. An ISO being present does not replace verification.
- **A component is missing or damaged:** close all instances and reopen the application. If needed, extract the ZIP again. With the application closed, you can also remove `%LOCALAPPDATA%/DVD-Audio-Maker/runtime`; the component cache is rebuilt on the next launch.

Full task logs and startup errors are stored in `%LOCALAPPDATA%/DVD-Audio-Maker/logs`. Output verification checks capacity, tracks, timing, menus and audio, including all MLP data inside the disc. It does not modify encoded files.

## Licenses and requirements

See [LICENSE](LICENSE) for the project license, [THIRD-PARTY.en.md](THIRD-PARTY.en.md) for components and fonts, and [RUNTIME.en.md](RUNTIME.en.md) for runtime requirements.
