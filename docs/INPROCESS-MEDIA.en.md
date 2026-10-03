# In-process media processing

[简体中文](INPROCESS-MEDIA.md) | [English](INPROCESS-MEDIA.en.md)

2026-10-02. Normal GUI source conversion, probing, decoding and output verification now call bundled Windows x64 DLLs inside the application. They do not launch ffmpeg.exe or ffprobe.exe. FFmpeg library code is still used; users no longer need a separate FFmpeg installation.

## Use and configuration

- Retain the complete media-native directory. The standard package is x64 GUI-only and framework-dependent, requiring .NET 10 Desktop Runtime x64.
- Existing config.env and JSON profiles remain readable. The GUI selects bundled media automatically and removes FFmpeg / FFprobe path controls.
- Defaults are DVDA_FFMPEG=builtin:media and DVDA_FFPROBE=builtin:probe. Explicit external paths remain available to the developer CLI for reference comparisons. The GUI ignores these legacy paths and does not fall back to external tools when libraries are missing.
- Optional ALAC-to-FLAC artwork/tag normalization uses the in-process `FlacMetadataEditor` and no longer requires Metaflac. ImageMagick is already in-process, and ISO creation is now provided by dvda-author's built-in writer; menu MPEG authoring tools retain their existing workflow.

## Implementation

BuiltinMedia maps the application's existing media requests to a small C ABI, called through P/Invoke into dvda-media.dll. It supports the application's requests rather than arbitrary FFmpeg command lines.

The native bridge implements probing, first-audio-stream decoding, ALAC packet inspection, PCM/FLAC output, MD5, artwork copying and DVD menu frame extraction. Progress, timeout and cancellation remain available. Outputs are staged beside the destination and replaced only on success; failure or cancellation removes the temporary file. Libraries load from media-native and Windows system directories, without borrowing PATH libraries.

The media build includes FFmpeg 9.0.2 avcodec, avformat, avutil, swresample and swscale, plus libsoxr, zlib and necessary runtime libraries. It is separate from the MLP-only authoring DLLs in menu-bin. Packaging validates the DLL manifest, SHA-256, AMD64 architecture and normal/delay imports.

MLP preparation retains SWR, disabled dithering, the existing 20-bit rounding/clipping policy and WAVE normalization. The native MLP core remains pinned to SHA-256 ece6d0a8033a26e2528042a7b74c66c249ea3c8d7378c06809fb94c8f6bd79b8. Media DLL hashes participate in cache identity, so an upgrade rebuilds old-identity cache entries. Encoded files are never patched to obtain byte identity.

## Validation

- 109/109 compatibility tests passed.
- 202/202 PCM/MLP comparisons passed: 84 native format combinations with smooth/noise signals, plus resampling, 16/20/24-bit conversion, side-surround layouts and six-channel ALAC. Target PCM matches the reference exactly; whole MLP files match direct reference encoding byte-for-byte.
- 35 media integration assertions passed: Unicode tags/paths, ALAC packets, exact FLAC artwork preservation, PCM MD5, SOXR diagnostic sample counts, menu-frame pixels, invalid input, cancellation and concurrent decoders.
- 94 release regression assertions passed. With PATH restricted to Windows/.NET and GUI profiles deliberately naming missing FFmpeg/FFprobe executables, the package completed bilingual startup, three encoding fixtures, multilingual menu ISO authoring and output verification. Three fixture MLPs and six authored track MLPs match the old package byte-for-byte; 42 menu/still images have identical decoded pixels.
- Added the in-process FLAC metadata editor: it parses Vorbis Comment and PICTURE blocks, exports/imports artwork, atomically rewrites the metadata prefix and preserves audio frames byte-for-byte.
- The final directory and ZIP match the tested package by file hashes. The package contains 89 files; all files except the manifest itself pass manifest hash checks. All media DLLs are x64; FFmpeg/FFprobe executables, CLI and .NET runtime files are absent.

Scope: SOXR is used only for diagnostic resampled sample counts, never MLP preparation. Those counts match exactly. Two additional 24-bit SOXR PCM probes showed at most one LSB rounding difference across libsoxr builds; arbitrary cross-build SOXR output is not claimed to be byte-identical. The actual SWR/quantization/MLP encoding path requires and passes exact comparisons.

ISO lossless verification retains the existing first-track sampling policy; all six authored MLP files were compared separately. Authoring regression retains the baseline's short native-tool paths and MENU_INDEX_MIN_ALBUMS=99. The existing single-page index-menu issue is not claimed as fixed.

## Package size

Local output: build/inprocess-media-x64-release/DVD-Audio-Maker, with DVD-Audio-Maker.zip beside it.

| Measurement | Previous MLP-only library package | In-process media package |
|---|---:|---:|
| ZIP bytes | 33,669,495 | 35,333,326 |
| Unpacked bytes | 54,524,247 | 59,780,290 |

The ZIP is about 33.70 MiB, an increase of 1.59 MiB. Unpacked size increases by 5.01 MiB because functionality previously supplied by a user-installed FFmpeg is now bundled. ZIP format and compression settings are unchanged. ZIP SHA-256: cc5ce39ebbe684469b19dcb4a428908e6ed3fbae6b05b0cb27c028b34d03ef16.

## Development

See [Windows build instructions](../tools/win-build/README.en.md) for native builds and reuse of prebuilt media libraries. Routine C# debugging needs existing DLLs, without rebuilding native code.

```text
dotnet run --project tests/DvdaMaker.CompatibilityTests
dotnet run --project tests/DvdaMaker.CompatibilityTests -- --builtin-pcm-integration <empty-directory>
dotnet run --project tests/DvdaMaker.CompatibilityTests -- --builtin-media-integration <empty-directory>
```

Integration comparisons use reference FFmpeg/FFprobe programs to create and inspect fixtures; these are not GUI runtime requirements. Media-operation tests no longer need Metaflac. Full checks, build configuration, DLL hashes and limitations are in [inprocess-media-validation.json](inprocess-media-validation.json).
