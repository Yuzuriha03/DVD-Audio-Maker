# In-process image processing

The project-built x64 `dvda-image.dll` handles covers, backgrounds, text,
button states, album indexes, stills, font detection and visual verification.
The Rust GUI and author share this DLL. They do not launch `magick`, `convert`,
`mogrify` or `identify` command-line programs.

## Retained capabilities

- JPEG/PNG read and write, WebP cover input, alpha, resize, crop, composite,
  gradients, drawing, captions and statistics;
- complete SC/JP/KR font faces in one TTC, including Unicode and paths with
  spaces;
- the required ImageMagick, FreeType, JPEG, PNG, WebP and zlib functionality,
  with external delegates, dynamic coders, video and document conversion
  disabled;
- cancellation, timeout, malformed-input and concurrent-request handling, with
  staged GUI writes published only after success.

This is an application-specific runtime. TIFF/PDF/SVG conversion and WebP
output are intentionally outside the product boundary.

## Build and validation

Native sources, pinned hashes and build entry points are documented in the
[Windows build instructions](../tools/win-build/README.en.md). The menu fixture
covers regular menus, index pages, stills, fonts and final ISO verification.
Source provenance and licenses for the image DLL are shipped through the
release NOTICE files.

```powershell
cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- --include-ignored
```

The release runtime is a native Rust GUI. Profiles are
JSON only, and release folders and ZIPs stay under the ignored `build` tree.
