# Windows x64 menu and disc verification modules

Build from the repository root with python tools/win-build/build-menu-runtime.py --msys-root C:/msys64.

- dvda-menu-spu.dll: DVD subpicture/button encoding and multiplexing.
- dvda-menu-nav.dll: project AMGM menu navigation and VOB authoring.
- dvda-disc-verify.dll: read-only streaming comparison of all MLP bytes in AOB PES payloads.

vendor/ORIGIN.json records the imported dvdauthor subset, original hashes and source identity. Modified C files retain their copyrights; GPL text is in vendor/COPYING. Generated VM parser/lexer C files and grammar sources are included; building does not need Bison/Flex.

The ABI is in menu-api.h. Load a DLL, make one dvda_menu_run call, then unload it. Each load owns private legacy state. session.c tracks heap, files, directories and COM objects; menu_exit/menu_abort return errors. The author calls modules sequentially. readxml.c adapts Windows XmlLite, rejecting DTDs; image pixels use the existing dvda-image.dll callback.

The stateless disc verifier receives 2048-byte-aligned chunks (zero for EOF, negative for cancellation/error), and reports failing track, offset and sector. It never changes files or input bytes. NativeDiscVerifier is the managed adapter.

Media encoding uses tools/win-build/native/dvda-menu-media.c and the FFmpeg menu profile. This supports the GUI workflow, not the entire dvdauthor/spumux CLI. See the [migration checklist](../../docs/NO-EXTERNAL-RUNTIME-MIGRATION.md).
