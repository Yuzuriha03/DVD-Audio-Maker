# Windows x64 menu modules

Build from the repository root with python tools/win-build/build-menu-runtime.py --msys-root C:/msys64.

- dvda-menu-spu.dll: DVD subpicture/button encoding and multiplexing.
- dvda-menu-nav.dll: project AMGM menu navigation and VOB authoring.

vendor/ORIGIN.json records the imported dvdauthor subset, original hashes and source identity. Modified C files retain their copyrights; GPL text is in vendor/COPYING. Generated VM parser/lexer C files and grammar sources are included; building does not need Bison/Flex.

The ABI is in menu-api.h. Load a DLL, make one dvda_menu_run call, then unload it. Each load owns private legacy state. session.c tracks heap, files, directories and COM objects; menu_exit/menu_abort return errors. The author calls modules sequentially. readxml.c adapts Windows XmlLite, rejecting DTDs; image pixels use the existing dvda-image.dll callback.

Read-only disc verification is implemented in Rust in `dvda-native`. NativeDiscVerifier receives 2048-byte-aligned chunks (zero for EOF, negative for cancellation/error), compares MLP bytes or LPCM samples in AOB PES payloads, and reports failing track, offset and sector. It never changes files or input bytes and requires no verifier DLL.

Media encoding uses tools/win-build/native/dvda-menu-media.c and the shared FFmpeg profile in standard releases (the menu profile remains for standalone developer builds). This supports the GUI workflow, not the entire dvdauthor/spumux CLI. See the [migration checklist](../../docs/NO-EXTERNAL-RUNTIME-MIGRATION.md).

## Release use and validation

The standard Windows x64 package is DVD-Audio-Maker-v1.0-win-x64.zip. The GUI and author share one validated native DLL set extracted from the application; no separate media tools need to be installed. Module build/provenance files remain local and are excluded from the user ZIP.

The desktop workflow is Check sources, Build discs, Verify output. Developer dry-run remains available through cli.cmd build --dry-run, without a desktop preview action. Do not make GUI regression scripts request the removed Preview action; use Prepare for source checking. This README describes native maintenance and is not shipped as the release user guide.

Rust LPCM comparison follows the GPL DVD-Audio PCM packing in tools/dvda-author-mlp8/src/audio.c. It checks format headers and every sample, including odd-frame title padding. Rust verifier tests retain rate/bit-depth/channel-count, title boundary, corruption and short-track coverage; authored fixtures use the project-built author. GPL attribution remains applicable to the translated implementation.
