# dvda-author source mirror

This directory records the controlled C/C++ changes applied to `dvda-author`
for DVD-Audio Maker. It is a review mirror for patches, provenance and isolated
build snapshots, not a complete standalone upstream source tree.

## Contents

```text
src/          modified author core sources
libutils/     modified common utility sources
MIRROR-NOTES.md
README.en.md
```

A complete build must use the full upstream working tree and apply
`docs/dvda-author-changes.patch`. Do not use this mirror to overwrite an
unverified working tree. Object files, executables, third-party libraries and
large assets are intentionally absent.

## Current integration

The Windows x64 release uses project-built author, media and image DLLs. Menu
media, subpictures, navigation and ISO writing use in-process C modules. The
user entry point is `DVD-Audio-Maker.exe`, with Check sources, Build discs and
Verify output. dry-run remains a developer CLI diagnostic.

The user package is `DVD-Audio-Maker-v1.0-win-x64.zip`; this maintenance mirror
is not copied into it. See the [Windows build instructions](../win-build/README.en.md),
[third-party components](../win-build/docs/THIRD-PARTY.en.md) and [native runtime
migration](../../docs/NO-EXTERNAL-RUNTIME-MIGRATION.en.md) for build inputs,
licenses and provenance.

## Synchronization rules

1. Build and test in the complete upstream working tree.
2. Regenerate `docs/dvda-author-changes.patch`.
3. Synchronize only changed `src/` and `libutils/` files, then compare the
   mirror, patch and full working tree byte-for-byte.
4. Do not copy objects, executables, generated Makefiles, third-party libraries
   or large assets.

## License

These files originate from `dvda-author` and include project changes. Use,
modification and redistribution must comply with the upstream and dependency
licenses. Keep the repository `LICENSE` and release NOTICE files.
