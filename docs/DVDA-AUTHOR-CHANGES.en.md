# dvda-author Toolchain Change Log

[简体中文](DVDA-AUTHOR-CHANGES.md) | [English](DVDA-AUTHOR-CHANGES.en.md)

> Historical engineering record: this document preserves experiments, hypotheses and results from their original development stages. Later experiments supersede some earlier conclusions. Disabled approaches are not current configuration recommendations; see the [project README](../README.en.md) for the supported workflow. Literal filenames and glyph samples are preserved where needed to explain the original tests.

The changes in this project are **fixed in the source tree** `tools/dvda-author-mlp8` and saved by git commits.

This document summarizes the **basis** for each change (why it was changed, counterexamples, test data). The content is taken from the docstrings of the original 25 patch scripts — the patch scripts have been deleted, and this is their equivalent.

| Item | Value |
|---|---|
| Upstream Baseline | `github.com/fabnicol/dvda-author` @ `8fca43a` |
| Change set | Source tree `dvda-maker` branch, or exported `dvda-author-changes.patch` |
| **Source Code** | Also mirrored in the repository: `tools/dvda-author-mlp8/` (about 1.2 MB) |
| Scope of changes | 20 files, 1251 insertions / 354 deletions (followed by multiple rounds of changes, see below) |

> **Where to read the source code**: If you want to look at the code without applying the patch with `git apply` first, just look directly
> `tools/dvda-author-mlp8/` —— That is the **read-only image** of the modified source code
> (`.c`/`.h` under `src/` and `libutils/`, synchronized byte by byte),
> For instructions, see the `README.md` in that directory. For modifications, please change the source tree on the workstation.
> Run `python3 local-bin/sync_repo.py` again to refresh the image.


---

## Build the foundation


### Basic fixes unrelated to FFmpeg version (winport / libsoxconvert)


Basic fixes unrelated to FFmpeg version for mlp8 build tree (idempotent).

Fixes:
1. winport.h: 4-parameter inline close_handles conflicts with 5-parameter implementation → renamed;
   Added declaration for value-passing close_handles under Linux.
2. winport.c: implementation changed to 4-parameter version with value passing.
3. libsoxconvert.c: added WITHOUT_sox guard and stub functions.


### fn_strtok() out-of-bounds stack write when string is empty


Fix out-of-bounds write in `fn_strtok()`: writes zero-length VLA when string is empty, corrupting the caller's `globals`.

`auxiliary.c`'s `fn_strtok()`:

    char *s = strdup(chain);
    ...
    uint32_t j = 1, k = 0;
    int32_t cut[strlen(s) / 2];        // ← VLA of length 0 (when chain is empty)
    cut[0] = -1;                       // ← out-of-bounds stack write
    do  if (s[j] == delim) { cut[++k] = j; }
    while (s[j++] != '\0');            // ← When the string is empty, starts from s[1], one byte of allocation has already been exceeded
    cut[k + 1] = j - 1;                // ← out-of-bounds again

Two defects:

1. When **string is empty** (`chain == ""`), `strdup` only allocates 1 byte, while scanning starts from `s[1]` —
   Already crossed the allocation boundary, will read all the way to the first one in the heap `\0` Just stopped;`cut` Another VLA of length 0,
   `cut[0] = -1` and `cut[k+1]` are both **out-of-bounds stack writes**.
   Actual measured result: the `globals` pointer saved by the caller on the stack was overwritten to `0x7ffd00000006`,
   Then in `create_stillpic_directory()`
   `change_directory(globals->settings.stillpicdir, globals)` segmentation fault.
   **The crash point and the cause are several layers apart, and it entirely depends on the stack layout** — the same binary,
   Under gdb (with a different layout) it doesn't crash, it runs normally and crashes; changing to other track counts may also only 'occasionally' crash.

2. **`cut` capacity is insufficient**: number of entries = number of delimiters + 2. Original formula according to `strlen(s) / 2`
   Open, right `",,,"` A string where each character is a delimiter only needs 1 item but requires writing 5 items.
   In `--stillpics`, `:::` is this kind of string.

When will it result in an empty string: `--stillpics` uses **an empty item to indicate using the previous picture**
(`--stillpics A.jpg::C.jpg`), when parsing, each empty item will be called
`fn_strtok("", ',', ...)` — that is, 'only store one cover per album,' which saves ASVS budget
The standard approach will inevitably trigger it. In actual tests, this project disc 2 (56 tracks / 39 empty items) consistently caused a segmentation fault.

Amend the law:
  · Check `strlen(s)` before scanning; skip scanning if the string is empty
  · `cut` starts with `slen + 2` (in the worst case, every character is a separator, just enough)


---

## FFmpeg 8 Migration


### mlp.c Packet Reading Path Migration to FFmpeg 8 (ch_layout / pkt_pos)


Migrate dvda-author's mlp.c from FFmpeg 4.x API to FFmpeg 8.x API.

Key Points:
- channels / channel_layout → ch_layout
- AVFrame's pkt_pos / pkt_duration / pkt_size have been removed:
  Now need to record input packet positions manually (MLP sector layout depends on pkt_pos)
- Packet reading method: av_parser_* → av_read_frame (position information is more reliable)
- avcodec_close → avcodec_free_context
- Encoding side switches to planar format (s16p/s32p), allowing 24-bit


### decode_mlp_file() extraction branch switched to av_read_frame


Migrate the second parsing loop of the "extraction branch" in decode_mlp_file to av_read_frame.


### Encoding side migrated to FFmpeg 8 with 24-bit enabled


Migrate the encoding side of mlp.c to FFmpeg 8 API and enable 24-bit.

- MLP encoder in FFmpeg 8 uses planar sample format (s16p/s32p),
  Original implementation filled frame->data[0] in packed mode; now need to fill by each plane.
- 24-bit was previously rejected; FFmpeg 8 supports s32p, so now enabled with <<8 alignment.


---

## Audio and title structure


### One audio group generates only one title


Make an audio group generate only **one title**, thereby supporting 'Next / Previous' track-by-track switching.

## Do not switch titles by album (tested with negative results, 2026-09-22)

Tried inserting `-z` at album boundaries in the command line to make the title granularity consistent with commercial disc
(Enigma '15 Years After' 99 tracks = 8 titles, Li Na's Selected 24 tracks = 2 titles).
**Tested results: neither beneficial nor free of side effects, has been rolled back.**

  · **cross-album continuous play fails** — after one album finishes, it stops and will not continue to the next album;
  · **previous/next track still breaks** — aligning to title granularity **does not** fix this issue.

Therefore, the layout of this project is fixed as '**one audio group = one title**, each song in the title is one track.'
`02_build.py` in `build_disc()` indicates 'do not insert `-z`', please read that note before making changes.

## Symptoms

Clicking 'Next Segment' in PowerDVD / hard disk player cannot switch tracks:
Pressing it on the first track moves to the second track, but from the second track onward, pressing **returns to the beginning of that track**, never reaching the third track.
Manually selecting the third and fourth tracks and pressing 'Next Segment' will still jump back to the second track.

## Root Cause

`amg2.c` determines 'where a new title begins' as follows:

    if (samplerate != previousTrack.samplerate || bitspersample != ... || channels != ...
        || cga != ...
        || files[group][track].type     == AFMT_MLP     // ← culprit
        || files[group][track - 1].type == AFMT_MLP)    // ← culprit
      files[group][track].newtitle = 1;

The comments immediately next to it indicate the author's attitude:

> apparently MLP does not allow "gapless" same-audio characteristics titles, which
> means that 3 following tracks with same audio specs will create 3 titles instead
> of 1 for gapless PCM. TODO: check if this is software-dependent

That is, **upstream, based on the guess that "MLP cannot seamlessly connect like PCM"**, makes it so that whenever the "previous track or current track is
MLP," a new title is always started. All audio sources in this project are MLP, so **each song becomes its
own title** (tested: disc 1 has 91 titles, disc 2 has 56 titles, with exactly 1 track per title;
even the old menu-less build is exactly the same, so this is not introduced by the menu functionality).

The semantics of "next segment / previous segment" (next chapter / previous chapter) on a DVD is
**to advance to the next track within the same title**. Each title has only one track, so the player has nowhere to go,
and can only fall back to its default behavior — manifesting as "pressing does nothing / jumps back to the beginning of this track".

## Fix

Remove the two MLP condition lines, keeping the audio attribute comparisons. Then one audio group = one title,
Each song in the title is a track (exactly the conventional layout of a commercial DVD-Audio disc):

    Before modification: title 1[track 1] title 2[track 2] title 3[track 3] ...   (Disc 2: 56 titles / 56 tracks)
    After modification: title 1[track 1 track 2 track 3 ... track 56]               (Disc 2: 1 title / 56 tracks)

During a single play, the entire group will play continuously, and the player's 'Next' function will have a stop point;
The associated benefit is that the gaps in the AOB are eliminated, and a disc can **play continuously from start to finish**.

Comparing retained attributes is still meaningful: if a group's sample rate/bit depth/channel number changes in the middle,
it will still be correctly divided into multiple titles (this project will not, groups have already been organized by attributes).

## Associated impact and verification

- **the menu is unaffected**: the menu buttons are `jump group G track K` on the dvdauthor side
  `dvdvmy.y` inside `TRACK_TOK NUM_TOK` and `TITLE_TOK NUM_TOK` are the same production
  (all `|128`), VM commands `0x0A` directly take `i2-128` as **track number** usage,
  and are unrelated to title granularity. (`dvdauthor-0.7.1/src/dvdcompile.c:994`)
- **playback cover (ASVS) expected to be retained**: the still image mechanism works **by track** ——
  ATSI's still image records are `(picture_number, track_number, onset)` triples, onset relative to each track
  (the first image of each track → 0), and **only records the tracks with images as**, not the tracks without records
  'Use the previous one'. ASVS side `ntitlepics[group][title]` will take the number of images in each track inside the title
  accumulates, so after changing to a single title, it just changes '28 items, 1 each' to '1 item, 28 each',
  The contents of the batched sector table (0x378) remain unchanged.
  But **this is an inference and must be verified in actual practice** (confirming one by one in PowerDVD whether the cover changes accordingly).
- The upstream sentence 'MLP does not allow seamless' if it holds true on certain players in ****, the symptom would be playing up to
  edge of track experiences stuttering/cutouts. Must pay attention in live trading.

## Related changes: `ats.c` must 'unconditionally follow the track to brush the pack'

Only removing the sub-title condition of `amg2.c` will cause a segment error in****(actually tested `write_lpcm_header`
reading `info->mlp_layout[880418]` out of bounds). Because the block in `ats.c` that refreshes the pack** simultaneously undertakes two things **, while it was originally attached to `if (files[i].newtitle)`:

```c
if (i < ntracks)
  {
    if (files[i].newtitle)          // ← In the original version, this applies to every track (each track in MLP has its own title)
      {
        write_pes_packet(fpout, &files[i-1], audio_buf, bytesinbuf, ...);
        ++pack;
        bytesinbuf = 0;
        pack_in_title = 0;          // ← Key: reset each track to zero
        totpayload = 0;
      }
    files[i].first_sector = files[i-1].last_sector + 1;
```

Two things are:

1. **Align sectors according to track**. `files[i].first_sector = files[i-1].last_sector + 1`
   only holds when 'the track starts from the pack boundary'. Not refreshing will cause the next track to start from the middle.
   Disk reading end `get_ps1()` Cannot get pack header → **The entire track is discarded**
   (This is the same kind of defect that was fixed before).
2. **Reset `pack_in_title` to zero**. The `info->mlp_layout[]` of MLP is
   `allocate_mlp_tracktable()` **allocated by track**, while
   `write_lpcm_header()` uses it to calculate PTS offset:

       frame_offset = info->mlp_layout[pack_in_title].pkt_pos - ...

   Accumulating across tracks will read out of the array (tested 880418) → segmentation fault.

Therefore, it was changed to **unconditionally refresh** by track. For an MLP disc, this behavior is exactly the same as before ****
(original version refreshed each track), only the title structure changed from "one title per track" to "one title per group",
so the byte content of the AOB remains unchanged, only the title tables of ATSI / AMG change.

## Restoration method

This patch makes multiple changes (`amg2.c` removes the MLP subtitle condition; `ats.c` is changed to unconditional track brushing +
PTS within title continuous; `atsi2.c` mark each track as track start; `structures.h` add fields),
just revert each change to the original; `build_dvda_author_mlp.sh`'s `[1b]` will restore these files from the original
source code, so disabling this script will return to the old behavior.

## Verification status at each place

| change | status |
|---|---|
| `amg2.c` A title | ✅ Verified (`numtitles=1`, `tracks=N`) |
| `ats.c` Brush track by track pack | ✅ Verified (will segment fault if not brushed) |
| `ats.c` Continuous PTS within the title | ✅ Verified (cell `first_pts` increments, AOB does not fall back) |

> Tried another change (adding the "track type" recorded by the ATSI timestamp `0xC000` to each track),
> Wanted to verify if it is the "track start" position — **real device test invalid, removed**.


---

## Upstream defect fix


### write_pes_padding() writes no bytes when length is 1~6


Fixed pack boundary misalignment in ats.c: the last pack of each MLP track may be 1~6 bytes short.

write_pes_padding() when length is 1~6 only prints an error and returns, **writes no bytes**.
Caller (last MLP packet) passes the "bytes needed to pad to 2048 boundary"; when this value is 1~6:

  · The last pack of the track is short by 1~6 bytes → file no longer 2048-byte aligned
  · The next track's pack header thereby falls in the middle of a sector (actually offset 2042 / 2043)
  · When reading this track according to the sector number given by the IFO, the sector does not start with a pack header →
    Cannot get stream id → the whole track is considered invalid and discarded
    (In testing, foo_input_dvda lost 1 track per disc: exactly the track immediately following the short pack header.)

Original criteria `length > 6` Also, it incidentally considers length == 6 as an error — and 6 bytes happens to
It is just an empty PES padding packet (3-byte start code + 1-byte stream id + 2-byte length),
It could have been written normally. Therefore, it is being released altogether.

Change
  1. length < 6: pad with zeros to the boundary (handled the same way as the length == 0 case)
  2. length >= 6: Normally write PES padding packet
  3. Change ff_buf to a size of length + 1 to avoid zero-length arrays when length == 0


### ATSI table dynamically allocated by number of tracks


Change the ATSI table to **allocate dynamically based on the number of tracks**, and calculate the number of sectors according to actual usage.

## The original question

In `create_atsi()` of `atsi2.c`, the ATSI table is a **fixed-size stack array**:

    uint8_t atsi[2048 * 3];                  // 3 sectors = 6144 bytes
    ...
    if (i > 4096) *atsi_sectors = 3;         // Only 2/3 levels
    else          *atsi_sectors = 2;

Writing more than 6144 bytes is a **stack overflow** (Section 3 of `docs/TROUBLESHOOTING.md`).
So `GROUP_TRACK_LIMIT` can only be reduced to 64, and the tracks on a single disc are split into multiple 'audio groups'—
For this version of the disc produced by SurCode, disc 1 is split into two groups of 66 and 25, and **the audio parameters of the two groups are exactly the same**
(Both are 48000/24), which means that grouping **is not a format requirement**, it's purely a result of this limit.

## Actual measured occupancy per track

Read the `i` field inside ATSI from production build (`atsi[0x804] + 0x801`):

| Group | Number of Tracks | i | Per Track |
|---|---|---|---|
| Disk 1 Group 1 | 65 | 5696 | 56.1 |
| Disk 1 Group 2 | 10 | 2616 | 56.7 |
| Disk 1 Group 3 | 14 | 2840 | 56.5 |

Baseline 2049 bytes (ATSI_MAT, etc. occupy 0x800) ⇒ **approximately 56.5 bytes per track**, consistent across different numbers of tracks.

## Change

1. **Buffer allocated on the heap according to the number of tracks** (no longer occupies a fixed-size stack):
   `2049 + ntracks × 96` (96 is approximately 1.7 times 56.5 as a margin), rounded up to the nearest sector plus one extra sector.
   99 tracks (`MAX_TRACKS`) only allocate about 12 KB.
2. **The number of sectors is calculated based on actual usage** (`ceil(i / 2048)`, minimum 2), no longer fixed at 2/3 two levels.
3. Allocation failed / When still unable to write, provide **a clear error** instead of silently going out of bounds.
4. Function exit `free()` drops buffer (this function only has one `return`).

## Why other places do not need to change

`*atsi_sectors` is still the only source of truth, the following **are all derived from it**, and will automatically adjust:

- Write disk length `create_file(..., 2048 * (*atsi_sectors), ...)`
- ATSI internal pointer: `atsi[12]` (`+ 2 * *atsi_sectors`),
  `atsi[28]`（`*atsi_sectors - 1`）、`atsi[196]`（`*atsi_sectors`）
- Sector accounting in other files: `sectors->atsi[..] * 2` in `amg2.c` (IFO + BUP),
  `2 * sectors.atsi[i]` of `launch_manager.c`, `samg2.c` of
  `+ sectors->atsi[0]` / `sectors->atsi[g] + sectors->atsi[g+1]`

Checked: there is no other hard-coded "3 sectors" in the entire project.


---

## Static image records


### Static image records cannot skip tracks that "reuse the previous one"


Candidate fix: add references to "reuse the previous one" to ATSI static image records.

> ✅ **status: connected to `[5/8]` step of `build_dvda_author_mlp.sh`, and real disc covers are normal.**
>
> 2026-09-21 Verified with finished discs: during playback, **can display each track’s album cover**
> (Disc 1: 28/28, Disc 2: 17/17 albums all correct). This build **already includes this patch**.
>
> ⚠️ There was previously a note saying "this script was not integrated," which was an error: it has always been part of the build process.
> If in the future it is necessary to determine whether **is required**, a comparison experiment must be done (rebuild a disc with this patch disabled).
> Retest the cover), you cannot draw conclusions just by looking at “cover usable”.

## Structure (the coordinating relationship on both sides)

- `asvs.c` generates `AUDIO_SV.IFO`, which contains a compact **title table with a**: each
  “title with an independent image” occupies one entry (8 bytes). Disk 1 of this project has 28 albums →
  28 entries, `totnumtitles = 28`.
- `atsi2.c` generates `ATS_xx_0.IFO`, where in the stills records it is written
  **“which title entry number to use”** (1-based):

      if (ntitlepics[j]) ++pictitlecount;        // Accumulate titles that have images
      ...
      atsi[i++] = pictitlecount;                // title-with-pics rank

  That is to say, the mapping from title → ASVS entries is handled by ATSI, and it is correct for ASVS side to remain compact.

## Defect

There is also a sentence in the same section:

    for (r = 0; r < ntitletracks[j]; ++r)      // ← One track per record design
      {
        ++trackcount;
        //  This might be taken off in some unclear cases.
        if ((ntitlepics[j] == 0) && (img->npics[trackcount - 1] == 0))
          continue;                            // ← skip
        atsi[i++] = pictitlecount;
        ...
      }

When `--stillpics` uses **empty entry** to represent “reuse the previous image”, the `img->npics` of those tracks is 0
(for MLP each song is its own title, so `ntitlepics[title]` is also 0) → these tracks are completely skipped.
Two consequences:

1. The number of records written for this title < `ntitletracks[j]`, breaking the 'one track, one record' format convention;
2. These tracks have **no static image references** in ATSI → the cover will not be displayed during playback.

Test disc 1: Out of 91 titles, only 28 (the first track of the albums) have records, **63 tracks have no cover**.
(`asvs.c` has 28 concise entries, corresponding to these 28 titles, which is fine in itself.)

## Amend the law

Remove this skip: as long as a title with an image has appeared before, write it as usual
`pictitlecount` — Its current value is exactly **the number of the most recent title with a picture**,
So these tracks reuse the same cover.`asvs.c` That image already exists and **does not take up additional sectors**.

Only skip when 'so far no title has an image' (retain the original behavior when 'completely without a cover').

Therefore, ATSI writes approximately 6 more bytes per title × the number of missing tracks (about +378 bytes for disc 1 in this project),
Absorbed by the dynamic allocation of `patch_atsi_dynamic.py`.


---

## ASVS / Static Diagram


### ASVS 0x19（activates buttons）1 → 0


Change ASVS's `0x19` (activates buttons) from a hard-coded 1 to 0.

## Based on (three commercial discs actually measured on 2026-09-22)

| Disk | `0x18` | `0x19` | Result |
|---|---|---|---|
| Enigma "15 Years After" | 0x53 | **0** | Normal |
| Li Na Selected Collection | 0x43 | **0** | Commercial Disk |
| Bach Brandenburg Concertos | 0x43 | **0** | Commercial Disk |
| This Project | 0x53 | **1** | Previous/Next Track Failed |

Three commercial disks **without exception are 0**, only dvda-author writes 1.

Source Code Original Text (`asvs.c`):

    asvs[0x19] = 0x1; // activates buttons // number of menus ? // or 0

The comment itself wrote "**or 0**", indicating the author was also unsure; here we choose the value consistent with the commercial disk 0.

## Why suspect it

The field name is "**activates buttons**". Our static image (`--stillpics` generated
720×576 screen) **has no buttons**, stating "buttons activated" does not match the real object.
If the player enters "button navigation" mode based on this, `Previous/Next` will be consumed by the button logic
— consistent with observed phenomena (track shows 0, previous section unresponsive, next section jumps back to the 1st track).

## Explanation

Only change this 1 byte, do not touch any other fields; `0x18` (Enigma 0x53 / Li Nah Bach 0x43)
Both are used in commercial discs, so the 0x53 of dvda-author is retained.


### ASVS per-diagram offset = base_sect + relative offset within this title


Fix ASVS's 'per-figure offset table': it must be **relative to the start of the title**, and each title resets to zero.

## Measured three commercial properties (2026-09-22)

Starting from offset `0x378` in `AUDIO_SV.IFO` is a 2-byte per cell offset table, **segmented by title**.
The first item of each segment is always 0, followed by the sector offset of the chart **relative to the start of this title**:

    Enigma:
      title1 (15 images, 23 sectors per image): 0  23  46  69 ... 322
      title2 (12 images, 20 sectors per image): 0  20  40  60 ... 220     ← start again from 0
      title3 (12 images, 19 sectors per image): 0 19 38 57 ... 209 ← then resets to zero again

Each segment length exactly equals the number of images for this title (= number of tracks).

## Where did we go wrong?

`totpicsectors` in `asvs.c` is **a global accumulation** (the 'starting sector' in the ASVS record)
It must be a global absolute value, so it cannot be changed). But it is used by **the same variable** to write this table:

    if (j)                                  /* Still skipping j==0 */
      uint16_copy(&asvs[t], totpicsectors); /* Writing the global cumulative value */

Thus, writing out a “global absolute sector that never resets” flat table:

    Us: 0 26 52 78 104 130 156 181 206 ... 1398

Consequence: Only **title1** (whose starting point is exactly 0) happens to match the “relative offset”;
After title2, all are too large → when the player gets pictures from the VOB according to “picture number + offset,” it goes out of bounds / gets wrong pictures,
manifesting as **only the cover of title1 can display, the subsequent ones all do not** (observed phenomenon).

## Fix method

Added **a per-title zeroed** counter `titlesectors`:

  · Write `titlesectors`into the table (write first, then add) → the first item naturally is 0, the following are relative offsets;
  · `totpicsectors` keeps global accumulation, still used for ASVS to record “starting sector” and
    the “total sectors” at the end of the file (these two places **must** be global values).


### Removed mplex empty DSI from still image navigation sectors


Changed the still image navigation sector from mplex’s “empty shell” to commercial disc format (remove empty DSI).

> ## 🚫 This script is obsolete — do not run
>
> Its effect **has already been re-implemented in C**: `src/menu.c` of
> `dvda_rewrite_nav_sector()`, in the static image loop of `generate_background_mpg()`
> immediately follows `dvda_pad_program_end()` calls.
>
> The target of modification for this script (the AOB zero-fill section inside `build_disc()` of `02_build.py`)
> **has been deleted in**, so it will definitely report `[FAIL] AOB padding block not found` now.
> The implementation details below are just historical records; to reapply, write C code according to this, do not run this script.

## According to (tested on 2026-09-23, five disks)

In `AUDIO_SV.VOB`, **sector 1 of each still picture is a navigation sector**:

| disk | starting code | PCI length | DSI |
|---|---|---|---|
| Bach | `ba, bb@0x0e, bf@0x23, be@0x312` | 745 | ❌ none |
| Li Na | same as above **byte-for-byte identical** | 745 | ❌ none |
| Enigma | same as above **byte-for-byte identical** | 745 | ❌ none |
| this project | `ba, bb@0x0e, bf@0x26, **bf@0x400**` | **980** | ✅ **contains** |

`mplex -f 8`'s manual itself states it is an empty shell:

> 8 - DVD (with NAV sectors). Don't get too excited. This is really a very
> minimal mux format. It includes **empty versions** of the peculiar VOBU
> start sectorsDVD VOB's include.

DSI (Data Search Information) is located at a **fixed offset 0x400** within the sector and contains the 'previous and next VOBU'
The sector pointer." mplex writes **all zeros** → which equals declaring 'the next VOBU is in sector 0.'
During actual testing, when jumping to track 50 from the menu, the player crashes directly.

The navigation sectors of the three commercial discs are **constant within each disc** (Disc 1 and Disc 2 are identical byte by byte),
And there are only 11 non-zero bytes in the PCI (Enigma has only 1 left) → **the player does not look at the PCI content**,
As long as the navigation sector exists and does not contain misleading DSI.

## Amendment

In `02_build.py`'s `build_disc()`, call **before packaging the ISO**
`fix_asvs_nav_sectors()`: According to the ASVU records in `AUDIO_SV.IFO` (starting from 0x60, each fixed
8 bytes) + per-image offset table (starting at 0x378, 2× number of images per entry, **two separate cursors**) to locate each static image
Navigation sector, rewrite the whole block into a commercial disc format:

    pack_header(14B, exactly the same as the original byte-by-byte in this project)
    + System Header (15B) + PCI (745B) + 0xBE Padding Packet + 0xFF Fill Up to 2048

The number of sectors remains unchanged, so the ASVS offset table, the size of each map, and any other tables do not need to be modified. **Idempotent**.

⚠️ The files extracted by xorriso are read-only (`-r-xr-xr-x`), so you need to `chmod u+w` them before testing.


### Program End Code for Still Images in a Dedicated Sector


Change the program end code of the still image VOB from 'last 4 bytes of the sector' to 'exclusive sector + padding 0xFF'.

## Empirical evidence (2026-09-22)

Each still image in `AUDIO_SV.VOB` should end with an MPEG program end code `00 00 01 B9`.
All three commercial discs have it, and the **position is consistent** — occupying a whole sector, which is then filled to the sector with `0xFF`:

    Enigma "15 Years After" 99 discs → 99 end codes, all located at the beginning of the sector (offset % 2048 == 0)
    Li Na Selected Works          12 discs → 12 end codes
    Bach Brandenburg          18 discs → 18 end codes

This project (dvda-author + mplex) writes the end code in the **last 4 bytes of the final sector of the segment**
(offset % 2048 == 2044), immediately followed by the pack header of the next still image segment:

    Commercial discs: ... pack data ... | B9 FF FF ... FF | ← end code occupies dedicated sector
    Ours: ... pack data ... FF FF FF FF B9 | BA ... ← squeezed at the end of the sector

DVD standard requires that after the program end code **,** fills to the sector boundary; our padding is before the end code,
meaning "the new pack comes directly after the end code." A strict decoder might treat the two still image segments as
one program, thus miscalculating "which track is currently playing."

## Amend the law

For the mpg of each static image (after `create_mpg` in `generate_background_mpg`,
Post-processing (before `stat_file_size`):

  1. If the last 4 bytes of the last sector are `00 00 01 B9`, change them to `FF FF FF FF`;
  2. Append a sector at the end of the file: `B9` + 2044 `FF`.

This way, the end of each still image segment is 'end code + padding,' consistent with the three commercial discs.

⚠️ Must be done **before** `stat_file_size()`, because it determines
`img->stillpicvobsize[]`, and the starting sector of each image in the ASVS table is accumulated from it;
After the change, it will be automatically consistent, no need to modify ASVS separately.


### The two offset fields recorded in the still image progress sequentially according to the track


ATS static image record: Two offset fields changed to **increment by track** (byte1 remains 0x00).

## Symptoms

**Only the first song in the same album can display the cover; switching to other tracks of the same album does not refresh the screen**
(Only refreshes when changing albums). User reported on 2026-09-24.

## Root Cause (Measured Three Discs on 2026-09-24)

The static chart layout of ATS is '**first record of 6 bytes/track, followed immediately by a 10-byte/list of charts**,'
Both types of records are positioned relative to the static chart origin (descriptor `+14`). Each track has 6 bytes:

    [Drawing number / highest title serial number: 1][byte1: 1][Start offset of this track list: 2][End offset of this track list: 2]

Upstream wrote the last two items as **, a constant unrelated to tracks, and**:

    uint16_copy(&atsi[i], 0x06 * ntitletracks[j]);                 /* start */
    uint16_copy(&atsi[i], (ntitletracks[j]-1)*0x6
                          + 0x0F + (ntitlepics[j]-1)*0xA);         /* Stop */

- `start = 0x06×track_count` —— only **track 1** happens to be correct
- `end = (track_count-1)×6 + 0x0F + (picture_count-1)×0xA = 6×track_count + 10×picture_count - 1`
  —— only **last track** happens to be correct

So all tracks within the same title claim 'My map is in [6n, 6n+10p-1]' → the player checks the table track by track
All that is retrieved is **picture 1**, so pictures do not change within the track.

### Commercial discs that work properly progress by track

| disc | track | a | b |
|---|---|---|---|
| Li Na (2 titles / 24 tracks / 12 pictures) | Track 1 | 72 = 6×12 + 10×0 | 81 = a + 9 |
| | Track 2 | 82 = 6×12 + 10×1 | 91 = a + 9 |
| Bach (1 title / 18 tracks / 18 images) | Track 1 | 108 = 6×18 + 10×0 | 117 = a + 9 |
| | Track 2 | 118 = 6×18 + 10×1 | 127 = a + 9 |
| **This project (before repair)** | Track 1..n | **36 (constant)** | **95 (constant)** |

That is `a(r) = 6×track_count + 10×(sum of pictures on tracks before r)`, `b(r) = a(r) + 10×pictures_on_this_track - 1`.
When each track has exactly 1 picture, it degenerates into `a = 6n + 10r`, `b = a + 9`.

> The third-party implementation foo_input_dvda also interprets these two fields according to 'each track's start and end'.

## byte1 remains 0x00

In the source code `0x04` corresponds to `--stilloptions manual`
('Enable browsable (manual advance) pictures', slideshow that can be manually flipped).
This project requires 'pictures switch with the track, no flipping', so 0x04 is not written.

Upstream **originally wrote it this way** (0x04 is only set when `--stilloptions` is passed),
This patch **does not change this logic** — in practice, this project never passes `--stilloptions`,
so byte1 is always 0x00.

## The lengths of both fields are not changed

Each track is still 6 bytes → total ATSI length remains unchanged, sector count, pointers at `0x0804`, etc., are all unchanged.

## Verification (tested after rebuilding)

- All 17 titles have become `a = [6n, 6n+10, 6n+20, …]`, `b = a+9`
- Before and After Repair: **78 differing bytes** (= 2×(56 tracks − 17 title),
  That is, the low byte of a excluding the first track + the low byte of b excluding the last track), **all fall within the static table**
- The remaining 6 system files (AUDIO_PP.IFO / AUDIO_SV.IFO / VOB / AUDIO_TS.IFO / BUP / VOB)
  are identical byte-by-byte to the pre-repair version ****.


### The standard and progressive/interlaced are changed to self-check according to the stream.


This allows dvda-author to detect the 'standard' and 'progressive/interlaced' by itself and fill the parameters truthfully.

## Why this patch is needed.

There were two parameters previously **hard-coded**, unrelated to the actual stream:

| Position | Upstream implementation | Problem |.
|---|---|---|
| `asvs.c` of `asvs[0x18]` | `0x53` (= PAL) | was still written as PAL when transmitted `-4 ntsc` → 'Declare NTSC but actually broadcast PAL' |.
| `amg2.c` of `amg[0x100]` / `unknown2` | `0x53000000` | same as above (menu video properties) |.
| Sequence extension `progressive_sequence` | copied verbatim from `mpeg2enc` of **0** | The still image is actually progressive single-frame, but the header declares interlaced |.

The first two cases were patched with bytes 'copied from commercial discs' (`patch_asvs_video_attr.py`, now disabled);
The third place is based on `patch_still_headers.py` **unconditionally** change this bit to 1 (also deactivated).
Unconditional editing is as unreliable as writing to death—once the input is replaced with interlaced material, it will write a false value.

## How to apply this patch: Fill in truthfully after detection

`menu.c` adds a new stream self-check, which reads directly from the actual MPEG-2**produced by**:

    Sequence Head 00 00 01 B3 Size / aspect_ratio_information /
                               frame_rate_code / bit_rate_value
    Sequence Extension     00 00 01 B5 + 0x1?   progressive_sequence
    Image coding extension 00 00 01 B5 + 0x8? picture_structure /
                               frame_pred_frame_dct / progressive_frame

Two conclusions can be drawn from this:

1. **standard**: `frame_rate_code` (3/6 = 625/50, rest = 525/60) and the sequence header
   Screen heights (576/480) verify each other; in case of conflict, the screen height prevails—the display format is determined by it.
   Based on this, calculate `video_attr` bytes: `0x43` = 525/60, `0x53` = 625/50.
   Write ASVS `0x18` with AMG `0x100` (menu area).
   If the sequence head cannot be detected, it returns to the `--norm` setting and `[WAR]`.
2. **Line-by-line/alternate lines**: Only if the image coding extension indicates that the picture is indeed 'full frame + frame_pred_frame_dct'
   + progressive_frame" only writes the bit back to 1 when the sequence expansion is written as 0.
   The content really does not change a single byte when skipping a line.

`progressive_sequence` are similar to `patch_still_end_code.py`, **must be in
`stat_file_size()` should be executed before**, otherwise the ASVS sector pointer will not match the physical object.

## only change these two parameters

The bitrate declared in the sequence header **is not changed**: that is the encoder's own setting, self-consistent (this project 7.5 Mbps,
commercial disk 9.0 Mbps only has two sets of encoding parameters, it's not a matter of true or false. It will be printed out in the self-check report for verification.


---

## Song Selection Menu


### Menu Pagination: The number of buttons per page really is the capacity per page


Fixed menu pagination: make 'Number of buttons per page' truly the capacity per page, instead of dividing the total number by the number of pages.

Original expression (two places in menu.c):
    img->maxbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) / img->nmenus;
    img->resbuttons = Min(MAX_BUTTON_Y_NUMBER - 2, totntracks) % img->nmenus;

should be: maxbuttons = the number of buttons that can be drawn per page, resbuttons = the few extra buttons on the last page.
But the original formula first truncates the total **to 32**, then **divides by the number of pages**, so no matter how large nmenus is set,
All pages combined have at most 32 buttons — for a disc with 89 tracks, only the first 32 tracks can enter the menu.
The rest cannot be clicked at all (and no error is reported).

comparison: In xml.c hierarchical branches, `maxbuttons = Min(..., ntracks[groupcount])` is the correct one
Correct usage (one group per page capacity) — it can be seen that the original meaning is 'capacity per page'.
The non-hierarchical branch was written incorrectly.

modification method:
    maxbuttons = Min(32, ceil(totntracks / nmenus))   // Capacity per page
    resbuttons = 0                                     // The last page is naturally truncated by the loop

By passing `-6 --nmenus=ceil(total_tracks/target_tracks_per_page)` from the caller, all tracks can be covered.
(menu_characteristics_coherence_test will deduce ncolumns from nmenus,
 so when nmenus = number of groups, ncolumns is exactly 1 — a single-column long list.)


### --background's "one per page" actually takes effect


Make `-b/--background`'s "one background per page" actually effective (automatic menu path).

`-b` accepts a comma-separated list of background jpgs (one per page), parses them, and stores them into
`img->backgroundpic[]`. But at the end of option parsing, the copy loop copies **for every page** from
`backgroundpic[0]`:

    copy_file2dir_rename(img->backgroundpic[0], tempdir, "bgpic0.jpg", ...);
    for (u = 1; u < img->nmenus; u++)
        copy_file2dir_rename(img->backgroundpic[0], tempdir,
                             "bgpic<u>.jpg", ...);          // <- again [0]

As a result, bgpic0..N.jpg under tempdir are all the same, and `-b` is essentially ineffective,
all menu pages can only share a single background.

Fix: for page u, take from `backgroundpic[u]`; if the list is not long enough or the item is empty, fall back to [0]
(consistent with the original behavior, does not affect the old method using a single background).

After the fix, `-b` can be used to assign different backgrounds for each page — for example, "one album cover per page."


### Rewrite --screentext parsing (the original version crashes as soon as it is used)


Fix `-O/--screentext` (menu text links) — current version crashes as soon as used.

There are three flaws in the call points (generate_menu_pics in menu.c, screentext branch),
which together cause `--screentext` **any input to segfault**:

1) `basemotif = fn_strtok(chain, '=', ..., 1, cutloop, remainder)`

   `count` is the "consume several substrings" counter passed to `cutloop`, and cutloop is
   `++loop; if (count > loop) return 1; else { loop=0; return 0; }`
   — with 1 passed in, substring 1 returns 0 and terminates the loop.
   Whereas the semantics of fn_strtok when breaking is: `array[0..k-1]` is valid, `array[k]` is set to sentinel
   NULL, `remainder` = the k-th substring.
   So for k=0: `basemotif[0]` becomes NULL (album title is lost), and `remainder` returns **the whole string**.

   Passing 2 is correct: substring 1 enters array[0], and the loop breaks at k=1,
   `remainder` = the part after the first '='.

2) `dim` is the number of slots **output by fn_strtok** (elements + 1 sentinel),
   but the iteration was written using `for (k = 0; k < dim; k++)`, and on the last read
   `grouparray[dim-1] == NULL` → `strlen(NULL)` segfaults.
   The upper bound must use the actual number of elements.

3) `size` is reused: passed as column width into `&size` as the output parameter of fn_strtok.
   , it is used when the **string is truncated to the length**
   `if (strlen(text) > size) text[size] = '\0'`。
   fn_strtok will overwrite `*size` with the number of substrings (for example, 3),
   Each track title was therefore **truncated to 3 bytes** (only 1 Chinese character).

The other two places `remainder`/`rem` do not initialize VLA, and fn_strtok only when breaking early
only writes them (when the number of substrings is exactly used up, **does not write**) → Reads uninitialized stack memory.
unifies first to `'\0'`.

can only be used after being repaired, `--screentext "Album=GroupTitle=Track1,Track2:Group2Title=Track3,Track4"`.


### Layout changed to use per-page capacity (fix >34 track stack overflow)


menu layout changed to use 'Capacity per page', fixing the stack overflow that would crash groups with more than 34 tracks.

`command->maxntracks` was assigned as **maximum number of tracks for the whole group** (amg2.c:165
`command->maxntracks = MAX(track, command->maxntracks)`），
And the menu layout treats it as 'how many lines per page':

  xml.c  compute_coordinates()：
      uint16_t y0[MAX_BUTTON_NUMBER], y1[MAX_BUTTON_NUMBER];   // 36
      for (j = 1; j < command->maxntracks + 2; ++j)
          y1[j] = ...; y0[j] = ...;
      then also uses y0[command->maxntracks] and y0[command->maxntracks + 1]

`MAX_BUTTON_NUMBER` is 36, so when maxntracks >= 35 it will write out of bounds of the stack array ——
actually tested 40-track group directly `***stack smashing detected***` interrupted.
(Group 1 of Disk 1 has 65 tracks, it will inevitably be affected; this code is not executed when there is no menu, so it was not exposed before.)

The correct 'number of rows' should be the number of buttons per page `img->maxbuttons`
(= Min(32, ceil(total track count / number of pages)), see patch_menu_paging.py),
It is constrained by the upper limit of the screen (<=32 < 34, it will not overflow),
It is the number of lines truly needed for layout — and it must match the coordinates of the spumux button,
Otherwise, the text and buttons will be misaligned (menu.c draws the text, xml.c draws the buttons, both sides share this value).

This patch replaces **all** `command->maxntracks` in menu.c / xml.c with
`img->maxbuttons`. Both instances have been verified that `img` and `command->img` are the same object:
  · amg2.c:82  `#define img command->img`
  · Both functions are called by amg2.c with `img == command->img`
All of these occurrences are within the menu layout path and have no other semantic meaning.


### Page turning arrow: text misalignment + last page repeated 5 times


Fix the menu page-turning arrows: text misalignment (starting from page 2, jumping to the top of the page) + last page drawn 5 times repeatedly.

## Defect 1: The arrow text is misaligned starting from page 2

When `menu.c` draws arrow text, it passes `offset` into `mogrify_img()`:

    mogrify_img(arrowstring, img->ncolumns - 1, img->maxbuttons,
                img, img->maxbuttons, command1, command2, offset, img->arrowcolor);
                                        ^^^^^^ track            ^^^^^^ offset

And inside `mogrify_img()` it is

    y0 = EVEN(y(track + 1 - offset, maxnumtracks + 4));

`offset` means **the global index of track 1 on this page**. It indexes track names (so that track 1 on each page
All are drawn on line 1). But the arrow uses a **fixed absolute line number** `img->maxbuttons` /
`img->maxbuttons + 1` (the arrow slot at the bottom of the page, originating from the same source as the button coordinates output in `xml.c`),
The offset should not be reduced anymore.

So:

| Page | offset | `y(track+1-offset)` | Result |
|---|---|---|---|
| Page 1 | 0 (initial value) | y(13) | Bottom ✔ |
| From page 2 | >0 | `12+1-offset` | **Top 1st and 2nd lines** ✗ |
| Last page | 0 (reset after completing the whole group) | y(13) | Bottom ✔ |

Measured (56 tracks, 5 pages, extracting text ink line ranges page by page):

```
Page 0  Ink ... 408-430, 441-455      ← 441-455 is the bottom Next
Page 1  Ink ... 378-400, 408-429      ← No 441-455! Arrow is drawn on the 1st and 2nd lines
Page 4  Ink ... 288-309, 441-455      ← Bottom Previous
```

In other words, **from page 2 through the second-to-last page (page count minus 1)**, arrow labels overlap the first two track names,
while the bottom button positions have no text — it looks like "Previous/Next not shown after turning pages".

Fix: When calling arrows, always pass `offset = 0` (position using absolute line number).

## Defect two: The last page draws the same arrow 5 times

Originally it was `do { ... } while (buttons < menubuttons + arrowbuttons);`.
`buttons` before entering this loop is **the number of buttons already drawn on this page**, while `menubuttons` still is
"Full page capacity": When the tracks exactly fill the page, the two just connect, but **the last page has only 8 tracks** (56 - 4×12)
When `buttons = 8`, the target is `12 + 1 = 13` → loop 5 times, using the same Previous
Draw repeatedly in the same position.

The isomorphic loop in `xml.c` will output 5 **completely overlapping** buttons, as tested:

```
Page 4   button09..button13 all y0=440..470   ← 5 overlapping buttons
```

Amendment: Change to a single check — there should be no repetition.

## Includes cleaning

Remove `char arrowstring[9]` + `strcpy` (`DEFAULT_PREVIOUS` is 8 characters + NUL = 9,
Just enough to fill this buffer, originally right against the boundary); directly pass the literal to `mogrify_img()`
(This function already has a usage that directly passes a literal.)

`menu.c` and `xml.c` must **remain consistent** (one draws text, the other outputs buttons),
So this patch changes two places at the same time.


### Segmentation fault when --stillpics coexists with the menu


Fixed a segmentation fault when `--stillpics` (still images per track) coexisted with the menu.

`pict` is a file-level static buffer in menu.c, allocated on demand in `create_mpg()`,
The allocation criterion is its own another static (within function) counter `s`:

    static unsigned long s;                 // static inside function
    ...
    if (s == 0)
      {
        s = MAX(strlen(globals->settings.stillpicdir) + 26,
                strlen(img->backgroundpic[rank]) + 1);
        pict  = calloc(s, sizeof(char *));
      }
    ...
    sprintf(pict, "%s" SEPARATOR "pic_%03u.jpg", globals->settings.stillpicdir, rank);

And `generate_background_mpg()` will be set to null when `pict` ends:

    FREE(pict)        // { free(pict); pict = NULL; }

Therefore, on the path of 'first do menu background (ANIMATEDVIDEO), then do still image background (STILLPICS)',
the second time create_mpg is entered, `s != 0` but `pict == NULL`,
directly `sprintf(NULL, ...)` → segmentation fault (tested).

Fix: also use 'pict is null' as a criterion for reallocation.


### --stillpics file list mode will definitely crash


Fix `--stillpics` 's **file list mode** (`--stillpics a.jpg:b.jpg:...`) will definitely crash.

`--stillpics` has two usages:
  · Directory mode: `--stillpics <directory>`, the directory contains pic_000.jpg, pic_001.jpg …
    → this branch will set `stillpicdir` to this directory
  · File list mode: `--stillpics a.jpg:b.jpg:...` (`:` split tracks, `,` multiple per track)
    → this branch **does not set** `stillpicdir`

And `create_stillpic_directory()` unconditionally when entered
`change_directory(globals->settings.stillpicdir, globals)`，
`change_directory` when the path does not exist `clean_exit(-1)`.

The default value of `stillpicdir` is filled in main() before command-line parsing
（`dvda-author.c`: `if (!stillpicdir) stillpicdir = strdup(tempdir)`），
At that time `-D/--tempdir` has not taken effect yet, so it is **compile-time default tempdir**
(<cwd>/.dvda-author/temp, usually does not exist) →
The file list mode must start with
`[ERR] Impossible to cd to <...>/.dvda-author/temp.` + Exit code 255 ended.

Law revision: In the document list branch, point `stillpicdir` to the **currently active tempdir**.
This also makes the paths on both sides consistent — `create_stillpic_directory()` copies the images to
`<tempdir>/pic_%03u.jpg` (k increments only for non-empty items), whereas `create_mpg()` reads
`<stillpicdir>/pic_%03u.jpg`; both must be in the same directory.

After fixing it, you can use an empty item to indicate 'this track reuses the previous image', that is
`--stillpics cover0.jpg::cover1.jpg` (Track 2 uses the image from Track 1)——
This is the key to saving on static image budget of 'only one cover per album' (ASVS maximum 1024 sectors per disc).


### Expand AMG buffer according to menu pages


Stretch the AMG buffer according to the number of menu pages — when there are many pages, `uint8_t amg[sectors->amg * 2048]` will overflow.

in `launch_manager.c`

    sectors.amg = SIZE_AMG + (globals->text ? 8 : 0)
                + (globals->topmenu <= TS_VOB_TYPE);

`SIZE_AMG` is the constant 3, so `sectors.amg` is always 4 (when opening the menu).
And in `create_amg()` of `amg2.c`

    uint8_t amg[sectors->amg * 2048];        // 4 * 2048 = 8192 bytes

Write a menu table that **grows with the number of pages** (the `menusector` branch, starting from `amg[0x1820]`):

    Page Index: 8 * (nmenus - 1)
    Items per page: 0x13A bytes, a total of nmenus copies
    → Requires 0x1820 + 8*(nmenus-1) + nmenus*0x13A bytes

| Pages | Needed | Buffer | Result |
|---|---|---|---|
| 1 | ~6490 | 8192 | ✔ |
| 4 | ~7456 | 8192 | ✔ (Number of pages passed on the 40-track test disk) |
| **9** | **~9066** | 8192 | **✗ Exceeds boundary by about 900 bytes** |

The out-of-bounds write occurred on the **stack** (VLA), so the crash point is far from the real cause: in actual tests, it runs all the way to
The `.data` segment error in `amg2.c` only occurs deep inside `create_amg()`, whereas the earlier menu encoding,
spumux and dvdauthor both completed normally — extremely difficult to pinpoint.

This patch increases `sectors.amg` by pages until it's sufficient. `sectors.amg` also determines
`sizeofamg = sizeof(amg)` and the write disk length, as well as sector pointers in various places
（`2*sectors->amg + ...`、`sectors->amg - 1`、`menusector * sectors->amg`），
So it will automatically adjust accordingly, and the IFO on the disk will also increase, which is self-consistent.


### AMG menu cell end address uses the wrong size


Fixed the issue where the end address of each menu cell in the AMG menu table used the wrong size (causing page jump to fail).

## Symptoms

menu, the 'Next / Previous' page buttons are not working and cannot go back to the previous page (**and there is more than one page**).
and **song selection buttons are functioning normally**.

This 'only the page turn is broken, the song selection is not broken' asymmetry is precisely the clue:
- track selection button goes `jump group G track K` → Located in the audio area via ATSI, unrelated to the menu table
- Page turning button goes `jump menu N` → The player looks up **AMG IFO’s menu PGC table**, and that table is
  `amg2.c` **handwritten** (the reverse-engineered structure)

## root cause

`amg2.c`, the end address of each menu's cell is calculated like this:

    if (j > 1) menuvobsize_sum += img->menuvobsize[j - 2] - 1;   // Accumulate the "previous" pages
    uint32_check(&amg[i], menuvobsize_sum + img->menuvobsize[img->nmenus - 1] - 1 - 1);
                                            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Always the size of the last page **of**

`menuvobsize_sum` is the cumulative total of the previous pages, but what is added is `menuvobsize[nmenus-1]` ——
**the size of the last page**, rather than the current page. So:

- `nmenus == 1` at that time it happens to be the current page → Correct (this is the reason why it has never been discovered)
- `nmenus > 1`, the end addresses of pages 1..n-1 are all wrong

Moreover, **errors will be masked by pages of similar size**: The 8-page VOBs of this project are
37/40/37/43/39/39/41/39 sectors, calculated

    Correct: [35, 74, 110, 152, 190, 228, 268, 306]
    Actual: [37, 73, 112, 148, 190, 228, 266, 306]
                                  ^^^  ^^^            ^^^
                                  Coincidentally the same

⇒ **pages 5, 6, and 8 happen to be correct, the other 5 pages are wrong**, which completely matches “more than one page has a problem.”

Verified in the finished IFO (menu table of `AUDIO_TS.IFO`, 8 addresses appear at equal intervals):

    Values written by code 37/73/112/148/190/228/266/306 ← All found in IFO
    Correct values 35/74/110/152/268 ← None exists

## Fix

Replace `img->menuvobsize[img->nmenus - 1]` with `img->menuvobsize[j - 1]`
(current page), keep the rest of the formula unchanged — behavior is completely unchanged when using `nmenus == 1`.

The corrected formula is equivalent to "number of valid sectors accumulated up to the current page - 1":

    Expected = Σ_{i<j} (sizes[i] - 1) - 1

(Subtract 1 because dvdauthor loses one data sector when handling topmenu, as noted in the original comment)

The start address side does not need to be changed: cell j uses start from `menuvobsize_sum`,
And cell j-1’s end + 1 exactly equals it — both sides are consistent.

Also add two self-checks (printed when `globals->debugging`), as well as the ones in `verify_menu.py`
**Cell continuity check**: `start_j == end_{j-1} + 1` and `start <= end`.


### "One Page per Album" Layout


Change the track selection menu to 'one album per page': Main title = disc title, subtitle = album name.

## Original behavior

Each segment of the menu text link `--screentext` corresponds to an **audio group**, and the pagination is
dvda-author calculates it itself by 'filling R rows per page' (R = ceil(total number of tracks / number of pages)).
So a single page may contain songs from multiple albums, and the only way to distinguish each song is by adding the 'Album Name | ' prefix to each one.

## Goal

    Main title (on every page) = CD title
    Subtitle (top of this page) = Album name
    Only one album per page; when the album changes, the page changes automatically
    Each page background = The cover of the album (change on the asset side, see menu_assets.py)

## Why can (it) be done this way?

The page drawing loop in `menu.c` when `ncolumns == 1` is

    do {
      mogrify_img(grouptext[groupcount][0], ...);            // Subtitle
      offset = track;
      do { mogrify_img(tracktext[groupcount][track], ...); track++; }
      while ((buttons < menubuttons) && (track < ntracks[groupcount]));
      if (track == ntracks[groupcount]) { group++; groupcount++; track = 0; }
      else break;                                            // This section is not finished → turn page
    } while ((group < img->ncolumns) && (groupcount < ngroups));

means **draws one text segment at a time, and within the segment, the music is completed before changing the page**. So as long as
`--screentext` Each section = one album, the number of pages equals the number of albums, and a new album automatically starts on a new page.
`albumtext` (before the first `=`) was drawn by `prepare_overlay_img()` at the top of each page of ****,
just works as a main title.

## needs to change

1. **page is decoupled from the audio group**. The button is `jump group G track K`, G/K is
   "Audio group + track number within the group" is unrelated to "page." One page per album; the page number != group number.
   must convert each page back to (group, first item within the group). Add `compute_menu_pages()` to do this:
   parses `--screentext` to get the number of tracks per page, then splits it according to the number of tracks in the audio group
   "Page → (Group, First track in group)". **If any step does not match, give up entirely** (return 0),
   Caller returns old behavior — rather keep the usable old layout than draw misaligned buttons.

2. **The number of lines per page should be based on the actual number of tracks on that page**. The original formula `maxbuttons` is
   `ceil(total_tracks/page_count)` A global value, if the number of tracks in each album is different, there will inevitably be pages that cannot fit or
   Leave blank. The number of lines also determines the line spacing of the text (`mogrify_img`'s `maxnumtracks`) and
   button rectangle (`compute_coordinates` uses `img->maxbuttons`), so both sides use it
   The same page-by-page retrieval can keep it consistent.

3. **`ntracks[]` to 'Number of tracks per page'**. In `menu.c`/`xml.c`
   `ntracks[k]` (k is the text segment number) was supposed to be 'the number of tracks in segment k'.
   was just originally the segment == the audio group happened to match. Now `amg2.c` passes in the array page by page.

## Why must it be verified instead of just being 'roughly correct'?

Button misalignment **will not report an error**: Just clicking one song plays another. So all the premises
(Number of pages == `--nmenus`, sum of page track numbers == sum of audio group tracks, no cross-group per page,
Explicitly check each page (no more than the number of screen lines); if any do not meet the requirement, the whole is rolled back.

## Rollback

Disabling this script (and removing it from the build script's patch list) will return to the old layout;
`[1b]` will restore menu.c / xml.c / amg2.c / structures.h from the original source code.

---

# Appendix: Original patch mechanism (discontinued)

's modifications were fixed into the source code, this project used 25 Python patch scripts to 'replay' onto the upstream source code. This mechanism was discontinued on 2026-09-24, and **was changed to directly submit to the source tree's git** (see the beginning of the document).
The following sections explain the pitfalls encountered at the time, and why **was changed to git**.

## ⚠️ Running these patches on a solidified source tree, **will mostly report MISS — this is normal**

These patches are applied by **in order**, and the later patch often overwrites the landing point of the previous patch.
So when using them one by one to run through the source trees that have already been fully applied, the `old` of the first few had long been overtaken by the later ones
patch has been changed → reports `[MISS]`. This does not mean the changes are lost.

**criterion does not look at these outputs, but at the source code itself**. Actual test (2026-09-24) among 25 patches
18 reports, `[SKIP] Already applied`, 7 reports `[MISS]`, but checked one by one in the source code,
**7 are in**:

| Patch for reporting MISS | Evidence in the source code |
|---|---|
| `patch_asvs_image_sectors.py` | `asvs.c:162` `uint32_copy(&asvs[k], totpicsectors)` (base_sect global); `:167` `titlesectors` (off_sect relative) |
| `patch_ats_pack.py` | `ats.c:300` `if (length < 6)` Zero-padding branch; `:323` `uint8_t ff_buf[length + 1]` |
| `patch_atsi_dynamic.py` | `atsi2.c:193` `atsi_cap = ((2049 + ntracks*96 + 2047)/2048 + 1) * 2048`；`:194` `calloc(atsi_cap,1)` |
| `patch_menu_paging.py` | `menu.c` in `compute_menu_pages` (2 places) |
| `patch_menu_backgrounds.py` | `command_line_parsing.c:199` `cli_background_list`; `:2517` set; `:2704` use it |
| `patch_menu_screentext.py` | The same piece has been rewritten |
| `patch_asvs_nav_sectors.py` | `menu.c`'s `dvda_rewrite_nav_sector()` (**its effect is now in C**, the script itself has been deprecated —— the `02_build.py` code segment it was supposed to modify has been deleted) |

**the strongest criterion is still `.o` oracle** (next section): 18 `.o` compared one by one with references,
Instruction sequences show **zero differences in all 18/18 objects**, demonstrating functional equivalence to the source that generated the working ISO.

---

## Patches must be idempotent—here are three that never existed

**`old` replaced by inserted** **remain after insertion (new content is inserted before it**),
So you can't use `old not in text` to check if 'applied'—that way, every run would **insert another**.

Tested (2026-09-24) Three patches made this mistake:

| Patch | Performance |
|---|---|
| `patch_read.py` | The `g_last_pkt_pos` declaration and the `g_last_nb_samples` statement were each duplicated into 3 copies |
| `patch_encode.py` | Insert duplicates of the same |
| `patch_menu_amg_size.py` | `NEW = OLD + growth_block`, the OLD always matches → AMG growth block stacked by 3 |

**criterion**: `if new in text: SKIP`, not `if old not in text: MISS`.
All three have been corrected, and retesting using the method of "running the snapshot → → comparing with MD5 → restore" method:
**25 All patches have idempotent** (no byte changes to the run of the pre-solidified source code tree).

```bash
# Retest Method (Reusable)
cd tools/dvda-author-mlp8
find src libutils -type f \( -name '*.c' -o -name '*.h' \) | while read -r f; do
  mkdir -p "/tmp/snap/$(dirname "$f")"; cp -p "$f" "/tmp/snap/$f"; done
for p in scripts/patches/_merged/*.py; do
  before=$(find src libutils -name '*.c' -o -name '*.h' | xargs md5sum | sort | md5sum)
  python3 "$p" >/dev/null 2>&1
  after=$(find src libutils -name '*.c' -o -name '*.h' | xargs md5sum | sort | md5sum)
  [ "$before" != "$after" ] && echo "Not idempotent: $p"
  # If it doesn't wait, reset and continue
done
```

> These patches **are not involved in the build** and are only applied manually when 'rebuilt from upstream'.
> But even if executed manually, being non-idempotent is enough to ruin the source code tree — so it must be fixed.

---

## ⚠️ `SOURCE-MANIFEST.txt` uses `.o` as the oracle verification method

After changes to the source code tree, how do you confirm that the changes are "correct"? Besides comparing the md5 of `SOURCE-MANIFEST.txt`,
A stronger method is **reverse-checking using compiled products**:

```bash
# 1) Backup all .o files before modifying the source code
mkdir -p /tmp/ref_o && cp src/*.o /tmp/ref_o/

# 2) After modifying the source code, recompile and compare the **instruction sequences** one by one (ignoring DWARF line numbers)
objdump -d --no-show-raw-insn /tmp/ref_o/foo.o | tail -n +3 > /tmp/a.dis
objdump -d --no-show-raw-insn src/foo.o          | tail -n +3 > /tmp/b.dis
diff /tmp/a.dis /tmp/b.dis      # No output = functionally equivalent
```

Key points:

- **The same compilation command must be used**. Manually running `make` in `src/` and
  The flags of `build_dvda_author_mlp.sh` are different (missing
  `-Wno-error=incompatible-pointer-types`, etc.), can produce false differences —
  This produced 9990 differing lines for `ats.o`; **rerunning the actual build reduced the difference to 0**.
- The md5 of `.o` is **not reproducible** (DWARF includes line numbers), so it should be compared by **instruction sequence** rather than md5.
- You can also use the opposite: when the source code is lost but the `.o` remains, it uses it to precisely reconstruct **** source code
  (Changing comments does not affect the instruction sequence, only the line number).

---

## Level 1 Menu (Album Index Page): Thumbnail Grid + `--index-pages`

**Requirement**: Before selecting songs, first display a thumbnail of the album cover on one screen, then click which one goes to the album's track selection page.

### Why not use upstream `--menustyle hierarchical`

Upstream does have layered menus (`img->hierarchical`, `nmenus = ngroups + 1`),
But it **** layered by audio group. Each disc in this project has only **1 audio group** (after unifying track parameters,
merge automatically), so that mechanism produces only 2 screens and is unsuitable. We therefore add an **album-based** layer.

### Page Order and Mapping (Pure Position, No Need to Pass Tables)

```
Page order: [Index page 0..I-1] [Album page 0..A-1]

The k-th slot on index page p → Album number a = p * INDEX_PER_PAGE + k
                  → Menu number (1-based) = index_pages + p * INDEX_PER_PAGE + k + 1
```

C side only needs a single number `--index-pages I` to calculate all jump targets.
`INDEX_PER_PAGE = INDEX_COLS * INDEX_ROWS = 4 * 3 = 12`。

### Key Implementation Points

| Location | Method |
|---|---|
| `command_line_parsing.c` | Added `--index-pages N` (Option 43) → `img->index_pages` |
| `command_line_parsing.c` | Add `--index-covers` (Option No. 44) → `img->indexcovers` (flat list, page order × grid order) |
| `structures.h` | `pic` Append at the end `index_pages`, `indexcovers`, `indexcoverssize` (`pic` initialized by position, must be last) |
| `menu.c` `compute_menu_pages()` | The first N sections are index pages: number of grids **not counted in** `total` (`total` must equal the total number of audio tracks); these pages `page_group/t0` set to 0 |
| `menu.c` `dvda_make_index_pages()` | **The screen is drawn here in real time** (see next section): background gradient + grid + vignetting, 4x3 cover collage, album name, thumbnail border |
| `amg2.c` `create_topmenu()` | Call the above after `compute_menu_pages()` and before `generate_background_mpg()` |
| `command_line_parsing.c` | Background copy **skip** index pages (the images of those pages are drawn above); the semantics of `--background` become "only overwrite non-index pages" |
| `menu.c` `generate_menu_pics()` | The index page **does not draw text**, only draws button outlines frame by frame + bottom arrow |
| `xml.c` | Use `jump menu T` for index; spumux uses **grid rectangles**, cannot use `compute_coordinates()` |
| `menu_assets.py` | Only provides **assets**: `MenuPlan.index_covers` flattens cover paths driven by `--index-covers`, album names go through `--screentext` |

### The screen is currently drawn by C (`dvda_make_index_pages()`)

**Why move into C**: Previously, the images were composed using external scripts with ImageMagick, via `--background`
It was brought in. In that case, the geometric constants exist in **two copies** (one in `menu.h` and one in the script), and if you change one place, you might forget the other.
It will be 'the one you select is not the one you wanted.' After moving into C, the **only source** of geometry is `menu.h`.

Material Division of Labor:
- **Cover Path** ← `--index-covers` (Comma-separated flat list, order = page order × grid order)
- **Album Name**   ← `--screentext`(The few paragraphs on the index page `Label=Name1,Name2,...`）

Screen = Three-layer background + Each frame [Cover + Album name] + Thumbnail border:

```
Background: diagonal gradient (top left cyan-blue → bottom right deep indigo) + fine grid aligned with squares + radial vignette
Grid: shrink the cover to INDEX_THUMB size, centered; place the album name below at INDEX_LABEL_H
Name   White text + `caption:` Automatic line break; font size estimated according to index_label_units()
Outline: Draw a dark gray circle around each cell to make the cover 'stand out' from the background.
```

The font size of the album name is estimated in C: `index_label_units()` counts by number of characters (10 for full-width, 5 for ASCII,
UTF-8 first byte determination), then `size = 10 * INDEX_LABEL_W / units` clamped
`INDEX_LABEL_FONT_MIN..MAX`. `caption:` It will also wrap by itself, so the estimate is just that the number of lines increases.
The text is reduced and centered, **will not overflow the cell**.

#### ⚠️ Four pitfalls I stepped on

1. **`-stroke` / `-fill` / `-compose` all mean 'stick'.** They are **settings** rather than
   The operator will remain effective until it is changed. When `-compose multiply` is used and not reset, the following
   `-flatten` is also combined using multiply — the whole page is darkened, the cover is multiplied with the background
   (The measured full-page average dropped from ~60 to 44, and the cover dropped from ~116 to ~40). So after finishing the vignette
   You must use `-compose over`; you must use `-stroke none` before drawing text.
2. **`-repage` must be written inside `( )`**. It is an **operator**, and if parentheses are not added, it will affect each image in the list
   Take effect (including the background of the first page), but the background is moved to the last frame, exposing `-flatten` on the canvas
   The default white background — only one image remains in the lower right corner of the entire page.
3. **The outline should be drawn separately after `-flatten`**. `-draw` will apply to each image in the list,
   When drawn together, the borders may be covered by the subsequent layering order or fall into the coordinate system of a particular small image.
4. **The subtitle on the cover is globally continuous**: `--index-covers` is a flat list, starting from 0 on each page if restarted
   Page 2 will repeat the cover of page 1 (in actual test, the two pages are exactly the same).

#### ⚠️ The generation command must check the **exit code**

Upstream is full of `if (system(...) == -1)` — which is only true when **fork fails**. The command
itself returns `status << 8` when it fails (wrong parameters, file unreadable), which is considered successful.
Therefore, we have encountered situations where 'convert silently fails, background image nonexistent, only the later message
"background image cannot be read" is seen'. `run_convert()` now checks
`WIFEXITED && WEXITSTATUS == 0`, and prints the failed command as is.

The most typical line when parameters are misconfigured: parentheses in `xc:rgb(62,107,138)` are shell syntax characters,
**without quotes is `syntax error near unexpected token '('`** — so all
`rgb(...)` must go through `cs_arg()` with quotes.

### Geometry (the only definition is in `menu.h`)

```
Screen 720x576
   0 .. 59     Main title (drawn by dvda-author's prepare_overlay_img(), ink y=28..54)
  60 .. 480    4x3 grid: row height 140 (60 + 3*140 = 480)
  65 .. 195    Grid 0: thumbnail 100x100 @(40,65), album name 170x28 @(5,167)
 496 .. 552    Page turn arrows (INDEX_ARROW_Y0/Y1)

Grid (col, row) content area = (col*180 + 5, 60 + row*140 + 5), size 170x130
```

- **Top 60 px is for the main title with**: the title is composed of `prepare_overlay_img()` on **every page**
  Drawn at y≈28..54. If the grid starts from 0, it will be covered by the title (tested: title ink
  362x26 at (47,28), while thumbnails start from y=5).
- These constants **are only defined once in `menu.h`**. There is another copy in `menu_assets.py`, but
  it is only used for two things: calculating "how many albums fit on one page" (`INDEX_PER_PAGE`) and **verifying**
  (`verify_menu.py` independently samples pixels to verify the position drawn by C).
- Grid bottom = 480, arrows are at 496..552, the two do not overlap.

### Text style and selection indication

**All text in one style**: White text body + draw the black text again at an offset of (+2, +2)
(= stroke/shadow), identical to the album names on the index page (there they are drawn twice in Python using `caption:`,
here in C, also drawn twice using `-draw "text"`). Offset `TEXT_SHADOW_DX/DY`.

**Text is exactly the same in both states** — selection is indicated only by the small triangle arrow on the left of the line ****.

This is not laziness; it is forced by the hard constraints of DVD subpictures:

| | |
|---|---|
| The entire frame only has **4 palette entries** (the 4th one is reserved for transparency) | `subgen.c` with `s->masterpal[]` a maximum of 4 |
| spumux Each **button's** palette only has **4 entries** | `subgen-image.c` of `checkcolor()`: `if (p->numpal == 4) return false;` |
| Colors are taken from **the colors of each pixel in the image layer itself** | You can't use a second color within the same layer |

Therefore, "unselected black outline, selected turn red outline" is impossible: the outline is the same piece of text offset by 2px,
and must overlap with the main text body, in the overlapping area there will be extra `white text + red outline` combinations. Actual tests show each button's
three-color combination increases to **5 types** → `pickbuttongroups()` all fail →
`ERR: Cannot pick button masks` → `assert(useimg)` abort → the full menu page is lost.

Change: The text in the two layers has **exactly the same** color (black outline + white fill), red is used only for
**graphics that do not intersect with the text**:

| animation layer | content | color |
|---|---|---|
| `impic` | All text (white fill + black outline) | White / Black |
| `hlpic` | Same as above + red arrow on the left side of the row + bordered grid on the index page + page-turn arrow text | White / Black / Red |
| `slpic` | and `impic` (`-pixel` replaced with an equivalent written as white, see below) | white / black |

The three-color combination has exactly 4 types: `transparent` / `white text` / `black outline` / `red arrow`. Tested on page 19
Each button is 4 (≤ 4 pass).

Arrow geometry (`menu.h`): `TEXT_ARROW_W/H/GAP`, left vertex at
`x0 - GAP - W`; when `ncolumns=1` then `x0=45` → left vertex 34, button rectangle starts from 33,
exactly inside and does not overlap with the text (starting from 45).

#### ⚠️ Two pitfalls stepped on

**1. `-stroke` in ImageMagick is "stuck"**

`mogrify_thumb()` uses `-stroke "rgb(255,0,0)"` to draw stroke grids, and this setting **keeps
effective until it is changed**. `-draw "text"` is **drawn simultaneously with `-fill` and `-stroke`**,
so the following text is all obscured by the 6px wide red stroke — the "red text" seen in the picture is actually
"Black text + red outline." Fix: explicitly write `-stroke none` before drawing each text/polygon.

**2. `snprintf` format string does not match parameters → segmentation fault**

When inserting `snprintf` in several places of `-stroke none`, forgot to add `%s` accordingly, causing the parameter table to be entirely misaligned,
`(int) floor(...)` landed on `%s` (int dereferenced as pointer) → `SIGSEGV`
(`dvda-author` exit code -11), the log only contains half of the mplex output, with no errors.
, you must count `%` and the parameters one by one (or watch `-Wformat` warnings).

#### title (CD title) needs to be added to `command1`

The title is baked in `svpic.png` and copied to three layers, so if you want to add an outline next to it, you can only do it additionally
Draw the highlight layer once. **cannot** directly change in `prepare_overlay_img()`
`img->highlightpic[menu]` file — `generate_menu_pics()` will follow immediately
`copy_file(impic, hlpic)` cover it entirely. And `command1` was before that sentence
`snprintf(command, ...)` has been reset once, additions must be made after the reset.

### Page turning button: Only turns pages within the same **block**

is `[index pages 0..I-1] [album pages 0..A-1]`, with the two groups forming separate blocks. Inside `menu.h`
`DVDA_HAS_NEXT()` / `DVDA_HAS_PREV()` (`menu.c` shares the same set with two XMLs)
Limit page turning within the block:

| | Index Page | Album Page |
|---|---|---|
| `Next` | `menu + 1 < index_pages` | `menu + 1 < nmenus` |
| `Previous` | `menu > 0` | `menu > index_pages` |

that is to say:

* **The last index page does not have Next** —— you can't use it to jump to the album content page (to go to the album, click on a thumbnail,
  Using Next will guess incorrectly which song the user wants to listen to)
* **The first album page has no Previous** —— going backward by page sequence, it is **the last index page** (across blocks),
  And 'Back to Index' already has a dedicated `Menu` button (jump to page 1); both buttons do this.
  It will only make people not know which one to press

Slot 1 (line `maxbuttons`) places Previous when there is no Next but there is Previous,
No empty slots will be left; the row number of the `Menu` is **fixed** (`MENU_BUTTON_ROW`), so
The first album page will leave a line blank (Next in slot 1, Menu in slot 3) — this is intentional:
`Menu` is in the same position on every page, making it easier to press than 'squeezing it up'.

> Note that the next/prev links in `AUDIO_TS.IFO` are **hard-coded in amg2.c according to `j±1`**,
> It has nothing to do with 'whether the button exists'. So `verify_menu.py` checks that particular chain.
> Please check the presence or absence of the button in `xmltemp` / `spu_xmltemp_*.xml`.

### The 'Return to Album Index' button (Menu) on the secondary page

At the bottom of the Level 2 (song selection) page, there is an extra slot, with the content fixed as `jump menu 1` (the first menu page)
It is just the index page 1). It **only appears when an index page exists** (`--index-pages > 0`) ——
Without an index page, there is nowhere to return; the index page itself is not drawn (that page is the index).

Slot order (must be consistent with the numbering order of the following three positions):

| Slot | Copy | Redirect | Appearance Condition |
|---|---|---|---|
| 1 | `Next` | `jump menu menu+2` | not the last page |
| 2 | `Previous` | `jump menu menu` | not the first page |
| 3 | `Menu` | `jump menu 1` | second-level page and `index_pages > 0` |

line number = `MENU_BUTTON_ROW(img)` = `maxbuttons + 2`. **three places must be consistent**:

* `menu.c` `generate_menu_pics()` —— draw text (`mogrify_img`'s track)
* `xml.c` dvdauthor XML —— output `jump menu 1`
* `xml.c` spumux XML —— output button rectangle

therefore `xml.c`'s `compute_coordinates()` needs to fill y into `maxbuttons + 2`;
missing one line will read uninitialized coordinates (historically it was exactly this coordinate that caused spumux to report
"Button coordinates out of range" and output 0 bytes).

⚠️ The arrow text on the second-level page is stacked vertically in the same column at the lower-left corner of **** (`x0[0]=33`):
`y()` assigns 48 px per line, so `Next`/`Previous`/`Menu` are at y≈384/432/480,
not overlapping each other. To change the layout, modify `x0[img->ncolumns - 1]`'s value —
but it must be synchronized with spumux's `x0[]`.
- Modifying these constants requires changing three places **at the same time**: `menu.h` (definition), `menu.c`
  (`mogrify_thumb()` drawing the outline), `xml.c` (spumux button rectangle),
  and `menu_assets.py` (background image). If they are inconsistent, it will cause 'the clicked item is not the one you want to select'.

### ⚠️ Three pitfalls that have been encountered (all hard to check)

**1. The page-turning arrows on the index page obtained uninitialized coordinates**

The code for the page-turning arrows executes unconditionally after `if/else` chain **, using**, with
`x0[]/y0[]/x1[]/y1[]` — those arrays are filled by `compute_coordinates()`,
but the index page does not call it at all (the grid does not use 'row division'). The result is garbage values:

```
ERR: Button coordinates out of range (720,576): (33,10944)-(708,35056)
spumux: subgen-image.c:901: imgfix: Assertion `useimg' failed.
```

spumux directly fails, `topmenu0/topmenu1` **outputs 0 bytes**, so
`menuvobsize[0..1] = 0` → `create_amg()` contains
`cell_end = sum + 0 - 2` **underflows to 0xFFFFFFFE** → the IFO cell address chain breaks.

**fix**: the grid only occupies the top 3 rows (432 px), **the bottom 144 px is reserved for the arrows**;
the arrows use **absolute coordinates** (`INDEX_ARROW_Y0/Y1`, `INDEX_PREV_X0/INDEX_NEXT_X0`),
`menu.c` and `xml.c` on both sides use the same set of constants.

**2. The grid size cannot be written `norm_x / COLS`**

`norm_y / INDEX_ROWS = 576/3 = 192` will evenly divide the entire screen, while the grid only occupies 432 px.
**explicit constant** `INDEX_CELL_W=180` / `INDEX_CELL_H=144` must be used,
otherwise the button area will be misaligned with the Python-generated thumbnails (measured y1 difference 48 px).

**3. Do not hardcode `12` into the jump formula**

It is easy to miss some places when changing the number of grids per page. Actual measurement: `menu * 16` did not change accordingly:
On index page 2, the 2nd grid calculates `tgt = 20 > nmenus(19)` → `break`,
only outputs 1 button, whereas spumux outputs 5 →
Self-check `Button counts match (jumps == rectangles)` **caught it**:

```
[Menu][FAIL] Button count mismatch (jump vs position): Page 2: 3 vs 7
```

### Contents in the grid (thumbnail + album name)

The album name **is not rendered by the C side**, but by Python baked into the background jpg together with the thumbnail.
Reason: The name is static, and it requires 'line breaks according to the actual layout + reduce the font size if it's not wide enough.'
grid-by-grid layout; the `mogrify_img()` on the C side arranges points by **rows**, not fitting the grid.

Inside the grid (starting from `col*180+5, 60 + row*140+5`, 170x130):

```
┌─ 170 ──────────────────┐
│   Thumbnail 100x100 (horizontally centered, left indent 40)  │  100
│                (2 px gap)                                 │    2
│   Album name 170x28, centered, auto line break + reduce font size    │   28
└────────────────────────┘  Total 130
```

- The actual measured full cover is all **3000x3000**, so for the thumbnail, take a **square without cropping** ——
  If filling 170x130 (1.3:1), the square cover will have about 23% of its height cropped top and bottom.
- **button area = the entire grid content area** (name is included), outputted by side C, so
  The thumbnail/name size can be freely changed, **will not** affect the clickable area.
- For line breaks/smaller font sizes, use ImageMagick's own `caption:` layout to measure (`-size Wx` height
  adapts automatically), then stepwise reduce the font size, taking the first one where ‘height after line break <= 28’.
  Estimating width by character count for mixed Chinese-English text (`Lulala! Lululala!`) is bound to be wrong.

Practical test on 45 albums in this project: 44 can fit in a single line at **17 pt**, only
`星炬不熄 [毕业合唱 Version]` lowered to 13 pt, no truncation. Name bar is only 28 px high,
**cannot hold two lines** (two lines need at least 30 px), so line breaks only happen if the name is particularly long;
If even 9 pt does not fit, it will be truncated with ellipsis (without allowing `caption:` to crop the lower half-line).

### ⚠️ `mogrify` missing a space at the end of the command string → silently draws nothing

The stroke frame of the index page (`mogrify_thumb()`) and the page-turn arrows (`mogrify_arrow_abs()`)
are to be applied from `strcat` to `command1`/`command2` as fragments, then finally append the output file path.
Forgot a space at the end of the format string, and the two got stuck as one parameter:

```
… -draw "rectangle 545,293 715,427""/path/hlpic0.png"
```

mogrify can't get the output file → write the result to stdout → **exit code 0**, the file remains unchanged.
Symptom: **button boxes and arrows do not appear at all, but the log is fine** (button self-check only checks the XML
ID, cannot find the screen).

Album pages are fine because `mogrify_img()` already had a space at the end of the format string.
Fix: add a space at the end of both functions, and align with `mogrify_img()`.
`02_build.py` `check_menu_overlay()` now makes it a build-time self-check:
The ink on each page of **`hlpic<N>.png` must be more than `impic<N>.png`**,
and the arrow strap on the index page must have ink as well. See section 22 of `docs/TROUBLESHOOTING.md`.

### ⚠️ `-repage` is a **operator**, if not inside parentheses it will ruin the entire page.
The grid generation uses 'layer-by-layer `-repage +x+y` positioning, finally `-flatten`'.
Key: **`-repage` must be written inside `( )`**.

It is an IM **operator** (operator) rather than a setting, so if not parenthesized it will affect
'each **in the current image list**' — including the first 720x576 black background image.
The resulting black background also gets moved to the last slot, exposing `-flatten` default **white background** on the canvas:

```
Symptom: only one cover in the bottom right of the page, the rest are all white (the white is not a cover, it's the default background of flatten).
     Full page average = 238 (normal ~86)
```

Stepped on **twice**: the first time was the thumbnail layer, the second time was the `caption` layer when adding the album name
(wrote as `... caption:Name ) -repage +5+111`, parentheses closed early).
`magick` does not report errors for either writing method ****, it can only be found by measuring pixels.

### Verification

```bash
dotnet run --project src/DvdaMaker.Cli -- verify menu
# Check specified ISO:
dotnet run --project src/DvdaMaker.Cli -- verify menu --iso /path/to/disc.iso
```
disc2 (17 albums / 56 tracks) actual test:

```
[OK] Number of menu pages = 19 (expected 19)          ← 2 index pages + 17 album pages
[OK] Overlay self-check passed (19 pages: all highlight layers have button frames/underlines, 2 index pages have page-turn arrows)
[Menu] Number of buttons consistent (19 pages, jump == position, all 17 album pages have 'Return to Index') ✔
[OK] Page-turn links: 19 pages cell addresses continuous (spans 27/28/41/41/43/..., last page end=746)
     Address chain 0→746 matches AUDIO_TS.VOB sector boundaries page by page
[OK] Menu screen not solid color (average 231, standard deviation 71)—— background image effective
```

Verification of index page content itself (`verify_menu.py` does not handle this)—— measure the cover average for each grid
With the brightest pixel of the name:

```python
# Each grid must have cover average > 3 and name brightest > 200
```

> Screen average increased from 100 to 231 —— secondary menu background darkening reduced from 70% to 35%
> （`DVDA_MENU_COVER_DIM`）。

---

## Play still image (cover): ASVS record count = number of titles, and encoding reaches the specification limit

During playback, the cover displayed for each track uses `AUDIO_SV.VOB` + `AUDIO_SV.IFO` (ASVS).

### ⚠️ Strict requirement: The number of ASVS records must be equal to the number of titles

Using a disc that **can display still images normally** (`E:\鸣潮DVD_Audio_ext`), you can match the files one by one:

| | `TITLE_MODE=one` | `TITLE_MODE=album` | Drive E (can produce images) |
|---|---|---|---|
| `AUDIO_SV.VOB` | md5 is **the same** as E drive | md5 is **the same** as E drive | — |
| ASVS `0x0D` Record Count | **1** | **28 / 17** ✅ | 28 / 17 |
| AMG `0x1000` Main Title | 1 | 28 / 17 ✅ | 28 / 17 |
| Real Device Static Image | ❌ Not Displayed | ✅ | ✅ |

The static image entity (`AUDIO_SV.VOB`) is **byte-for-byte identical** in both modes —
So 'unable to read the disc' has nothing to do with the bitrate, it only relates to the **number of IFO records**.
Records segmented by album (`picture_count/first_picture_number` = `2/1, 5/3, 1/8, 3/9…`），
that is **an album one record** → `02_build.py` default `DVDA_TITLE_MODE` is `album`.

> ⚠️ Once set the default to `one`, with the reason being "the player has defects with multiple-record ASVS"—
> **That conclusion is wrong**, because the benchmark disk used for comparison is itself a `one` product.
> Using a disk that cannot be read as a reference naturally leads to the opposite conclusion. The benchmark for **A/B must be the usable one.**

### Code: `-q 1 -b 9800 -H`

`create_mpg()` originally only transmitted `-f 8 -n <norm> -a <aspect>`. A still image is **single-frame** picture,
When playing, the entire track needs to be paused, so 'bitrate' has no significance for the viewing experience — what determines clarity is only
**How many bits were used for this frame**, which is determined by the default value of mpeg2enc: cover 30.6 dB.

| link | PSNR |
|---|---|
| `jpeg2yuv` Pass-through (except mpeg2enc) | 38.7 dB ← Ceiling |
| After passing through mpeg2enc, default parameters | 30.6 dB |
| only sweep `-q` from 12 to 1 | 30.1 → 30.8 (almost not moving) |
| **`-q 1 -b 9800 -H`** | **33.0 dB** |

`-q` alone is almost useless → the bottleneck is in **bitrate**, not in quantization.
`-b` 9800 is the upper limit of DVD specifications, any higher and mpeg2enc will directly output **0 bytes**
(`-b 20000` / `--no-constraints` both measured as 0 bytes).
`-H` (keep-hf) adds about 0.15 dB; `-K hi-res` is equivalent to it but introduces a custom quantization matrix, not used.

constant is defined in `menu.h` (`MPEG2ENC_QUALITY` / `MPEG2ENC_BITRATE`).
`create_mpg()` is covered by `ANIMATEDVIDEO` (menu background) and `STILLPICS` (playback cover)
and **share**, so the menu background benefits as well.

**cost**: 23 → 32 sectors per frame (`AUDIO_SV.VOB` disc1 2281→3306、
disc2 1424→1960). Static image upper limit 1024 sectors/track, `verify_menu.py` alarm line entire disk 4096,
is all within the range.

### Output: The entire cover centered, without distortion

`menu_assets.make_still()` originally `-resize 720x720` again `-extent 720x576`,
and `-extent` for **larger** image is **cropped** —— 20% of the cover is discarded. Additional compensation is also required
PAL 16:15 pixel aspect ratio (otherwise it will be stretched 6.7% horizontally during playback). See details
`TROUBLESHOOTING.md` Section 31.

### Verification

* `AUDIO_SV.IFO` / `AUDIO_SV.VOB` / `ATS_01_0.IFO` are isomorphic to E disk ****
  (ASVS record count, number of images per segment / starting image number is identical for each entry)
* End-to-end PSNR 33.04 dB (30.86 before improvement)
* Decoded frame cover content area 540~554x576 centered, left and right 60px mean 0 (black border)
* `verify.sh all` All pass; `verify_menu.py` All pass
  (Page 31/19 cell chain complete, playback cover 91/56 images = number of tracks, 28/17 records)

---

## Why switch to git commits

| | Patch script (old) | git commit (current) |
|---|---|---|
| Volume | 25 scripts 4589 lines | One diff, 20 files 1251 insertions |
| Accuracy | **Position matching** Replay, landing points overwritten by subsequent patches cause mismatch | Character-by-character diff, precise |
| Idempotence | Must be ensured individually (previously 3 were non-idempotent, which corrupted the source) | Naturally idempotent |
| Rollback | Manual `cp` Backup | `git checkout` |
| Porting | Path hardcoded in the script | `git format-patch` |

## Rebuild from upstream

```bash
git clone https://github.com/fabnicol/dvda-author tools/dvda-author-mlp8
cd tools/dvda-author-mlp8
git checkout 8fca43a                       # Baseline (shallow clone requires fetch --unshallow first)
git apply /path/to/dvda-author-changes.patch
```

Export change sets:

```bash
cd tools/dvda-author-mlp8
git diff 8fca43a -- src libutils > dvda-author-changes.patch
```

> If these changes have already been committed in the git of the source tree, they can also be directly
> `git format-patch 8fca43a -- src libutils`。
