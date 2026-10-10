# Retired C MLP encoder

The C source, build recipe, frozen DLLs and old validation output have been
removed. Their historical Git revision is
`6c5086127590001c544373783653fe991f0ebaeb`. The original oracle DLL SHA256 was:

```text
ECE6D0A8033A26E2528042A7B74C66C249EA3C8D7378C06809FB94C8F6BD79B8
```

Production links `dvda-mlp` directly into the application. Developer ABI builds
use the Rust DLL in `build/mlp-encoder`; no C encoder is required.

Build and test the Rust implementation from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File rust\crates\dvda-mlp\build-runtime.ps1
```

The default recipe runs Rust unit tests, strict Clippy and a release build.
Optional historical comparisons require an explicitly supplied external
`-FrozenEncoder` and the `external-oracle` test feature; no comparison DLL is
stored in this checkout. Historical acceptance results remain documented in
[the Rust encoder record](../../docs/MLP-ENCODER.md).
