# Windows x64 build and package

The supported product is a native Rust GUI for Windows x64. It does not require a .NET runtime at run time. Native rebuilds require Rust, the existing MSYS2/MinGW-w64 GCC toolchain and Python.

## Rust build

```bat
cargo build --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline
cargo fmt --manifest-path rust\Cargo.toml --all -- --check
cargo clippy --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- --include-ignored
```

`gui.cmd`, `gui-debug.cmd`, `cli.cmd`, `build.cmd` and `verify.cmd` invoke the Rust workspace. Profiles are JSON files selected with `--profile`; `config.env`, `DVDA_CONFIG` and `--config` are unsupported.

## Native inputs

The package assembler expects validated x64 directories:

| Input | Default | Required content |
| --- | --- | --- |
| Media | `build/media-native-shared` | `dvda-media.dll` and its FFmpeg-derived DLLs |
| Image | `build/image-native` | `dvda-image.dll`, XML policy/type/color files and notice |
| Author | `build/rust-author-current` | `dvda-author-dev.exe` and menu DLLs |
| Fonts/assets | `build/release-menu-final` or `--prebuilt` | menu assets and CJK fonts |
| MLP | `native/mlp-encoder/win-x64` | pinned `mlp_encoder.dll` |

FFmpeg and ImageMagick command-line programs are not runtime inputs. Their required libraries are built from source and loaded through the project C ABI. The MLP encoder source and build recipe are under `native/mlp-encoder`.

Format parsing, MLP CRC/parity, PCM comparison and read-only AOB disc verification are implemented in Rust. No format or verifier DLL is required; packaging no longer accepts `--formats-runtime`.

## Release ZIP

After native inputs are available:

```bat
tools\win-build\build-all.cmd ^
  --media-runtime build\media-native-shared ^
  --image-runtime build\image-native ^
  --image-author build\rust-author-current ^
  --prebuilt build\release-menu-final ^
  --version v1.0
```

This calls `dvda-toolchain package` and writes `DVD-Audio-Maker-v1.0-win-x64.zip` under the ignored release directory. The staging root contains one GUI EXE with embedded runtime components and companion user documentation. It excludes .NET files, the developer CLI, PDBs, build JSON and local profiles.

The packager normalizes Windows extended paths before invoking `Compress-Archive`, checks the archive can be extracted, and audits the final file list. Place the three user READMEs, licenses and both NOTICE files beside the EXE in the ZIP; runtime files are embedded.
