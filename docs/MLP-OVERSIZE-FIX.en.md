# Lossless fallback of oversized MLP access units (October 2, 2026)

[简体中文](MLP-OVERSIZE-FIX.md) | [English](MLP-OVERSIZE-FIX.en.md)

## Problem

A deterministic high-noise sample with 11,042 frames at 88.2 kHz / 24-bit / six channels produced a 791-word (1,582-byte) AU in the final restart interval, exceeding the existing 768-word (1,536-byte) limit. The previous code promoted the 0x2000 flag directly to a generic -5 error. The reference implementation performs the same size check, corresponding to Access unit too large.

## Fix

1. Retain the normal algorithm's plan and pre-serialize the complete restart interval before committing interval output or timing state.
2. If the normal plan fits, use it unchanged so bytes remain identical for previously successful inputs.
3. On overflow, use saved exact PCM after the reversible matrix and existing QSS to select entropy parameters without FIR/IIR prediction for that interval, reducing filter-parameter overhead.
4. Preserve matrix and bypass bits, channel scaling, AU count, restart boundaries, integrity checks and explicit auxiliary metadata. Check size again and retain FIFO timing validation.
5. If constraints still cannot be met, fail explicitly and let managed code remove temporary output. The limit is not removed; sample rate and bit depth do not change; audio is not silently altered; encoded bytes are not patched afterward.

The saved unpredicted PCM uses a bounded per-call interval buffer. Fallback state is not shared between jobs. This change does not affect eac3to/FFmpeg source conversion entry points.

## Completed validation

- Original SurCode was tested with exactly the same 11,042-frame PCM. It also reported Access unit too large and produced no output; the updated encoder completes losslessly.

- The failing fixture successfully. The formerly oversized AU is 1,526 bytes; the largest AU in the file is 1,530 bytes.
- FFmpeg, original MLP_VFY.dll, the verifier EXE and DLL all passed. PCM is exact, with only the existing final-AU zero padding.
- 198 format/conversion/high-noise cases passed. All 197 previously successful complete outputs were unchanged byte for byte.
- All 78 verified original comparisons matched, totaling 95,138,694 bytes.
- New managed regressions cover determinism, concurrency, explicit metadata, AU limits, AU counts and independent decoding.

## Boundaries

For an input the original encoder cannot encode, there is no valid original MLP to compare. The fallback is accepted based on lossless PCM, legal AUs and successful validators. Existing samples with valid original output still require complete-file byte identity. Persistently incompressible data may still exceed transfer-rate or buffer constraints; removing those constraints is not a valid way to claim success.

The validation summary is in mlp-oversize-fix-validation.json. The native change is in Native/source/encode_file.c. The pinned DLL resource records its new SHA256, which also invalidates old encoder cache identities.

- Final compatibility result: 101/101 passed.
- The published GUI encoded the fixture successfully with byte-identical output to the independent core test; cache reuse, normal ISO authoring and output verification passed.
- GUI, CLI and MLP DLL are AMD64. All 146 release-manifest hashes passed, and the ZIP was updated.

Release entry point: build/gui-x64-release/DVD-Audio-Maker/DVD-Audio-Maker.exe.
