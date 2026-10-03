# GUI and in-process MLP encoding implementation plan

[简体中文](GUI-AND-DLL-PLAN.md) | [English](GUI-AND-DLL-PLAN.en.md)

Date: October 1, 2026. Status: complete; delivered as native x64 as requested.

## Goals and boundaries

Use a Windows GUI as the everyday entry point. Folder pickers, option controls and task buttons cover preparation, preview, authoring and verification without requiring manual config.env editing. Keep the CLI and config.env reader, reusing existing build, menu, verification and cache logic.

Replace launching mlp_encode.exe with direct calls to the mlpencoder mlp_encoder.dll. No MLP encoding subprocess runs. At this checkpoint, eac3to, FFmpeg decoding and authoring tools remained external tools. The application itself is a normal Windows GUI EXE.

## Technical choices

- .NET 10 WinForms, initially with a Chinese interface: folder/tool selection, grouped settings, live logs, stage progress, cancellation and run state. Slow work runs in the background; tasks cannot overlap.
- The GUI calls PreparationPipeline, BuildPipeline and VerificationPipeline directly, without temporary env files or CLI subprocesses for passing settings.
- Store independent JSON settings in the user directory and support opening/saving profiles. Explicit env imports preserve the existing format and unknown keys. First launch can discover an existing env file and shows its source; saved GUI settings take priority to avoid silent replacement.
- Group settings into paths/discs, encoding, menus and advanced tools, with validation. Visible GUI values take priority for GUI tasks, so hidden environment variables cannot override controls. CLI environment precedence stays unchanged.
- Use the pinned Windows x64 encoder DLL and synchronous streaming callback API. GUI/CLI target win-x64 and can invoke external x64 tools. Native x64 is required. Preserve the algorithm and explicit floating-point precision semantics, and rerun full-file original comparisons for acceptance.
- Embed the DLL and extract/load it by SHA256. Managed streaming callbacks carry PCM and output, supporting Chinese paths, bounded buffers, cancellation, timeouts, exception propagation and failure cleanup. Independent per-job state supports concurrency.
- Supply auxiliary metadata before encoding. Do not read a reference file as encoding input or patch finished MLP. Preserve the default empty-metadata policy and explicit historical-context support.

## Implementation steps

1. [Complete] Integrate the streaming DLL encoder, remove the embedded MLP EXE, adjust process architecture and verify ABI, cancellation and exact output.
2. [Complete] Add a configuration model that constructs DvdaOptions directly, JSON save/load, env import and configuration checks.
3. [Complete] Implement the main GUI, grouped editors, background tasks, logs, progress and cancellation using existing pipelines.
4. [Complete] Update the solution, launch scripts, self-contained package and user documentation. Make the GUI the default release entry point while retaining the CLI.
5. [Complete] Run regression and GUI smoke tests. Cover 84 default formats with generated PCM, compare 78 available original samples byte for byte, and record results.

## Acceptance criteria

- Double-click launches the GUI; configuration and execution require no config.env. Existing env files can be imported, and independent profiles persist and reopen.
- Preparation, preview, real builds and verification clearly report success, failure or cancellation. The GUI remains responsive. Closing during a task cancels it and waits for resources to be released.
- The MLP encoding path uses no Process.Start or encoder EXE and has no original SurCode dependency. DLL and C-source fingerprints are traceable.
- Identical PCM, encoding parameters and auxiliary metadata yield identical complete MLP bytes. Replacement is complete only when every existing comparison passes.
- Verify configuration compatibility, concurrent isolation, cache invalidation and prevention of partial publication after failure or cancellation.
- WinForms and the self-contained win-x64 release run successfully. Missing conversion or authoring tools produce actionable errors.

## Explicit limitations

Encoding callbacks stream data and avoid loading an entire PCM track in the bridge. Existing validators may still read large MLP files in full; that remains a future large-file optimization. The batch GUI still uses one target sample rate and bit depth. Mixed channel-group interfaces require separate work and are not advertised as supported by the GUI.

## Architecture update, October 1, 2026

The user required x64, replacing the x86 host proposal. GUI, CLI and MLP DLL all use win-x64, with no hidden x86 EXE proxy. Acceptance includes x64 builds, ABI layout, floating-point control and byte-for-byte regressions.

## Final acceptance record

- GUI, CLI and encoder DLL all have PE Machine AMD64.
- 97/97 regression checks passed; 84/84 default-format cases decoded to exact PCM.
- Through the actual batch entry point, the x64 DLL matched all 78 original files, totaling 95,138,694 bytes.
- The actual GUI successfully checked/previewed sources, built a test ISO and verified output.
- Self-contained package: build/gui-x64-release/DVD-Audio-Maker. DVD-Audio-Maker.exe in its root is the GUI entry point.
- Detailed results: gui-dll-validation.json.
