# Rust MLP encoder status

`dvda-mlp` is an independent Rust MLP bitstream encoder, not a C wrapper. It
implements lossless entropy modes 0-3, adaptive FIR and stable IIR prediction,
reversible multichannel matrix decorrelation, and common-bit scaling for
DVD-Audio layouts and native-rate groups. Original C sources and the frozen C
DLL remain required as oracles.

Build/test from the repository root:

```powershell
cargo test --manifest-path rust\Cargo.toml -p dvda-mlp
cargo build --manifest-path rust\Cargo.toml -p dvda-mlp --release
```

Production uses this crate's `rlib`, linked directly into the GUI and CLI with
`rust-mlp` enabled by default in core and both applications. The normal encoder
entry point skips encoder DLL loading/hashing and retains shared WAV conversion,
metadata loading, cancellation/timeout, staged output and final rename behavior.
The frozen C DLL is a test oracle only; GUI-ONLY releases ship neither a CLI nor
an encoder DLL. The public
`direct::encode_stream` API accepts `Config`, `Profile`, primary frame count,
optional secondary frame count, a mandatory metadata timeline, and `StreamIo`.
`Config.metadata` is not used by this streaming API; pass records explicitly.
`StreamIo` receives bounded PCM/output slices, returns frame counts (not sample
counts), and can cancel before reads/writes. Uniform input is canonical speaker
order; independent groups use assignment order. Callback panics return `-5`;
all output must be discarded on failure. Calls are synchronous, independent,
and support concurrency/reentrancy. Internal allocation exhaustion can still
abort. The `cdylib` is retained only as a differential acceptance adapter.

The differential DLL exports `mlp_encoder_abi_version`, `mlp_encode_stream`, the explicit
layout/depth entrypoints, and `mlp_encode_stream_groups` with the original
configuration/result layout and callback calling convention. It validates
multiple metadata records (including a partial final record), uses DVD
assignment channel order, supports distinct group precision and deterministic
half-rate interpolation, applies a port of the native 1200-AU timing/FIFO
lookahead, and buffers at most one restart interval per input group. Output
callbacks receive individual AUs. Nonzero read/write callbacks return -3/-4
respectively. Callers must discard output on any nonzero final result.

`build-runtime.ps1` is the owned fail-closed acceptance recipe. It checks the
frozen oracle hash, builds independent unchanged-C timing and group references,
runs unit tests, strict Clippy, release build, separate standard and grouped
stress differential suites, rechecks the frozen hash, and emits
`mlp_encoder.dll` plus `encoder-build.json`. The manifest is ABI version 1,
identifies the Rust implementation and DLL SHA-256/size, but deliberately sets
`production_ready` to `false`.

**This DLL is not yet a production replacement. Do not rename/package it as
encoder.dll.** Remaining acceptance blockers are concrete:

- Infallible Rust allocations can abort rather than reliably map exhaustion to
  ABI error `-2`.
- Exhaustive arbitrary-input FIFO/capability comparison is not yet certified.
- Native mixed-rate declarations are rejected by available FFmpeg and bridge
  probes. Grouped acceptance validates the unmodified native headers, CRC,
  depth, rate, assignment and AU parity, then uses an unchanged C interpolation
  oracle for exact reconstructed PCM. Diagnostic header normalization is never
  counted as native-player certification.

Frozen-C differential command:

```powershell
powershell -ExecutionPolicy Bypass -File rust\crates\dvda-mlp\build-runtime.ps1
```

The complete recipe has been executed successfully with output directory
`build\mlp-encoder-validation`. Its actual release DLL passes 142 standard,
layout, depth and long-stream PCM cases and 228 grouped profiles against the
frozen C DLL and independent interpolation oracle. Grouped cases additionally
check primary input, secondary input and output cancellation. Standard tests
cover immediate/late errors, first-output pacing, counters, nested reentrancy,
eight simultaneous calls, 13 malformed ABI cases and metadata bit timelines.
The direct API tests cover grouped equal/half-rate reads, bounded buffers,
concurrency, invalid duration, panic containment and cancellation. Thirteen
unit tests pass, including 72,000 direct unchanged-C timing transitions and
independent quantized FIR/IIR state reconstruction. Strict Clippy passes.
The stress streams contain 70,001 standard frames and 9,602 grouped primary
frames. These finite tests are not exhaustive FIFO/compression certification.

The existing `tools\win-build\test-rust-decoder.ps1` was also exercised with
both probes' media-runtime DLLs on PATH. All nine available fixtures passed
PCM/metadata comparison, and malformed-input/output-error checks passed. The
recipe nevertheless failed its final capability comparison: the Rust probe
returns usage/status 2 for `--capabilities`, while the legacy probe returns DLL
capabilities/status 11. This is not a passed decoder acceptance run and does
not certify native mixed-rate playback.

No production fallback to C is implemented. Packaging and deletion remain
blocked until the outstanding acceptance requirements above are resolved.
