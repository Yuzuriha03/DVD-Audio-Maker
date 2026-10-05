# C17 format runtime

The format boundary is implemented in C17 and exposed through the bundled
`dvda-formats.dll`. The Rust application uses it for the narrow operations that
must remain deterministic across the GUI, CLI and verification paths:

- MLP access-unit inspection, CRC/parity checks and alignment;
- byte-for-byte PCM comparison with the documented zero-tail rule;
- PTS, sample-rate, peak-rate and MLP checksum parsing.

The DLL does not encode MLP. Encoding is provided by `native/mlp-encoder`.
There is no C# fallback and there is no external format utility fallback.

## Build

```powershell
python tools/win-build/build-formats-runtime.py --msys-root C:/msys64 --output build/formats-native
```

The command uses the repository's x64 MSYS2 MinGW-w64 GCC toolchain. The
resulting DLL imports only Windows system libraries and the system C runtime.
`dvda-toolchain package` copies the validated DLL into the release menu runtime.

## Validation

The Rust workspace tests exercise the native ABI and the Rust callers:

```powershell
cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- --include-ignored
```

For a sample directory containing MLP files, the development CLI can inspect
and align every file through the native runtime:

```powershell
cargo run --manifest-path rust/Cargo.toml --offline -p dvda-cli -- formats-sample C:/samples/mlp
```

The command reports file count, bytes, valid headers, alignment results and PTS
round trips. The normal release package contains the DLL beside the menu
authoring components; no C# project or .NET runtime is required.
