# In-process image processing

Date: 2026-10-02.

Covers, backgrounds, text, three button states, album indexes, stills, font detection and visual verification use `image-native/dvda-image.dll`. The C# GUI and native author share this Windows x64 library. They do not launch ImageMagick command-line executables. Audio continues to use the existing media libraries and mlpencoder MLP core.

## Retained capabilities

- JPEG/PNG input and output, WebP cover input, transparency, resize/crop/composite, gradients, drawing, captions and statistics.
- Complete SC/JP/KR font faces share the existing TTC. No glyphs are removed; Unicode and space-containing paths work.
- Tailored static ImageMagick 7.0.8-47 Q16 HDRI, FreeType, JPEG/PNG/WebP/zlib. No external delegates, loadable coder modules, video or document conversion chain.
- Cancellation, timeout, malformed images and concurrent requests are handled. GUI writes are staged and replace the destination only on success.

This is an application-specific runtime; TIFF/PDF/SVG and WebP output are not provided.

## Validation scope

- 23 image operation checks cover formats, regional fonts, alpha/statistics, Unicode, native command whitespace, malformed data, cancellation, timeout and concurrency.
- Real GUI preview, ISO authoring and verification cover regular menus, a single index page and two index pages with navigation arrows. Windows child-process tracing finds no external ImageMagick, FFmpeg/FFprobe or SurCode encoder process.
- PATH contains only Windows/.NET; imported FFmpeg/FFprobe paths deliberately do not exist.
- Six regular-menu MLP files and six/fourteen index-fixture files retain exact baseline bytes. The MLP core and media DLLs are unchanged; no encoded-file patching is used.
- All 42 regular-menu images retain their dimensions. 24 are pixel-identical after decoding; 18 text overlays have glyph-edge differences. Content, layout and legibility were reviewed. Universal image pixel identity is not claimed.

The native index command now accepts leading whitespace. A single index page no longer incorrectly requires a paging arrow. See [the validation record](inprocess-images-validation.json) for execution results, hashes and package sizes. That record describes the image migration stage; menu media, subpictures, navigation and ISO writing are now in process. See the [subsequent migration checklist](NO-EXTERNAL-RUNTIME-MIGRATION.en.md).

## Development

See [Windows build instructions](../tools/win-build/README.en.md). ImageMagick/FreeType archive hashes are pinned; unchanged extracted upstream files are verified and patches are rebuilt from original content. Matching UCRT jump contexts and the current stack pointer keep codec errors/cancellation compatible with the protected .NET host stack. Host exception handlers and Windows protection policies are preserved.

The author is compiled in an isolated source snapshot. `native/author-inprocess-images.patch` records seven migrated image calls; the supplied native source tree is not overwritten. License notices and source provenance accompany the DLL.

The release remains x64, GUI-only and framework-dependent, requiring .NET 10 Desktop Runtime x64. config.env import and source-tree developer CLI/debugging remain available. Local release folders and ZIPs stay under ignored build directories and are not committed.

## Measured package sizes

| | Previous | In-process images | Saved |
|---|---:|---:|---:|
| ZIP | 35,333,326 bytes | 30,918,284 bytes | 4,415,042 bytes |
| Unpacked | 59,780,290 bytes | 57,659,623 bytes | 2,120,667 bytes |

ZIP format and compression settings are unchanged. Compatibility tests: 108/108 passed.
