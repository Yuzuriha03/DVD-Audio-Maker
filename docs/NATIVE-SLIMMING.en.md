# Native tool size reduction

[简体中文](NATIVE-SLIMMING.md) | [English](NATIVE-SLIMMING.en.md)

This records the ImageMagick optimization stage. The subsequent FFmpeg rebuild reduces the ZIP further to approximately 32.11 MiB; see [the rebuild record](MINIMAL-FFMPEG.en.md).

Date: 2026-10-02. The package remains Windows x64, GUI only, without a bundled .NET runtime. The ZIP format and CompressionLevel.SmallestSize setting are unchanged.

| Item | Shared-font package | Native optimization | Reduction |
|---|---:|---:|---:|
| ZIP | 92.30 MiB | 76.98 MiB | 15.32 MiB / 16.60% |
| Unpacked | 193.35 MiB | 172.53 MiB | 20.82 MiB / 10.77% |

ZIP size: 96,780,971 → 80,716,656 bytes, a cumulative 34.55% reduction from the original 123,318,008-byte v1.0 package.

Final package: `build/native-slim-x64-release/DVD-Audio-Maker.zip`.

SHA-256: `e39417ca1d30f072356002179eb554143e5d236d8cf2a5ee6a733270e5b2801f`.

## Changes

The existing `magick.exe` image core is retained. Each 6,847,032-byte `convert.exe` and `mogrify.exe` is replaced by a 5,632-byte project-owned native x64 forwarder. It passes the raw argument tail to the corresponding subcommand, preserving streams, exit status and working directory. A Windows Job Object terminates descendants when the wrapper is killed. Only Kernel32 is imported; no runtime or first-run extraction is added.

Six unused files are removed: `libfftw3-3.dll`, `liblqr-1-0.dll`, `libltdl-7.dll`, `libMagickCore-7.Q16HDRI-10.dll`, `libMagickWand-7.Q16HDRI-10.dll` and `libraqm-0.dll`. A 118-file SHA-256 profile restricts cleanup to the tested native bundle. Normal and delayed imports and binary/XML name references are checked, with retained dependencies protected transitively. Unknown builds, additional native components and referenced files are not automatically pruned.

Inputs are never rewritten. Every other native program and DLL, font and MLP encoder remains byte-identical. AVCodec, AVFormat and their imported H.265/AV1 and other libraries remain intact.

## Validation

- 108/108 compatibility tests, including PE normal/delay imports, x64 forwarder validation and unknown-build retention.
- 25 native forwarder checks: regional text, image expressions/quoting, paths with spaces, JPG/PNG/WebP, in-place operations, binary pipes, failed commands, missing core and forced cancellation.
- 90 package and native authoring checks. PATH was restricted to Windows system directories and .NET to avoid borrowing developer DLLs.
- Three whole-file MLP comparisons against existing baselines: 48 kHz/16-bit/mono, 48 kHz/24-bit/stereo and 96 kHz/24-bit/six channels.
- Complete authoring and verification for three Chinese/Japanese/Korean albums and six four-second tracks. All six MLP files are byte-identical and 42 menu, button and still images have identical pixels.
- Final repackaging changes only the two third-party notices; all runtime files match the tested candidate byte for byte. ZIP/manifest verification and Chinese/English GUI startup also pass.

Authoring uses an ASCII volume label, Windows short paths for native tools, one track per album menu page, still pictures, and an index threshold of 99. The baseline single-index-page `MENU_INDEX_ARROW_MISSING` issue is excluded from the successful integration scope. The existing ImageMagick font limitation under non-ASCII installation paths is also unchanged.

See [native-slimming-validation.json](native-slimming-validation.json) for evidence and the [Windows build instructions](../tools/win-build/README.en.md#native-tool-size-reduction) for reproducible commands. Existing optimized tool directories can be reused without recompiling the forwarder; the final package exercises that path.

This is a local candidate. Changes have not been committed or pushed, and the public v1.0 tag and asset are unchanged. Release archives are excluded from Git.
