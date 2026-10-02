# Compact release without .NET

[简体中文](MINIMAL-RELEASE.md) | [English](MINIMAL-RELEASE.en.md)

Date: 2026-10-02. Complete. The package retains GUI, native MLP encoding, authoring, menus and fonts while excluding .NET runtime files, CLI and development symbols. Source debugging remains available.

## Requirements and packaging

Install **.NET 10 Desktop Runtime for Windows x64**. The plain .NET Runtime, ASP.NET Core Runtime and .NET Framework 4.x do not replace it. Existing compatible desktop installations can be reused. Both bundled README translations begin with this requirement and link to the official Microsoft installer page; RUNTIME.md and RUNTIME.en.md provide the same information. Existing external tool requirements such as FFmpeg remain.

Add `--framework-dependent` to the normal tools/win-build/build-all.cmd invocation with --source, --prebuilt and --output. Omitting the flag keeps self-contained publishing. Without --output, the compact profile defaults to tools/win-build/release-framework-dependent. CLI remains excluded unless --include-cli is explicitly added.

Output: build/gui-minimal-x64-release/DVD-Audio-Maker and the adjacent DVD-Audio-Maker.zip. Extract the whole directory; do not copy the EXE alone. Previous self-contained packages are retained.

## Measurements and validation

| Metric | Previous GUI self-contained | Without .NET | Reduction |
|---|---:|---:|---:|
| Unpacked | 340.93 MiB | 223.83 MiB | 34.35% |
| ZIP | 164.93 MiB | 117.61 MiB | 28.70% |
| Files | 423 | 159 | 264 files |

All 26 targeted checks passed. The package excludes .NET runtime binaries, declares installed .NET 10 Core/Desktop framework dependencies and retains an AMD64 apphost. Chinese and English GUI checks use the installed desktop runtime, confirmed by host traces. Startup fails when runtime paths are isolated.

Actual GUI encoding at 48 kHz/16-bit/mono, 48 kHz/24-bit/stereo and 96 kHz/24-bit/six channels produces complete MLP files identical to the previous package. Native encoder bytes, third-party tools, fonts and menu assets remain unchanged. Manifest and archive contents pass SHA-256 checks. The previously passing 107 general compatibility tests were not repeated for this packaging-only change.

See [minimal-release-validation.json](minimal-release-validation.json).
