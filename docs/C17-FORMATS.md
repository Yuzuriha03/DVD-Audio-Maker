# Rust format processing

Format processing is implemented directly in `dvda-native`. GUI, CLI and
verification paths share the same deterministic implementation:

- MLP access-unit inspection, CRC/parity checks and alignment;
- byte-for-byte PCM comparison with the documented zero-tail rule;
- PTS, sample-rate, peak-rate and MLP checksum parsing.

Format processing does not encode MLP. Encoding remains in
`native/mlp-encoder`, with its existing C17 core and fixed floating-point options.
No format DLL, external utility or fallback implementation is required.

## Rust migration regression checks

The formats and read-only MLP/LPCM disc verifiers no longer require a C implementation or production DLL. With `DVDA_FORMATS_ORACLE=1`, the frozen DLL differential tests also encode nine real MLP profiles (44.1–192 kHz, 16/20/24 bits, mono/stereo/six channels) using the pinned encoder, then compare inspection and alignment results. The oracle remains test-only.

Set `DVDA_TEST_AUTHOR` to the project-built author executable and run `cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-native --test authored_lpcm` for the full authored LPCM matrix; leave `DVDA_TEST_AUTHOR_QUICK` unset. This covers gapless/separate titles, short and odd-length PCM, corrupted audio/headers, and truncation rejection.

For native application integration, `DVDA_TEST_MENU_DATA` must point to the source directory containing `menu`, with `menu-bin` beside that source directory. Supplying only extracted `data` without its adjacent menu DLL directory is insufficient. Application (8), conversion (1), and encoder (5, excluding the opt-in benchmark) integration tests passed with the migrated Rust implementation. Strict all-target Clippy, workspace tests, and local onefile packaging passed; the actual EXE archive index contains neither `dvda-formats.dll` nor `dvda-disc-verify.dll`. These checks do not certify hardware playback.

## Validation

The regular Rust tests run without native format libraries:

```powershell
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-native
```

For a directory containing MLP files, the development CLI can inspect and align
every file through the Rust implementation:

```powershell
cargo run --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-cli -- formats-sample C:\samples\mlp
```

The command reports file count, bytes, valid headers, alignment results and PTS
round trips. Optional migration differential tests use a previously built C DLL
as a read-only oracle, never as a production fallback or release component.
Enable `DVDA_FORMATS_ORACLE=1` and put the old DLL at
`build\c-rust-migration-oracle\dvda-formats.dll` for the synthetic MLP/checksum
and PCM/PTS differential tests. Set `DVDA_FORMATS_REAL_MLP` to an existing MLP
file to also run `frozen_dll_real_mlp_file_differential`; without that variable,
the optional real-file check does not perform a comparison.

MLP cache checks retain full-stream CRC/parity validation. CRC tables and frame
buffers are reused; the Rust coordinator reports per-track progress with bounded
parallelism. See [development regression checks](DEVELOPMENT.en.md)
for validation commands.
