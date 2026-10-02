# Changes that were tested but not integrated

[简体中文](DVDA-AUTHOR-DISABLED.md) | [English](DVDA-AUTHOR-DISABLED.en.md)

> Historical engineering record: this document preserves experiments, hypotheses and results from their original development stages. Later experiments supersede some earlier conclusions. Disabled approaches are not current configuration recommendations; see the [project README](../README.en.md) for the supported workflow. Literal filenames and glyph samples are preserved where needed to explain the original tests.

These changes **are not in the source tree**. They are kept for two reasons: first, to record "this hypothesis has already been excluded" to avoid repeated experiments; second, in case the evidence changes in the future, the code is still there.

Originally, they were Python patches under `scripts/patches/_disabled/`, and when sorted on 2026-09-24, the basis was summarized in this document (the patch scripts have been deleted).


---


---

### `patch_asvs_video_attr.py`


ASVS's `video_attr` (`AUDIO_SV.IFO` offset `0x18`) was changed to **NTSC**.

## Field meanings (confirmed by experimental verification on 2026-09-23)

`0x43` and `0x53` **only differ by bit4**, and bit4 is exactly `video_format`:

    0x43 = 0100 0011   MPEG-2 / video_format=0 (NTSC) / 4:3
    0x53 = 0101 0011   MPEG-2 / video_format=1 (PAL)  / 4:3
                       ↑ bit4 = video_format

Uses the same encoding as AMG's `amgm_video_attr` (`AUDIO_TS.IFO` offset `0x100`).

## Values of the four discs (matching the static image actual format 4/4)

| disc | static image actual stream | ASVS `0x18` | AMG `0x100` |
|---|---|---|---|
| Bach | 720x480 29.97 NTSC | `0x43` | `00` (this disc has no menu) |
| Li Na | 720x480 29.97 NTSC | `0x43` | `0x43` |
| Enigma | 720x576 25 PAL | `0x53` | `0x53` |
| This project (before modification) | 720x576 25 PAL | `0x53` | `0x53` |

## Why has it now been changed to 0x43

This patch works with `patch_still_ntsc.py`: **static image encoding switched to NTSC (720x480)**,
So ASVS's `video_attr` must also declare NTSC — the declaration must match reality.

Basis: Two commercial discs that never have problems (Bach/Li Na) still images are NTSC; PAL's Enigma
The static image has defects, and the PAL static image in this project is abnormal in the previous/next segment. All other variables have been checked one by one.
tested and excluded (see the complete list of `patch_still_ntsc.py`).

⚠️ The upstream `dvda-author` was originally hard-coded `0x53` (PAL, see `asvs.c`'s
`asvs[0x18] = 0x53; // unknown, or 0x43` —— The author also doesn't know this is a video attribute).
This patch changes it to a value that matches the actual format of the static image.

**The format of the menu screen is not affected** —— Then declared by AMG's `amgm_video_attr` (`0x100`),
is still the standard set by `config.env` (`0x53` for PAL).


---

### `patch_asvs_header_mode.py`


ASVS Header field alignment: `0x18` = video attribute (format), `0x0E` = record count matching value.

## 0x18 is the video attribute of ASVS (tested on four disks, decisive evidence)

`0x43` differs from `0x53` **only by bit4**:

    0x43 = 0100 0011   MPEG-2 / video_format=0 (NTSC) / 4:3
    0x53 = 0101 0011   MPEG-2 / video_format=1 (PAL)  / 4:3
                       ↑ bit4 = video_format

This uses the same set of coding **as AMG's `amgm_video_attr` (`AUDIO_TS.IFO` offset `0x100`)**:

| disc | still picture format | ASVS `0x18` | AMG `0x100` |
|---|---|---|---|
| Bach Brandenburg | NTSC 720x480 | `0x43` | `00` (this disc has no menu) |
| Li Na Selected Works | NTSC 720x480 | `0x43` | `0x43` |
| Enigma       | PAL  720x576 | `0x53` | `0x53` |
| **this project** | **PAL 720x576** | **`0x53`** | `0x53` |

→ **this project is PAL, `0x18` must be `0x53`.**
dvda-author was originally unconditionally hard-coding `0x53`, which happened to be correct (the source code comment says
`asvs[0x18] = 0x53; // unknown, or 0x43` —— even the author didn't know this was the video attribute).

⚠️ **Historical lesson (mistake in this patch before)**: once followed the "single-record format" **Bach** and copied this byte
together as `0x43`. Bach is NTSC, so 0x43 there is correct; this project is PAL,
Copying it over becomes '**Declare NTSC + actual PAL stream**' → The player initializes the decoder according to NTSC,
but what is obtained is a PAL sequence → silent failure → **black screen**.
The "single-record form" only applies to `0x0E`; the **format must follow its own still image and cannot copy the format together**.

## 0x0E

`0x0E` is paired with the number of recordings (`0x0C`):

| disc | number of recordings `0x0C` | `0x0E` |
|---|---|---|
| Enigma | 8 | `0x0012` |
| Li Na / Bach | 1 | `0x0000` |

This project has 1 record, take `0x0000`.
(dvda-author unconditionally writes `0x0012`, that is Enigma’s value.)

## Modification method

    asvs[0x18]: keep 0x53 (PAL — this is already correct for this project, do not change)
    asvs[0x0E] : 0x0012 → 0x0000

## Idempotent / error correction

This patch will **correct source code already in an erroneous state** (versions that once wrote 0x18 as 0x43),
re-run and it will automatically fix itself, no manual rollback needed.


---

### `patch_asvs_palette.py`


ASVS palette (64 bytes starting at 0x20) is changed to values from the commercial disc: `00 10 10 10` × 16.

## According to (four-disc actual test)

| disc | starting at 0x20 16 sets | cover |
|---|---|---|
| Bach (1 title / 18 tracks) | **`00 10 10 10` × 16 (identical)** | ✔ Normal |
| Li Na (1 record / 24 tracks) | **`00 10 10 10` × 16 (identical)** | ✔ Normal |
| Enigma (8 records / 99 tracks) | `00 10 80 80` × 16 | ✔ Normal |
| This project | `00 e6 80 7f` / `00 00 00 00` / `00 e6 80 7f` / `00 90 22 35` / `00 88 b3 3a` / the rest 0 | ✗ Black screen |

Upstream command line `--active*-palette` (**menu** color scheme: background/text/highlight/selected)
Write the values into ASVS — but ASVS's palette serves **static display**, using menu colors in
PowerDVD shows as **black screen** (image not displayed and does not switch with track).

Two "single record" commercial discs (Bach, Li Na) are both `00 10 10 10` × 16,
that is, the same YUV = (0x10, 0x10, 0x10) color, 16 groups identical. This project aligns them.

## Modification method

Originally in `asvs.c` there are 4 `uint32_copy` (take `img->active*-palette`),
plus a commented-out section of 11 values. Replace the whole with a loop writing `0x00101010` × 16.


---

### `patch_still_bitrate.py`


Encode still images with a peak bitrate cap to keep ASVS size within the player buffer.

## Based on (confirmed by actual test on 2026-09-23)

**The total number of ASVS still images must be ≤ 1024 sectors (2 MB)** —— this is built into the dvda-author source code
Limit (`asvs.c`: `if (totpicsectors > 1024)` warning
"Exceeding stillpic buffer limit (2 MB)"), which is also the actual buffer limit of the player.

Five disks were tested, **completely matches**:

| Disk | Number of still images | Total ASVS size | Result |
|---|---|---|---|
| Li Na | 12 | **436 sectors** | ✅ Images displayed |
| P Edition (3 small album disks) | 17 | **431 sectors** | ✅ **Images displayed (confirmed on actual device)** |
| Bach | 18 | **792 sectors** | ✅ Images displayed |
| Enigma | 99 | **1950 sectors** | ❌ User confirmed still image issues |
| G version | 56 | **1424 sectors** | ❌ **completely fails to display** |

→ Both two images above 1024 are all abnormal, all three below 1024 are normal.
(Previously mistakenly thought "1 record is fine", then mistakenly thought "2048 sector limit", both are wrong.)

## Measured number of sectors per single image at various bitrates

720×576 still image, including mplex navigation sectors (measured through mplex, not calculated):

    Default    25 sectors/image    56 images = 1400  ❌
    -b 8000 27             56 images = 1512  ❌
    -b 6000 21             56 images = 1176  ❌
    -b 5000 18             56 images = 1008  ⚠ Stuck frame
    -b 4500 17             56 images =  952  ✅ Remaining 7%
    -b 4000 16             56 images =  896  ✅ Remaining 12%
    -b 3000 14             56 images =  784  ✅

## Why not rely on JPEG quality

`make_still` Already used `-quality 90`; practical tests show reducing JPEG quality to 60~80 has almost no effect on final
MPEG-2 size **(the output of mpeg2enc is controlled by the bitrate):**

    jpeg q90 → 45634 B (23 sectors)      jpeg q60 → 49318 B (25 sectors)

What is truly effective is `-b` of mpeg2enc (peak bitrate).

## Modification method

In `menu.c`, still images and menu backgrounds share the same set of mpeg2enc parameters. Only when
`img->action == STILLPICS` (playing still images) is `-b 4500` added, while menu backgrounds remain unchanged.

⚠️ **Disc 1 (91 images) requires attention to**: 91 × 17 = 1547 sectors, still exceeding 1024.
If Disc 1 also needs to output images, each image must be reduced to ≤ 11 sectors (about `-b 2000`), or reduce the number of images.


---

### `patch_still_headers.py`


Still-image MPEG-2 header-aligned commercial disc: `progressive_sequence=1` + declare bitrate 9.0 Mbps.

## According to (2026-09-23, literal byte-by-byte test on five discs)

In the MPEG-2 **sequence header + sequence extension** of the still image, there are two items that are inconsistent with the three commercial discs:

| items | Bach | Li Na | Enigma | This project (before modification) |
|---|---|---|---|---|
| `progressive_sequence` | **1** | **1** | **1** | 0 |
| sequence header declared bitrate | **9.0 Mbps** | **9.0** | **9.0** | 3.2 / 7.5 |

Original bytes (sequence extension header `14 8a` vs `14 82`, the difference is precisely bit12):

    Commercial disc: 00 00 01 b5 | 14 8a 00 01 00 00    progressive_sequence=1
    This project: 00 00 01 b5 | 14 82 00 01 00 00    progressive_sequence=0

### Why it is a safe and self-consistent change

In this project, each static image actually encodes a **single progressive I-frame**, and all the flags are inherently progressive:

    picture_structure      = 3 (Frame)
    frame_pred_frame_dct   = 1
    progressive_frame      = 1
    top_field_first        = 0

Only the `progressive_sequence` bit was written as 0 by `mpeg2enc`. In actual tests, the command line cannot affect it.
(`-I 0/1/2`, `-q`, `-s` all tried, output is always `14 82`; `-I 2` also causes a segmentation fault).
Since the actual bitstream is line-by-line, changing this bit to 1 is a truthful description, not a fabrication.

### Declare bitrate

The sequence header `bit_rate_value` (unit 400 bps) and `vbv_buffer_size_value` constitute the VBV model:
The declared bitrate only needs to be **no lower than** the actual peak. For commercial versions, always declare 9.0 Mbps (= maximum setting).
This project states the actual value used by the encoder. Changing it to 9000 is consistent with commercial drives and has no impact on the data.

## Amendment

`dvda_pad_program_end()` (the post-processing added to menu.c by `patch_still_end_code.py`)
In the function) also fix these two places along the way —— it has already obtained the entire mpg segment, and **must be in `stat_file_size()`
Previously**execute (otherwise the ASVS position pointer does not match the actual object).

Only modify the **header bytes**, do not touch any image data:
  · Sequence extension `progressive_sequence` 0 → 1
  · The `bit_rate_value` of the sequence header → 9000 kbps


---

### `patch_still_ntsc.py`


Switch still images to **NTSC** encoding (the menu screen remains in the original format).

## According to (2026-09-23, complete comparison of five charts)

| Disk | Still Image Format | Still Image | Previous Segment / Next Segment |
|---|---|---|---|
| Bach (Commercial) | **720x480 NTSC** | ✅ | ✅ |
| Li Na (Business) | **720x480 NTSC** | ✅ | ✅ |
| Enigma (Business) | 720x576 PAL | ❌ Defective | ✅ |
| This Project | 720x576 PAL | ✅/❌ | ❌ |

**The two commercial discs that never have any problems are NTSC; the two PAL ones always have problems.**

Before this, all other variables had been tested and excluded one by one:

| Excluded | How it was excluded |
|---|---|
| title number (1 / 17) | Q version 17 title, R version 1 title, both upper and lower sections are corrupted |
| ASVS `0x0E` | Change `0x0000` Invalid |
| ASVS palette | Change `00101010` Invalid |
| ATS still image table `f2`/`f3` | Change progression (per Bach/Enigma) invalid |
| ATS still image table `byte1` | Change `0x04` Invalid |
| image index `track` | Change 0 (per Bach) Invalid (W version) |
| still image declaration bitrate | Change 9000 (per commercial disc) Invalid (T version) |
| IFO standard version number | Change 1.1 format Invalid (I version) |
| SAMG absolute sector pointer | Verify track by track correct |
| SAMG audio attributes 12 bytes | Only Bach has them, Enigma/Li Na are all zero → Non-essential |
| ATS PGC / title descriptor / sector table / PTS table | Isomorphic with Enigma field by field |
| att_srpt / aott_srpt | `0xc1` are valid 'last title' markers (Enigma last item as well) |
| AOB layout and ats_last_sector | are all correct |
| Hardware acceleration | After turning off, commercial discs work normally, we still fail |
| Player cache | Clearing it does not improve |

## Modification

`menu.c` in `create_mpg()`, still images and menus share the same set of encoding parameters. This patch only changes the standard to NTSC when
`img->action == STILLPICS`:

    Frame rate : img->framerate  →  '30000/1001'
    Standard   : img->norm      →  'n'

Menu screen (ANIMATEDVIDEO) is not affected, it still uses the standard set in `config.env`.

Supporting changes (see also `menu_assets.py`'s `STILL_W/STILL_H`): still image material should be generated
**720x480**, consistent with this patch's NTSC encoding.

ASVS's `video_attr` (`0x18`) should also be correspondingly changed to `0x43` (NTSC) — see
`patches/patch_asvs_video_attr.py`。


---

### `disc_spec_versions.py`


[Disabled] aligning the specification version numbers of the four IFOs from 0x12 to 0x11 / 0x00.

## Status: not in the build pipeline

Previously handled by `02_build.py`'s `fix_spec_versions()`, the call site has always been written
`if False and not fix_spec_versions(...)`. On 2026-09-24, when organizing the script, moved from
`02_build.py`, original text is saved here to avoid losing the reference.

## Reference (2026-09-23 actual test, five disks)

| Disk | AMG@0x21 | ATSI@0x21 | ASVS@0x0E | SAMG@0x0E | Performance |
|---|---|---|---|---|---|
| Bach | **0x11** | **0x11** | **0x0000** | **0x0000** | ✅ Static images completely normal |
| Li Na | **0x11** | **0x11** | **0x0000** | **0x0000** | ✅ Static images completely normal |
| Enigma | 0x12 | 0x12 | 0x0012 | 0x0012 | ❌ Only the first few albums refresh |
| This project | 0x12 | 0x12 | 0x0012 | 0x0012 | ❌ Same as above + occasional crash |

`dvda-author` upstream unconditionally writes 0x12 (DVD-Audio 1.2), while the two discs **never show any
The commercial discs of** use the 1.1 format. PowerDVD 8's 1.2 parsing path has defects ——
Repeatedly appears in the crash log (`%LOCALAPPDATA%\CyberLink\CLHelper\PowerDVD8.log`)
`CLNavX.ax` 0xC0000094 (integer divide by zero), 0xC0000005 (access violation), and `CLVsd.ax`
0xC0000005, but it never appeared when testing the commercial disk.

## ⚠️ Why hasn't it been merged into the source code?

**Its ASVS rules are the same change as `patch_asvs_header_mode.py`**, and the latter has been practically tested
**Will ruin still images** (together with the palette and the two patches that progress the still image chart, it is regarded as a 'still image killer',
See `patches/_disabled/patch_asvs_header_mode.py`).

    patch_asvs_header_mode.py : asvs[0xE] 0x0012 → 0x0000
    This document RULES[ASVS]        : Byte 0x0F → 0x00 (u16 @0x0E i.e. 0x0012→0x0000)

The same change is considered harmful in one case and stays in the pipeline in another — this is the contradiction found during cleanup.
Since the evidence points to 'harmful,' it should be discontinued altogether (rather than quietly merging the behavior to be verified into the source code).

## The part that can be verified

Based on the G version, only these 14 bytes (4 IFO + 8 copies of SAMG) were modified, as tested by users.
**No longer crashes**. So there is some evidence that 'version number 0x12 is related to the crash,' but 'after changing to 1.1...
Whether the still image can be displayed has no definitive conclusion.

To re-verify: put this file under `scripts/`, in `build_disc()` of `02_build.py`
You only need to call it once before packaging the ISO (the function is idempotent).


---

### `_experiments_18xx/patch_asvs_per_track.py`


Plan B: Change the 'Play Cover' table from **by title** to **by track**.

## Symptoms

When there are multiple tracks in an audio group (currently 'one title per album', with several tracks within the group),
**No matter which song is played**, it only shows the cover of the **first album**.

## Root cause (both tables are organized by title, while the player looks up the table by 'current track')

- `AUDIO_SV.IFO` (ASVS) records are **one per title**:
  `picture_count(u8) + first_picture_number(u16) + first_sector(u32)`, plus `0x378` 2 bytes per image
  Sector table. When 'each track forms its own title,' **one record corresponds exactly to one track**.
- The first byte of ATSI's static image record is '**which record**', determined by `pictitlecount`
  ('Which number has a picture **title**') is given.

It turns out that both are 'by title', and title == track, so they correspond one to one.
After adding multiple tracks to a title: ASVS only writes **1 record**, `pictitlecount` is always **1**
—— So all tracks refer to record #1 → all show the first image.

## Amend the code (only change the iteration granularity from title to track, without touching any unknown structures)

| File | Original | Changed to |
|---|---|---|
| `asvs.c` | Write one record for each **title** | Write one for each **track with images** |
| `atsi2.c` | `pictitlecount` accumulates by **title** | accumulates by **tracks with images** |

**Fully compatible with the original**: When there is one title and one track, the bytes output by both are completely consistent with the original
(At that time, 'tracks with pictures' was 'titles with pictures'.)

In this way, ASVS will write N records (N = the number of tracks with images), while ATSI references 1..N,
**All fall within the table**. Both sides **must be modified in pairs**: changing only the ATSI references will cause it to go out of bounds,
The player crashed directly.

## Why is there no switch?

Earlier, this patch was wrapped with the environment variable `DVDA_ASVS_PER_TRACK`; if not set, it would follow the original behavior.
Since the changes are correct under both layouts (see the compatibility argument above), that switch is just redundant.
Layer one, deleted — this patch now **takes effect unconditionally**.


---

### `_experiments_18xx/patch_asvs_per_image.py`


Change the ASVS record to 'one entry per image.' Match the ATS numbers to increment sequentially (1..56).

Actual device test (PowerDVD):
  · Navigation is normal ⟺ Only one title
  · Cover is normal ⟺ The "figure number" in ATS is a valid ASVS record index

So: there is only 1 title (navigation), ASVS writes 56 records (figure numbers 1..56 exactly index them).
Each record has 1 image, off_sect=0, base_sect=the global start point of that image → absolute position is correct.


---

### `_experiments_18xx/patch_ats_ptt_srpt.py`


Supplement `ATS_PTT_SRPT` (Part Of Title Search Pointer Table).

⚠️⚠️ Status: **is obsolete · do not wire**

Tried it once, results:
  · "Next track" still does not advance;
  · Also, jumping to later tracks in **PowerDVD crashes directly**.

The crash indicates that the layout/values of this table are wrong (or it isn’t the parser entry for "next track")——
The player addresses out of bounds due to wrong pointers. Since it crashes, it must never remain in the build process.

## Why it is wrong

The structure of this table is **inferred from `VTS_PTT_SRPT` of DVD-Video to**:
  · In `ifo.h` of `foo_input_dvda` there is the structure definition for the DVD-Video version;
  · In the dvda-author source code, **completely lacks** PTT-related code;
  · Neither local Docs nor public sources can find the DVD-Audio version definition.

That is to say, for **I do not know the real layout of this** table for DVD-Audio; it was written by analogy.
Changes like these 'guess a binary structure' carry extremely high risks: if wrong, it won't report an error, but will crash the player.

## In the future, if you want to try again, you must meet the requirements first

  1. Only connect the wiring in this script, do not make any other changes (if it fails, it can be instantly reverted);
  2. There is a **known good reference disc of the same type** (commercial DVD-Audio) that can be used to compare this field,
     Rather than relying on inference;
  3. First verify on a disposable test disk; do not touch the finished product.

## The following implementation is for record only

`main()` has been changed to be rejected from execution directly.


---

### `_experiments_18xx/patch_ats_album_rank.py`


Change the ATS drawing number to 'Album Serial Number' to correspond one-to-one with ASVS records by album.

Works with patch_asvs_per_image.py (ASVS writes N records per album).
The grouping comes from the environment variable DVDA_ASVS_ALBUM_TRACKS ("6,5,6,1,...").


---

### `_experiments_18xx/patch_ats_still_manual.py`


The byte1 recorded in the still image is set to 0x04 (= Bach's notation).

Bach (1 title / 18 tracks / ASVS single recording) navigation and cover are normal, and its still image records each track as
  01 04 ...  —— 2nd byte = 0x04
And we write 00. In the source code, 0x04 is only written when `--stilloptions manual` is passed:

    if ((img->options) && (img->options[s]) && (img->options[s]->manual))
      atsi[i] = 0x04;

Here unconditionally write 0x04 (to align Bach).


---

### `_experiments_18xx/patch_ats_still_pertrack.py`


Change the last two fields of the static image record to **different for each track** (according to Bach's rules).

## Based on: Bach (1 title? 18 tracks)

Bach is '1 title + 18 tracks + ASVS single record (number of pictures = 18)', and its **navigation and cover are both normal**.
Its ATS still image table (6 bytes/track) has different values for each track:

    Track 1: 01 04 00 6c 00 75      → Field 2: 108, Field 3: 117
    Track 2: 01 04 00 76 00 7f      → 118, 127
    Track 3: 01 04 00 80 00 89      → 128, 137
    ...each track +10, 3rd = 2nd + 9

That is: Field 2 = 6 × number of tracks + 10 × track sequence; Field 3 = Field 2 + 9.
(The dvda-author original wrote it as 'the entire track is the same' → the player cannot distinguish the current track → track 0.)

---

# Appendix: Historical status records

The following sections record the states that **tried before and were later replaced**, not the current status.
(Originally in `PATCHES.md`, consolidated into this document during the 2026-09-24 update.)

## 'G version' form (2026-09-23 rollback baseline) — ⚠️ Historical record

> **this section describes a past intermediate state, not the current form**
> On 2026-09-23, to troubleshoot the still image issue, it was entirely rolled back to the G form; later (09-24) it also added
> `patch_mpeg2_autodetect.py` (standard/line-by-line self-check) and `dvda_rewrite_nav_sector()`
> (navigation sector rewrite). Therefore the following 'still image MPEG header = upstream original' 'still image navigation sector =
> mplex native "Four IFO version numbers = 0x12" Three **are no longer current**.
> This table is retained because the data in the section "Field values extracted byte by byte from three commercial discs" is still valid.

For disc 2, `测试_G_单记录.iso` was able to display still images normally on real hardware (PowerDVD 8),
its form as the **rollback reference** is recorded as follows, and should be reproducible item by item during reconstruction:

| configuration item | value |
|---|---|
| `DVDA_TITLE_MODE` | **`one`** (one title for the whole disc 1) |
| ATS `nr_of_titles` | **1** |
| ASVS `nr_of_asvs_records` | **1** (56 images recorded, `base_sect=0`) |
| ATS still image table `f2` / `f3` | **upstream constants** `6×track_count` / `(track_count-1)×6 + 15 + (picture_count-1)×10` |
| still image table `byte1` | **`0x00`** (does not open slideshow) |
| ASVS `0x0E..0x0F` | **`0x0012`** (upstream original values) |
| ASVS `0x18` | `0x53` (PAL, consistent with the still image format of this project) |
| ASVS `0x19` | `0x00` (`patch_asvs_no_buttons.py`, **reserved**) |
| ASVS palette | **upstream menu colors** (`--active*-palette` values) |
| Still image program end code | Exclusive sector + fill with `0xFF` (`patch_still_end_code.py`, **retain**) |
| Static image MPEG header | **upstream as-is**: `progressive_sequence=0`, declared bitrate = encoder actual value |
| Static Map Navigation Sector | **mplex Native** (PCI 980B + DSI 1018B, including null pointer) |
| Four IFO standard version numbers | **`0x12`** (upstream original values) |

Corresponding build parameters:
```
DVDA_TITLE_MODE=one bash local-bin/dvda.sh one 2
```

### 2026-09-23 Patches disabled when rolling back

The following patches are **retained in `patches/` directory** (with complete basis records), but **has been removed from
`build_dvda_author_mlp.sh` 's `PATCH_SEQ` and commented out in**, restoring upstream original behavior.
The reason for deactivation is 'rollback to version G status'—version G (`DVDA_TITLE_MODE=one` +
A single ASVS record + upstream static chart fields) was the form that could normally display the static chart at that time.

| Patch | What's changed | Why disabled |
|---|---|---|
| `patch_stills_per_track_rank.py` | After the static image table, the last two fields changed to 'progress by track', `byte1=0x04` | `byte1=0x04` is **pagable slideshow** (`--stilloptions manual`), not the form required by this project; and practical tests could not fix the static image |
| `patch_asvs_header_mode.py` | ASVS `0x0E` → `0x0000` (record commercial disk as is) | actual test changing to `0x0000` would **break the static image**; the upstream original value `0x0012` is what's used by G version |
| `patch_asvs_palette.py` | ASVS color palette → `00101010 × 16` | practical test would break the static image; G version uses the upstream menu color |
| `patch_still_bitrate.py` | Static image mpeg2enc add `-b 3200` (reduce bitrate) | user requires **high-quality static image**; the bitrate limit was added for 'ASVS total 2048 sector limit', and that conclusion was measured during hardware acceleration crash, not reliable |
| `patch_still_headers.py` | Static image header `progressive_sequence` 0→1, bitrate→9000 | The theoretical basis to align with commercial disk is valid, but practical test could not fix the static image, rolled back together for comparison |

> **Note**: The 'data basis' section recorded in these patches still holds (all measured byte by byte from three
> commercial disks), it's just **conclusion not supported by practical test**. Before re-enabling, please first use
> `DVDA_TITLE_MODE=one` + use upstream fields to make a baseline comparison.

### Rolled-back build post-processing (`02_build.py`)

There are two post-processing functions in `build_disc()`, **the code is retained but the calls are under `if False and ...`
Deactivate** (Rollback to G state):

| Function | Purpose | Status |
|---|---|---|
| `fix_asvs_nav_sectors()` | Change the navigation sectors of each still image in `AUDIO_SV.VOB` from the mplex 'empty shell' to the commercial disc format (remove empty DSI) | **Disabled** |
| `fix_spec_versions()` | Align the specification version numbers of the four IFOs to the 1.1 form of Bach/Lina (`0x11`/`0x00`) | **Deprecated** |

Both functions can be **idempotently rerun**; if needed, just remove `if False and `.

---

---

## Three 'Aligned Commercial Units' Repairs (2026-09-22)

These three places were all byte-by-byte compared using three commercial copies as the benchmark (Enigma '15 Years After',
Li Na's selected works, Bach's Brandenburg Concertos), the three are **completely identical**:

| Project | Commercial Development | Before Renovation |
|---|---|---|
| ASVS `0x19` | **0** | 1 (dvda-author hard-coded, comment claims "or 0") |
| Each static image one `00 00 01 B9` | One per image, **occupies a dedicated sector** | squeezed in the last 4 bytes of the segment-end sector |
| ASVS Each image offset table `0x378` | Segmented by title, `base_sect` = global start of that title, `off_sect` relative within segment | globally tiled, never reset |

### Fourth case: ATS static image record's "by track" field — ⚠️ **has been rolled back, not integrated into**

> 2026-09-23 Rolled back when the G version state was stopped. See the record below under "intentionally not integrated."

Bach's "Brandenburg Concertos" is **1 title / 18 tracks / ASVS single record** — along with "single title +
Its static image table (6 bytes/track):

```
Track 1: 01 04 00 6c 00 75     → Field 2: 108, Field 3: 117
Track 2: 01 04 00 76 00 7f     → 118, 127
Track 3: 01 04 00 80 00 89     → 128, 137
```

The pattern deduced at the time: **byte1 = 0x04**; **Field 2 = 6×track number + 10×track order**;
**Field 3 = Field 2 + 9**. Patch `patch_stills_per_track_rank.py`.

**But byte1 = 0x04 is wrong** — it corresponds to `--stilloptions manual` in the source code
('Enable browsable (manual advance) pictures'), that is, **slides that can be manually turned**,
This is not the form required by this project. The patch has been disabled, and subsequent tests have not been able to prove that it can fix static images.

---
