# Rebuilding the native FFmpeg dependencies

[简体中文](MINIMAL-FFMPEG.md) | [English](MINIMAL-FFMPEG.en.md)

Date: 2026-10-02. This follows font sharing and the ImageMagick forwarder changes. The release remains Windows x64, GUI only and framework dependent. ZIP format and compression settings are unchanged.

## Result

| Metric | Previous optimized package | Rebuilt package |
|---|---:|---:|
| ZIP | 76.98 MiB | 32.11 MiB |
| Extracted | 172.53 MiB | 52.00 MiB |

The ZIP is approximately 58.29% smaller in this round, or 72.70% below the original 117.61 MiB v1.0 package. Exact sizes, final SHA-256 and validation results are in [minimal-ffmpeg-validation.json](minimal-ffmpeg-validation.json).

Final local package: `build/ffmpeg-minimal-x64-release/DVD-Audio-Maker.zip`. Build artifacts remain ignored by Git. This work does not commit, push or replace the public v1.0 release.

## Changes

- Rebuild `avcodec-63.dll`, `avformat-63.dll` and `avutil-61.dll` from signature-verified, unmodified FFmpeg 9.0.2 source. ABI versions remain 63.1.102, 63.1.102 and 61.1.102.
- Enable only the MLP decoder, encoder, parser, raw MLP demuxer/muxer and file/pipe protocols. The encoder preserves the native tool's existing interface; GUI encoding still uses the separate encoder DLL.
- Disable video codecs, networking, external codec libraries and unrelated modules. Retain x86 assembly and runtime CPU detection. The three rebuilt libraries total 2,407,424 bytes.
- Remove 74 newly unused DLLs, including x264/x265, AOM/dav1d/SVT-AV1, JPEG XL, Rsvg and networking dependencies. Together with the previous six removals, the native bundle contains 80 fewer DLLs than v1.0.
- Keep shared runtimes that are still imported, including libwinpthread. Fonts, the ImageMagick core, authoring executables and menu video tools retain their original bytes.

The original bundle and replacement hashes are pinned in `NativeOptimizationProfile.json` and `MinimalFfmpegProfile.json`. The packager checks normal/delay imports and binary/XML references and preserves transitive dependencies before deleting files. Unknown files or builds disable automatic pruning. Input directories are not overwritten.

GUI audio conversion, decoding and disc verification still use the externally configured FFmpeg. Neither the MLP encoder DLL nor its managed assembly changes. Encoded files receive no output patching.

## Validation

- All 108 project compatibility tests pass, including rejection of unverified replacement libraries.
- Verify 86 imported FFmpeg symbols used by existing tools and the probe. All three new libraries are native AMD64 PE files with the same ABI versions.
- Generate 84 PCM configurations: 44.1/48/88.2/96 kHz × 16/20/24 bit × 1–6 channels, plus 176.4/192 kHz × 16/20/24 bit × 1–2 channels. Each channel count uses the project's default layout. Signals exercise distinct channels, low sample bits, silence and nonintegral access-unit lengths.
- Encode with the unchanged DLL and decode with both original and rebuilt FFmpeg libraries. All 84 outputs match byte for byte, restore the original PCM and agree on frames/rate/depth/channels; trailing padding is zero. Verify the actual loaded DLL paths, with a restricted PATH.
- All 91 package regression checks pass: Chinese/English GUI startup, three existing whole-file MLP references, full authoring and lossless verification of three multilingual albums with six four-second tracks, six identical authored MLP files, and 42 identical menu/button/still image pixel outputs.
- Compare every runtime file in the final package against the tested candidate, then verify the ZIP, SHA-256 manifest and bilingual GUI startup. The archive remains standard Deflate without bundled .NET or CLI.

The 84 configurations compare old/new decoding and input PCM. They do not claim the original SurCode generated or supported every configuration; high rates unsupported by SurCode are covered by the encoder regression.

The authoring fixture retains its ASCII volume label and Windows 8.3 native tool paths. The existing single-index-page `MENU_INDEX_ARROW_MISSING` issue is outside the passing scope; the index threshold is 99. The existing ImageMagick font limitation for non-ASCII installation paths is also unchanged. The validation record retains these boundaries.

## Rebuilding

Ordinary C# GUI development still requires only the .NET 10 SDK. Optional native maintenance additionally requires Windows x64, Python 3.12+, MSYS2 bash/make/gpg and MinGW-w64 GCC/strip. The script does not modify the MSYS2 installation or use WSL.

```bat
python tools\win-build\build-minimal-ffmpeg.py --msys-root "D:\dev\msys64"
```

Source, NASM, logs, import libraries and DLLs go into the ignored `build/ffmpeg-minimal` directory. Use `--work-directory` or `--jobs` to adjust. Paths with spaces use Windows 8.3 names; choose a space-free work directory if short names are unavailable.

The recipe pins source/NASM hashes and verifies the FFmpeg release signature and extracted source. See [minimal-ffmpeg-build.json](minimal-ffmpeg-build.json) for provenance, the compiler version and full configuration. A different compiler, path or configuration can change the resulting hashes; this is a repeatable recipe, not a promise of identical binaries across toolchains.

Package from a full prebuilt native tool directory:

```bat
tools\win-build\build-all.cmd --source "D:\dev\winbuild\src" --prebuilt "D:\dev\winbuild\menu-bin" --framework-dependent --magick-shim "build\magick-shim\magick-shim.exe" --ffmpeg-libraries "build\ffmpeg-minimal\install\bin"
```

A previously validated minimal `menu-bin` can be reused as `--prebuilt` without either replacement option. Developer CLI/F5 entrypoints remain available in the source tree. Native test helpers are not shipped.

For a new native configuration, first put the three rebuilt DLLs in a separate candidate tool directory and compare it to the baseline. The output directory must not already exist:

```bat
python tools\win-build\test-minimal-ffmpeg.py --baseline "old-package\menu-bin" --candidate "candidate\menu-bin" --prefix "build\ffmpeg-minimal\install" --msys-root "D:\dev\msys64" --output "build\ffmpeg-validation-new"
```

`native/mlp-decode-probe.c` validates the native libav decoding path; `native/pe_dependencies.py` inspects PE interfaces. Update `MinimalFfmpegProfile.json` only after interface, audio and complete authoring validation. The packager rejects explicit replacements whose hashes do not match the validated profile.
