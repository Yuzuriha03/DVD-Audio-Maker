# Reducing package size by sharing font data

[简体中文](SHARED-FONTS.md) | [English](SHARED-FONTS.en.md)

Follow-up: [native tool size reduction](NATIVE-SLIMMING.en.md) brings the package to 76.98 MiB. This page retains the measurements from the font-sharing stage.

Date: 2026-10-02. The package remains Windows x64, GUI only, and dependent on an installed .NET 10 Desktop Runtime. The ZIP format and `CompressionLevel.SmallestSize` setting are unchanged.

| Item | Published v1.0 | Shared fonts | Reduction |
|---|---:|---:|---:|
| ZIP | 117.61 MiB | 92.30 MiB | 25.31 MiB / 21.52% |
| Unpacked | 223.83 MiB | 193.35 MiB | 30.48 MiB / 13.62% |
| Font files | 47.05 MiB | 16.57 MiB | 30.49 MiB |

The ZIP decreases from 123,318,008 to 96,780,971 bytes. The local candidate is `build/font-shared-x64-final/DVD-Audio-Maker.zip`.

The SC, JP and KR fonts shared identical glyph outlines and other OpenType tables. Packaging stores identical tables once in `DvdaNotoCJK-Regular.ttc`, retaining all three complete regional faces. No glyphs are removed. ImageMagick selects the appropriate face through explicitly indexed names in `type-dvda-cjk.xml`. The application reads the collection directly; there is no first-run font extraction or download.

GUI defaults and the optional developer CLI launcher select the registered names. Saved paths to the original bundled OTF files inside the current installation directory migrate automatically; other custom user fonts remain unchanged. Packaging accepts either the original three OTF files or an existing shared collection, without modifying its input directory. All native tools, DLLs, MLP encoding code and menu assets are preserved.

Validation includes 107 compatibility tests; exact preservation of every original font table; identical regional rendering at 18 and 36 points; relocation with spaces; GUI startup in both languages; three complete MLP comparisons covering mono, stereo and six channels; and ZIP/manifest hash checks.

Complete authoring and disc verification pass for three Chinese, Japanese and Korean albums with six tracks and JPG, PNG and WebP covers. All six MLP files match the old package byte for byte, and 42 menu, button and still images have identical pixels.

The baseline font resolver rejects whitespace in font file paths, and the native menu tools have an executable-path quoting limitation. Authoring comparisons use Windows short paths to the same files, without modifying the original package. Both old OTF and new TTC font loading also encounter the existing native ImageMagick limitation with non-ASCII installation paths. Third-party binaries were not changed in this task.

The authoring fixture uses an ASCII volume label, four-second tracks, one track per album menu page, still pictures, and an index threshold of 99. A single index page triggered `MENU_INDEX_ARROW_MISSING` in the baseline. Index pages are therefore excluded from this authoring comparison and are not counted as a successful native integration check.

See [shared-font-validation.json](shared-font-validation.json) for the results. Final ZIP SHA-256: `2cdad133addfe64ef4527a829730597cbf6dbe7949d3e155ee059ed7cf643901`.

The published v1.0 tag and Release asset have not been replaced.
