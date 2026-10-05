# In-process media processing

The Rust GUI uses the project-built x64 `dvda-media.dll` for source probing,
decoding, PCM/FLAC output, ALAC packet checks, artwork handling and output
verification. It does not launch `ffmpeg.exe` or `ffprobe.exe`, and users do not
need a separate FFmpeg installation.

The media DLL is built from the required FFmpeg library sources and is shipped
in the release package beside the GUI. It is loaded from the package layout and
validated by the toolchain; a missing DLL is an error. The application does not
search PATH or silently fall back to external programs. The same rule applies to
ImageMagick, Metaflac, eac3to and original SurCode.

## Profiles and runtime

Application settings are versioned JSON profiles. The default is
`%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`; `--profile` selects another JSON
file. `config.env` and old executable-path settings are not read. Developers
may select validated native component directories through explicit environment
overrides while testing, but release packages use their adjacent DLLs.

The media runtime contains only the FFmpeg libraries needed by this workflow:
`avcodec`, `avformat`, `avutil`, `swresample`, `swscale`, `libsoxr`, zlib and
the required x64 runtime libraries. The MLP encoder and DVD menu authoring DLLs
are separate components. Cache identities include the media library identity,
so replacing a native build invalidates old preparation entries.

ALAC-to-FLAC metadata normalization is performed by the in-process metadata
editor. Temporary outputs are written beside the destination and atomically
published only after validation; failed or cancelled operations remove the
temporary file. Encoded MLP output is never patched after serialization.

## Build and validation

See [Windows build instructions](../tools/win-build/README.en.md) for the
native build inputs. Run the Rust tests with the validated native directories:

```powershell
$env:DVDA_MEDIA_NATIVE_DIR = (Resolve-Path build/media-native).Path
cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- --include-ignored
```

The integration corpus covers Unicode paths and tags, ALAC, FLAC artwork
preservation, PCM hashes, invalid input, cancellation, concurrent decoders and
menu-frame extraction. Reference FFmpeg tools may be used to create fixtures in
development; they are not release runtime dependencies.
