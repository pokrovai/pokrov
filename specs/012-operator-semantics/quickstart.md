# Quickstart: Operator Semantics Freeze

## 1. Confirm feature context

```bash
git branch --show-current
test -f specs/012-operator-semantics/spec.md
test -f specs/012-operator-semantics/plan.md
```

Expected result: active branch is `012-operator-semantics` and feature docs exist.

## 2. Implement transform semantics in existing crates

Focus implementation in:

- `crates/pokrov-core/src/transform/`
- `crates/pokrov-core/src/policy/`
- `crates/pokrov-core/src/traversal/`
- `crates/pokrov-core/src/types/`
- `crates/pokrov-config/src/` (only if profile/operator validation updates are required)

Implementation goals:

- freeze support to `replace|redact|mask|hash|keep`
- enforce fail-closed `block` on unsupported operators
- keep deterministic post-overlap application order
- preserve JSON validity via string-leaf-only mutation
- ensure metadata-only explain/audit outputs and explicit `keep` visibility

## 3. Add or extend verification coverage

Add tests in existing suites:

- unit tests in `pokrov-core` for each operator and deterministic ordering
- contract tests under `tests/contract/` for outcome shape stability
- integration tests under `tests/integration/` for block and non-blocking nested JSON flows
- security tests under `tests/security/` for metadata-only guarantees
- performance tests under `tests/performance/` to confirm latency budget preservation

## 4. Run required validation commands

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features
```

## 5. Capture acceptance evidence

Collect evidence for:

- deterministic replay equality for identical resolved-hit sets
- explicit unsupported-operator `block` with `unsupported_operator` reason
- deterministic one-way `hash` behavior
- explicit `keep` marker in explain/audit summaries
- JSON validity preservation for nested object/array payloads
- metadata-only safety for block and transformed outcomes
