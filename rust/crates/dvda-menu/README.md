# dvda-menu

In-process Rust implementation of the DVD-Audio menu pipeline: subpicture
generation and navigation. No C menu vendor, static vendor archives or menu DLLs
are built or loaded; dvda-core and the embedded author call this crate directly.

## Modules

- `spu`: menu XML model, palette/button composition, DVD subpicture RLE encoder
  and MPEG-PS muxer.
- `nav`: navigation XML model/parser, DVD VM command compiler, private
  dvdauthor-data sector parsing, MPEG program-stream primitives, VOB scan/write,
  NAV pack (PCI/DSI) generation, colour remapping and AMGM menu orchestration.
- `resources`, `session`, `direct`: request ABI, resource handling and the
  `direct-link` entry points used by dvda-core/dvda-bridges.

## Build

The supported target is `x86_64-pc-windows-msvc`. The crate has no build script:
`cargo build -p dvda-menu` needs no `DVDA_MENU_NATIVE_DIR` and no vendor archive.
The `direct-link` feature exposes the in-process entry points; it does not embed
third-party archives.

## API

- `run_spu_files(xml, input, output, &mut ReadRgba)` and `run_navigation(xml, output)`.
- Raw callers use `MenuRequest` with `dvda_menu_run_spu` / `dvda_menu_run_navigation`.
- `run_spu` is unsafe because the RGBA callback must honour the image ABI; the
  callback must not unwind, longjmp, retain pointers or call back into the menu
  APIs. Failed runs may leave partial files that callers must discard.

## Validation

`cargo test -p dvda-menu` runs the unit and differential tests, including
byte-identical SPU output, VOB/NAV pack comparison against frozen C baselines,
multi-page menu assembly and colour-remapping tests.

`tools/win-build/test-rust-menu-session.py` drives fresh-process SPU/navigation
sessions (no vendor directory required); production mode uses the assembled
runtime. `tools/win-build/test-rust-workflow.ps1 -Mode Functional` runs the
opt-in workspace integration tests that need source-built media/image runtimes.

XML input is UTF-8/XML 1.0 only and is validated before any output is written.
