# Author golden fixtures

The `.ifo` and `.csv` files are fixed regression inputs for the Rust
`atsi_parity.rs` and `manager_parity.rs` tests. Their bytes are retained from the
completed C-to-Rust migration. Tests compare Rust output directly with these
files and do not compile or run a C author.

The `atsi/provenance.json` and `manager/provenance.json` records retain the
original fixture hashes, source hashes, compiler settings and generation notes.
Those settings describe historical generation, rather than a current build
entry point. The fixture metadata is independently specified in the Rust tests.

The old author mirror and its runnable C oracle harnesses were retired during
cleanup. [legacy-source-provenance.json](legacy-source-provenance.json) records
the removed mirror's SHA256 values, Git blob IDs and copyright/license notices.
Its historical source is available at commit
`6c5086127590001c544373783653fe991f0ebaeb`:

```powershell
git show 6c5086127590001c544373783653fe991f0ebaeb:tools/dvda-author-mlp8/src/audio.c
git archive -o legacy-author-source.tar 6c5086127590001c544373783653fe991f0ebaeb tools/dvda-author-mlp8
```

The two fixture-specific C harnesses were untracked migration aids; their hashes
are preserved, while their cases remain implemented in the Rust tests. They are
not represented as recoverable files in that historical commit. Upstream
copyright and GPL obligations continue to apply to translated code; see the
repository's license and third-party attribution records.

Run the retained regressions from the repository root:

```powershell
cargo test --manifest-path rust/Cargo.toml -p dvda-author --test atsi_parity --test manager_parity
```
