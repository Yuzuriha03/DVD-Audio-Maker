# Licensing and legal notes

[简体中文](LICENSING.md) | [English](LICENSING.en.md)

> This document includes architecture and distribution notes from earlier project stages. For the current repository and release contents, see the [project README](../README.en.md) and [third-party notices](../tools/win-build/docs/THIRD-PARTY.en.md). Historical workflow descriptions do not describe current runtime dependencies.

This document records the repository's licensing, attribution of third-party components and legal considerations for MLP encoding.

---

## 1. Repository license: GPL-3.0

**The repository is distributed under [GPL-3.0](https://www.gnu.org/licenses/gpl-3.0.html).** See the root `LICENSE` for the full text.

### Why GPL-3.0 is used

This project makes **substantial changes** to the [dvda-author](https://github.com/fabnicol/dvda-author) source.
As a derivative work, it must remain consistent with the upstream license. The repository includes:

- `docs/dvda-author-changes.patch`: the complete changes to 20 source files, ready to apply
- `docs/DVDA-AUTHOR-CHANGES.md` / `DVDA-AUTHOR-DISABLED.md`: rationale
  and code excerpts
- `scripts/build_dvda_author_mlp.sh`: building the toolchain with this project's changes

> At the time of this licensing review, the changes were committed on the `dvda-maker` branch of a full `tools/dvda-author-mlp8` checkout.
> That 1.7 GB upstream clone was not included in the repository; the patch was included instead.

The upstream dvda-author licensing record is:

| Item | Value |
|------|-----|
| Repository `COPYING` | **GNU GPL version 3** (June 29, 2007) |
| Most source-file headers | "GNU General Public License ... either **version 2** of the License, or (at your option) any later version" |
| In-program license statement | "either **version 3** of the License, or (at your option) any later version" |

The source headers say "v2 or later", permitting use under GPL-3.0.
The project also distributes GPL-3.0 in `COPYING`, so this repository uses **GPL-3.0**.

### This repository's own programs

The C# CLI and compatibility wrappers invoke `dvda-author` as a subprocess rather than linking to it. Under the usual interpretation of the GPL,
that arrangement need not make the caller a derivative work, so a separate license could have been chosen. For clarity across the repository and to avoid
mixing licenses, the project uniformly adopts **GPL-3.0**. Removed root-level Python files remain only as historical migration references;
they are neither included in production packages nor invoked by production entry points.

---

## 2. Third-party attribution

### dvda-author

```
Copyright Dave Chapman 2005
Copyright Fabrice Nicol 2007-2019
Copyright Lee and Tim Feldkamp 2008-2009
License: GPL-3.0
```

Copyright in the upstream code belongs to its original authors. This repository supplies modifications as a patch and accompanying documentation.

### FFmpeg

The early workflow described in this review used system FFmpeg (tested with 8.0.1) for decoding and MLP encoding:

| Build or component | License |
|------|------|
| Default FFmpeg build | **LGPL v2.1 or later** |
| Ubuntu/Debian or builds using `--enable-gpl` | Some files are **GPL v2 or later**; the combined build must be treated as GPL |
| **MLP codecs** (`mlpdec.c` / `mlpenc.c`) | **LGPL v2.1 or later** |

Linking compatibility considered in that review:

- dvda-author (GPL-3.0) with FFmpeg GPL-2.0-or-later components: compatible because "or later" permits GPL-3.0.
- The MLP codecs themselves are LGPL-2.1+, compatible with GPL-3.0.

To avoid GPL-enabled FFmpeg components, you can build **LGPL-only** FFmpeg yourself,
without `--enable-gpl` or `--enable-nonfree`. The historical workflow needed `libavcodec`,
`libavformat`, `libavutil`, `libswresample` and the MLP codecs.

### mkisofs（cdrtools）

`mkisofs -dvd-audio` comes from Jörg Schilling's cdrtools patch and uses **CDDL**
(Common Development and Distribution License). The repository does not include that binary;
it invokes it through the command line. Obtain it separately and follow its license.

---

## 3. Legal considerations for MLP encoding

**Review this point carefully.**

MLP (Meridian Lossless Packing) is technology owned by **Dolby**. FFmpeg marks its MLP encoder
as **experimental**, which is why its historical encoding command requires `-strict -2`:

```bash
ffmpeg -i input.flac -c:a mlp -strict -2 output.mlp
#                          ^^^^^^^^^^^^ Without this, it reports "Experimental feature"
```

dvda-author's own help text also states:

> `--encode` Use this option to encode to MLP audio.
> This option is based on the ffmpeg encoder and **subject to the same legal
> restrictions as those applying to the MLP ffmpeg encoder**.

The historical toolchain did not use dvda-author's `--encode`: it pre-encoded MLP with the FFmpeg CLI,
then passed MLP to dvda-author for packaging. That used the same encoder, so the same warning applied to that historical workflow.

**Practical notes:**

- Check the rules applicable to MLP encoding in your jurisdiction.
- Keep your sources unchanged, as this workflow does. Consider an LPCM workflow when appropriate.
- This repository supplies a technical implementation, not a guarantee of legal compliance.

---

## 4. Audio content

**The repository contains no audio, music or artwork files.**

Users supply their own source content and must have the necessary rights to use it.
The toolchain contains no decryption or copyright-protection bypass. If an input is damaged,
such as an incomplete protected download, decoding checks report the problem and stop rather than attempting to bypass protection.

---

## 5. Disclaimer

This software is distributed under GPL-3.0 **without warranty**. The author is not responsible for consequences
of using it, including disc-authoring failure, data loss or legal disputes.
