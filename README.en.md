# DVD-Audio Maker

Task recovery: only Start production asks Continue production / Restart production. Failure or cancellation asks Keep progress / Delete progress. Continuing uses the original task parameters; source checks use a separate temporary workspace. Successful production and verification remove intermediate files. Independent rechecks of re-encoded built-in MLP use mlp-coded-v1-no-auxiliary-stamp, validating original header parity before excluding the entire auxiliary identification stream and its dependent parity bit. This is coded-stream equality, not raw-file equality. Post-build verification and external MLP comparisons remain byte-for-byte.

[简体中文](README.md) | [English](README.en.md) | [日本語](README.ja.md)

Create DVD-Audio ISO images from FLAC and ALAC/M4A music, with MLP and LPCM encoding, automatic disc splitting, track menus, album covers and output verification. The application runs on Windows x64 and supports Chinese, English and Japanese interfaces and logs.

The release is a native Windows x64 Rust GUI.

## Install and open

1. Download `DVD-Audio-Maker-v1.0-win-x64.zip` from [GitHub Releases](https://github.com/Yuzuriha03/DVD-Audio-Maker/releases/tag/v1.0).
2. Extract the ZIP to a folder of your choice and double-click `DVD-Audio-Maker.exe`.
3. Required components are embedded in the EXE and extracted to a unique temporary directory on each launch, removed after exit. No separate FFmpeg, ImageMagick, eac3to or external MLP encoder installation is required.

The application, instructions in three languages and licenses are together in the extracted folder. Retain `LICENSE`, `THIRD-PARTY.md`, `THIRD-PARTY.en.md`, `NOTICE-Image.txt` and `NOTICE-Menu.txt`.

## Make your first disc

1. **Choose the sources.** Select your music folder; subfolders are included. Keep each album in its own folder and provide album, title, track number and date tags.
2. **Choose the output folder.** Set the ISO destination, disc title and DVD5, DVD9 or custom capacity. Albums stay intact; discs are filled in track order, and a new disc starts when the next album will not fit.
3. **Choose the audio format.** Normally select “MLP encoding”. Choose a sample rate and bit depth suitable for the source and playback device. Reducing either can lose source precision; increasing them does not add detail.
4. **Configure menus.** Optionally enable track menus, album indexes and playback covers. JPG, PNG and WebP covers are supported, and Chinese, Japanese and Korean menu fonts are included.
5. Click **“Check sources”** and resolve any issues. After the check succeeds, click **“Build discs”** to encode audio and create ISOs.
6. When finished, click **“Verify output”**. After verification passes, use “Open output” to find your ISOs.

**Build discs** checks sources automatically. Failed checks block the build. Separate source checks do not retain cross-task caches.

Tasks can be canceled. Closing the window during a task stops it and waits for cleanup. Canceling does not delete completed output. Burning and playback require software or hardware that supports DVD-Audio; an ISO is not a guarantee of compatibility with ordinary DVD-Video players.

## Audio options

| Mode | Storage | Size and limits | When to choose it |
| --- | --- | --- | --- |
| MLP encoding | Losslessly compressed PCM | Usually smaller; supports 16 / 20 / 24 bits | Save space or prepare high-resolution multichannel audio |
| LPCM encoding | Uncompressed PCM | Larger; currently supports 16 / 24 bits, up to 9.6 Mb/s | Store audio without compression |

**Both preserve the same audio quality when given identical PCM at the same sample rate, bit depth and channel layout.** LPCM does not gain detail by using more space. Lowering the output rate or depth can lose precision; raising it does not create new source detail.

LPCM must also meet the bitrate limit. For example, 96 kHz / 24-bit / 6-channel LPCM requires 13.824 Mb/s and is rejected; choose MLP or reduce the target format. 20-bit output currently requires MLP. LPCM also needs more temporary working space.

- 44.1, 48, 88.2 and 96 kHz support one to six channels; 176.4 and 192 kHz support mono and stereo.
- Target bit depths are 16, 20 and 24 bits. The input channel layout is retained.
- Extremely noisy material may exceed MLP stream limits. If encoding fails, review the track's log and retry with a suitable audio format.
- **Import existing MLP files:** Use this option for MLP files encoded with another MLP encoder. Select their folder and keep album directories and filenames aligned with the corresponding sources. The application uses the existing encoded files directly; it does not re-encode them or launch an external MLP encoder. Retain the original audio and encoding settings, including sample rate and bit depth, for source checks and lossless verification. A successful import alone does not verify files with unknown origins or conversion settings.

Task files use the system temporary directory by default and are deleted after success, failure or normal cancellation; published ISOs are retained. Only “Resume unfinished work” retains an unfinished task under unfinished-task in the selected working folder. Successful production and verification still delete it; completed-task caches are not retained. Resume uses saved progress only. Forced termination or power loss may leave temporary files until the next launch. Existing legacy folders and user inputs are not deleted.

## Save settings and import configurations

Settings and language are saved automatically to `settings.json` beside the EXE and loaded on startup, not stored in `%LOCALAPPDATA%`. The EXE directory must be writable; save failures are reported. Save profile writes to the selected JSON profile; Save as creates another profile; Open profile loads it. An imported profile changes only when explicitly saved. Advanced settings can be folded, small windows scroll, and invalid numbers prevent saving or starting a task.

The application reads JSON profiles only. Use “Open profile” to select `settings.example.json` or another JSON profile. Existing `config.env` files are not read.

## Progress and troubleshooting

The log area shows task summaries by default. For troubleshooting, switch to detailed logs or save the full log. You can filter issues, pause log display and copy text. Pausing the log display does not pause the task.

- **The application will not open:** extract the release again and ensure that `DVD-Audio-Maker.exe` is intact and the system temporary folder is writable.
- **Source checking fails:** review the named track and reason; check that the file is readable and contains supported lossless audio.
- **Not enough disk space:** both working and output folders need free space. Change the working folder under “More settings” if needed.
- **Building or verification fails:** save the full log and retain the working folder, resolve the problem and retry. An ISO being present does not replace verification.
- **A component is missing or damaged:** restart the application to check and repair embedded components. Extract the release again if the EXE itself is damaged.

Session logs are saved beside the EXE as `gui-timestamp-pid-sequence.log`; only the latest three runs are retained. Task logs are deleted with the workspace; explicit export is also available. Rechecking after cleanup reconstructs temporary references from current sources and settings and may re-encode audio; it explicitly excludes historical build-log audit. Output verification checks capacity, IFO tracks and timing, menu navigation and images, encoded files against disc audio, and target PCM reconstructed from sources. Verification never modifies encoded files. Preparation reports use the selected language; unknown native diagnostics keep their original text.

### Stage and resource metrics

Per-track PCM verification for built-in MLP/LPCM writes normalized samples directly as S24, avoiding an intermediate normalized WAV and its repeated decode. Source conversion policy, precision checks, coded-stream checks and full PCM comparison are unchanged. A dedicated lossless path expands 16-bit PCM to S24; 20-bit PCM checks the low four bits directly, avoiding generic per-sample conversion overhead. Packing 32-bit storage to 24-bit fills output blocks directly while retaining the lossless zero-low-byte check. PCM verification compares whole blocks first and locates the first differing byte only on mismatch; every sample and permitted zero tail remain fully checked.

Detailed and archived logs retain `[METRIC] ` JSON lines (`version=1`); summaries, current activity and problem filters hide them. Executed preparation/source checks, acquisition, MLP batch, author/ISO, publication and verification (including PCM and lossless streams) emit `start`/`finish` events paired by stage and log order. Reused preparation can skip that stage. `elapsed_ms` measures operation wall time, excluding sampler startup/join and finish logging; `succeeded` records normal success/failure. Cancellation returning normally also finishes; panics or forced termination may not.

`worker_limit` and `active_workers_peak` describe this layer only, not encoder-internal threads; a parent limit of 0 means uncounted, not disabled concurrency. `completed_items`, `completed_input_file_bytes` and `input_size_unavailable` count successful items, logical input-file lengths and unavailable lengths (publication counts staged ISO source lengths), not physical I/O or partial failed work. `stream_bytes_read` currently counts only AOB chunks yielded during lossless verification, including reads before a later failure; elsewhere it is 0. Nested/repeated stages must not be summed; zero may mean uninstrumented.

Start events list deduplicated `temporary_roots`, possibly including output folders and existing files. `temporary_space` contains `sampled_peak_bytes`, `samples` and `incomplete_samples`, or is null without roots/on sampler-thread failure. Sampling runs at startup/end and roughly every second, scans at most 16,384 entries per pass, skips links/reparse points and marks errors/truncation incomplete. This is a non-atomic logical-size high-water observation including the baseline, not exact peak, allocated size or growth: short-lived files may be missed, hard links may count twice, and incomplete samples are lower bounds. Sampling adds directory I/O and slow storage can delay finishing. No performance benchmark or speedup is claimed; metrics do not affect encoding acceptance.

## Licenses and requirements

See [LICENSE](LICENSE) for the project license and [third-party notices](tools/win-build/docs/THIRD-PARTY.en.md) for components and fonts. Releases use dedicated user documentation in Chinese, English and Japanese, without this development section.

## Development and builds

Development requires Windows x64, the Rust toolchain and matching native components. Native rebuilds use MSYS2/MinGW-w64 and Python. The dvda-author folder is a partial source mirror; prepare full dependencies as described in the build guide.

```bat
gui-debug.cmd
cli.cmd abi.version
cli.cmd build --dry-run --profile "C:/work/settings.json"
cargo build --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --workspace
```

`dry-run` is a developer debugging option in the source CLI. It prepares audio, encodes MLP and writes a separate index without creating an ISO; it is not a zero-write operation. The GUI offers checking, building and verification. Use `mlp-batch` for built-in encoding and `external` for importing existing files. The setting keys are `DVDA_MLP_SAMPLE_RATE` and `DVDA_MLP_BITS`; update older profiles to these names.

- [Windows builds, native components and packaging](tools/win-build/README.en.md)

Development, migration and diagnostics documents are kept only in the local source working copy and are not published with the repository.

Default packaging combines one EXE with embedded components and companion user documentation into `DVD-Audio-Maker-v1.0-win-x64.zip`. Use `--version` for later versions. The standard user package excludes the developer CLI, PDBs, build-provenance JSON and the .NET runtime. Artifacts remain in ignored directories and are distributed only as GitHub Release assets.
