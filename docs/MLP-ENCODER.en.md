# MLP encoding

The normal `surcode-batch` workflow calls the bundled MLP encoder through its
streaming x64 C ABI. It does not start `surcodemlp.exe`, eac3to, an MLP encoder
subprocess, or an external FFmpeg executable.

The current path is:

```text
source audio -> bundled media DLL -> integer PCM WAVE -> mlp_encoder.dll
             -> read-only verification -> MLP cache -> DVD authoring
```

The encoder source and the pinned Windows x64 DLL are under
`native/mlp-encoder`. Build the DLL with the repository's MinGW-w64 recipe in
`native/mlp-encoder/build.cmd`; the delivered package already contains the
validated DLL. The GUI and CLI use the same Rust pipeline. There is no separate
MLP encoder EXE.

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

The release ZIP is native Windows x64. Media and image functionality
is provided by project-built DLLs shipped beside the GUI. The MLP DLL is loaded
from the package and checked by SHA-256. Development overrides may select
validated native component directories, but missing components are reported as
errors rather than silently falling back to external programs.

Cache identity includes source content, encoder identity, PCM conversion policy,
format parameters, metadata context and output content. Old entries without
that provenance are rebuilt. Failed or cancelled jobs do not publish partial
MLP files.

## Validation

Run the Rust workspace checks from the repository root:

```powershell
cargo fmt --all -- --check
cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- --include-ignored
cargo clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
```

The native ABI tests cover the encoder, PCM comparison, format parsing and
failure paths. The original-file byte comparison corpus remains recorded in
`native/mlp-encoder/mlpencoder-validation.json`; it is evidence for the pinned
encoder artifact and is separate from the generated-output cache.

The encoder rejects unsupported layouts, floating-point PCM, damaged input and
tracks that exceed the current RIFF/WAVE 32-bit length boundary. Oversized
access units use the lossless bounded fallback documented in
`docs/MLP-OVERSIZE-FIX.en.md`.
