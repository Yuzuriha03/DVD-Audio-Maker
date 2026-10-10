# dvda-author

Complete Rust DVD-Audio authoring: integer PCM WAV/MLP input, AOB packetization,
gapless/title scheduling, ATSI/SAMG/AMG/ASVS, menus/stills, and ISO9660/UDF 1.02.

This crate uses only the standard library. The application and standalone Rust
author share `command::run`; menu image/video codecs and SPU/navigation are
supplied through `menu::Backend` using existing third-party libraries. There is
no Python library or C author subprocess in the production author.

`atsi::encode_checked` is the production encoder. `atsi::encode` retains C's
format-index quirks solely for frozen differential fixtures. AMG tables expand
without overlap, SAMG uses actual ISO addresses, and failed author/ISO writes
preserve existing output.

```powershell
cargo test --manifest-path rust/Cargo.toml -p dvda-author --offline
cargo clippy --manifest-path rust/Cargo.toml -p dvda-author --all-targets --offline -- -D warnings
```

The production builder is `tools/win-build/build-rust-author.py`.
`build-image-author.py` is a compatibility entry that delegates to it. The retired
C producer and its sources have been removed; frozen test data retains source
hashes and the historical Git revision. Python and pycdlib are development
acceptance tools only.

See [the migration record](../../../docs/RUST-AUTHOR-MIGRATION.md) for interfaces,
build inputs, verification and implementation limits.
