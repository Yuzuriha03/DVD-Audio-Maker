# Development and debugging

The application is a Windows x64 Rust workspace. User packages contain only
the GUI; the developer CLI remains in source. Media, image, format, verifier
and MLP components are called through project-built native DLLs.

## Source entry points

From the repository root:

```bat
gui-debug.cmd
cli.cmd abi.version
cli.cmd prepare --profile "C:\work\settings.json"
cli.cmd build --dry-run --profile "C:\work\settings.json"
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline
```

`settings.json` is the only file-based configuration format. The Git-ignored
`settings.local.json` is suitable for local paths, and an isolated profile can
be selected with `--profile`. `config.env`, `DVDA_CONFIG` and `--config` are not
supported.

The GUI offers source checking, disc building and output verification. dry-run
is a developer CLI diagnostic and is not exposed in the GUI.

## Native components

Build native components using the [Windows build instructions](../tools/win-build/README.en.md). Source launchers use rust-dev-env.cmd to select the build directories; explicit environment overrides take precedence. The user EXE embeds its own runtime.
Development tests may explicitly select validated components with
`DVDA_MEDIA_NATIVE_DIR`, `DVDA_IMAGE_NATIVE_DIR`, `DVDA_FORMATS_NATIVE_DIR` and
`DVDA_ENCODER_LIBRARY`; missing components fail explicitly.

The MLP C17 core is under `native/mlp-encoder`, and the format C17 DLL sources
are under `tools/formats-native`. Use matching MinGW symbols and a native
debugger; do not change the production encoder's floating-point options for
debugging.

## VS Code

Use Rust Analyzer and CodeLLDB (or WinDbg) for `dvda-desktop` and `dvda-cli`.
`.vscode/tasks.json` contains workspace build, test, release and packaging
tasks; `.vscode/launch.json` contains x64 GUI and CLI debug entries.

## Release

```powershell
cargo build --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --release --workspace --offline
dvda-toolchain.exe package --repo . --output build/release --media-runtime build/media-native-shared --image-runtime build/image-native --image-author build/rust-author-current --prebuilt build/release-menu-final --version v1.0
```

`DVD-Audio-Maker-v1.0-win-x64.zip` excludes the .NET runtime, developer CLI,
PDBs, build-provenance JSON, `config.env` and personal profiles. Build outputs
remain under the Git-ignored `build` directory.

## Acceptance

```powershell
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File tools/win-build/test-rust-workflow.ps1
```

The menu fixture generates menus, index pages, stills, AUDIO_TS and an ISO with
the project-built author. MLP byte comparisons, PCM comparisons and C17 ABI
checks are recorded in the component documentation.


## SHA-256 backend regression and benchmark

Production SHA-256 is pinned to `sha2 = 0.11.0`. On x86/x64 it uses SHA-NI
when runtime `sha`, `sse2`, `ssse3` and `sse4.1` features are available, and
otherwise falls back to the portable implementation; the build does not force
`target-cpu=native`. No hand-written SHA-256 implementation is retained.
Regression tests cover standard vectors and compare the wrapper's one-shot,
streaming and reader paths against the upstream `sha2` API.

```powershell
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --release --offline -p dvda-core --lib hash:: -- --skip release_files_benchmark
```

The read-only Release file benchmark accepts a JSON array through
`DVDA_HASH_BENCH_INPUTS` and writes a new JSON report to
`DVDA_HASH_BENCH_REPORT`. It measures sha2 memory and warm-file hashing for real
DLLs, an MLP cache sample and the release ZIP using seven-round medians.
New reports contain sha2 timings only; existing before/after reports are historical.
Results cover the digest path only, not cold-disk or
end-to-end authoring time. Reports stay under the Git-ignored `build` folder.
`sha2` and its transitive dependencies retain their upstream MIT or
Apache-2.0 licensing.


Cache reuse still requires source/output fingerprints, encoder identity, target
parameters and a complete MLP CRC/parity scan. Unknown or missing parameter
cache evidence is probed again, never used to skip the scan. Automatic checking
uses `min(available cores, 16, track count)` workers; cancellation or errors do
not save this run's parameter cache.

```powershell
python tools\win-build\build-formats-runtime.py --msys-root C:\msys64 --output build\formats-native
python tools\win-build\test-formats-optimization.py --before C:\previous\dvda-formats.dll --after build\formats-native\dvda-formats.dll --samples C:\samples\mlp --report build\formats-report.json
$env:DVDA_FORMATS_NATIVE_DIR = (Resolve-Path build\formats-native).Path
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-native -- --include-ignored
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-core --lib mlp_workflow -- --include-ignored --skip real_cached --skip another_volume
```

The comparison script requires a previous DLL and actual MLP samples. It checks
full-scan ABI results, CRC boundaries and alignment output. The read-only
`real_cached_mlp_preflight_benchmark_is_read_only` test needs
`DVDA_MLP_PREFLIGHT_MANIFEST`, `ROOT`, `OUTPUT`, `MEDIA`, `ENCODER` and `REPORT`
(all six use the `DVDA_MLP_PREFLIGHT_` prefix). Media/encoder DLL identities
must match the cache. Any cache miss fails without encoding or overwriting
samples. Benchmark timings do not replace full disc and playback validation.
