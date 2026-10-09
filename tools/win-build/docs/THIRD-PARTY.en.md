# Third-party components and licenses

The release contains the required runtime pieces of several open-source
projects and project changes. User-facing license and provenance notices are at
the ZIP root as `LICENSE`, `NOTICE-Image.txt` and `NOTICE-Menu.txt`.

| Component | Use | License/source |
|---|---|---|
| `dvda-author-dev.exe` | DVD-Audio menus, navigation and ISO writing | GPL v3, upstream `dvda-author` plus project patches |
| `dvda-menu-nav.dll`, `dvda-menu-spu.dll` | Menu navigation and subpictures | GPL v2 or later, from `tools/menu-native/vendor` |
| `dvda-media.dll` | In-process media operations | Project GPL v3 C ABI linked to required FFmpeg libraries |
| `dvda-image.dll` | In-process image operations | Project bridge code and the upstream ImageMagick/FreeType licenses |
| Rust formats and disc verification | Built-in format parsing, PCM comparison and AOB MLP/LPCM verification | Project GPL v3; LPCM packing rules translated from `dvda-author`, retaining upstream attribution |
| `mlp_encoder.dll` | MLP encoding | Project C17 core, GPL v3 |
| `DvdaNotoCJK-Regular.ttc` | Chinese, Japanese and Korean menu fonts | SIL Open Font License 1.1 |

FFmpeg, ImageMagick and the other libraries are built by the project with only
the workflow features enabled. The release does not contain `ffmpeg.exe`,
`ffprobe.exe`, ImageMagick command-line programs, eac3to or SurCode. Build
scripts, source hashes and full provenance stay in the repository and local
`build` tree; they are not copied into the user package.

When redistributing or modifying the software, retain all license and NOTICE
files and provide source or patches as required by each upstream license. See
the [Windows build instructions](../README.en.md) and the [native runtime
migration](../../../docs/NO-EXTERNAL-RUNTIME-MIGRATION.en.md) for the current
build boundary.
