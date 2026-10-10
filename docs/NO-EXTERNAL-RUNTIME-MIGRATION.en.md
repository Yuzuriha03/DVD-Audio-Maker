# Native runtime migration

The Windows x64 workflow is now implemented by the Rust application and the
project-built native components. The runtime boundary is deliberate: the GUI
does not invoke command-line copies of FFmpeg, FFprobe, ImageMagick, Metaflac,
eac3to, SurCode, dvdauthor or mkisofs.

## Completed boundaries

| Area | Native implementation |
|---|---|
| Source media | Linked Rust media bridge calling third-party FFmpeg for probing, decoding, PCM/FLAC output and ALAC checks |
| Images | Linked Rust image bridge calling third-party ImageMagick for required conversions |
| MLP | Linked `dvda-mlp` Rust encoder |
| DVD authoring / menus | `dvda-author` Rust AOB/IFO and menu orchestration; Rust menu bridge with static third-party SPU/navigation vendor |
| ISO writing | `dvda-author` in-process Rust ISO9660/UDF 1.02 writer |
| Disc verification | Rust ISO reading, streaming AOB MLP/LPCM comparison and PCM checks |
| Configuration | versioned JSON profiles only |

The release ZIP is a native Windows x64 GUI package. Required third-party DLLs,
menu resources and fonts are embedded in the GUI; licenses and user documentation
are shipped beside it. See [the Rust author migration record](RUST-AUTHOR-MIGRATION.md)
for complete authoring and ISO acceptance.

## MLP identity rule

The MLP path receives the target PCM, format parameters and auxiliary metadata
before serialization. It never edits encoded bytes after serialization. A
reference file can therefore match byte-for-byte only when those inputs and the
encoder behavior match. Existing MLP files can be imported read-only; the import
path is intended for files produced by the original SurCode MLP encoder.

## Explicitly out of scope

The repository still contains build recipes and source provenance for the
third-party libraries used to produce the native DLLs. Python, MSYS2/MinGW-w64
GCC and other build tools are development prerequisites, not application
runtime dependencies. General DVD-Video features, subtitle authoring and unused
command-line utilities are not part of this workflow.

The old C# projects, Rust-to-C# bridge and `config.env` parser have been removed.
The application does not execute legacy `DVDA_MKISOFS`, `DVDA_METAFLAC`, eac3to
or media executable path settings.

## Acceptance

From the repository root:

```powershell
cargo fmt --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- --include-ignored
```

The menu fixture and the native ABI tests cover authoring, menu resources, ISO
writing, PCM comparison and failure handling. Physical-player playback remains
an operational check outside automated tests.
