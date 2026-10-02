# Native Windows release assembly

[简体中文](README.md) | [English](README.en.md)

This directory assembles a self-contained DVD-Audio Maker release using Windows CMD and C#.

Assembly does not invoke PowerShell, WSL, Bash, MSYS2, Autotools or Make.

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
magick.exe
fonts\NotoSansCJKsc-Regular.otf
fonts\NotoSansCJKjp-Regular.otf
fonts\NotoSansCJKkr-Regular.otf
```

Include the required DLLs, ImageMagick XML files and other runtime files too. The packager copies the directory unchanged; it neither guesses nor downloads dependencies.

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

`build-all.cmd` starts `src/DvdaMaker.Toolchain`, checks inputs, publishes the native x64 C# GUI and CLI, copies tools and assets, writes `dvda.cmd`, generates a SHA-256 manifest and creates the ZIP.

## Output

```text
tools\win-build\release\
├── DVD-Audio-Maker\
│   ├── DVD-Audio-Maker.exe
│   ├── app\
│   ├── menu-bin\
│   ├── data\menu\
│   ├── config.env
│   ├── dvda.cmd
│   ├── MANIFEST.txt
│   ├── README.md
│   ├── THIRD-PARTY.md
│   └── LICENSE
└── DVD-Audio-Maker.zip
```

The target machine needs no installed .NET Runtime. Double-click `DVD-Audio-Maker.exe` or run `dvda.cmd` without arguments for the GUI. Commands with arguments use the CLI. The GUI imports existing `config.env` files and saves independent JSON profiles. MLP uses the embedded native x64 DLL, without a standalone encoder EXE.

FFmpeg, FFprobe and Metaflac must be in `menu-bin`, on `PATH`, or configured with full paths in the GUI / `config.env`. eac3to is no longer required.

## Use the release

```bat
cd tools\win-build\release\DVD-Audio-Maker
dvda.cmd config --check
dvda.cmd prepare
dvda.cmd build --dry-run
dvda.cmd build
dvda.cmd verify all
```

## Diagnosing failures

- A missing prebuilt directory produces an explicit failure, with no Bash or WSL fallback.
- Incomplete directories report each missing EXE or font.
- DLLs are not checked against a fixed list; the provider of the prebuilt directory must ensure it is complete.
- If assets are missing, use `--source` to point to a complete tree containing `menu`.

## Related documentation

- [`../../README.md`](../../README.en.md): project overview
- [`../../docs/DVDA-AUTHOR-CHANGES.md`](../../docs/DVDA-AUTHOR-CHANGES.en.md): third-party source changes
- [`../../docs/LICENSING.md`](../../docs/LICENSING.en.md): third-party licenses
