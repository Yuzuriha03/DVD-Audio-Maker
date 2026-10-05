# Development and debugging

The application is a Windows x64 Rust workspace. User packages contain only
the GUI; the developer CLI remains in source. Media, image, format, verifier
and MLP components are called through project-built native DLLs.

## Source entry points

From the repository root:

```bat
gui-debug.cmd
cli.cmd abi.version
cli.cmd prepare --profile "C:\work\settings.json"
cli.cmd build --dry-run --profile "C:\work\settings.json"
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline
```

`settings.json` is the only file-based configuration format. The Git-ignored
`settings.local.json` is suitable for local paths, and an isolated profile can
be selected with `--profile`. `config.env`, `DVDA_CONFIG` and `--config` are not
supported.

The GUI offers source checking, disc building and output verification. dry-run
is a developer CLI diagnostic and is not exposed in the GUI.

## Native components

Build native components using the [Windows build instructions](../tools/win-build/README.en.md). Source launchers use rust-dev-env.cmd to select the build directories; explicit environment overrides take precedence. The user EXE embeds its own runtime.
Development tests may explicitly select validated components with
`DVDA_MEDIA_NATIVE_DIR`, `DVDA_IMAGE_NATIVE_DIR`, `DVDA_FORMATS_NATIVE_DIR` and
`DVDA_ENCODER_LIBRARY`; missing components fail explicitly.

The MLP C17 core is under `native/mlp-encoder`, and the format C17 DLL sources
are under `tools/formats-native`. Use matching MinGW symbols and a native
debugger; do not change the production encoder's floating-point options for
debugging.

## VS Code

Use Rust Analyzer and CodeLLDB (or WinDbg) for `dvda-desktop` and `dvda-cli`.
`.vscode/tasks.json` contains workspace build, test, release and packaging
tasks; `.vscode/launch.json` contains x64 GUI and CLI debug entries.

## Release

```powershell
cargo build --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --release --workspace --offline
dvda-toolchain.exe package --repo . --output build/release --media-runtime build/media-native-shared --image-runtime build/image-native --image-author build/rust-author-current --prebuilt build/release-menu-final --version v1.0
```

`DVD-Audio-Maker-v1.0-win-x64.zip` excludes the .NET runtime, developer CLI,
PDBs, build-provenance JSON, `config.env` and personal profiles. Build outputs
remain under the Git-ignored `build` directory.

## Acceptance

```powershell
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File tools/win-build/test-rust-workflow.ps1
```

The menu fixture generates menus, index pages, stills, AUDIO_TS and an ISO with
the project-built author. MLP byte comparisons, PCM comparisons and C17 ABI
checks are recorded in the component documentation.
