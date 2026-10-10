# Development and debugging

The application is a Windows x64 Rust workspace. User packages contain only
the GUI; the developer CLI remains in source. Format processing and read-only
disc verification use Rust directly. CLI and GUI default to `direct-bridges`
and `rust-mlp`: owned media, image, menu adapters and MLP encoding are linked
into the application. Third-party FFmpeg libraries remain native dependencies.
Production encoding now uses Rust MLP, but packaging and end-to-end product
acceptance are still being completed; a successful default build is not release
certification. Use `--no-default-features` for legacy CLI/GUI oracle regressions.
The linked encoder does not read `DVDA_ENCODER_LIBRARY`.

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

Build third-party libraries and the Rust author using the [Windows build instructions](../tools/win-build/README.en.md). Linked builds require `DVDA_FFMPEG_PREFIX`, `DVDA_MAGICK_WORK`, `DVDA_MSYS_ROOT` and `DVDA_MENU_NATIVE_DIR` pointing to the static menu vendor. FFmpeg imports are delay-loaded: packaged startup extracts the runtime and registers its DLL search directory before invoking media APIs. EXE-only startup with fresh `LOCALAPPDATA` and a system-only PATH creates the desktop window and extracts only nine third-party DLLs. Direct developer tests still need an explicit third-party DLL search path.

Only `--no-default-features` compatibility builds and frozen differential tests use
`DVDA_MEDIA_NATIVE_DIR`, `DVDA_IMAGE_NATIVE_DIR` and `DVDA_ENCODER_LIBRARY` to
select migrated backends. Default linked builds do not select them via those variables.

Production encoding uses the statically linked `dvda-mlp`; frozen legacy encoder
artifacts are differential oracles only. Format parsing, MLP CRC/parity, PCM comparison and
AOB disc verification are implemented in Rust in `dvda-native`, without a
format or disc-verification DLL dependency.

The complete author now lives in `dvda-author`: AOB, ATSI/SAMG/AMG/ASVS,
menu/still orchestration and ISO9660/UDF writing use Rust. GUI and CLI call
`dvda-core::author_runtime` in-process. The retired C author sources, build
entry and binaries have been removed; retained third-party image, video and menu libraries are
called through Rust bridges. See [the Rust author migration record](RUST-AUTHOR-MIGRATION.md)
for interfaces, build inputs and independent disc acceptance.

Owned menu sessions, private heap management, UTF-8 files/directories and resource
cleanup now use `dvda-menu`; vendor state reset is a generated `no_std` Rust object.
Production uses `build/menu-direct-vendor-rust-session`, and build/package gates
reject legacy C session/reset manifests. The small C jump boundary, third-party
code and build-only ABI probes are listed in the
[menu session migration record](RUST-MENU-SESSION-MIGRATION.md).

## VS Code

Default encoder tests/builds require no C DLL. The Rust developer ABI adapter is
generated in `build/mlp-encoder`; optional historical comparisons require an
explicit external oracle and the `external-oracle` test feature.

Use Rust Analyzer and CodeLLDB (or WinDbg) for `dvda-desktop` and `dvda-cli`.
`.vscode/tasks.json` contains workspace build, test, release and packaging
tasks; `.vscode/launch.json` contains x64 GUI and CLI debug entries.

## Release

```powershell
cargo build --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --release --workspace --offline
dvda-toolchain.exe package --repo . --output build/release --media-runtime build/media-native-shared --image-runtime build/rust-image-runtime --image-author build/rust-author-production --prebuilt build/release-menu-final --version v1.0
```

The default GUI links the Rust MLP encoder into the application and ships no
encoder DLL. An explicit `--encoder-runtime` additionally requires build-produced
`encoder-build.json` identifying Rust ABI v1; packaging verifies hashes, sizes,
x64 PE architecture and dependency closure.

The Rust author output contains `author-build.json` and `menu-build.json`.
Packaging authenticates sources, static SPU/navigation vendor archives, the
executable, third-party DLLs and resources. The standalone developer author is
validated but excluded from the GUI's embedded runtime. Missing, tampered or
mismatched inputs are rejected before release staging, preserving existing files.

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
Set `DVDA_TEST_MENU_DATA` to a directory containing the `menu` assets.
For direct builds, `DVDA_MENU_NATIVE_DIR` points to the static vendor archives.
Run the compiled test EXE with media/image directories and font configuration
pointing to the assembled production runtime. Menu DLL paths apply only to
dynamic compatibility tests. Assets need not have a sibling legacy `menu-bin`.


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
uses `min(2 × available logical processors, track count)` workers; cancellation or errors do
not save this run's parameter cache.

```powershell
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-native
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-core --lib mlp_workflow -- --include-ignored --skip real_cached --skip another_volume
```

Rust tests cover full scans, CRC boundaries, alignment output and PCM comparison.
Optional legacy-DLL differential tests are described in [format processing](C17-FORMATS.md).
The read-only
`real_cached_mlp_preflight_benchmark_is_read_only` test needs
`DVDA_MLP_PREFLIGHT_MANIFEST`, `ROOT`, `OUTPUT`, `MEDIA`, `ENCODER` and `REPORT`
(all six use the `DVDA_MLP_PREFLIGHT_` prefix). Media/encoder DLL identities
must match the cache. Any cache miss fails without encoding or overwriting
samples. Benchmark timings do not replace full disc and playback validation.

## PCM preparation and worker evaluation

All production worker pools use twice the detected logical processor count (saturating, minimum one), capped only by work items. Legacy job fields are ignored; CLI `--jobs` and evaluated `MlpJobs` are removed. Windows media codec contexts request the same automatic policy, subject to codec threading support: menu stills decode under that policy, but the one-shot PNG encode stays single-threaded because a frame-thread encoder only returns output after a flush and otherwise fails with EAGAIN. Build scripts share `worker_policy.py`; fixed build caps and the FFmpeg build `--jobs` selector are removed. GUI orchestration and the child-process tracing listener are single-task coordination, not parallel work pools.

Batch encoding reuses the decoded temporary WAV only when its rate, storage/valid bits, channel mask, RIFF structure, and PCM precision satisfy the target. Otherwise normalization remains in place. Reusing 20-bit PCM checks the low four bits; cancellation and temporary-file cleanup are preserved. Normalization bulk-copies 16-to-16 and 24-to-24 PCM, validates then copies 24-to-20 PCM, and uses a dedicated 32-bit-storage-to-24-bit packing path without relaxing precision checks.

The real cached preflight benchmark retains legacy 1/2/4/8/16 labels as compatibility probes: requested counts are ignored, and reports record both `requested_workers` and the actual automatic count. This measures cache checking, not full decoding/encoding throughput. Use real cached inputs and matching media/encoder DLLs, compare repeated-run medians, and monitor memory and CPU separately. Worker-policy unit tests alone do not justify production scheduling changes or performance claims.

For a bounded synthetic end-to-end benchmark, opt into the ignored `synthetic_batch_worker_benchmark` test in `encoder_native.rs`. Set `DVDA_BATCH_BENCH=1`, `DVDA_BATCH_BENCH_ROOT` to a **new short absolute directory** under ignored `build`, `DVDA_BATCH_BENCH_EXE` to an extracted shipped onefile EXE, and `DVDA_ENCODER_LIBRARY` to the pinned encoder DLL. Run `cargo test --manifest-path rust\Cargo.toml -p dvda-core --test encoder_native --release --target x86_64-pc-windows-gnu synthetic_batch_worker_benchmark -- --ignored --exact --nocapture`. The test extracts checksum-verified embedded media dependencies, generates 16 half-second stereo/6-channel tracks per 16/20/24-bit batch, measures the automatic policy over three repeats with alternating legacy requested-count labels (1/2/4/8/16), and verifies all 720 outputs by deterministic MLP digest and lossless decoded PCM (including zero final-AU padding). It deletes each verified run's outputs and writes `worker-benchmark.json` inside the owned root. Short roots avoid legacy Win32 path limits during runtime extraction; timings exclude sample generation and verification, and are not a before/after optimization comparison or representative long-album benchmark.
