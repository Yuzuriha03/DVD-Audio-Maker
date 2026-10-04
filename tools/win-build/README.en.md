# Windows x64 builds and packaging

[简体中文](README.md) | [English](README.en.md)

The default release is **one Windows x64 GUI EXE without .NET**. Users install .NET 10 Desktop Runtime x64; builds require the .NET 10 SDK. Defaults are `--onefile --framework-dependent`. Use `--directory` for a diagnostic folder or explicitly `--directory --self-contained` to include .NET.

Routine C# builds and packaging reuse prepared Windows artifacts. Python, MSYS2/MinGW-w64, Make and development libraries are needed only when maintaining native components, not when running the product.

## Packaging inputs

| Input | Default location or option | Contents |
|---|---|---|
| GUI media runtime | `build/media-native-shared` or `--media-runtime` | Shared-profile DLLs and media-build.json |
| C17 format runtime | `build/formats-native` or `--formats-runtime` | `dvda-formats.dll` and formats-build.json |
| Image runtime | `build/image-native` | dvda-image.dll, image-build.json, XML and NOTICE.txt |
| Author with in-process images | `build/image-author-shared` or `--image-author` | dvda-author-dev.exe and author-build.json |
| Tools and fonts | `--prebuilt` or DVDA_PREBUILT_DIR | Complete menu-bin runtime dependencies |
| Authoring assets | `--source` or DVDA_SRC_TREE | menu/silence.wav and menu/activeheader |

Git does not contain the full third-party tool bundle. tools/dvda-author-mlp8 is a partial review mirror, not an independently buildable tree or complete asset source.

The prebuilt directory needs these programs and their DLL dependencies:

```text
dvda-author-dev.exe
# The final author and required DLLs come from --image-author.
```

Provide NotoSansCJKsc/jp/kr-Regular.otf, or the verified `fonts/DvdaNotoCJK-Regular.ttc`. Packaging shares identical font tables while retaining all glyphs and regional faces. magick.exe, convert.exe, mogrify.exe and identify.exe are no longer required inputs.

The base author must include the project's MLP, timeline, UTF-8 and menu fixes. Packaging replaces its staging copy with the verified `--image-author` build. Legacy menu executables are no longer required and are removed from staging.

## Build the compact single EXE

Build shared FFmpeg, the media bridge and author as described below, then run:

```bat
tools\win-build\build-all.cmd ^
  --source "D:\dev\winbuild\src" ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --media-runtime "build\media-native-shared" ^
  --formats-runtime "build\formats-native" ^
  --image-author "build\image-author-shared"
```

Default output directory: `tools/win-build/release-onefile`. `DVD-Audio-Maker.exe` embeds only required runtime components. README/runtime instructions, licenses, `config.env.example` and `NOTICE-Image.txt` and `NOTICE-Menu.txt` at the ZIP root accompany it in `DVD-Audio-Maker-v1.0-win-x64.zip`. No .NET runtime is included, and the example configuration is not loaded automatically. Runtime assets are extracted to the user cache. Debug builds, the source CLI and F5 remain available.

| Option | Purpose |
|---|---|
| `--version` | Archive version, default v1.0; for example v1.1.0 |
| `--onefile` | Default: GUI single EXE; requires shared native builds |
| `--directory` | Folder plus ZIP for diagnostics |
| `--framework-dependent` | Default: exclude .NET |
| `--self-contained` | Directory mode only: include .NET |
| `--include-cli` | Directory mode only: add developer CLI |
| `--media-runtime` / `--image-author` | Verified media and author builds |
| `--formats-runtime` | Verified C17 MLP/PCM/PTS runtime (defaults to `build/formats-native`) |
| `--source` / `--prebuilt` | Menu assets and fonts |
| `--output` / `--repo` | Output directory and repository root |

Directory mode retains build/media-native and build/image-author defaults. Passing the shared builds explicitly produces the same single DLL set as onefile. Artifacts stay ignored by Git and are distributed through GitHub Releases. See [onefile design](../../docs/ONEFILE-PUBLISH.md).

## Reuse native components for C# debugging

Point runtime loading at a complete release that matches the current source, retaining its font/configuration layout:

```bat
set "DVDA_MEDIA_NATIVE_DIR=D:\DVD-Audio-Maker\menu-bin"
set "DVDA_IMAGE_NATIVE_DIR=D:\DVD-Audio-Maker\image-native"
gui-debug.cmd
```

These variables select runtime locations; packaging still needs the build inputs above. When copying components, retain media provenance and image configuration/notices. Release type.xml resolves fonts relative to image-native: copying only the DLL, or using that configuration without the sibling menu-bin/fonts directory, is insufficient.

Native output defaults to build. MSBuild properties NativeMediaDirectory and NativeImageDirectory select alternative component inputs. Configure the author and asset paths separately in the GUI; final ISO writing is built into the author and no `mkisofs.exe` path is needed. See [development/debugging](../../docs/DEVELOPMENT.en.md).

## Rebuild native components when needed

Use Windows x64, Python 3.12+ and MSYS2/MinGW-w64. Media builds also use Make/GPG, a pinned NASM archive and libsoxr/zlib. Image builds use static JPEG/PNG/WebP/zlib development archives. Recipes and manifests record source versions, archive hashes, configuration and dependency hashes.

### Shared FFmpeg source build (required for onefile)

```bat
python tools\win-build\build-minimal-ffmpeg.py --profile shared --work-directory build\ffmpeg-shared --msys-root "D:\dev\msys64"
python tools\win-build\build-media-bridge.py --prefix build\ffmpeg-shared\install --output build\media-native-shared --msys-root "D:\dev\msys64"
python tools\win-build\build-image-author.py --source "D:\dev\winbuild\src" --ffmpeg-runtime build\ffmpeg-shared\install --work-directory build\image-author-shared --msys-root "D:\dev\msys64"
```

The shared profile combines the media and menu codecs, parsers, demuxers, muxers, swscale, swresample, SOXR and zlib in one pinned source build. Both C consumers link that prefix. Packaging verifies both manifests and all colliding DLL hashes, then keeps one copy of every DLL in menu-bin for both consumers. The MSYS2 location is an example and must provide matching soxr/zlib headers and import libraries; the build does not change compilers.

Separate profiles below remain available for historical comparisons; default onefile publishing rejects mixed builds.

### GUI media runtime

```bat
python tools\win-build\build-minimal-ffmpeg.py --msys-root "D:\dev\msys64" --work-directory build\ffmpeg-media --profile media
python tools\win-build\build-media-bridge.py --msys-root "D:\dev\msys64"
```

This supplies the required audio decoders, SWR/SOXR, menu-video reading and C interface, without FFmpeg/FFprobe executables. MLP encoding remains a separate MLP encoder. See [in-process media](../../docs/INPROCESS-MEDIA.en.md).

### C17 format runtime

The format helper DLL is built from the repository's C17 source and has no bundled third-party library. It contains streaming MLP inspection/alignment, PCM byte comparison and PTS/MLP format parsing. Build and validate it before packaging:

```bat
python tools\win-build\build-formats-runtime.py --msys-root "C:\msys64" --output build\formats-native
```

The GUI uses the DLL when it is present and falls back to the managed implementation in development builds. The compatibility runner can compare both implementations against a local SurCode sample corpus:

```bat
set "DVDA_FORMATS_NATIVE_DIR=%CD%\build\formats-native"
tests\DvdaMaker.CompatibilityTests\bin\Debug\net10.0-windows\win-x64\DvdaMaker.CompatibilityTests.exe --native-format-samples "D:\samples\mlp"
```

### Image runtime and native author

```bat
python tools\win-build\build-image-runtime.py --msys-root "D:\dev\msys64"
python tools\win-build\build-image-bridge.py --msys-root "D:\dev\msys64"
python tools\win-build\build-minimal-ffmpeg.py --profile menu --work-directory build\ffmpeg-menu --msys-root "C:\msys64"
python tools\win-build\build-menu-runtime.py --msys-root "C:\msys64"
python tools\win-build\build-image-author.py --source "D:\dev\winbuild\src" --msys-root "D:\dev\msys64" --ffmpeg-runtime "build\ffmpeg-menu\install"
```

ImageMagick/FreeType archives are pinned by SHA-256. The recipe verifies upstream files and regenerates project patches. The image runtime retains Q16 HDRI, JPEG/PNG reading/writing, WebP reading, drawing/captions/statistics and required font functionality. External delegates and loadable coders are disabled. One x64 image DLL imports only Windows system libraries.

The author script snapshots a configured full project tree, copies the current mirror and C interfaces, and links the FFmpeg menu profile plus menu modules. Defaults: build/ffmpeg-menu/install and build/menu-native; overrides: --ffmpeg-runtime and --menu-runtime. It leaves source/compiler installations unchanged and never builds or starts ffmpeg.exe.
The generated `dvda-author-dev.exe` must be used with the `runtime_files` listed in `author-build.json`; the script collects that PE import closure, so copying only the executable is incomplete. Release packaging copies those DLLs into `menu-bin` as well.

Release packaging retains only the DLLs in the validated author runtime manifest in menu-bin, removing leftovers from the old tool bundle. This cleanup applies only to the new release staging directory; it does not modify --prebuilt inputs, source trees or compiler installations.

### Historical MLP-only profile

```bat
python tools\win-build\build-minimal-ffmpeg.py --msys-root "D:\dev\msys64"
```

The old mlp profile remains for reference and independent maintenance. Standard single-file releases require --profile shared for both the author and GUI media bridge. Separate menu/media profiles remain only for directory builds and historical comparisons; MLP-only libraries cannot substitute for them.

## Validation and cache maintenance

Packaging verifies hashes, x64 architecture and imports, produces MANIFEST.txt, and inserts runtime-specific notices into Chinese, English and Japanese README/RUNTIME files. All font glyphs remain. ZIP format and compression settings are not changed as a size-reduction technique.

```bat
dotnet build DVD-Audio-Maker.sln -c Debug -p:SelfContained=false
tests\DvdaMaker.CompatibilityTests\bin\Debug\net10.0-windows\win-x64\DvdaMaker.CompatibilityTests.exe
```

Migration passes compatibility, menu media, native comparisons and full-stream corruption checks. GUI tracing covers regular/index menus, menu-off, multiple discs/groups and converted PCM without external image/media/menu tools. See [completion records](../../docs/menu-migration-validation.json).

`test-image-release.py` requires an independent old package, matching source/reference fixtures and a fresh output directory. Old ImageMagick runs only in the test's reference branch. See [the validation record](../../docs/inprocess-images-validation.json).

Local build/README.md describes retained components, references and the current package. These are not supplied by a Git clone. Keep source and personal settings. bin/obj and publishing staging areas can be regenerated; deleting native components under build affects debugging/packaging unless reusable artifacts or a rebuild are available.

## User interface and documentation

The GUI offers Check sources, Build discs and Verify output. Preview is available only through the developer CLI: cli.cmd build --dry-run. It writes audio caches and a separate index without creating ISOs. The old SurCode import choice is removed; legacy configuration values map to generic external MLP import.

The release README templates are docs/README.md, docs/README.en.md and docs/README.ja.md in this directory. They contain installation and usage instructions, not compiler commands or validation reports. The root repository README links to developer documentation. Update all three languages before packaging; --version must match the release tag and the documented asset name.

MLP and LPCM are separate GUI encoding options. LPCM uses normalized integer WAVE and native DVD-Audio packing, with no MLP encoder call. Validate native changes with test-lpcm-native.py; use small synthetic fixtures for new format paths.
