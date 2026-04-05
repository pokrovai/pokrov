# Quickstart: Safe Explainability and Audit

## 1. Confirm feature context

```bash
git branch --show-current
test -f specs/013-safe-explainability-audit/spec.md
test -f specs/013-safe-explainability-audit/plan.md
```

Expected result: active branch is `013-safe-explainability-audit` and feature docs exist.

## 2. Implement safe explain/audit contracts in existing crates

Focus implementation in:

- `crates/pokrov-core/src/analyzer/`
- `crates/pokrov-core/src/explain/`
- `crates/pokrov-core/src/audit/`
- `crates/pokrov-core/src/policy/`
- `crates/pokrov-core/src/types/`
- `crates/pokrov-config/src/` (only if policy/config validation updates are required)

Implementation goals:

- freeze metadata-only explain and audit output schemas
- enforce reason-code catalog usage and confidence bucket policy
- preserve deterministic outputs for identical input+config
- enforce mode-based failure behavior (`fail closed` runtime, `degraded continue` non-enforcing evaluation)
- enforce 30-day retention and least-privilege read access constraints

## 3. Add or extend verification coverage

Add tests in existing suites:

- unit tests for explain and audit builders, reason-code mapping, confidence bucketing
- contract tests under `tests/contract/` for output shape safety and prohibited field regression
- integration tests under `tests/integration/` for allow/transform/block/degraded flows
- security tests under `tests/security/` for no-raw-content leakage and role-based access restrictions
- performance tests under `tests/performance/` to verify explain+audit overhead <=10 ms p95

## 4. Run required validation commands

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features
```

## 5. Capture acceptance evidence

Collect evidence for:

- metadata-only explain output across deterministic recognizer scenarios
- metadata-only audit output across allow/transform/block/degraded outcomes
- reason-code catalog completeness and regression coverage
- mode-based failure behavior correctness
- retention-window and post-window deletion behavior
- least-privilege access controls for explain/audit retrieval
- explain+audit p95 overhead <=10 ms
