# Directly linked native bridges

Production integration is a Cargo path dependency with `features = ["media", "image"]`
(default features are empty). Project-owned media and image code is linked as an
rlib; these features never load `dvda-media.dll` or `dvda-image.dll`.
`author-loader` is a differential-only adapter and is deliberately excluded from
this feature combination. The C ABI exports remain available for native author
linkage and frozen-oracle differential tests.

## Integration contract

`direct::MediaRequest` owns its UTF-8 C strings. Operation/options retain the
legacy numeric ABI; set required transcode options explicitly. `tags` is an even
sequence of key/value C strings. `run()` discards callback messages;
`run_with_callbacks(Call)` supports log/cancellation callbacks, with caller-owned
state remaining valid and callbacks never unwinding. Invalid tag pairs fail with
-22 without dereferencing missing values.

`direct::{read_rgba,write_y4m,image_command,image_run}` offer safe owned-buffer and
CStr/CString entry points. `image_run_with_callbacks` has the same callback safety
contract. `image::dvda_image_read_rgba` is the C-compatible callback for the
menu crate's explicit `ReadRgba` injection (no dependency cycle).
ImageMagick is initialized once and serialized; media callback routing is
thread-local. Put `policy.xml` and `colors.xml` beside the final executable.

`DVDA_FFMPEG_PREFIX` names the retained FFmpeg shared install;
`DVDA_MAGICK_WORK` names the existing ImageMagick work root including static
Wand/Core, `libfreetype-minimal.a` and `libucrt-jump.a`. `DVDA_MSYS_ROOT` defaults
to `C:\msys64`. Third-party FFmpeg runtime dependencies are still required;
ImageMagick static libraries and their existing delegates remain third-party.

`tools\win-build\build-rust-bridges.ps1` defaults to `-Component direct`, emitting
an rlib and provenance manifest, not adapter DLLs. A final executable build must
resolve and audit its own PE dependency closure; the rlib manifest is not a
runtime-package acceptance claim. Explicit media/image/author modes are retained
only for differential tests. All manifests keep `production_accepted=false`.

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
- `bridge-direct-acceptance.log`: owned API Unicode, concurrent image/media,
  callback isolation, cancellation, errors and buffer sizing.

Production readiness remains **gated**: final application/embedded-author direct
linkage and final executable dependency closure are not certified here.
Malformed ABI coverage is bounded to the cases above; post-packet cancellation
is certified, but mid-operation cancellation during audio encoding, frame/cover
extraction and image processing remains a release gate. Operation 4 extracts
the first video frame as PNG; general video transcoding is not a bridge API
operation. Do not represent these passes as complete feature certification.

## Embedded font configuration

The image bridge uses `DVDA_IMAGE_NATIVE_DIR` for ImageMagick configuration when
set by embedded-runtime initialization. Direct linking places the bridge in the
EXE, not alongside the extracted `type.xml` and `fonts` directory. Without a
runtime override, standalone DLL builds retain module-local configuration.

Validate the actual bundled CJK fonts from a directory outside the executable:

```powershell
cargo run --offline --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu -p dvda-bridges --features image --example font-acceptance -- <extracted-runtime-directory>
```

This checks 18 rendered script/face combinations, nonzero ink and no native font
warnings; it is a representative rendering check, not exhaustive glyph coverage.

## Decoder validation

```powershell
cargo build --offline --manifest-path rust\Cargo.toml -p dvda-bridges --features media --bin mlp-decode-probe
tools\win-build\test-rust-decoder.ps1 -RustProbe rust\target\debug\mlp-decode-probe.exe -LegacyProbe build\legacy-mlp-decode-probe.exe
```

Nine retained rate/depth/channel fixtures have byte-exact PCM/metadata parity;
malformed input, unwritable output and capabilities statuses match. The
`--capabilities` contract rejects non-MLP codecs (11) and incorrect component
counts (12). Full retained FFmpeg gives 11, not minimal-build acceptance.
