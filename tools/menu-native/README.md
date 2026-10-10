# Windows x64 menu modules

Build from the repository root with python tools/win-build/build-menu-runtime.py --msys-root C:/msys64.

- dvda-menu-spu.dll: DVD subpicture/button encoding and multiplexing.
- dvda-menu-nav.dll: project AMGM menu navigation and VOB authoring.

vendor/ORIGIN.json records the imported dvdauthor subset, original hashes and source identity. Modified C files retain their copyrights; GPL text is in vendor/COPYING. Generated VM parser/lexer C files and grammar sources are included; building does not need Bison/Flex.

The ABI is in menu-api.h. Load a DLL, make one dvda_menu_run call, then unload it. Each load owns private legacy state; calls must be sequential, non-reentrant and remain on one thread. The image callback must return an error status, never throw, unwind, or longjmp. Rust `dvda-menu` owns UTF-8 XML tokenization, validation, descriptor dispatch, boolean parsing and a thread-local LIFO file/directory/COM resource registry. DTDs, processing instructions, invalid XML characters, duplicate attributes, malformed declarations and excessive depth are rejected. XML 1.0/UTF-8 is the supported input format.

`session-rust.c` retains the Windows private heap and CRT adapters because the third-party vendor algorithm allocates C structures and exits with `longjmp`. Rust enters every vendor element/attribute callback and parser-body allocation through a **local C setjmp island**. Vendor exits jump only over C frames, returning a status to Rust; Rust exports catch panics and return errors. Resource registration returns from Rust before a C allocation failure exits. The two vendor `readxml` call sites are top-level, not recursively entered by callbacks. Final cleanup drains Rust ownership before destroying the C heap. XML is validated before vendor callbacks, but the transaction may already have created an empty output; callers must discard failed transaction output.

Obsolete owned `session.c`/`readxml.c` were retired only after frozen-C PAL/NTSC, one/two-PGC byte parity passed. The external frozen DLL oracle remains intact. The build links `rust/crates/dvda-menu` and records Rust inputs/compiler/archive hashes in `menu-build.json`; there is no legacy-C build mode. This is a real parser/resource-bookkeeping migration, **not** a rewrite of vendor SPU/VM/VOB algorithms or the Windows heap/CRT boundary. Image pixels still use the existing callback; media/image bridge selection is a separate integration.

`tools/win-build/test-menu-rust.py` compares frozen-C/Rust SPU and every navigation output byte, including button highlights and pre/button/post VM bodies. It generates MPEG using developer FFmpeg, exercises UTF-8 XML/image/MPEG/output paths, malformed XML and vendor-fatal booleans, repeated fresh-load failure→success recovery and Windows handle counts. Pass `--runtime`, `--oracle`, `--image-runtime` (DLL), `--assets` (black_PAL_720x576.png and silence.wav), `--ffmpeg` and a nonexistent `--output` directory. Final Rust-only build passed PAL/NTSC × one/two PGCs (146 checks each, 584 total), including cross-menu jumps. Reports in `build/menu-production-{pal,ntsc}-{1,2}/report.json` include runtime/oracle DLL hashes. This is not exhaustive coverage of arbitrary menu structures or media profiles.

## Integration handoff

Production build from the repository root:

```powershell
E:\Python314\python.exe tools\win-build\build-menu-runtime.py --msys-root C:\msys64 --output build\menu-rust-production
```

Implementation sources are `rust/crates/dvda-menu`, `tools/menu-native/session-rust.c`, the retained ABI headers, and `tools/menu-native/vendor`. Verified output is `build/menu-rust-production`; `menu-build.json` records compiler versions, source/archive hashes and DLL imports. DLL dependencies are Windows system/API-set libraries only; no Rust runtime DLL, XmlLite, or shlwapi is required.

Acceptance fixture source: `D:\dvda-release-build-20261007\menu-test-runtime\source`, assets in its `menu` child. Its sibling `D:\dvda-release-build-20261007\menu-test-runtime\menu-bin` is the **frozen-C oracle**, not the newly built output; it was not overwritten. Image callback DLL: `D:\dvda-release-build-20261007\image-native\dvda-image.dll`.

Parent integration must copy both new DLLs together into the selected runtime's sibling `menu-bin`, retain local provenance/NOTICE and vendor licensing, and update parent-owned build/packaging validation to invoke the Rust-only build (no `--legacy-c`). Preserve the frozen oracle before replacing its directory. No production package/toolchain switch is performed here.

Remaining limitations: one-load/sequential ABI; failed outputs must be discarded; Rust allocation exhaustion may abort (not forced in tests); directory-enumeration UTF-8 coverage is not explicit. XML supports the documented XML 1.0 UTF-8 subset, not exhaustive declaration/name grammar validation. Vendor algorithms and the safe C setjmp/private-heap/CRT boundary intentionally remain.

Read-only disc verification is implemented in Rust in `dvda-native`. NativeDiscVerifier receives 2048-byte-aligned chunks (zero for EOF, negative for cancellation/error), compares MLP bytes or LPCM samples in AOB PES payloads, and reports failing track, offset and sector. It never changes files or input bytes and requires no verifier DLL.

Media encoding uses tools/win-build/native/dvda-menu-media.c and the shared FFmpeg profile in standard releases (the menu profile remains for standalone developer builds). This supports the GUI workflow, not the entire dvdauthor/spumux CLI. See the [migration checklist](../../docs/NO-EXTERNAL-RUNTIME-MIGRATION.md).

## Release use and validation

The standard Windows x64 package is DVD-Audio-Maker-v1.0-win-x64.zip. The GUI and author share one validated native DLL set extracted from the application; no separate media tools need to be installed. Module build/provenance files remain local and are excluded from the user ZIP.

The desktop workflow is Check sources, Build discs, Verify output. Developer dry-run remains available through cli.cmd build --dry-run, without a desktop preview action. Do not make GUI regression scripts request the removed Preview action; use Prepare for source checking. This README describes native maintenance and is not shipped as the release user guide.

Rust LPCM comparison follows the GPL DVD-Audio PCM packing in tools/dvda-author-mlp8/src/audio.c. It checks format headers and every sample, including odd-frame title padding. Rust verifier tests retain rate/bit-depth/channel-count, title boundary, corruption and short-track coverage; authored fixtures use the project-built author. GPL attribution remains applicable to the translated implementation.
