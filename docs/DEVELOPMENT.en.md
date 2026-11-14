# Development and debugging

The application is a Windows x64/ARM64 Rust workspace. User packages contain
only the GUI; the developer CLI remains in source. CLI and GUI enable `rust-mlp`;
owned media, image, menu adapters and MLP encoding are linked into the application,
while third-party FFmpeg libraries remain native dependencies. The old
`direct-bridges` switch and dynamic-loading fallback have been removed. Bounded
std thread pools provide parallelism; rayon is not used.

## Source entry points

From the repository root:

```bat
gui-debug.cmd
cli.cmd abi.version
cli.cmd prepare --profile "C:\work\settings.json"
cli.cmd build --dry-run --profile "C:\work\settings.json"
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --workspace --offline
```

`settings.json` is the only file-based configuration format. The Git-ignored
`settings.local.json` is suitable for local paths, and an isolated profile can
be selected with `--profile`. `config.env`, `DVDA_CONFIG` and `--config` are not
supported.

The GUI offers source checking, disc building and output verification. dry-run
is a developer CLI diagnostic and is not exposed in the GUI.

## Native components

Build third-party libraries and runtime assets using the [Windows build instructions](../tools/win-build/README.en.md). Linked builds use `DVDA_FFMPEG_PREFIX`; `cargo-msvc.ps1` supplies defaults. `DVDA_MAGICK_WORK` and `DVDA_IMAGE_DEPS_DIR` are retired; neither ImageMagick nor a menu vendor is needed. The production FFmpeg static profile links all five libraries and soxr into the executable, without native zlib; only fonts, XML configuration and menu assets are extracted. The shared profile remains available for development testing without external oracles.

External native/oracle differential entrypoints and their library/image environment
variables have been removed. Builds and tests use the current linked backends.

Production encoding uses the statically linked `dvda-mlp`; historical C oracle
acceptance has been removed. Format parsing, MLP CRC/parity, PCM comparison and
AOB disc verification are implemented in Rust in `dvda-native`, without a
format or disc-verification DLL dependency.

The complete author now lives in `dvda-author`: AOB, ATSI/SAMG/AMG/ASVS,
menu/still orchestration and ISO9660/UDF writing use Rust. GUI and CLI call
`dvda-core::author_runtime` in-process. The retired C author sources, build
entry and binaries have been removed; retained third-party image, video and menu libraries are
called through Rust bridges. See [the Rust author migration record](RUST-AUTHOR-MIGRATION.md)
for interfaces, build inputs and independent disc acceptance.

Menu sessions, SPU, navigation and resource management now run in-process in
Rust `dvda-menu`; no menu vendor or C jump bridge is built. Third-party image/video
libraries and build-only ABI probes remain. Historical stages are described in the
[menu session migration record](RUST-MENU-SESSION-MIGRATION.md).

## VS Code

Default encoder tests/builds require no C DLL. The Rust developer ABI adapter is
generated in `build/mlp-encoder`; the historical `external-oracle` feature and
comparison harness have been removed.

Use Rust Analyzer and CodeLLDB (or WinDbg) for `dvda-desktop` and `dvda-cli`.
`.vscode/tasks.json` contains workspace build, test, release and packaging
tasks; `.vscode/launch.json` contains x64 GUI and CLI debug entries.

## Release

```powershell
cargo build --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --release --workspace --offline
dvda-toolchain.exe package --repo . --output build/release --media-runtime build/msvc-native/ffmpeg-static/install --image-author build/rust-author-production --prebuilt build/release-menu-final --version v1.0
```

The default GUI links the Rust MLP encoder into the application and ships no
encoder DLL. The retired `--encoder-runtime` external comparison validation option has been
removed; packaging accepts only current release inputs.

The Rust author output contains `author-build.json`, authenticating source and
runtime resources; no `menu-build.json` or static menu vendor is required.
The standalone developer author is excluded from the GUI's embedded runtime
and built separately by the workflow tests. Missing, tampered or mismatched
inputs are rejected before release staging, preserving existing files.

`DVD-Audio-Maker-v1.0-win-x64.zip` excludes the .NET runtime, developer CLI,
PDBs, build-provenance JSON, `config.env` and personal profiles. Build outputs
remain under the Git-ignored `build` directory.

Native manifests under `build/msvc-native` describe the third-party libraries
and their hashes. Keep them with those libraries. After deleting them, rebuild
FFmpeg for the current static Rust author; ImageMagick and the menu vendor are
no longer required. `build-rust-author.py` takes `--ffmpeg-runtime`, `--menu-assets`
and `--font`; retired `--magick-work` / `--image-dependencies` explicitly fail.
Its asset inventory contains only copied fonts and menu assets, not ImageMagick
XML configuration. Static package validation no longer requires the old image
directory. Full release and visual differential acceptance remain in progress.

## Acceptance

```powershell
cargo  fmt --manifest-path rust/Cargo.toml --all -- --check
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo  clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --locked --offline --workspace --all-targets --all-features -- -D warnings"
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo  clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --locked --offline --workspace --all-targets -- -D warnings"
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo  clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --locked --offline --workspace --all-targets --no-default-features -- -D warnings"
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo  clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --locked --offline -p dvda-core --all-targets --no-default-features -- -D warnings"
powershell -NoProfile -ExecutionPolicy Bypass -File tools/win-build/test-rust-workflow.ps1
```

The workspace keeps `clippy::pedantic` at `warn`; strict acceptance promotes it to an error with `-D warnings`. Protocol conversions and C-mirrored implementations use scoped, justified exemptions only. Fix generated FFmpeg bindings in `build.rs`, never directly under `target`.

To audit existing exemptions, also run `cargo  clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --workspace --all-targets --all-features --locked --offline --message-format=json -- --force-warn clippy::pedantic`, and collect separate default / no-default-features results. This exposes suppressed diagnostics; a successful exit does not mean zero warnings. For comparisons, take only the first primary span per diagnostic and deduplicate by rule, file, and line using the same toolchain and features, while also checking `unfulfilled_lint_expectations`. Workspace no-default-features does not disable Core defaults re-enabled by dependents, so the command above separately checks Core's fallback branches. Remove documentation exemptions after documenting concrete Errors/Panics contracts; replacing `allow` with `expect` is not a fix. Split long functions at independent responsibility boundaries. GUI assembly sharing control registries, flat profile mappings, GDI resource lifetimes, and sequential WAV parsing may retain justified function-level exemptions rather than introduce parallel state structures merely to reduce line counts. C-mirrored MLP function boundaries, arithmetic, and call relationships are outside lint refactoring scope.

The menu fixture generates menus, index pages, stills, AUDIO_TS and an ISO with
the project-built author. MLP byte comparisons, PCM comparisons and C17 ABI
checks are recorded in the component documentation.
The workflow defaults to `-Mode Functional`: it builds `dvda-author-dev` through
`cargo-msvc.ps1`, reads the Cargo artifact path, stages it with runtime DLLs in a
unique `build/rust-workflow/author-*` directory and removes that directory afterward.
It never uses a leftover production author EXE. Media/image runtime directories and
menu assets must already exist (default: `build/rust-author-production` and its `data`).
`DVDA_MENU_NATIVE_DIR` selects runtime resources, not build-time vendor archives.

Use `-Mode Benchmarks` for the seven manual benchmarks, built in Release and selected
by exact test names; functional tests are not run in that mode. Each missing optional
prerequisite is printed as `NOT RUN`, never counted as a pass:

- Cross-volume: `-OtherVolume` (or `DVDA_TEST_OTHER_VOLUME`), writable and on a different
  volume from the temporary directory; an invalid supplied directory is an error.
- Lossless float matrix: `ffmpeg` on PATH; this is a functional test, not a benchmark.
- Hash benchmark: `DVDA_HASH_BENCH_INPUTS` JSON file-path array and
  `DVDA_HASH_BENCH_REPORT` with an existing parent directory.
- Real cached MLP: all six `DVDA_MLP_PREFLIGHT_{MANIFEST,ROOT,OUTPUT,MEDIA,ENCODER,REPORT}`
  values. Paths must exist (report parent must exist); the test validates cache identity.
- Batch benchmark: `DVDA_BATCH_BENCH=1`, a new `DVDA_BATCH_BENCH_ROOT`,
  and `DVDA_BATCH_BENCH_EXE` pointing to a packaged executable.

Test failures produce a nonzero exit code; functional execution continues through
all test executables to expose independent failures. Runtime assets remain external
inputs; this command does not rebuild third-party dependencies or certify playback.
Do not set `DVDA_TEST_AUTHOR_QUICK` when checking the full LPCM matrix.


## SHA-256 backend regression and benchmark

Production SHA-256 is pinned to `sha2 = 0.11.0`. On x86/x64 it uses SHA-NI
when runtime `sha`, `sse2`, `ssse3` and `sse4.1` features are available, and
otherwise falls back to the portable implementation; the build does not force
`target-cpu=native`. No hand-written SHA-256 implementation is retained.
Regression tests cover standard vectors and compare the wrapper's one-shot,
streaming and reader paths against the upstream `sha2` API.

```powershell
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --release --offline -p dvda-core --lib hash:: -- --skip release_files_benchmark
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
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --offline -p dvda-native
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --offline -p dvda-core --lib mlp_workflow -- --include-ignored --skip real_cached --skip another_volume
```

Rust tests cover full scans, CRC boundaries, alignment output and PCM comparison.
External legacy-DLL differential entrypoints have been removed; current format regressions use Rust and retained golden data. See [format processing](C17-FORMATS.md).
The read-only
`real_cached_mlp_preflight_benchmark_is_read_only` test needs
`DVDA_MLP_PREFLIGHT_MANIFEST`, `ROOT`, `OUTPUT`, `MEDIA`, `ENCODER` and `REPORT`
(all six use the `DVDA_MLP_PREFLIGHT_` prefix). Media/encoder component identities
must match the cache. Any cache miss fails without encoding or overwriting
samples. Benchmark timings do not replace full disc and playback validation.

## PCM preparation and worker evaluation

All production worker pools use the detected logical processor count (minimum one), capped by work items, without a multiplier or a fixed 16-worker limit. Deprecated job `jobs` fields, CLI `--jobs`, and evaluated `MlpJobs` are removed. Windows media codec contexts divide logical processors by active media jobs (rounded up), preventing nested oversubscription while a single job can still use every core; actual threading remains subject to codec support: menu stills decode under that policy, but the one-shot PNG encode stays single-threaded because a frame-thread encoder only returns output after a flush and otherwise fails with EAGAIN. Build scripts share `worker_policy.py`; fixed build caps and the FFmpeg build `--jobs` selector are removed. GUI orchestration and the child-process tracing listener are single-task coordination, not parallel work pools.

Batch MLP encoding and LPCM preparation reuse the decoded temporary WAV only when its rate, storage/valid bits, channel mask, RIFF structure, and PCM precision satisfy the target. Otherwise normalization remains in place. Reusing 20-bit PCM checks the low four bits; cancellation and temporary-file cleanup are preserved. Normalization bulk-copies 16-to-16 and 24-to-24 PCM, validates then copies 24-to-20 PCM, and uses a dedicated 32-bit-storage-to-24-bit packing path without relaxing precision checks.

When a production build needs a fresh full decode, it can write and normalize production PCM in the same pass. The default Rust MLP path instead uses that decode to feed a bounded in-memory queue and directly writes a temporary MLP, with no production WAV. The manifest carries source/intermediate identities and a signature covering target parameters, dither, and media components. MLP/LPCM reuse requires every item to match and otherwise falls back to the original conversion. Explicit MLP metadata and builds with `rust-mlp` disabled retain the WAV path. Standalone inspection writes no PCM. Direct-build verification decodes source and output concurrently, compares samples through a bounded queue without temporary WAV/raw files, and preserves first-mismatch offsets and zero-padding checks. ALAC cookie scanning, packet-bit inspection and repair use bounded file I/O, with repaired output still published atomically through a temporary file. Full verification runs menu-structure and lossless-content checks concurrently, then merges diagnostics in stable stage order.

WAV container parsing and writing are shared through `dvda-audio`; `dvda-core` retains DVD-Audio parameter restrictions and PCM conversion only. Authoring uses the integer RIFF reader policy (reject duplicate fmt/data, skip ancillary metadata, tolerate only a missing terminal data padding byte). Normalization uses the shared Writer's compact RIFF mode, preserving its 68-byte header and 32-bit size limit. General media input retains its existing RIFF/RF64 policy, and general output retains the 104-byte header with RF64 reservation.

The real cached preflight benchmark compares the initial automatic-policy check with parameter-cache reuse and reports only the actual worker count. This measures cache checking, not full decoding/encoding throughput. Use real cached inputs and matching media/encoder components, compare repeated-run medians, and monitor memory and CPU separately. Worker-policy unit tests alone do not justify production scheduling changes or performance claims.

For a bounded synthetic end-to-end benchmark, opt into the ignored `synthetic_batch_worker_benchmark` test in `encoder_native.rs`. Set `DVDA_BATCH_BENCH=1`, `DVDA_BATCH_BENCH_ROOT` to a **new short absolute directory** under ignored `build`, `DVDA_BATCH_BENCH_EXE` to an extracted shipped onefile EXE,; encoding uses the current Rust test executable without an external encoder DLL. Run `cargo test --manifest-path rust\Cargo.toml -p dvda-core --test encoder_native --release --target x86_64-pc-windows-msvc synthetic_batch_worker_benchmark -- --ignored --exact --nocapture`. The test extracts checksum-verified embedded media dependencies, generates 16 half-second stereo/6-channel tracks per 16/20/24-bit batch, measures the automatic policy over three repeats of five automatic-policy runs, and verifies all 720 outputs by deterministic MLP digest and lossless decoded PCM (including zero final-AU padding). It deletes each verified run's outputs and writes `worker-benchmark.json` inside the owned root. Short roots avoid legacy Win32 path limits during runtime extraction; timings exclude sample generation and verification, and are not a before/after optimization comparison or representative long-album benchmark.

Profiles use canonical `DVDA_*` keys. Legacy expansion of SourceDirectory, FinalDirectory, BuildDirectory, Title, IsoPrefix, MlpSource, and MlpExternalDirectory into profile keys has been removed. Evaluated fields with these names are unchanged.
