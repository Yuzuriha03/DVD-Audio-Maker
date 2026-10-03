# Removing External Runtime Dependencies

[简体中文](NO-EXTERNAL-RUNTIME-MIGRATION.md)

This document lists only the external runtime functions that still need to move into the current GUI full-disc workflow. Project code, the MLP core, the in-process media/image libraries and \`dvda-author-dev.exe\` are excluded because they can already be built from this repository and its existing native source. Their compilers and build inputs remain development dependencies, not user runtime dependencies.

## Boundary

**Current status:** the ISO writer and FLAC metadata migrations are complete. Menu-off builds now create ISO9660 images inside `dvda-author-dev.exe` and no longer require or package `mkisofs.exe`. M4A/ALAC organization now reads and rewrites FLAC metadata in process and no longer starts `metaflac.exe`. Menu-on authoring tools remain listed below as migration work.

The target release keeps the application and its DLLs, Windows x64 system DLLs, and .NET 10 Desktop Runtime x64 (or a bundled runtime for a self-contained build). Menu-off builds no longer need the ISO executable; menu-on builds still carry the menu authoring tools until their migrations are complete. Historical regression programs, old FFmpeg branches, ImageMagick reference tools, CLI comparison paths and build tools are outside this list.

## Required migrations

### ISO writer

**Status: implemented.** The new C `dvda_iso_write` implementation writes primary and terminator descriptors, both path tables, directory records, 2048-byte sectors and streamed file content. `DiscBuildExecutor` passes the destination to the author, keeps its capacity/staging/publish checks, and no longer starts an external process. The generated image is read back by `Iso9660Reader`; compatibility coverage is 109/109. The release packager omits `mkisofs.exe`, while the legacy `DVDA_MKISOFS` configuration key remains readable for old config files.

The previous implementation used `mkisofs.exe`; that dependency has been replaced by the C writer described above. The reader remains responsible for verification.

### DVD-Video menu authoring

\`dvdauthor.exe\` currently creates VMG/VTS menus, PGCs, buttons, navigation commands, VOB/IFO/BUP files and connections between menus and DVD-Audio entries. \`dvda-author-dev.exe\` already provides the project's DVD-Audio authoring, but it does not automatically provide these DVD-Video menu functions. They may be merged into its authoring core or exposed through an in-process project DLL.

### Subpicture buttons

\`spumux.exe\` and \`spuunmux.exe\` provide button states, transparency, color tables, subpicture RLE encoding/parsing, coordinates and navigation validation. These operations must become an in-process menu component.

### MPEG-2 menu video

\`jpeg2yuv.exe\` and \`mpeg2enc.exe\` provide image-to-YUV conversion and PAL/NTSC MPEG-2 menu video. The replacement must cover 720x576/720x480, frame rates, sequence headers, GOP/end markers, DVD menu bitrate and I-frame compatibility.

### Menu audio and multiplexing

\`mp2enc.exe\` and \`mplex.exe\` provide menu audio and MPEG-PS multiplexing. The replacement must cover menu silence/background audio, audio encoding, PTS/SCR, pack alignment and VOB output.

## Later migrations

The optional M4A/ALAC-to-FLAC organization feature is now handled by `FlacMetadataEditor`. It parses Vorbis Comment and PICTURE blocks, exports/imports artwork and atomically rewrites the metadata prefix while copying audio frames byte-for-byte. It does not start `metaflac.exe`, and a new FLAC audio encoder is unnecessary.

Full in-process verification can later add an ISO/AOB reader, MLP decoder, full-track PCM comparison and menu/PTS parsers. The current first-track sample can remain while authoring migration proceeds.

## Explicit exclusions

The following are not migration targets because they are already buildable from the repository or existing project native source, or are development/regression-only:

- C# GUI, CLI, shared libraries and tests;
- \`mlp_encoder.dll\` and \`Native/source\`;
- \`dvda-media.dll\`, \`dvda-image.dll\` and bridge code;
- \`dvda-author-dev.exe\` and its project patches;
- MLP parsing, caching, disc planning, menu planning and logging;
- historical external FFmpeg/FFprobe comparison paths;
- ImageMagick reference tools, magick-shim and image regression helpers;
- eac3to, original SurCode and old \`surcode.exe\` entry points;
- Python, MSYS2, MinGW, Make, GPG and NASM build tools;
- independent regression packages and third-party reference binaries.

Exclusion means these functions do not need to be reimplemented as runtime components outside the repository. Developers may still use the tools to rebuild or test native components.

## Order

Items 1 through 3 below are complete. The next remaining runtime migrations are menu MPEG video, menu audio/multiplexing, subpictures and DVD-Video authoring.

1. ISO writer. **Complete.**
2. Remove `mkisofs.exe` in menu-off mode. **Complete.**
3. FLAC metadata editing. **Complete.**
4. Menu MPEG video, menu audio and multiplexing.
5. Subpictures and DVD-Video authoring.
6. Connect the menu API to `dvda-author-dev`.
7. Full in-process verification.

Use C language for all new codes much as possible. Each step needs regression comparison against current directory layouts, track tables, navigation, PTS, AOB and ISO output. MLP acceptance remains byte identity when target PCM, encoding settings and metadata context match.

The current boundary is therefore: a menu-off disc is fully in-process for authoring and ISO creation; menu-on discs still invoke the listed menu encoders and DVD-Video authoring tools until those C implementations are migrated.
