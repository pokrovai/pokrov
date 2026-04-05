# Quickstart: Deterministic Recognizers

## 1. Confirm the planning context

```bash
git branch --show-current
test -f specs/011-deterministic-recognizers/spec.md
test -f specs/011-deterministic-recognizers/plan.md
```

Expected result: branch `011-deterministic-recognizers` is active and the spec/plan artifacts exist.

## 2. Implement the core contract changes

Focus implementation in:

- `crates/pokrov-core/src/detection/`
- `crates/pokrov-core/src/policy/`
- `crates/pokrov-core/src/types.rs`
- `crates/pokrov-core/src/types/foundation/`
- `crates/pokrov-config/src/model.rs`
- `crates/pokrov-config/src/validate.rs`

Implementation goal: add deterministic recognizer family configuration, compiled startup state, shared candidate normalization, deterministic precedence, and metadata-only explain/audit alignment without expanding crate boundaries.

## 3. Add verification coverage

Add or extend tests in:

- `crates/pokrov-core` unit tests for pattern, validator, context, allowlist, denylist, and overlap behavior
- `tests/contract/` for analyzer/foundation contract stability
- `tests/integration/` for structured payload and runtime flow parity
- `tests/security/` for metadata-only safety and tenant/profile scoping
- `tests/performance/` for overhead budget confirmation

## 4. Run the required validation commands

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features
```

If implementation-specific suites are added, run them directly during development before the full workspace pass.

## 5. Capture acceptance evidence

Collect evidence for:

- deterministic replay on repeated identical inputs
- validation pass and validation reject behavior
- exact-match allowlist suppression and denylist positive handling
- same-span precedence ordering
- plain-text and structured-field parity
- metadata-only explain and audit outputs
- performance budget preservation
