# Native Windows release assembly

[简体中文](README.md) | [English](README.en.md)

This directory assembles a self-contained DVD-Audio Maker release using Windows CMD and C#.

Assembly does not invoke PowerShell, WSL, Bash, an MSYS2 shell, Autotools or Make. Native image/media maintenance builds use Python and MSYS2/MinGW-w64 on Windows; normal C# GUI builds reuse verified artifacts.

## Architecture boundaries

The C# projects build directly with the .NET 10 SDK. Third-party C tools currently have no maintained CMake or Visual Studio projects, so the packager does not attempt to translate the upstream Autotools build.

Prepare a complete Windows x64 tool directory in advance. Its default location is:

```text
tools\win-build\prebuilt\
```

Alternatively, set:

```bat
set DVDA_PREBUILT_DIR=D:\dev\winbuild\menu-bin
```

## Required third-party files

The prebuilt directory must contain at least:

```text
dvda-author-dev.exe
mkisofs.exe
dvdauthor.exe
spumux.exe
spuunmux.exe
jpeg2yuv.exe
mpeg2enc.exe
mplex.exe
mp2enc.exe
fonts\NotoSansCJKsc-Regular.otf
fonts\NotoSansCJKjp-Regular.otf
fonts\NotoSansCJKkr-Regular.otf
```

Include the native tool DLLs and other runtime files. ImageMagick EXEs/XML are no longer required inputs; image-native supplies its own configuration. The packager does not download dependencies. It prunes DLLs only for fully fingerprinted, verified profiles: six unused ImageMagick DLLs for the original profile, plus 74 unused DLLs when the validated MLP-only FFmpeg libraries are present. Unknown builds are retained.

The packager shares identical tables from the three OTF fonts in a standard TTC collection, retaining every SC, JP and KR glyph and regional mapping. It generates ImageMagick configuration with explicit face indices. The prebuilt directory may instead supply fonts/DvdaNotoCJK-Regular.ttc from a previous package; all three regional faces are checked. No characters are removed and the ZIP compression settings are unchanged.

`dvda-author-dev.exe` must include this project's 24-bit MLP, menu, multilingual font and UTF-8 argv fixes. Menu tools must support AMGM and `jump group ... track ...`.

## Runtime assets

Packaging also requires these files from a complete asset tree:

```text
menu\silence.wav
menu\activeheader
```

The default is `tools\dvda-author-mlp8\`. If the partial mirror lacks assets, set `DVDA_SRC_TREE` or use `--source`.

## Build a release

```bat
tools\win-build\build-all.cmd ^
  --source "D:\dev\winbuild\src" ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --output "D:\DVD-Audio-Maker-Release"
```

Available options:

| Option | Description |
|---|---|
| `--repo` | Repository root; normally discovered automatically |
| `--source` | Complete runtime asset tree |
| `--prebuilt` | Windows third-party tool directory |
| `--output` | Release output root |
| `--include-cli` | Optional developer diagnostic package with CLI; the default is GUI only |
| `--framework-dependent` | Compact package without .NET; users install .NET 10 Desktop Runtime x64 |
| `--image-author` | Rebuilt native author and author-build.json directory; default build/image-author |
| `--ffmpeg-libraries` | Verified MLP-only x64 FFmpeg DLL directory; replaces three libraries and prunes fingerprinted unused dependencies |

`build-all.cmd` starts `src/DvdaMaker.Toolchain`, checks inputs, publishes only the x64 GUI by default, copies tools and assets, generates a SHA-256 manifest and creates the ZIP. The standard package neither builds nor includes CLI files or `dvda.cmd`.

Add `--framework-dependent` to omit .NET. Without --output, this profile writes to tools/win-build/release-framework-dependent. Omitting the flag retains self-contained publishing. Both modes generate their runtime requirements at the top of README and in RUNTIME.en.md.

Only `--include-cli` adds the developer CLI, `dvda.cmd` and CLI documentation. That profile shares the GUI runtime, requires identical bytes for duplicate assembly names and fails on conflicts. WinForms and every font glyph are retained. The ZIP compression level is unchanged.

## Native image library

GUI image preparation and the native author use the same x64 `image-native/dvda-image.dll`. ImageMagick EXEs and their old XML configuration are removed from release staging. Build these artifacts once before packaging, or reuse verified image-native and image-author directories:

```bat
python tools\win-build\build-image-runtime.py --msys-root "D:\dev\msys64"
python tools\win-build\build-image-bridge.py --msys-root "D:\dev\msys64"
python tools\win-build\build-image-author.py --source "D:\dev\winbuild\src" --msys-root "D:\dev\msys64"
tools\win-build\build-all.cmd --source "D:\dev\winbuild\src" --prebuilt "D:\dev\winbuild\menu-bin" --framework-dependent
```

ImageMagick and FreeType archives are pinned by SHA-256; extracted upstream files are verified and project patches are regenerated. Recipes use Python 3.12+, MSYS2/MinGW-w64, JPEG/PNG/WebP/zlib development archives and static linking. The author is built from a private snapshot; the supplied source tree and compiler installation are not modified. The old forwarding EXEs are historical test tools.

Daily C# development may copy a complete verified `image-native` directory to `build/image-native` or set `DVDA_IMAGE_NATIVE_DIR` at runtime. Keep configuration, notices and provenance with the DLL. Release type.xml resolves fonts relative to image-native; retain the sibling menu-bin/fonts directory too. See [capabilities and validation](../../docs/INPROCESS-IMAGES.en.md).

An already optimized `menu-bin` can be reused as `--prebuilt` without rebuilding or supplying replacement options. `NativeOptimizationProfile.json` pins the original 118-file bundle; `MinimalFfmpegProfile.json` pins the three rebuilt libraries and 74 additional unused DLLs. Normal imports, delay imports and binary/XML name references are checked before deletion. Unknown or referenced files are retained, and input directories are never modified.

The separate FFmpeg maintenance build requires Windows x64, Python 3.12+, MSYS2 Make/GPG and MinGW-w64 GCC. It verifies the pinned FFmpeg 9.0.2 archive and signature and a pinned NASM download, producing native Windows x64 DLLs.

```bat
python tools\win-build\build-minimal-ffmpeg.py --msys-root "D:\dev\msys64"
tools\win-build\build-all.cmd --source "D:\dev\winbuild\src" --prebuilt "D:\dev\winbuild\menu-bin" --framework-dependent --ffmpeg-libraries "build\ffmpeg-minimal\install\bin"
```

A changed compiler, configuration or version changes the hashes. The packager rejects unverified replacements. Complete the interface, audio and authoring checks described in [the rebuild record](../../docs/MINIMAL-FFMPEG.en.md) before updating the profile. The default mlp profile serves the menu-bin authoring tools; the MLP encoder is a separate component.

## In-process GUI media libraries

GUI audio processing no longer starts external FFmpeg / FFprobe programs. For routine C# development, copy media-native from a validated release to build/media-native; rebuilding native code is unnecessary. The NativeMediaDirectory MSBuild property can select another build input, and DVDA_MEDIA_NATIVE_DIR can select a runtime directory for debugging. Packaging requires all DLLs plus media-build.json and validates hashes, x64 architecture and imported dependencies.

For native maintenance, build the separate media profile (MSYS2 also needs libsoxr and zlib development libraries):

```bat
python tools\win-build\build-minimal-ffmpeg.py --msys-root "D:\dev\msys64" --work-directory build\ffmpeg-media --profile media
python tools\win-build\build-media-bridge.py --msys-root "D:\dev\msys64"
```

This runtime includes audio decoding, SWR/SOXR, MPEG-2/PNG and a small C interface, without FFmpeg command-line executables. It is separate from the MLP-only authoring libraries in menu-bin. See [in-process media](../../docs/INPROCESS-MEDIA.en.md) for scope and validation.

Developer check: `python tools/win-build/test-magick-shim.py <old-menu-bin> <forwarder-exe> <new-test-output-directory>`. Python is used only for development tests and is not shipped.

## Output

```text
tools\win-build\release\
├── DVD-Audio-Maker\
│   ├── DVD-Audio-Maker.exe
│   ├── *.dll / *.json / language resources
│   ├── menu-bin\
│   ├── image-native\
│   ├── media-native\
│   ├── data\menu\
│   ├── config.env
│   ├── MANIFEST.txt
│   ├── README.md
│   ├── THIRD-PARTY.md
│   └── LICENSE
└── DVD-Audio-Maker.zip
```

The self-contained package needs no installed .NET runtime. The compact package requires .NET 10 Desktop Runtime for Windows x64. Double-click DVD-Audio-Maker.exe for the GUI, which imports existing config.env files and saves JSON profiles. MLP uses the embedded native x64 DLL without a standalone encoder EXE.

Extract and distribute the entire directory. The executable depends on the adjacent runtime and application assemblies and cannot be copied on its own.

The GUI requires no FFmpeg or FFprobe installation. The optional M4A/ALAC-to-FLAC normalization feature still needs Metaflac in menu-bin, on PATH or configured in the GUI. The developer CLI retains explicit external FFmpeg paths as a reference backend.

## Use the release

```bat
cd tools\win-build\release\DVD-Audio-Maker
DVD-Audio-Maker.exe
```

Check, preview, build and verify from the GUI. The source checkout retains cli.cmd, gui-debug.cmd and VS Code F5 configurations; see [Development](../../docs/DEVELOPMENT.en.md).

## Diagnosing failures

- A missing prebuilt directory produces an explicit failure, with no Bash or WSL fallback.
- Incomplete directories report each missing EXE or font.
- DLLs are not checked against a fixed list; the provider of the prebuilt directory must ensure it is complete.
- If assets are missing, use `--source` to point to a complete tree containing `menu`.

## Related documentation

- [`../../README.md`](../../README.en.md): project overview
- [`../../docs/DVDA-AUTHOR-CHANGES.md`](../../docs/DVDA-AUTHOR-CHANGES.en.md): third-party source changes
- [`../../docs/LICENSING.md`](../../docs/LICENSING.en.md): third-party licenses
