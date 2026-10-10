# Windows x64 menu vendor boundary

The production GUI and Rust author statically link the imported dvdauthor SPU/DVD navigation algorithms. Build from the repository root:

```powershell
python tools/win-build/build-menu-runtime.py --direct-link --msys-root C:/msys64 --output build/menu-direct-vendor-rust-session
```

The output contains `libdvda_menu_spu_vendor.a`, `libdvda_menu_nav_vendor.a`, `menu-build.json` and `NOTICE.txt`. The manifest authenticates vendor inputs, Rust inputs, archives and the generated Rust reset objects. The Rust author builder requires `session.implementation=rust`, `state_reset.implementation=rust`, and current Rust source hashes. See [dvda-menu](../../rust/crates/dvda-menu/README.md) for the direct API.

`vendor/ORIGIN.json` records the imported dvdauthor subset, original hashes and source identity. Modified vendor files retain their copyrights; GPL text is in `vendor/COPYING`. Generated VM parser/lexer sources are included; building does not require Bison/Flex. These third-party algorithms remain C.

Project-owned XML parsing, descriptor dispatch, boolean parsing, private heap allocation, UTF-8 file/directory access, resource acquisition and cleanup, request validation and session lifecycle are implemented in `rust/crates/dvda-menu`. The direct builder generates a `no_std` Rust object to snapshot and restore every writable vendor section. It records the exact section inventory and reset source/object hashes.

`session-rust.c` retains only C `setjmp`/`longjmp` containment, variadic `open` argument decoding and promotion of returned Rust error statuses. Rust enters vendor callbacks through local C islands. A vendor failure returns to Rust after the jump has completed; Rust then closes resources and destroys the private heap. Rust hooks never call `longjmp`, and image callbacks must not unwind or jump across Rust frames.

Direct calls serialize access to vendor state and reject re-entry. A failed call can leave partial output; the Rust author discards its staging transaction. Input strings must be UTF-8. XML supports the documented XML 1.0 UTF-8 subset, and rejects DTDs, processing instructions, duplicate attributes and excessive depth. This parser does not claim exhaustive XML name/namespace grammar coverage.

Without `--direct-link`, the developer builder produces `dvda-menu-spu.dll` and `dvda-menu-nav.dll` for historical ABI comparisons. Each DLL load permits one sequential `dvda_menu_run` call and must then be unloaded. Its manifest records `state_reset.implementation=not-required` with `strategy=fresh-dll-state`. These owned adapter DLLs are excluded from the production package.

`test-menu-rust-reset.py` verifies real writable-section restoration and independent SPU/NAV state. `test-rust-menu-session.py` verifies complete output equality between fresh Rust processes, Unicode paths, repeated failure/retry, re-entry, serialized concurrent calls and heap/handle cleanup, then checks actual Rust author menus, ISO extraction and failure rollback. Developer Python and `pycdlib` are acceptance tools only; the author and ISO writer run in Rust. Current evidence and the remaining C inventory are in the [owned C migration record](../../docs/RUST-MENU-SESSION-MIGRATION.md).

Historical frozen-C parity reports are retained; old C builds and the retired author source tree have been removed. The older differential scripts require an explicitly supplied external comparison runtime. Build-only FFmpeg/ImageMagick ABI probes remain. Production media/image bridges, MLP encoding, complete DVD-Audio authoring, ISO writing and disc verification use Rust project code.
