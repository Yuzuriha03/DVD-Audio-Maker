# Rust format processing

Format processing is implemented directly in `dvda-native`. GUI, CLI and
verification paths share the same deterministic implementation:

- MLP access-unit inspection, CRC/parity checks and alignment;
- byte-for-byte PCM comparison with the documented zero-tail rule;
- PTS, sample-rate, peak-rate and MLP checksum parsing.

Format processing does not encode MLP. Encoding is implemented in
`rust/crates/dvda-mlp` and linked directly into the GUI and CLI.
No format DLL, external utility or fallback implementation is required.

## Rust migration regression checks

The formats and read-only MLP/LPCM disc verifiers require no C implementation or production DLL. The old C sources and frozen DLLs have been removed from the checkout. Historical format differential tests covered nine real MLP profiles (44.1–192 kHz, 16/20/24 bits, mono/stereo/six channels). The external comparison entry has now been removed; current regressions use the Rust implementation and retained golden data only.

Set `DVDA_TEST_AUTHOR` to the project-built author executable and run `cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --offline -p dvda-native --test authored_lpcm` for the full authored LPCM matrix; leave `DVDA_TEST_AUTHOR_QUICK` unset. This covers gapless/separate titles, short and odd-length PCM, corrupted audio/headers, and truncation rejection.

For application integration, `DVDA_TEST_MENU_DATA` points to the prepared directory containing `menu`, normally `build/rust-author-production/data`. Compilation links the in-process Rust menu crate; execution uses the assembled Rust runtime and its image/font configuration. See `tools/win-build/test-rust-workflow.ps1`. Historical application, conversion, encoder, Clippy, workspace and onefile checks passed; the EXE archive contains neither `dvda-formats.dll` nor `dvda-disc-verify.dll`. These checks do not certify hardware playback.

## Validation

The regular Rust tests run without native format libraries:

```powershell
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --offline -p dvda-native
```

For a directory containing MLP files, the development CLI can inspect and align
every file through the Rust implementation:

```powershell
cargo run --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --offline -p dvda-cli -- formats-sample C:\samples\mlp
```

The command reports file count, bytes, valid headers, alignment results and PTS
round trips. The former external format, encoder and disc-verifier oracle
entrypoints and their environment variables have been removed. They are not
available as production fallbacks or optional test modes.

MLP cache checks retain full-stream CRC/parity validation. CRC tables and frame
buffers are reused; the Rust coordinator reports per-track progress with bounded
parallelism. See [development regression checks](DEVELOPMENT.en.md)
for validation commands.
