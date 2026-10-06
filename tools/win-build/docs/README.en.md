# DVD-Audio Maker

[中文](README.md) · [日本語](README.ja.md)

For Windows 10/11 x64. Extract the ZIP and run **DVD-Audio-Maker.exe**.

Required components are embedded and prepared automatically on first launch, which may take longer. Documentation and licenses are beside the EXE.

## Create a disc

1. Select the FLAC/M4A source folder, output folder and work folder.
2. Set the disc title, capacity and encoding mode. Enable cover menus if desired.
3. Select **Start build**. The app checks sources, creates the discs and verifies the finished output automatically. Valid checks are reused for unchanged files.
4. Open the ISO using DVD-Audio compatible burning software or a player.

**MLP encoding** compresses PCM losslessly, usually fitting more music on a disc. **LPCM encoding** stores uncompressed PCM and usually needs more space. Neither uses lossy compression. Changing sample rate or bit depth changes target PCM; choose settings that suit your sources and equipment.

**Import existing MLP** is intended for files previously encoded by SurCode MLP or another compatible tool. Keep the matching original audio and select the MLP folder. Importing does not re-encode audio. Unknown resampling or bit-depth conversions may prevent verification against the originals.

Settings and language are saved automatically to %LOCALAPPDATA%/DVD-Audio-Maker/settings.json. Use Save profile, Save as and Open profile to manage JSON profiles. Legacy config.env is no longer supported. Chinese, English and Japanese interfaces are available.

Planned disc count and MLP encoding workers can both be set to `auto`; the app chooses them based on the sources and available processors.

After cancellation, keep the work folder. Resume checks inputs, settings and completed discs before reuse. Session logs are in %LOCALAPPDATA%/DVD-Audio-Maker/logs; build.log is also written in the work folder. Save a detailed log when reporting a problem.
