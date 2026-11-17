# Rust developer CLI

The standard release ZIP contains the GUI only. The source checkout also provides `cli.cmd`, a Rust diagnostic entry point for repeatable development runs.

## JSON profiles

The CLI accepts JSON profiles with `--profile`. Without an explicit profile, `cli.cmd` uses the ignored `settings.local.json` when it exists; the application default is `settings.json (EXE directory)`.

```bat
cli.cmd abi.version
cli.cmd build --dry-run --profile "C:\work\settings.json"
```

`config.env` and `DVDA_CONFIG` are not supported. `--config` is a developer CLI alias for a JSON `--profile`, never for an env file. The CLI does not read or execute tool paths from old profiles. Media, image, format and encoder DLLs must be the validated project-built x64 components selected by the profile or package layout.

## Developer operations

The CLI provides `config --check/--shell/--shell-all`, `plan`, and `build --dry-run/--no-resume`. Configuration exports contain evaluated paths and values; removed executable override keys stay excluded.

`dvda-cli convert PATH... [--in-place] [--dry-run] [--level 0..8]` converts ALAC to FLAC; worker count is automatic: one worker per detected logical processor, capped only by the number of files. It preserves tags and cover bytes and compares full-precision PCM before publishing. Explicit `--in-place` deletes sources only after every conversion succeeds. `alac check INPUT` and `alac repair INPUT [OUTPUT]` inspect or repair copies.

`iso list ISO [INNER]`, `iso extract ISO INNER OUTPUT`, `aob-pts FILE...`, and `mlp --check FILE...` provide format diagnostics. `mlp --align` remains an explicitly invoked developer repair operation; normal MLP encoding never patches its output through this command.

Font operations are available as `dvda-toolchain fonts extract/verify/pack/verify-collection`, for example `fonts extract INPUT.ttc OUTPUT_DIR` and `fonts verify FONT.otf "Noto Sans CJK SC"`. Developer tools are not included in the user ZIP.

## Build and test

```bat
cargo build --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --workspace --offline
cargo fmt --manifest-path rust\Cargo.toml --all -- --check
cargo clippy --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --workspace --offline -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File tools\win-build\test-rust-workflow.ps1 -OtherVolume D:\
```

`build.cmd`, `gui.cmd` and `gui-debug.cmd` are thin Rust wrappers. `build --dry-run` is a developer diagnostic mode; it can prepare and encode caches but does not create an ISO.
