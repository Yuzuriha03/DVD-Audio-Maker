# C# migration status

[简体中文](CSHARP-MIGRATION.md) | [English](CSHARP-MIGRATION.en.md)

> Historical engineering record: this document preserves experiments, hypotheses and results from their original development stages. Later experiments supersede some earlier conclusions. Disabled approaches are not current configuration recommendations; see the [project README](../README.en.md) for the supported workflow.

## Migration completion

The first independently buildable .NET 10 projects are in place:

- `DvdaMaker.Configuration`: configuration parsing, source priority and derived values.
- `DvdaMaker.Cli`: entry points for configuration, preparation, planning, building, M4A conversion and output verification.
- `DvdaMaker.Formats`: ISO9660 reading, MLP alignment checks and PES PTS parsing.
- `DvdaMaker.Processes`: shared external-process execution, argument passing, output capture, timeouts and cancellation.
- `DvdaMaker.Preparation`: source scanning, ffprobe metadata, album normalization, ffmpeg decoding checks and manifest generation.
- `DvdaMaker.Building`: manifest reading, global ordering, greedy disc planning by album and parameter grouping within each disc.
- `DvdaMaker.CompatibilityTests`: a compatibility baseline without a NuGet test framework (88 checks at this migration checkpoint).

C# covers configuration, preparation, conversion, building, menu assets, font coverage, AMG/ASVS,
menu visuals and output auditing. `build.cmd` and `verify.cmd` are thin native Windows wrappers around the C# CLI.
The old Python business scripts in the repository root were removed on September 29, 2026. Their final versions remain
under the Git tag `python-reference-final`.

## Build and test

Requires the .NET 10 SDK:

    dotnet build DVD-Audio-Maker.sln
    dotnet run --project tests/DvdaMaker.CompatibilityTests

The migration checkpoint had 88 ordinary compatibility checks covering configuration, format parsing, external processes, source preparation, ALAC repair,
zero-write M4A dry-runs, rejection of non-ALAC inputs, per-file failure isolation, audit-log parsing, MLP acquisition and indexing,
disc planning and grouping, end-to-end builds using fake `dvda-author`, `build.cmd` argument branches,
equal-length PCM comparison, working-drive space preflight, source caches, MLP cache credentials, per-disc resumption and concurrent encoding.

Compare against real reference discs and SurCode MLP:

        dotnet run --project tests/DvdaMaker.CompatibilityTests -- \
            --real-fixtures \
            "E:\DISCs\DVD-Audio\Wuthering Waves Singles EPs" \
            "D:\Music\ReferenceMlp"

Real-fixture tests do not copy multi-gigabyte ISOs or MLP files into the repository. They retain the paths, LBAs, file hashes,
sector-sample hashes and MLP structure statistics read by the pre-migration reference implementation. Ordinary tests
can still run independently when external fixtures are unavailable.

Inspect configuration:

    dotnet run --project src/DvdaMaker.Cli -- config
    dotnet run --project src/DvdaMaker.Cli -- config --shell
    dotnet run --project src/DvdaMaker.Cli -- config --check

Run C# source preparation:

    dotnet run --project src/DvdaMaker.Cli -- prepare

Preview a C# build plan using an existing MLP cache:

    dotnet run --project src/DvdaMaker.Cli -- plan

`plan` does not invoke `dvda-author` or the in-process ISO writer; it reports errors for missing MLP cache entries. It handles
track ordering, album aggregation, size estimates, greedy disc allocation, parameter grouping and album boundaries.

Run a build preview:

    dotnet run --project src/DvdaMaker.Cli -- build --dry-run

According to configuration, this command encodes/reuses MLP with the MLP encoder or locates and probes external MLP, then plans discs and
writes a separate `mlp_index-dryrun.json`. It does not overwrite `mlp_index.json` used by completed output.
The MLP encoder serializes complete headers, checksums and termination flags. Output is verified without post-encoding patches.

Build actual discs without menus:

    dotnet run --project src/DvdaMaker.Cli -- build

The real build path integrates `dvda-author` and its in-process ISO writer, audit-compatible build logs, ISO capacity checks,
final publication and cleanup after success. With `DVDA_MENU=on`,
C# generates menu assets, authors menus and verifies the finished menus. The final index is first written to
`mlp_index.pending.json`. All planned discs are staged separately, then the full ISO set and index
are committed as one rollback-capable transaction. If a destination is locked, copying fails, capacity is exceeded, cancellation occurs or a later stage fails,
the previous ISO set and `mlp_index.json` remain consistent. No disconnected `_new.iso` is published.

`prepare` now handles FLAC/M4A scanning, parameter normalization, channel consistency, decoded sample-count
checks, reports and the manifest, including automatic repair of missing END markers in Apple ALAC uncompressed frames.
Repair writes a copy in the working directory and leaves originals untouched. Repaired-frame records appear in reports and the manifest.

The standalone `convert` / `m4a2flac` workflow is migrated. It accepts only `.m4a` files whose first audio stream is ALAC,
detects and repairs END markers before conversion, preserves normalized tags and artwork, and verifies repaired PCM MD5, output tags and
artwork bytes. `--dry-run` detects only; it creates neither repaired copies nor FLAC files. One file's failure does not abort the batch.

Output verification has two distinct entry points:

- `quick-check`: total IFO track counts across all ISOs; check track-start packs, PGC cell timelines,
    title durations and ATSI/ASVS still-picture reference bounds.
- `audit`: select the newest candidate build log by modification time, strip ANSI, map each disc's groups from dvda-author
    commands, and check AOB sectors, inter-track continuity, PTS completeness and title-start resets.

Both standalone entry points accept input-path overrides:

    dvda quick-check --iso-dir DIR --manifest FILE --log FILE
    dvda audit --iso-dir DIR --manifest FILE --log FILE

A single menu ISO can be checked outside the default output directory:

    dvda verify menu --iso FILE

If the final index contains a plan for that ISO, page counts, album index cells and still counts are checked too. Without
an index, generic AMG/ASVS structure and per-page image checks still run. After structural errors, visual checks continue
and their results are combined whenever the menu VOB and PGC cell ranges remain readable.

Missing logs, track tables or dvda-author commands mean "audit unavailable" and return exit code 2. Actual output
defects return exit code 1. `verify menu/all` extracts images from `AUDIO_TS.VOB` page by page using AMG PGC cell ranges,
checks image statistics with ImageMagick and inspects backgrounds, thumbnails and bright title text in every cell of the 4×3 grid
on every top-level index page, including a partially filled final page.
Missing external tools are reported as unavailable, never as a pass.

External MLP mode now prefers mirrored paths, rejects ambiguous basenames and empty files, and checks MLP structure and
EOS. It also rejects multiple tracks sharing one MLP path before writing `mlp_index.json`.

The actual disc executor passed both success and failure end-to-end fixtures. The success path verifies authoring output,
track-table logs, ISO publication and cleanup. The failure path verifies diagnostics, blocked publication and retained evidence.

`DVDA_FINAL_DIR` is the only final output directory. Once the publication transaction succeeds, ISOs go directly there,
without Robocopy or a second destination directory.

## Retained boundaries

- `build_dvda_author_mlp.sh` and internal toolchain build scripts under `tools/` remain. They build third-party/local
    C tools and are not part of the Python business workflow.
- Windows releases now use `dotnet publish -r win-x64 --self-contained` to include the C# CLI.
    They neither copy nor launch Python business scripts; target machines need neither Python nor a .NET Runtime installation.
- `tools/win-build/make-menu-font.sh` now uses `DvdaMaker.FontTool` to extract SC, JP and KR faces by family name from
    TTC collections, rebuild standalone OpenType in pure C#, repair checksums and
    verify coverage of four character sets. Development and packaging no longer depend on Python/fontTools either.
- View removed Python reference files with `git show python-reference-final:<file>`,
    or create a temporary worktree at that tag. The main branch no longer carries duplicate implementations.

## Frozen configuration contract

- Priority: environment variables > selected configuration file > built-in defaults.
- Parse only `KEY=VALUE`; no variable expansion or command substitution.
- Support paired single/double quotes and trailing comments on unquoted values.
- Convert backslashes in likely paths to `/`, retaining the Python implementation's behavior.
- Preserve rules for derived paths, ISO prefixes, volume labels, file names and numeric limits.
- `--shell` preserves Bash single-quote escaping and the original key set.
- `--shell-all` includes C# additions for menus, the legacy metadata-tool compatibility key and derived directories.
- `--check` returns exit code 2 when required paths are missing.

## Continued validation

1. Keep running the ordinary compatibility suite and the three real ISO/SurCode MLP baselines; the migration checkpoint had 88 ordinary tests.
2. Verify tags, artwork and PCM MD5 for newly added real ALAC fixtures.
3. Extract every page from real multipage menu ISOs and smoke-test the Windows self-contained release.
4. Use `python-reference-final` as a read-only historical baseline when investigating migration differences.
