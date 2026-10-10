# dvda-menu direct link

The `direct-link` feature embeds the namespaced menu vendor archives in the Rust crate. It is intended for the main application and embedded author; production callers do not load a menu DLL.

## Build

The supported target is `x86_64-pc-windows-gnu`. With the repository `.venv` configured, Cargo automatically invokes `tools/win-build/build-menu-runtime.py --direct-link`. Set `DVDA_MENU_NATIVE_DIR` to a directory containing prebuilt `libdvda_menu_spu_vendor.a` and `libdvda_menu_nav_vendor.a` archives to skip regeneration.

The vendor builder requires MSYS2 MinGW64 GCC/binutils at `C:\msys64\mingw64\bin`, Cargo/rustc under `%USERPROFILE%\.cargo\bin`, and the repository `.venv\Scripts\python.exe`. The acceptance harness uses an image DLL only as a test callback provider; production callers supply the directly linked bridge callback instead.

The vendor archives remain a static third-party boundary. Vendor `exit`/`longjmp` paths are contained by C callback islands and never cross Rust frames. Calls are serialized and per-call state is reset; re-entry and poisoned-session calls fail closed.

## API

`run_spu(xml, input, output, read_rgba)` is an unsafe Rust wrapper because the supplied callback must satisfy the image-buffer contract; `run_navigation(xml, output)` is safe. Both return `Result<(), MenuError>`. The SPU image callback has the `ReadRgba` ABI used by `dvda_bridges::image::dvda_image_read_rgba`. Raw C callers use `MenuRequest` and `dvda_menu_run_spu` / `dvda_menu_run_navigation`.

Paths and XML must be valid UTF-8 C strings. Callback buffers and dimensions must obey the declared capacity and remain valid only for the call. Callbacks must not unwind, longjmp, retain pointers, or invoke menu entry points recursively. Failed calls may produce partial files; callers must discard them. The direct API reports a nonzero `MenuError` code on vendor/parser failure.

## Core and author integration

`dvda-native/direct-bridges` enables the bridge `menu` feature along with media/image. `dvda-bridges::menu` exports dvd-author's `dvda_menu_subpictures` and `dvda_menu_navigation` ABI using the static menu implementation; SPU uses the directly linked image callback when `image` is enabled. Core's direct feature skips the two menu DLL existence checks while retaining menu assets/fonts checks. Default-feature selection belongs to the main integration.

The legacy `author-loader` configuration still loads menu DLLs. Opting into `author-loader,menu` replaces only that menu loader and retains its image DLL callback provider. `build-rust-bridges.ps1 -Component author -DirectMenu` records that feature combination; `build-image-author.py` accepts it and verifies the static vendor manifest rather than copying menu DLLs. A complete author/main production run remains required before marking a release accepted.

## Frozen C versus Rust source comparison

The frozen `HEAD:tools/menu-native/readxml.c` uses XmlLite, qualified element/attribute names, descriptor parent states, ordered start/attribute/end callbacks, depth below ten, body accumulation, and ASCII-insensitive `1/on/yes` and `0/off/no` booleans. Rust retains that dispatch and boolean behavior. Literal attribute CR/LF/tab normalization now matches XML 1.0; character references preserve their referenced whitespace. Invalid `standalone` declarations are rejected. Both implementations reject DTDs, processing instructions, unknown descriptors and non-body non-whitespace text.

The frozen `session.c` linked-list registry is replaced by `resources.rs`'s thread-local vector with reverse-order cleanup and exact `(pointer, kind)` disowning. Resource acquisition/release, Win32/CRT UTF-8 adapters, private heap, and setjmp/longjmp callback containment remain in `session-rust.c`. Direct calls serialize and reset vendor state; the legacy session was one-shot.

Remaining parser parity limits: XmlLite can decode additional XML encodings, while Rust deliberately accepts UTF-8/XML 1.0 only; Rust validates the entire document before callbacks whereas C validates while dispatching, so malformed inputs can fail at different points and leave different partial files. Rust's tokenizer does not fully implement XmlLite's XML name/namespace and declaration grammar validation; generated menu XML does not exercise those forms. Qualified-name descriptor matching is preserved, rather than stripping namespace prefixes.

The third-party C algorithms are still compiled from `vendor/subgen*.c`, `dvdauthor.c`, `dvdcompile.c`, `dvdvml.c`, `dvdvmy.c`, `dvdifo.c`, `dvdvob.c`, `dvdpgc.c`, `dvdcli.c`, and `compat.c`, including DVD VM parsing and SPU/DVD binary construction. They have not been rewritten in Rust. There is no C++ menu replacement in this boundary. DLL removal and owned XML/resource/dispatch migration therefore do not establish an all-Rust menu implementation.

## Validation

`test-menu-direct.py --bridge-archive <libdvda_bridges.a>` links an executable against the dvd-author ABI and directly linked image callback, without loading an owned menu/image DLL. All four existing PAL/NTSC one-/two-menu fixtures passed SPU byte equality and every navigation file equality against frozen legacy outputs, plus repeated missing-file/invalid-boolean recovery and five null-argument ABI rejection checks. Reports are in `build\menu-author-abi-{pal-1,pal-2,ntsc-1,ntsc-2}\report.json`. Static crate API tests passed the same four fixtures in `build\menu-integrated-*`. All four menu unit tests passed after attribute/declaration fixes. Core direct-feature checking passed with `DVDA_FFMPEG_PREFIX`, `DVDA_MAGICK_WORK`, and `DVDA_MENU_NATIVE_DIR` pointing to the retained source-built dependencies; the earlier concurrent `encode_rust` blocker is no longer present. The 13 filtered legacy core menu tests also passed. Both legacy `author-loader` and `menu,author-loader` checks passed. The `author -DirectMenu` package builder succeeded, and its archive provenance acceptance/stale-hash rejection checks passed; its media-enabled archive is not supported by the menu-only harness linker (which omits FFmpeg import libraries). These are bridge/fixture evidence, not a completed main-program menu authoring acceptance run.

## Closure and limitations

This crate closes the menu DLL dependency for direct-link consumers, but it does not eliminate the vendor implementation or its Windows runtime dependencies (`ws2_32` and the GNU/Windows CRT). The legacy DLL build remains available for differential testing only. Cross-compilation, non-Windows targets, callback unwinding, and concurrent vendor calls are unsupported.
