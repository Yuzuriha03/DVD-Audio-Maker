# Development and contribution

## Workspace layout

- Keep user entry points, READMEs, licenses, contributor guides, and example configuration at the root. `build.cmd`, `cli.cmd`, `gui*.cmd`, and `verify.cmd` are public launchers and must not be moved for cosmetic reasons.
- `rust/` is the Cargo workspace and `rust/crates/` is organized by responsibility. Move modules only for call-graph or API reasons, never for formatting or lint counts.
- `tools/` contains build, release, and validation scripts; `assets/` contains source resources.
- `docs/` retains the local-development convention: only licensing and necessary guides explicitly allowlisted in `.gitignore` are version controlled. See the [development guide](docs/DEVELOPMENT.en.md) for full build and validation instructions; other specialized diagnostics remain local.
- `build/`, `target/`, `rust/target/`, and `.venv/` contain generated artifacts or local environments. Do not manually reorganize compiler output or submit it as source releases.
- `build/clippy-audit/` contains only reproducible evidence: `baseline/` for baselines, `strict/` for strict checks, `tests/` for tests and workflows, and `expanded/` for per-feature expanded diagnostics. Root `current-force-warn.jsonl` is the current audit and `implementation-summary.json` is the summary. Remove rebuildable isolated targets, temporary fixtures, and debugging instrumentation output instead of accumulating them.

## Clippy pedantic maintenance

The workspace keeps `clippy::pedantic = "warn"` in `rust/Cargo.toml`; strict acceptance uses `-D warnings`. The goal is to keep diagnostics effective, not to drive the number of exemptions to zero.

### Exemption rules

- Do not add group exemptions such as `clippy::pedantic`, `clippy::all`, or `warnings`, and do not hide problems by raising lint thresholds.
- Do not place broad exemptions on a crate, file, or unrelated module. Keep necessary exceptions on the exact function, struct, or expression and provide a specific `reason`.
- `expect` is also an exemption; `allow -> expect` is not a fix. Every retained `expect` must be fulfilled by the item actually compiled, and `unfulfilled_lint_expectations` must be checked.
- Reasons must state the value range, low-bit/two's-complement rule, ABI/alignment contract, public ownership contract, or resource lifetime. “C compatibility” and “preserving behavior” alone are insufficient.
- Do not add meaningless `clone`, `drop`, `Copy`, or wrappers merely to silence a lint.

### Review only the module being changed

When changing a module, review only the affected functions and nearby exemptions rather than starting another repository-wide cleanup:

1. Run strict Clippy for the affected crate.
2. Prefer real equivalent fixes for private candidates without ABI/protocol constraints and with test coverage: borrow private helpers, move local declarations, and use checked conversions with proven bounds.
3. Preserve public Rust APIs, C/Win32 ABI, JSON and operational protocols, valid input/output behavior, cancellation and error priority, rollback, and commit order.
4. Change protocol low-bit extraction, wrapping, unaligned reads, public owned arguments, or numerical algorithms only when safety is proven. Otherwise keep a minimal exception.
5. Review malformed-input behavior, protocol scope, and resource-policy changes as separate behavioral fixes, never as lint cleanup.
6. Run the relevant behavioral tests and review the expanded diagnostics. The number of local attributes may increase as scopes shrink; a smaller count is not the sole objective.

### Reproducible commands

From the repository root, after initializing the existing MSVC environment:

```powershell
cargo +1.99.0 fmt --manifest-path rust/Cargo.toml --all -- --check
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --workspace --all-targets --all-features --locked --offline -- -D warnings"
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --workspace --all-targets --locked --offline -- -D warnings"
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --workspace --all-targets --no-default-features --locked --offline -- -D warnings"
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc -p dvda-core --all-targets --no-default-features --locked --offline -- -D warnings"
```

To audit suppressed diagnostics, collect results separately for each feature configuration:

```powershell
cmd /d /c "call tools\win-build\rust-dev-env.cmd && cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --workspace --all-targets --all-features --locked --offline --message-format=json -- --force-warn clippy::pedantic"
```

A successful expanded-audit exit does not mean zero warnings. Deduplicate by rule, file, and line of the first primary span using the same toolchain and features, and check `unfulfilled_lint_expectations`. Workspace `no-default-features` does not cover Core defaults re-enabled by dependents, so retain the separate Core check above.

Write build reports to the ignored `build/clippy-audit/` directory; they are not release artifacts. Tests lacking an external oracle, FFmpeg, media fixtures, another volume, or ARM64 native libraries must be marked NOT RUN rather than passed.
