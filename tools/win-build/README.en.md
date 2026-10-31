# Windows x64 build and package

The supported product is a native Rust GUI for Windows x64. It does not require a .NET runtime at run time. Native rebuilds require Rust, Visual Studio C++ x64 tools, the Windows SDK, Python and native Windows CMake. FFmpeg configure/make uses an MSYS2 shell, while every C/C++ compile and link uses MSVC.

## Rust build

```bat
cargo build --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --workspace --offline
cargo fmt --manifest-path rust\Cargo.toml --all -- --check
cargo clippy --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --workspace --offline -- -D warnings
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --workspace --offline -- --include-ignored
```

`gui.cmd`, `gui-debug.cmd`, `cli.cmd`, `build.cmd` and `verify.cmd` invoke the Rust workspace. Profiles are JSON files selected with `--profile`; `config.env`, `DVDA_CONFIG` and `--config` are unsupported.

The complete author runs in Rust through a shared application/CLI entry. `build-image-author.py` delegates to `build-rust-author.py`; the retired C producer, sources and frozen binaries have been removed. Historical golden-fixture source hashes are retained. AOB, table, menu/still and ISO acceptance records are kept in the local source working copy.

## Native inputs

The package assembler expects validated x64 directories:

| Input | Default | Required content |
| --- | --- | --- |
| Media | `build/msvc-native/ffmpeg-static/install` | authenticated static FFmpeg, soxr and zlib archives |
| Image | `build/image-native` | `dvda-image.dll`, XML policy/type/color files and notice |
| Author | explicit `--image-author` | complete Rust author built with the same static FFmpeg manifest |
| Fonts/assets | `tools/win-build/prebuilt` or explicit `--prebuilt` | prepared menu assets and CJK fonts; missing inputs fail explicitly |
| MLP oracle (optional) | `--encoder-runtime` | authenticated encoder oracle; never shipped |

The packager verifies hashes, sizes, x64 architecture, the full import closure and assets. The MLP encoder is compiled directly into Rust. Image provenance must identify the Rust implementation; the local acceptance uses `build/rust-image-runtime`. Media/image adapter DLLs are development provenance inputs and are excluded from linked releases.

FFmpeg and ImageMagick command-line programs are not runtime inputs. Rust bridges call their retained source-built libraries.

Build static FFmpeg in a separate directory to preserve shared comparison builds:

```bat
python tools\win-build\build-msvc-ffmpeg.py --profile static --work-directory build\msvc-native\ffmpeg-static --msys-root D:\dev\msys64 --cmake PATH\TO\cmake.exe --source-cache build\msvc-native\ffmpeg\downloads
```

The launch scripts default to this static prefix. Override `DVDA_FFMPEG_PREFIX`
explicitly for shared comparison builds. Static releases extract fonts and menu
assets only; all five FFmpeg libraries and their static CRT/dependencies are linked
into the EXE. Static linkage alone is not a measured encoding speed improvement.

Format parsing, MLP CRC/parity, PCM comparison and read-only AOB disc verification are implemented in Rust. No format or verifier DLL is required; packaging no longer accepts `--formats-runtime`.

## Release ZIP

After native inputs are available:

```bat
tools\win-build\build-all.cmd ^
  --media-runtime build\msvc-native\ffmpeg-static\install ^
  --image-runtime build\msvc-native\image-runtime ^
  --image-author build\rust-author-production ^
  --prebuilt build\msvc-native\package-assets ^
  --version v1.0
```

This calls `dvda-toolchain package` and writes `DVD-Audio-Maker-v1.0-win-x64.zip` under the ignored release directory. The staging root contains one GUI EXE with embedded runtime components and companion user documentation. It excludes .NET files, the developer CLI, PDBs, build JSON and local profiles.

The packager normalizes Windows extended paths before invoking `Compress-Archive`, checks the archive can be extracted, and audits the final file list. Place the three user READMEs, licenses and both NOTICE files beside the EXE in the ZIP; runtime files are embedded.

### Lossless menu font optimization

For the static Regular TTC containing all four SC/JP/KR/TC faces, use the build-only Python dependencies `fonttools==4.68.0` and `cffsubr==0.4.0`:

```powershell
.venv\Scripts\python.exe tools\win-build\optimize-menu-fonts.py build\font-sources\DvdaNotoCJK-Regular.ttc build\font-sources\optimized\DvdaNotoCJK-Regular.ttc
```

The destination must not exist. The script compresses the shared CFF2 outlines using subroutines, shares identical collection tables, and writes only after checking all 65,535 glyph outlines, font/private parameters, glyph order, and every non-CFF2 table (excluding the dependent `head.checksumAdjustment`). Unsupported hinted or variable programs are rejected. No characters or regional faces are removed; mappings, layout, metrics and metadata remain unchanged. After native rendering acceptance and `dvda-toolchain font verify-collection`, copy the result into `build\msvc-native\package-assets\fonts\DvdaNotoCJK-Regular.ttc` before packaging. Keep the source unchanged for comparison. Raw font savings are not the same as compressed release ZIP savings.
