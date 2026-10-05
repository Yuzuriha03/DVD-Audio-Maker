# GUI and native DLL boundary

The supported user entry point is a native Windows x64 Rust GUI. The GUI, CLI
and workflow libraries share one JSON profile reader. `config.env`, temporary
environment files and hidden legacy profile imports have been removed.

## Current structure

- `rust/crates/dvda-desktop` provides the Win32 GUI, JSON profile open/save,
  background tasks and user-facing logs.
- `rust/crates/dvda-cli` provides development diagnostics, sample inspection and
  automation. The GUI does not expose dry-run.
- `rust/crates/dvda-core` provides preparation, MLP/LPCM encoding dispatch,
  caching, menus, authoring and final verification.
- `rust/crates/dvda-native` loads and calls the validated media, image, format,
  verifier and MLP C ABIs.
- `native/mlp-encoder` contains the MLP C17 core and pinned x64 DLL. Encoded
  output is produced by the algorithm and is never patched afterward.

## Runtime boundary

The GUI does not start FFmpeg, FFprobe, ImageMagick, Metaflac, eac3to, original
SurCode or a separate MLP encoder. Required libraries are built from source and
shipped with the release package. Missing components fail explicitly instead of
falling back to PATH programs.

MLP encoding uses the streaming C ABI and preserves x87 PC53 arithmetic and the
floating-point control state around callbacks. The same PCM, parameters and
auxiliary metadata context must produce the same complete MLP. Verification is
read-only and never edits the file.

## JSON profiles

The default profile is `%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`. Developer
wrappers may discover the Git-ignored `settings.local.json` at the repository
root, or callers can pass `--profile PATH`. The example settings.example.json remains in source only. The release bundles no JSON profiles.

## Acceptance

```powershell
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File tools/win-build/test-rust-workflow.ps1
```

The release audit must find one GUI EXE with required DLLs, menu resources and fonts embedded, plus the three user README files, licenses and notices only. It must not find
.NET runtime files, the developer CLI, PDBs, build-provenance JSON or
`config.env`.
