# Components and licenses (THIRD-PARTY)

[简体中文](THIRD-PARTY.md) | [English](THIRD-PARTY.en.md)

This package redistributes several open-source programs, including modifications for this use. The inventory follows.

## Main programs

| Component | Version | License | Notes |
|---|---|---|---|
| **dvda-author** (`menu-bin/dvda-author-dev.exe`) | Upstream `8fca43a` plus project changes | **GPL v3** | See `LICENSE`. Changes include 24-bit lossless MLP, FFmpeg 9 compatibility, menu/still-picture fixes and Windows support |
| **dvdauthor/spumux C subset** (dvda-menu-nav.dll / dvda-menu-spu.dll) | 0.7.1 + AMGM/project changes | **GPL v2 or later** (source headers) | tools/menu-native/vendor; ORIGIN.json and COPYING; menu-NOTICE.txt |

The main programs are GPL-licensed. The package includes the license text in `LICENSE`.
Upstream source is available from `github.com/fabnicol/dvda-author`; project modifications
are provided as a patch applicable with `git apply`.

## Bundled tools (binary redistribution)

| Component | Version | License |
|---|---|---|
| **ImageMagick** (`image-native/dvda-image.dll`) | 7.0.8-47 Q16 HDRI | **ImageMagick License**, Apache-2.0-style |
| **FFmpeg libraries** (menu-bin, media-native) | 9.0.2; Windows x64 builds tailored to each role | **GPL v3 or later** for this configuration |
| Other DLLs | Respective MSYS2/MinGW-w64 builds | See each project |

## FFmpeg build

FFmpeg libraries use unmodified source from `https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz`. Source SHA-256: `8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e`. menu-bin includes MLP authoring plus MPEG-2/MP2 menu encoding and DVD MPEG-PS muxing; the author dynamically links three source-built shared libraries; `media-native` adds FLAC/ALAC/AAC/PCM/MLP decoding, FLAC/PCM output, SWR/SOXR resampling and MPEG-2/PNG/JPEG processing for the GUI process. The MLP encoding core remains separate. No FFmpeg command-line executable is shipped.

Build recipes are tools/win-build/build-minimal-ffmpeg.py (mlp / menu / media profiles) and build-media-bridge.py. media-native/media-build.json records configuration, source hashes and all DLL hashes/imports. libsoxr, zlib and their runtime dependencies are included; FFmpeg / FFprobe command-line executables are not. The project's dvda-media.dll interface is implemented in tools/win-build/native/dvda-media.c under the project's GPL v3 license. See docs/MINIMAL-FFMPEG.en.md and docs/INPROCESS-MEDIA.en.md for configuration and validation.


## Fonts

| File | License |
|---|---|
| `menu-bin/fonts/DvdaNotoCJK-Regular.ttc` (SC / JP / KR faces) | **SIL Open Font License 1.1**（Google Noto Sans CJK） |

Identical OpenType tables are shared in one collection containing the complete SC, JP and KR faces. Every glyph, character mapping and regional distinction is retained. `image-native/type.xml` registers three names with explicit face indices, so ImageMagick selects the intended region instead of always loading face 0.

**Why three faces:** the issue is glyph selection, not missing characters. Each face covers
Chinese, Japanese, Korean and Latin, but shared Han characters have regional glyph variants (for example 直/骨/令/次/别).
The menu chooses a face for each text item: Chinese → SC, Japanese → JP, Korean → KR.

OFL permits redistribution and embedding with software. Do not sell the font by itself.

## This project's own programs

`DVD-Audio-Maker.exe`, the optional developer CLI `dvda.exe`, and their C# source are **GPL v3**, matching dvda-author.
The release no longer contains or invokes Python business scripts.

---

## GPL compliance

This package distributes executables. If you redistribute it or a modified version:

- Retain `LICENSE` and this document.
- Provide the corresponding source: upstream plus this project's patch. GPL v3 section 6 permits
  certain written-offer arrangements instead of accompanying source; providing source access alongside the package is the straightforward approach described here.

## Tailored image runtime

The image DLL statically includes the required ImageMagick core, FreeType outline fonts, JPEG/PNG codecs, WebP decoding and zlib. JPEG/PNG are readable and writable; WebP is input-only. External delegates and loadable coder modules are disabled. The project bridges are tools/win-build/native/dvda-image.c and author-image-loader.c; recipes are build-image-runtime.py, build-image-bridge.py and build-image-author.py. Runtime provenance and source patch hashes are recorded in image-native/image-build.json and author-build.json. Component license texts are included in image-native/NOTICE.txt.
