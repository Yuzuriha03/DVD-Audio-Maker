# Rust MLP encoder status

## Module layout

- `lib.rs` declares modules and re-exports the batch API.
- `config.rs` defines public configuration/errors and borrowed batch validation.
- `bitstream.rs` contains the internal bit writer and checksums.
- `encoder.rs` contains batch encoding and the fallback AU encoder.
- `direct.rs` adapts the safe `StreamIo` interface to the synchronous FFI host.
- `ffi.rs` and `ffi_layout.rs` implement the ABI and its format adapters.
- `streaming.rs` validates metadata packets and owns the fallback metadata cursor.
- `analysis/` groups decorrelation, matrix transforms, scaling, prediction,
 interval partitioning, search and arithmetic support.
- `pipeline/` owns streaming orchestration, per-channel encoding, matrix
 preparation, callback PCM input and metadata stamp progression. Input adapters
 and their tests are separate from matrix preparation.
- `syntax/` serializes residuals, restart headers, parameters, substreams and
 planned access units. Access-unit size checks share the same serializer.
- `output/` owns the delayed output queue and rate state; `timing.rs` provides
 shared timing calculations.
- `tests.rs` and module-local tests cover the internal and public contracts.

These internal module groups are x86-64-only, as before. Historical reference
function boundaries and call relationships are retained without `c_` module
names or a runtime C dependency.

Internal state uses named parameter blocks, owned output queues, borrowed PCM
and prediction plans, and typed outcomes/errors. Candidate order, strict-less
cost tie breaking, arithmetic order, serialization and failure commit points
remain part of the behavior contract. Refactoring must not merge corresponding
reference functions or reorganize their call relationships.

## Unsafe boundary

Production unsafe code is restricted by a crate-level lint to `direct` and
`ffi` (including its private adapters); the optional external-oracle test module
is separately exempted. Unsafe operations in unsafe functions must still use
explicit blocks. The encoding, analysis, serialization and output modules use
safe Rust.

`ffi_callbacks.rs` owns private, non-cloneable reader/writer capabilities with
unsafe constructors confined to the FFI module. Their lifetime contracts require
live synchronous contexts and sequential access when contexts are shared. Safe
methods accept slices, check frame capacity before invoking a reader, and reject
zero/overreported frame counts and callback failures. The host and grouped-input
logic use these methods, not raw function pointers. This cannot protect against
an external callback that violates its unsafe contract by writing out of bounds.

The remaining raw operations initialize ABI results, borrow validated foreign
config/metadata, invoke foreign callbacks, and bridge the safe direct API through
synchronous trampolines. Config/metadata must stay initialized and immutable;
results and callback buffers must obey the documented exclusive-access rules.
Direct callbacks borrow their shared context one at a time and retain no buffer
references. Callback panics are caught only with `panic = "unwind"`; release
`panic = "abort"` still terminates the process. Oracle DLL loading/function-pointer
conversion is test-only and is not a production dependency.

## Refactoring verification

The crate-wide idiomatic rewrite remains in progress. The phase reports record
what was actually changed and tested, including limits of the evidence:
[refactoring plan and results](../../../docs/MLP-RUST-REFACTOR-PHASE-1.md).
Historical performance/decoder measurements below are not fresh results for the
current revision and do not establish frozen C acceptance or release approval.

For a Windows Rust regression comparison, explicitly select an independently
built, pre-change DLL. The candidate is the Rust crate linked into the example,
not a second DLL argument:

```powershell
cargo run --manifest-path rust\Cargo.toml -p dvda-mlp --release --example differential -- 'C:\path\to\pre-refactor\dvda_mlp.dll' --rust-baseline
```

The non-stress suite compares 361 complete fixed-metadata streams using raw
bytes and SHA256 (133 standard/layout/depth and 228 grouped cases). With
`--stress`, it compares 370 streams (142 standard/layout/depth and 228 grouped
cases), including longer grouped inputs. A valid multi-record/partial-metadata
case additionally compares complete raw bytes and SHA256; it is not included in
those totals. Cancellation, reentrancy, admission and malformed-metadata checks
are additional contract checks, not additional complete-stream comparisons.
Without `--rust-baseline`, the harness requires the frozen C oracle SHA256 before
loading the supplied DLL. Rust baseline results must not be reported as
historical C acceptance.

Phase-six x86-64 Windows validation passed the 370-case stress comparison plus
the additional metadata comparison against the independently saved phase-two
pre-change Rust DLL, all 47 Release unit tests, both integration tests (including
the normally ignored FFmpeg matrix), and all-target Clippy. Other architectures
remain unverified with the installed toolchain. The batch baseline independence
limitation in the phase report remains open; no fresh frozen C acceptance or
release approval is implied.

## Streaming API safety

`direct::StreamIo` is the safe Rust streaming interface. Its borrowed context is
used synchronously; read, write and metadata refresh callbacks run sequentially.
The adapter limits unsafe blocks to pointer access and FFI calls, and uses the
shared diagnostic writer for callback failures. With `panic = "unwind"`, Rust
callback panics are caught and reported as status -5. The workspace release
profile uses `panic = "abort"`: panics terminate the process, so callers should
return errors instead. Foreign callbacks must never unwind across the C ABI.

The FFI entry-point documentation specifies pointer validity, exclusive result
storage and callback buffer lifetimes directly. The internal callback host is
unsafe to call because it relies on those contracts; raw-pointer validity is not
established by format validation. ABI v1 structures and entry-point admission
ordering are unchanged. Regression tests pin 64-bit field offsets, sizes and
alignment, null-result handling, initialized failure results, and diagnostic
truncation/clearing. The phase-four 361-case fixed-metadata differential against
the actual pre-change Rust DLL passed complete raw-byte and SHA256 comparisons.
This does not constitute frozen C acceptance or validation on other architectures.

## Historical real-audio serialization optimization

Bit insertion packs the pending bits and each input into a 64-bit value, then
emits at most two 16-bit words without a per-chunk loop. Capacity checks run the same validating serializer
in count-only mode, without materializing sample payloads or calculating their
checksum. Restart headers still materialize their bits for CRC validation.
No serialized-output caching or buffer reuse is introduced.

Five alternating-order runs of three 60-second, stereo 48 kHz/24-bit clips
used explicit metadata and restart interval 8. Encoding-only median times were:

| Clip | Before (ms) | After (ms) | Reduction |
| --- | ---: | ---: | ---: |
| Against the Tide | 1705.797 | 1480.899 | 13.18% |
| Beautiful Tomorrow | 1621.636 | 1212.522 | 25.23% |
| 沉沦幻海 | 1483.506 | 1247.001 | 15.94% |

These measurements combine the writer and capacity-check changes; they do not
attribute gains separately. All 15 paired measurements improved, but timing
variation was substantial. Final raw MLP SHA256 values matched the pre-change
Rust baseline on all three clips, with sizes 14,222,332, 12,976,944 and 13,004,530
bytes respectively. Independent FFmpeg decoding matched the prepared PCM.
The IIR loop experiment was reverted because its additional gain was unstable.

A subsequent packed-writer comparison against the 16-bit chunk-loop baseline used
24 local music clips (60 seconds each, stereo 48 kHz/24-bit), six alternating-order
rounds, and 288 timed encodes. The sum of per-clip median times fell from
33,535.951 ms to 32,246.812 ms (3.84%). Twenty-one clips improved by median;
119 of 144 paired measurements improved. All six round totals improved
(1.28% to 4.88%), but three clip medians regressed, so this is not a universal
speedup guarantee. Fixed-metadata raw MLP SHA256 matched the Rust baseline in
every timed encode; all 24 clips independently decoded to identical PCM.
These results cover one machine, one collection and one audio configuration.
Boundary and 10,000-insertion sequence tests compare the writer with a bitwise
reference; the 192-case FFmpeg matrix also passed with 12,801 input frames.

Validation: 22 release unit tests, 192 independent FFmpeg lossless cases at
12,801 frames each, and Clippy with the existing `chunks_exact_to_as_chunks`
exemption passed. Tests cover chunk boundaries against a per-bit reference and
counted versus materialized lengths/errors.

### Long-stream rate-limit diagnostics

The 64,001-frame, 88.2 kHz/20-bit/6-channel pattern-3 failure is a FIFO
rate-limit rejection, not a serializer failure. This pseudorandom input has a
raw PCM rate of 10.584 Mb/s; a failing 80-frame AU occupied 1,210 bytes
(about 10.67 Mb/s), above the configured roughly 9.6 Mb/s delivery rate.
Short inputs can fit within the initial FIFO timing margin; sufficiently long
incompressible streams exhaust it. No lossless encoder can guarantee that
arbitrary input fits below its information rate.

The encoder retains status `-5` for ABI compatibility, but now preserves the
specific diagnostic in `EncoderResult.error` through the public API and GUI
error path: `MLP FIFO rate limit exceeded; input cannot be encoded at the required delivery rate`.
All partial output must be discarded on failure. No rate checks, losslessness,
or format constraints are relaxed. A non-ignored regression checks this exact
long input and successful silence at the same format and length. The optional
lossless matrix still expects success and is not an arbitrary-length acceptance
test for incompressible inputs above the delivery limit.


## Native floating-point mode

The matrix analysis and half-rate interpolation arithmetic now use Rust `f64`
(including `f64::ln`), not x87 inline assembly or software x87 emulation.
Function boundaries, operation order and the integer lossless coding pipeline
are retained. Decoded PCM, format compatibility and output-size checks are useful
regressions. By user authorization, complete historical C byte/coded-stream
comparison is no longer a migration or release gate, now or for future releases.
Historical explicit-metadata comparisons still report byte-for-byte and raw
SHA256 equality; default-metadata comparisons may use the documented
`mlp-coded-v1-no-auxiliary-stamp` scheme after validating original header parity.
This is coded-stream equality, not raw-file equality. See
[MLP parity documentation](../../../docs/MLP-PARITY.md). The native-float results
below do not establish historical C equality.

The encoder no longer saves, changes or restores x87/MXCSR control state. Callers
and callbacks must use the standard Rust floating-point environment (round to
nearest, no flush-to-zero or enabled floating-point traps) and must not change it
during encoding. Cross-platform bit-identical transcendental results are not
promised. Existing platform gates remain; this change alone is not a portability
certification. Half-rate interpolation synthesizes samples and may round
differently from the old implementation.

On the initial Windows x86-64 validation, all 192 cases decoded exactly; both
x87 and native-float outputs totaled 596,226 bytes, with no per-case size changes.
These are short synthetic inputs (1,281 frames each), not a real-album compression
benchmark or proof of byte equality.

An opt-in independent FFmpeg decoder test covers 192 deterministic combinations
of all supported sample rates and depths, 1/2/6 channels where supported, silence,
extreme values, correlated ramps and pseudorandom input. It checks exact PCM and
zero padding through the production direct-streaming API:

```powershell
$env:DVDA_FLOAT_REPORT = 'C:\temp\native-float.csv' # optional existing parent directory
$env:DVDA_FLOAT_FRAMES = '12801' # optional; default 1281 frames per case
cargo test --manifest-path rust\Cargo.toml -p dvda-mlp --release --test native_float -- --ignored --nocapture
```

### Prediction-search budget experiment

The primary joint predictor search again attempts 16 coefficient perturbations,
matching the historical C implementation. The earlier reduction to 4 changed
candidate choices and RNG progression, causing frozen-vector output differences.
Restoring 16 passes all 116 explicit-metadata frozen vectors, including their
output lengths and raw SHA256 hashes. Historical C comparison remains available
for diagnostics, but is no longer a release gate by user authorization.
Serialization caching and output-buffer
reuse experiments were reverted at user request.

The following measurements describe the historical four-perturbation experiment,
not the current production search budget.

On Windows x64, three alternating baseline/current runs of the 192-case matrix
(12,801 frames each) measured summed encoder times of 1.023/0.970,
1.024/0.971 and 1.008/1.012 seconds. Median totals improved about 5.0%; one pair
was slightly slower, so this is not a general throughput guarantee. FFmpeg
reconstructed exact PCM and padding for all cases. Total output changed from
5,789,668 to 5,787,766 bytes (-0.033%); 15 cases changed size, with the largest
increase 584 bytes (+1.43%, 48 kHz/16-bit/6-channel correlated ramp).
Compression acceptance remains subject to user review and real-audio testing.

A subsequent arithmetic-loop optimization expresses the autocorrelation inner
product as paired bounded slices instead of repeated indexed accesses. It keeps
scalar accumulation order and the search budget unchanged, without caching or
buffer reuse. Against the four-perturbation baseline, three alternating runs of
the same 12,801-frame matrix measured encoder totals of 1.372/1.341,
1.340/1.277 and 1.374/1.379 seconds (baseline/current). Median totals decreased
about 2.3%, with one slightly slower pair. All 192 output sizes were unchanged
in every pair, and every decode matched PCM and padding exactly. Equal sizes
alone do not establish byte-identical bitstreams. These timings are from a
separate run and should not be compared directly with the earlier experiment.

A zero-order IIR fast-path experiment was rejected: skipping unit-impulse
calculations alone gave no consistent benefit; additionally bypassing zero
products gave baseline/current encoder totals of 0.977/1.040, 1.036/1.024 and
1.003/0.984 seconds in three alternating 192-case, 12,801-frame runs. The median
increased about 2.2%. All sizes and decoded PCM/padding matched, but the
production fast path was reverted. A unit-impulse equivalence regression test
was retained, including extreme finite correlations and invalid lag/order pairs.

A longer 64,001-frame matrix exposed a FIFO rate-limit rejection (-5) at
88.2 kHz/20-bit/6-channel pseudorandom input in both earlier pipeline versions.
See the rate-limit diagnostics above: arbitrary incompressible input cannot be
guaranteed to fit the delivery rate. Short-matrix success does not certify long
inputs, memory savings or physical-player compatibility.

FFmpeg must be on PATH. The test does not certify physical DVD players or all
mixed-rate layouts. Historical oracle tests are optional for normal Rust builds,
but complete frozen C comparison is no longer a migration or release gate;
see [MLP-PARITY](../../../docs/MLP-PARITY.md).

## Building and using the encoder

`dvda-mlp` is an independent Rust MLP bitstream encoder, not a C wrapper. It
implements lossless entropy modes 0-3, adaptive FIR and stable IIR prediction,
reversible multichannel matrix decorrelation, and common-bit scaling for
DVD-Audio layouts and native-rate groups. Translated C sources, their build
recipes and frozen C DLLs have been removed from the checkout.

Build/test from the repository root:

```powershell
cargo test --manifest-path rust\Cargo.toml -p dvda-mlp
cargo build --manifest-path rust\Cargo.toml -p dvda-mlp --release
```

Production uses this crate's `rlib`, linked directly into the GUI and CLI with
`rust-mlp` enabled by default in core and both applications. The normal encoder
entry point skips encoder DLL loading/hashing and retains shared WAV conversion,
metadata loading, cancellation/timeout, staged output and final rename behavior.
GUI-ONLY releases ship neither a CLI nor an encoder DLL. The public
`direct::encode_stream` API accepts `Config`, `Profile`, primary frame count,
optional secondary frame count, a mandatory metadata timeline, and `StreamIo`.
`Config.metadata` is not used by this streaming API; pass records explicitly.
`StreamIo` receives bounded PCM/output slices, returns frame counts (not sample
counts), and can cancel before reads/writes. Uniform input is canonical speaker
order; independent groups use assignment order. Callback panics return `-5`
only in unwind builds; release builds abort (see Streaming API safety above).
All output must be discarded on failure. Calls are synchronous, independent,
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

`build-runtime.ps1` runs the default Rust tests, strict Clippy and a release build,
resolves the DLL from Cargo's JSON artifact output (including configured targets),
then emits
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
  depth, rate, assignment and AU parity, then compares decoded PCM from the frozen and Rust DLLs. Diagnostic header normalization is never
  counted as native-player certification.

Rust build command:

```powershell
powershell -ExecutionPolicy Bypass -File rust\crates\dvda-mlp\build-runtime.ps1
```

An optional `-FrozenEncoder <external-DLL>` enables historical comparisons with
the `external-oracle` feature, validates the original hash before and after, and
runs standard/grouped differential stress suites. Those four tests require an
explicit comparison runtime; default builds and tests require only Rust.

### Historical acceptance records

These records describe earlier revisions, not a rerun or acceptance of the
current native-float implementation. Decoder/PCM-only checks are not raw-byte
parity checks, including the additional standard stress cases.

The historical recipe with an external C oracle was executed successfully with output directory
`build\mlp-encoder-validation`. Its actual release DLL passes 142 standard,
layout, depth and long-stream PCM cases and 228 grouped profiles against the
frozen C DLL. Grouped cases additionally
check primary input, secondary input and output cancellation. Standard tests
cover immediate/late errors, first-output pacing, counters, nested reentrancy,
eight simultaneous calls, 13 malformed ABI cases and metadata bit timelines.
The direct API tests cover grouped equal/half-rate reads, bounded buffers,
concurrency, invalid duration, panic containment and cancellation. That external-oracle run passed 21 tests, including 27,360 grouped raw-byte/SHA256 cases and independent quantized FIR/IIR state reconstruction. Four historical differential tests require the explicit external oracle feature. Source-only C state/interpolation tests were removed. Strict Clippy is required.
The stress streams contain 70,001 standard frames and 9,602 grouped primary
frames. These finite tests are not exhaustive FIFO/compression certification.

The existing `tools\win-build\test-rust-decoder.ps1` was also exercised with
both probes' media-runtime DLLs on PATH. All nine available fixtures passed
PCM/metadata comparison, and malformed-input/output-error checks passed. The
recipe nevertheless failed its final capability comparison: the Rust probe
returns usage/status 2 for `--capabilities`, while the legacy probe returns DLL
capabilities/status 11. This is not a passed decoder acceptance run and does
not certify native mixed-rate playback.

No production fallback to C is implemented. By user authorization, complete
historical C comparison is no longer required for this or future releases.
Lossless/format regression checks and honest reporting of observed differences
remain applicable. Publication still requires user authorization; this does not
restore a default C dependency.
