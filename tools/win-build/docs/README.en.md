# DVD-Audio Maker

Task recovery: only Start production asks Continue production / Restart production. Failure or cancellation asks Keep progress / Delete progress. Continuing uses the original task parameters; source checks use a separate temporary workspace. Successful production and verification remove intermediate files. Independent rechecks of re-encoded built-in MLP use mlp-coded-v1-no-auxiliary-stamp, validating original header parity before excluding the entire auxiliary identification stream and its dependent parity bit. This is coded-stream equality, not raw-file equality. Post-build verification and external MLP comparisons remain byte-for-byte.

[中文](README.md) · [日本語](README.ja.md)

For Windows 10/11 x64. Extract the ZIP and run **DVD-Audio-Maker.exe**.

Required components are embedded and prepared automatically on first launch, which may take longer. Documentation and licenses are beside the EXE.

## Create a disc

1. Select the FLAC/M4A source folder, output folder and work folder.
2. Set the disc title, capacity and encoding mode. Enable cover menus if desired, and pick the video standard (PAL, SECAM or NTSC) that matches the player.
3. Select **Start build**. The app checks sources, creates the discs and verifies the finished output automatically. Valid checks are reused for unchanged files.
4. Open the ISO using DVD-Audio compatible burning software or a player.

**MLP encoding** compresses PCM losslessly, usually fitting more music on a disc. **LPCM encoding** stores uncompressed PCM and usually needs more space. Neither uses lossy compression. Changing sample rate or bit depth changes target PCM; choose settings that suit your sources and equipment.

**Import existing MLP** is intended for files previously encoded by a compatible MLP encoder. Keep the matching original audio and select the MLP folder. Importing does not re-encode audio. Unknown resampling or bit-depth conversions may prevent verification against the originals.

Settings and language are saved automatically to settings.json (EXE directory). Use Save profile, Save as and Open profile to manage JSON profiles. Legacy config.env is no longer supported. Chinese, English and Japanese interfaces are available.

Albums stay intact and discs are filled in track order up to the selected capacity before the next disc starts. MLP encoding concurrency is allocated automatically from available processors.

After cancellation, keep the work folder. Resume checks inputs, settings and completed discs before reuse. Session logs are in gui-*.log (EXE directory; latest 3 runs only); build.log is also written in the work folder. Save a detailed log when reporting a problem.
