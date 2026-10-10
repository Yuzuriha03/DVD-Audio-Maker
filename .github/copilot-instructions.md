# MLP encoder migration requirements

- Use the original frozen C `mlp_encoder` as the sole behavioral reference for the Rust migration. Its SHA256 is `ECE6D0A8033A26E2528042A7B74C66C249EA3C8D7378C06809FB94C8F6BD79B8`; do not replace or modify this oracle.
- Translate the C implementation function by function into Rust. Preserve corresponding function boundaries, state structures, parameter semantics, branch and loop ordering, arithmetic and rounding, candidate ordering, tie-breaking, and serialization behavior. Adapt syntax and ownership for Rust without substituting a different algorithm.
- Preserve the call relationships between corresponding functions. Do not merge functions, reorganize the call graph, or introduce an independently designed encoding pipeline in place of the C implementation.
- At final validation, directly compare the C and Rust source implementations function by function and check their call relationships, in addition to the applicable tests and integration checks.
- Acceptance requires identical PCM, encoding parameters, and metadata to produce original MLP files that are byte-for-byte identical and have identical SHA256 hashes. Compare before any diagnostic normalization; decoded PCM equality alone is insufficient.
- Do not patch encoded output, normalize files, or call the C encoder in the Rust production implementation to manufacture parity. C reference adapters are permitted only for testing.
- Report remaining differences honestly. Do not declare migration complete or release the Rust encoder until the strict acceptance above passes. Do not commit, push, replace tags, or publish without renewed user authorization.
