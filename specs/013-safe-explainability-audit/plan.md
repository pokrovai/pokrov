# Implementation Plan: Safe Explainability and Audit

**Branch**: `013-safe-explainability-audit` | **Date**: 2026-04-05 | **Specification**: [spec.md](./spec.md)  
**Input**: Feature specification from `specs/013-safe-explainability-audit/spec.md`

## Summary

Freeze and implement metadata-only explain and audit contracts in the analyzer flow so runtime and evaluation consumers receive deterministic, safe provenance/reason/decision metadata without any raw payload leakage, with mode-based failure handling, explicit retention, and least-privilege access boundaries.

## Technical Context

**Language/Version**: Rust stable 1.85+ (edition 2021 workspace)  
**Primary Dependencies**: Existing workspace crates (`pokrov-core`, `pokrov-config`, `pokrov-proxy-llm`, `pokrov-proxy-mcp`) with `serde`, `serde_json`, `thiserror`, `tracing`, `axum`, `tokio`; no new external dependencies required  
**Storage**: N/A for persistent storage; in-memory analyzer outputs plus metadata-only audit/log sinks with 30-day operational retention policy  
**Testing**: `cargo test`, contract tests under `tests/contract/`, integration tests under `tests/integration/`, security tests under `tests/security/`, performance tests under `tests/performance/`  
**Target Platform**: Self-hosted Linux container runtime  
**Project Type**: Rust workspace security proxy (core analyzer + proxy/runtime crates)  
**Performance Goals**: Preserve v1 budgets (p95 overhead <= 50 ms, p99 <= 100 ms, startup <= 5 s, baseline throughput >= 500 RPS) and enforce explain/audit overhead <=10 ms p95 per request  
**Constraints**: Sanitization-first, deterministic policy outcomes, metadata-only explain/audit/logging, no raw snippets/context leakage, fail-closed for enforcement/runtime explain-audit failures, fail-open only for non-enforcing evaluation flows, least-privilege read access (security/ops only), no v1 scope expansion  
**Scale/Scope**: v1 explainability and audit safety hardening for allow/transform/block/degraded flows, reused across runtime and evaluation/parity outputs

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- `PASS`: Sanitization and policy enforcement remain pre-upstream; explain/audit/logging contracts are metadata-only and explicitly prohibit raw payload fragments.
- `PASS`: Determinism is preserved via explicit reason-code catalog, stable confidence buckets, and fixed safe output fields.
- `PASS`: Scope stays within approved v1 boundaries and existing crates; no A2A/RBAC/SIEM/control-plane additions.
- `PASS`: Observability requirements remain explicit (`request_id`, structured logs/metrics, readiness behavior, degradation markers).
- `PASS`: Verification gates are defined for unit/integration/security/performance, including leakage checks, mode-based failure policy, retention, and access control.
- `PASS`: No constitutional deviations are required.

## Project Structure

### Feature Documentation

```text
specs/013-safe-explainability-audit/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── safe-explainability-audit-contract.md
└── tasks.md
```

### Source Code

```text
crates/
├── pokrov-core/
│   └── src/
│       ├── analyzer/
│       ├── explain/
│       ├── audit/
│       ├── policy/
│       └── types/
├── pokrov-config/
│   └── src/
├── pokrov-proxy-llm/
├── pokrov-proxy-mcp/
└── pokrov-runtime/

tests/
├── contract/
├── integration/
├── performance/
└── security/
```

**Structure Decision**: Keep explain/audit schema and builder work centered in `pokrov-core` analyzer contracts with any profile validation alignment in `pokrov-config`; reuse existing proxy/runtime crates and test suites without adding new crate boundaries.

## Phase 0: Research

Research outcomes are recorded in [research.md](./research.md). Key decisions are:

1. Freeze an explicit metadata-only allowlist for explain and audit fields and reject any raw-content field additions by contract tests.
2. Standardize reason-code governance with a versioned catalog and deterministic stage emission.
3. Use confidence buckets in external-safe outputs while keeping internal scoring metadata constrained to safe contracts.
4. Enforce mode-based failure semantics for explain/audit generation errors.
5. Reuse runtime and evaluation/parity outputs from the same safe explain/audit contracts.
6. Enforce 30-day retention policy and least-privilege read access checks in operational validation.

## Phase 1: Design & Contracts

### Data Model

The Phase 1 data model is documented in [data-model.md](./data-model.md). It defines:

- safe explain summary schema and field invariants
- safe audit summary schema and field invariants
- reason-code catalog and emission rules
- confidence bucket policy
- mode-based failure handling behavior
- retention and access-control policy surfaces for explain/audit artifacts

### Contracts

The Phase 1 contract is documented in [contracts/safe-explainability-audit-contract.md](./contracts/safe-explainability-audit-contract.md). It freezes:

- allowed vs prohibited explain/audit fields
- reason-code catalog and extension/versioning rules
- deterministic confidence bucket requirements
- failure-mode behavior across enforcement/runtime vs evaluation flows
- runtime/evaluation consumer reuse guarantees

### Quickstart

Implementation and verification flow is documented in [quickstart.md](./quickstart.md).

## Post-Design Constitution Check

- `PASS`: Design keeps sanitization-first guarantees and prohibits raw content in explain/audit/logging surfaces.
- `PASS`: Deterministic behavior remains explicit for reason codes, confidence buckets, and failure policy outputs.
- `PASS`: Scope remains bounded to v1 and existing crate boundaries.
- `PASS`: Observability stays metadata-only and includes degradation/failure markers for operational debugging.
- `PASS`: Verification requirements are concrete across unit, contract, integration, security, and performance gates.

## Complexity Tracking

No constitutional deviations or out-of-scope exceptions are required for this plan.
