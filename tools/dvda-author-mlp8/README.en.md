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

```bash
git clone https://github.com/fabnicol/dvda-author /path/to/dvda-author
cd /path/to/dvda-author
git checkout 8fca43a
git apply /path/to/DVD-Audio-Maker/docs/dvda-author-changes.patch
```

Complete patch set:

- [`../../docs/dvda-author-changes.patch`](../../docs/dvda-author-changes.patch)

Change descriptions and rationale:

- [`../../docs/DVDA-AUTHOR-CHANGES.md`](../../docs/DVDA-AUTHOR-CHANGES.en.md)

Experiments that are not enabled:

- [`../../docs/DVDA-AUTHOR-DISABLED.md`](../../docs/DVDA-AUTHOR-DISABLED.en.md)

Actual build entry point:

- Linux/WSL：[`../../build_dvda_author_mlp.sh`](../../build_dvda_author_mlp.sh)
- Windows/MSYS2：[`../win-build/README.md`](../win-build/README.en.md)

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
