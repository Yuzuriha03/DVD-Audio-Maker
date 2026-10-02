# MLP core — Windows x64

Maintainer reference, updated 2026-10-03. For normal use, see the [project README](../../../README.en.md).

The application embeds `win-x64/mlp_encoder.dll` and calls its streaming C ABI in process. There is no standalone MLP encoder EXE or x86 proxy in the product. The DLL exports are declared in `source/mlp_encoder.h`. GUI and CLI both target win-x64.

The x64 port uses explicit x87 PC53/round-to-nearest and saves/restores x87 and SSE control state around host callbacks. Compile with MinGW-w64 GCC and `-mfpmath=387 -fexcess-precision=standard -ffp-contract=off`; do not silently change floating-point options. The normal encoding path retains the decisions and serialization. The x64 host FP bridge lives in [encode_file.c](source/encode_file.c) and [mlp_group_input.inc](source/mlp_group_input.inc). The oversized-AU fallback described below is an additional lossless fallback.

Current DLL/source fingerprints, compiler flags, and original source fingerprints are recorded in `mlpencoder-validation.json`. The Windows build recipe is `build.cmd`; set MLP_CC to a Windows x86_64-w64-mingw32-gcc executable if necessary. Rebuilds produce a separate .rebuilt.dll and do not replace the pinned tested artifact. The delivered application needs no compiler or WSL.

Managed input/output uses bounded streaming buffers, validates explicit metadata, propagates cancellation/exceptions and publishes only complete output. No encoded-output byte patching is performed. See [application integration](../../../docs/MLP-ENCODER.md) and [GUI plan](../../../docs/GUI-AND-DLL-PLAN.md).

## Oversized access-unit fallback (2026-10-02)

Before emitting a restart interval, the encoder serializes its normal plan to check every access unit against the existing 768-word (1536-byte) limit. If the normal plan fits, it is emitted unchanged. An oversized plan is retried without FIR/IIR prediction for that entire restart interval, using retained exact PCM after the reversible matrix and existing QSS. Matrix/bypass data, channel shifts, frame counts, restart boundaries, integrity checks and explicit auxiliary metadata are retained. This avoids predictor parameter overhead without quantizing source audio.

The retry must pass the same size and FIFO checks before publication. The encoder does not raise limits or rewrite an encoded stream. Material that still cannot fit losslessly is rejected; this is not a guarantee that arbitrary sustained full-scale noise fits the delivery rate.

The formerly failing 88.2 kHz / 24-bit / six-channel short noise fixture now passes FFmpeg and original/MLP verification with exact PCM; its oversized restart AU falls from 1582 to 1526 bytes. All 78 existing successful original profiles retain complete byte identity. See [the oversized-AU record](../../../docs/MLP-OVERSIZE-FIX.md) for the exact regression scope.

## Current application pipeline

Source audio is prepared by the in-process media libraries, then streamed to this encoder. Image generation is handled by a separate in-process image runtime. Neither migration changes this pinned MLP DLL. The GUI does not launch external FFmpeg/FFprobe, ImageMagick or original SurCode for those operations; developer reference tests can still use external tools.

The current GUI-only x64 package is framework-dependent and requires .NET 10 Desktop Runtime x64. The core remains embedded in the application assembly, with runtime extraction and SHA-256 verification. There is no separate MLP encoder EXE to distribute.

Complete byte identity requires matching target PCM, encoding settings and auxiliary metadata. The default metadata context is deterministic; a historical original file may require its explicit context. Do not replace the pin with a newly compiled DLL solely because decoding succeeds: independent PCM checks and full-file comparisons are separate acceptance criteria.

For routine debugging, use the repository's GUI/CLI and compatibility-test entry points. The 2026-10-03 workspace check passed 108 compatibility checks and 23 image checks; these do not replace the dedicated original-MLP regression evidence in mlpencoder-validation.json. See [current media integration](../../../docs/INPROCESS-MEDIA.en.md), [current image integration](../../../docs/INPROCESS-IMAGES.en.md) and [development instructions](../../../docs/DEVELOPMENT.en.md).
