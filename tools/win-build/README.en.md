# Windows x64 builds and packaging

[简体中文](README.md) | [English](README.en.md)

The current distribution profile is **GUI-only, Windows x64, framework-dependent**. Users install .NET 10 Desktop Runtime x64; developers and packagers use the .NET 10 SDK. Pass `--framework-dependent` explicitly. Omitting it still creates a self-contained package.

Routine C# builds and packaging reuse prepared Windows artifacts. Python, MSYS2/MinGW-w64, Make and development libraries are needed only when maintaining native components, not when running the product.

## Packaging inputs

| Input | Default location or option | Contents |
|---|---|---|
| GUI media runtime | `build/media-native` | DLLs and media-build.json |
| Image runtime | `build/image-native` | dvda-image.dll, image-build.json, XML and NOTICE.txt |
| Author with in-process images | `build/image-author` or `--image-author` | dvda-author-dev.exe and author-build.json |
| Tools and fonts | `--prebuilt` or DVDA_PREBUILT_DIR | Complete menu-bin runtime dependencies |
| Authoring assets | `--source` or DVDA_SRC_TREE | menu/silence.wav and menu/activeheader |

Git does not contain the full third-party tool bundle. tools/dvda-author-mlp8 is a partial review mirror, not an independently buildable tree or complete asset source.

The prebuilt directory needs these programs and their DLL dependencies:

```text
dvda-author-dev.exe   mkisofs.exe      dvdauthor.exe
spumux.exe            spuunmux.exe     jpeg2yuv.exe
mpeg2enc.exe          mplex.exe        mp2enc.exe
```

Provide NotoSansCJKsc/jp/kr-Regular.otf, or the verified `fonts/DvdaNotoCJK-Regular.ttc`. Packaging shares identical font tables while retaining all glyphs and regional faces. magick.exe, convert.exe, mogrify.exe and identify.exe are no longer required inputs.

The base author must include the project's MLP, timeline, UTF-8 and menu fixes. Packaging replaces its staging copy with the verified `--image-author` build. The other menu tools still need the appropriate AMGM/jump support.

## Build the compact package

Run from the repository root, replacing example paths with prepared directories:

```bat
tools\win-build\build-all.cmd ^
  --framework-dependent ^
  --source "D:\dev\winbuild\src" ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --image-author "build\image-author"
```

| Option | Purpose |
|---|---|
| `--repo` | Repository root, normally detected automatically |
| `--source` | Full asset tree; packaging does not compile this tree |
| `--prebuilt` | Windows tools/fonts, default tools/win-build/prebuilt |
| `--image-author` | Rebuilt author directory, default build/image-author |
| `--output` | Release output root |
| `--framework-dependent` | Exclude .NET; the current distribution profile |
| `--include-cli` | Optional developer diagnostic package |
| `--ffmpeg-libraries` | Optional verified MLP-only authoring library replacements |

The compact profile defaults to:

```text
tools/win-build/release-framework-dependent/
├── DVD-Audio-Maker/
│   ├── DVD-Audio-Maker.exe
│   ├── Application DLLs, JSON and language resources
│   ├── media-native/
│   ├── image-native/
│   ├── menu-bin/fonts/
│   ├── data/menu/
│   ├── config.env, MANIFEST.txt
│   └── README, RUNTIME, THIRD-PARTY, LICENSE
└── DVD-Audio-Maker.zip
```

Without `--framework-dependent`, output defaults to tools/win-build/release and includes .NET. The normal package has no dvda.exe/dvda.cmd; source-tree cli.cmd and VS Code debugging remain available. Releases stay in ignored directories, and ZIPs are distributed through GitHub Releases rather than Git commits.

## Reuse native components for C# debugging

Point runtime loading at a complete release that matches the current source, retaining its font/configuration layout:

```bat
set "DVDA_MEDIA_NATIVE_DIR=D:\DVD-Audio-Maker\media-native"
set "DVDA_IMAGE_NATIVE_DIR=D:\DVD-Audio-Maker\image-native"
gui-debug.cmd
```

These variables select runtime locations; packaging still needs the build inputs above. When copying components, retain media provenance and image configuration/notices. Release type.xml resolves fonts relative to image-native: copying only the DLL, or using that configuration without the sibling menu-bin/fonts directory, is insufficient.

Native output defaults to build. MSBuild properties NativeMediaDirectory and NativeImageDirectory select alternative component inputs. Configure valid author, mkisofs and asset paths separately in the GUI. See [development/debugging](../../docs/DEVELOPMENT.en.md).

## Rebuild native components when needed

Use Windows x64, Python 3.12+ and MSYS2/MinGW-w64. Media builds also use Make/GPG, a pinned NASM archive and libsoxr/zlib. Image builds use static JPEG/PNG/WebP/zlib development archives. Recipes and manifests record source versions, archive hashes, configuration and dependency hashes.

### GUI media runtime

```bat
python tools\win-build\build-minimal-ffmpeg.py --msys-root "D:\dev\msys64" --work-directory build\ffmpeg-media --profile media
python tools\win-build\build-media-bridge.py --msys-root "D:\dev\msys64"
```

This supplies the required audio decoders, SWR/SOXR, menu-video reading and C interface, without FFmpeg/FFprobe executables. MLP encoding remains a separate MLP core. See [in-process media](../../docs/INPROCESS-MEDIA.en.md).

### Image runtime and native author

```bat
python tools\win-build\build-image-runtime.py --msys-root "D:\dev\msys64"
python tools\win-build\build-image-bridge.py --msys-root "D:\dev\msys64"
python tools\win-build\build-image-author.py --source "D:\dev\winbuild\src" --msys-root "D:\dev\msys64"
```

ImageMagick/FreeType archives are pinned by SHA-256. The recipe verifies upstream files and regenerates project patches. The image runtime retains Q16 HDRI, JPEG/PNG reading/writing, WebP reading, drawing/captions/statistics and required font functionality. External delegates and loadable coders are disabled. One x64 image DLL imports only Windows system libraries.

The author script snapshots the full tree with base project changes, then transforms image calls. Do not pre-apply the same image delta. It does not overwrite the source tree or compiler installation. See [mirror boundaries](../dvda-author-mlp8/README.en.md) and [in-process images](../../docs/INPROCESS-IMAGES.en.md). The old magick-shim remains solely for historical reference tests.

### MLP-only authoring decoder libraries

```bat
python tools\win-build\build-minimal-ffmpeg.py --msys-root "D:\dev\msys64"
```

The default mlp profile serves menu-bin authoring tools. It is separate from the GUI media profile and encoder. Pass `--ffmpeg-libraries build\ffmpeg-minimal\install\bin` when replacements are needed. The packager verifies the known profile before pruning unused dependencies. Compiler/configuration changes require regression testing before fingerprints are updated. See [the minimal-library record](../../docs/MINIMAL-FFMPEG.en.md).

## Validation and cache maintenance

Packaging verifies hashes, x64 architecture and imports, produces MANIFEST.txt, and inserts runtime-specific notices into bilingual README/RUNTIME files. All font glyphs remain. ZIP format and compression settings are not changed as a size-reduction technique.

```bat
dotnet build DVD-Audio-Maker.sln -c Debug -p:SelfContained=false
tests\DvdaMaker.CompatibilityTests\bin\Debug\net10.0-windows\win-x64\DvdaMaker.CompatibilityTests.exe
```

Current recorded checks pass 108/108 compatibility cases and 23/23 image cases. Full GUI process tracing covers regular menus and single/multiple index pages without external ImageMagick, FFmpeg/FFprobe or SurCode. Necessary menu encoders/muxers and ISO tools still run externally.

`test-image-release.py` requires an independent old package, matching source/reference fixtures and a fresh output directory. Old ImageMagick runs only in the test's reference branch. See [the validation record](../../docs/inprocess-images-validation.json).

Local build/README.md describes retained components, references and the current package. These are not supplied by a Git clone. Keep source and personal settings. bin/obj and publishing staging areas can be regenerated; deleting native components under build affects debugging/packaging unless reusable artifacts or a rebuild are available.
