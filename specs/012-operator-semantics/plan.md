# Implementation Plan: Operator Semantics Freeze

**Branch**: `012-operator-semantics` | **Date**: 2026-04-05 | **Specification**: [spec.md](./spec.md)  
**Input**: Feature specification from `/specs/012-operator-semantics/spec.md`

## Summary

Freeze and implement one deterministic transform contract for `replace`, `redact`, `mask`, `hash`, and `keep` in `pokrov-core`, with explicit fail-closed handling for unsupported operators, deterministic transform ordering after overlap resolution, JSON-safe structured traversal guarantees, and metadata-only explain/audit outputs shared by LLM, MCP, and evaluation flows.

## Technical Context

**Language/Version**: Rust stable 1.85+ (edition 2021 workspace)  
**Primary Dependencies**: Existing workspace crates (`pokrov-core`, `pokrov-config`, `pokrov-proxy-llm`, `pokrov-proxy-mcp`) with `serde`, `serde_json`, `thiserror`, `tracing`, `axum`, `tokio`; no new external dependencies required  
**Storage**: N/A for persistent storage; in-memory transform plans/results and metadata-only audit summaries  
**Testing**: `cargo test`, contract tests under `tests/contract/`, integration tests under `tests/integration/`, security tests under `tests/security/`, performance tests under `tests/performance/`  
**Target Platform**: Self-hosted Linux container runtime  
**Project Type**: Rust workspace security proxy (core library + proxy/runtime crates)  
**Performance Goals**: Preserve v1 budgets: p95 overhead <= 50 ms, p99 <= 100 ms, startup <= 5 s, baseline throughput >= 500 RPS  
**Constraints**: Sanitization-first, metadata-only logs/audit/explain, deterministic replay behavior, JSON-safe leaf-only transforms, fail-closed `block` on unsupported operators, no reversible deanonymization, no runtime lambda operators, no crate-boundary expansion  
**Scale/Scope**: v1 operator semantics hardening across plain text + nested JSON payloads, shared analyzer/transform contract for runtime and evaluation consumers

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- `PASS`: Sanitization and policy enforcement remain before any upstream forwarding; transformed and blocked outcomes keep raw payloads out of logs/audit/explain contracts.
- `PASS`: Deterministic behavior is explicit: identical resolved hits yield identical ordering, operators, outputs, and metadata.
- `PASS`: Scope is bounded to v1 operator semantics in existing crates; no A2A/RBAC/SIEM/control-plane additions.
- `PASS`: Observability remains metadata-only and reuses request_id/structured metrics semantics.
- `PASS`: Unit, integration, performance, and security verification requirements are defined and mapped to existing test surfaces.
- `PASS`: No constitutional deviation is required.

## Project Structure

### Feature Documentation

```text
specs/012-operator-semantics/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── operator-semantics-contract.md
└── tasks.md
```

### Source Code

```text
crates/
├── pokrov-core/
│   └── src/
│       ├── transform/
│       ├── policy/
│       ├── traversal/
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

**Structure Decision**: Keep operator semantics changes centered in `pokrov-core` transform/policy/traversal contracts, with any profile-validation alignment in `pokrov-config`; reuse existing proxy/runtime crates and test directories without introducing new crate boundaries.

## Phase 0: Research

Research outcomes are recorded in [research.md](./research.md). The key decisions are:

1. Freeze exactly five supported core operators and reject all others via fail-closed block semantics.
2. Apply transforms only on post-policy resolved hits in deterministic order after overlap suppression.
3. Enforce one-way deterministic `hash` behavior scoped by profile identity.
4. Keep `keep` as an explicit policy action with mandatory metadata-only explain/audit signaling.
5. Reuse existing verification surfaces for deterministic replay, block path isolation, JSON validity, and metadata-only safety.

## Phase 1: Design & Contracts

### Data Model

The Phase 1 data model is documented in [data-model.md](./data-model.md). It defines:

- operator policy bindings and runtime-resolved transform plans
- deterministic per-hit application ordering and suppression traces
- blocked-vs-transformed result shapes
- JSON traversal invariants for string-leaf mutation only
- metadata-only explain/audit summaries for operator outcomes

### Contracts

The Phase 1 contract is documented in [contracts/operator-semantics-contract.md](./contracts/operator-semantics-contract.md). It freezes:

- supported operator set and forbidden operator handling
- deterministic transform application order
- block-path payload suppression behavior
- JSON-validity guarantees for non-blocking flows
- consumer-visible metadata fields for explain/audit/evaluation

### Quickstart

Implementation and verification flow is documented in [quickstart.md](./quickstart.md).

## Post-Design Constitution Check

- `PASS`: Design keeps sanitization-first and prevents payload forwarding on terminal `block`.
- `PASS`: Determinism is explicit for ordering, `hash`, overlap suppression, and replay equality.
- `PASS`: Scope remains within approved v1 boundaries and existing crates.
- `PASS`: Observability and audit requirements stay metadata-only and testable.
- `PASS`: Verification gates are concrete across unit, integration, security, and performance checks.

## Complexity Tracking

No constitutional deviations or out-of-scope exceptions are required for this plan.
