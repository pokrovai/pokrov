# Implementation Plan: Structured JSON Processing

**Branch**: `014-structured-json-processing` | **Date**: 2026-04-05 | **Specification**: [spec.md](./spec.md)  
**Input**: Feature specification from `specs/014-structured-json-processing/spec.md`

## Summary

Implement deterministic inline structured JSON processing that analyzes only string leaves with path-aware policy binding, preserves payload shape, and emits metadata-only explain/audit summaries without exact path leakage.

## Technical Context

**Language/Version**: Rust stable 1.85+ (edition 2021 workspace)  
**Primary Dependencies**: Existing workspace crates (`pokrov-core`, `pokrov-config`, `pokrov-proxy-llm`, `pokrov-proxy-mcp`) with `serde`, `serde_json`, `serde_yaml`, `thiserror`, `tracing`, `axum`, `tokio`; no new external dependencies required  
**Storage**: N/A for persistent storage; in-memory traversal/context state plus metadata-only logs/audit summaries  
**Testing**: `cargo test`, contract tests under `tests/contract/`, integration tests under `tests/integration/`, security tests under `tests/security/`, performance checks under `tests/performance/`  
**Target Platform**: Self-hosted Linux container runtime  
**Project Type**: Rust workspace security proxy (core analyzer + LLM/MCP proxy crates)  
**Performance Goals**: For payload <=1 MB: p95 sanitization+proxy overhead <=50 ms (p99 <=100 ms aligned with v1); for payload >1 MB: best-effort processing without latency SLA while preserving safety invariants  
**Constraints**: Sanitization-first, deterministic traversal and precedence, transform only string leaves, preserve JSON shape, metadata-only explain/audit, no exact JSON pointer in summaries, fail-closed on high-risk processing errors, no v1 scope expansion  
**Scale/Scope**: v1 inline structured JSON processing for nested object/array payloads in LLM and MCP mediation paths

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- `PASS`: Sanitization and policy enforcement remain pre-upstream; explain/audit outputs remain metadata-only and exclude raw payload data.
- `PASS`: Determinism is explicit via stable traversal behavior and fixed binding precedence (exact pointer -> alias -> subtree -> profile -> global).
- `PASS`: Scope remains within approved v1 boundaries and existing crate architecture; no A2A/RBAC/SIEM/control-plane additions.
- `PASS`: Observability remains explicit (`request_id`, structured logs/metrics, readiness compatibility) with safe summary fields.
- `PASS`: Verification gates cover unit/integration/security/performance, including shape preservation, precedence correctness, and leakage prevention.
- `PASS`: No constitutional deviations required.

## Project Structure

### Feature Documentation

```text
specs/014-structured-json-processing/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── structured-json-processing-contract.md
└── tasks.md
```

### Source Code

```text
crates/
├── pokrov-core/
│   └── src/
│       ├── analyzer/
│       ├── transform/
│       ├── policy/
│       ├── explain/
│       └── audit/
├── pokrov-config/
│   └── src/
├── pokrov-proxy-llm/
└── pokrov-proxy-mcp/

tests/
├── contract/
├── integration/
├── performance/
└── security/
```

**Structure Decision**: Keep structured traversal, path-context, and precedence enforcement in `pokrov-core`; keep config/profile validation in `pokrov-config`; wire behavior through existing LLM/MCP proxy paths without introducing new crates.

## Phase 0: Research

Research outcomes are documented in [research.md](./research.md). Key outcomes:

1. Deterministic traversal and path-context contract for nested JSON.
2. Path-aware rule precedence and conflict resolution contract.
3. Payload-size policy split (<=1 MB SLA, >1 MB best-effort) with security invariants unchanged.
4. Failure policy for processing errors (fail-closed for high-risk).
5. Metadata-only explain/audit contract with path-safe categories only.
6. Test and evidence strategy to prove no raw leakage and structural preservation.

## Phase 1: Design & Contracts

### Data Model

Data structures and invariants are specified in [data-model.md](./data-model.md), including:

- traversal context for string leaves
- path binding rule model and precedence ordering
- transformation result model preserving JSON structure
- safe summary model for explain/audit without exact pointers
- failure and size-policy behavior model

### Contracts

Design contract is documented in [contracts/structured-json-processing-contract.md](./contracts/structured-json-processing-contract.md), including:

- deterministic traversal semantics
- path-aware binding precedence
- size handling and error behavior requirements
- summary safety and prohibited fields
- cross-flow contract reuse (plain text + structured)

### Quickstart

Implementation and verification workflow is documented in [quickstart.md](./quickstart.md).

## Post-Design Constitution Check

- `PASS`: Design keeps sanitization-first behavior and blocks high-risk failures without exposing unsanitized data.
- `PASS`: Determinism is encoded in traversal/order and precedence rules.
- `PASS`: Scope remains bounded to v1 inline structured JSON mode and existing crate boundaries.
- `PASS`: Observability and audit safety remain metadata-only and path-safe.
- `PASS`: Verification plan covers unit, integration, performance, and security evidence.

## Complexity Tracking

No constitutional deviations or out-of-scope exceptions are required for this plan.
