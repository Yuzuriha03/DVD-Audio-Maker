# DVD-Audio Maker

[简体中文](README.md) | [English](README.en.md) | [日本語](README.ja.md) | [한국어](README.ko.md)

Create DVD-Audio ISO images from FLAC and ALAC/M4A music, with MLP or LPCM encoding, automatic disc splitting, track menus, album covers and output verification. A portable Windows application: extract the ZIP and run it, with no installation required.

## Download and install

1. Open [GitHub Releases](https://github.com/Yuzuriha03/DVD-Audio-Maker/releases/latest) and download the package for your PC:
   - Intel / AMD PCs: `DVD-Audio-Maker-v1.1-win-x64.zip`
   - Windows on ARM devices: `DVD-Audio-Maker-v1.1-win-arm64.zip`
2. Extract the ZIP to a folder of your choice and double-click `DVD-Audio-Maker.exe`.
3. Keep `LICENSE`, `THIRD-PARTY.md`, `THIRD-PARTY.en.md`, `NOTICE-Image.txt` and `NOTICE-Menu.txt` in the extracted folder.

**Requirements:** Windows 10 / 11 on x64 or ARM64. All required components are built in, so no extra runtime needs to be installed. The folder containing the EXE must be writable. Building an ISO and its temporary files also needs enough free disk space; reserve roughly the size of the finished output.

## Features

- Create DVD-Audio ISOs from FLAC and ALAC/M4A music
- MLP lossless compression or LPCM uncompressed audio
- Automatic splitting across DVD5, DVD9 or custom capacity discs while keeping albums intact
- Track menus, album indexes and playback covers
- Source checking before building and output verification afterwards
- Resume an unfinished task after interruption
- Portable settings stored next to the application; no registry entries

## Quick start

1. **Choose the source folder.** Keep each album in its own folder; the application scans `.flac` and `.m4a` files, including subfolders. Fill in album, title, track number and date tags where possible.
2. **Choose the output location.** Set the ISO folder, disc title and capacity. Discs are filled in track order; when the next album does not fit, the application starts a new disc.
3. **Choose audio settings.** “MLP encoding” is the usual choice. Pick a sample rate and bit depth that suit the source and your player. Lowering either can reduce precision; raising them does not add detail.
4. **Configure menus.** Optionally enable track menus, album indexes and playback covers. Menu fonts are included.
5. **Check sources.** Click “Check sources” and resolve any reported issues.
6. **Build discs.** After the check succeeds, click “Build discs” to encode audio and create the ISOs.
7. **Verify output.** When building finishes, click “Verify output”; after it passes, use “Open output” to find the ISO folder.

“Build discs” checks sources automatically; a failed check blocks the build. You can cancel a task while it runs. Canceling does not delete discs that were already completed.

## Source requirements

- **FLAC** and **ALAC/M4A** (`.m4a`) are supported.
- Lossless sources are recommended. Lossy sources keep their existing loss; re-encoding cannot restore it.
- Source sample rate, bit depth and channel count may differ from the target; the application converts them according to your settings.
- Clear file names with track numbers, and complete album, title, artist, track and date tags, make menus and logs easier to read.

## Source folder layout

Organize the library with one folder per album:

```text
Music/                            ← DVDA_SRC
├─ Album A/
│  ├─ 01 - Track.flac
│  ├─ 02 - Track.flac
│  ├─ 03 - Track.m4a
│  └─ cover.jpg                  ← optional album cover
├─ Album B/
│  ├─ 01 - Track.flac
│  ├─ 02 - Track.flac
│  └─ cover.png
└─ Album C/
   ├─ 01 - Track.flac
   └─ 02 - Track.flac
```

- `DVDA_SRC` points to the library root; the application scans it recursively for `.flac` and `.m4a` files.
- **Albums are grouped by the `Album` tag, not only by folder name.** Make the Album tag identical for every track in an album. Empty or inconsistent tags may merge tracks into one album or split them across albums.
- Name tracks like `01 - Track.flac` and fill in the `Track` tag as well. Files are read in filename order; tags drive display and ordering.
- Do not nest extra folders inside an album folder. Put the cover at the album folder root.
- A single album may mix FLAC and M4A, but using one format per album is recommended.
- Keep the output folder (`DVDA_FINAL_DIR`) outside the music library.

In a JSON profile the keys are:

```json
"DVDA_SRC": "D:/Music/MyAlbums",
"DVDA_FINAL_DIR": "D:/DVD-Audio/Output"
```

In the GUI, “Source folder” maps to `DVDA_SRC` and “Output folder” maps to `DVDA_FINAL_DIR`.

## Audio options

| Mode | Storage | Size and limits | When to choose it |
| --- | --- | --- | --- |
| MLP encoding | Losslessly compressed PCM | Usually smaller; supports 16 / 20 / 24 bits | Save space, or author high-resolution multichannel audio |
| LPCM encoding | Uncompressed PCM | Larger; currently supports 16 / 24 bits, up to 9.6 Mb/s | Store audio without compression |

**Both modes preserve the same audio quality when given identical PCM at the same sample rate, bit depth and channel layout.** LPCM does not gain detail by using more space.

- Target bit depth: MLP supports 16 / 20 / 24 bits; LPCM supports 16 / 24 bits. Use MLP for 20-bit output.
- When the target is 16-bit and the source is higher than 16-bit or floating-point, TPDF dither is added by default to reduce quantization distortion. You can turn it off; sources at 16 bits or lower are never dithered.
- If the source bit depth is higher than the target, it is converted and is not bit-perfect. To preserve it exactly, choose a target bit depth equal to or higher than the source.
- Sample rates: 44.1, 48, 88.2 and 96 kHz support 1–6 channels; 176.4 and 192 kHz support mono and stereo.
- LPCM has a bitrate limit. For example, 96 kHz / 24-bit / 6-channel is about 13.8 Mb/s and cannot be stored as LPCM; choose MLP or lower the target.
- Extremely noisy material may exceed MLP stream limits. If encoding fails, review the track log and retry with a different audio format.

## Menus and covers

- Add a cover to each album folder as `cover.jpg`, `cover.jpeg`, `cover.png` or `cover.webp`.
- Menus can show track selection, album indexes and playback covers; a missing cover uses a black background.
- Menu fonts are bundled and cover Simplified Chinese, Traditional Chinese, Japanese and Korean text.

## Tasks, resume and logs

- By default, task data uses the system temporary folder and is removed after success, failure or normal cancellation; only finished ISOs remain.
- If “Resume unfinished work” is enabled, an unfinished task is kept under `unfinished-task` in the selected working folder and can be resumed on the next launch; it is still removed after a successful build and verification.
- Session logs are saved beside the EXE as `gui-timestamp-pid-sequence.log`; only the last three runs are kept.
- For troubleshooting, switch to detailed logging or click “Save detailed log”.

## Troubleshooting

**Can I play the ISO on an ordinary DVD player?**  
Not necessarily. DVD-Audio requires software or hardware that supports DVD-Audio; ordinary DVD-Video players may not play it.

**Why can't I select 20-bit LPCM?**  
The current LPCM path supports 16 / 24 bits only; use MLP for 20-bit output. LPCM is also limited to 9.6 Mb/s.

**“Check sources” fails.**  
Review the reported track and reason, confirm the file is readable and is FLAC or M4A, and check its tags and path. Fix the issue and check again; checks do not reuse caches between tasks.

**Building or verification fails.**  
Save the detailed log and keep the working folder. Check free disk space, write permissions and source integrity, then retry. Do not skip verification just because an ISO was produced.

**Where are settings stored?**  
In `settings.json` next to the EXE; it is loaded at startup and is not written to `%LOCALAPPDATA%`. The folder must be writable. “Save profile” and “Open profile” manage JSON profiles.

**How do I uninstall it?**  
Delete the application folder; no registry entries are created. Move any ISOs and logs you want to keep first.

## Interface languages

The interface and logs support Simplified Chinese, Traditional Chinese, English, Japanese and Korean. The last selected language is remembered, and you can switch manually.

## Licenses and third-party components

See [LICENSE](LICENSE) for the project license and these notices for bundled components and fonts:

- [Third-party components (Chinese)](tools/win-build/docs/THIRD-PARTY.md)
- [Third-party components (English)](tools/win-build/docs/THIRD-PARTY.en.md)

The release also includes `NOTICE-Image.txt` and `NOTICE-Menu.txt`; please keep them.

See [CONTRIBUTING.en.md](CONTRIBUTING.en.md) for development and Clippy pedantic maintenance guidance.
