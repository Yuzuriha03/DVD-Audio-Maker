# MLP core — Windows x64

The application embeds `win-x64/mlp_encoder.dll` and calls its streaming C ABI in process. There is no standalone MLP encoder EXE or x86 proxy in the product. The DLL exports are declared in `source/mlp_encoder.h`. GUI and CLI both target win-x64.

The x64 port uses explicit x87 PC53/round-to-nearest and saves/restores x87 and SSE control state around host callbacks. Compile with MinGW-w64 GCC and `-mfpmath=387 -fexcess-precision=standard -ffp-contract=off`; do not silently change floating-point options. Only `encode_file.c` and `mlp_group_input.inc` change the native host FP bridge relative to the source snapshot; the encoding decisions and bit serialization remain the algorithm.

Current DLL/source fingerprints, compiler flags, and original source fingerprints are recorded in `mlpencoder-validation.json`. The Windows build recipe is `build.cmd`; set MLP_CC to a Windows x86_64-w64-mingw32-gcc executable if necessary. Rebuilds produce a separate .rebuilt.dll and do not replace the pinned tested artifact. The delivered application needs no compiler or WSL.

Managed input/output uses bounded streaming buffers, validates explicit metadata, propagates cancellation/exceptions and publishes only complete output. No encoded-output byte patching is performed. See [application integration](../../../docs/MLP-ENCODER.md) and [GUI plan](../../../docs/GUI-AND-DLL-PLAN.md).
