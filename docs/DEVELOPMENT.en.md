# Development and debugging

The application is a Windows x64 Rust workspace. User packages contain only
the GUI; the developer CLI remains in source. Format processing and read-only
disc verification use Rust directly. Media, image and MLP encoding components
still use project-built native DLLs.

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
`DVDA_MEDIA_NATIVE_DIR`, `DVDA_IMAGE_NATIVE_DIR` and
`DVDA_ENCODER_LIBRARY`; missing components fail explicitly.

The MLP C17 core is under `native/mlp-encoder`; its production floating-point
options remain unchanged. Format parsing, MLP CRC/parity, PCM comparison and
AOB disc verification are implemented in Rust in `dvda-native`, without a
format or disc-verification DLL dependency.

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
