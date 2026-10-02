# Release size optimization

[简体中文](RELEASE-SIZE.md) | [English](RELEASE-SIZE.en.md)

Date: 2026-10-02. Status: complete. GUI and CLI now share one self-contained Windows x64 Desktop runtime and one copy of their common application assemblies.

The packager enables `ShareDesktopRuntime=true` for the packaged CLI so its runtime assets match the GUI. Duplicate file names must have identical bytes; conflicts fail packaging instead of overwriting dependencies. Normal CLI builds retain their original framework references.

Both `DVD-Audio-Maker.exe` and `dvda.exe` are in the package root. `dvda.cmd` remains the CLI launcher and opens the GUI when given no arguments. Extract and distribute the entire directory; the executables require their adjacent DLLs, JSON files and resources. No separate .NET installation is required.

All third-party tools, dynamic libraries, fonts and language resources are retained. ZIP uses the smallest-size compression level. No WinForms trimming or native MLP compiler changes are introduced.

## Measurements

| Metric | Before | After | Reduction |
|---|---:|---:|---:|
| Unpacked | 405.23 MiB | 341.17 MiB | 64.05 MiB / 15.81% |
| ZIP | 197.01 MiB | 165.03 MiB | 31.98 MiB / 16.23% |
| Files | 147 | 428 | Runtime files are unpacked and shared |

New output: `build/gui-x64-compact-release/DVD-Audio-Maker`. The original `build/gui-x64-release` remains available as the baseline.

## Validation

- 107/107 compatibility tests and 32/32 release checks passed.
- GUI, CLI and CoreCLR are AMD64. Both entry points start with invalid global DOTNET_ROOT paths, multilevel lookup disabled and a restricted PATH; host traces confirm the packaged coreclr.dll is used. The installed machine runtime was not uninstalled.
- Chinese and English GUI controls and log self-tests passed, along with the updated command launcher.
- Third-party tools, fonts and menu assets retain identical file sets and SHA-256 hashes. The manifest covers all files, and ZIP integrity, file sets and contents match the release directory.
- Both real release builds prepared and encoded generated sources at 48 kHz / 16-bit / mono, 48 kHz / 24-bit / stereo, and 96 kHz / 24-bit / six channels. Complete old/new MLP files are byte-identical; decoded PCM is identical to the input.
- Native encoder SHA-256 remains `ece6d0a8033a26e2528042a7b74c66c249ea3c8d7378c06809fb94c8f6bd79b8`. This validates the packaging change; the entire original-SurCode historical matrix was not rerun.

See [release-size-validation.json](release-size-validation.json). Detailed logs and host traces are in `build/release-size-verification/run-20261002-103607`.
