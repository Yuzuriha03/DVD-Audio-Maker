# Modified dvda-author source mirror

[简体中文](README.md) | [English](README.en.md)

This directory contains core C/C++ files modified by DVD-Audio Maker, primarily for:

- Reviewing and searching the modified implementation in this repository.
- Recording MLP, timeline, menu, font and Windows compatibility changes.
- Comparing against the full working tree or patch file.
- Keeping the complete upstream source, third-party dependencies and build outputs out of the main repository.

## Important

**This is not a complete, independently buildable dvda-author source tree.**

It normally contains only:

```text
src/          Modified dvda-author core source
libutils/     Modified shared utility source
MIRROR-NOTES.md
README.md
```

It omits content required for a complete build, including:

```text
configure
configure.ac
Makefile.in
libfixwav/
menu/
m4.extra.dvdauthor/
dvdauthor-0.7.1/
local.w10/
```

Do not run `configure`, `make` or Windows toolchain scripts in this directory.

## Authoritative source

Build from a complete `dvda-author` working tree with this project's changes applied. The recommended upstream pin is `8fca43a`:

```bat
git clone https://github.com/fabnicol/dvda-author "D:/work/dvda-author"
cd /d "D:/work/dvda-author"
git checkout 8fca43a
git apply "D:/work/DVD-Audio-Maker/docs/dvda-author-changes.patch"
```

Complete patch set:

- [`../../docs/dvda-author-changes.patch`](../../docs/dvda-author-changes.patch)

Change descriptions and rationale:

- [`../../docs/DVDA-AUTHOR-CHANGES.md`](../../docs/DVDA-AUTHOR-CHANGES.en.md)

Experiments that are not enabled:

- [`../../docs/DVDA-AUTHOR-DISABLED.md`](../../docs/DVDA-AUTHOR-DISABLED.en.md)

The product targets Windows x64. Native maintenance uses MSYS2/MinGW-w64 on Windows; routine C# development and release assembly reuse verified artifacts. See [Windows build instructions](../win-build/README.en.md).

## Current menu image calls

This directory and the base patch remain review copies of the underlying implementation. The current author is built by [build-image-author.py](../win-build/build-image-author.py) from an isolated snapshot of the full tree. The script migrates seven external ImageMagick calls to the x64 image DLL. See [author-inprocess-images.patch](../win-build/native/author-inprocess-images.patch) and the [native loader](../win-build/native/author-image-loader.c).

Keep base changes separate from this build-time transformation. The script expects the base tree without the image delta already applied; do not manually apply the same delta before invoking it. It does not overwrite the supplied full tree. Input and output hashes are recorded in build/image-author/author-build.json.

The GUI and native author use image-native/dvda-image.dll. Menu media, subpictures, navigation and ISO writing now use in-process C implementations; see the [migration checklist](../../docs/NO-EXTERNAL-RUNTIME-MIGRATION.en.md). See [in-process images](../../docs/INPROCESS-IMAGES.en.md) for capabilities and validation limits.

## Main areas of change

The mirrored files may include changes in these categories:

- 24-bit MLP input and newer FFmpeg API support.
- MLP frames, track boundaries and byte alignment.
- ATSI, AOB, PTS and title timeline fixes.
- Menu generation, AMG/ASVS links and playback still pictures.
- SC, JP and KR font selection by text language.
- Windows/MinGW paths, processes, pipes and UTF-8 compatibility.
- Upstream crash fixes, buffer limits and resource cleanup.

The patch and full working tree define the actual changes. Do not infer complete coverage from this directory alone.

## Synchronization rules

When changes in the full working tree are updated:

1. Update that working tree and confirm it builds and passes tests.
2. Regenerate `docs/dvda-author-changes.patch`.
3. Synchronize modified `src/` and `libutils/` files into this mirror.
4. Compare the mirror, applied patch and full working tree, requiring byte-identical corresponding files.
5. Do not copy object files, executables, generated Makefiles, third-party libraries or large assets.

This mirror is a review copy. Do not use it to overwrite an unverified full working tree.

## License

These files originate from `dvda-author` and include project modifications. See the licensing and third-party notes:

- [`../../LICENSE`](../../LICENSE)
- [`../../docs/LICENSING.md`](../../docs/LICENSING.en.md)

Use, modification and redistribution must comply with the upstream project's and dependencies' licenses.
