# Verification Evidence: 009-architecture-foundation

Date: 2026-04-05

## Scope

This note captures the verification evidence for the Presidio architecture foundation feature.
It records the shared-contract scaffolding, stage-boundary invariants, runtime and evaluation contract reuse proof, metadata-only safety checks, and final validation commands.

## Commands

```bash
cargo test --test contract sanitization_foundation
cargo test --test integration sanitization_foundation
cargo test --test security sanitization_foundation
cargo test --test performance sanitization_foundation
cargo test -p pokrov-core
cargo test --workspace
RUSTC=$(rustup which --toolchain stable rustc) \
  CARGO_TARGET_DIR=target-stable-clippy-rustc \
  rustup run stable cargo clippy --all-targets --all-features
```

## Results

- `cargo test --test contract sanitization_foundation`: PASS (2 passed)
- `cargo test --test integration sanitization_foundation`: PASS (3 passed)
- `cargo test --test security sanitization_foundation`: PASS (3 passed)
- `cargo test --test performance sanitization_foundation`: PASS (1 passed)
- `cargo test -p pokrov-core`: PASS (19 unit tests, 0 doc tests)
- `cargo test --workspace`: PASS
- `rustup`-scoped `cargo clippy --all-targets --all-features`: PASS (exit 0; warnings reported, no lint errors)

## Notes

- Compile-visible foundation contracts now live in `crates/pokrov-core/src/types/foundation.rs` and are re-exported from `crates/pokrov-core/src/types.rs`.
- `SanitizationEngine::trace_foundation_flow` is the executable proof surface for shared runtime and evaluation contract families.
- Metadata-only explain and audit placeholders are verified by `tests/security/sanitization_foundation_metadata_leakage.rs` and `crates/pokrov-core/src/audit/mod.rs` unit tests.
- Repo-safe fixture guidance for evaluation boundaries lives in `tests/fixtures/eval/README.md`.
- Direct `cargo clippy --all-targets --all-features` from the shell resolved `/usr/local/bin/cargo` and `/usr/local/bin/rustc` (`1.91.1`), while the installed `clippy` component belonged to `rustup` stable (`1.94.1`). The recorded lint result uses an isolated target dir plus explicit `rustup` `RUSTC` to avoid mixed-toolchain artifacts.
