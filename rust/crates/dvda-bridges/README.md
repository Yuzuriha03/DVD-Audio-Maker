# Directly linked native bridges

Production integration uses the `media` feature for FFmpeg bridges and the
`dvda-image` crate for Rust image decoding and rendering (default features are
empty). The `direct` build enables `media,menu`; the `author` build enables
`media,menu,author-iso`. Neither requires
ImageMagick libraries or XML configuration. `author-loader` is a differential-only
adapter. The C ABI exports remain available for native author linkage and
frozen-oracle differential tests.

## Integration contract

`direct::MediaRequest` owns its UTF-8 C strings. Operation/options retain the
legacy numeric ABI; set required transcode options explicitly. `tags` is an even
sequence of key/value C strings. `run()` discards callback messages;
`run_with_callbacks(Call)` supports log/cancellation callbacks, with caller-owned
state remaining valid and callbacks never unwinding. Invalid tag pairs fail with
-22 without dereferencing missing values.

The legacy ImageMagick bridge, layout probe, shim and image feature have been retired.
Image decoding and rendering use `dvda-image`. Explicit frozen-oracle tests remain
in `dvda-core`, using the hash-validated external adapter in `dvda-native`.
No ImageMagick libraries or XML configuration are used by production builds.

`DVDA_FFMPEG_PREFIX` names the retained FFmpeg install; its native dependencies
remain required. Media callback routing is thread-local.

`tools\win-build\build-rust-bridges.ps1` defaults to `-Component direct`, emitting
an rlib and provenance manifest, not adapter DLLs. A final executable build must
resolve and audit its own PE dependency closure; the rlib manifest is not a
runtime-package acceptance claim. `author` emits the static author bridge;
The explicit media mode is a differential adapter. Image/shim modes and legacy image inputs are no longer supported. `FfmpegPrefix` may be absolute or relative
to the invoking working directory; it is resolved before Cargo runs.
All manifests keep `production_accepted=false`.

## Verified evidence and remaining gates

Fresh retained-oracle runs under `build`:

- `bridge-image-fresh.log`: four image fixtures, CLI commands, RGBA capacity
  boundaries and eight Y4M profiles; status/callback/output parity, plus each
  null RGBA/Y4M pointer argument. CLI boundary checks cover zero/short/excess
  argument counts, a null argument array and a null individual argument.
- `bridge-media-fresh.log`, `bridge-cover-acceptance.log` and
  `bridge-video-fresh.log`: probe, packet list, audio conversion option matrix,
  cover extraction, MPEG first-frame PNG extraction (operation 4), errors,
  initial and post-packet cancellation; callback and successful output byte
  parity (attached-picture FLAC included). Malformed media requests cover null
  request/callback/input, short/wrong ABI headers and an invalid operation.
- `bridge-menu-fresh.log`: 96 byte-identical PAL/NTSC/aspect/still/audio-length
  combinations plus missing input/output, invalid norm/aspect and malformed Y4M.
- `bridge-loader-fresh.log`: author-loader missing-runtime retry, Unicode RGBA via
  SPU, image command/Y4M, navigation and error paths. Run in a fresh target;
  acceptance refuses to rename or overwrite a pre-existing image runtime.
- `direct-acceptance` now requires only `media`: it decodes images through
  `dvda-image` while exercising four concurrent media callback streams, Unicode
  audio paths, media cancellation, missing-image errors and RGBA length checks.
  Run with `--features media --example direct-acceptance -- <audio.wav> <image.png>`;
  no ImageMagick library or configuration is needed for this acceptance example.
  Legacy image CLI callback/cancellation checks belong to the frozen-oracle tests.

Production readiness remains **gated**: final application/embedded-author direct
linkage and final executable dependency closure are not certified here.
Malformed ABI coverage is bounded to the cases above; post-packet cancellation
is certified, but mid-operation cancellation during audio encoding, frame/cover
extraction and image processing remains a release gate. Operation 4 extracts
the first video frame as PNG; general video transcoding is not a bridge API
operation. Do not represent these passes as complete feature certification.

## Fonts

Bundled CJK fonts are selected and rendered by `dvda-image`; production does not
load ImageMagick font configuration. See the crate's text tests and the application's
PAL/NTSC menu acceptance tests for current coverage. Historical bridge logs above
are not substitutes for Rust rendering acceptance.

## Decoder validation

```powershell
cargo build --offline --manifest-path rust\Cargo.toml -p dvda-bridges --features media --bin mlp-decode-probe
tools\win-build\test-rust-decoder.ps1 -RustProbe rust\target\debug\mlp-decode-probe.exe -LegacyProbe build\legacy-mlp-decode-probe.exe
```

Nine retained rate/depth/channel fixtures have byte-exact PCM/metadata parity;
malformed input, unwritable output and capabilities statuses match. The
`--capabilities` contract rejects non-MLP codecs (11) and incorrect component
counts (12). Full retained FFmpeg gives 11, not minimal-build acceptance.
