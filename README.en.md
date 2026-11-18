# DVD-Audio Maker

[简体中文](README.md) | [English](README.en.md) | [日本語](README.ja.md) | [한국어](README.ko.md)

Create DVD-Audio ISO images from FLAC and ALAC/M4A music. Windows and Android share the Rust authoring workflow, with MLP and LPCM encoding, import of existing MLP files, automatic disc splitting, track menus and automatic verification.

## Download and install

Get the file for your device from [GitHub Releases](https://github.com/Yuzuriha03/DVD-Audio-Maker/releases/latest). The filenames below are for v1.2.

- **Windows 10 / 11:** use `DVD-Audio-Maker-v1.2-win-x64.exe` on Intel / AMD PCs or `DVD-Audio-Maker-v1.2-win-arm64.exe` on Windows on ARM. Run the single EXE from a writable folder; no extraction, installation or extra runtime is needed. The first launch prepares the embedded components.
- **Android 7.0 or later (ARM64):** download and open `DVD-Audio-Maker-v1.2-android-arm64.apk`. If prompted, allow your browser or file manager to install apps from that source. The APK is distributed here for sideloading, not through Google Play.

Allow enough space for intermediate files and the finished ISOs. On Android, check both app-internal storage and the selected output location.

## Features

- Create DVD-Audio ISOs from FLAC and ALAC/M4A music
- MLP lossless compression or LPCM uncompressed audio
- Automatic splitting across DVD5, DVD9 or custom capacity discs while keeping albums intact
- Track menus, album indexes and playback covers
- Source checking before building and output verification afterwards
- Resume an unfinished task after interruption
- Save and open JSON profiles, copy the current log view or export the full task log

## Quick start

1. **Choose the source folder.** Keep each album in its own folder. The application scans recursively for `.flac` and `.m4a` files. On Android, grant access to your source and output folders with the system picker; the app does not request all-files access.
2. **Choose the output location.** Set the ISO folder, disc title and capacity. Discs are filled in track order; when the next album does not fit, the application starts a new disc.
3. **Choose audio settings.** “MLP encoding” is the usual choice. Pick a sample rate and bit depth that suit the source and your player. Lowering either can reduce precision; raising them does not add detail.
4. **Configure menus.** Optionally enable track menus, album indexes and playback covers. Menu fonts are included.
5. **Build discs.** Click “Build discs” to check sources, encode audio, create the ISOs and verify the output automatically; once the task completes, use “Open output” on Windows or open the folder from the Android task screen.

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
- Keep the `Album` tag consistent within each album folder. Name tracks like `01 - Track.flac` and fill in the `Track` tag.
- Do not nest extra folders inside an album folder. Put the cover at the album folder root.
- A single album may mix FLAC and M4A, but using one format per album is recommended.
- Keep the output folder (`DVDA_FINAL_DIR`) outside the music library.

On Windows, these fields go in a JSON profile’s `Values` object:

```json
{
  "DVDA_SRC": "D:/Music/MyAlbums",
  "DVDA_FINAL_DIR": "D:/DVD-Audio/Output"
}
```

On Android, paths in a profile do not grant access. Select the required folders on the device.

## Audio options

| Mode | Storage | Size and limits | When to choose it |
| --- | --- | --- | --- |
| MLP encoding | Losslessly compressed PCM | Usually smaller; supports 16 / 20 / 24 bits | Save space, or author high-resolution multichannel audio |
| LPCM encoding | Uncompressed PCM | Larger; currently supports 16 / 24 bits, up to 9.6 Mb/s | Store audio without compression |

**Both modes preserve the same audio quality when given identical PCM at the same sample rate, bit depth and channel layout.** LPCM does not gain detail by using more space.

- If you already have compatible MLP files, choose “Import MLP” and select a folder with the same layout as the source library.
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

- After a failure or stop, you can keep or discard progress when prompted. If you keep it, the next time you select “Build discs,” you can resume or start over. A successful task normally cleans up its intermediate files.
- Copying the current view and exporting the full log are separate actions. Export the full log for troubleshooting; it is not limited by the panel view or the “Problems only” filter.
- **Windows:** session logs are stored beside the EXE as `gui-timestamp-pid-sequence.log`, with the latest three retained. Everyday settings are stored there in `settings.json`.
- **Android:** use the system picker to open or save JSON profiles and export the full task log. Verified ISOs are exported to the granted folder; a failed export preserves the previous output and the recoverable task. Discarding progress and clearing caches does not delete granted folders or exported ISOs.
- **Android notifications:** build progress can appear in a system notification. On Android 16 or later, Live Updates are available when the device, system settings and notification permissions allow them. In-app progress still works without notification permission.

## Troubleshooting

**Can I play the ISO on an ordinary DVD player?**

Not necessarily. DVD-Audio requires software or hardware that supports DVD-Audio; ordinary DVD-Video players may not play it.

**Why can't I select 20-bit LPCM?**

The current LPCM path supports 16 / 24 bits only; use MLP for 20-bit output. LPCM is also limited to 9.6 Mb/s.

**The pre-build source check fails.**
Review the reported track and reason, confirm the file is readable and is FLAC or M4A, and check its tags and path. Fix the issue and restart the build.

**Building or verification fails.**

Save the detailed log and keep the working folder. Check free disk space, write permissions and source integrity, then retry. Do not skip verification just because an ISO was produced.

**Where are settings stored?**

Windows stores everyday settings in `settings.json` beside the EXE; Android stores them in app-private storage. Both builds can save and open JSON profiles. On Android, grant access to folders through the system picker even after importing a profile.

**How do I uninstall it?**

On Windows, delete the application folder after moving any ISOs, profiles and logs you want to keep. On Android, uninstall from system settings. This removes private app data, not files already exported to your folders.

## Interface languages

The interface and logs support Simplified Chinese, Traditional Chinese, English, Japanese and Korean. The last selected language is remembered, and you can switch manually.

## Licenses and third-party components

See [LICENSE](LICENSE) for the project license and these notices for bundled components and fonts:

- [Third-party components (Chinese)](tools/win-build/docs/THIRD-PARTY.txt)
- [Third-party components (English)](tools/win-build/docs/THIRD-PARTY.en.txt)

Both Windows and Android make the bundled licenses and notices available in About.

See [CONTRIBUTING.en.md](CONTRIBUTING.en.md) and the [Android build and verification guide](apps/android/README.md) for development.
