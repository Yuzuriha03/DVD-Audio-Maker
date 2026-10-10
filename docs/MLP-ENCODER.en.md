# MLP encoding

Cleanup on 2026-10-10 removed the C source and frozen DLLs from this checkout.
The default `build-runtime.ps1` runs 17 Rust tests, Clippy and a release build.
Historical DLL comparisons require an explicit external `-FrozenEncoder` and
the `external-oracle` feature. C parity matrices below are historical evidence.

The normal `surcode-batch` workflow calls the Rust MLP encoder linked directly
into the main executable. It does not load an encoder DLL or start an encoder
subprocess or external FFmpeg executable.

```text
source audio -> built-in media bridge -> integer PCM WAVE -> built-in Rust MLP
             -> read-only verification -> MLP cache -> DVD authoring
```

Rust sources are under `rust/crates/dvda-mlp`. Only the immutable Windows x64
C DLL remains under `native/mlp-encoder` as a test oracle; the translated C
sources, headers, source-level test adapters and C build recipe were removed.
GUI-ONLY releases ship neither an encoder DLL nor a CLI.

## Settings

Application settings are versioned JSON profiles. The default profile is
`%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`; a developer can pass another
path with `--profile`. The old `config.env` format and its parser have been
removed.

Relevant values are:

```json
{
  "DVDA_MLP_SOURCE": "surcode-batch",
  "DVDA_MLP_SURCODE_SAMPLE_RATE": "48000",
  "DVDA_MLP_SURCODE_BITS": "24",
  "DVDA_MLP_METADATA_CONTEXT": "",
  "DVDA_MLP_EXTERNAL_DIR": ""
}
```

`DVDA_MLP_JOBS` is ignored for compatibility. Production encoding uses `min(2 × logical processors, work items)` automatically; parallelism is between tracks, so the encoding order and bytes of each individual track remain unchanged.

DVD-Audio sample rates are 44.1, 48, 88.2, 96, 176.4 and 192 kHz. The
supported integer depths are 16, 20 and 24 bits. The channel limit follows the
DVD-Audio layout rules: up to six channels at the lower rates and up to two at
176.4/192 kHz. The LPCM mode is selected with `DVDA_MLP_SOURCE=lpcm`.

The encoder receives normalized PCM and metadata before serialization. It never
patches encoded bytes after the fact. Exact file identity therefore requires the
same PCM, format parameters and auxiliary metadata context as the reference
file. An explicit `MSCTX001` context can be supplied through
`DVDA_MLP_METADATA_CONTEXT` when reproducing a historical file. Existing MLP
files may be imported through `DVDA_MLP_EXTERNAL_DIR`; imported files are
verified read-only and are not re-encoded.

## Runtime and cache

The release ZIP is GUI-ONLY native Windows x64. Project-owned media, image,
menu and MLP implementations are linked into the GUI. The frozen MLP DLL is
used only for acceptance and is not packaged. Missing required third-party
components are errors, never a silent fallback to external programs.

Cache identity includes source content, encoder identity, PCM conversion policy,
format parameters, metadata context and output content. Old entries without
that provenance are rebuilt. Failed or cancelled jobs do not publish partial
MLP files.

## Validation

Run the Rust workspace checks from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File rust\crates\dvda-mlp\build-runtime.ps1
```

This recipe needs only the frozen DLL, not C sources or a C compiler. It checks
its SHA256 before and after tests, runs 21 Rust tests including 27,360 grouped
raw-byte/SHA256 cases, strict all-targets Clippy, and 142 standard plus 228 grouped
stress profiles. Identical PCM, parameters and metadata must produce identical
raw MLP bytes and SHA256. Source-only C state/interpolation tests have been
removed rather than skipped; grouped diagnostic decode compares the frozen and
Rust outputs and does not certify native mixed-rate player compatibility.

The native ABI tests cover the encoder, PCM comparison, format parsing and
failure paths. The original-file byte comparison corpus remains recorded in
`native/mlp-encoder/mlpencoder-validation.json`; it is evidence for the pinned
encoder artifact and is separate from the generated-output cache.

The encoder rejects unsupported layouts, floating-point PCM, damaged input and
tracks that exceed the current RIFF/WAVE 32-bit length boundary. Oversized
access units use the lossless bounded fallback documented in
`docs/MLP-OVERSIZE-FIX.en.md`.
