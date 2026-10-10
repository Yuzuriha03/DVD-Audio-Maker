# Third-party components and licenses

The release contains the project's Rust implementation and required open-source
libraries. License and provenance notices are at the ZIP root as `LICENSE`,
`NOTICE-Image.txt` and `NOTICE-Menu.txt`.

| Component | Use | License/source |
|---|---|---|
| Built-in Rust author and ISO writer | DVD-Audio IFO/AOB, menu orchestration and ISO9660/UDF images | Project GPL v3; rules translated from `dvda-author` retain upstream attribution |
| Built-in Rust menu session and resource management | Menu lifecycle, failure recovery and state reset | Project GPL v3 |
| Statically linked dvdauthor/spumux menu libraries | DVD-Video navigation and subpictures | GPL v2 or later, from `tools/menu-native/vendor`; a small C error-handling and varargs boundary is retained |
| Built-in Rust media adapter and FFmpeg libraries | Decoding, resampling, PCM checks and menu MPEG generation | Project GPL v3; FFmpeg and dependencies retain their upstream licenses |
| Built-in Rust image adapter and ImageMagick/FreeType | Image operations, menu drawing and font rendering | Project GPL v3 and upstream ImageMagick, FreeType and dependency licenses |
| Built-in Rust formats and disc verification | IFO/AOB, MLP/LPCM and image verification | Project GPL v3; LPCM rules retain `dvda-author` attribution |
| Built-in Rust MLP encoder | MLP encoding | Project GPL v3 |
| `DvdaNotoCJK-Regular.ttc` | Chinese, Japanese and Korean menu fonts | SIL Open Font License 1.1 |

Users run a single Rust GUI executable. The author, project adapters and MLP
encoder are statically integrated; required third-party DLLs, fonts and menu data
are embedded. The old project C author, adapter DLLs and C17 encoder have been
removed. FFmpeg, ImageMagick and other libraries enable only workflow features.
The release does not contain `ffmpeg.exe`, `ffprobe.exe`, ImageMagick command-line
programs, eac3to or SurCode.

Build scripts, source hashes and full provenance stay in the repository and local
`build` tree. Retired author source notices and hashes are recorded in
`rust/crates/dvda-author/tests/fixtures/legacy-source-provenance.json`; original
sources can be recovered from Git history. When redistributing or modifying the
software, retain license and NOTICE files and provide source or patches as
required by each upstream license. See the [Windows build instructions](../README.en.md)
and [native runtime migration](../../../docs/NO-EXTERNAL-RUNTIME-MIGRATION.en.md)
for the current build boundary.
