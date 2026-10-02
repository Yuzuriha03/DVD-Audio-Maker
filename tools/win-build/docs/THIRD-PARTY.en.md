# Components and licenses (THIRD-PARTY)

[简体中文](THIRD-PARTY.md) | [English](THIRD-PARTY.en.md)

This package redistributes several open-source programs, including modifications for this use. The inventory follows.

## Main programs

| Component | Version | License | Notes |
|---|---|---|---|
| **dvda-author** (`menu-bin/dvda-author-dev.exe`) | Upstream `8fca43a` plus project changes | **GPL v3** | See `LICENSE`. Changes include 24-bit lossless MLP, FFmpeg 9 compatibility, menu/still-picture fixes and Windows support |
| **dvdauthor** (`menu-bin/dvdauthor.exe`) | 0.7.1 | **GPL v2** | Includes AMGM menus and `jump group` patches; upstream does not recognize that navigation syntax |
| **spumux** / **spuunmux** | 0.7.1 | **GPL v2** | Built from the same source as dvdauthor |

The main programs are GPL-licensed. The package includes the license text in `LICENSE`.
Upstream source is available from `github.com/fabnicol/dvda-author`; project modifications
are provided as a patch applicable with `git apply`.

## Bundled tools (binary redistribution)

| Component | Version | License |
|---|---|---|
| **cdrtools / mkisofs** (`mkisofs.exe`) | 3.02a | **CDDL** for the mkisofs component |
| **mjpegtools** (`jpeg2yuv` / `mpeg2enc` / `mplex` / `mp2enc`) | 2.1.0 | **GPL v2** |
| **ImageMagick** (`magick` / `convert` / `mogrify` plus 11 `.xml` files) | 7.0.8-47 Q16 | **ImageMagick License**, Apache-2.0-style |
| Other DLLs | Respective MSYS2/MinGW-w64 builds | See each project |

## Fonts

| File | License |
|---|---|
| `menu-bin/fonts/NotoSansCJKsc-Regular.otf` | **SIL Open Font License 1.1**（Google Noto Sans CJK） |
| `menu-bin/fonts/NotoSansCJKjp-Regular.otf` | Same as above |
| `menu-bin/fonts/NotoSansCJKkr-Regular.otf` | Same as above |

These three files are standalone SC, JP and KR faces extracted from `NotoSansCJK-Regular.ttc`,
so ImageMagick can load them by path. See `make-menu-font.sh`.

**Why three fonts:** the issue is glyph selection, not missing characters. Each face covers
Chinese, Japanese, Korean and Latin, but shared Han characters have regional glyph variants (for example 直/骨/令/次/别).
The menu chooses a face for each text item: Chinese → SC, Japanese → JP, Korean → KR.

OFL permits redistribution and embedding with software. Do not sell the font by itself.

## This project's own programs

The bundled `app/dvda.exe` and its C# source are **GPL v3**, matching dvda-author.
The release no longer contains or invokes Python business scripts.

---

## GPL compliance

This package distributes executables. If you redistribute it or a modified version:

- Retain `LICENSE` and this document.
- Provide the corresponding source: upstream plus this project's patch. GPL v3 section 6 permits
  certain written-offer arrangements instead of accompanying source; providing source access alongside the package is the straightforward approach described here.
