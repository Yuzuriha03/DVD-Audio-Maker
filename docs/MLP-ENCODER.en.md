# MlpEncoder-core MLP encoding (October 2, 2026)

[简体中文](MLP-ENCODER.md) | [English](MLP-ENCODER.en.md)

## Current execution chain

`surcode-batch` (also accepting `batch-surcode`) now invokes the MLP encoder embedded in this project:

`Source → FFmpeg → integer PCM WAVE → in-process encoder DLL → read-only verification → MLP cache → disc authoring`

The original `surcodemlp.exe`, GUI automation and SSF sessions have been removed from the execution chain. The FFmpeg MLP encoding provider is also removed. FFmpeg/FFprobe remain for source processing, decoding, probing and output verification. Existing external MLP import through `external` / `surcode` remains available. `DVDA_MLP_SOURCE=ffmpeg` now produces an explicit error.

## Configuration

```ini
DVDA_MLP_SOURCE=surcode-batch
DVDA_FFMPEG=C:/tools/ffmpeg/bin/ffmpeg.exe
DVDA_MLP_SURCODE_SAMPLE_RATE=48000
DVDA_MLP_SURCODE_BITS=24
DVDA_MLP_JOBS=1
DVDA_MLP_EXTERNAL_DIR=
DVDA_MLP_METADATA_CONTEXT=
```

The existing sample-rate and bit-depth setting names remain for compatibility and define the batch target format. Supported rates are 44100, 48000, 88200, 96000, 176400 and 192000 Hz; supported depths are 16, 20 and 24 bits. Channels follow the input layout: up to six at the four lower rates, and up to two at the two higher rates. An empty MLP output directory uses `mlp` under the build directory. `DVDA_MLP_SURCODE_EXE` is no longer required.

Conversion preserves channel layout and PCM order. FFmpeg prepares sample rate and bit depth; WAVE normalization checks significant bits and handles side-surround labels, retaining compatibility with legacy PCM that omits a final RIFF alignment byte. 20-bit PCM uses explicit quantization in 24-bit storage. Without resampling or precision reduction, PCM is unchanged. Encoded output is never repaired with patches. The batch GUI currently uses one sample rate and bit depth; the MLP encoder's mixed channel-group interface is not exposed as project configuration.

## Conditions for byte identity

Input PCM, encoding parameters and auxiliary metadata context must all match. The default is a deterministic empty auxiliary TLV context, without injecting the current clock. To reproduce a historical original, specify the corresponding `MSCTX001` context file as `DVDA_MLP_METADATA_CONTEXT=.../metadata.stampctx`. Context is supplied before encoding. The encoder does not read the comparison MLP, and the application does not replace output bytes afterward.

Explicit context contains AU totals and intervals and must match the input track. Validate historical files with single-track jobs; do not reuse one track's context blindly across an album. Output with default empty context cannot be claimed identical to arbitrary historical files containing timestamps or other auxiliary data.

## Release and caches

GUI, CLI and embedded DLL are Windows x64. The host calls the MLP encoder directly through a streaming C ABI, explicitly preserving x87 PC53 arithmetic and floating-point control state around callbacks. No MLP encoding subprocess runs. The core and encoder source are under `src/DvdaMaker.SurcodeTool/Native`; the external development workspace and original installation directory are not dependencies.

Single-file releases contain the resource. At runtime it is extracted to `%LOCALAPPDATA%/DVD-Audio-Maker/native/<SHA256>/mlp_encoder.dll` and SHA256-verified. Users need no C compiler. Cache identity includes source content, core, actual FFmpeg binary, PCM conversion policy, metadata context, format parameters and output content. Old caches without provenance are rebuilt. Preparation or encoding failure preserves published MLP.

Current WAVE input uses RIFF's 32-bit length. Tracks exceeding approximately 4 GiB of PCM need future RF64/streaming support. Unsupported layouts, floating-point PCM, loss of significant precision and damaged inputs are rejected.

## Validation coverage and reruns

Before the FFmpeg preparation migration, the self-contained Windows x64 single-file package passed 78 complete original comparisons: synthetic FLAC through actual eac3to, the project's batch entry and the MLP encoder produced **95,138,694 bytes** identical to the original files. Each case also checked independent core decoding and a second-run cache hit, with matching PCM and explicit auxiliary metadata.

The project passed 84 default channel-layout cases through native encoding, independent FFmpeg decoding and exact PCM comparison. Before migration, seven additional real FLAC → eac3to → mlpencoder cases passed PCM comparisons and full-file comparisons against direct core encoding. Coverage included Chinese paths, final-AU zero padding, 16/20/24 bits and all six rates. Cache, concurrency and failure protection were included in the 97/97 compatibility checkpoint.

```powershell
dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release
dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release -- --mlpencoder-integration
dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release -- --mlpencoder-batch-integration C:/tools/ffmpeg/bin/ffmpeg.exe
```

The final two commands require real FFmpeg. The batch entry no longer requires eac3to. Tests generate only synthetic audio and print the temporary directory retaining their evidence.

The earlier 720 core-parameter checks and 78 original comparisons are upstream evidence; they do not mean every mixed-group configuration is exposed here. See `Native/mlpencoder-validation.json` for source/binary fingerprints and upstream scope. Historical EXE-integration results are in `mlpencoder-integration.json`.

## GUI / x64 DLL integration

GUI controls replace manual configuration editing. Existing env files can be imported or read by the CLI. The GUI calls the same pipelines directly. Independent DLL-integration regression results are in gui-dll-validation.json, separate from the earlier EXE-integration records.

## Lossless oversized-AU fallback (October 2, 2026)

The MLP encoder now checks AU size before committing each restart interval. If the normal encoding plan fits the existing 1,536-byte limit, complete output remains unchanged. Only overflowing intervals use lossless coding without prediction filters, preserving actual PCM, reversible matrices, channel order, AU count and explicit metadata. The fallback must still pass existing size and FIFO checks, with no bit-depth reduction, resampling or output patching.

The 88.2 kHz / 24-bit / six-channel high-noise fixture's oversized block decreased from 1,582 to 1,526 bytes. FFmpeg, original VFY and mlpencoder VFY all confirmed complete PCM fallback. All 198 PCM cases passed; 197 previously successful outputs were unchanged, and all 78 original comparisons totaling 95,138,694 bytes remained identical. See the [fix notes](MLP-OVERSIZE-FIX.en.md) for implementation, limits and results.

New regression entry point (requires FFmpeg):

    dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release -- --oversized-au-integration

## FFmpeg source preparation migration (October 2, 2026)

The batch entry now reads DVDA_FFMPEG, supporting full paths and PATH lookup. Old eac3to configuration remains readable but is not executed. Converter or policy changes rebuild old MLP caches. Native encoder source and the pinned DLL were unchanged. See the [migration plan and acceptance results](FFMPEG-PCM-MIGRATION.en.md).

Post-migration validation passed 202/202 FFmpeg batch PCM cases, 78/78 original full-file byte comparisons, 104/104 compatibility checks, plus published x64 GUI transcoding, old-cache migration, ISO authoring and output verification.
