# Implementation Plan: Deterministic Recognizers

**Branch**: `011-deterministic-recognizers` | **Date**: 2026-04-05 | **Specification**: [spec.md](./spec.md)  
**Input**: Feature specification from `specs/011-deterministic-recognizers/spec.md`

## Summary

Implement the first native deterministic recognizer subsystem inside the existing Pokrov analyzer pipeline by extending `pokrov-core` and `pokrov-config` with pattern, validation, context, denylist, and allowlist families that compile at startup, emit metadata-only normalized candidates, and preserve deterministic replay, JSON-safe traversal, and metadata-only explain and audit behavior for LLM, MCP, runtime, and evaluation consumers.

## Technical Context

**Language/Version**: Rust stable 1.85+ in a workspace using edition 2021  
**Primary Dependencies**: `pokrov-core`, `pokrov-config`, `serde`, `serde_json`, `serde_yaml`, `regex`, `thiserror`, existing tracing-compatible metadata contracts  
**Storage**: N/A for persistent storage; in-memory compiled profile state plus metadata-only explain and audit summaries  
**Testing**: `cargo test`, existing contract tests under `tests/contract/`, integration tests under `tests/integration/`, security tests under `tests/security/`, performance tests under `tests/performance/`  
**Target Platform**: Self-hosted Linux container runtime  
**Project Type**: Rust workspace with a core sanitization library and proxy/runtime crates  
**Performance Goals**: Preserve Pokrov v1 targets of p95 overhead <= 50 ms, p99 <= 100 ms, startup <= 5 s, and baseline throughput >= 500 RPS  
**Constraints**: Sanitization-first before upstream traffic, metadata-only audit and explain outputs, deterministic replay identity, JSON-safe traversal over string leaves, no ML recognizers, no remote recognizer implementation in this feature, no new control-plane concepts, no new crates unless required by accepted design  
**Scale/Scope**: Phase-one deterministic recognizers for the analyzer core, profile-scoped list controls, EN/RU lexical context, and shared runtime/evaluation contracts reused by LLM and MCP flows

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- `PASS`: Sanitization and policy enforcement remain upstream of external execution, and the feature design keeps raw matched values out of logs, audit payloads, and explain outputs.
- `PASS`: Deterministic behavior is central to the design. The plan fixes validation defaults, same-span ordering, allowlist scope, and negative-context defaults before implementation work begins.
- `PASS`: Scope remains within Pokrov v1. The feature adds native deterministic recognizers only and explicitly excludes ML recognizers, remote recognizer implementation, and broader entity-pack expansion.
- `PASS`: Observability remains metadata-only and extends existing explain and audit summaries rather than introducing new raw-content telemetry.
- `PASS`: Required verification is planned across unit, integration, security, contract, and performance gates using the existing repository test layout.
- `PASS`: No constitutional deviation is currently required.

## Project Structure

### Feature Documentation

```text
specs/011-deterministic-recognizers/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── deterministic-recognizer-contract.md
└── tasks.md
```

### Source Code

```text
crates/
├── pokrov-core/
│   └── src/
│       ├── detection/
│       ├── policy/
│       ├── traversal/
│       ├── types.rs
│       └── types/foundation/
├── pokrov-config/
│   └── src/
│       ├── model.rs
│       └── validate.rs
├── pokrov-proxy-llm/
├── pokrov-proxy-mcp/
└── pokrov-runtime/

tests/
├── contract/
├── integration/
├── performance/
└── security/
```

**Structure Decision**: Keep implementation centered in `crates/pokrov-core` for recognizer execution, candidate normalization, and deterministic overlap analysis; extend `crates/pokrov-config` for startup-validated profile configuration; reuse existing `tests/` suites for contract, integration, security, and performance evidence; do not move crate boundaries.

## Phase 0: Research

Research outcomes are recorded in [research.md](./research.md). The main decisions are:

1. Extend the existing analyzer pipeline rather than adding a sidecar recognition service or a policy-layer workaround.
2. Compile deterministic recognizer families and scoped list controls at startup from `pokrov-config` profile definitions.
3. Normalize all deterministic family outputs into the shared foundation hit model before overlap resolution and policy selection.
4. Preserve security-first defaults established during clarification:
   - validation failure rejects by default
   - same-span winners use final score, then family priority, then stable recognizer ordering
   - allowlists suppress only exact normalized matches
   - negative context downscores by default and suppresses only when a family explicitly opts in
5. Reuse existing contract, integration, performance, and security test suites instead of creating a parallel verification surface.

## Phase 1: Design & Contracts

### Data Model

The Phase 1 data model is documented in [data-model.md](./data-model.md). It introduces:

- compiled deterministic recognizer definitions per profile
- validator and context policy metadata
- scoped allowlist and denylist entries
- deterministic candidate lifecycle from raw match to resolved hit
- explicit precedence trace and suppression metadata

### Contracts

The Phase 1 runtime and configuration contract is documented in [contracts/deterministic-recognizer-contract.md](./contracts/deterministic-recognizer-contract.md). It freezes:

- profile-scoped deterministic recognizer configuration additions
- the deterministic execution pipeline order
- normalized candidate fields required by the analyzer foundation
- exact-match allowlist semantics and candidate suppression metadata
- same-span ordering and validation/context default behavior

### Quickstart

Implementation and verification flow is documented in [quickstart.md](./quickstart.md).

## Post-Design Constitution Check

- `PASS`: The design keeps recognizer execution separate from policy resolution and transformation.
- `PASS`: Candidate and resolved-hit contracts remain metadata-only and preserve JSON-safe traversal semantics.
- `PASS`: The implementation plan stays inside approved v1 crate boundaries and does not introduce new product scope.
- `PASS`: Logging, metrics, health/readiness implications, and verification evidence are explicitly included in design artifacts.
- `PASS`: The design provides concrete verification paths for unit, contract, integration, security, and performance checks.

## Complexity Tracking

No constitutional deviations or extra-scope exceptions are required for this plan.
