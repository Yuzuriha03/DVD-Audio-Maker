> 2026-10-01 Update: FFmpeg MLP encoding and the original SurCode automation have been replaced by the fallback core;

[简体中文](TROUBLESHOOTING.md) | [English](TROUBLESHOOTING.en.md)

> Historical engineering record: this document preserves experiments, hypotheses and results from their original development stages. Later experiments supersede some earlier conclusions. Disabled approaches are not current configuration recommendations; see the [project README](../README.en.md) for the supported workflow. Literal filenames and glyph samples are preserved where needed to explain the original tests.
> The following related coding/patching process belongs to historical troubleshooting records. The current process can be seen in MLP-ENCODER.md.

# Troubleshooting Record

This document summarizes the practical problems encountered during the development process, diagnostic methods, and solutions.
Each case includes reproducible verification commands.

> **Migration Note (2026-09-29)**: The Python business scripts in the root directory have been deleted, and all current commands are now provided by
> Provided by the C# CLI. When the article mentions names such as `01_prepare.py`, `02_build.py`, `verify_menu.py`,
> If it belongs to accident review or evolution implementation, it should be understood according to historical records; the final source code is stored in the Git tag.
> `python-reference-final`. Do not copy Python commands from old paragraphs for the current main branch.

---

## Table of Contents

1. [Playback Speed / Progress Bar Cannot Be Dragged](#1-playback-acceleration--progress-bar-cannot-be-dragged)
2. [Various Errors in Compiling `dvda-author`](#2-errors-when-compiling-dvda-author)
3. [`stack smashing detected` (too many tracks)](#3-stack-smashing-detected-too-many-tracks)
4. [`--aob-extract` Segmentation Fault](#4---aob-extract-segmentation-fault)
5. [ffmpeg decoding frame loss (Apple ALAC uncompressed frames missing END marker)](#5-ffmpeg-frame-loss-on-decoding-apple-alac-uncompressed-frames-missing-end-marker)
6. [Last track AOB missing a few bytes](#6-the-last-track-aob-is-missing-a-few-bytes)
7. [Bit depth loss (produces illegal audio group)](#7-bit-depth-loss-produces-illegal-audio-group)
8. [MLP does not record duration (risk of checksum failure)](#8-mlp-does-not-record-duration-risk-of-verification-failure)
9. [Inconsistent number of channels (preventive check)](#9-inconsistent-number-of-channels-preventive-check)
10. [Audit false positive: got the log from the previous build](#10-audit-false-positives-retrieved-logs-from-the-previous-build)
11. [Group matching error in the verification script (ATS_01_1.AOB is group 1)](#11-verification-script-group-matching-error-ats_01_1aob-is-group-1)
12. [Audit/verification false positive: dry-run overwrote the build log](#12-auditverify-false-positives-dry-run-overwrote-the-build-log)
13. [Tag key name case causes album to be split, normalization silently skipped](#13-the-case-sensitivity-of-tag-key-names-causes-albums-to-be-split-and-normalized-silently-skipped)
14. [ffmpeg's MLP encoder does not write END_OF_STREAM](#14-ffmpegs-mlp-encoder-does-not-write-end_of_stream)
15. [Each disc missing a track: pack not filled to sector boundary](#15-one-less-track-per-disc-pack-not-padded-to-sector-boundary)
16. [Song selection menu (AMG / ASVS) pitfalls (34 measures)](#16-pitfalls-of-the-selection-menu-amg--asvs)
17. [ISO overall md5 not reproducible](#17-iso-overall-md5-not-reproducible)
18. [Static images only refresh on the first track of each album](#18-static-images-are-refreshed-only-for-the-first-track-of-each-album)
19. [Patches are not idempotent; running in batch can generate duplicate code](#19-the-patch-is-not-idempotent-and-when-rerun-in-batches-duplicate-code-will-be-generated)
20. [Quick reference for diagnostic methods](#20-quick-reference-for-diagnostic-methods)
21. [ImageMagick `-repage` is **operator**](#21-imagemagick--repage-is-a-operator-not-using-parentheses-will-ruin-the-entire-page)
22. [`mogrify` command string missing space at the end → silently draws nothing](#22-the-mogrify-command-string-is-missing-a-space-at-the-end--silently-draws-nothing-returns-0)
23. [Subframe has only 4 palette items](#23-subpicture-only-has-4-palette-entries--only-one-color-can-be-used-per-layer)
24. [`mogrify` / `-draw` fails to draw: several known reasons](#24-mogrify---draw-cant-draw-several-reasons-already-encountered)
25. [ImageMagick's `-stroke` is "sticky"](#25-imagemagicks--stroke-is-stuck--all-the-text-after-it-is-outlined)
26. [`snprintf` format string does not match parameters → segmentation fault](#26-snprintf-format-string-does-not-match-the-arguments--segmentation-fault-and-nothing-can-be-seen-in-the-logs)
27. [Don't confuse ImageMagick's "settings" with "operators"](#27-dont-confuse-imagemagicks-settings-and-operators)
28. [`rgb(...)` must be quoted in the shell](#28-rgb-must-be-quoted-in-the-shell)
29. [`system()` return value cannot be judged only by `-1`](#29-the-return-value-of-system-cannot-be-judged-only-as--1)
30. [The real reason why static images do not display: ASVS record count must equal title count](#30-the-real-reason-the-static-image-does-not-display-the-number-of-asvs-records-must-equal-the-number-of-titles)
31. [`-extent` is **cropping** not **padding**](#31--extent-is-cropping-not-padding-20-of-the-cover-was-cropped)
32. [Static image encoding not passed `-q`/`-b` → clarity stuck at 30.6 dB](#32-when-static-image-encoding-does-not-transmit--q-b--the-clarity-is-stuck-at-the-default-value-of-306-db)
33. [Native Windows port (without WSL)](#33-native-windows-port-without-wsl)
    - 33.1 [Non-ASCII filenames in argv become `?`](#331-non-ascii-filenames-in-argv-become-)
    - 33.2 [`Could not open default file dvda-author.conf` is a false alarm](#332-could-not-open-default-file-dvda-authorconf-is-false-alarm)
    - 33.3 [ImageMagick 7 does not have a standalone `identify.exe`](#333-imagemagick-7-does-not-have-a-standalone-identifyexe)
    - 33.4 [Error handling covers up the real error](#334-error-handling-covered-up-the-real-error-itself)
    - 33.5 [Other Windows adaptation checklist](#335-other-windows-adaptation-checklist)
    - 33.6 [Three pitfalls of the build environment](#336-three-pitfalls-of-the-build-environment)
    - 33.7 [Installing ImageMagick in MSYS2: Output differs from Linux](#337-install-imagemagick-in-msys2-output-differs-from-linux)
    - 33.8 [Release Directory Layout](#338-release-directory-layout)
34. [The Chinese menu uses **Japanese character forms** + font override detection fails completely](#34-the-chinese-menu-uses-japanese-character-forms--font-override-detection-fails-completely)
    - 34.1 [Symptoms](#341-symptoms)
    - 34.2 [Root Cause 1: `.ttc` Collection + File Path = Obtaining JP Face](#342-root-cause-1-ttc-collection--file-path--get-jp-face)
    - 34.3 [Root Cause 2: Backslash font path silently swallowed](#343-root-cause-2-backslash-font-paths-are-silently-swallowed-by-imagemagick-when--font-is-given-a-backslash--path)
    - 34.4 [Additionally: Probe characters on Windows cannot go through argv](#344-additional-probe-characters-on-windows-cannot-go-through-argv)
    - 34.5 [★ Lesson: Probes without negative controls will lie](#345--lesson-probes-without-a-negative-control-will-lie)
    - 34.6 [What is currently packaged in](#346-what-is-packaged-now)
    - 34.7 [Also provide guardrails on the configuration side](#347-incidentally-guardrails-for-the-configuration-side)
35. [Distribute font faces by language (JP for Japanese, KR for Korean)](#35-assign-font-face-according-to-language-use-jp-for-japanese-kr-for-korean)
    - 35.1 [First measure "which faces are needed"](#351-first-measure-which-faces-are-needed-then-start-working)
    - 35.2 [Feasibility: Switching Fonts Within the Same Command](#352-feasibility-switch-fonts-within-the-same-command)
    - 35.3 [Implementation](#353-implementation)
    - 35.4 [Differential Verification](#354-differential-verification-the-hardest-step-in-this-section)
    - 35.5 [Bug in the Construction Script Fixed Along the Way](#355-also-fixed-the-script-bug-by-the-way)
    - 35.6 [Lessons Learned](#356-lesson)
    - 35.7 [Supplement: `--fontname-jp/-kr` Passed a Backslash Path](#357--additional-note---fontname-jp-kr-passed-backslash-path--silently-fallback-to-default-font)
36. [Windows Native Toolkit (Without WSL)](#36-windows-native-build-tools-without-wsl)
    - 36.1 [First Determine Where WSL is Really Required](#361-first-find-out-where-there-is-actually-a-dependency-on-wsl)
    - 36.2 [Toolkit Design (Two Principles)](#362-tool-package-design-two-principles)
    - 36.3 [Two Windows Details of build-all.bat](#363-two-windows-details-of-build-allbat)
    - 36.4 [False Negative #1: `lib` Prefixed Twice](#364-false-negative-one-lib-the-prefix-was-typed-twice)
    - 36.5 [False Negative #2: Double Backslash Treated as UNC Path](#365-false-negative-2-double-slashes-are-treated-as-a-unc-path)
    - 36.6 [False Negative #3: `cygpath -u` Compressed Multi-Level Path to Root](#366-false-negative-no-3-cygpath--u-compress-multi-level-path-into-root)
    - 36.7 [Bare `bash` is WSL — absolute paths are required](#367-bare-bash-is-wsl--you-must-use-an-absolute-path)
    - 36.8 [False positives in health check scripts are worse than having no check](#the-368-health-check-scripts-false-positives-are-worse-than-no-health-check)
    - 36.9 [Fonts: single face is the product, ttc is just a means](#369-font-single-face-is-the-product-ttc-is-just-the-means)
    - 36.10 [Silent absence in packaging and documentation](#3610-silent-missing-of-packaging-and-documentation)
    - 36.11 [How to determine if two builds are behaviorally equivalent or the code changed](#3611-how-to-determine-whether-two-builds-are-behaviorally-equivalent-or-the-code-changed)
    - 36.12 [Fork overhead differs by 100 times, and the measurement method itself has pitfalls](#3612-fork-overhead-differs-by-100-times-and-the-measurement-method-itself-has-pitfalls)
    - 36.13 [About configure cache: it was done and then removed](#3613-about-configure-caching-it-was-done-and-then-removed)
    - 36.14 [Distribution and usage](#3614-distribution-and-usage)
    - 36.15 [Lessons summary](#3615-summary-of-lessons)
37. [Windows end-to-end build from scratch](#37-building-windows-from-scratch-for-end-to-end-execution)
    - 37.1 [`to_wsl()` breaks Windows paths — the error points to the wrong location](#371--to_wsl-breaks-windows-paths--the-error-points-to-the-wrong-location)
    - 37.2 [Verification script depends on `xorriso` / `dd` — completely unusable on Windows](#372--validation-scripts-depend-on-xorriso--dd--completely-unusable-on-windows)
    - 37.3 [Change set expired: build does not report errors, but functionality is missing](#373--the-change-set-is-outdated-building-does-not-report-errors-but-functionality-is-missing)
    - 37.4 [ImageMagick: difference between modular build and self-contained build](#374--imagemagick-difference-between-modular-build-and-self-contained-build)
    - 37.5 [How to prove "WSL was truly not used at any step"](#375--how-to-prove-wsl-really-wasnt-used-at-all)
    - 37.6 [Overwriting traps during bidirectional sync (overwriting the document just written)](#376--the-overwrite-trap-during-two-way-synchronization-overwriting-the-document-just-written)
    - 37.7 [Intermittent unreachability of GitHub](#377-github-intermittently-unreachable)
    - 37.8 [Independent verification: do not use "building your own verification script" as the sole evidence](#378--independent-verification-dont-use-building-your-own-verification-script-as-the-only-evidence)
    - 37.9 [Semantic errors masked by my own changes: `blocks` is a sector, not a byte](#379--semantic-error-obscured-by-my-own-changes-blocks-are-sectors-not-bytes)
    - 37.10 [Audit criteria can become outdated as implementations evolve (Item D: per track vs per title)](#3710--audit-criteria-may-expire-as-implementation-evolves-item-d-per-track-vs-per-title)
    - 37.11 [Hidden dependencies in independent scripts: `identify`, `menu-bin` not in PATH, hardcoded `/tmp`](#3711--hidden-dependencies-of-independent-scripts-identify-menu-bin-not-in-path-hardcoded-tmp)
    - 37.12 [Measured Data (2026-09-27, Windows Full Process)](#3712-measured-data-2026-09-27-windows-full-process)
    - 37.13 [Summary of Lessons](#3713-lesson-summary)

---

## 1. Playback acceleration / progress bar cannot be dragged

### Symptoms

- The progress bar cannot be dragged when foobar2000 is playing
- Some sections sound **sped up**
- Time display abnormal

### Diagnosis

Directly parse the PES header timestamps of each sector in the AOB, and check if they advance with playback:

```bash
dotnet run --project src/DvdaMaker.Cli -- aob-pts /path/to/ATS_01_1.AOB
# In the release package: dvda aob-pts /path/to/ATS_01_1.AOB
```

**Output in case of an exception**:

```
File: ATS_01_1.AOB
Total sectors: 524288  Including PTS sectors: 524288
First PTS: 98  Last PTS: 98          ← First and last are the same!
Time span: 0.000 seconds
Step length: Minimum 0 Maximum 0 Negative steps 0 Zero steps 524287
Abnormal stride count: 524287   Proportion: 100.000%
```

Or spot-check sectors in certain locations:

```
Sector        0    1    2    3   10  100  1000  10000  100000  524287
PTS value     98   98   98   98   98   98    98     98      98      98
```

**Normal:**

```
First PTS: 98  Last PTS: 16084673
Time span: 178.718 seconds
Step size: Minimum 599 Maximum 4125 Negative steps 0 Zero steps 0
Abnormal step count: 0 Proportion: 0.000%
```

### Root cause

The PES header timestamps of the MLP disc come from the `info->pts[]` table, which is provided by `calc_PTS_DTS_MLP()`
Filled according to `mlp_layout[].nb_samples` (cumulative number of samples per sector).

In `decode_mlp_file()`, the layout accumulatively reads `frame->nb_samples`:

```c
cumbytes_written = decode(context, codec, codecpar, packet, frame, ...);
...
totnbsamples += frame->nb_samples;      // ← Always 0
```

The problem is that FFmpeg's `avcodec_receive_frame()` before returning `EAGAIN` / `EOF`,
**Will first call `av_frame_unref(frame)`** to clear the frame information. Meanwhile, `decode()` will keep looping internally.
It only returns after receiving EAGAIN, so reading `frame->nb_samples` outside the loop is bound to be 0.

Instrumentation verification:

```
[DIAG_ACC] #0     frame->nb_samples=0  totnbsamples(before)=0
[DIAG_ACC] #5000  frame->nb_samples=0  total number of samples (before)=0
[DIAG_ACC] #15000 frame->nb_samples=0  totnbsamples (before)=0
```

But `decode()` internally is clearly normal:

```
[DIAG] Received frame #1: nb_samples=40 format=2 ch=2     ← has value
```

Link consequences:

```
nb_samples always equals 0
  → info->numsamples = 0
  → info->PTS_length = 0
  → The pts[] calculated by calc_PTS_DTS_MLP() are all the same value
  → Each sector PTS is 98
  → Player cannot locate progress
```

### Repair

Immediately save the sample count **at the moment the frame is successfully received** (`docs/DVDA-AUTHOR-CHANGES.md`):

```c
static int g_last_nb_samples = 0;      // Newly added to the file header

ret = avcodec_receive_frame(context, frame);
if (ret == AVERROR(EAGAIN) || ret == AVERROR(EOF))
  break;

/* nb_samples must be taken before EAGAIN */
g_last_nb_samples = frame->nb_samples;
```

Layout accumulation switched to use this variable:

```c
totnbsamples += g_last_nb_samples;
```

> **Note**: The insertion point must be **outside** the `if/else` chain, otherwise the `else` branch will be isolated.
> `error: 'else' without a previous 'if'`。

### Before and after repair comparison

| Metric | Before Repair | After Repair |
|------|--------|--------|
| `PTS_length` | 0 | 16,084,725 |
| Sector PTS | Constant 98 | 98 → 16,084,673 Increment |
| Time Span | 0 seconds | 178.718 seconds (exactly as in the source) |
| Abnormal Step Ratio | 100% | 0.000% |

### Verification

```bash
# Single Track Verification
bash verify.sh timeline

# All AOB: Number of Sectors, Inter-track Continuity, PTS Integrity, Drop Point Reset
bash verify.sh audit
```

---

## 2. Errors when compiling `dvda-author`

Ordered as encountered. Running `build_dvda_author_mlp.sh` will automatically handle all.

### 2.1 SoX API Incompatibility

```
libsoxconvert.c:150: error: assignment to 'int *' from 'int'
libsoxconvert.c:152: error: request for member 'signal' in something
                        not a structure or union
```

The system SoX header files do not match the expected 14.4.2 version in the source code.

**Fix**: Disable SoX during compilation and add stub functions (`docs/DVDA-AUTHOR-CHANGES.md`).

```bash
CFLAGS="-DWITHOUT_sox"     # Must be placed in CFLAGS
```

> The Makefile of this project injects `$(if test ...)` into `-DWITHOUT_sox`. This method is bad,
> therefore `CPPFLAGS` has no effect, and **can only pass** through CFLAGS.

### 2.2 `close_handles` Naming Conflict

```
winport.c:327: error: conflicting types for 'close_handles';
               have 'void(int *, int *, int *, int *, int *)'
```

`winport.h` has a 4-parameter inline version with the same name as `winport.c`'s 5-parameter implementation.

**Fix**: Rename the inline version to `close_file_descriptors`, and add the correct declaration for Linux;
Change the implementation so that **passes parameters by value to** (the call site passes `FILE_DESCRIPTOR` which is `int` value).

### 2.3 FFmpeg Version Mismatch

```
mlp.c:86:  error: 'AVFrame' has no member named 'channels'
mlp.c:123: error: 'AVFrame' has no member named 'pkt_pos'
mlp.c:553: error: implicit declaration of function 'avcodec_close'
```

`mlp.c` is written according to the FFmpeg 4.x API, but it is linked to FFmpeg 8.

**Fix**: Pointing to the FFmpeg 4.2.4 included with the package (`local.ubuntu.20.10`) can resolve this error,
But **that can only support 16-bit**. To support 24-bit, you must migrate to FFmpeg 8, see the next section.

### 2.4 Key Points for Migrating the FFmpeg 8 API

| FFmpeg 4.x | FFmpeg 8.x |
|------------|------------|
| `ctx->channels` | `ctx->ch_layout.nb_channels` |
| `ctx->channel_layout` | `ctx->ch_layout` |
| `frame->pkt_pos / pkt_duration / pkt_size` | **Removed**, need to record manually |
| `av_parser_parse2()` | `av_read_frame()` |
| `avcodec_close()` | `avcodec_free_context()` |
| `AV_SAMPLE_FMT_S16 / S32` | `AV_SAMPLE_FMT_S16P / S32P`（planer） |

Two of them are **functional** issues, not just about renaming:

**① Alternative to `pkt_pos`**: The MLP sector layout depends on it. Change to manually record the input packet positions:

```c
static int64_t g_last_pkt_pos = 0;
...
g_last_pkt_pos = packet->pos;      // Save when reading the packet
```

**② planar format filling**: The original code writes to `frame->data[0]` according to packed format,
The planer format needs to be written plane by plane according to each channel:

```c
const int nch = c->ch_layout.nb_channels;
for (int ch = 0; ch < nch; ++ch)
  memset(frame->data[ch], 0, nframe * 4);          // s32p: 4 bytes per sample

for (int s = 0; s < nframe; ++s)
  for (int ch = 0; ch < nch; ++ch) {
    uint8_t *dst = (uint8_t *)frame->data[ch] + (size_t)s * 4 + 1;
    fread(dst, 1, 3, in_fp);                        // Store 24-bit into the high 3 bytes
  }
```

### 2.5 Link Error

```
cannot find /root/dvda-author-mlp8/local/lib/libswresample.a
```

The Makefile hardcodes the `local/lib/*.a` path.

**Fix**: Create `local/lib` and link system libraries; remove from the link line if the system does not have `libavfilter.so`.

```bash
for l in avcodec avformat avutil swresample; do
  ln -sfn /usr/lib/x86_64-linux-gnu/lib$l.so $SRC/local/lib/lib$l.a
done
```

> **Pitfall**: When cleaning object files, **do not use** `find -name '*.a' -delete` —— it will delete the files that were just created
> Delete the library symbolic links together. Only the `.o` files under `src/`, `libutils/`, and `libfixwav/` should be cleaned.

---

## 3. `stack smashing detected` (too many tracks)

### Symptoms

```
*** stack smashing detected ***: terminated
[FAIL] dvda-author failed to create disc 1
```

### Positioning

Testing different numbers of tracks using the binary search method:

```
n=5   OK      n=30  OK      n=60  OK
n=10  OK      n=40  OK      n=70  OK
n=20  OK      n=50  OK      n=77  Failed rc=134  Stack corruption=1
```

The threshold falls between 70 and 77.

### Root cause

`atsi2.c`：

```c
uint8_t atsi[2048 * 3];        // Fixed 3 sectors = 6144 bytes
int ntitletracks[99];
```

The ATSI table buffer fixes 3 sectors, while each track requires about 52 bytes → **about 70 tracks will overflow the stack buffer**.
Moreover, the downstream only supports `atsi_sectors` taking values 2 or 3, and cannot scale automatically.

### Repair (Two-Stage)

#### Phase 1: Limitations at the assembly line level (without modifying upstream code at the time)

Set `GROUP_TRACK_LIMIT = 64` in `02_build.py` (a maximum of 64 tracks per group, leaving a safety margin),
If it exceeds, split into another group according to the **album boundary** (the album itself is still not split). In actual tests, splitting 93 tracks into 3 groups caused zero stack corruption.

This solved the crash, but **the cost was grouping**: the tracks of one disc were split into multiple "audio groups".
For this version of the disc produced by SurCode, disc 1 is split into two groups of 66 and 25, and **the audio parameters of the two groups are exactly the same**
(both 48000/24) — in other words, the grouping **was not a format requirement**, purely a product of this upper limit.

#### Second phase: change the ATSI table to allocate dynamically according to the number of tracks (`patch_atsi_dynamic.py`)

First, measure the space used by each track: read out the `i` field from the ATSI constructed from production (`atsi[0x804] + 0x801`)

| Group | Number of tracks | `i` | Per track |
|---|---|---|---|
| Disc 1 Group 1 | 65 | 5696 | 56.1 |
| Disc 1 Group 2 | 10 | 2616 | 56.7 |
| Disc 1 Group 3 | 14 | 2840 | 56.5 |

Baseline 2049 bytes (ATSI_MAT etc. occupy 0x800) ⇒ **per track ≈ 56.5 bytes**, highly consistent across different track numbers.

So the fixed stack array was replaced with **a heap buffer allocated according to the number of tracks**:

```c
  /* 2049 baseline + 96 bytes/track (measured ~1.7 times the 56.5 capacity), rounded up to the next sector and leaving an extra sector */
  size_t atsi_cap = ((2049 + (size_t) ntracks * 96 + 2047) / 2048 + 1) * 2048;
  uint8_t *atsi = (uint8_t *) calloc(atsi_cap, 1);
  ...
  /* The number of sectors is calculated based on actual usage, no longer fixed as 2/3 two tiers */
  *atsi_sectors = (uint8_t) ((i + 2047) / 2048);
  if (*atsi_sectors < 2) *atsi_sectors = 2;
  ...
  FREE(atsi)
```

Track 99 (`MAX_TRACKS`) is also only allocated about 12 KB of heap memory. Testing shows that the number of sectors grows correctly with the number of tracks:

| number of tracks | `i` | sectors |
|---|---|---|
| 1 | 2112 | 2 |
| 10 | 2616 | 2 |
| 40 | 4296 | 3 |
| 70 | 5976 | 3 |
| 91 | 7152 | 4 |

Small combinations are still 2 sectors (not wasting disk), while large combinations grow on demand.

Therefore, `DVDA_GROUP_TRACK_LIMIT` can be placed into **99** (= `MAX_TRACKS` = the maximum number of tracks in a single group),
The 91 songs on disk 1 of this project are henceforth in one group **of**. This also brings two additional benefits:

1. **menu page count is only determined by the total number of tracks**, no longer affected by group divisions (`dim == 1`);
2. Defects related to cross-group issues (inconsistent pagination, `tracktext` insufficient entries, `cutloop` static variable leaks)
   **all disappear** — all the segmentation faults encountered in this round occurred on cross-group paths.

> **lesson**: when encountering 'upstream using fixed-size buffer truncates capacity', first measure **the actual usage per unit**
> (reading the length field it wrote itself is the most reliable), then decide how much to expand — this is much more stable than just picking a number to "loosen to N",
> and it can conveniently change the sector count from "fixed allocation" to "actual usage",
> small inputs are not wasted, large inputs do not overflow.

---

## 4. `--aob-extract` Segmentation Fault

### Symptoms

```bash
dvda-author-dev --aob-extract ATS_01_1.AOB -o out -W -P0 -n
# → Segmentation fault
```

But the files extracted by **are complete and correct**:

```
39831398  track_01_title_01.mlp     ← same size as the source MLP
efbf4eb0430047e1e6f5e46a0131acc3    ← MD5 exactly matches
```

### Explanation

This is known behavior in the final stages upstream, **it does not affect the correctness of data extraction, nor does it affect CD playback**.

The verification script ignores its return code accordingly and only compares the extraction result:

```bash
"$NEW" --aob-extract "$AOB" -o "$EXT" -W -P0 -n >/dev/null 2>&1 || true
cmp -s "$SRC_MLP" "$EXTRACTED_MLP" && echo "Matches ✔"
```

---

## 5. ffmpeg frame loss on decoding (Apple ALAC uncompressed frames missing END marker)

### Symptoms

- Pipeline reports "all successful," but **the audio sounds abnormal** (clicks/skips around 3 minutes)
- The same file plays perfectly fine on **foobar2000 / Apple player**
- ffmpeg reports errors but exit code is 0:

```bash
ffmpeg -v error -i x.m4a -f null -
# [alac] invalid element channel count
# [alac] Error submitting packet to decoder: Invalid data found
# echo $?  →  0        ← still returns success!
```

Frame loss (every 4096 samples):

| File | Error lines | Lost samples | Missing duration |
|------|----------|----------|----------|
| Japanese version | 6 | 12,288 | 256 ms |
| English version | 2 | 4,096 | 85 ms |
| Korean version | 14 | 31,208 | 650 ms |

### Key Criterion: Not source corruption

| Check | Result | Conclusion |
|------|------|------|
| Are packet positions continuously covered `mdat` | 2507/2507, 100.00% | Container index normal |
| Failed frames as a percentage of total frames | 3/2507 = **0.12%** | Not "format not supported" |
| `-ignore_editlist` / `-advanced_editlist 0` / `-threads 1` | Number of errors unchanged | Unrelated to edit list or threads |
| After clean remux | Number of errors unchanged | Not a container-level issue |
| Actual user playback | **Completely normal** | Source data is fine |

### Root Cause

Apple’s ALAC encoder periodically inserts "uncompressed frames" **(raw PCM, used for random access location),**
Characteristics:

- `is_compressed = 0` (the `0x02` bit in frame-header byte 3)
- `extra_bits = 0`
- Packet size = `4 + n_samples × channels × sample_size/8`

Observed to appear at `115.712s / 147.712s / 179.712s`, with intervals **exactly 32.000 seconds** (every 375 frames).

The bit count of such frames is exactly:

```
Frame header 23 bits + sampled data 4096 × 2 × 24 = 196608 bits → total 196631 bits
```

The packet has $24580 \times 8 = 196640$ bits, **has 9 bits left**, should write an END element (3 bits `111`) according to the specification.
But Apple writes `000`.

Therefore, ffmpeg reads it as an SCE element:

```c
element = get_bits(&alac->gb, 3);          // Read 000 = TYPE_SCE
channels = (element == TYPE_CPE) ? 2 : 1;  // → 1
if (ch + channels > alac->channels ||
    ff_alac_channel_layout_offsets[...] + channels > alac->channels) {
    av_log(avctx, AV_LOG_ERROR, "invalid element channel count\n");
    return AVERROR_INVALIDDATA;
}
```

The first loop does not report an error ($ch=0$, $0+1 \le 2$ passes), **In the second loop, $ch$ is already 1,
$1 + 1 > 2$ is when an error occurs** — this exactly explains why 'the number of errors corresponds one-to-one with the number of exception frames'.

> This is also why **players can play** normally: Apple’s CoreAudio does not require the END marker
> to locate frame boundaries (it segments according to packet length), while ffmpeg relies on the natural end of the decoder’s element loop.

### Diagnosis

**① Frame header bit analysis** —— Find all `is_compressed=0` frames and check their END mark:

```python
HDR_BITS = 3 + 4 + 12 + 1 + 2 + 1        # = 23
end_bit = HDR_BITS + n_samples * channels * sample_size
# Read 3 bits from end_bit, if != 0b111 then it is a problem frame
```

Actual output:

```
eb_raw distribution: 0=3, 1=2504
  #1356  115.712s  size=24580  byte2=0x03 hex=200003cc598a6a34
  #1731  147.712s  size=24580  byte2=0x02 hex=2000021460ea7236
  #2106  179.712s  size=24580  byte2=0x03 hex=2000039c728a081a
Adjacent normal packet:      byte2=0x04 hex=2000040404130809   ← is_compressed=0, extra_bits=8
```

Pay attention to normal adjacent packets `extra_bits=8` and abnormal packets `extra_bits=0`, combined with the packet header size
to confirm that it is an 'uncompressed frame' rather than corrupted data.

**② Packet size verification**

```
24580 = 4 (frame header) + 4096 × 2 × 3     ← exact byte count of a 24-bit uncompressed frame
16388 = 4 (frame header) + 4096 × 2 × 2 ← 16-bit version (Korean version)
```

The byte count **is exactly correct**, indicating that the data is complete and meaningful.

### Repair

Write the END marker (`111`) back to the correct position. **does not modify any sample data**, only changing the frame footer padding bits:

```
Original:  200003cc 598a6a34 ... [sample data] ... 000
Repaired:  200003cc 598a6a34 ... [sample data] ... 111
```

Current tool: C# CLI `alac` subcommand.

```bash
dotnet run --project src/DvdaMaker.Cli -- alac check x.m4a
dotnet run --project src/DvdaMaker.Cli -- alac repair x.m4a x.fixed.m4a
# In the release package: dvda alac check x.m4a
#           dvda alac repair x.m4a x.fixed.m4a
```

Repair effect (**sample count precisely matches**):

| File | Before Repair | After Repair | Container Declaration | Error |
|------|--------|--------|----------|------|
| Japanese version | 10,253,856 | 10,266,144 | 10,266,144 ✔ | 6 → 0 |
| English Version | 10,262,048 | 10,266,144 | 10,266,144 ✔ | 2 → 0 |
| Korean Version | 9,364,628 | 9,393,300 | 9,393,300 ✔ | 14 → 0 |

### Two-Level Losslessness Verification

**V1. Original PCM in package == ffmpeg decoded result**

The sample data of uncompressed frames is raw PCM, which can be directly extracted from the package and compared byte by byte with the ffmpeg repaired decoding output — proving that the data read after adding END is exactly the original data in the package.
Compare the decoded outputs byte for byte: this proves that adding END exposes exactly the data already present in the packet.

**V2. Removing the restored frames == Original decoded result**

From the repaired decoded PCM, remove the restored frames at known positions, it should be completely consistent with **the original decoded result** — proving that except for the restored frames, no other audio bytes were changed.

### Integration into the pipeline

C# `dvda prepare` will automatically attempt repair **when decoding verification fails**:

1. Decoding error detected → call `AlacEndRepairer` to locate fixable frames
2. Fixable frames available → repair to `$DVDA_ALAC_FIX_DIR` (default `/root/dvda-build/alacfix`)
3. **original file is never modified**, manifest points to the repaired copy
4. After repair, re-decode for verification, the number of samples must be exactly correct, otherwise it is still considered FAIL

will be written into `decode_report.txt` and recorded in the `repaired` /
`orig_src` field in the manifest for traceability.

### Lessons

> **'Playback is normal but transcoding reports an error' is a strong signal of a decoder problem.**
>
> It is wrong to determine that the source file is corrupted solely based on 'ffmpeg error + missing sample count'.
> The user feedback that truly overturned this conclusion was: **Playing the original file is fine**.
>
> To determine whether the source file is corrupted, cross-validation is required:
> 1. Is the container index complete (are the packet positions continuously covering the mdat)
> 2. Failure ratio (0.12% indicates a local characteristic, not 'unsupported format')
> 3. **Whether the same file plays normally on other players/decoders**
> 4. Whether the data is 'meaningfully complete' (e.g., uncompressed frame bytes match the theoretical value exactly)

---

## 6. The last track AOB is missing a few bytes

### Symptoms

The audit report counts 1 fewer sector than the track table:

```
Group 3: A. Sector count AOB = 334525, Track table = 334526, inconsistent ✗
```

Check file size:

```
685109244 bytes = 334525.998 sectors   ← not divisible by 2048; remainder 2044
```

### Explanation

`dvda-author` When writing the last pack of the last track, 4 bytes of padding were underwritten,
while IFO has declared full sectors (334526).

**Verify audio integrity** (extract the last track and compare with the source MLP):

```
51819200  track_14_title_14.mlp
Match ✔
```

**The audio data is complete**, only padding is missing.

### Fix

`02_build.py` Zero-pad AOB to the sector boundary before packaging:

```python
for aob in sorted(glob.glob(os.path.join(out, "AUDIO_TS", "*.AOB"))):
    size = os.path.getsize(aob)
    rem = size % 2048
    if rem:
        with open(aob, "ab") as fp:
            fp.write(b"\x00" * (2048 - rem))
```

---

## 7. Bit depth loss (produces illegal audio group)

### Symptoms

`dvda-author` In the output track list, the bit depth of a certain track is different from other tracks in the same group:

```
Attempt 1 (incorrect):
    1     01   48000   24   2 L-R     11612440
    1     02   48000   24   2 L-R     11588000
    1     03   48000   24   2 L-R     11636800
    1     04   48000   16   2 L-R     11544000   ← The whole group is 24-bit, but this track is 16-bit
    1     05   48000   24   2 L-R     11612440
```

Corresponding log:

```
Group   Title  Track  First_Sect   Last_Sect  First_PTS  PTS_length cga
    1   04/05     4       92380      111901          98    21645000   1
```

**The toolchain reported no errors**, ISO was generated normally, and the capacity is normal.

### Root cause

- The source file is 44.1kHz / **16-bit**
- Normalization decided to "resample to 48kHz / 24-bit"
- But `aresample` **only changes the sample rate, not the bit depth**
- The MLP encoder **keeps the input bit depth**, so this track was encoded as 16-bit

### Diagnosis

Have `dvda-author` report parameters for each track:

```bash
/root/dvda-author-mlp8/src/dvda-author-dev -g a.mlp b.mlp \
  -o out -D tmp -W -P0 -n 2>&1 | grep -E 'Found MLP audio|MTabLayout|Track'
```

Or directly use `ffprobe` (MLP does not record duration, but sample rate/bit depth can be read):

```bash
ffprobe -v error -select_streams a:0 \
  -show_entries stream=sample_rate,bits_per_raw_sample \
  -of default=nw=1:nk=1 x.mlp
# 48000
# 24
```

### Fix: explicitly specify `-sample_fmt`

The MLP encoder only accepts planer format `s16p` / `s32p`.
Note that **cannot be used with `s32`**, it will report:

```
[mlp @ ...] Specified sample format s32 is not supported by the mlp encoder
[mlp @ ...] Supported sample formats:
[mlp @ ...]   s16p
[mlp @ ...]   s32p
```

Correct usage:

```python
cmd += ["-sample_fmt", "s16p" if bits == 16 else "s32p",
        "-c:a", "mlp", "-strict", "-2", mlp]
```

### Verification: the specified bit depth before and after must be different

Using a 44.1k/16 source as an example (resampled to 48k/24), compare "specified bit depth" with "not specified":

```bash
SRC=x.flac

# A: Explicitly specify s32p
ffmpeg -i "$SRC" -af aresample=48000:resampler=soxr \
       -sample_fmt s32p -c:a mlp -strict -2 A.mlp

# E: Bit depth not specified
ffmpeg -i "$SRC" -af aresample=48000:resampler=soxr \
       -c:a mlp -strict -2 E.mlp

# Use dvda-author to report the actual bit depth
for f in A E; do
  echo -n "$f: "
  dvda-author-dev -g $f.mlp -o o -D t -W -P0 -n 2>&1 \
    | grep -m1 'Found MLP audio'
  rm -rf o t
done
```

Measured results:

```
A: Found MLP audio: 2 channels, 24 bits per sample, 48000 Hz   ✔
E: Found MLP audio: 2 channels, 16 bits per sample, 48000 Hz   ← wrong
A.mlp 61018364 B
E.mlp 39140014 B        ← size is half, because bit depth is half
```

It can also achieve the same effect using `aformat` (`aformat=sample_fmts=s32` or `s32p` both work):

```
A_sfmt_s32p        61018364 B
C_aformat_s32      61018364 B
D_aformat_s32p     61018364 B   All three outputs match byte by byte
```

### Reinforcement

After encoding with `02_build.py`, verify with `ffprobe`, and if inconsistent, delete the file and throw an error:

```python
got = ffprobe_mlp(mlp)           # (sample_rate, bits_per_raw_sample)
exp = (track["sr"], track["bits"])
if got != exp:
    os.remove(mlp)
    raise RuntimeError(f"MLP parameter mismatch: expected {exp}, actual {got}")
```

---

## 8. MLP does not record duration (risk of verification failure)

### Symptoms

MLP container **does not record duration**:

```bash
ffprobe -v error -show_entries format=duration -of default=nw=1 x.mlp
# duration=N/A
```

Therefore, the integrity cannot be verified by "reading back the output duration." Without finding another method, corrupted source files will
**pass silently** (losing 4096 samples each time).

A valuable corroboration: when the source file is corrupted, the encoded product will be **prematurely truncated** ——
For example, a corrupted M4A has an MLP of only 33,454,262 B, while normally it should be 53,084,192 B.
Truncation itself is also a signal, but it depends on "knowing the normal size in advance," which is not reliable enough.

### Fix: use astats for sample count

Read the sample count during the same decoding session (`-f null -` without writing to disk):

```bash
ffmpeg -hide_banner -v info -i x.flac -af astats=metadata=1 -f null - 2>&1 \
  | grep 'Number of samples'
# [Parsed_astats_0 @ ...] Number of samples: 10267032
```

Compare with 'source duration × target sample rate'. Actual measurement accuracy is extremely high (normal files differ by **+0** samples):

```
[PASS] Normal FLAC 48k/24        Expected 10267032 / Actual 10267032  Difference +0      Errors 0
[PASS] Normal FLAC 44.1k/24→48k  Expected 11636770 / Actual 11636770  Difference +0  Errors 0
[FAIL] Corrupted M4A (Korean version)  Expected 10224000 / Actual 10192792  Short 650ms  Errors 14
[FAIL] Corrupted M4A (English version)  Expected 10266144 / Actual 10262048  Short 85ms   Errors 2
```

**and must handle cases where "sample count cannot be obtained"** —— otherwise when verification fails, it will pass silently again:

```python
if samples is None:
    level = "FAIL"
    reasons.append("Failed to read decoded sample count (astats produced no output)")
```

### Side effects: Downstream scripts require another set of data sources.

`verify.sh` and `verify_pts_length.py` (deleted) require the **source audio duration per track,**,
but it cannot be read in MLP. For this, `02_build.py` outputs `mlp_index.json`:

```json
{
  "__meta__":    { "discs": 2, "tracks": 147, "dry_run": false },
  "__discs__":   [ { "disc": 1, "volid": "… 1",
                     "groups": [ { "group": 1, "aob": "ATS_01_1.AOB",
                                   "sr": 48000, "bits": 24,
                                   "tracks": [ { "mlp": "…/group_48000_24__0001__01. xxx.mlp",
                                                 "src": "…/01. xxx.flac" } ] } ] } ],
  "…/group_48000_24__0001__01. xxx.mlp": {
    "src": "/mnt/c/.../01. xxx.flac",
    "dur": 241.925667,
    "sr": 48000,
    "bits": 24,
    "resample_to": null,
    "title": "xxx"
  }
}
```

Each track item uses the **MLP path as the key**; `__meta__` / `__discs__` are metadata segments,
When traversing the index to count tracks, keys starting with `__` should be skipped.
(This is exactly how `verify_pts_length.py` (deleted) does it).

`__discs__` records the correspondence of 'Group N → `AUDIO_TS/ATS_01_N.AOB`',
This is the basis for `verify.sh` to locate 'Disc 1, Group 1, Track 1'; see section 11 for details.

The reference for comparison during verification is 'original audio + the same resampling filter chain':

```bash
ffmpeg -i "$src" -af "aresample=${rto}:resampler=soxr" -f s24le src.raw
ffmpeg -i "$mlp" -f s24le dec.raw
```

---

## 9. Inconsistent number of channels (preventive check)

All tracks within the same audio group in DVD-Audio must have the same number of channels. Mono and stereo **cannot be losslessly converted into each other**,
so this toolchain does not perform channel conversion; it directly checks and fails if inconsistent.

### Empirical observation: a whole batch of sources are stereo

```bash
ffprobe -v error -select_streams a:0 \
  -show_entries stream=channels,channel_layout \
  -of default=nw=1 src.flac
```

Statistics for 147 files:

```
  2 channels: 147
All are 2ch / stereo ✔
```

Parameter combinations (within the same batch, sample rate and bit depth can be mixed; channel count cannot):

| quantity | channels | layout | sample rate | bit depth |
|------|------|------|--------|------|
| 128 | 2 | stereo | 48000 | 24 |
| 13 | 2 | stereo | 44100 | 24 |
| 4 | 2 | stereo | 44100 | 16 |
| 1 | 2 | stereo | 48000 | 16 |
| 1 | 2 | stereo | 96000 | 24 |

`stereo` layout is FL (Front Left) + FR (Front Right), corresponding to `L-R` in the `dvda-author` report.
After normalization, there are only two audio groups: `48000/24` (131 tracks) and `44100/24` (16 tracks).

> Note: `ffprobe -of default=nw=1` will **remove the key names**, only output the values.
> When parsing, use `default=nw=1:nk=1` and take values line by line, or retain the key names.
> Otherwise, it will misjudge "label does not exist".

---

## 10. Audit false positives: Retrieved logs from the previous build.

### Symptoms

After re-spinning the disk, the audit reports "inconsistent sector count," and the PTS drop points do not fall on track boundaries:

```
Disk 1 Group 1  65 tracks
  A. Sector count AOB=1588763 Track table=1588632 Inconsistent ✗
  D. PTS drop points 64; falling on track boundary Abnormal ✗
    Drop points not on track boundaries: [834886, 861611, 888172, ...]
    Track start points that should drop but did not: [1442050, 1072775, ...]
```

**but AOB / IFO / ISO themselves are completely correct.**

### Root cause

`audit_disc.py` Previously logs were taken in a fixed order of "the first existing."

```python
LOG_CANDIDATES = [
    pathlib.Path(os.path.join(BUILD_DIR, "rebuild-final.log")),
    pathlib.Path(os.path.join(BUILD_DIR, "finalrebuild.log")),
    pathlib.Path(os.path.join(BUILD_DIR, "build.log")),
]
LOG = next((p for p in LOG_CANDIDATES if p and p.exists()), None)
```

And `build.sh` each run writes `build.log`, **old logs are not automatically deleted**.
Therefore: Compare the newly built AOB with the track table from the old build → inevitably inconsistent.

### Diagnosis

List the time of each log and the key values within:

```
Log                     Time              disc1 group1 max last_sector
rebuild-final.log        09-18 18:57        1588631  -> +1 = 1588632   ← Misused
finalrebuild.log         09-18 18:27        1588631  -> +1 = 1588632
build.log (this build)    09-19 08:36        1588762  -> +1 = 1588763   ✔
```

Actual total sectors in AOB = 1,588,763 —— consistent with the track table in **this** build.log.

Verify with the correct log:

```
A. Number of sectors AOB=1588763 Track table=1588763  Matches ✔
D. PTS drop points 64; fall on track boundaries All hit ✔
```

### Fix

Change to take the latest **mtime** candidate from the candidate list, and print the used log path and time:

```python
_existing = [p for p in _LOGS if p and p.exists()]
LOG = max(_existing, key=lambda p: p.stat().st_mtime) if _existing else None
if LOG is not None:
    print(f"Build log: {LOG}  (mtime {time.strftime(...)})")
```

And give a warning if another log with a close time (< 60 seconds) is found:

```
⚠ There is 1 other log with a similar timestamp; confirm that the selected log belongs to this build
```

`verify_pts_length.py` (deleted) and `verify.sh` are fixed similarly, the latter also supports
`DVDA_BUILD_LOG` environment variable explicitly specified.

> Lesson from **to**: Whenever you 'compare A and B', make sure A and B come from the same **build**.
> Using a fixed file name sequence to guess which is the current product will inevitably lead to errors when there are historical remnants.
>
> Another more hidden variant: `--dry-run` **truncates and overwrites** the log of the actual output.
> See [Section 12](#12-auditverify-false-positives-dry-run-overwrote-the-build-log).

---

## 11. Verification script group matching error (ATS_01_1.AOB is group 1)

### Symptoms

`verify.sh lossless` report:

```
[2] Finished ISO internal audio track verification failed ✗
```

### Root cause

Check 2 takes the first MLP in sorted order from `mlp_index.json` and compares it with
The audio tracks extracted from `ATS_01_1.AOB` are compared. But the two are **not necessarily from the same group**:

- `ATS_01_1.AOB` stores **Group 1** (the group that was passed in with `-g` first)
- `mlp_index.json` is sorted by path, the first one might be `group_44100_24__0001__*`
  (Belongs to Group 2), so using Group 2's MLP to compare with Group 1's AOB → bound to fail

### Repair

Parse the `dvda-author` command line for disc 1 from the build log and select the MLP for **group 1, track 1**:

```python
for line in t.splitlines():
    if not line.startswith("+ ") or " -g " not in line:
        continue
    if "/disc1 " not in line and "/disc1\t" not in line:
        continue
    seg = line.split(" -g ", 1)[1].split(" -o ", 1)[0]
    ...
```

> **Note**: MLP filenames contain spaces, **do not use `split()` to separate**.
> Must retrieve each one according to the MLP directory prefix ending with `.mlp` (consistent with `verify_pts_length.py` (deleted)):
>
> ```python
> for chunk in seg.split(pfx)[1:]:
>     e = chunk.find(".mlp")
>     if e >= 0:
>         names.append(pfx + chunk[:e + 4])
> ```

After revision:

```
[1] MLP decoded PCM matches the source audio byte by byte ✔
[2] Audio tracks in the finished ISO match the source MLP ✔
```

### Subsequent reinforcement: switch to disc-split plan for location, if unavailable, report an error

The above relies only on the build log for location, still has two gaps:

- Once the log is overwritten or rotated by `--dry-run`, the `-g` command line can no longer be parsed
- Old code in this situation rolls back **to `sorted(keys)[0]`** — but `group_44100_*`
  is sorted before `group_48000_*` alphabetically, so it may take the wrong group's MLP,
  judging an ISO that is completely correct for ****as failed (same pit, different entry)

Now changed to three-level processing,** will no longer guess **:

1. First, check the `dvda-author` command line in the build log
2. Then check `mlp_index.json`'s `__discs__` disc-split plan
   (`02_build.py` directly writes out "Group N → `ATS_01_N.AOB`" and each track's source MLP,
   does not depend on whether the log still exists)
3. If neither route is available → print `[FAIL]` and the reason, return 1

```
Basis for location: mlp_index.json disc-split plan (Disc 1 Group 1 Track 1)
```

---

## 12. Audit/verify false positives: dry-run overwrote the build log

### Symptoms

After running `bash build.sh --dry-run` once (everything normal), running `bash verify.sh` reports an error:

```
=================== CD Consistency Audit ===================
[Skipped] Track table not parsed in build log

=================== MLP Lossless Verification ===================
[2] Finished ISO internal audio track check failed ✗

=================== Issues exist, see above ✗ ===================
```

But the ISO itself is correct — manual comparison can confirm:

```bash
got=$(dvda-author --aob-extract a.AOB -o ext -W -P0 -n 2>/dev/null; \
      find ext -name 'track_01_title_01.mlp')
cmp "$got" "$BUILD_DIR/mlp/group_48000_24__0001__01. xxx.mlp"   # Byte-by-byte match
```

### Root cause

`--dry-run` dvda-author is not executed, so the log it writes **doesn’t contain a track table at all**
(`First_Sect` / `Last_Sect` / `PTS_length`). And the log name continued to use `build.log`,
so the previously properly outputted track table **was truncated and overwritten**.

Before migration, there were two write points that would truncate:

| Location | Behavior |
|------|------|
| Old `build.sh` | prepare output wrote logs without `-a` using `tee`, causing truncation |
| Old Python builder | opened the log in append mode, but the previous log had already been truncated |

Audit requires a track table to compare AOB sector numbers; non-destructive verification without a `__discs__` plan
Under the old index, it degenerates to `sorted()[0]`, so both checks are falsely reported at the same time.

### Repair

The current C# implementation uses a stricter isolation method:

1. Dry-run logs are written to `build-dryrun.log` and do not overwrite the official `build.log`
2. dry-run index write to `mlp_index-dryrun.json`, with `dry_run=true`
3. The official index is first written to `mlp_index.pending.json`
4. Only after all ISOs are successfully built will the ISO collection and the official index be released together as a rollbackable transaction.
5. Auditing and non-destructive verification refuse to treat dry-run indexes as the basis for official products

Verification (fingerprint remains unchanged before and after dry-run):

```bash
md5sum "$DVDA_BUILD_DIR/build.log" > /tmp/before.md5
bash build.sh --dry-run
md5sum -c /tmp/before.md5        # build.log: OK
```

> Lesson: **The same log shared by two running modes will interfere with each other.**
> Having different modes write to different files is much more reliable than guessing on the reading side 'whether this log is usable'.

---

## 13. The case sensitivity of tag key names causes albums to be split and normalized silently skipped

### Symptoms

After converting a batch of Apple Music m4a files to FLAC and ripping them again, an exception appeared in the log:

```
A total of 9 tracks need to be resampled          ← Previously it was 10 tracks
group_44100_16: 1 track     ← An extra lonely group
```

The song `Lulala! Lululala! (韩文版)` (44100/16), which was originally supposed to be normalized to 48000/24
No longer appears in the resampling list, but forms its own group. The album it belongs to has 5 songs, the remaining 4 are all
48000/24 should be normalized according to the 'majority sample rate'.

**On the disk, there will appear an additional audio group with only 1 track, and the sample rate/bit depth of that track is different from other tracks in the same album.**

### Cause

The tag reading of `01_prepare.py` strictly matches the original case of **:**

```python
d["album"] = tags.get("album", "")
```

However, tagging habits vary depending on the source:

| Source | Key Name |
|------|------|
| MP4 / m4a (Apple) | lowercase `album` / `title` / `date` |
| FLAC (according to Vorbis convention, most tagging tools) | uppercase `ALBUM` / `TITLE` / `DATE` |

Therefore, in albums with the same name:

```
01. Chinese version.flac    album = 'Lulala! Lululala!(...) - EP'    ← original FLAC, lowercase key
02. Japanese version.flac    album = ''                                ← converted, uppercase ALBUM
03. English version.flac    album = ''
04. Korean version.flac    album = ''
05. Instrumental  album = 'Lulala! Lululala!(...) - EP'      ← original FLAC, lowercase key
```

The album key changes to `''` along with the real name, and the group is split into `{01, 05}` and `{02, 03, 04}`;
In the latter, the one with 44100/16 cannot be normalized within the group like the other two, forming an independent group.

> **specification is based on**: Vorbis comment field names **are case-insensitive** (
> [vorbis-spec-ref](https://xiph.org/vorbis/doc/v-comment.html)：
> "the field name ... is case-insensitive"）。
> Both MP4 and FLAC are valid, **the reading side has a bug**.

### Diagnosis

Directly print the tags of the 5 files:

```bash
for f in *.flac; do echo "--- $f"; \
  metaflac --list --block-type=VORBIS_COMMENT "$f" | grep -i 'album'; done
```

You will see two key names: uppercase and lowercase. Using ffprobe to cross-verify is more direct:

```bash
ffprobe -v error -show_format -of json "$f" | python3 -c \
  "import json,sys; print(json.load(sys.stdin)['format']['tags'])"
```

ffprobe will **preserve the key name case for FLAC**, so the difference can be observed.

### Fix

Convert all to lowercase when reading:

```python
tags[k.lower()] = v
...
d["date"] = tags.get("date") or tags.get("releasetime") or ""
d["title"] = tags.get("title", os.path.splitext(os.path.basename(path))[0])
d["album"] = tags.get("album", "")
```

Rerun step 1 after fixing: the sample count restores from 9 to 10, and extra groups disappear.

> **Lesson**: Don’t focus only on audio parameters when adding a new type of audio source.
> Metadata is also an interface, and **case sensitivity, normalization, encoding** can all be inconsistent.
> The audio stream this time is completely correct (lossless, parameters are correct), the only issue is the string key names.

---

## 14. ffmpeg's MLP encoder does not write END_OF_STREAM

### Symptoms

Using MLP encoded by ffmpeg for disc burning, **ffmpeg itself can decode normally**, but cannot confirm in
Works properly on hardware DVD-Audio players — compared byte by byte with the reference implementation SurCode MLP output
After comparison, it was found that the end of the stream is missing a marker.

### Root cause

The MLP specification requires writing `END_OF_STREAM` (`0xD234D234`) at the end of the stream. ffmpeg's encoder:

```c
if (ctx->last_frames == 0 && ctx->shorten_by) {
    put_bits32(&pb, END_OF_STREAM);
}
```

`shorten_by = frame_size - frame->nb_samples`. And `mlp` encoder **does not declare**
`AV_CODEC_CAP_SMALL_LAST_FRAME` (has `truehd`):

```
$ ffmpeg -h encoder=mlp     → dr1 delay exp            ← no small
$ ffmpeg -h encoder=truehd  → dr1 delay small exp      ← has small
```

Therefore, ffmpeg's common layer always pads the last frame to a complete `frame_size` (40 samples),
`shorten_by` is always 0, the branch writing the end marker **is never entered**.
This is inevitable, **cannot bypass** via command-line parameters.

### Troubleshooting

Determine using an access unit parser that **should not use substring search for**:

```python
# ✗ Can misjudge: the compressed data may occasionally contain these 4 bytes d234d234
EOS in data

# ✔ Search only at the end of substream 1's payload in the final AU
```

In 147 tested files, 1 file happened to contain this byte sequence in the compressed data,
which would be misjudged as "end marker already exists" when using substring search.

### Fix

`mlp_align.py` performs pure byte patching (see the header comments of the file for details). Key points:

1. Insert the 4 bytes `d2 34 d2 34` at the **end** of substream 1's payload in the final AU
2. Recalculate the `end` of the `substream header` (+2 words)
3. Recalculate the `length` of the `access unit header` (+2 words) **and its 4-bit parity**
4. Recalculate the `parity` and `checksum` of this substream

Step 3 is easy to miss — the 4-digit parity of the AU head is
The folding result of `XOR(input_timing, length_words, all substream-header bytes)` should change along with the length if the length changes.

### Verification methods

To modify a binary stream with a checksum, the prerequisite is to **independently verify the changes**.
`mlp_align.py` replicates ffmpeg's `av_crc`:

```python
crc_2D = build_table(poly=0x002D, bits=16, le=0)   # After building the table, each value bswap32
crc_63 = create_table(poly=0x0063, bits=8, le=0)

checksum16 = av_crc(crc_2D, 0, buf[:n-2]) ^ AV_RL16(buf[n-2:])
checksum8  = av_crc(crc_63, 0x3c, buf[:n-1]) ^ buf[n-1]
```

Criterion: **Whether the existing checksum in the file can be recalculated**. Based on actual outputs from ffmpeg and SurCode
All verification passed (147 files, over 210,000 access units' parity/checksum all matched).

After repair, it should meet:

```
[1] Alignment self-check passed (checksum/parity/substream/end marker)
[2] 'End of stream indicated.' occurs during ffmpeg decoding.
[3] Decoded PCM is byte-for-byte identical to before repair (audio not altered).
[4] Byte-for-byte identical to SurCode's major sync.
```

### Additional findings

With the same track and same parameters, the two encoders' major syncs (28 bytes) differ in only 3 places:

| Offset | Field | SurCode | ffmpeg |
|------|------|---------|--------|
| `[14:16]` | `peak_bitrate` | 3200 | 3199 |
| `[16]` | `extended_substream_info` | 1 | 0 |
| `[26:28]` | `checksum16` | — | — |

`peak_bitrate` difference comes from ffmpeg using floor rounding `((peak<<4)-8)/rate`,
while the decoding formula is `(raw*rate+8)>>4` — writing 3199 at 48000 Hz will reverse-calculate to 9597000.
Changing to ceiling rounding makes the round-trip precise.

> Lesson from ****: "software decoder can play" and "compliant with specification" are two different things.
> The project's software side almost entirely uses libavcodec's `mlp` decoders, same source as encoders,
> making self-consistency easy to satisfy; to judge specification compliance, you must find an independent **implementation** as a reference
> (here it is SurCode) and compare field by field.

---

## 15. One less track per disc: pack not padded to sector boundary

### Symptom

Burned discs in foo_input_dvda (foobar2000's DVD-Audio plugin) **one less track per disc**:

| Disk | the IFO statement | foobar actually shows |
|---|---|---|
| External MLP Edition Disc 1 | 91 | 90 |
| ffmpeg Edition Disc 1 | 89 | 88 |
| Two Plates 2 | Normal | Normal |

**and all routine checks pass**:

- In IFO, the sum of `nr_of_titles` and the `tracks` of each title is correct
- Sum of all titles in  IF `len_in_pts` O == Total source duration (538.1 points, exact same)
- Track sector table is correct: first rail starts from 0, ends are connected, no overlap or voids, last rail last sector == AOB total sector − 1
- In AOB, the PES header is parsed sector by sector, and the number of PTS drop points is correct (65 + 24 + 55)
- Track duration matches the source file with 147/147 for full consistency

In other words: nothing is missing on the **plate, but one of the songs cannot be seen at the reading end**

### Diagnostic pathway

The key is to first confirm whether the missing song is the same song, or if the two versions lack different tracks. The answer determines the direction:

```bash
# 1. Is there a record of a failed completion in the build log (the fastest step)
grep -c 'pes_padding length must be higher' <BUILD_DIR>/build.log

# 2. Find sections where "multiple consecutive sectors do not start with the pack header."
#    Normally, each sector starts with 00 00 01 BA
```

The tracks that are missing in the two versions are **different** (track 48 in the ffmpeg version, track 16 in the external version), so it's not any single song
It's not a problem with the audio content, but rather related to the calculation of track boundaries.

### Root cause

`ats.c` of `write_pes_padding()` at `length` For **1~6**, only print one error message `return`，
**Don't write a single byte**:

```c
if (length > 6)
  {
    length -= 6; // We have 6 bytes of PES header.
    ...
  }
else
  {
    foutput("%s
", ERR "pes_padding length must be higher than 6;");
    return;      // ← write nothing
  }
```

The caller (the last packet of the MLP) sends the 'number of bytes needed to pad to the 2048 boundary,' and when this value falls between 1 and 6:

1. The last pack of this track is 1~6 bytes short → the file is no longer aligned to 2048
2. **The pack header of the next track thus falls into the middle of the sector** (tested offset 2042 / 2043)
3. `get_ps1()` of `foo_input_dvda` requires the sector to start with `00 00 01 BA` in order to retrieve the stream id;
   Cannot retrieve → `get_audio_stream_info()` returns false → **This track is completely discarded**
4. The end of the track was successfully filled, and the difference was made up → **Misalignment self-corrects after just one track**

So the phenomenon is that 'each disc is missing exactly one song,' and the missing one is always **the song immediately following the short intro**.

Empirical (ffmpeg version):

```
Sector 1175803 last 32 bytes: ... d2 34 d2 34 e9 3d 00 00 01 ba 44 00
                                            ↑ EOS  ↑ 2 bytes ↑ pack header (outside the sector)
Sector 1175804 starting 32 bytes: 04 00 04 01 01 89 c3 f8 00 00 01 bb ...
```

`d2 34 d2 34` is the `END_OF_STREAM` we appended, after which there should have been '2 bytes + pack header' stored
Within the 2048 boundary, but the pack header was pushed to offset 2042 of the next sector.

> By the way: the original criterion `length > 6` also **considered `length == 6` as wrong**.
> And 6 bytes happen to be exactly one empty PES padding packet (3-byte start code + 1-byte stream id +
> 2-byte length), could have been written normally. This off-by-one caused the trigger window to be twice as large as necessary.

### Repair

`docs/DVDA-AUTHOR-CHANGES.md` (already referenced in step [5/7] of `build_dvda_author_mlp.sh`):

- `length < 6` → pad with zeros to the boundary (handled the same way as the `length == 0` branch)
- Remove `length == 6` → normally write PES padding packet
- Change `ff_buf` to `length + 1` to avoid zero-length arrays when `length == 0`

Patch precisely matches `write_pes_padding` (whose error message **does not** have leading spaces and includes `maxverbose`)
That line), `read_pes_padding` (which has two leading spaces) will not be mistakenly modified.

After modification, **dvda-author** needs to be rebuilt:

```bash
bash build_dvda_author_mlp.sh
```

### Verify

```bash
# 1. In the build log, this error should always occur 0 times
grep -c 'pes_padding length must be higher' <BUILD_DIR>/build.log

# 2. Directly read the few sectors that were originally damaged (no need to decode the entire disk)
xorriso -indev <ISO> -find /AUDIO_TS -name ATS_01_3.AOB -exec report_lba
#   outputs in the following format: File data lba:  0 , <LBA> , <Blocks> , <Size> , '<path>'
#   Note that it is comma-separated, do not use \s+ regex to split
dd if=<ISO> bs=2048 skip=$((LBA + rel - 1)) count=3 status=none | xxd | head
#   The end of the previous sector should be '... 00 00 01 ba', and the beginning of this sector should be '00 00 01 ba'

# 3. Quick structure check (will check the first sector track by track, about 3 seconds)
bash verify.sh quick
```

Test results: In the build logs of both versions, this error occurred 0 times; the two boundaries that were previously broken
(ffmpeg disk 1 sector 1175804, external version disk 1 sector 355313) now both start with a pack header.

### Lesson

> **audio data being entirely correct does not mean the container structure is correct.**
>
> Under this defect, every check in the 'content layer' is green — duration correct, track count correct, per-track sector table correct,
> is also correct. The only anomaly is the **byte alignment**: a certain pack crossed the sector boundary.
>
> So the audit added a check for F (the first sector of each track must start with a pack header). It is the only thing that can block this type.
> Checking the problem, which had been missing before—the defect thus avoided many rounds of full verification.
>
> Additionally: **must be verified using an independent disc reading implementation of**. Here, foobar2000 on the user's machine is used
> `foo_input_dvda` plugin — its source code (and its behavior) has nothing to do with our writing side at all,
> So the external information 'it shows 90 tracks' is actually the starting point for discovering the problem.
>
> (Its tracklist is cached by path, after rebuilding the ISO you need to remove the album in foobar and re-add it,
> otherwise it might display the results from the previous build — this also needs to be ruled out first.)

## 16. Pitfalls of the selection menu (AMG / ASVS)

A series of problems encountered when adding 'Track Selection Menu + Play Cover' to the disc. Some of the dvda-author menu functions are above.
**Basically only tested on the scale of 1 group + single page + several dozen tracks**, this project is 3 groups, 91 tracks,
27 covers, almost every scale assumption will be broken.

The fix all falls to `docs/DVDA-AUTHOR-CHANGES.md` (the selection menu section) and `scripts/menu_assets.py`.

### 16.1 Pagination formula error: the total number of buttons on all pages is always ≤ 32

`menu.c` at two places (`menu_characteristics_coherence_test` and `generate_menu_pics`):

```c
img->maxbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) / img->nmenus;
img->resbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) % img->nmenus;
```

`maxbuttons` should be '**capacity per page**', but the original formula first truncates to 32 and then divides by the number of pages —
so no matter how large `--nmenus` is set, the total number of buttons across all pages in **is at most 32**.
On a disc with 91 tracks, only the first 32 can be selected, the rest cannot, **and no error is reported**.

Comparison: the hierarchical branching in `xml.c` is written as `Min(..., ntracks[groupcount])` (number of tracks in a group),
so it is evident that the original intent was indeed 'capacity per page', the hierarchical branch was not written incorrectly.

**judgment basis**: count the total number of `<button>` in `spu_xmltemp_*.xml`.
should be ≥ total number of tracks (any extra are the page-turning arrows).

**fix**: `patch_menu_paging.py` changed to
`maxbuttons = Min(32, ceil(totntracks / nmenus))`，`resbuttons = 0`。

### 16.2 The page number guardrail also shrinks the non-hierarchical menu

There is also a section in the same function (originally only valid for hierarchical menus):

```c
if ((img->ncolumns) * ngroups < img->nmenus - 1) img->nmenus = ngroups * ncolumns + 1;
```

The number of pages for non-hierarchical menus is only constrained by the total number of buttons. Applying this formula will result in `--nmenus` provided by the user.
silently suppressed (measured 3 times: 8 → 4). **plus `img->hierarchical &&` limits**.

### 16.3 The background image on every page is the same (`--background` is essentially redundant)

`-b/--background` accepts comma-separated **one background jpg per page**, parsing is correct, but the options end
is fixed with `backgroundpic[0]`:

```c
copy_file2dir_rename(img->backgroundpic[0], tempdir, "bgpic0.jpg", ...);
for (u = 1; u < img->nmenus; u++)
    copy_file2dir_rename(img->backgroundpic[0], tempdir, "bgpic<u>.jpg", ...);  // Again [0]
```

**verification method**: Compare page by page with the source image's md5 after running `tmp/discN/bgpic<u>.jpg`.

### 16.4 `--blankscreen` overwrite in reverse `--background`

Option ending: As long as `--blankscreen` is given, convert this png to jpg **and write it in
`backgroundpic[0]`** (write before `unlink`). So
`--background /tmp/bg0.jpg`, `/tmp/bg0.jpg` was rewritten on the spot
'jpg exported from blankscreen' (tested to be identical byte by byte to the bundled `menu/black_PAL_720x576.jpg`),
has been deleted.

The semantics of the two do not conflict: `--blankscreen` is the **text overlay background image** (`prepare_overlay_img()`
Copy it as `svpic.png` and then write on it), `--background` is **the background video image for each page**.
Use `cli_background_list` to mark and remember 'the user provided a list'; if provided, do not overwrite it.

### 16.5 `--blankscreen` must be fully transparent, and `mogrify` cannot draw ASCII

- `--blankscreen` png is the text overlay background image: **fully transparent** to allow the background video to show through
  (`xc:none`, do not add `-alpha set -depth 8 PNG32:` — that will turn it into a superimposed state).
- ⚠️ **There is no font on this machine that can display both Chinese and English**:

| Font | ASCII | Chinese characters | Kana | Korean | CJK punctuation |
|---|---|---|---|---|---|
| `fonts-droid-fallback` (system default) | **✗** | ✓ | ✓ | **✗** | ✓ |
| `fonts-wqy-microhei` | ✓ | ✓ | ✓ | **✗** | ✓ |
| `fonts-noto-cjk` | ✓ | ✓ | ✓ | ✓ | ✓ |

  Ubuntu's `fonts-droid-fallback` is a **CJK-only minimal version**:
  `fc-list ':charset=0041'` can't find it — not even `A`.
  And the title of this project simultaneously contains **Chinese + Japanese kana + Korean**, missing any one of them and the song is considered blank.

  **Criteria must be assessed using functional testing**, not by checking the font table:

  ```bash
  # Really render a small piece to see if there is any ink (>0 counts as drawable)
  convert -size 160x48 xc:none -font "$F" -pointsize 20 -fill white \
          -annotate +2+32 "Ag" -format "%[fx:mean.a]" info:
  ```

  Check the character set of `fc-query` to avoid being fooled —— Droid's font info says it covers Basic Latin,
  but in reality, not a single pixel shows, and ImageMagick **doesn't report an error**.

- ⚠️ Spaces in the name of **`-font` will break the command**: `menu.c` is composed of
  `snprintf(..., "-font", img->textfont, ...)`, **without quotes**.
  Passing `"Droid Sans Fallback"` will be split into two parameters by the shell →
  `mogrify: no decode delegate for this image format 'Sans'` → text will not be rendered at all.
  You must use the ImageMagick font name (replace spaces with hyphens): `Droid-Sans-Fallback`.
- ⚠️ Unknown font names in **will be silently replaced**: ImageMagick will not report an error when it can't find the name, and will use the default font,
  so the "Latin drawable" detection result comes from another font, while Chinese characters still can't be rendered.
  Therefore, you must first `convert -list font` confirm the name exists, then perform functionality detection.
  （`menu_assets.font_exists()` + `font_coverage()`）。

### Font size 16.6 is too large → the menu is directly missing, while dvda-author returns 0

`compute_pointsize(img, 10, img->maxbuttons, globals)` counts the font size up to the upper limit of 35,
and with 11 lines per page, the line spacing is only about 34 px (`y()`'s `labelheight + 12`) →
text overlaps with the underline rectangle → `spumux` reports

```
spumux: src/subgen-image.c:901: imgfix: Assertion `useimg' failed.
```

**does not produce `AUDIO_TS.VOB`**, but dvda-author still exits with 0.

Field test for whether each font size can produce VOB (40 tracks, 4 pages):

| `--fontsize` | Menu VOB |
|---|---|
| Automatic (calculated as 35) | **None** |
| 12 / 18 / 22 / 25 | Yes |

**Correct algorithm** (`menu_assets.compute_fontsize`):

```
Number of lines R = Min(32, ceil(total number of tracks / number of pages))
labelheight = (576 - 56 - 40 - (R+4)*12) / (R+4)      # integer division
Line spacing S = labelheight + 12
Font size P = min(S - 10, 35), and >= 7                       # underline is 4~6px below baseline
```

**Must explicitly check after building `AUDIO_TS.VOB` whether it actually produces** —— this is the only way to verify.

### 16.7 `groups with more than 34 tracks` → `***stack smashing detected***`

`xml.c` of `compute_coordinates()`:

```c
uint16_t y0[MAX_BUTTON_NUMBER], y1[MAX_BUTTON_NUMBER];   // 36
for (j = 1; j < command->maxntracks + 2; ++j) { y1[j] = ...; y0[j] = ...; }
```

And `command->maxntracks` is **`MAX(track, command->maxntracks)`** ——
**Total tracks of the group**, not number of lines per page (`amg2.c:165`). If the group has ≥ 35 tracks, it will overflow the stack.
Group 1 of disc 1 has 65 tracks, inevitably affected; when there is no menu, this code path is not executed, so it was never exposed.

**fix**: `patch_menu_layout.py` Replace all **in `menu.c` / `xml.c` with**
`command->maxntracks` with `img->maxbuttons` (= number of rows per page).
· Write `img->maxbuttons` in `menu.c` (these functions all have `img` parameters)
· The `compute_coordinates()` in `xml.c` has only `command`, need to write `command->img->maxbuttons`
  (`amg2.c:82` has `#define img command->img`, the two places refer to the same object)

### 16.8 `--screentext` crashes as soon as it’s used, and the track name is truncated to 3 bytes

Three defects combined cause `-O/--screentext` to **any input segfault**:

1. `basemotif = fn_strtok(chain, '=', ..., count=1, cutloop, remainder)`
   — `count` specifies how many substrings `cutloop` consumes; with 1 passed in, **substring 1
   just return 0 and break**; meanwhile `fn_strtok` at break `array[k]` is set to sentinel NULL,
   `remainder` = the k-th substring. So `albumtext = basemotif[0]` becomes **NULL**.
   Passing **2** is correct.
2. `dim` is the number of **slots** output by `fn_strtok` (number of elements + 1 sentinel),
   but traversal is written as `for (k = 0; k < dim; k++)` → last time read to
   `grouparray[dim-1] == NULL` → `strlen(NULL)` segfault.
   The upper limit should use `arraylength()`.
3. `size` is reused: it is passed as column width to `&size` as an output parameter, and later used as **string truncation length**:

   ```c
   size = (norm_x - 40 - 20*(ncolumns-1)) / ncolumns;      // for example 213
   basemotif = fn_strtok(..., &size, ...);                  // ← overwritten to 3
   ...
   if (strlen(tracktext[g][t]) > size) tracktext[g][t][size] = '\0';   // truncated to 3 bytes
   ```

   Thus **each track name is truncated to 3 bytes** (for Chinese, only 1 character remains). A separate variable `ncut` should be used.

Additionally `remainder` / `rem` are uninitialized VLAs, and `fn_strtok` is only written when breaking early
(if the number of substrings is exactly exhausted, **does not write**) → reading uninitialized stack memory.

**fix**: `patch_menu_screentext.py`.

### 16.9 Pure menu background (without `--stillpics`) will cause `background_movie_N.mpg` to be missing

When `--blankscreen` is fully transparent and `--background` is not provided, the background frames are completely black,
`mpeg2enc` cannot encode a valid MPEG-2 → subsequent errors reported

```
[ERR] freopen (stdin)
img->backgroundmpg[0]=.../background_movie_0.mpg errno=2: No such file or directory
```

**work around**: provide each page with an opaque background image (this project uses a cover collage, darkened by 70%).

### 16.10 `--stillpics` file list mode inevitably fails cd (exit code 255)

`: ` Separate tracks, `,` separate multiple images of the track. But **the file list branch does not set `stillpicdir`**,
And `create_stillpic_directory()` immediately calls `change_directory(stillpicdir)` when it starts;
The default value of `stillpicdir` is set in `main()` **before command-line parsing**
(At that time, `-D/--tempdir` was not yet effective), so it is the **compile-time default**
`<cwd>/.dvda-author/temp` — usually does not exist:

```
[ERR]  Impossible to cd to /tmp/menu-test/.dvda-author/temp.
[ERR]: No such file or directory          → Exit code 255
```

**Fix**: `patch_menu_stillpics_list.py` points `stillpicdir` to the current tempdir
(At the same time, also make the 'destination of copied images' consistent with the 'path of the images being read').

### 16.11 Segmentation fault occurs after `pict` is set to NULL and sprintf is called again

`generate_background_mpg()` ends with `FREE(pict)` (= `free(pict); pict = NULL;`),
In `create_mpg()`, `pict` is a **file-level** static, while the allocation criterion `s` is a **function-level** static.
(No reset):

```c
if (s == 0) { s = MAX(...); pict = calloc(s, sizeof(char *)); }
sprintf(pict, "%s/pic_%03u.jpg", ...);      // ← pict == NULL on the second entry
```

This path of 'first make the menu background, then make the static image background' will inevitably trigger it.

**Fix**: `patch_menu_stillpics.py`, add `|| (pict == NULL)` to the condition.

### 16.12 Pagination must exactly replicate dvda-author's allocation (otherwise background and buttons will be misaligned)

**This is a pitfall I introduced myself and then fixed**, worth noting separately.

dvda-author **decides which tracks go on which page**:
`R = Min(32, ceil(total_tracks / page_count))` Fill R rows per page, continuous within groups, no page crosses a group.
(`ncolumns=1` Ensure it does not cross groups `while ((group < img->ncolumns) && ...)`).
Our task is just to match the "background and text on each page" **to its** allocation.

Initially, to "keep albums complete on a page," I preemptively broke pages at album boundaries ****, therefore:

1. The actual number of pages increased from 9 to 10;
2. I passed 10 to `--nmenus=10`;
3. dvda-author's `R` then became `ceil(91/10) = 10` (originally counted as 11 pages for 9 pages);
4. I formatted it as '11 lines + album boundaries,' but it formatted as '10 lines, ignoring album boundaries'.
   → **Two sets of pagination are misaligned, so backgrounds and titles do not match the buttons**.

**Criterion**: after solving for (page count N, R), use the same formula in reverse
`sum_g ceil(n_g / R)`, it must **exactly equal** N:

```
surcode Disk 1 [66,25] → 9 pages x 11 (6+3 = 9) ✔
surcode disc 2 [56]     → 5 pages x 12   (5 = 5) ✔
ffmpeg  Disk 1  [65,10,14]  → 14 pages x 7   (10+2+2 = 14) ✔
ffmpeg  Disk 2  [56,2]      → 8 pages x 8    (7+1 = 8)  ✔
```

**fixes**: `menu_assets.group_pages()` only accepted when `p == pages`,
`build_menu()` removes album boundary page breaks, `02_build.py` runs this set of self-checks each build.

> **lesson**: as long as the upstream tool **also executes**, the decision we want to simulate,
> then its algorithm **must be copied word for word in** and add self-check —-
> Missing one line divisor results in a disc that "looks normal but contents are misaligned."

### 16.13 After opening the menu, one empty `VIDEO_TS` appears in the ISO root directory

At the end of the menu, `dvdauthor -o <outdir> -x <xml>` must be run once to write virtual machine commands,
And `dvdauthor` is the **DVD-Video tool**, which by convention creates an empty one under the `-o` directory
`VIDEO_TS`. It has nothing to do with dvda-author's `-n/--no-videozone` **** (that only handles
whether dvda-author itself needs to create it).

Practical test: after adding `--topmenu` with the same set of parameters, the ISO root directory changes from "only `AUDIO_TS`"
Become '`AUDIO_TS` + `VIDEO_TS`'.

**is retained in this project** (user explicitly requested). Along the way, one counterintuitive observation:
After deleting `VIDEO_TS`, dvda-author actually adds an extra line.
`[ERR] Directory '<...>/VIDEO_TS'` —— it keeps track of the sector accounting for this directory,
Keeping it actually makes the log cleaner.

### 16.14 `--aob-extract` reporting `Aborted` is known behavior

See Section 4, not related to the menu; it will also appear on the menu plate.

### 16.15 Static map capacity: 1024 sectors/disk, and only 'reuse the same map' can save space

in `asvs.c`

```c
totpicsectors += img->stillpicvobsize[index + j];
if (totpicsectors > 1024) foutput(ERR "Exceeding stillpic buffer limit (2 MB) ...");
```

`totpicsectors` is initialized **outside the loop** → it is a **cumulative total for the entire disk**, with a maximum of 1024 sectors ≈ 2 MB.

And `generate_background_mpg()` for STILLPICS is **encoded once per track and then `cat_file`
Splicing**, so each image separately takes up the quota, **even if the same image is given repeatedly, it still takes up the quota**.
You must use **null item reuse** to save it:

```
--stillpics A.jpg::C.jpg     ← Track 2 continues to use A, occupying only 2 parts
```

Actual measurement (3 tracks): `A,B,C` → 75 sectors; `A::C` (1 shared disk) → 52 sectors.

Budget conversion: approximately **15 sectors per sheet** → maximum approximately **46 sheets per disk**.
· This project disc 1 has 27~28 albums ✔ (if one track per album, then 89 albums ✗ exceeds limit)

**Note**: The number of `--stillpics` items must **exactly match the total number of tracks**, otherwise an error will be reported
`You forgot at least one track on --stillpics` and exit 255.

### 16.16 `[WAR] Coherence test for ISO start sector failed` is an existing phenomenon upstream

```
[WAR]  Coherence test for ISO start sector failed: start sector assessed as: 287 but should be: 289
```

It is just `startsector == ntotalfiles + 272` account reconciliation (`launch_manager.c:518`).
Actual measurement shows that **production build without menu is also 287 vs 289** (difference of 2 sectors), unrelated to the menu;
The verification script also does not scan `[ERR]`/`[WAR]`, which does not affect the determination.

(If the empty `VIDEO_TS` is deleted, `ntotalfiles` is 1 less → becomes `287 vs 288`, still a difference of 1.)

### 16.17 When the number of pages increases, segmentation fault occurs in a "far away" location (AMG buffer does not grow with the number of pages)

**symptoms**: MLP audit, writing AOB, menu background encoding, `mogrify`, `spumux`, `dvdauthor`
**all completed normally**, the last sentence of the log is `mplex`'s `MUX STATUS: no under-runs detected.`,
then dvda-author segmentation fault (`exit=139`), **has no `[ERR]`**.

`dmesg` / gdb shows crash in `amg2.c`'s `create_amg()` `uint32_copy()`:

```
uint32_copy (buf=0x8000000016d4 <Cannot access memory>, x=1081472) at c_utils.h:423
#1  create_amg (...) at amg2.c:1357
#2  launch_manager (...) at launch_manager.c:472
```

Root cause of **** is VLA out-of-bounds stack write:

```c
// launch_manager.c —— SIZE_AMG is a constant 3, so it is always 4 sectors
sectors.amg = SIZE_AMG + (globals->text ? 8 : 0) + (globals->topmenu <= TS_VOB_TYPE);

// amg2.c —— buffer size is determined by it
uint8_t amg[sectors->amg * 2048];        // 4 * 2048 = 8192 bytes
```

And the menu table (`menusector` branch, starting from `amg[0x1820]`) **grows with the number of pages**:

```
Required = 0x1820 + 8 * (nmenus - 1) + nmenus * 0x13A
```

| Pages | Required | Buffer | Result |
|---|---|---|---|
| 1 | ~6490 | 8192 | ✔ |
| 4 | ~7456 | 8192 | ✔ ← 40-track test disk has this number of pages, so **was not exposed at that time** |
| **9** | **~9066** | 8192 | **✗ Overflow about 900 bytes** |

The overflowed write is to the **stack** (VLA), so the crash point is far from the real cause —— all previous stages had run
before it crashes, and the stack has been corrupted (in gdb `create_amg` input parameters are all garbage values).

**fix**: `patch_menu_amg_size.py` —— After `sectors.amg` initialization, adjust
`img->nmenus` enough:

```c
  if (globals->topmenu <= TS_VOB_TYPE && img->nmenus > 1)
    {
      uint32_t need = 0x1820 + 8 * (img->nmenus - 1) + img->nmenus * 0x13A;
      uint32_t need_sectors = (need + 2047) / 2048;
      if (need_sectors > sectors.amg) sectors.amg = need_sectors;
    }
```

`nmenus` during the command-line parsing phase (`menu_characteristics_coherence_test`) has been finalized,
so the value read here is the final one. `sectors.amg` also determines `sizeofamg = sizeof(amg)`
the write disk length, and the sector pointers everywhere (`2*sectors->amg + ...`, `sectors->amg - 1`,
`menusector * sectors->amg`), will automatically adjust —— after expanding
`AUDIO_TS.IFO` changed from 4 sectors to 5 sectors, **self-consistent**.

**Criterion**: The number of bytes in `AUDIO_TS.IFO` = `sectors.amg * 2048`;
When opening a 9-page menu, it should be 10240 (5 sectors). If it is still 8192, it means the patch has not taken effect.

> **Lesson**: The crash point caused by out-of-bounds writing in a VLA can be far from the cause (after running the entire process for more than ten minutes)
> Only when it crashes), and it **will destroy the parameters at the crash site**, causing gdb to display a bunch of garbage values.
> When encountering 'all stages succeed, but a mysterious segmentation fault at the end,' suspect this first
> **Automatically sized array (VLA) determined by an external variable**.

### 16.18 `fn_strtok("")` writes out of bounds on the stack, corrupting the caller's `globals`

**This is the most hidden one**: crash point (the `change_directory` in `create_stillpic_directory`)
Separated by several layers from the cause (a zero-length VLA of `fn_strtok`), and **entirely dependent on the stack layout** —
Under the same binary gdb, it doesn't crash, runs directly and crashes; with the same set of parameters on a different disk, it only 'occasionally' crashes.

**Symptoms**: dvda-author places all products (AOB / `AUDIO_TS.VOB` / `AUDIO_SV.VOB` /
The segmentation fault occurs **only after all `AUDIO_TS.IFO`** files have been generated; the log only contains the start banner line.
(stdout is fully buffered, all output before the crash was lost), there was no `[ERR]` at all.

**positioning techniques** (this set of skills is worth reusing):

```bash
# 1. Enable core dump (core_pattern=core, but ulimit -c is 0 by default)
python3 -c "import resource; resource.setrlimit(resource.RLIMIT_CORE, (-1,-1))
import subprocess; subprocess.run([...])"

# 2. Analyze core — completely does not change memory layout, so reproducible (under gdb it might not crash)
gdb -batch -ex bt ./dvda-author-dev ./core.12345
```

The backtrace in core points directly to:

```
#0 create_stillpic_directory (string="", count=-1, globals=0x7ffd00000006) ← globals is garbage
gdb> info line
Line 1363 of src/menu.c:  change_directory(globals->settings.stillpicdir, globals);
```

The true value of `globals` is `0x7ffdd1a7ebf0`, but it turned into `0x7ffd00000006` ——
Typical high-byte preserved, low-byte overwritten **stack corruption characteristic**.

The root cause of **** lies in `auxiliary.c`’s `fn_strtok()`:

```c
  char *s = strdup(chain);          // chain == "" → only allocates 1 byte
  uint32_t j = 1, k = 0;
  int32_t cut[strlen(s) / 2];       // strlen("")/2 == 0 → VLA of length 0
  cut[0] = -1;                      // ← stack out-of-bounds write
  do  if (s[j] == delim) { cut[++k] = j; }
  while (s[j++] != '\0');           // ← scanning from s[1], out-of-bounds for empty string
  cut[k + 1] = j - 1;               // ← another out-of-bounds
```

Two flaws:

1. **Empty string**:`strdup("")` Only 1 byte, but the scan starts from `s[1]` Start —— Cross the allocation boundary,
   Will read all the way to the first one in the heap `\0` Just stopped;`cut` Another 0-length VLA,
   `cut[0] = -1` and `cut[k+1]` are both **out-of-bounds stack writes**.
2. **`cut` capacity insufficient**: Number of entries = Number of delimiters + 2. The original formula starts with `strlen(s)/2`,
   And `",,,"`(Each character is a separator) 5 items are needed, but only 1 item is given.

**When will it reach an empty string**: `--stillpics` uses **an empty item to indicate using the previous image**
（`--stillpics A.jpg::C.jpg`During parsing, each empty item is called `fn_strtok("", ',', ...)`。
In other words, the standard practice of 'storing only one cover per album,' which saves ASVS budget, will inevitably be triggered.
Actual test shows that Disk 2 of this project (56 tracks / 39 empty sectors) has a stable segment error, while Disk 1 (63 empty sectors)
It was just **luck** that it didn't crash — both share the same broken stack layout, it's just that the corrupted positions are different.

**Fix**: `patch_fix_fn_strtok.py`

```c
  size_t slen = strlen(s);
  int32_t cut[slen + 2];            // In the worst case, every character is a delimiter
  cut[0] = -1;
  uint32_t j = 1, k = 0;
  if (slen > 0)                     // Skip scanning for empty string
    do  if (s[j] == delim) { cut[++k] = j; }
    while (s[j++] != '\0');
  cut[k + 1] = j - 1;
```

After the fix, the `returncode` of disk 2 changed from `-11` to `0`, and all outputs are complete.

> **Lesson**:
> 1. **'All products exist, only collapse at the end' = look for out-of-bounds writing, do not look for logical errors**.
>    All the files have been written, indicating that the main process is fine; the crash occurs during the cleanup phase or subsequent steps—
>    In other words, **some previous out-of-bounds write just happened to hit the critical variable**.
> 2. **Does not crash under gdb + crashes when run directly → use core dump**.
>    gdb will change the memory layout (by default it also disables ASLR), `set disable-randomization off`
>    It may not necessarily be reproducible; core dump does not affect the layout, which is the correct solution for this kind of bug.
> 3. **Zero-length VLA (`int32_t x[strlen(s)/2]` + empty string) is a classic landmine**.
>    For any 'VLA declared based on input length', you should ask the question, 'What if the length is 0?'
> 4. **Don't take "just because a certain disk didn't crash" as meaning there's no problem**. Disk 1 and Disk 2 follow the same defective path.
>    The code path, just one hit the key variable, the other didn’t.

### 16.19 `tracktext[group]` insufficient entries → `strlen(NULL)` segmentation fault

When `generate_menu_pics()` in `menu.c` draws text, it uses the **`ntracks[group]`** index:

```c
do {
      maxtracklength = MAX(maxtracklength, strlen(tracktext[group][track]));
      ...
      track++;
} while (track < ntracks[group]);
```

The `tracktext[group]` is generated by `fn_strtok(rem, ',', ...)`, and the number of entries depends on
The group listed several song titles in `--screentext`. **When the number of items is less than `ntracks[group]`,
The last slot is `fn_strtok` written as NULL sentinel → `strlen(NULL)` segmentation fault.**

⚠️ **is completely unrelated to text content, only related to the number of items**, so "the song titles all look correct" could also crash.

Two causes:
1. `--screentext` gave too few groups (number of groups defined < number of audio groups);
2. **missed something between groups `:`** — the format is
   `AlbumTitle=Group1Title=Track1,Track2:Group2Title=Track3,Track4`.
   When the colon is missing, the whole string is parsed as only 1 group: that group's "group title" is assigned the list of track names,
   while the actual track text ends up after the next `=`, so the number of items is only about half of the original.

**Fix**: after parsing, `patch_menu_screentext.py` **pads each group's track list to `ntracks[k]` entries**, using empty strings for missing entries. Undefined groups receive empty group titles:

```c
      for (k = 0; k < dim; k++)
        {
          if (!grouptext[k]) { grouptext[k] = calloc(2, ...); grouptext[k][0] = strdup(""); }
          int need = (k < (int) ngroups) ? (int) ntracks[k] : 0;
          int have = tracktext[k] ? arraylength(tracktext[k]) : 0;
          if (have >= need) continue;
          tracktext[k] = realloc(tracktext[k], (need + 1) * sizeof(char *));
          for (int i = have; i < need; ++i) tracktext[k][i] = strdup("");
          tracktext[k][need] = NULL;
        }
```

**criterion**: under gdb `p tracktext[group][track]` — if it's `(char *) 0x0` then it's this pit.

### 16.21 Page-turning arrows: text from page 2 onwards is misaligned to the top, the last page repeats 5 times

**symptom** (user report): "Menu doesn't show Previous / Next after turning pages".

**actual test location** (56 tracks, 5 pages, extract the ink line ranges of menu text images page by page):

```
Page 0  ink ... 408-430, 441-455      ← 441-455 is the bottom Next    ✔
Page 1  Ink … 378-400, 408-429      ← Missing 441-455! The arrow runs to lines 1 and 2
Page 4  Ink ... 288-309, 441-455      ← Bottom is Previous          ✔
```

In other words, **from page 2 through the second-to-last page (page count minus 1)**, arrow labels overlap the first two track names,
And the bottom arrow slot has no text — it looks like 'no Previous/Next after turning the page'.

**Root cause**: `menu.c` passed the `offset` into `mogrify_img()` when drawing the arrow:

```c
mogrify_img(arrowstring, img->ncolumns - 1, img->maxbuttons,
            img, img->maxbuttons, command1, command2, offset, img->arrowcolor);
                                /* track absolute line number */  /* offset passed in */
```

And inside `mogrify_img()` it is

```c
y0 = EVEN(y(track + 1 - offset, maxnumtracks + 4));
```

`offset` means **the global index of track 1 on this page**. It indexes track names (so that track 1 on each page
All are drawn on line 1). But the arrow uses a **fixed absolute line number** `img->maxbuttons` /
`img->maxbuttons + 1` (the two arrow slots at the bottom of the page, originating from the same source as the button coordinates output by `xml.c`),
**Should no longer reduce offset**:

| Page | `offset` | `y(track+1-offset)` = `y(13-offset)` | Result |
|---|---|---|---|
| Page 1 | 0 (initial value) | `y(13)` | Bottom ✔ |
| From page 2 | 12, 24, … | `y(1)`, `y(-11)` … | **Top row 1, 2** ✗ |
| Last Page | 0 (reset when the entire group is completed) | `y(13)` | Bottom ✔ |

**Side Effects (Same Block of Code)**: Originally written as
`do { ... } while (buttons < menubuttons + arrowbuttons);`。
`buttons` before entering the loop is **the number of buttons already drawn on this page**, while `menubuttons` is still the "full page capacity":
The tracks match perfectly when they fill the space, but **the last page has only 8 songs** (56 - 4×12)
`buttons = 8`, target `12 + 1 = 13` → loop **5 rounds**, keep the same Previous
Repeat drawing at the same position. The isomorphic loop in `xml.c` will output 5 **completely overlapping** buttons:

```
Page 4   button09..button13 all y0=440..470    ← 5 overlapping buttons
```

**Fix**: `patch_menu_arrows.py` (also modify `menu.c` and `xml.c`, both must be consistent —
A text drawing, an output button

- Arrow calls always pass `offset = 0` (positioning using absolute line numbers)
- Remove `do-while` and change to a single check, and add `arrows_drawn`/`arrows_emitted`
  Debug comparison with `arrowbuttons` (alert on inconsistency)
- Conveniently remove `char arrowstring[9]` + `strcpy`
  (`DEFAULT_PREVIOUS` is 8 characters + NUL = 9, exactly filling this buffer, originally right at the boundary),
  Directly pass the literal to `mogrify_img()`

**fixed, actual test on**:

```
Page 0: 13 buttons, arrow area 441-455
Page 1: 14 buttons, arrow area 441-455, 471-485 ← the bottom two rows now align
Page 2: 14 buttons, arrow area 441-455, 471-485
Page 3: 14 buttons, arrow area 441-455, 471-485
Page 4: 9 buttons, arrow area 441-455 ← reduced from 13 to 9 (removed 5 overlapping ones)
```

Last page `button09`'s `y1=470`, same 'Previous' position as previous pages ✔

> Lesson from **to**: Mixing 'relative line numbers' and 'absolute line numbers' in the same code segment is a common cause of this kind of misalignment.
> `mogrify_img(track, offset)` This signature set requires the caller to specify which type it is —
> **Song titles use relative (with offset), arrows use absolute (pass 0 as offset)**.
> Method for judgment: Compare the coordinates of 'drawing text' and 'outputting buttons' by **page by page**,
> Then check the actual text ink line range by frame-by-frame review; only looking at menu screenshots can easily make one think 'it's just not drawn'.
>
> Furthermore: `do { ... } while (buttons < target)` This kind of 'loop based on cumulative count' implementation,
> Once the radix of the `buttons` entry differs from the `target` base (one contains the number already drawn on this page,
> the other is the full page capacity), the last page will rotate a few extra times — changing to a single check is more stable.

### In 16.22, the ASCII comma in the song title splits one tag into two.

**Symptoms** (user report): `03. 繁星、新生,与你.flac` In the menu, it is split into **two buttons**.

**Root cause**: `--screentext` Uses `=`, `:`, `,` three characters for layering, but **has no escape mechanism**
(`fn_strtok` is simply split by the delimiter). The song title `繁星、新生,与你` contains a **ASCII comma**
(the previous `、` is an enumeration comma U+3001, not a delimiter), so this tag was split into two:

```
…, Fuxing, Xinsheng, Yu Ni, …      ← expected 1 tag
…, Fuxing, Xinsheng, Yu Ni, …      ← actually 2 tags
```

The extra one causes **all subsequent tags to shift by one position**, leaving no space for the final real tag
(`tracktext[group]`’s count will only be supplemented, not truncated, so the extra one replaced the last one).
It manifests as “a song split into two buttons,” with no errors reported on either side.

In actual tests, this project has **4** song titles containing ASCII commas:

```
Fuxing, Xinsheng, Yu Ni
Shiyue Zhi Jia, Yingxu Zhi Yuan (feat. VISION SOUND & Thena A)
Shackles of Oath, Vow of Promise (feat. Thena A & VISION SOUND) [Instrumental Version]
Rushing Flow, Never Ceasing Because of You (Original Soundtrack of the Game 'Naraka: Bladepoint')
```

**Positioning Method**: Break `--screentext` into groups according to `:`, then break into tracks according to `,`,
Count the number of tracks in each group — if it doesn’t match `ntracks[]`, it indicates a delimiter conflict:

```
Segment 0 track count = 95 ← but this group only has 91 tracks
```

**Fix**: `menu_assets.sanitize()` — replace these three characters with **visually equivalent safe characters**

| Character | Chinese context | Pure ASCII context |
|---|---|---|
| `,` | `，` (full-width comma) | `·` |
| `:` | `：` | `·` |
| `=` | `＝` | `·` |

Titles containing Chinese characters use the full-width form (reads almost the same), pure ASCII titles use the interpunct.
Determined titles are recorded into `MenuPlan.sanitized` for checking the differences between displayed text and source tags.

After fixing, the track count of segment 0 goes from 95 back to **91** ✔

> **Lesson**: Any string protocol that "concatenates with delimiters without escaping" must be checked
> **if the data contains delimiters** — especially when data comes from user tags (commas, colons, equals signs in song/album names are very common).
> The way to check is not "by reading the code",
> but **by parsing it back and counting whether the number of items matches the expectation**.

### 16.23 Previous Pressed but can't go back: the number of jump commands does not match the number of button positions

**symptoms** (user reported): A few pages cannot go back to the previous page when pressing Previous.

**root cause**: The arrow has **two separate outputs** in dvda-author, both must be with `arrowbuttons`
The quantities match, and in the first round I only fixed one of them:

| output | file/function | content |
|---|---|---|
| Button **jump command** | `xml.c` of `generate_amgm_xml()` | `jump menu N;` (handed over to dvdauthor) |
| button **position rectangle** | `xml.c` of `generate_spumux_xml()` | `<button x0=… y0=…/>` (hand over to spumux) |

The `do { ... } while (buttons < menubuttons + arrowbuttons);` **in the two locations are the same defect**:
`buttons` before entering the loop is 'the number of buttons drawn on this page', while `menubuttons` is 'the full page capacity',
has fewer tracks, the former is much smaller than the latter → Turn several more rounds, repeating the same arrow.

Actual measurement last page (Track 91 Page 8):

```
amgm (jump): 13 buttons = 7 tracks + 6 repeated Previous (button08..button13)
spumux (position): 8 buttons = 7 tracks + 1 Previous (button08)
```

**13 vs 8 Inconsistent** → dvdauthor Assign both by number, with extra buttons 09. 13
**there is no highlighted area** → The Previous selection on this page cannot be selected or the previous page cannot be selected.

**Fix**:`patch_menu_arrows.py` Now processing **three** (previously only two were processed):

1. `menu.c` arrow **text** —— pass 0 to `offset` (absolute line number) + remove do-while
2. `xml.c` Arrow **position** (spumux) — Remove the do-while
3. `xml.c` Arrow **Jump** (AMGM) — Remove the do-while ← **add** this round

**Measured** after repair (page by page comparison with serial number):

```
page AMGM button spumux button Consistent arrow jump
0       13        13          ✔      [Next → menu 2]
1       14        14          ✔      [Next → menu 3, Previous → menu 1]
…       …         …           ✔      …
7       8         8           ✔      [Previous → menu 7]
```

**and this invariant is made into a constructed self-check** (`check_menu_buttons()` of `02_build.py`):
After running dvda-author, immediately parse two XML files and compare the button number sequence page by page,
If inconsistent, the drive is declared a failure and page numbers and quantities are printed. Self-check: Test data from old builds has been verified and can be caught
(Old page 5: 13 vs 9 → FAIL).

> **lesson**: When the same "logic" is output to **multiple product** (here text/position/jump are three parts),
> Each item must be checked one by one; you can't assume it's fixed just because one part is fixed — 'in the right place' doesn't mean 'jump is correct.'
> The most effective method is to turn 'the number/ID on both sides must match' into **a machine-testable invariant**,
> instead of relying on the human eye to look at menu screenshots. This is also the only way to detect this defect in advance in this round.

---

### 16.24 Previous does not return to the previous page: the menu cell end address uses the wrong size

**Symptom** (user report): on several pages, pressing Previous does not go back to the previous page — **more than one page**,
and **the track selection button works perfectly**.

The asymmetry 'only pagination is broken, track selection is not' is a clue:

| Button | jump instruction | is parsed by |
|---|---|---|
| Track selection | `jump group G track K` | through **ATSI** locates the audio area (`atsi2.c`) |
| Pagination | `jump menu N` | is looked up by the player in the **AMG IFO menu PGC table** (`amg2.c` **handwritten**) |

So the issue is with `amg2.c` the handwritten menu table, and has nothing to do with the previously fixed button geometry/numbering.

**Root cause**: each menu cell end address is calculated like this:

```c
if (j > 1)
  menuvobsize_sum += img->menuvobsize[j - 2] - 1;      // sum the sizes of the 'previous' pages
uint32_copy(&amg[i], menuvobsize_sum
                     + img->menuvobsize[img->nmenus - 1] - 1 - 1);
                     /* ↑ Always the size of the last page ** **, not the current page */
```

When `nmenus == 1`, it happens to be the current page → correct (so it was never noticed);
When `nmenus > 1`, the end addresses of pages 1..n-1 are all wrong.

**Moreover, the error is hidden by the similar sizes of each page's VOB**. Actually tested, the 8-page VOB has
37/40/37/43/39/39/41/39 sectors, which calculates as:

```
Correct: [35, 74, 110, 152, 190, 228, 268, 306]
Actual: [37, 73, 112, 148, 190, 228, 266, 306]
                              ^^^  ^^^            ^^^
                              Coincidentally the same
```

⇒ **pages 5, 6, 8 are exactly correct, the other 5 pages are wrong** — perfectly matches the case of “more than one page has issues.”
This also explains why it seems totally irregular.

**criterion (directly verified in the final IFO)**:

```python
# AUDIO_TS.IFO menu table: starts at 0x1820, each entry 0x13A bytes
# Correct values are found in the table, old values are also found — both exist, indicating old code
```

Actually tested old product: the sequence appearing in IFO is exactly the old sequence `37/73/112/148/190/228/266/306`,
while the correct values `35/74/110/152/268` **do not appear at all**.

**fix**: `patch_menu_amg_cells.py` — replace `menuvobsize[img->nmenus - 1]`
with `menuvobsize[j - 1]` (current page). The corrected formula is equivalent to
'Number of valid sectors accumulated up to the current page − 1':

```
Expectation = Σ_{i<j} (sizes[i] - 1) - 1
```

side: the start of cell j = `menuvobsize_sum`
= end of cell j-1 + 1, consistent on both sides.

and testing: all correct values hit, old values disappeared (only 3 places that were originally equal remain).
Two finished disks (8 pages / 5 pages) checked byte by byte after reconstruction:

```
Disk 1: Pages 1..8  start=0,38,72,110,152,195,236,276  end=37,71,109,151,194,235,275,316
      start_j == end_{j-1} + 1 holds throughout; last page end = 316 = 317 - 1; next/prev = j + 1 / j - 1
Disk 2: Pages 1..5  start=0,40,81,124,166  end=39,80,123,165,204
      (The previously constructed start=[40,81,123,165] was a broken chain, now it is continuous)
```

> ⚠️ **Pitfall record: I fixed the wrong place the first time. In** `amg2.c` there are two copies of **in** this piece of code:

> | function | effect |
> |---|---|
> | `create_topmenu()` | what is written is dvdauthor/spumux running the placeholder IFO before **** |
> | `create_amg()` | What's written is **final** IFO (the one on the disc) |

> The only difference between the two is `uint32_check` vs `uint32_copy`, so the patch's 'unique match' check
> only hits the former. As a result, **seems fixed and recompilation passes, but the value in the final IFO does not change**.
> I discovered this by 'not finding the newly added debug string in the binary' — meaning:
> Just looking at the patch report [OK] for **is not enough, the product must be verified**.
>
> Incidentally discovered that `src/amg2.c` was not in the restore list of `build_dvda_author_mlp.sh`,
> it has been added (otherwise the idempotency of the patch is not guaranteed).

### 16.25 check: menu cell address chain (`verify_menu.py`)

That above defect is hard to locate based on symptoms (errors are offset when page sizes are similar), so it was made into
**machine-checkable invariant**.

#### Field positions (actual measurement, don't trust constants in the source)

Menu table starts from `0x1810`: 4 bytes of `0x181C` are the relative pointer of page 1 PGC of ****
(baseline `0x1810`), starting at `0x1820` is the index table of `nmenus-1` items (8 bytes per item,
last 4 bytes are also relative pointers). Within-page relative PGC start:

```
+0x09C Next page menu number   +0x09E Previous page menu number   (uint16)
+0x11E cell starting sector  +0x126 write the same value again  (uint32)
+0x12A cell end sector                        (uint32)
```

⚠️ The `0x13A` in **`amg2.c` is not the PGC step length**. The measured step length is `0x132`, 8 bytes less
It happens to be **the index entry following this page** — so 'one line' = PGC(0x132) + index entry(8) = 0x13A,
source code formula is self-consistent, but using **to locate PGC according to `0x13A` will cause the entire paragraph to be misaligned from page 2 onwards**,
, when read out, is all irrelevant data (and can even be made into 'fake values that look like addresses'). This is exactly how the first version of the verification worked.
wrote: It only uses the heuristic of 'appearing twice,' which is unrelated to offsets. On 8 pages, it only recognized 2 starts.
can only degrade to 'better than nothing.' After changing to direct reading based on measured deviation:

```
Page   start  end  span  next prev
1       0     37   38    2    0
2      38     71   34    3    1
3      72    109   38    4    2
...
8     276    316   41    0    7
Last page end=316 = 317-1 ✔ Sum of spans 317 = VOB sector count ✔
```

#### Criterion

1. Each page has two identical copies of `start`;
2. `start_j == end_{j-1} + 1` (chain continuous, no gaps / no overlap);
3. == the total number of sectors of `AUDIO_TS.VOB`;
4. Page 1 `start == 0`, last page `end == total_sectors - 1`;
5. `next/prev` The menu numbers are `j+1` / `j-1` (last page `next = 0`).

#### Reverse verification (mandatory)

A positive pass doesn't prove anything——**one needs to prove that checking on the old disk will report an error**. The approach is to use the finished IFO
Copy one, rewrite each page `+0x12A` according to the old formula (`end` always uses the span of the last page):

```
Before modification: 8 pages, 0 issues → Passed ✔
After modification: 8 pages, 7 issues → Error ✔
  · Page 1 cell ends at 40, but Page 2 starts at 38 (should be 41) — address chain broken
  · Page 2 cell ends at 78, but page 3 starts at 72 (should be 79) — address chain broken
  · ... (Page 6 not reported, its old value is exactly the same as the correct value)
  · The sum of cell spans on each page 328 != Number of sectors in AUDIO_TS.VOB 317
```

**Page 6 didn't get reported** This matter itself is the best commentary: the mistake was offset by coincidence,
Only looking at the appearance will only result in 'a few pages don't work'.

---

### 16.26 Play Cover (ASVS): How the 'one per album' rule is established

requirement is 'display the album cover when each song is played.' It is handled by ASVS:
`AUDIO_SV.IFO` (table) + `AUDIO_SV.VOB` (still image) are two separate mechanisms from the menu (AMG).

#### Implementation Method

`--stillpics` corresponds to a track, with a **empty item indicating 'use the previous diagram'**. We only
**Give an image for the first track of each album**, leave the rest of the tracks in the same album blank:

```
Album A Track 1 → A.jpg      Album A Track 2 → (empty)   Album A Track 3 → (empty)
Album B Track 1 → B.jpg      Album B Track 2 → (empty)   ...
```

Therefore, **each album only occupies one image** (Disc 1: 28 images / 91 tracks, Disc 2: 17 images / 56 tracks),
The player maintains the previous image display within the same album — this is exactly the desired "correspondence",
Also, it reduces the ASVS budget to the minimum (about 15~25 sectors per image, maximum 1024).

⚠️ This results in a **implicit dependency**: "reusing the previous image" within an album is **effective** sequentially,
so once a static image entry of an album is broken (for example, the album has no `cover.jpg`),
the following tracks will **display the cover of the previous album**, instead of leaving it empty.
`verify_menu.py` will now report this situation as `[WARN]`.

#### `AUDIO_SV.IFO` fields (tested, corresponding to `asvs.c`)

```
0x0C  u16  Number of tracks with independent static images (= number of albums with covers)
0x14  u32  Total sectors of static images minus 1
0x60  Start   Each record 8 bytes: Number of images (u8) Starting image number (u16) Starting sector (u32)
```

Disc 1 tested: 28 records, 1 image per record, starting image numbers 1..28 consecutive,
starting sectors `0,25,46,69,…,645` consecutive, `0x14 = 669 = 670-1`.

#### Checksum (`verify_menu.py` item 5c)

`check_stills()` Check: Number of items == Number of albums with covers, each item ≥1 image,
starting diagram number is consecutive from 1, `0x14 == AUDIO_SV.VOB sector_count-1`, starting sector does not exceed bounds.

**has been reverse-validated** (directly feeding the modified IFO to `check_stills_data()`,
bypass the internal unpacking that time, otherwise it will be overwritten by repacking:

| Change | Result |
|---|---|
| Total number of sectors `0x14` Correct error | Report error ✔ |
| A certain starting drawing number was changed incorrectly | Error reported ✔ |
| Few items declaration 1 | Error ✔ |
| Set a certain image count to 0 | Error ✔ |
| The expected number of albums is more than the actual (missing cover) | Error ✔ |

> Why separate fetching files and verification into `check_stills()` / `check_stills_data()`:
> First edition combines into a single function, and when testing, I wanted to feed in "corrupted data" but found that **internally re-unpacks it.
> overwrites the changes to**, so the 'error test' always fails (it looks like the check is invalid).

#### About `docs/DVDA-AUTHOR-CHANGES.md`

It is aimed at the `atsi2.c`'s `continue` — skip writing when a track does not have an independent image
ATSI, with the reason being 'ATSI does not display the cover when playing tracks without references.'
**But actual verification on live playback: the cover displays normally, this patch is not needed** (28/17 tracks in foobar2000
all correspond correctly). This script **has still not been integrated into the build script**, keeping the file for situations where
"the cover does not display after switching to another player", it must be verified on real device before integration.

### In version 16.27, "Next segment" cannot switch tracks: one audio group is split into each track as a separate title

**Symptom**: clicking "Next segment" in the player cannot switch tracks — the 1st track can jump to the 2nd,
but from the 2nd onwards, pressing again **returns to the beginning of that track**, never reaching the 3rd track; manually selecting the 3rd and 4th tracks
and then pressing "Next segment" will still jump back to the 2nd track.

**Root cause** lies in `amg2.c` the determination of "where a new title starts":

```c
if (samplerate != previous.track.samplerate || bitspersample != ... || channels != ...
    || cga != ...
    || files[group][track].type     == AFMT_MLP      // ← culprit
    || files[group][track - 1].type == AFMT_MLP)     // ← culprit
  files[group][track].newtitle = 1;
```

The adjacent comment explains the author's view:

> apparently MLP does not allow "gapless" same-audio characteristics titles,
> which means that 3 following tracks with same audio specs will create 3 titles
> instead of 1 for gapless PCM. TODO: check if this is software-dependent

That is, upstream **, based on the guess that "MLP cannot be seamlessly connected like PCM"**, makes "the previous track or the current track
is MLP, a new title is always started. All the audio sources in this project are MLP, so **treats each song as an individual
title** (in actual tests, disc 1 = 91 titles, disc 2 = 56 titles, each title has exactly 1 track;
the old menuless build is completely identical, so this is not introduced by menu functionality).

In DVDs, the meaning of "next section / previous section" (next chapter / previous chapter) is
**moving to the next track within the same title**. Each title has only one track, so the player has nowhere to go.

**fix**: remove those two MLP clauses (`docs/DVDA-AUTHOR-CHANGES.md`),
one audio group = one title, one track per song within the title — this is the standard layout of a commercial DVD-Audio disc.
The additional benefit is that a disc can be **played continuously to the end**.

Audio attribute retention comparison: if sampling rate/bit depth/channel count changes in the middle of the group, it will still correctly split into multiple
titles (this project's grouping has been done according to attributes, so it won't be triggered).

#### However, changing only this place will crash it (must also change `ats.c`).

The block that flushes the pack in `ats.c` hangs on `if (files[i].newtitle)`,
it **simultaneously performs two tasks**:

```c
if (files[i].newtitle)
  {
    write_pes_packet(...);      // ① Flush remaining data of the previous track into an independent pack
    ++pack;
    bytesinbuf = 0;
    pack_in_title = 0;          // ② Reset per track
  }
files[i].first_sector = files[i - 1].last_sector + 1;
```

- ① Ensures `files[i].first_sector = files[i-1].last_sector + 1` is valid in
  **pack on the boundary**; if not flushed, the next track starts from the middle of the sector, and the read disk end `get_ps1()` cannot be obtained
  pack head → **the whole song is discarded**.
- ② The `info->mlp_layout[]` of MLP is **allocated according to**.
  (`allocate_mlp_tracktable()`), and `write_lpcm_header()` uses it to calculate the offset:

  ```c
  frame_offset = info->mlp_layout[pack_in_title].pkt_pos - ...
  ```

  Cross-track accumulation will read outside the array.

Measured symptoms: `write_lpcm_header` received `pack_in_title = 880418`,
reads `mlp_layout[880418]` → **segmentation fault** (core traceback is clear at a glance).

to **flush unconditionally for each track**. For MLP discs, this is **identical to the behavior before the change**.
(every track is brushed in the original version), so the byte content of AOB remains unchanged, only the title table of ATSI/AMG changes
—— Verification also confirmed this: the finished ISO internal audio track is byte-for-byte identical to the source MLP****.

, the actual measurement shows: `ATS_01_0.IFO`'s `numtitles = 1` and `title1: tracks = 56`,
a title containing all tracks.

---

### 16.28 Menu changed to 'One Album Per Page': Stack Array Capacity and ASVS Record Count

Requirements:** Main title = CD title, subtitle = Album name, one album per page, each page background = the cover of that album. **The good news is that dvda-author already has this mechanism: `menu.c` during `ncolumns == 1`
's page loop** draws only one text section at a time, and only after finishing all the tracks within the section does it turn the page **; whereas `--screentext`
The first `=` was previously `albumtext`, painted by `prepare_overlay_img()`** at the top of each page **. So just let `--screentext`** each paragraph = an album **:

- Section Title →** Subheading **(`grouptext[section][0]`, Font Size = `0.8 × --fontsize`)
- `albumtext` →** Main Title **(Font size hard-coded `DEFAULT_POINTSIZE = 25`, appears on every page)

therefore `MAX_POINTSIZE` received**30**, ensuring 'Main Title (25) > Subtitle (24)'.

needs to do by themselves are:

1.** page is decoupled from the audio group **. The button is `jump group G track K`, G/K is
   "Audio group + track number within the group" is unrelated to "page." Added `compute_menu_pages()`.
   parses `--screentext` to get the number of tracks per page, and then converts each page back according to the number of tracks in the audio group
   `(group, first_track_in_group)`.** If any step does not match, then completely abandon **and revert to the old layout ——
   button being misaligned won't cause an error; it will just play one song but start another.
2.** The number of lines per page is taken according to the actual number of tracks on that page **. Original `maxbuttons = ceil(total_tracks/page_count)`
   is a global value, and since the number of tracks in each album is different, it is inevitable that some pages cannot fit them all. The number of lines also determines the line spacing of the text.
   (`mogrify_img`'s `maxnumtracks`) and button rectangle (`compute_coordinates`
   Use `img->maxbuttons`, and taking the same value per page can ensure alignment on both sides.
3.**The meaning of `ntracks[]` is changed to 'number of tracks per page'**. In `menu.c` / `xml.c`
   `ntracks[k]` is supposed to be 'the number of tracks in segment k', it just so happened that originally segment == audio group.

#### Pitfall 1: The stack array in `main` has only 9 elements

```c
uint32_t tab0[9] = {0};   // soundtracksize
uint32_t tab1[9] = {0};   // grouptextsize  ← globals->grouptextsize points to it
uint32_t tab2[9] = {0};   // tracktextsize  ← globals->tracktextsize points to it
```

`menu.c` will **write directly into these two arrays** (`globals->grouptextsize[k] = 2;`).
Originally, "text field == audio group" could have up to 9; now, text field == number of pages (tested 17 / 28),
Writing `tab1[16]` will overwrite the stack canary of `main`:

```
*** stack smashing detected ***: terminated (exit code 134)
```

The core backtrace shows that the overflow was only detected **when returning** from `main` (`dvda-author.c:435`)
—— In other words, **the breaking point is far from the accident point**. Fix: change the capacity to `MENU_TEXT_GROUP_MAX`
(256, because `--nmenus` is `uint8_t`), and add it at the parsing point
Error guard for `dim > MENU_TEXT_GROUP_MAX`.

#### Pitfall 2: ASVS records are grouped by **title**

, the static chart of `AUDIO_SV.IFO` changes from '28 records, 1 image each' to
'**1 record**, 28 images'. Both are legal groupings:

| title structure | ASVS record count | total number of images |
|---|---|---|
| one title per track | 1 per album = 28 records | 28 |
| one title per group | 1 record | 28 |

Therefore, the verification criterion must be **'total number of images == number of albums with covers'**, not
'record count == number of albums' — otherwise correct output will be marked as failed
(`verify_menu.py` has been corrected accordingly). During playback, 'which track displays which image' is determined by ATSI's static image
record `(picture_number, track_number, onset)`, not related to the number of records; that part is originally **by track**,
'no record for subsequent tracks of the same album → reuse the previous image' mechanism remains unchanged.

#### Pitfall 3: tighter container

One album per page → more pages → one full cover per page → `AUDIO_TS.VOB` increases from 317 sectors
to **943 sectors** (Disc 1). Remaining space on Disc 1 drops from 56.2 MB to **54.9 MB**,
still burnable, but with less margin.

#### How to confirm 'the images are correct'

does not want to rely solely on the naked eye to view screenshots, **directly analyzes the rendered output**: `<build>/tmp/discN/` contains
`hlpic<page>.png` (text layer with alpha) and `bgpic<page>.jpg` (background frames).
By counting the alpha ink line by line, you can get the y-range of each line of text:

```
y 28..76 right boundary x=408 ← main title (baseline 48) + subtitle (baseline 74) combined into one strip
y  97..124  right boundary x=134   ← Track 1 (y(1, maxbuttons+4=10) = 122) ✔
y 145..172                 ← Track 2 (baseline 170) ✔
...
y 388..410                 ← Next arrow (baseline 410) ✔
```

You can also verify that "text does not exceed the screen" (right boundary < 720), and that **has different line spacings when the number of lines per page differs
It indeed changes accordingly** (Page 1 six-track line spacing 48, Page 2 five-track line spacing 53). For the background,
`compare -metric RMSE` is compared with `cover.jpg` of the album (actual measurement: Page 1 vs. "Flying Snowflake"
cover RMSE = **1.4%** → that’s the one; Page 2 vs. it 12% → not it).

---

### 16.29 "Next track" jumps back to track 1: the timeline within a title is not continuous

After changing "one title per track" to "one title per group" (16.27), **both track selection and auto-play work normally**,
but **always jumps back to track 1 when pressing "Next track", no matter which track it is**.

#### Root cause: The PTS of MLP is along the track, and there was no translation after merging

MLP each track is counted starting from `PTS0` (measured 98), and in AOB, the pack head PTS of each audio track starts again from 98
Start (Measured that an AOB has 24 pullbacks, which exactly matches the number of tracks in that AOB). In the original version, each track is independent
title, so **each track has a timeline**, no problem; after merging into one title, follow the DVD structure
**A title (PGC) has only one timeline**, and `info->first_PTS` is the first pack on the track
is where the PTS is read:

```c
if (start_of_file)
  {
    if (pack_in_title == 0 || wait_for_next_pack)
      {
        info->first_PTS = PTS;   /* ← So all 56 cells have first_pts as 98 */
```

So the `ATS_01_0.IFO` table of **cell timestamps in** (the logical timeline of PGC) became 56 cells
all `first_pts = 98` —— The timeline is broken into 56 segments, each starting from 0. The player follows
'Next song = jump to the next Program/Cell' When checking this timeline, addressing any song will land at
at PTS 98, which is **track 1**.

This also explains some earlier behavior: at that time, each track had a title, and Next = 'the next one within this title'
PTT", and there is only track 1 in the title → loops back to the beginning of the first track (exactly the symptom I noticed at first).

#### Modification: Add cumulative PTS offset to subsequent tracks

`patch_mlp_one_title.py`：

```c
/* create_ats: After passing each track boundary, accumulate the duration of the previous track */
pts_shift += files[i - 1].PTS_length;
files[i].pts_shift = pts_shift;

/* write_pes_packet: Move this track to the timeline of the title it belongs to */
PTS += info->pts_shift;
DTS += info->pts_shift;
SCR += (uint64_t) info->pts_shift * 300;   /* 27MHz / 90kHz = 300 */
```

`first_PTS` is taken from PTS within the same function, so it will **automatically** become a cumulative value ——
does not need to be changed separately from `atsi2.c`. The offset uses the duration declared in the previous track `PTS_length` (as in AMG
title length is homologous); the measured inter-track gap is < 1000 ticks (< 11 ms), harmless.

incidentally added the `pts_shift` field to `fileinfo_t` (**appended at the end of the structure**, because this structure
is also afraid of initializing by position.

#### actual measurement

```
Before repair: the first_pts of cell1..56 are all 98
      → quick_check reported 'PGC timeline is not continuous' (already used the pre-change finished product for reverse verification)

After repair: cell1 = 98
      cell2  = 21016373        (= 98 + 21016275)
      cell56 = 1141013198
      end of cell 1161646073 vs len_in_pts 1161645975 (difference 98 = PTS0) ✔

AOB also continued: 98 → 392794298 → 787155098 → 1161645548 (dropped back to 0)
```

#### This exactly matches the DVD-Audio 'single PGC + multiple Cell' model.

- **PGC** provides a **continuous logical timeline** (which is the cell timestamp table above)
- **Cell** is an audio segment on this timeline, one cell per song
- cell/program is the track boundary; the player plays sequentially within the PGC and automatically goes to the next one when finished
- "Next Track" = Jump to the next Program/Cell

`quick_check.py` added this invariant (the `first_pts` of all cells within each title must
strictly increases, the last cell must match `len_in_pts`, with a tolerance of 1 second.

> Lesson: **The timeline is part of the 'structure', not just metadata**. Take N items each starting from 0
> fragments must be moved as a whole when merging with a PGC — otherwise, whether it can play or not won't show any issues.
> but **positioning** will all collapse to the first paragraph.

---

### 16.30 No matter which song is played, it's always the cover of the first album: the image number is counted by 'title'

**Symptoms**: After merging into one title, playing **any** song shows **the cover of the first album**.

**Root-cause hypothesis at that stage**: **byte 1 of each ATSI still-picture record was treated as the picture number**, whereas dvda-author wrote

```c
atsi[i++] = pictitlecount;      /* "Which numbered title has a picture", counted by title */
```

and `AUDIO_SV.IFO` over there is recorded as **global** drawing number:

```c
uint16_copy(&asvs[k], pict + 1);   /* pict accumulates across titles → global frame number */
pict += npics;
```

Originally, "each track forms its own title" (28 titles × 1 image), at that time, the two **happened to be equal**:
title k's `pictitlecount = k+1`, the overall drawing number is also `k+1` — once again 'counted by title'
happens to be equal to the track number.

into a single title, `pictitlecount` always becomes **1**. The measured ATSI of the finished product:

```
Track 1..8: Drawing number=1 flag=0x00 u16a=0x0150 u16b=0x01f9     ← All 56 items are 1
```

All tracks consequently point to picture 1, so every track shows the first album's cover.

> ⚠️ **My first version of the "revision" here was wrong, and it also crashed the player.**
> At the time, I thought that byte was 'diagram number' and changed it to 1..17. In fact, it was '**which ASVS item'
> Records**, whereas ASVS records are **one per title** (with the number of images + starting image number contained within the record)
> —— So referencing 2..17 pointed to a non-existent record **in**, causing the player to go out of bounds → **jumps to the next track
> crashes immediately**. This change has been reverted.
>
> The correct way to modify the method is **to change the granularity of both tables from title to track**
> (`patch_asvs_per_track.py`), see 16.32.

---

### 16.31 `ATS_PTT_SRPT` has never written

`ATSI_MAT`, the `ATS_PTT_SRPT` (offset `0xC8`) **dvda-author is always written as 0** —
source code, there is only one line: `uint32_copy(&atsi[0xCC], 1);  // Start sector of ATST_PGCI_UT`,
`0xC8` has never been assigned a value. Tested finished product: `ats_pgcit = 1` while `ATS_PTT_SRPT = 0`.

This means in a DVD: **there are no 'chapter points'**. And the definition of 'Next / Previous track' is
'Jump to the next / previous **PTT (Part Of Title)**'.

has been supplemented according to the layout isomorphic to DVD-Video `VTS_PTT_SRPT` (`patch_ats_ptt_srpt.py`):

```
+0x00 u16 nr_of_srpts        Number of titles in this titleset
+0x02 u16 zero
+0x04 u32 last_byte
+0x08 u32 ttu_offset[nr_of_srpts]     TTU offset for each title (relative to this table)
TTU: +0x00 u16 nr_of_ptts    Number of tracks in this title
     +0x02 u16 zero
     +0x04 { u16 pgcn; u16 pgn; }[nr_of_ptts]
```

Measured Output Table (Disk 2):

```
ATSI Size=8192 ATS_PTT_SRPT(0xC8)=3 Sector → 0x1800
PTT Table: nr_of_srpts=1 last_byte=239 ttu_offset[0]=12
  TTU@0x180c: nr_of_ptts=56
  (pgcn,pgn) = (1,1) (1,2) … (1,56)
```

table is written at the sector-aligned position after PGCI; `i` advances to the end of the table,
`*atsi_sectors = ceil(i/2048)` automatically extends ATSI from 3 sectors to **4 sectors**,
and `atstt_vobs` (AOB starting sector), `atsi[28]`, `atsi[12]` are all derived from it,
Automatically adjusts accordingly (tested `atstt_vobs = 4` ✓).

⚠️⚠️ **Status: Deprecated.** Real device test results: **'Next track' still does not advance, and after the jump
Crashes immediately** — this indicates that the layout/value of this table is wrong (or it isn't the parsing for 'next track' at all)
Entrance), if the player addresses according to the wrong pointer, it will go out of bounds. Therefore, this script has been removed from the build process and changed to
"Execution equals refusal." Analysis of failure reasons and subsequent prerequisites can be found in 16.32.

---

### 16.32 'Cover Slide Along Track' Plan B, and a Lesson That Crashed the Player Once

#### Lesson: The Cost of Two Blind Trials

16.30 The “Figure number counted by title” recorded in the book is correct, but **the first version of the fix I made based on this was wrong**,
And the consequences are much more serious than the original defect —

1. I initially treated byte 1 of each ATSI still-picture record as a global picture number and changed it to 1..17.
   In fact, it is '**which ASVS item**', and the ASVS items are **recorded one per title**
   (Records only contain the number of diagrams internally + starting diagram number).
   → Reference 2..17 points to **a non-existent record** → Player out of bounds →
   **Jumping to the next track causes an immediate crash**.
2. At the same time, I supplemented `ATS_PTT_SRPT` according to the 'by analogy with DVD-Video' structure.
   (16.31) → 'Next track' still does not advance, **and crashes after jumping**.

Both places have been rolled back; `patch_ats_ptt_srpt.py` has been changed to **reject on execution**, to prevent miswiring.

**Fundamental Lesson**: Don't try to fill in a binary table by analogy. This kind of change 'doesn't give an error when wrong, but instead causes the player
"Collapse," at a very high cost. Before using the ATSI/ASVS tables again, you must first obtain authoritative structural definitions, or obtain
Compare the fields with known good reference discs of the same type; if neither is available, you should **stop and state the truth**.

#### Plan B: Change the granularity from title to track (modify both sides in pairs)

The correct problem description is not 'the figure number is wrong,' but **both tables are organized by title, while the player is based on
"Current Track" Lookup Table**:

| File | Original | Changed to |
|---|---|---|
| `asvs.c` | Write one record for each **title** (number of tracks + starting track number + starting sector, with the sector table starting at `0x378`) | Write one for each **track with images** |
| `atsi2.c` | `pictitlecount` accumulates by **title** | accumulates by **tracks with images** |

So 'each track forms its own title,' both are equivalent to 'by track'; after merging into one title,
The former only writes **1 record**, while the latter is always **1**, so all tracks query the 1st record → all are displayed
The first picture.

**Key: both sides must be changed in pairs.** Only modify the ATSI reference values (references 1..17) while ASVS remains unchanged.
1 record, which is the crash mentioned above.

Because this is "modifying the number and content of records" rather than "guessing the layout of an unknown table," the risk nature is different.
Enabled via environment variable `DVDA_ASVS_PER_TRACK=1`; if not set, the original behavior is completely preserved.
(no need to recompile to switch, can quickly revert if it fails).

Actual output tested:

```
AUDIO_SV.IFO number of records (0x0C) = 17        ← one per album
  Record 1: number of pictures=1 start picture number=1 start sector=0
  Record 2: number of pictures=1 start picture number=2 start sector=25
  Record 3: number of pictures=1 start picture number=3 start sector=49
ATSI still picture reference numbers = [1,1,1,1,1,1, 2,2,2,2,2, 3,3,3,3,3,3, 4, 5,5, …]
  Maximum = 17 ≤ 17 ✔ not out of bounds
```

Reference sequence perfectly matches album structure (Album 1 = Tracks 1-6 → Record 1; Album 2 = Tracks 7-11 →
Record 2; …).

> ✅ **verified on real hardware (PowerDVD)**: cover **changes correctly with album**, and **no longer crashes**.

#### New check: still picture reference numbers must not exceed ASVS record count

`quick_check.py` added one check (this is exactly the check that should have prevented that crash):

```
[OK] Still picture reference number 17 <= AUDIO_SV.IFO record count 17 ✔
```

Out-of-bounds report FAILs and directly points out the consequences and causes. **has done a reverse validation**: change a certain reference number to 99
(ASVS has only 17 items) rebuild ISO, check for accurate error reporting:

```
✗ Static image reference number 99 > AUDIO_SV.IFO count 17
   Consequence: Player fetches cover based on out-of-bounds record number → ** crashes ** (crashes when skipping to the next track)
   Cause: Inconsistency between ATSI and ASVS’s 'by title / by track'; both must be modified in pairs
```

---

### 16.34 The exact phenomenon of 'Previous / Next track' and the current conclusion

(**not yet resolved**. This section records the phenomenon accurately and lists the directions already ruled out to prevent future people from repeating the detours I took.)

#### Phenomenon (PowerDVD, disc 2, 56 tracks)

| operation | observation |
|---|---|
| **auto-play** goes to next track | track number **displays correctly**, and **audio indeed progresses** |
| but in this state, pressing 'Next section' | **returns to track 1**, track number changes `0/56` |
| in the same state, pressing 'Previous section' | **returns to the previous track** (track number correct), pressing again **has no response** |
| **Direct jump** (using menu / list to select a track) | Audio **starts from the beginning 0:00**, track number display `0/56` |
| Press Next after a jump | Returns to track 1 |
| The "Jump to" list itself | **Track number is correct** (1..56), whichever track is selected it jumps to that track |

Key difference: **In auto-play, the player's "current track" status is correct** (can move forward, can move backward),
but **as soon as a jump occurs (or using Previous/Next Segment once) it reverts to "not positioned within a track"**,
after which all operations based on "current track" degrade.

#### Conclusions drawn from this

- **"Track N → Position" forward parsing is valid**: list number is correct, jump is correct —
  It uses that track-by-track table in ATSI (`first_pts` + `first_sector/last_sector`),
  and that table we have confirmed to be correct and with continuous timeline (16.29).
- **"Current position → Track number" reverse tracking is valid under the "auto-play" path**, but
  **after a single jump it fails** (reverts to 0). So it's not that "the player completely lacks this mechanism",
  but **the required information cannot be obtained along the jump path** — the most likely carrier is still
  `ATS_PTT_SRPT` (16.31): The PTT table gives the interval for each track.
  The player relies on it to map the 'current time' back to 'which track'.
  When jumping directly, it can only set the position to 'time 0:00 with the track number undefined', so it displays 0/56.
  This also explains why **after jumping, it always starts from the beginning (0:00)** —— it doesn't have 'the starting point of this track at
  The position on the timeline is available, but you can only return to the origin of the timeline.

#### ⚠️ But **do not** continue writing by analogy with the DVD-Video table

I supplemented `ATS_PTT_SRPT` (16.31) according to the structure of `VTS_PTT_SRPT`:
"'Next Track' **did not improve**, and **jumping to a later track causes PowerDVD to crash immediately**"
(An incorrect pointer caused an out-of-bounds error). It has been removed and the script has been changed to **reject immediately upon execution**.

Conclusion: **It should not be altered without an authoritative structure definition**. The failure mode of this kind of change is not 'ineffective',
Instead, it is 'to crash the player,' with a cost far higher than ordinary soft defects (see 16.32 and Lesson 26 for details).

#### To continue addressing this issue, any one of the following is required

1. **A known good reference disc of the same type** (commercial DVD-Audio).
   By comparing its `ATS_PTT_SRPT` with ours field by field, you can know the real layout.
   This is the most direct route — you can just read its `ATS_xx_0.IFO` using ready-made tools.
   (The parsing code of this toolchain can be directly reused).
2. **Navigation section of the DVD-Audio specification** (ATS chapter of Part 4),
   contains the accurate field definitions for each table.

Before the above conditions are met, it is recommended to treat "Previous Track / Next Track" as known limitations of **that** accepts:
- Automatic playback, menu track selection, "Jump to" list, and playback cover **all work normally**;
- The only loss is that you cannot use the player's Previous/Next Track buttons to step through tracks one by one.

(Incidentally, recording a disproven guess: I once thought that the ATSI timestamp recorded the first byte of
`0xC000` as the "track start" bit—after filling it for each track on a real device, **showed no change**, and this was retracted.
This indicates that this bit only has significance for `t == 0`, not the answer here.)

---

### 16.35 Lessons Learned Summary

1. The upstream "automatic" function of **is often only verified on a small scale**. Before adding the menu, run it on the smallest scale
   (3 tracks, 1 page), then gradually scale up to the real size (multiple groups / multiple pages / multiple albums),
   leaving a verifiable criterion at each step.

2. **"does not report errors" does not mean "does it correctly"**. Among this series of menu issues, the most dangerous ones
   (buttons cover only 32 tracks, menu VOB not generated at all, AMG buffer overflow) result in exit=0 or
   crashes in completely unrelated places. It is necessary to explicitly check the outputs: total number of buttons, `AUDIO_TS.VOB` /
   whether `AUDIO_SV.VOB` exists, and the number of `AUDIO_TS.IFO` sectors.

3. Wherever **requires "we also count it once," it must be aligned word-for-word with upstream and include self-checking** (see 16.12).

4. **should undergo functional testing based on the actual character set used** (see 16.5),
   rather than checking the font table or testing only a 'Chinese' probe — Korean was overlooked in this way.

5. **`--stillpics` 'capacity' is 'number of sheets × 15 sectors', not 'image size'**;
   the only way to save capacity is **reuse** (empty entries), repeated assignments to the same path do not count as savings.

6. **'all stages succeed, final inexplicable segment fault' → suspect VLA** first
   (automatic arrays whose size is determined by external variables). Its crash point can be tens of minutes away from the cause,
   and it can corrupt local variables at the crash site.

7. **debugging method**: `stdout` full buffering will swallow output before the crash (GNU `foutput` uses stdio).
   Effective method: `stdbuf -o0 -e0`, `dmesg`, and **reproduce** using small samples of the same structure
   — replace 91 full-length inputs with 91 inputs of 1 second each, reducing reproduction time from about 10 minutes to about 1 minute.
   Reusable reproduction script for this project: `/tmp/m91/run.sh` (66+25 two sets, 9 pages,
   parameters consistent with real build).

8. **'all outputs are present, crash occurs at the end' → look for out-of-bounds writes, not logical errors**.
   The fact that all files were written indicates the main process is correct; the crash happens in the cleanup phase —
   that is, **some previous out-of-bounds write just touched a critical variable** (see 16.18).

9. **Does not crash under gdb + crashes immediately when run normally → use core dump**.
   gdb will change memory layout (by default ASLR is also off), `set disable-randomization off` might not be able to reproduce
   the bug. Core dump does not affect layout; it is the correct solution for this kind of bug:
   `resource.setrlimit(RLIMIT_CORE, (-1,-1))` + `gdb -batch -ex bt prog core.N`

10. **for any 'VLA allocated according to input length', you must ask 'what happens when the length is 0'**.
   `int32_t cut[strlen(s) / 2]` plus an empty string is a classic landmine (see 16.18).

11. **Do not treat 'just because one disk didn’t crash' as no problem**. Disk 1 and Disk 2 go through the same defective code path,
   the only difference is that one hits a critical variable and the other doesn’t. You must run disks of different scales.

12. **For a protocol 'concatenated with a delimiter without escaping,' you must check if the data contains the delimiter**.
    Song titles/album names commonly contain commas, colons, equal signs (in this project, 4 songs have ASCII commas).
    The way to check is **check the number of items after parsing, not by looking at the code (see 16.22)**.

13. **When the same logic outputs to multiple products, check them one by one**.
    The menu arrow has 'text/position/jump' outputs; fixing two of them doesn’t mean it’s fixed.
    — 'Position is correct' does not mean 'jump is correct' (see 16.23).
    The most effective way is to turn 'the quantity/number on both sides must be equal' into **a machine-checkable invariant**.

14. **Self-check must be able to verify on old data that 'it really will report an error'**.
    After writing `check_menu_buttons()`, I ran it once using the test data from the **old build**.
    Make sure it actually reports FAIL (13 vs 9) — otherwise, you can't rule out 'self-check always passes'.

### Overview of upstream defects fixed in this project (19 patch scripts)

| Patch | Target File | Fixed Issues |
|---|---|---|
| `patch_base` | Multiple | Basic fixes (compilation/environment related) |
| `patch_read` / `_read2` / `_encode` / `_ats_pack` | `mlp.c`, `ats.c` | FFmpeg 8 API migration, 24-bit, pack boundaries |
| `patch_fix_fn_strtok` | `auxiliary.c` | **Out-of-bounds write of zero-length VLA for empty string** (see 16.18) |
| `patch_menu_paging` | `menu.c` | Paging formula (total always ≤32 buttons), page guard false positives |
| `patch_menu_backgrounds` | `command_line_parsing.c` | Wrong background taken per page, blankscreen overwrite, heap overflow |
| `patch_menu_screentext` | `menu.c` | `cutloop` static variable leak, insufficient number of entries, `size` reuse |
| `patch_menu_layout` | `menu.c`, `xml.c` | `maxntracks` Track crashes stack when line number used → >34 |
| `patch_menu_arrows` | `menu.c`, `xml.c` | Arrow text misalignment (see 16.21), last page repeated (see 16.23) |
| `patch_menu_stillpics` / `_list` | `menu.c`, `command_line_parsing.c` | `pict` Set to NULL, file list mode cd fails |
| `patch_menu_amg_size` | `launch_manager.c` | AMG buffer does not grow with page count (see 16.17) |
| `patch_menu_amg_cells` | `amg2.c` | Menu cell end address used wrong size → Some pages Previous invalid (see 16.24) |
| `patch_atsi_dynamic` | `atsi2.c` | ATSI table fixed 3 sectors → One group up to ~65 tracks (see 16.19) |
| `patch_mlp_one_title` | `amg2.c`, `ats.c` | MLP each track self-contained title → “Next segment” cannot skip song (see 16.27) |
| `patch_stillpics_atsi_record` | `atsi2.c` | Static image record skipped (cover reference) |
| `patch_menu_one_album_per_page` | `menu.c`, `xml.c`, `amg2.c`, `structures.h`, `menu.h`, `commonvars.h`, `dvda-author.c` | One album per page (large/small title, cover on each page) (see 16.28) |

---

15. **Patch report [OK] does not mean the product has changed — the product must be verified**.
    The patch only changed "the first match in the source code", but the same section of code appears twice in the file
    (one writes the placeholder IFO, the other writes the final IFO), as a result it compiles successfully, but the product remains unchanged
    (see 16.24). The method of judgment is **to search for features of the new code in the binary/product**
    (for example, newly added debug strings), rather than only looking at the patch log.

16. **There may be multiple copies of the same piece of code in a single file**. Before changing, first
    `grep -c` count them, do not rely on "unique match" checks to find them for you —
    it can only tell you "there are multiple copies", it cannot tell you "which one to change".

17. **Leave a machine-checkable invariant for defects that are "difficult to locate based on symptoms"**.
    The cell address chain was fixed this way (see 16.25): when the VOB size per page is similar,
    incorrect addresses will be exactly canceled out on some pages, relying only on observing symptoms can only conclude "a few pages don't work".

18. **A piece of code may serve two purposes**. Before deleting a condition, first see clearly what it actually does —
    `if (files[i].newtitle)` In addition to "splitting title", it also **aligns packs by track**
    and reset `pack_in_title` to zero. If you only remove the former, the latter silently fails → segmentation fault.
    Criterion: after modification, the parts of **in the product that are unrelated to the changes should remain unchanged byte by byte**.
    (This time confirmed by “the audio track in the finished ISO matches the source MLP byte by byte”).

19. A fixed-capacity stack array of **will explode when encountering a “magnitude change”**. `tab1[9]` config
    “Up to 9 audio groups” is sufficient; after the text field is changed to “pages,” it becomes 17/28, which directly overwrites
    the canary of `main`, while the error point reported by **(when main returns) is far from the actual cause**.
    For any container where the “quantity is determined by the data,” first ask what assumptions its capacity is based on.

20. The criterion for **must change along with the structure**. The number of ASVS records has changed from “one per album” to “one per title.”
    These are two legal packings of the same data for ****; if the verification hardcodes “record count == album count,”
    it will incorrectly mark the correct product as failed. Changing it to “total number of images == album count” is the true invariant.

21. **can verify typesetting without seeing the images**: the text layer is a PNG with alpha; by counting ink line by line,
    you can calculate the y-range of each line of text and then compare it with `menu.c`’s `y(track, maxnumtracks)`
    formula — more accurate than the naked eye, and can conveniently detect “out-of-frame” text as well.

---



---

22. **’s timeline is part of the “structure,” not just metadata**. Take N segments, each starting from 0
    integrates a PGC, the PTS must be translated as a whole: otherwise, 'whether it can play' is completely normal, but **positioning** will
    all collapsed to the first section (see 16.29). The criteria for this type of defect should be applied to the **structure field**
    (whether the cell's first_pts is increasing), rather than 'whether it sounds right'.
23. **Behind the same 'Next track is wrong' there can be two completely different structural problems**:
    first is the title granularity (16.27), then the continuity of the timeline (16.29). Each time you have to go back again
    **product fields** cannot be located based on 'it was like that last time' to apply conclusions.

---

24. **'Count by title' is an assumption that repeatedly appears in dvda-author**. After merging titles,
    Anything that happens to fit the rule 'each track originally forms its own title' needs to be re-examined:
    timeline (16.29) and cover image number (16.30) are the same pattern ——
    happens to be equal to the track number, and it collapses into a constant after merging.
    Troubleshooting method: **print two quantities that should correspond one-to-one side by side** (such as 'ATSI drawing number sequence' pair
    'Album Boundary'), you can tell at a glance which one has collapsed.
25. **It is easier to judge a missing form than a wrongly filled form**. `ATS_PTT_SRPT` Always 0 is 'none',
    can be directly confirmed from the product; whereas `0xC000` is the kind of 'a certain position might mean this'.
    You can only know after trying guesses — so the latter must be **instantly revertible** (separate patch + backup),
    If it fails, withdraw it, leaving no partial state behind.

---

26. **Do not rely on analogy to fill in the binary table**. `ATS_PTT_SRPT` I follow the table with the same name as DVD-Video
    , the result is that the player crashes directly. This kind of change 'doesn't report an error when wrong, but instead makes the player crash.'
    costs far more than ordinary defects — it is necessary to obtain an authoritative definition or a known good reference disk of the same type.
27. **move only one place at a time, and be able to instantly undo**. I take 'change reference value' and 'add a guessed table'
    released together, but when it crashed, we couldn't tell which one it was (and it wasted a round of actual testing).
    Later, an `DVDA_ASVS_PER_TRACK` environment switch was added to Plan B, allowing switching without changing the code.
28. **Paired agreements must be changed in pairs**. 'Record number referenced by ATSI' and 'Number of records in ASVS'
    is a pair: changing only one side will cause an out-of-bounds crash (16.32). Anything of this kind 'reference value ≤ number of table entries'
    relationships should all become a machine-check invariant — the check could have originally stopped this crash.

---

## 17. ISO overall md5 not reproducible

**Phenomenon**: With the same source code, the same audio, and the same parameters, building twice in a row,
The md5 hashes of `out/*.iso` differ. This looks like an unstable build, but the actual contents are identical.

**cause**: `mkisofs` Every time it writes **the current time** to ISO.

Minimal reproduction (package twice in the same directory):

```bash
mkdir -p root/AUDIO_TS && echo hi > root/AUDIO_TS/X.TXT
mkisofs -dvd-audio -V Repro -o a.iso root
sleep 2
mkisofs -dvd-audio -V Repro -o b.iso root
md5sum a.iso b.iso
# 48991b91c609827110441f382104f341  a.iso
# e6045a79b002baf03db916c80eaa3d20  b.iso     ← different
```

The difference **is only 52 bytes**, distributed in sectors 16 / 21 / 32 / 48 / 64 / 257 / 259 / 261 / 263
(volume descriptor and directory record area), content is ISO9660 ASCII timestamp
(format `YYYYMMDDHHMMSScc`):

```
First difference @0x8330:
  a: 36 30 39 32 34 31 33 32 36 31 37 33 33   = "6092413261733"
  b: 36 30 39 32 34 31 33 32 36 31 39 33 34   = "6092413261934"
```

**Correct verification method**: Compare the system files under `AUDIO_TS/`, not the whole ISO.

```bash
for f in ATS_01_0.IFO ATS_01_0.BUP AUDIO_PP.IFO AUDIO_SV.IFO AUDIO_SV.BUP \
         AUDIO_SV.VOB AUDIO_TS.IFO AUDIO_TS.BUP AUDIO_TS.VOB; do
  xorriso -osirrox on -indev NEW.iso -extract "/AUDIO_TS/$f" /tmp/new/$f
done
md5sum /tmp/new/*
```

Actual test after reconstruction **9/9 system files are identical byte by byte** — this is the criterion for "reproducible build."

**Lesson**:

- "Different md5" does not equal "content changed." First check the position and quantity of **differences** —
  A few dozen bytes, concentrated in metadata sectors, should suspect timestamps rather than data.
- Audio data must be verified byte by byte separately (`verify.sh lossless` this is exactly what was done:
  Decode PCM and compare byte by byte with the source, compare tracks in ISO with source MLP by md5).

---

## 18. Static images are refreshed only for the first track of each album

**Phenomenon (user report)**: The cover is refreshed only when switching albums during playback; switching tracks within the same album does not change the image.

### Root cause: The two offset fields of the ATS static image table were written as constants

ATS static image table (title descriptor `+14`) layout is:

```
[6 bytes per track record] × number of tracks     ← immediately follows
[10 bytes per picture list] × number of pictures
```

Each track's 6 bytes:

```
[Picture number / highest title index: 1][byte1:1][track list start offset: 2][track list end offset: 2]
```

Upstream writes the last two items as **, a constant unrelated to the track,** (`atsi2.c`):

```c
uint16_copy(&atsi[i], 0x06 * ntitletracks[j]);                 /* start */
uint16_copy(&atsi[i], (ntitletracks[j]-1)*0x6
                      + 0x0F + (ntitlepics[j]-1)*0xA);         /* Stop */
```

- `start = 0x06×track_count` — coincidentally correct only for **track 1**
- `end = 6×track_count + 10×picture_count - 1` —— only the last track **of** happens to be correct

Therefore, all tracks within the same title claim ‘My picture is at `[6n, 6n+10p-1]`’ →
Every per-track lookup returns **picture 1**, so the picture never changes during playback.

### Comparison: Commercial discs with normal covers progress track by track

```
Li Na Track 1: 01 00 00 48 00 51   a=72 =6×12+10×0   b=81 =a+9
Li Na Track 2: 01 00 00 52 00 5b   a=82 =6×12+10×1   b=91 =a+9
Bach Track 1: 01 04 00 6c 00 75   a=108=6×18+10×0   b=117=a+9

This project (before repair) Track 1..n: 01 00 00 24 00 5f   a=36 (constant)  b=95 (constant)
```

General formula: `a(r) = 6×track_count + 10×(sum of pictures on tracks before r)`,
`b(r) = a(r) + 10×pictures_on_this_track - 1`. When there is exactly 1 image per track, it degenerates to `a = 6n + 10r`, `b = a + 9`.

> `byte1` **do not touch**: in the source code, `0x04` corresponds to `--stilloptions manual`
> (manually pageable slides). Upstream originally only sets 0x04 when this option is passed,
> This project never passes it, actually always `0x00`.

### Verification method (reusable)

To know "which bytes changed" in **, first extract the two versions before and after repair of `ATS_01_0.IFO` and then diff them byte by byte**,
and map the differing positions back to the static image table sections of each title:

```python
diff = [i for i in range(len(A)) if A[i] != B[i]]        # A=new, B=old
# For each title, calculate the static image table section [st, st + 6*ntracks) and check whether all diffs fall within it
```

In practice: **78 differing bytes = 2×(56 tracks − 17 titles)**, **all fall within the static image table**;
The remaining 6 system files (`AUDIO_PP.IFO` / `AUDIO_SV.IFO/VOB` / `AUDIO_TS.IFO/BUP/VOB`)
are identical byte by byte to the pre-repair ****.

That `2×(track_count−title_count)` is not a coincidence: each track has 2 fields, but
**The `a` of the first track and the `b` of the last track are naturally correct**, so each title is missing 2 fields;
Also, since the values are all < 256, only the low byte changes → exactly 2 bytes per field.

### lesson

**When disabling (rolling back) several patches at once, each one requires individually confirming the reason.**
The `patch_stills_per_track_rank.py` contains two parts of changes (progressive offset per track +
A comment about 0x04), back then the entire patch was discarded because '0x04 is suspicious'.
But the actual build doesn't pass `--stilloptions` at all — so a correct fix is wasted,
Directly caused this bug. See the 'Rehabilitated' section in `docs/DVDA-AUTHOR-DISABLED.md` for details.

---

## 19. The patch is not idempotent, and when rerun in batches, duplicate code will be generated.

**How it was caught (2026-09-24)**: In order to check 'which changes have not yet been solidified into the source code,'
I wrote a loop to run all 25 patch scripts under `patches/_merged/` at that time to see each of their outputs.
(This directory was deleted on the same day, and the patch mechanism was replaced by git commits — see `docs/DVDA-AUTHOR-CHANGES.md`; this section is retained as an incident record.)
As a result, three patches **were inserted again into the already modified source code**:

```
mlp.c   : g_last_pkt_pos declaration ×3, g_last_nb_samples statement ×3
launch_manager.c : 'AMG buffer grown for' growth block ×3
```

The first two places are direct **redefinitions** (won't compile); the third place is dead code.

### Root cause: `old not in text` cannot be used as the criterion for 'applied'

These patches are **insertive** — they insert new content **before** `old`, while `old` itself is retained:

```python
def rep(old, new, label):
    if old not in text:        # ✘ After insertion, old still exists!
        print("[MISS]")
        return False
    text = text.replace(old, new, 1)
```

So **every time it runs, it inserts another copy**. `patch_menu_amg_size.py` is more covert:
Its `NEW = OLD + growth_block`, that is, `OLD` is the **prefix** of `NEW` —
Its guard is written as `if OLD not in text: ... SKIP`, just wrapped in an extra layer,
Similarly, it will never reach the SKIP branch.

**The correct criterion is to see whether `new` is in place**:

```python
def rep(old, new, label):
    if new in text:            # ✔ Already fixed
        print("[SKIP] %s (applied)" % label)
        return True
    if old not in text:
        print("[MISS] %s" % label)
        return False
    text = text.replace(old, new, 1)
```

### How to find all non-idempotent patches

**Snapshot → Run once → Compare md5 → Restore**. At this point, the source tree is already in the 'All applied' state,
So any change in bytes implies non-idempotence:

```bash
cd tools/dvda-author-mlp8
rm -rf /tmp/snap && mkdir -p /tmp/snap
find src libutils -type f \( -name '*.c' -o -name '*.h' \) | while read -r f; do
  mkdir -p "/tmp/snap/$(dirname "$f")"; cp -p "$f" "/tmp/snap/$f"
done
for p in scripts/patches/_merged/*.py; do
  before=$(find src libutils -name '*.c' -o -name '*.h' | xargs md5sum | sort | md5sum)
  python3 "$p" >/dev/null 2>&1
  after=$(find src libutils -name '*.c' -o -name '*.h' | xargs md5sum | sort | md5sum)
  if [ "$before" != "$after" ]; then
    echo "Not idempotent: $p"
    (cd /tmp/snap && find . -type f) | while read -r f; do cp -p "/tmp/snap/$f" "$f"; done
  fi
done
```

Actually found 3 (`patch_read.py` / `patch_encode.py` / `patch_menu_amg_size.py`),
Retest after correction: **All 25 are idempotent** (running once does not produce any byte changes).

### ⚠️ Secondary Accident: Do not use `git checkout` to restore the source tree

After discovering that `launch_manager.c` was messed up, I instinctively ran `git checkout -- src/launch_manager.c`
—— **This wiped out our changes (unstaged) together**, because these changes were not in the source tree's git.

**Lesson**: The git of `tools/dvda-author-mlp8` **only tracks the upstream state**,
All the changes in this project are 'uncommitted workspace modifications'. Therefore:

- ❌ Do not use `git checkout` / `git restore` / `git stash` on the files of this tree
- ✔ Before breaking it, first `cp` a copy to `/tmp`
- ✔ When you need the 'pre-change state', rely on `SOURCE-MANIFEST.txt` + `.o` oracle (see
  `docs/DVDA-AUTHOR-CHANGES.md`）

### Use the compiled output as the 'standard answer'

The source code has been corrupted, but **the old `.o` files are still there** (`build_dvda_author_mlp.sh` clears the `.o` files every time,
So by backing up immediately after building, you can deduce:

1. Compare the **instruction sequences** (`objdump -d --no-show-raw-insn`) of the `.o` files compiled from the candidate source code with the old ones
2. Difference is 0 → functionally equivalent (comments/line numbers do not affect the instruction sequence)

Actual test: After rewriting `launch_manager.c`, the instruction sequence difference is **0**; after removing duplicate blocks in `mlp.c`
md5 **is exactly the same as the baseline** (indicating that deduplication precisely restored the original file).

---

## 20. Quick Reference for Diagnostic Methods

### Parsing PES Timestamps of AOB

```python
def parse_pts(b):
    PTS/DTS is 5 bytes: 4-bit marker + 3×(1-bit marker + 15-bit value)
    return ((((b[0] >> 1) & 0x07) << 30)
            | ((((b[1] << 8) | b[2]) >> 1) << 15)
            | (((b[3] << 8) | b[4]) >> 1))

sec = data[s * 2048:(s + 1) * 2048]
idx = sec.find(b"\x00\x00\x01\xBD", 4, 64)     # PES start code
if idx >= 0 and (sec[idx + 7] & 0x80):         # PTS present
    pts = parse_pts(sec[idx + 9:idx + 14])
```

### Pay attention to ANSI escape codes when parsing the track table in logs

Track table rows contain color codes, which must be stripped before regex matching:

```python
text = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", text)
```

Track rows have 9 fields (**easy to miscount**):

```
   Group  Title Number/Total  Track Number  First Sector  Last Sector  First_PTS  PTS_length  cga
    3      02/14       2   23635   46942      105     17935755     1
```

### Use gdb to locate crashes

```bash
gdb -batch -ex run -ex bt --args ./dvda-author-dev -g file.mlp -o out -D tmp -W -P0 -n
```

If symbols are stripped, you need to remove `-s` at link time when recompiling:

```bash
sed -i 's/ -s  dvda-author.o/ dvda-author.o/' src/Makefile
```

### Trace `strlen` for null pointers

```gdb
break strlen
commands
silent
printf "strlen arg=%p caller=%p\n", $rdi, *(void**)$rsp
continue
end
run
```

The last `arg=(nil)` in the output is the crash point.

### Determine whether the problem is with the source or the tool

Key idea: **compares the raw PCM of "source decoding" and "MLP decoding"**.

```bash
# Source → Raw PCM (requires the same resampling applied as during encoding)
ffmpeg -v quiet -i src.m4a -f s24le a.raw
# MLP → Raw PCM
ffmpeg -v quiet -i pipe.mlp -f s24le c.raw

cmp a.raw c.raw    # Match → MLP is lossless, encoding chain is faithful
```

If both match but are shorter than the declared source duration, it indicates `ffmpeg` lost data when decoding the source ——
At this point **should not rush to determine source corruption**, first verify with another criterion (see Section 5):
Check if the same file plays normally on other players.

---

## 21. ImageMagick `-repage` is a **operator**; not using parentheses will ruin the entire page.

The thumbnail grid uses "layer-by-layer `-repage +x+y` positioning, and finally `-flatten`":

```bash
magick -size 720x576 xc:black \
  \( cover1.jpg -resize 104x104^ -extent 104x104 -repage +38+5 \) "
  \( cover2.jpg -resize 104x104^ -extent 104x104 -repage +218+5 \) "
  -flatten Page.jpg
```

`-repage` **must be written inside `( )`**. It is an IM **operator**, not a setting —
if not enclosed in parentheses, it affects "each **image** in the current image list", including the first
720x576 black background image. After the black background is moved to the last slot, the canvas reveals `-flatten`'s
default **white background**:

```
Symptom: the whole page shows only one cover in the bottom right corner, the rest is all white
      (that white is not a cover, it is the default background of flatten)
      The page average is 238 (normal ~86)
```

`magick` **does not report an error**, exit code 0, you can only detect it by checking pixels:

```bash
magick Page.jpg -format "%[fx:mean*255]\n" info:      # 238 → suspicious
# Measure the cover averages per slot to find which slots are empty
for i in 0 1 2 3; do
  magick page.jpg -crop 104x104+$((38+i*180))+5 +repage "
    -format "%[fx:mean*255]\n" info:
done
```

Fell into the same trap as **twice by**, the second time was when adding the album name and wrote
`... caption:Name ) -repage +5+111` (parentheses closed too early).

Lesson from **to**: When concatenating `( )` into an IM command, `-repage` / `-rotate` / `-resize` type operators
positions must be verified character by character — compared to the way the section next to it is written, the `)` positions in both sections must be consistent.

---

## 22. The `mogrify` command string is missing a space at the end → **silently draws nothing** (returns 0)

When assembling the mogrify command, each section goes to the same buffer `strcat`, and finally `strcat` outputs
the filename. **If the previous section doesn't end with a space, they get concatenated into a single argument**:

```
mogrify +antialias -fill none -stroke "rgb(255,0,0)" -strokewidth 6 \
        -draw "rectangle 5,5 175,139""/path/hlpic0.png"
```

Thus mogrify:

* treats the last 'argument' as part of the coordinate string, **doesn't output a file**
* and writes the result to **stdout** by default (captured by the log)
* The file doesn't change a single pixel
* **exit code 0** → upper `system(...) == -1` level check lets it pass

Symptom: **Buttons or page-turn arrows on certain pages simply do not appear, while the build log is completely normal**
(Button self-check only looks up numbers in XML, cannot detect the visual outcome).

Test result: This time it hit the index page: 12 outlines and page-turn arrows were completely missing, yet album pages were normal —
Because the format string of `mogrify_img()` originally has a space at the end, the two functions I wrote missed it.

Method to check **** (don’t rely on eyesight):

```bash
# Each page has impic<N>.png (text layer) and hlpic<N>.png (highlight layer = text layer + button frame)
identify -format "%[fx:mean.a*w*h]\n" impic0.png hlpic0.png
```

The ink of `hlpic` **must be more than** `impic`. If they are equal = that command silently fails.
`02_build.py`’s `check_menu_overlay()` now makes this invariant a build-time self-check.

Lesson of ****: when you stitch `mogrify` command yourself, **leave a space at the end of each segment**;
Or simply concatenate the output file name immediately after the beginning of the command ****, don’t rely on the tail of the previous segment.

---

## 23. Subpicture only has 4 palette entries → only one color can be used per layer

Worth noting when giving menu text a “black shadow” and encountering this.

The DVD subpicture (subpicture) as a whole **only has 4 palette entries**. spumux’s
`s->pal[]` is filled according to "**each pixel of the image layer’s own color**" (see dvdauthor’s
`subgen-image.c`）：

```c
for (i = 0; i < w * h; i++) {
    if (s->fimg[i] != 255) s->pal[s->fimg[i]] = s->img.pal[s->img.img[i]];
}
...
for (j = 0; j < 4; j++) if (s->pal[j].a == 0 && s->pal[j].r == 255) {
    s->pal[j] = *p; goto if_found;      /* insert new color, up to 4 */
}
fprintf(stderr, "ERR:  Too many colors in base picture\n");
```

Consequence:

* **the same layer cannot have a second color** — either it is merged into the same palette entry
  (still looks like a single color), or it directly reports `Too many colors`;
* Moreover, dvda-author writes `impic` / `hlpic` / `slpic` for each of the three layers.
  **Up to three layers of color** (fimg occupies 3 + 1 transparent in `s->pal`).

So 'white text + black shadow' can only be **two layers with two colors**:

| Layer | Content | Color |
|---|---|---|
| Image layer `impic` | Main text | White (`DEFAULT_TEXTCOLOR_PIC`) |
| Highlight Layer `hlpic` | Underline / Button Frame / **Text Shadow** | Black (`DEFAULT_HCOLOR_PIC`) |

`_PALETTE` is the set of colors actually seen on the screen (`_PIC` is just an internal marker in dvda-author).
The two sets of default values are deliberately kept consistent in `commonvars.h`.

⚠️ `_PIC` all three **must be different from each other** — there is a check for this in `command_line_parsing.c`,
If any two items are the same, the entire set resets to the default values.

### By the way, a pitfall: changing `hlpic` in `prepare_overlay_img()` will be overwritten

The title (CD title) is baked into `svpic.png` and copied to three layers, so a shadow needs to be added next to it
Can only **append to `command1`** (the mogrify command that applies to hlpic). Directly in
Run mogrify once on `img->highlightpic[menu]` in `prepare_overlay_img()`
**invalid** —— `generate_menu_pics()` will follow immediately

```c
copy_file(img->imagepic[menu], img->highlightpic[menu], globals);
```

completely covers it. And `command1` was **before that sentence `copy_file`**
`snprintf(command, ...)` has been reset once, so additions must occur after the reset.

---

## 24. `mogrify` / `-draw` Can't draw: Several reasons already encountered

1. **command string is missing a space at the end** → The output file name is stuck to the previous segment (Section 22).
2. **`-draw` acts on a multi-layer image list** → When 'apply texture first, then draw borders', the border is
   behind `-flatten` is overridden, or it falls into the coordinate system of a specific map.
   Correct approach: **First combine into the finished file, then run `-draw`** separately.
3. **`rgba(0,0,0,alpha)` is ignored on the grayscale image** → semi-transparent white will follow
   alpha changes, but semi-transparent black is always pure black. To get a definite effect, use **opaque color**.
4. **`rgb(...)` No quotes added** → Parentheses are shell syntax characters,
   is directly `syntax error near unexpected token '('`. See section 27.

---

## 25. ImageMagick's `-stroke` is 'stuck' — all the text after it is outlined

`mogrify_thumb()` (used for the grid outline on the index page)

```c
" -fill none -stroke \"rgb(255,0,0)\" -strokewidth 6 -draw \"rectangle ...\""
```

`-stroke` **is not a one-time setting**; it will remain in the parameter stream until it is changed. And
`-draw "text"` is **drawn using both `-fill` and `-stroke`** (IM also strokes text).
So all subsequent text was given a 6 px wide red outline:

```
Symptoms: The bottom of the album page and the main text look like 'red text', but the background color is actually black.
      There are several thousand pixels measured with R>200, G<60 — but the color of the text in the source code is black
Side effects: The three-color combination of each button increases from 4 to 5 → spumux
        ERR: Cannot pick button masks → Entire menu page missing
```

**Amendment**: Explicitly write `-stroke none` before drawing each text/polygon.

```c
" -stroke none -fill \"rgb(%s)\" -font %s -pointsize %d"
" -draw \"text %u,%u '%s'\" "
```

**Lesson**: When frantically using mogrify/convert commands, **geometric settings (`-stroke`,
`-strokewidth`, `-gravity`, `-fill`) will continue to take effect**. Either each segment is complete
Specify it once, or explicitly reset it. Only 'operators' (`-draw`, `-rotate`, `-repage`)
Actions of that kind) are the ones that are one-time.

---

## 26. `snprintf` format string does not match the arguments → segmentation fault, and nothing can be seen in the logs

When adding `-stroke none` to several places in `snprintf`, **forgot to add `%s` in the format string accordingly**:

```c
snprintf(str, sizeof(str),
         " -stroke none -fill \"rgb(%s)\" -font %s -pointsize %d"
         " -draw \"text %u,%u '%s'\" ",   /* ← 6 format specifiers, but below there are 7 arguments */
         q, img->textfont, (int) img->pointsize,      /* ... only 5 were given */
         cx, cy, (int) (spy + TEXT_ARROW_H), text);   /* The extra falls onto %s */
```

Consequence: the entire parameter list is misaligned, `(int)` falls onto `%s` (integer dereferenced as pointer) →
`SIGSEGV`。

```
Phenomenon: dvda-author exit code **-11** (= killed by signal 11)
      Logs stop halfway through mplex output, ** has no ERR / Assertion messages **
      The only clue in the build log is "the last command is dvda-author"
```

**Diagnosis** (don’t rely on compilation, gcc by default does not check this):

```bash
# 1) Run the current build entry and observe dvda-author exit code (negative = signal)
dotnet run --project src/DvdaMaker.Cli -- build
# In release package: dvda build

# 2) Count each "format specifier" and "parameter" one by one to see if they match
#    Be careful, string literals are concatenated, merge adjacent "" first then count
```

Or temporarily add `-Wformat` to let the compiler check (this source code example passed thanks to it).

**Lesson**: when modifying `snprintf`’s format string, the **parameter list must also be modified**. Insert one
`"-stroke", "none"` and you must insert a `%s` at the same time.

---

## 27. Don’t confuse ImageMagick’s "settings" and "operators"

This is the type of pit that is most often fallen into, summarized here. IM parameters are divided into two types:

| Type | Example | Scope |
|---|---|---|
| **Setting** (setting) | `-stroke` `-fill` `-compose` `-gravity` `-font` `-pointsize` | **Always effective**, until changed |
| **Operator** (operator) | `-draw` `-compose` After `-composite` `-flatten` `-rotate` `-repage` `-resize` | Only acts on **the current moment's** image list |

There are three kinds of consequences:

**1. Setting residue, subsequent operations are contaminated**

```bash
magick base.png -stroke "rgb(255,0,0)" -draw "rectangle ..."   # Draw an outline rectangle
        -draw "text 10,20 'hello'"                             # ← Text is also smeared by the red outline
```
`-draw "text"` It is **drawn simultaneously with `-fill` and `-stroke`** (IM also outlines text).
Fix: Explicitly `-stroke none` before the text.

`-compose` Similarly: `-compose multiply -composite` not reset afterwards, the following
`-flatten` will also be combined according to multiply (tested: the whole page is darkened, the cover multiplies with the background).
Fix: use `-compose over`.

**2. Write the operator outside the parentheses, applying it to the entire list**.

```bash
magick xc:black \( a.jpg \) -repage +38+5 \( b.jpg \) -repage +218+5 -flatten
                         ^^^^^^^^ outside the parentheses → applies to the entire 'current list'.
```
The base color is also taken away, revealing `-flatten`'s default white background. Fix: write the operator inside `( )`.

**3. `-draw` will take effect layer by layer on multiple layers of lists**.

```bash
magick bg.jpg \( cover.jpg -repage +40+65 \) -flatten -draw "rectangle ..."
```
`-draw` will be applied to every **in the list**, while `-flatten` stacks according to the list order ——
Frames drawn on the background are covered by the cover, and frames drawn on the cover end up elsewhere due to different coordinate systems.
Fix: **first `-flatten` into the finished product, then run `-draw`** separately.

---

## 28. `rgb(...)` must be quoted in the shell.

```bash
magick -size 720x576 xc:rgb(62,107,138) ...     # ✗ syntax error near '('
magick -size 720x576 'xc:rgb(62,107,138)' ...   # ✓
```

Parentheses are shell **metacharacters**; without quotes, it’s a syntax error. This is especially easy to forget when assembling commands in C.
Forget — because `"rgb(%s)"` written this way looks like 'quoted' inside `-fill "rgb(...)"`,
but `xc:rgb(...)` is assembled into a single parameter, so it’s easy to miss.

**lesson**: any string to be included in a shell command (especially paths and colors),
should uniformly use the 'add quotes' wrapper (in this project, `cs_arg()`), and do not manually `snprintf`
until midway.

---

## 29. The return value of `system()` cannot be judged only as `-1`

```c
if (system(cmd) == -1) EXIT_ON_RUNTIME_ERROR("failed");   // ✗ Almost never triggers
```

`system()` returns the status code of `wait()`:

* `-1` — only true when **fork/exec fails** (for example, when the shell can't even start)
* `N << 8` — This is when the command **itself** fails (wrong parameter, file cannot be read)

So 'command failed' must be judged this way:

```c
int rc = system(cmd);
if (rc == -1 || !WIFEXITED(rc) || WEXITSTATUS(rc) != 0) { ... }
```

Upstream everywhere only returns `-1`, and as a result, we have encountered 'wrong `convert` parameter, silent failure, background image completely missing'
"Generate," but the log only shows the latter sentence 'Background image cannot be read' — the direction of troubleshooting was off from the very beginning.

**Lesson**: When spelling out external commands, in case of failure, **print the command exactly as it is** (in this project
`run_convert()` will output `command:` and `exit status:`), otherwise you can only guess.

---

## 30. The real reason the static image does not display: **The number of ASVS records must equal the number of titles**

This is the trickiest pitfall in this project — it was once mistakenly judged as 'the player having a defect with multiple recorded ASVS.'
Went in the wrong direction for a whole round. **Positioning method: Compare files one by one using a disc that can normally display still images.**

The comparison target is `E:\鸣潮DVD_Audio_ext` (the build from 00:57 that day, still images are normal):

| file | `TITLE_MODE=one` | `TITLE_MODE=album` |
|---|---|---|
| **`AUDIO_SV.VOB` (static image itself)** | md5 **exactly the same** | md5 **exactly the same** |
| `AUDIO_SV.IFO` `0x0D` (ASVS record count) | **1** (both disk 1/disk 2) | **28 / 17** ✅ same as disk E |
| `AUDIO_TS.IFO` `0x1000` (AMG main title) | **1** | **28 / 17** ✅ |
| `ATS_01_0.IFO` | same | same |
| real device static image | ❌ not displayed | ✅ |

**→ requirement is "ASVS record count = number of titles."** and disk E's ASVS records are **by album**
segmented (`picture_count/first_picture_number` = `2/1, 5/3, 1/8, 3/9…`), meaning one album per record.

⚠️ **key lesson: at that time the reference disk used for "comparison" was itself a `one` product.**
Taking a disk that cannot read images as a reference naturally leads to the reverse conclusion "segmenting by album will break" ****.
So changing the default to `one` actually solidified the problem. When doing A/B with **, the reference disc must be the usable one.**

### Why it was initially misjudged as a problem with the encoding parameters

In the same round, two static image-related things were also modified (encoding `-b`, cover geometry),
**Changing multiple variables at once + using the wrong reference disk** → after it broke, the blame was placed on the coding parameters,
And a note was written with the erroneous conclusion "Do not add `-b` to static images."

Later verification: under `TITLE_MODE=album`, `-q 1 -b 9800 -H` and geometry corrections
**are completely normal**, static image-related files are isomorphic with E drive.

---

## 31. `-extent` is **cropping**, not **padding**: 20% of the cover was cropped

```bash
# ✗ The comment says "centered with black borders," but actually 72px is cropped from top and bottom
magick cover.jpg -resize 720x720 -background black -gravity center \
  -extent 720x576 out.jpg
```

`-extent` is setting the canvas **to the given** size, with the orientation depending on which is larger, the image or the canvas:

* If the image **is smaller** → use `-background` **padding**
* If the image **is larger** → **cropping** (`-gravity` determines which side to keep)

A 720x720 image with `-extent 720x576` falls into the latter — 72px lost from top and bottom,
**loses 20% of the entire cover**. The docstring says "centered with black borders at 1:1",
indicating that **originally intended padding**, but the operator was used incorrectly.
Criterion: `-fuzz 3% -trim` The measured content area has always been full 720x576,
not the "narrower area with black bars on both sides."

Correct method (`make_still()`):

```bash
magick cover.jpg \
  -resize "93.75%x100%!" \                              # 1. Pixel aspect ratio compensation
  -resize 720x576 \                                     # 2. Fit into frame (no cropping)
  -background black -gravity center -extent 720x576 \   # 3. Fill black borders
  -quality 92 out.jpg
```

⚠️ Step 1 cannot be skipped: PAL 720x576 declares DAR 4:3, single pixel is **16:15**
(SAR 1.0667), during playback the picture is **stretched horizontally by 6.7%** —— a perfect circle becomes an ellipse.
Must first narrow horizontally in sample space by 1/1.0667 = 93.75%, only then will it display correctly.

**Verification method**: calculate the horizontal and vertical scaling factors from "source → display," **they must be equal to avoid distortion**.
Old code: horizontal 0.24 × 1.0667 = 0.2560, vertical 0.24 → difference 6.7% (distortion).
New code: horizontal (540/3000) × 1.0667 = 0.192, vertical 576/3000 = 0.192 ✓.
You can also use `-trim` + to adjust the brightness of the left and right borders: the new code sets the mean value of the left and right 60px = 0 (black borders).

**Lesson**: `-extent` Simultaneously taking on two opposite meanings, 'edge padding' and 'cropping';
When annotations contradict behavior, **trust actual measurements** (`-trim` measuring the content area), do not trust annotations.

---

## 32. When static image encoding does not transmit `-q`/`-b` → the clarity is stuck at the default value of 30.6 dB

`create_mpg()` originally only transmitted `-f 8 -n <norm> -a <aspect>`.
Static images are **single-frame** scenes, during playback it needs to pause the entire audio track, so 'bitrate' is meaningless for viewing experience —
The only thing determining clarity is how many bits **in total were used for that one frame**, which is decided by the default value of mpeg2enc.

The troubleshooting method is **to dismantle the pipeline and measure** at each step:

| step | PSNR |
|---|---|
| `jpeg2yuv` direct pass (excluding mpeg2enc) | 38.7 dB ← pipeline ceiling |
| then pass through mpeg2enc, default parameters | 30.6 dB |
| only scan `-q` from 12 to 1 | 30.1 → 30.8 dB (**hardly changes**) |
| `-q 1 -b 9800 -H` | **33.3 dB** |

`-q` adjusting individually is almost useless, which is evidence that 'the bottleneck is not in quantization but in **bitrate**' —
Only focusing on `-q` scanning would lead to the incorrect conclusion that 'it has already reached the limit.'

⚠️ `-b` 9800 is the **intentional specification limit**: DVD video bitrate cannot be higher. `-b 20000`,
`-b 50000`, and adding `--no-constraints` to increase the bitrate, all output **0 bytes** in actual tests
(mpeg2enc is directly rejected, and does not report an error).

`-H` (keep-hf: I-frames do not undergo high-frequency suppression) adds about 0.15 dB, which is a feature of the MPEG-2 standard.
`-K hi-res` has been tested to be equivalent to `-H`, but using a custom quantization matrix may additionally introduce compatibility risks with players.

⚠️ `create_mpg()` is used by `ANIMATEDVIDEO` (menu background) and `STILLPICS` (playback cover)
**Both paths are shared**, so the menu background also benefits (31.3 → 34.0 dB).

**Cost and Budget**: 23 → 32 sectors per frame. `AUDIO_SV.VOB` disc1 2281→3306,
disc2 1424→1960. The maximum limit for static images is 1024 sectors per track, with sufficient margin;
The alert threshold of `verify_menu.py` is **the whole disk** 4096, which is also within the range.

⚠️ Here's a counterintuitive conclusion: **increasing the JPEG quality of the intermediate image actually makes it worse**
(q90 → q100 loses about 0.7 dB) because the fixed bit budget is consumed by high-frequency quantization noise.
Actual measurement of the four levels 85/90/92/95 on 4 covers in terms of end-to-end PSNR, **92 is the best overall**
(85 only stand out on individual covers), so `make_still()` uses `-quality 92`.

---

## 33. Native Windows Port (Without WSL)

Goal: Make the toolchain directly usable on **Windows machines without WSL**,
The product is a self-contained directory (including all DLLs, exe files, fonts, and ImageMagick configuration).

### 33.1 Non-ASCII filenames in `argv` become `?`

#### Symptoms

```
[ERR]  Le terme D:/.../EP/04. ??? ?? ??.mlp n'est pas un fichier.  Fin du programme...
```

The MLP file with the Korean track name cannot be recognized, causing the entire Disc 1 build to fail.
Note **only some tracks trigger this** — Chinese and Japanese work, pure Korean does not.

#### Root cause

The `argv` obtained by a MinGW program is encoded in **ANSI (system code page)**,
Not UTF-8. On Simplified Chinese machines, the code page is **CP936 (GBK)**:

```
CP936 can represent → Chinese, Japanese Kana/Chinese characters (most of them)
CP936 cannot represent → Korean Hangul (자유로운 영혼의왕), katakana long vowel mark, etc.
```

Characters that cannot be encoded are replaced with `?`, so the file path does not reach the actual file name.

**Key Criterion**: The same MLP works normally on Linux, but reports errors on Windows
`n'est pas un fichier` — but the file clearly exists. This is an encoding issue, not a path problem.

#### Repair

Link a manifest declaring `activeCodePage=UTF-8` (resource type 24) to the exe:

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>
    </windowsSettings>
  </application>
</assembly>
```

```bash
printf '1 24 "da-utf8.manifest"\n' > da-utf8.rc
windres -i da-utf8.rc -o da-utf8.o      # Add to OBJECTS
```

Requires Windows 10 1903 or later.

#### ⚠️⚠️ Light plus `.o` **does not take effect** (the most frustrating step)

GCC, there are:

```
%{!shared:%:if-exists(default-manifest.o%s)}
```

It will automatically link the `default-manifest.o` that comes with mingw,
**appears first and overrides** our manifest (the linker takes the first one). The behavior is:
added `da-utf8.o`, compilation passed, `windres` also did not report an error, but `argv` was still broken.

patch method is to replace this default manifest with garbage:

```bash
DFM="$MSYS/lib/default-manifest.o"
[ -f "$DFM.orig" ] || cp -p "$DFM" "$DFM.orig"    # Backup (idempotent)
printf 'int __dvda_empty_default_manifest;\n' > /tmp/e.c
gcc -c /tmp/e.c -o "$DFM"
```

**verification method** (don't just look at successful compilation):

```bash
grep -c activeCodePage menu-bin/dvda-author-dev.exe    # Should be 1
```

In addition, keep the manifest content **purely ASCII** — writing Chinese in comments carries parsing risks.

### 33.2 `Could not open default file dvda-author.conf` is **false alarm**

#### phenomenon

directly by hand and then `dvda-author-dev.exe` will show:

```
[ERR]  fopen(/d/dev/winbuild/src/install/share/applications/dvda-author-dev/dvda-author.conf, "rb") crashed
[ERR]  Could not open default file dvda-author.conf
```

exit code 255. `src/lexer.c` here is `exit(-1)`, **looks** very fatal.

#### Root cause: The formal process does not read this document at all

`02_build.py` Passed **`-W`** (= `--disable-lexer`):

```python
args += ["-o", out, "-D", tmp, "-W", "-P0", "-n"]
```

`dvda-author.c` When scanning `argv`, if you encounter a `-W`, just `goto launch` directly,
**skipped `lexer_analysis()`**, meaning they didn't read conf. The ironclad evidence in the logs is:

```
[PAR]  Lexer was deactivated
```

**I was misled by this error for a**: when manually testing exe, I missed `-W`,
thought the build would fail too. In fact, the Linux version reported the same 255—
In other words, this error **is never the reason for** build failure.

#### Lessons

> When the single-test command and the production call **parameters are inconsistent,** failure conclusion is unreliable.
> First, extract the complete production command line from the log and copy it verbatim.

#### Incidentally: `install/` The directory has been deleted

The `INSTALL_CONF_DIR` compiled into binary comes from the `SHORTLINKDIR` of configurable
(can be overwritten by `--with-config=DIR`). The Linux side `install/` was once cleaned up,
So **running directly** `src/dvda-author-dev` would crash — but the build was unaffected
(with `-W`). Placing another copy at the expected path also makes manual invocation work.

There's a potential pitfall: the path in Windows binaries is **MSYS style**
(`/d/dev/winbuild/...`), MinGW program is unknown.
So **never run the exe manually without `-W`**.

### 33.3 ImageMagick 7 does not have a standalone `identify.exe`

#### Symptoms

Disc 1 **all 91 tracks are fully encoded, and the menu is also generated**, but then it crashed at the self-check step:

```
FileNotFoundError: [WinError 2] The system cannot find the specified file.
  File "02_build.py", line 774, in ink
    r = _sp.run(["identify", "-format", "%[fx:mean.a*w*h]", path], ...)
```

#### Root cause

ImageMagick 7 merges all tools into a single `magick`.
`identify` becomes a **subcommand** (`magick identify`).
The Windows distribution directory we use only contains `magick.exe` / `convert.exe` / `mogrify.exe`.

```
Linux  /usr/bin/identify            ← Standalone executable of IM6/7
Windows menu-bin/magick.exe         ← IM7, no identify.exe
```

**Why it took so long to notice**: the error occurs at a point **late** in the process,
I did over 90 tracks of coding for nothing.

#### Repair

Added `dvda_config.magick_identify_cmd()`, which returns a **list of command prefixes**:

```python
exe = shutil.which("identify")
if exe:
    return [exe]
exe = shutil.which("magick")
if exe:
    return [exe, "identify"]
raise RuntimeError(...)
```

Three call sites should use it: `check_menu_overlay()` in `02_build.py` (2 places)
With `menu_assets.image_size()`. Returns the **absolute path** of `which()`,
Avoid the quirks of `CreateProcess` on Windows and `set PATH` in the cmd line
(`set PATH=X&& exe` does not work **on the same line**, it can lead to incorrect judgment during testing).

### 33.4 Error handling covered up the real error itself

The `run()` in `02_build.py` returns a lightweight object when `log_output=True`:

```python
return type("R", (), {"returncode": r.returncode})()      # ✘
```

But the failure branch needs to print debug information:

```python
for ln in (r.stdout or "")[-600:].splitlines()[-3:]:
```

→ `AttributeError: 'R' object has no attribute 'stdout'`，
**Thoroughly bury the real reason for failure**. The amendment is to add this field:

```python
return type("R", (), {"returncode": r.returncode, "stdout": text})()   # ✔
```

> Lesson: **Code on the wrong path will not be tested by the normal flow**.
> Once it goes wrong, what you lose is exactly the only clue to locating the problem.

### 33.5 Other Windows Adaptation Checklist

| Position | Issue | Handling |
|---|---|---|
| `dvda_config` | Windows Python outputs stdout in GBK encoding, printing Korean immediately causes `UnicodeEncodeError` (**crashes midway, like it hung**) | `_fix_console_encoding()`: `reconfigure(encoding="utf-8")` |
| `02_build` | The menu program name has no extension, `which()` cannot find it | `_menu_exe()` adds `.exe`; `_prepend_to_path()` inserts bindir at the front of PATH |
| `menu_assets` / `command_line_parsing.c` | `--stillpics` split the inter-track path by `:`, **Windows paths themselves contain `:`** (`D:`) → drive letters get fragmented | on Windows using `;` (illegal characters in the path); Linux keeps `:` unchanged byte by byte |
| `winport.h` | `truncate`/`ftruncate` missing declaration | MinGW's `<unistd.h>` real implementation provided by libmingwex |
| `winport.c` | `_setmode(_fileno(g_hChildStd_IN_wr), _O_BINARY)` | HANDLE* is not FILE*; `CreatePipe` + `WriteFile` inherently bypass CRT stream conversion |
| `c_utils.h` | `WIFEXITED`/`WEXITSTATUS` missing | MSVCRT's `system()` **directly returns the exit code**, unlike Linux returning wait status |
| `c_utils.h` | `getsubopt()` missing | supplement glibc equivalent implementation (**does not modify** input string, compare with length) |
| `fixwav_auxiliary` | `isok()` declared without a prototype, defined with a `globals` parameter, callers all pass 0 arguments | C99's 'no prototype' hides it, **C23 (GCC 14+) directly reports type conflict** → unify to `isok(void)` |
| `compat.h` (dvdauthor) | `mkdir`/`fsync` missing | add `da_mkdir`/`da_fsync` **shim function**. ⚠️ `mkdir` **cannot use macro** — `<io.h>` declaration only has 1 parameter, will report `macro 'mkdir' requires 2 arguments` |
| `compat.h` | `htonl` family, `bzero`/`bcopy` missing | `__builtin_bswap32/16` (zero dependency), `memset`/`memmove` macro |
| `compat.c` | `nl_langinfo(CODESET)` missing | `snprintf(buf, size, "CP%u", GetACP())` |
| `dvdauthor` | `mpeg2desc` cannot be compiled | Use `select()` to monitor stdin + files, **Win32 select only supports sockets** → skip ( `dvdauthor`/`spumux` do not need it) |

### 33.6 three pitfalls of the build environment

1. **`configure` will clear `local/`** (line 4288 of configure)
   `rm -rf "local" && mkdir "local"`) → FFmpeg import library/header files
   must be copied in after configure ****.
2.**`PKG_CONFIG_PATH` must explicitly give **— MSYS2 pkg-config only searches
   `/usr/lib/pkgconfig`, could not find mingw64 `.pc`:
   `PKG_CONFIG_PATH=/mingw64/lib/pkgconfig:/mingw64/share/pkgconfig`
3.** make target names containing `-C src` and `.exe`**:
   `make -C src dvdauthor.exe spumux.exe spuunmux.exe`
   (`util_make` That set of target names does not exist on Windows)

The only way to call MSYS2's** from WSL **:

```bash
/mnt/d/dev/msys64/usr/bin/bash.exe -lc 'export MSYSTEM=MINGW64; export PATH=/mingw64/bin:$PATH; <cmd>'
```

- ✗ `MSYSTEM=MINGW64 bash.exe -lc '...'` —— PATH not set up
- ✗ `mingw64.exe -lc '...'` —— When calling from WSL **no output at all**

### 33.7 Install ImageMagick in MSYS2: **output differs from Linux**

Incidentally installed MSYS2’s `mingw-w64-x86_64-imagemagick` (7.1.2-31),
same series as Linux (7.1.2-18), to see if the outputs can be aligned byte-for-byte.

Same cover, same commands, three implementations:

| implementation | byte count | md5 |
|---|---|---|
| Linux IM 7.1.2-18 **Q16** | 115334 | `13a65eb8…` |
| old `local.w10` IM 7.0.8-47 **Q16** | 115422 | `427a46b8…` |
| MSYS2 IM 7.1.2-31 **Q16-HDRI** | 115422 | `427a46b8…` |

Two Windows **implementations are identical byte-for-byte, both different from Linux** ——
showing that the differences come from the **platform** (libjpeg/floating point path), not the IM version.
Changing IM gave no benefit, so keeping the originally used `local.w10` version.

#### Should this difference matter: **It doesn't matter** (quantified)

File-by-file comparison between Windows output and authoritative reference disc:

| file | result |
|---|---|
| ** `ATS_01_*.AOB` (audio content, 8 files) **|** identical byte by byte **✔ |
|**`ATS_01_0.IFO` / `.BUP` (including ATSI: all track tables + still image tables)**|**Byte-for-byte identical**✔ |
| `AUDIO_PP.IFO` / `AUDIO_SV.BUP` / `AUDIO_TS.BUP` | same size, different content |
| `AUDIO_SV.VOB` (still image) + `AUDIO_TS.VOB` (menu) | tens of KB |

All structural fields match: ASVS record count 28/17, number of images per segment, starting image number, total still images 91/56,
AMG main title 28/17, video attributes `0x53`, buttons `0x19=0`, ATSI record count.

Only in `AUDIO_SV.VOB` `base_sect` (starting image sector number per segment** **) changes with still image encoding size,** image count and starting image number are completely consistent ** — meaning the player's positioning logic is unaffected.

Pixel differences after decoding still images quantified:

```
Max absolute difference = 18/255      Average absolute difference = 0.24
RMSE = 0.783             PSNR = 50.26 dB
```

50 dB is considered 'indistinguishable to the naked eye', the difference is due to internal rounding by the JPEG encoder.

### 33.8 Release directory layout

A self-contained directory, can be used on any Windows machine if copied:

```
DVD-Audio-Maker\
  dvda.cmd              Launcher (set tool path + UTF-8 console, then call python)
  README.md / THIRD-PARTY.md / LICENSE / MANIFEST.txt
  scripts\              .py + distribution config.sh (only keep the items the user needs to modify)
  menu-bin\             12 exe + 100 dll + 11 IM configuration xml
    fonts\              NotoSansCJK-Regular.ttc
  data\menu\            Material directory for dvda-author (silence.wav, activeheader)
```

#### Why use a launcher instead of writing paths into `config.sh`

`dvda_config` priority is **environment variable > config.sh > built-in default value**,
so the launcher sets tool paths as environment variables, leaving only what the user really wants to change in `config.sh`
(audio source, output directory). The whole directory can then be moved freely with ****.

#### ⚠️ Installation path must not contain spaces

`dvda-author` when composing `mogrify` commands **does not quote paths**
(`" -font %s -pointsize %d"`) — directories with spaces will be split by command parsing,
font and material paths will fail together, manifesting as **menu text appearing empty** (no error reported).

#### ⚠️ `.cmd` / `.ps1` must be pure ASCII

`cmd.exe` reads script files according to the OEM code page, in PowerShell 5.1 it reads according to ANSI.
UTF-8 Chinese comments will cause the parser to report a syntax error. There is an automatic check in the release script:

```bash
LC_ALL=C grep -q '[^ -~]' dvda.cmd && echo "[Warning] Contains non-ASCII characters"
```

#### Fonts must be included

On Windows systems **there is no** font that can independently cover Chinese, Japanese, and Korean languages:

| font | Chinese | Japanese | Korean |
|---|---|---|---|
| NotoSansSC-VF | ✔ | △ | ✘ |
| NotoSansJP-VF | △ | ✔ | ✘ |
| msyh.ttc | ✔ | △ | ✘ |
| malgun.ttf | ✘ | ✘ | ✔ |
| **NotoSansCJK-Regular.ttc** | ✔ | ✔ | ✔ |

And our disc titles are in all four languages (Chinese/Japanese/English/Korean versions of the same song),
so `NotoSansCJK-Regular.ttc` must be included with the package (18.6 MB, SIL OFL 1.1 allows redistribution).

What is passed to `dvda-author` is **the full path of the font file** (`--fontname D:/…/NotoSansCJK-Regular.ttc`),
ImageMagick can load any `.ttc` by path, without being restricted by the system font list
(the Windows IM font list only has `Noto-Sans-SC`/`Noto-Sans-JP`, and does not include Hangul).

---

## 34. The Chinese menu uses **Japanese character forms** + font override detection fails completely

Two independent defects occur together. **does not report an error**, it is only visible on the screen.

### 34.1 Symptoms

The way Chinese characters like "直 / 骨 / 令 / 次" are written on the menu is different from expected (more Japanese-style),
Building the same disk on the Linux side is also correct.

### 34.2 Root Cause 1: `.ttc` collection + file path = get **JP face**

`NotoSansCJK-Regular.ttc` contains **10 faces**:

```
[0] Noto Sans CJK JP      ← This is what ImageMagick loads when using the file path
[1] Noto Sans CJK KR
[2] Noto Sans CJK SC      ← This is what we want
[3] Noto Sans CJK TC
[4] Noto Sans CJK HK
[5..9] Noto Sans Mono CJK {JP,KR,SC,TC,HK}
```

**ImageMagick only supports two ways to specify: 'by file path' or 'by family name', both have pitfalls**:

| Method | Linux | Windows |
|---|---|---|
| Family name `Noto-Sans-CJK-SC` | ✔ Correctly maps to SC via fontconfig | ✘ `UnableToReadFont` (These CJK names are not in `type.xml` of IM 7.0.8) |
| File path `.../NotoSansCJK-Regular.ttc` | ✘ Also picks face 0 | ✘ Picking face 0 = **JP** |
| `...ttc[2]` / `...:index=2` | ✘ `UnableToReadFont` | ✘ `UnableToReadFont` |

**Empirical evidence** (same IM, same file, only changing face specification) — exact pixel-by-pixel character count:

| Character | By path (face 0) | SC face | JP face |
|---|---|---|---|
| Straight | **1341** | 1363 | **1341** |
| Bone | **1244** | 1320 | **1244** |
| Order | 901 | 846 | 906 |
| Time | 957 | 962 | 958 |
| Other | 1168 | 1167 | 1167 |

Match the value by path exactly with **JP** → Confirm that JP is taken.

**Fix method**: Extract SC face into **single face file**, so that 'load by path' is unambiguous.

```
bash make-menu-font.sh          # Generate NotoSansCJKsc-Regular.otf (16.4 MB)
```

Currently, within the repository pure C# `DvdaMaker.FontTool` locate the face by **family name**
(more stable than hard-coded index, which may change with Noto versions), and rebuild standalone OpenType,
Fix table offsets and checksums.
After extraction, recheck character by character: **5/5 exactly matches SC**.

> ⚠️ Do not hardcode face 2 in the extraction tool — it should parse the `name` table and match
> family `Noto Sans CJK SC`。

### 34.3 Root cause 2: Backslash font paths are silently swallowed by ImageMagick ****When `-font` is given a** backslash ** path:

```
UnableToReadFont `D:devwinbuildmenu-binfontsNotoSansCJKsc-Regular.otf'
                   ↑ All backslashes disappear
```

The same file works fine using **forward slashes**:

| font value | probe `Ag(` | probe `汉字` |
|---|---|---|
| `D:\dev\...\NotoSansCJKsc-Regular.otf` | 0.021322 ← **default font** | **0** |
| `D:/dev/.../NotoSansCJKsc-Regular.otf` | 0.0204667 | **0.0249964** ✔ |

**This is the trickiest part**: ASCII probes still leave marks using ImageMagick's **default font**
It seems like “the font is usable”; so `font_coverage()` reports for any backslash path
“only ASCII, missing Chinese/Kana/Korean” — no errors along the way, just blank spaces in the image.

**fix**: unify through a `menu_assets.font_spec()`, on Windows replace `\` with `/`.
Needed in two places: `_ink()` (detection) and `MenuPlan.args()` (passed to `--fontname`,
because `dvda-author` incorporates it into the mogrify command and will be affected as well).

### 34.4 Additional: Probe characters on Windows cannot go through argv

`magick.exe` is a 2019 build (**does not have a UTF-8 manifest**), the argv obtained is
ANSI code page — The Chinese characters/Kanji/Hangul in the probe turn into `?` before reaching ImageMagick.
This has a homology problem on `dvda-author` (see §33.1).

Fix: On Windows, write the probe text into a temporary file, and read it with `-annotate @file`
(IM supported; Linux IM security policy prohibits `@`, so it is only done on Windows).
Reliably confirmed using **Private Use Area characters** (U+E000, not available in any font) for comparison:

```
Chinese characters (Zhigu Ling) = 452.895
Private Area (Reference) = 0.0        ← Confirmed the characters were drawn correctly
```

### 34.5 ★ Lesson: **probes without a negative control will lie**

This bug took an entire cycle to locate, because the detection method had two layers of "ways to get by unnoticed" passages:

1. ASCII probe → fallback to default font → has ink marks → seems like the method works
2. All other character sets are 0 → interpreted as 'missing font characters' (a reasonable conclusion), rather than
   'The font didn't load at all'

The real criterion is that **U+E000 private use area correspondence**: no font can have its glyph.
, so its amount of ink is the benchmark for 'unable to draw'. With it, the two hypotheses can be immediately distinguished:

- font missing characters → Private use area is also 0, but CJK should be **not 0**
- Font not loaded → Private area 0 **and** CJK also 0

**For all detections of “Using A works normally ⇒ method is reliable,” a negative control of “A must fail” must be included.**
(This is isomorphic to the lesson from §30: if the benchmark is wrong, the conclusion will be entirely reversed.)

### 34.6 What is packaged now

| | |
|---|---|
| File | `menu-bin/fonts/NotoSansCJKsc-Regular.otf` (single face SC, 16.4 MB) |
| Generate | `make-menu-font.sh` (repeatable; with single face + family + four-language encoding table self-check) |
| License | Noto Sans CJK, SIL OFL 1.1 (redistribution allowed) |
| Four-language coverage | ASCII / Han characters / Kana / Hangul / CJK punctuation — verified **all have ink marks** |

**Not** using `NotoSansCJK-VF.otf.ttc` (variable font collection):
It is also a collection (face 0 = JP), and its default instance, as tested **, has strokes only about static Regular
1/2.4** (same character and same size ink marks 63.4 vs 153.1), menu text will appear noticeably thinner.
Its four-script coverage is fine, but not suitable for current use.

### 34.7 Incidentally, guardrails for the configuration side

`menu_assets.pick_font()` Now it will proactively warn for `.ttc` ending `DVDA_MENU_FONT`:
Collection font + file path = will definitely select face 0 (JP), Chinese characters will change to Japanese glyphs.
This type of defect does not cause runtime errors and can only be indicated by guardrails.

---

## 35. Assign font face according to language (use JP for Japanese, KR for Korean)

Section 34 changed `.ttc` to **single-face SC**, resolving the issue of 'Chinese using Japanese glyphs'.
But that only did half of it correctly — SC single face will make **Japanese** titles display in Simplified Chinese characters.
In this section, change it to select faces item by item according to **text content**.

### 35.1 First measure 'which faces are needed', then start working

Each face of Noto Sans CJK **contains** the Chinese, Japanese, Korean, and Latin character sets,
So this is **not** a 'missing character' problem, but rather a problem of **regional variant forms of the same set of Chinese characters**.
(Straight / Bone / Order / Next / Separate... different writings).

Render the **actual song title** of this project and compare it pixel by pixel (render onto a fixed canvas and then compare the grayscale raw data):

| Group | Number of Tracks | SC vs JP | SC vs KR | JP vs KR |
|---|---|---|---|---|
| Japanese (including kana) | 9 | **Up to 1464 px** | Same magnitude | 0–309 |
| **Korean (Hangul)** | 9 | **0** | **0** | **0** |
| Chinese (pure Chinese characters) | 99 | **up to 2624 px** | same level | 243–706 |

Two conclusions directly determine the implementation:

1. **Chinese must be SC, Japanese must be JP** —— using wrong one causes glyph errors (and no error is reported)
2. **Whichever Korean face is used, the output is pixel-for-pixel identical** —— Hangul in Noto CJK is consistent across faces
   **There are no** regional variants (unlike Chinese characters). So KR in this project is a no-op,
   but the mechanism is implemented anyway (symmetrically, and it's only useful if mixed North Korean-Chinese characters appear in Korean titles in the future)

Also, the script distribution was counted: 147 in total **Japanese 9 / Korean 9 / Chinese 99**,
**None of the Japanese and Korean are mixed in one piece** —— judgment order is unambiguous (in implementation "Hangul takes precedence over Kana").

### 35.2 Feasibility: switch fonts within the same command

First, confirm that the architecture allows it. Each page of the menu only runs **2 `mogrify` times** (base layer `impic` +
highlight layer `hlpic`), while there are three character drawing paths:

| Position | What to draw |
|---|---|
| `append_shadow()` | Shadow/outline of text (each layer once, different colors) |
| `mogrify_img()` of `str2` | text **character body** |
| `prepare_overlay_img()` | Album title (first line of secondary menu) |

**Each of the three lines resend once `-font img->textfont`**, while ImageMagick's `-font`
is **persistent setting** — so as long as you change the font values of these three places to 'Select by text content',
you can switch within the same command, **without having to split the command or change the layout**.

### 35.3 Implementation

**Side**

```c
/* menu.c: Select face by content (Hangul preferred over Kana); returns default when both fields are NULL */
static const char *textfont_for(const char *text, const pic *img);
static unsigned int utf8_next(const char **p);   /* written by oneself, only recognizes 4 lengths */
```

- `structures.h` of `pic` **Append to the end** `char* textfont_jp; char* textfont_kr;`
  — ⚠️ Must be at the end: `dvda-author.c` in `pic img0 = {1, 0, 0, ...}` is **initialized by position**, inserting in the middle will misalign all following fields
- `command_line_parsing.c` New `--fontname-jp`(45) / `--fontname-kr`(46)
  (option id 44 not occupied afterwards)

**Python side**

- `DVDA_MENU_FONT_JP` / `DVDA_MENU_FONT_KR` (leave empty to let **automatically infer**)
- `menu_assets._other_face(font, tag)` Named according to **family rules**, both forms are supported:

  ```
  File path (Windows release package)  NotoSansCJKsc-Regular.otf  ->  ...jp-Regular.otf
  Family name (Linux + fontconfig) Noto-Sans-CJK-SC           ->  Noto-Sans-CJK-JP
  ```

If neither of these two options is given, the C side will fully fall back to `--fontname` — the old behavior of **remains completely unchanged**.

### 35.4 Differential verification (the hardest step in this section)

Render the same page three times, **compares lines of text by** in pixels:

```
A = only --fontname (all SC)
B = --fontname + --fontname-jp/kr (assignment)
C = only --fontname but provided JP (all JP)
```

Line baseline is back-calculated from `menu.c`’s `y()` (`maxbuttons=3` → `maxnumtracks=7`),
`labelheight=(576-56-40-7*12)/7=56`）：

```
y = 56 + t*(56+12) + 28  ->  84 (Title) / 152 (Track 1) / 220 (Track 2) / 288 (Track 3)
```

| line | A vs B | A vs C | B vs C | determine |
|---|---|---|---|---|
| Album Title (ASCII) | 0 | 0 | 0 | OK |
| **Track 1 Chinese** | **0** | 192 | 192 | OK ← Still use SC under assignment |
| **Track 2 Japanese** | **118** | 118 | **0** | OK ← Under assignment, same as entire JP |
| **Track 3 Korean** | 0 | 0 | 0 | OK |

**100% Meets expectations** —— This proves that Japanese switching is effective and that Chinese **has not been switched**
(Just seeing "Japanese changed" is not enough, we must also confirm that other languages were not mistakenly affected).

Cross-check: the same `mogrify` command appears **9 times `-font` in the log, after deduplication
sc / jp / kr three faces are all in**; the three `[PAR] Fontname...` lines are all printed correctly.

### 35.5 Also fixed the script bug by the way

**`da-utf8.o` Cannot be added into `OBJECTS`** (Manifest of section 33). In the Makefile:

```make
$(OBJECTS): %.o: $(ROOT)/src/%.c
```

make **merges multiple rules for the same target**, so `da-utf8.o` appears one extra time
Prerequisite for `da-utf8.c`:

```
make[1]: *** No rule to make target '.../src/da-utf8.c', needed by 'da-utf8.o'
```

⚠️ Moreover, **the compile error count is 0** (`grep -c "error:"`), which can easily be misjudged as
Confused by 'No errors reported but no output'.

The correct way to connect it is to use it as a **linking premise**:

```make
dvda-author: da-utf8.o
```

`dvda-author` does not have an explicit recipe; it uses the built-in make rule `%: %.o`.
It takes **`$^` (all prerequisites)** and concatenates them into the link command — adding a prerequisite is equivalent to adding a `.o`
Add it to the link line and completely bypass that static mode rule.

Similar pitfall: **MSYS2 treats `foo` and `foo.exe` as the same file**, so
`mv -f dvda-author-dev dvda-author-dev.exe` will report
"'For the same file' is not a zero exit, and being caught by `set -e` causes the entire build to fail."
(And the product was actually ready a long time ago). Check before renaming
`[ "$src" -ef "$TARGET" ]`。

### 35.6 Lesson

- **Measure first, act later**: Before taking action, use real data to determine 'which language really needs which face'.
  The result directly overturned the intuition of 'wanting all three' — Korean is actually a no-op.
- **Verification must be able to prove 'not mistakenly harmed'**: Just testing 'the Japanese changed' is not enough; it must be confirmed simultaneously
  Chinese line **remains pixel-perfect**. Only single-variable comparison (A/B/C three groups) can rule out luck.
- **When modifying header files, you must clear `.o`**: `pic` Layout has changed, and old object files can cause odd behavior.
- **Build artifacts need to be synchronized to the directory that uses them**: artifacts are in `src/src/`, but the test script uses
  `menu-bin/` a copy inside —— I therefore ran an extra round for nothing (the log only printed `Fontname:`,
  no `Fontname (JP):`, and exit code 4294967295).

### 35.7 ★ Additional note: `--fontname-jp/-kr` passed **backslash** path → silently fallback to default font.

The first time the implementation in this section ran a full build, **stepped on its own trap**: in the log

```
--fontname     D:/dev/.../fonts/NotoSansCJKsc-Regular.otf     ← forward slashes
--fontname-jp  D:\dev\...\fonts\NotoSansCJKjp-Regular.otf     ← backslashes!
--fontname-kr  D:\dev\...\fonts\NotoSansCJKkr-Regular.otf     ← backslashes!
```

And 34.3 has already proven: **ImageMagick will remove all backslashes in paths with backslashes**,

```
UnableToReadFont `D:devwinbuildeleaseDVD-Audio-Makermenu-binfontsNotoSansCJKjp-Regular.otf'
```

so the Japanese/Korean lines fall back to **default font** (likely missing characters), **but the build still succeeds**
(exit code 0, log with no errors) — completely consistent with the note in section 34 about 'only visible on the screen.'

#### Root Cause: Two overlapping issues

1. `_other_face()` Use `os.path.join()` to assemble paths. On Windows
   `os.path.join("D:/x/fonts", "a.otf")` a **`D:/x/fonts\a.otf`** is given
   — note it is **mixed separators**, first half forward slash, second half backslash. During manual review
   it is not easy to spot the problem.
2. `MenuPlan.args()` only adjusted `font_spec()` for the **main font** (fix added in 34.3),
   the newly added `font_jp` / `font_kr` was directly inserted into the parameter table — missed.

#### Law amendment: closing + guardrail

```python
# args(): all font values passed to ImageMagick must go through font_spec()
def _f(v):
    return font_spec(v) if v else v

if fontname or self.font:
    a += ["--fontname", _f(fontname or self.font)]
if self.font_jp:
    a += ["--fontname-jp", _f(self.font_jp)]
if self.font_kr:
    a += ["--fontname-kr", _f(self.font_kr)]

# Guardrail: font values cannot contain backslashes, otherwise execution is directly aborted
for k, v in zip(a, a[1:]):
    if k.startswith("--fontname") and "\\" in v:
        raise RuntimeError(...)
```

The return value of `_other_face()` also goes through `font_spec()` (double protection).

#### Lesson

- Similar fixes for **must be done in one go**: 34.3 only fixed the call point that existed at that time,
  newly added parameters were missed. **should either use a unified helper or add assertions** —
  relying on "remember to check every call point" will inevitably miss some.
- **`os.path.join()` produces backslashes on Windows**, and combined with existing forward slashes
  forms **mixed separators**. After cross-platform path assembly, always normalize explicitly when passing to external tools.
- **These kinds of defects do not cause runtime errors**: precisely enough to make it worth adding a **build-time abort** safeguard
  (Otherwise, the only way to discover it is to view the screen on the actual device).
- Discovery path: Read the full command line of `dvda-author` in the build log and compare the three
  The **value** of `--fontname*`. **Just looking at whether the parameter was passed is not enough; you need to look at the form of the value.**

---

## 36. Windows Native Build Tools (without WSL)

Section 33 gets the build **running** on MSYS2, but the orchestration scripts still rely on WSL (hard-coded
`/d/dev/winbuild/...` log path, take `.py` from `/home/yyz57/dvda/scripts`).
This section also makes the arrangement native to Windows: a **position-independent, zero WSL call** toolkit
`tools/win-build/`。

The latter half of this section (starting from 36.4) records almost entirely defects of **'no error reported, but wrong results'** —
First, list all the ones that have been stepped on here, because they share a common form, and the diagnostic methods can be reused.

### 36.1 First find out where there is actually a dependency on WSL

| Session | Depends on WSL |
|---|---|
| Compile dvda-author | ✘ MSYS2 is already sufficient |
| Compile dvdauthor / spumux | ✘ Same as above |
| Runtime materials `menu/` | ✘ Already included with the source tree |
| Precompiled binaries `local.w10/bin/` | ✘ Already included with the source tree |
| Fonts | ✘ As long as there is one static `.ttc`, it has nothing to do with WSL |
| **Orchestration scripts** | ✔ **Hardcoded log paths, taken from WSL directories `.py`** |

So what needs to be done **is rewriting the orchestration**, not changing the build itself.

> **Incidentally correct a concept**: MSYS2's `bash.exe` is **a real Windows program**
> (PE header `4D 5A`, `uname -s` reports `MSYS_NT-10.0-26200`, `uname -o` reports `Msys`),
> Not `Linux`. It is just **a syntax interpreter**, every tool it calls (`gcc.exe`, `make.exe`,
> `windres.exe`) is a Windows program, and the output is also PE.
>
> So "`.sh` cannot run on Windows" is not valid — the accurate statement is
> **`.sh` Needs a POSIX shell to run**, `.bat` needs `cmd.exe`, `.py` needs
> `python.exe`, and MSYS2 happens to provide `bash.exe`. **upstream dvda-author is
> autotools** (`configure` itself is a 200,000-line `/bin/sh` script),
> running autotools on Windows requires a POSIX shell — which is also the path recommended by the upstream's own
> `BUILD.MSYS2` documentation.
>
> And the **release package doesn’t need any shell**: it only contains `.exe` / `.dll` / `.py` / `.otf`,
> the entry `dvda.cmd` is native batch.

### 36.2 Tool package design (two principles)

1. **is location-independent**: all paths are derived from `common.sh`'s own location.
   The entire package can be placed in any directory (`D:\build`, USB drive…), without modifying scripts.
2. **zero WSL invocation**: only depends on MSYS2. The script does not include
   `wsl.exe` / `\\wsl.localhost` / `/mnt/c`。

When detecting MSYS2, it should check the **prefix** rather than guessing the installation root:

```bash
for pfx in /mingw64 /clang64 /ucrt64 /mingw32 /clang32; do
    [ -d "$pfx/lib/pkgconfig" ] && { echo "$pfx"; return 0; }
done
```

MSYS2 under bash, `/` is the installation root, and `/mingw64`, `/ucrt64`, `/clang64` are its
different toolchain variants — detecting by prefix is more reliable.

### 36.3 Two Windows details of build-all.bat

1. **8.3 short path removes spaces**: `set "KITS=%~sdp0"`. When there are spaces in the directory name
   `bash.exe -lc "cd /c/My Dir && ..."` quotes can easily be broken;
   short path ensures no spaces, so bash calls can be written very simply.
2. **environment variables automatically inherited**: `DVDA_SRC_TREE` / `DVDA_FONT_SRC` / `DVDA_SCRIPTS` /
   `MSYS2_ROOT` is automatically passed from cmd to the child process, `.bat` inside **does not need** to explicitly retransmit it again
   (a trap of missing one layer of quote escaping). Their values can be in Windows form (`D:\x\y`),
   bash side unified conversion.

### 36.4 False negative one: `lib` The prefix was typed twice

```bash
for h in libavcodec libavformat ...          # $h already contains "lib"
    [ -f "$MSYS/lib/lib$h.dll.a" ]           # → liblibavcodec.dll.a ✗
```

medical examination therefore reported 'FFmpeg not installed', and `pkg-config --modversion libavcodec`
Clearly has output. **The same logic in the old `build.sh` is correct** (written over there
`for l in avcodec ...`) — I got the meanings of the variables mixed up while copying.

### 36.5 False Negative #2: Double slashes are treated as a UNC path

```bash
MSYS_ROOT="$(cd "$MSYS/.." && pwd)"   # -> "/"
MSYS="$MSYS_ROOT/mingw64"             # -> "//mingw64"
```

**MSYS2 treats `//` as a UNC/network path**, so all `[ -f //mingw64/... ]`
**are both invalid**. Actual measurement comparison:

```
[ -e //usr/bin/grep ]    -> false
[ -e ///usr/bin/grep ]   -> True      ← Note /// actually works!
[ -d /mingw64/include/libavcodec ]   -> true
```

`///` will be folded back `/` therefore **can use**, this determines the stealthiness of this bug:
It **will not report an error**, it just makes `[ -f ]` and PATH lookup quietly fail.

symptoms are exactly the same as 36.4 **** (reported FFmpeg missing), so the first repair still failed,
looks like 'the fix didn't work' — in fact, it is two separate bugs, **, stacked on the same symptom**.

modification method introduces `MSYSBASE`, which removes the trailing slash, and **only uses it** to construct paths:

```bash
MSYSBASE="${MSYS_ROOT%/}"      # Empty string if the root is "/"
MSYS="${MSYSBASE}/$(basename "$MSYS")"
export PATH="$MSYS/bin:$MSYSBASE/usr/bin:$PATH"
# At this time, "$MSYSBASE/usr/bin" happens to be "/usr/bin"
```

and add **self-check directly at the `common.sh` entrance `exit 1`**:

```bash
if [ ! -d "$MSYSBASE/usr/bin" ] || [ ! -d "$MSYS/lib/pkgconfig" ]; then
    echo "[Failed] MSYS2 path concatenation exception, refusing to continue:" >&2
    echo "        MSYS_ROOT='$MSYS_ROOT'  MSYSBASE='$MSYSBASE'  MSYS='$MSYS'" >&2
    exit 1
fi
```

> is worth adding this check because defects like **will not cause the build to fail**: the compilation output is still the same
> 'Error count 0' 'BUILD OK', but the behavior is quietly wrong. It's better to stop directly at the entry point.

### 36.6 False Negative No. 3: `cygpath -u` Compress multi-level path into root

This is the sneakiest one of this round. `to_unix()` Originally used `cygpath -u` to
`D:\x\y` to MSYS path. Tested (`MSYSTEM=MINGW64`):

```
cygpath -u 'D:\dev\msys64'   ->  /                 ← Wrong!
cygpath -m 'D:\dev\msys64'   ->  D:/dev/msys64     ← Correct
cygpath -u 'D:\'             ->  /d/               ← Single level actually correct
```

**Multi-level paths were compressed into `/`.** Therefore:

```
to_unix("D:\dev\msys64") -> "/"
  → [ -d "$MSYS2_ROOT" ] evaluates as true for "/"
  → The branch "specify MSYS2_ROOT" degrades entirely to "detect mingw64 under /"
  → MSYS_ROOT becomes "/"
  → In the subscript "$MSYS_ROOT/ucrt64" becomes "//ucrt64"
  → Collecting DLLs fails to find msys-2.0.dll
```

**No errors occurred throughout**, it’s just that the "specify MSYS2_ROOT" feature silently doesn’t work,
and because `/mingw64` happens to be detectable, the build can still proceed.

The fix is **no longer uses `cygpath`**, instead using pure string conversion: certain, and zero fork.

```bash
to_unix() {
    local p="${1:-}" d rest
    [ -n "$p" ] || { printf ''; return 0; }
    case "$p" in /*) printf '%s' "$p"; return 0 ;; esac
    p="${p#\\\\?\\}"
    case "$p" in
        [A-Za-z]:*)
            d="${p:0:1}"; d="${d,,}"      # ${var,,} lowercases, zero fork
            rest="${p:2}"; rest="${rest//\\//}"
            printf '/%s%s' "$d" "$rest" ;;
        *) printf '%s' "$p" ;;
    esac
}
```

> Similarly, `MSYS_ROOT="$(cd "$MSYS/.." && pwd)"` is also changed to pure string conversion
> `${MSYS%/*}` —— saves a fork and also avoids the situation where "after `cd` `pwd` becomes `/`"
> This kind of environment-dependent value.
>
> also handled display: the installation root in MSYS2 is `/` in bash, directly printed
> `MSYS2 : /` will make people think that path parsing is broken, so an extra calculation is done in Windows format
> (`D:/dev/msys64`) is used for display.

### 36.7 bare `bash` is WSL — you must use an absolute path

```
in PowerShell:  where.exe bash → C:\...\WindowsApps\bash.exe    ← WSL launcher
in MSYS2 bash:  where.exe bash → D:\dev\msys64\usr\bin\bash.exe ← correct
```

**the same `bash`, the result depends on PATH. After** enters MSYS2, `/usr/bin` usually takes priority,
So naked `bash` can be used at the moment —— but that means 'whether to build with WSL or not' is silent and machine-dependent.

processing is divided into three layers:

1. **uniformly uses `$SELF_BASH`** (= `${BASH}`, that is, `/usr/bin/bash`) to call the sub-script,
   `build-all.bat` The inner layer also uses `exec /usr/bin/bash` (originally written as `exec bash`).
2. **`common.sh` Hard Intercept**:

   ```bash
   if [ -n "${WSL_DISTRO_NAME:-}${WSL_INTEROP:-}" ]; then
       echo "[Failure] Detected WSL (WSL_DISTRO_NAME=...)" >&2; exit 1
   fi
   case "$(uname -s 2>/dev/null)" in
       Linux|*Linux*) echo "[Failed] This is WSL/container, not MSYS2" >&2; exit 1 ;;
   esac
   ```

   Negative test: `WSL_DISTRO_NAME=U bash -c '. common.sh'` → exit code 1, denied.
3. Bare `bash` / `sh`, if it parses to WindowsApps then **warning**.

> ⚠️ **When verifying 'WSL is off', do not touch that shim** ——
> `WindowsApps\bash.exe -c 'echo x'` itself will **start a WSL instance**
> (`vmmemWSL`'s pid changes each time), belongs to a self-failing test.
> Another reason WSL cannot be turned off on the local machine is **the current VS Code workspace is inside WSL**
> (`\\wsl.localhost\...`), keeping the window open maintains the distro.

### The 36.8 health check script's false positives are worse than no health check.

`check-src.sh` When determining fonts, it was originally written as:

```bash
if FONT_TTC="$(ls "$SRC"/NotoSansCJK-Regular.ttc 2>/dev/null | head -1)"; then
    printf '  ok    ...'
```

**`head` returns 0 even for empty input**, so the file does not exist but still prints `ok`.
Therefore, I thought fonts were complete until the build failed at step 4.

> Conversely: the health check output `ok` is **a conclusion**, not evidence. Wherever it prints
> 'OK / Ready', one should ask 'Could this judgment actually be false?'.

Fix: change to a check that can actually be false, and cross-verify **the same facts**:

- `font_faces_dir()` —— Are all three single faces present
- `find_font_ttc()` —— Is there a source ttc
- Neither has it, so it reports 'Missing!!'
- `.NET 10 SDK` **is only considered necessary when you need to re-extract face from ttc or build a C# release package**

was actually repaired, the physical inspection output changed to:

```
ok    trilingual single face      /d/dev/winbuild/menu-bin/fonts
```

instead of the original hollow `ok  NotoSansCJK-Regular.ttc`.

### 36.9 Font: Single face is the product, ttc is just the means

Sections 34/35 have already stated "must use single-face OTF." Here is an additional operational lesson:
`make-menu-font.sh` originally **only recognized ttc**, once ttc was missing it would immediately report 'font not found',
makes people think they are going to redo a copy —— while the three faces drawn by **are still lying there nicely
`menu-bin/fonts/` in** (that is the build artifact).

```
[Failed] NotoSansCJK-Regular.ttc not found
```

Revision: Added `font_faces_dir()`, once found **directly reuse**; `FONT_REEXTRACT=1`
can be forcibly redrawn.

> Lesson: **One must distinguish between 'the output' and 'the means to produce it'**. Means can be one-time,
> could disappear at any time (temporarily downloaded files); the output is what constitutes the real dependency.

### 36.10 Silent Missing of Packaging and Documentation

Both belong to 'reported as completed, but actually not done':

**The packaging command was only output by `echo`, never executed.**

```bash
# At the end of make-release.sh
echo "  Packaging: cd \"$(dirname \"$DEST\")\" && tar -czf DVD-Audio-Maker.tar.gz ..."
```

Therefore the release directory never actually contained `.tar.gz`, but that line in the log made people think it was done
(the previous tar.gz was manually added). Changed to actually execute, and added
`DVDA_TARBALL=0` for skipping.

**The document search missed one directory level.**

```bash
for c in "$HERE/docs/$d" "$SCRIPTS/$d"; do ... done    # Missing $SCRIPTS/docs/
```

The repository layout is `<repository>/docs/{README.md,THIRD-PARTY.md,LICENSE}`, so in the release package
**silently lacked README / LICENSE / THIRD-PARTY**, yet this step still reported 'completed'.
Form four candidate positions + echo source for each file + missing explicit warning.

**Along the way fixed similar issues**: `xargs` didn’t add `-r`, when input is empty it still executes once —

```
basename: missing operand
```

### 36.11 How to determine whether two builds are behaviorally equivalent or the code changed

With the same source, parameters and toolchain, the two executables have **identical sizes but differ at 32849 bytes**.
At this time, you cannot rely on 'looks about the same'; you need to follow the three steps below.

**Step 1: Count differences by section** (`objdump -h` take the section table)

```
section        bytes       diff
.text          333472       2063  differs
.rdata         106320      30214  differs
.pdata           5568          0  IDENTICAL
.xdata           6112          0  IDENTICAL
.idata           6484          0  IDENTICAL
```

`.pdata` (expand information function by function) the difference is **0** → **function boundaries and counts remain unchanged**.
This already strongly suggests 'not a code rewrite'.

**Step 2: Only look at the disassembled mnemonic sequences** (`objdump -d --no-show-raw-insn`)

```
instructions : old=75994  new=75994
operand-differing lines  : 1685
MNEMONIC-differing lines: 0        ← Determinative
```

**All 75994 instructions match; 0 lines have different mnemonics** —
the compiler's **instruction selection, control flow and ordering** are identical; only the **operands** on 1685 lines differ.

**Step 3: Look at the form of the differences**

```asm
mov 0x6c7bc(%rip),%rbx # 14006d880 <.refptr.__native_startup_lock>
mov 0x6c7fc(%rip),%rbx # 14006d8c0 <.refptr.__native_startup_lock>   ← Target moved back 0x40
```

All are **RIP-relative offsets** pointing to `.refptr.*` such CRT symbols; delta concentrated at
64 (944 instances)/ 56 (836 instances). In addition, `.text` the differences are only 1~2 bytes long
(1309 single-byte characters + 374 double-byte characters, **no 4-byte sequences at all**)——
These 'scattered 1~2 byte' differences themselves are fingerprints of offset drift, not code rewriting.

**Finally, use the set of strings to find the root cause** (`strings -n 6 | sort -u` + `comm`):

```
unique strings: old=3368  new=3368
--- only in OLD ---                --- only in NEW ---
/mingw64/bin/convert               /d/dev/msys64/mingw64/bin/convert
/mingw64/bin/curl                  /d/dev/msys64/mingw64/bin/curl
/mingw64/bin/mogrify               /d/dev/msys64/mingw64/bin/mogrify
```

**Each one is 13 bytes long** — these 13 bytes are written into `.data`, causing the subsequent data to shift overall
(about 64 bytes), so all RIP-relative offsets and the pointer table in `.rdata` drift accordingly.
The difference **fully explains**, and it has nothing to do with the code.

> **★ Thus, we get a reusable check**: having the local path burned into the exe ≠ non-portable,
> you first need to see whether this path is **reachable** in the target process.
>
> In this case, it is not reachable — `libutils/src/libc_utils.c`’s `create_binary_path()`:
>
> ```c
> if (symbolic_constant[0]) {
>     if (globals->settings.bindir == NULL)
>         local = strdup(symbolic_constant);                    // using compile-time path
>     else
>         local = win32quote(conc(bindir, basename + ".exe"));  // Use --bindir
> }
> ```
>
> And `menu_assets.py`’s `args()` **explicitly passed `--bindir <menu-bin>`**,
> so it always takes the `else` branch — those two MSYS2 paths at compile time are **dead code**.
> **This is also why the release package can run on machines without MSYS2 installed**
> (`menu-bin/` packaged `mogrify.exe`).

### 36.12 fork overhead differs by 100 times, and the measurement method itself has pitfalls

| Environment | fork/exec | Each check | configure whole step |
|---|---|---|---|
| Kaspersky 21 `kl*` Filter driver resident | **2~4 seconds/time** | ≈ 15 seconds | **≈ 1 hour** |
| Uninstall Kaspersky + Restart | **27 ms/time** | ≈ 0.4 s | **≈ 1 minute** |

Full build (configure + two projects + assembly + packaging) takes **1 minute 40 seconds** in a fast environment.

> ⚠️ **Two actual tests**: this round first ran once “configure not finished after 68 minutes,” after reboot
> **Finished all in 100 seconds**. The difference is not in MSYS2, but in the **kernel filter driver of antivirus**.
> Therefore in early notes the statement “MSYS2 fork is 4~7 seconds/time” is **wrong**.

**The measurement method itself also has pitfalls**: the first time I used `$(date)` to take a timestamp,
and `date` itself is a fork — the measured and measuring share the same overhead, result was inflated.
Switching to bash built-in `$EPOCHREALTIME` (zero fork) is accurate:

```bash
a=$EPOCHREALTIME; for i in $(seq 1 10); do /usr/bin/true; done; b=$EPOCHREALTIME
awk -v a="$a" -v b="$b" 'BEGIN{printf "%.0f ms\n",(b-a)*100}'
# -> /usr/bin/true (fork+exec) x10 : 272 ms   (approximately 27 ms/time)
```

### 36.13 About configure caching: it was done and then removed

Once implemented a mechanism to 'save and reuse configure artifacts' (caching + path rewriting + self-verification),
Later **removed** it, for three reasons:

1. **the configure artifacts had the absolute path of the source tree hardcoded**, which needed `sed` rewriting when reusing.
   And path rewriting was precisely the easiest place for **silent errors** to occur — that `cygpath` flaw from 36.6
   If used with caching, it would restore a Makefile with **path confusion but appearing normal**,
   reporting a bunch of "mysteriously missing files".
2. **the cost is asymmetric**: cache failure goes unnoticed → compiles wrong items, yet the build still reports OK;
   running configure one more time → only takes 1 minute.
3. **the benefit was already small**: actual test with cache 1 min 40 sec vs without cache 1 min 41 sec.
   Maintaining an extra layer of path rewriting to save this little time is not worth it.

> Conclusion: `build-author.sh` now **runs configure for real every time** (unconditionally clears old artifacts).
> The correct approach in slow environments is **to handle antivirus** (see 36.12), rather than adding an extra indirection in the build script.

### 36.14 Distribution and usage

```
DVD-Audio-Maker\ <- Repository root
  01_prepare.py 02_build.py ... <- python script
  docs\ <- Source of documentation for the release package
  tools\win-build\ <- This toolkit
    build-all.bat < - One-Click (Windows Entry)
    build-all.sh
    check-src.sh  build-author.sh  build-dvdauthor.sh
    assemble-menu-bin.sh  collect-dlls.sh  make-menu-font.sh
    make-release.sh  make-release-manifest.sh
    common.sh  da-utf8.manifest  da-utf8.rc
    README.md
```

```
build-all.bat      # Health Check → Editing dvda-author → Editing dvdauthor → Assembly → Packaging
```

Product:

```
<source-tree>/.. /menu-bin/ Tool Directory (Intermediate Product)
<toolkit>/release/DVD-Audio-Maker/ Available for distribution (self-included)
<toolkit>/release/DVD-Audio-Maker.tar.gz
<toolkit>/logs/ logs for each step
```

**The target machine only requires**: Windows 10 1903+ / Python 3.8+ / FFmpeg in PATH.
No need for MSYS2, no need for WSL, and no need for font installation.

### 36.15 Summary of Lessons

- **one symptom may correspond to multiple independent defect**. After completing one, you must **redo** before making a judgment,
  You can't conclude that the repair is ineffective just because 'the symptoms are still there' (36.4 + 36.5 are two bugs with the same symptom).
- **`//x` and `///x` behave differently in MSYS2** (the former is UNC, the latter falls back `/`),
  so errors like 'double backslash' **do not throw errors**. Before concatenating paths, first `"${VAR%/}"`.
  This type of issue only occurs when 'the root is `/`', which happens to be the situation most easily missed in testing.
- At points where **reports OK, ask 'could it really be true?'**. `head` returns 0 for empty input.
  These false positives can direct attention the wrong way during inspections (36.8).
- **cannot distinguish between 'artifacts' and 'the means that produced them'**, and may treat one-time files as necessary dependencies (36.9).
- **`echo` One line of command does not equal executing it**; missing files in release packages are harder to spot than errors (36.10).
- **When judging whether an exe is equivalent, don’t rely on size or intuition**, follow 'section → mnemonic sequence → operand form'.
  Three-step approach, usually conclusions can be drawn by the second step (36.11).
- **The measurement tools themselves may introduce bias** (`$(date)` will fork);
  For time-consuming measurements, prioritize using built-in variables (36.12).
- **Toolkits/scripts themselves also need to be testable** — a script that gives false positives is worse than none.
- **`.bat` / `.cmd` / `.ps1` must be pure ASCII**: cmd.exe uses the OEM code page (native
  CP936) decode `.bat`, PowerShell 5.1 decodes `.ps1` as ANSI; UTF-8 Chinese
  Under GBK, pairing by 2 bytes, an odd-numbered byte will **consume the first character of the next line**
  （`REM ...` become `EM ...`), report `'Windows' is not recognized as an internal or external command` this kind
  An error that has nothing to do with the problem. **Comment lines are not spared either** — the parsing occurs after decoding.

---

## 37. Building Windows from scratch for end-to-end execution

Section 36 made the **toolchain** native to Windows. This section records the first time of actually completing the process.
Issues exposed during the full process of 'Clean Windows machine → Two finished ISOs'.

The three defects share a common pattern: **they either do not report errors during construction/validation, or they report errors in the wrong place**. So in this section
The focus is not on 'how to fix it,' but on 'how to discover it' and 'how not to be misled.'

### 37.1 ★★★★★ `to_wsl()` breaks Windows paths — the error points to the wrong location

`02_build.py` should record the source file size for each track when reading the list:

```python
src = to_wsl(f["src"])        # Original code
...
"size": os.path.getsize(src),
```

`to_wsl()` unconditionally converts `D:/x` to `/mnt/d/x` (the mount point notation in WSL),
So the first step on Windows crashed:

```
FileNotFoundError: [WinError 3] The system cannot find the path specified.:
  '/mnt/d/yyz57/Music/completed/…/01. Waking of a World (feat. Gigi Yim) [Chinese Version].flac'
```

> **★ This error is very misleading.** It looks like 'the path is wrong in `manifest.json`',
> but actually all 147 entries in the list **are correct `D:/…`** — it's this conversion logic that messed them up.
>
> So the troubleshooting order should be: **first print the original values from the list**, then suspect the conversion logic.
> A `json.load` plus printing the first 3 `src` fields can give a qualitative understanding in 10 seconds;
> Doing it the other way around, reverse-engineering the list from `01_prepare.py`, would waste a lot of time.

The fix is to add platform detection and **convert only on non-Windows systems**:

```python
def to_native(p):
    """Convert the source paths in the list to a form that Python can directly open on the current platform.

    · WSL / Linux：`D:/x` -> `/mnt/d/x`
    · Windows: return as is (Windows Python already recognizes `D:/x`,
      converting to `/mnt/d/x` may cause it to not open)
    """
    p = p.replace("\\", "/")
    if os.name != "nt" and re.match(r"^[a-zA-Z]:", p):
        p = "/mnt/" + p[0].lower() + p[2:]
    return p
```

**Lesson**: In cross-platform scripts, any function that 'converts from form A to form B' should check
**‘Does this conversion still hold on the target platform?’**. The names of such functions usually indicate the applicable platform
(`to_wsl`), but the call sites are easily overlooked during porting.

### 37.2 ★★★★★ Validation scripts depend on `xorriso` / `dd` — completely unusable on Windows

All three validation scripts need to access ISO content and each uses an external command:

```python
subprocess.run(["dd", "if=%s" % path, "bs=2048", "skip=%d" % lba, ...])      # Read by sector
subprocess.run(["xorriso", "-indev", str(path), "-find", "/AUDIO_TS", ...])  # List directory
subprocess.run(["xorriso", "-osirrox", "on", "-indev", iso,
                "-extract", "/AUDIO_TS", dest])                             # Get file/directory
```

None of these exist on Windows. And **this is not a "just install it and you have it" problem**:

```
$ pacman -Ss xorriso
(no output)
```

**MSYS2 doesn’t have an xorriso package** — so these scripts have no way to be used on Windows,
and they are the main verification tools on the WSL side. It’s easy to miss such "indirect dependencies" when porting.

The fix is to write a **shared module `iso9660.py`** (using only the standard library), and have all three scripts use it:

```python
from iso9660 import Iso9660, IsoError

with Iso9660(iso_path) as iso:
    names = iso.all_paths()                          # List all paths
    data  = iso.read_file('AUDIO_TS/AUDIO_TS.IFO')   # Get a file
    iso.extract('AUDIO_TS', r'D:\tmp\AUDIO_TS')      # Get file or whole directory
    raw   = iso.read_sectors(5173, 8)                # Read by sector (replacement for dd)
```

**Why make it a shared module instead of inlining each one**: Writing three copies of the same thing (parsing ISO9660)
Will inevitably drift — Section 34 already encountered this once due to 'no closure for similar fixes'.
If one of the three copies is wrong, it will create a false conflict of 'one script says pass, another says fail'.

Parsing key points:

- **Primary Volume Descriptor starts at sector 16**, `CD001` signature + type byte `1`;
  Read sector size from PVD offset 128, do not hardcode 2048
- **Root directory record** is at PVD offset 156, length 34; get its LBA and byte count
- **Directory records are variable length**: length byte 0 indicates the remaining part of the sector is padding,
  Need to jump to the next sector boundary (`i = (i // SEC + 1) * SEC`)
- Data does not necessarily start from the first byte of the extent: record offset 1 is 'extended attribute record length'
  (in **sectors**), the actual data is at `lba + xar`
- Names need to remove the version suffix `;1`. Note `ATSI;1` these **have no dot**,
  Cannot process with `split('.')`
- Directory entry judgment `flags & 0x02`; `flags & 0x80` is a 'multi-extent file' —
  Each AOB in this project is about 1 GiB (< 4 GiB) will not hit, but if hit must **explicitly report an error**,
  Cannot silently truncate
- is UTF-16BE, `decode("ascii")` will throw an exception — just skip it directly,
  because the names we are looking for are all ISO level 1 short names

A side unexpected benefit: **about 10 times faster** (disc 1 + 2 dropped from 3 seconds to **0.33 seconds**),
because it no longer starts a `dd` subprocess for each sector.

Lesson from ****: the porting checklist should specifically include "Which external commands this script calls".
`grep -n 'subprocess\|os.system\|shutil.which' *.py` can list them all,
more reliably than reading each script individually.

> ⚠️ When changing the implementation, also confirm the unit and meaning of **return values**.
> I made a mistake here — see 37.9.

### 37.3 ★★★★★ The change set is outdated: building **does not report errors**, but functionality is missing

Changes to upstream `dvda-author` in this project exist as patches in the repository
(`docs/dvda-author-changes.patch`). The first time following the README instructions

```bash
git clone https://github.com/fabnicol/dvda-author tools/win-build/src
cd tools/win-build/src
git apply docs/dvda-author-changes.patch
```

the generated source tree **did not include changes for language-specific font assignments**. Check this patch:

```
textfont_jp          0 locations
textfont_kr          0 locations
fontname-jp          0 locations
fontname-kr          0 locations
utf8_next            0 locations
textfont_for         at 0
DVDA_STILLPICS_SEP   at 0
```

And in the source tree (development branch), all of these exist — the patch stopped at **2026-09-25**,
and the font assignment was done only at **09-27**.

The consequences are twofold:

1. **Explicit failure**: `check-src.sh` it will report missing (it specifically checks
   `src/command_line_parsing.c` whether there is `--fontname-jp/-kr` inside).
2. **Worse case — silent error**: even if you bypass the check and compile it,
   the menu will make **Japanese titles use Simplified glyphs** (`直`/`骨`/`令` written differently),
   and the entire build process returns 0, with not a single error in the logs. **You only notice it on a real device**.

The fix is to regenerate (the command is recorded in the README):

```bash
cd <source tree>
git diff master -- src libutils > <repo>/docs/dvda-author-changes.patch
```

172,444 → 187,286 bytes.

#### ★★★★ "Whether the change set can be used" must be tested practically; it’s not enough that it just looks right.

Comparing only file sizes is not convincing. **The decisive test is "apply it to a clean upstream"**:

```bash
cd <source tree>
git worktree add --detach /tmp/applytest master     # A clean upstream copy
cd /tmp/applytest
git apply --check -v <patch>                          # Dry run first
git apply -v <patch>                                  # Then apply for real
```

- During dry run, each file should have `Checking patch <file>...`, no errors
- After real application, **searches for features** one by one (`textfont_jp` should be in 3 files, `fontname-jp` in 2 files)
- Finally, **compared byte by byte with the development branch**:

```bash
diff -r --brief /tmp/applytest/src     <source tree>/src
diff -r --brief /tmp/applytest/libutils <source tree>/libutils
```

There should only be differences in `*.o` / `Makefile` / compiled products like executables; all **/`.c`/`.h` must be identical**.

> Use `git worktree` to locally create a "clean upstream" instead of actually going to `git clone` GitHub —
> The local network to GitHub often fails (see 37.7), and the `master` branch already exists in the local repository,
> which is equivalent in effect.

**lesson**: for projects like this where "the products are derivatives of another repository," the change set **must have one
Executable verification steps** enter CI or pre-disk release checklist. It expires too easily, and the consequence of expiration
is 'compilation passes, but functionality quietly missing.'

### 37.4 ★★★ ImageMagick: Difference between modular build and self-contained build

First disk release fails when generating the menu:

```
magick.EXE: UnableToOpenConfigureFile `delegates.xml' @ warning/configure.c
magick.EXE: NoDecodeDelegateForThisImageFormat `…cover.jpg' @ error/constitute.c
```

It's not 'file not found,' but **ImageMagick cannot find its own configuration**.

Reason: MSYS2's ImageMagick is a **`--with-modules` build** — codecs
(JPEG, etc.) are independent `.dll`, placed under `modules-Q16HDRI/coders/`,
and also need `etc/ImageMagick-7/*.xml` configuration. It is designed for **with MSYS2 native**.

While the upstream's built-in `local.w10/bin/` set is **7.0.8-47 self-contained build**:
a single 6.8 MB exe, only relying on Windows system DLLs (GDI32 / gdiplus / ole32 …),
**zero third-party dependencies** — in practical tests putting it in an empty directory, without any xml, without modules,
it still can decode JPEG.

**This is decisive for us**: dvda-author is **calling ImageMagick as a subprocess**
(not linking it), so any version placed in the tool directory works; but the release package must be able to run on any machine,
so we must use the **self-contained** version.

And——`local.w10`'s 7.0.8 is precisely the output of the E drive with verified benchmarks (AOB is byte-for-byte identical to the benchmark)
. Switching back to it not only resolves dependency issues but also aligns with historical artifacts.

**Lesson**: When choosing an external tool with the same name, **don't just look at the version number**, you should look at
**deployment forms** (self-contained vs dependent on runtime/configuration/module directory). The latter can be used in "native development,
crashes when copied to others'.

### 37.5 ★★★★ How to prove 'WSL really wasn't used at all'

'No need for WSL' is easy to say, but it is worth verifying. Four pieces of independent evidence:

**(1) The executable path of the process and the PE header**

```powershell
Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^(python|ffmpeg|…)\.exe$' } |
    Select-Object Name, ProcessId, ExecutablePath
```

`python.exe` is `E:\Python314\python.exe`, and the parent process is
`C:\Windows\system32\cmd.exe`, grandfather is `powershell.exe`;
`ffmpeg.exe` path is at `C:\Users\…\WinGet\…\ffmpeg-9.0-full_build\bin\`.
does not contain `wsl` / `/mnt/`, and the first two bytes are `4D 5A` (`MZ` = PE).

**(2) Which modules were loaded by the process (the hardest one)**

```powershell
Get-Process -Id <pid> -Module | ForEach-Object { $_.FileName }
```

This time Python loaded 20 modules, the directory only has `C:\Windows\System32`,
`E:\Python314`, `E:\Python314\DLLs` —— **WSL related to 0**.
If you are really using the WSL file system, components related to 9P / network file systems will definitely be loaded.

**(3) Path count in the build log**

```powershell
$pats = '/mnt/', 'home/yyz57', 'wsl.localhost', 'wsl.exe', 'WSL_DISTRO'
```

Actual measurement: All of the above are **0**, while `D:/` appeared **126** times.

**(4) Is the WSL virtual machine participating in I/O**

```powershell
$a = (Get-Process vmmemWSL).ReadOperationCount
Start-Sleep 3
$b = (Get-Process vmmemWSL).ReadOperationCount
```

The delta of read/write ops within 3 seconds **is 0** — it is completely idle.

> ⚠️ **Note**: `vmmemWSL` process **exists** does not mean WSL is in use. It has always existed on this machine,
> because the current VS Code **workspace directory is in WSL**
> (`\\wsl.localhost\Ubuntu-26.04\…`) — as long as the window is open, WSL will be maintained.
> Therefore, to "prove no WSL use" you cannot rely on "WSL process does not exist", you have to rely on the above (1)~(4).
>
> ⚠️ Also: **do not test that shim after shutdown**.
> `WindowsApps\bash.exe -c 'echo x'` This action itself will **start a WSL instance**
> (`vmmemWSL` the pid changes every time), which is a self-defeating test.

### 37.6 ★★★ The overwrite trap during two-way synchronization (overwriting the document just written)

The toolchain repair is done on the WSL side and then synced back to the repository, so I wrote a sync script, including the following rules:

```powershell
foreach ($n in @('dvda-author-changes.patch','DVDA-AUTHOR-CHANGES.md','TROUBLESHOOTING.md')) {
    Copy-Item (Join-Path $wsl "docs\$n") (Join-Path $repo "docs\$n") -Force
}
```

As a result, **the repository's `TROUBLESHOOTING.md` was overwritten**:
That was **the version just written today, including section 36** (200,552 bytes),
while the WSL side version was older (179,647 bytes) —
Then the just-written 20 KB document instantly disappeared.

fallback (the file was already committed, so it was easy):

```bash
git checkout -- docs/TROUBLESHOOTING.md
```

**Lesson**:

- In the synchronization rules, **each item must be able to answer 'which side is authoritative'**. Different files in the same repository
  can have different authoritative sides: `.py` scripts are authoritative in WSL (development happens there),
  **`docs/` the repository is authoritative** (documents are written only in the repository). Mixing them together will overwrite each other.
- Before performing overwrite operations **compare first, then act**, do not do it unconditionally `Copy-Item -Force`.
  If the previous section included a sentence like 'stop and report if the size/hash is different,' accidents would not happen.
- Fortunately, `docs/TROUBLESHOOTING.md` it had already been committed, `git checkout` fallback was just one sentence.
  **If uncommitted files are overwritten, they can only be rewritten** — so committing long documents casually is worthwhile.

### 37.7 GitHub intermittently unreachable

On the same machine, in the same session:

```
(earlier) git push origin main   →  2bd95f9..a5e9833  main -> main     success
(later) git push origin main   →  Failed to connect to github.com:443 after 21079 ms
        Test-NetConnection github.com -Port 443   →   False
```

Therefore:

- The 'starting from scratch' process of this repository **cannot rely on GitHub being reachable**.
  Check the local changes `git worktree` verification (37.3), don't expect the clone to work yet.
- Push failure **does not mean there is a problem locally** — the commit is already in the local repository, just push it again once the network recovers.
- Check the network using `Test-NetConnection <host> -Port 443`, faster than waiting for git to timeout.

### 37.8 ★★★★ Independent verification: don't use 'building your own verification script' as the only evidence.

Using the bundled `quick_check.py` / `check_aob_pts.py` build is very useful, but they and the build
come from the same codebase — **a shared underlying error can make both fail**. So you still need
**verification that does not rely on those scripts**.

#### (1) Lossless audio: decode to raw stream and compare byte by byte

```bash
# The source FLAC and corresponding MLP are both decoded into s32le raw stream.
ffmpeg -i <source>.flac -f s32le -acodec pcm_s32le - > src.raw
ffmpeg -i <corresponding>.mlp  -f s32le -acodec pcm_s32le - > mlp.raw

sha256sum src.raw mlp.raw            # lengths may differ by a few frames (decoder padding)
# Compare ** common prefix **
head -c $n src.raw > a.pfx ; head -c $n mlp.raw > b.pfx
sha256sum a.pfx b.pfx
```

Actual measurement:

```
src.raw : 68627928    mlp.raw : 68628160        (difference 232 bytes)
src prefix sha256 : b87e7ae7f5b18090efed3f6b1b64cb60…
mlp prefix sha256 : b87e7ae7f5b18090efed3f6b1b64cb60…   ← same
=> Lossless; the only difference is 232 trailing bytes of 0 padding (29 frames / 0.6 ms)
```

The durations also match: `src 178.718562 s` vs `mlp 178.719167 s` (difference 0.0006 s).

> **Why not compare the entire stream directly**: The decoder pads frames to whole frames at the end of the stream,
> So a difference of a few frames in length is **normal**. Judging it as a failure just because the 'lengths are unequal' will result in a false alarm;
> The correct approach is to compare the common prefix and prove that the extra part consists purely of 0s.

#### (2) AOB can be recognized by standard tools

```bash
ffmpeg -i <extracted AOB>
#   Input #0, mpeg, from '…ATS_01_1.AOB':
#     Duration: 00:07:40.41, bitrate: 81324 kb/s
#     Stream #0:0[0xa1]: Audio: mlp, 48000 Hz, stereo, s32 (24 bit)
```

#### (3) Use sector boundaries to find AOB and verify the volume

The starting point of AOB can be scanned from ISO (sector-aligned DVD pack header)
`00 00 01 BA` + MLP sync word `F8 72 6F` within the sector:

```
Find a **continuous** hit starting point: Sector 5173
Last sector (from the build log `Absolute sector pointer to last AOB sector`): 2290469
Extracted 2285297 sectors = 4,680,288,256 bytes
The last byte must fall on a 2048 boundary ✔
```

Check the volume ratio again: `AOB ÷ MLP = 4.685e9 / 4.588e9 = 1.0208`,
Matches the existing measured coefficient **1.0215~1.0220** — indicating that no entire segment of audio was added or omitted.

### 37.9 ★★★★★ Semantic error obscured by my own changes: `blocks` are sectors, not bytes

replaced `xorriso`, I rewrote the 'list directory' function. The original implementation (xorriso's
`report_lba`) the output is `lba , blocks , bytes , 'path'`, the code takes the first two:

```python
ifos[name] = (int(mm.group(1)), int(mm.group(2)))       # (lba, blocks)
aobs.append((..., int(mm.group(1)), int(mm.group(2))))  # (lba, blocks)
...
for _i, name, lba, blocks in lst:
    base.append((off, off + blocks, name, lba))         # Accumulate by ** sector **
    off += blocks
```

In other words, `blocks` must be **number of sectors**. And when I rewrote it, I returned **number of bytes**:

```python
out[e.name] = (e.lba, e.size)          # ← Error: size is in bytes
```

**Why it wasn't discovered**: This piece of code creates a mapping from 'sector number within the group → LBA on the disk', after the mapping is misaligned,
will end up in another AOB**of** —— and each sector in AOB **starts with a pack header
`00 00 01 BA` starts**. So the criterion 'whether this sector is a pack header' **still passes**,
, it's just that what it checks is not the position of that track at all.

crossing AOB groups may be exposed (only groups with 1 AOB, `off` starting from 0, happen to be correct),
And even if it crosses, both before and after are continuous pack areas, the result still 'looks normal'.

**How was it caught**: On the same disk there is another **independent** criterion --
Item A of `audit_disc.py`: 'Total number of AOB sectors in the group == maximum last sector in the track table + 1.'
When using the number of bytes, it is impossible to be equal (off by 2048 times), it reports an error as soon as it runs. After fixing it:

```
A. Number of sectors AOB=1939135 Track table=1939135 Consistent ✔
D. PTS decreased by 22 points (title starting point 22); landed on the track boundary all hits ✔
```

**Lesson**:

- **When rewriting a function, first understand the unit and meaning of the return value**. 'Two numbers in the same position'
  It's easy to write `blocks` as `size` when rewriting, and since both are of type `int`, no one will stop you.
- **When one criterion is 'masked by the homogenization of continuous regions,' another independent criterion is needed as a backup.**
  Item A (total reconciliation) saved the lives of items B/C/D/F.
- When facing this kind of error, using `--verbose` to print intermediate values (the lba/blocks of each AOB) is the fastest way:
  It is obvious at a glance that `blocks=1073741824` cannot be the number of sectors.

### 37.10 ★★★★ Audit criteria may expire as implementation evolves (Item D: per track vs per title)

The original D item in `audit_disc.py` is '**Each track** start should have a PTS drop point'——
Because in the early stages each track had its own title, PTS was reset for each track. Later
`patch_mlp_one_title` merges all tracks in the same group into **one title**, with the timeline inside the title
Become **continuous** (this is exactly the change that fixes 'playback acceleration / progress bar cannot be dragged'),
Therefore, the drop points only appear at the start of the new title **.**

As a result, item D always fails according to the original criteria **,** and the failure message is very misleading:

```
D. PTS drop points: 22; falls at track boundary anomaly ✗
   Track start points where it should have dropped but did not: [1316224, 1421312, 1702785, ...]
```

It looks like "the timeline is disconnected," but in fact, the 22 drop points exactly match **the 22 titles in that group**
(Group 1: 23 titles → 22 drop points, Group 2: 4 titles → 3, Disc 2 Group 1: 17 titles → 16,
Disc 2 Group 2: 1 title → 0). **The numbers themselves are the answer**.

The fix is to change the "should drop" positions from **track start** to **title start**:

```python
t_starts = set()
prev_t = None
for r in rs:
    if r["title"] != prev_t:
        if r["first"] != 0:
            t_starts.add(r["first"])
        prev_t = r["title"]
not_boundary = [i for i in drops if i not in starts]        # still at track starts
missing = [s for s in t_starts if s < n and s not in drops]  # changed to title starts
```

After fixing, item D directly becomes a self-verifying output:

```
D. PTS drop points: 22 (title start 22); all hit track boundaries ✔
```

**Lesson**:

- **Criteria must clearly state the "implementation conventions" they depend on**. Item D relies on the convention that "each track constitutes a title".
  And that agreement was later overturned by subsequent changes. The criteria itself didn’t mention this, so no one remembered to change it.
- **A long-term failing check gets ignored** (nobody looks at a light that's always red),
  burying the real problems it could have caught. **Red lights must either be fixed or removed.**
- When reading criterion failure information, **first check if the numbers have an obvious explanation**
  (22 drop points vs 22 titles), which is much faster than going through the code.

### 37.11 ★★★ Hidden dependencies of independent scripts: `identify`, `menu-bin` not in PATH, hardcoded `/tmp`

`verify_menu.py` Works fine on Linux, but when ported to Windows, it crashed three times in a row,
each time due to **another** overlooked Windows assumption:

**(1) hardcoded `identify`**

```python
rc, _ = _run(["identify", "-crop", ...])     # Written this way in two places
```

IM 7 merges various tools into `magick`, **does not have a separate `identify.exe`**.
This project had already written `dvda_config.magick_identify_cmd()` for this, but these two call points
didn’t go through it → `FileNotFoundError: [WinError 2] The system cannot find the file specified`.

> ⚠️ Note this form of error: **is not 'recognition failure', it’s that the program didn’t even start**.
> If you only look at the output being empty and parsed as 0, it will be misjudged as 'the image wasn’t drawn correctly'.

**(2) `menu-bin` not in PATH**

`magick_identify_cmd()` originally only checks `shutil.which("identify")` /
`shutil.which("magick")`. And `dvda.cmd` **deliberately does not add `menu-bin\` to the PATH**
(There are 106 DLLs in there, putting the whole directory into PATH could cause other programs to mistakenly load older DLLs with the same name,
On the contrary, it is even more likely to hit `0xC0000139`). So:

- `02_build.py` is fine — it has `_prepend_to_path()` by itself
- Standalone scripts like `verify_menu.py` crash immediately

The fix is to **fall back to find the version included in the package** in `magick_identify_cmd()`.
(`<script-directory>/../menu-bin/`), all scripts benefit together:

```python
here = os.path.dirname(os.path.abspath(__file__))
for names in (("magick.exe", "identify"), ("magick", "identify"),
              ("identify.exe",), ("identify",)):
    cand = os.path.join(os.path.dirname(here), "menu-bin", names[0])
    if os.path.exists(cand):
        return [cand] + list(names[1:])
```

> Why not modify `dvda.cmd` to add `menu-bin` to the PATH: that would introduce
> DLL hijacking risk, and 'who needs which tool' should originally be resolved by the fallback path of the parsing function.
> **Repair on the layer closest to the problem and with the least impact.**

**(3) Hard-coded `/tmp/…`**

```python
png = "/tmp/_menu_frame.png"          # frame extraction
png = "/tmp/_index_frame.png"
...    f"/tmp/verify-menu/disc{i}"    # unpack directory
```

Linux convention. On Windows it will become the current drive's `\tmp\…`(usually does not exist),
So **the two steps of frame skipping silently failed** (`return True` exited, it looked like 'skipping the check').
Switch to `tempfile.gettempdir()`.

**Lesson**:

- "This script has always worked well on Linux" **does not imply** that it will work on Windows.
  You should **list each external command it calls and the hard-coded paths**:
  `grep -n 'subprocess\|os.system\|shutil.which\|"/tmp' *.py`
- The same ability (finding/identifying) **should not be written separately in multiple places**. Here it is.
  `dvda_config` has a correct implementation, while there are two hard-coded instances in `verify_menu`.
  **Similar repairs should be completed in one go** (this point was already mentioned once in Section 34).
- Functions that analyze external tools should **come with a fallback** (PATH → directory within the package),
  Instead of scattering the requirement that 'PATH must include it' across every call site.

### 37.12 Measured Data (2026-09-27, Windows Full Process)

Audio source: 45 albums / 147 FLAC tracks, 6.43 GiB → MLP 7.30 GiB (+13.49%)

| Stage | Duration |
|---|---|
| `build-all.bat` (configure + two projects + assemble + package) | **1 min 41 sec** |
| `01_prepare.py` (Scan + decode verification of 147 tracks) | About 2 minutes |
| `02_build.py` first run (147 first MLP encoding) | about 12 minutes |
| `02_build.py` Again (MLP full cache) | **5 minutes 48 seconds** |
| `quick_check.py` | **0.33 seconds** |

Products:

```
Wuthering_Waves_Singles_EPs_1.iso   4,691,195,904  (89 tracks, remaining 16 MB)
Wuthering_Waves_Singles_EPs_2.iso   3,328,147,456  (58 tracks, remaining 1.38 GB)
```

Structure check (`quick_check.py`):

```
disc1: Group 1: 75 tracks / 23 titles + Group 2: 14 tracks / 4 titles = 89 tracks
       Still image reference number 27 <= AUDIO_SV.IFO record count 27 ✔
       Randomly checked the first sector of 89 tracks, all start with pack header ✔
disc2: Group 1 56 tracks / 17 titles + Group 2 2 tracks / 1 title = 58 tracks
       Randomly checked the first sector of 58 tracks, all start with pack header ✔
Total discs 147 tracks, consistent with source track count ✔
```

### 37.13 Lesson summary

- **The error reported does not necessarily indicate the location that needs to be fixed**. The error in 37.1 says "file not found",
  but all paths in the list are correct — it’s the conversion during reading that caused the corruption. **Print the original data first** before suspecting the logic.
- **The transplantation checklist should specifically check for 'external command dependencies'**:
  `grep -n 'subprocess\|os.system\|shutil.which' *.py`。
  Some dependencies (such as `xorriso`) **simply have no packages available** on the target platform.
- **Derivative products (patches/artifacts) must have executable validity checks**,
  Moreover, the inspection must verify that it is 'identical to the source byte by byte,' not just check the size (37.3).
- **When choosing an external tool with the same name, you need to consider the deployment form**, not just the version number:
  Self-contained vs dependent on runtime/configuration/module directory (37.4).
- **Statements like 'Not using WSL' need to be evidenced**: process path + PE header + loaded modules +
  Log path count + virtual machine I/O count (37.5). Note: 'Process exists' ≠ 'is participating'.
- **Two-way synchronization must distinguish the authoritative side**; compare before overwriting, do not use unconditional `-Force` (37.6).
- **Do not rely only on your own verification script** — add a completely independent channel (raw stream comparison,
  Third-party tool recognition, sector boundaries, volume ratio) (37.8).
- **Don't judge failure just because the lengths are unequal**: the decoder will pad frames at the end.
  Compare the common prefix and prove that the extra part is purely padding.
