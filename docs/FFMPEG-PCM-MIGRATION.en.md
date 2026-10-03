# FFmpeg source preparation migration (October 2, 2026)

[简体中文](FFMPEG-PCM-MIGRATION.md) | [English](FFMPEG-PCM-MIGRATION.en.md)

## Goal and scope

Use DVDA_FFMPEG for surcode-batch source decoding, resampling and target bit-depth preparation. MLP remains encoded by the existing Windows x64 in-process encoder DLL. Do not change the native encoding algorithm or restore FFmpeg's MLP encoding branch.

## Implementation sequence

1. Integrate PCM-validated FFmpeg arguments, preserve channel layouts and explicitly prepare 16/20/24-bit integer PCM. Without resampling or precision reduction, require sample-exact PCM.
2. Update configuration, GUI, logs and cache identity. Existing config.env files remain readable; old eac3to paths no longer participate in execution. FFmpeg supports absolute paths and PATH lookup.
3. Add tests through the real batch entry point for all DVD-Audio formats, channel order, 20-bit precision, conversion, failure protection, cancellation and caches.
4. Compare complete bytes of successful original samples. Identical PCM and metadata must produce identical MLP. Resampling/bit-depth reduction does not promise eac3to-identical conversion; require the new pipeline's target PCM to match decoded MLP exactly.
5. Run compatibility tests, refresh the Windows x64 package, and validate published encoding, caches and disc authoring.

## PCM policy

Select only the first audio stream. Do not mix channels or force a different layout. Use fixed swresample parameters and reproducible conversion with dithering disabled. Quantize 20-bit PCM explicitly in FFmpeg filters and store it in a 24-bit container. Existing WAVE normalization validates significant bits and DVD surround labels; it does not patch MLP output.

Parameter semantics were checked against the installed FFmpeg help for filter=aresample and filter=aeval. Existing high-noise and ordinary-signal baselines were extended into real batch-entry tests.

## Completed validation

- 104/104 compatibility tests passed, including failures, cancellation, output protection, PATH, existing env files and GUI logs.
- 202/202 real batch cases passed: 84 native formats × ordinary/high-noise signals, 15 conversions × both signal types, plus three side-surround layouts and one six-channel ALAC case.
- Every case verified target PCM against independent FFmpeg decoding, allowed only existing AU zero-tail padding, and compared the full output with direct core encoding. Native formats also preserved input PCM. The 20-bit reduction cases checked rounding and clipping.
- 78/78 original comparisons passed, totaling 95,138,694 bytes. The chain was source WAVE → FLAC → FFmpeg through the new batch entry → pinned DLL, with identical auxiliary metadata.
- Native MLP source and the pinned DLL were unchanged. DLL SHA256 remains ece6d0a8033a26e2528042a7b74c66c249ea3c8d7378c06809fb94c8f6bd79b8.
- The published GUI passed transcoding of the historical oversized case, cache reuse, rebuilding a cache with old eac3to identity, ISO authoring and output verification. Tests deliberately specified a nonexistent eac3to path.
- GUI, CLI, MLP DLL and installed FFmpeg are AMD64. All 146 release-manifest hashes passed, and the release ZIP was updated.

See [ffmpeg-pcm-migration-validation.json](ffmpeg-pcm-migration-validation.json) for the complete acceptance summary and cases.

## Usage

On the GUI's "Audio encoding" page, set "Audio conversion and verification", corresponding to DVDA_FFMPEG. The default ffmpeg is resolved through PATH; alternatively select the full ffmpeg.exe path. Existing config.env/JSON DVDA_MLP_EAC3TO_EXE values remain readable but do not affect validation, execution or cache identity. DVDA_MLP_SOURCE remains surcode-batch; MLP encoding stays in-process.

The first migrated run rebuilds old MLP caches using the new converter identity. Conversion failure or cancellation does not overwrite existing MLP. Resampled or lower-bit-depth target PCM may differ from eac3to output; this difference occurs during source conversion and is not hidden by modifying encoded files.

Reproduction commands:

    dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release
    dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release -- --ffmpeg-pcm-integration <empty-output-directory>
    dotnet run --project tests/DvdaMaker.CompatibilityTests -c Release -- --ffmpeg-original-corpus <original-matrix.json> <empty-output-directory>

Release entry point: build/gui-x64-release/DVD-Audio-Maker/DVD-Audio-Maker.exe. Real-source test materials are under build/ffmpeg-migration-20261002. Tests do not modify the user's music directory.
