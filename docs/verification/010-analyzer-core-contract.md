# Verification: 010 Analyzer Core Contract

Date: 2026-04-05
Feature: `specs/010-analyzer-core-contract`

## Foundational checkpoints

- Canonical analyzer request contract now includes `effective_language` and optional filter families in `pokrov-core`.
- Canonical analyzer result contract now exposes `decision`, `transform`, `explain`, `audit`, `executed`, and `degraded` sections.
- Unified metadata-only resolved-location records are exported through `EvaluateDecision.resolved_locations`.
- Policy block remains a successful analyzer outcome; analyzer invalid input/profile/runtime failures are represented as analyzer errors.

## Runtime and consumer compatibility evidence

- Evaluate handler now accepts optional `effective_language` and passes the canonical request contract to the core engine.
- LLM and MCP adapters now pass canonical request fields and preserve shared policy semantics.
- Foundation trace now exports shared `executed` and `degraded` metadata sections for runtime/evaluation compatibility proofs.

## Test and verification commands

```bash
cargo check --workspace --all-targets
```

Command results are appended after final verification run.

## Verification results (2026-04-05)

- `cargo check --workspace --all-targets` -> PASS
- `cargo test --test contract sanitization_evaluate_contract` -> PASS (3 passed, 0 failed)
- `cargo test --test integration sanitization_foundation_shared_contracts` -> PASS (1 passed, 0 failed)
- `cargo test --test security sanitization_foundation_metadata_leakage` -> PASS (1 passed, 0 failed)
- `cargo test --test performance sanitization_foundation_contract_overhead` -> PASS (1 passed, 0 failed)
- `cargo test --workspace` -> PASS
