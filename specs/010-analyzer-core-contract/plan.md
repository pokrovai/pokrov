# Implementation Plan: Analyzer Core Contract For Presidio Rework

**Branch**: `010-analyzer-core-contract` | **Date**: 2026-04-05 | **Spec**: [specs/010-analyzer-core-contract/spec.md](specs/010-analyzer-core-contract/spec.md)
**Input**: Feature specification from `specs/010-analyzer-core-contract/spec.md`

## Summary

Freeze one shared analyzer-facing request and result contract by evolving the current `pokrov-core` evaluation path into the canonical analyzer contract consumed by runtime adapters and evaluation flows. The plan is intentionally narrow: it extends the existing `EvaluateRequest`, `EvaluateResult`, foundation trace, and adapter expectations so LLM, MCP, structured JSON, and evaluation consumers reuse one deterministic, metadata-safe result family with unified resolved-location records, mandatory effective language, and successful fail-closed degraded outcomes, without introducing a second parallel contract stack or a new public wire API.

## Technical Context

**Language/Version**: Rust stable 1.85+  
**Primary Dependencies**: Existing workspace crates and dependencies, especially `serde`, `serde_json`, `thiserror`, `tracing`, `axum`, `tokio`; current analyzer/foundation exports already live in `pokrov-core`  
**Storage**: N/A for persistent storage; in-memory analyzer request/result metadata and metadata-only audit summaries  
**Testing**: `cargo test`, targeted contract/integration/security/performance suites, shared consumer-compatibility proofs, workspace regression checks  
**Target Platform**: Linux x86_64/aarch64, Docker-compatible self-hosted runtime  
**Project Type**: Rust workspace, multi-crate security proxy service (`pokrov-core`, `pokrov-api`, `pokrov-proxy-llm`, `pokrov-proxy-mcp`, `pokrov-config`, `pokrov-runtime`, `pokrov-metrics`)  
**Performance Goals**: Preserve v1 constraints: p95 overhead <= 50 ms, p99 <= 100 ms, no duplicate hot-path contract conversions or replay-identity recomputation outside `pokrov-core`  
**Constraints**: Sanitization-first, deterministic replay, metadata-only explain/audit/execution/degradation sections, no raw payload logging, mandatory effective language in the shared request contract, fail-closed degradation when required evidence is missing, no new public external wire API, no consumer-local result forks  
**Scale/Scope**: Analyzer-core contract milestone affecting `pokrov-core`, the evaluate HTTP adapter, LLM/MCP adapter expectations, and shared contract tests

## Constitution Check

*GATE: Passed before Phase 0 research and re-checked after Phase 1 design.*

### Pre-Design Gate

- **Sanitization Before External Access**: PASS. The feature freezes the analyzer contract and keeps policy-block handling inside the existing pre-upstream pipeline.
- **Deterministic Policy Application**: PASS. The plan centers on deterministic replay identity, deterministic ordering, unified resolved-location records, and one shared decision/result model.
- **Approved Interfaces And Bounded Scope**: PASS. Scope is limited to current Rust workspace contracts and adapter reuse rules; no new product surface or out-of-scope platform capability is introduced.
- **Observability And Explainable Operations**: PASS. The plan preserves metadata-only explain/audit behavior and formalizes safe `executed` and `degraded` reporting, including fail-closed degradation facts, without payload leakage.
- **Verification Without Exceptions**: PASS. The plan requires contract, integration, security, and performance evidence for analyzer result reuse, fail-closed degradation behavior, and policy-block/error separation.
- **Constitution Deviations**: PASS. No deviation is required.

### Post-Design Re-Check

- `research.md` fixes the contract-evolution path, result-section design, fail-closed degradation semantics, effective-language handling, compatibility-proof strategy, and consumer-surface rules.
- `data-model.md` defines the analyzer request/result families, unified metadata-safe location records, execution/degradation summaries, effective-language handling, and analyzer error model.
- `contracts/` documents the internal analyzer surface and the downstream consumer compatibility rules, including fail-closed degradation and unified location reuse.
- `quickstart.md` describes how to validate request/result completeness, block-versus-error behavior, consumer reuse, replay stability, effective-language handling, and metadata-only safety after implementation.

## Project Structure

### Feature Documentation

```text
specs/010-analyzer-core-contract/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── analyzer-surface.md
│   └── consumer-compatibility.md
└── tasks.md
```

### Source Code

```text
Cargo.toml
crates/
├── pokrov-api/
├── pokrov-config/
├── pokrov-core/
├── pokrov-metrics/
├── pokrov-proxy-llm/
├── pokrov-proxy-mcp/
└── pokrov-runtime/
docs/
specs/
tests/
├── contract/
├── integration/
├── performance/
└── security/
```

**Structure Decision**: The implementation remains inside current crate boundaries. The canonical analyzer contract should be owned by `pokrov-core`, reused by `pokrov-api` evaluate handling, and consumed consistently by LLM, MCP, structured, and evaluation paths through shared tests and adapter-local response wrappers. `pokrov-core` owns the effective-language field, the unified resolved-location model, and the degraded/fail-closed contract semantics so adapters do not fork those behaviors locally.

## Phase 0: Research Output

- Decide whether the analyzer contract should evolve the existing `EvaluateRequest`/`EvaluateResult` path or introduce a parallel analyzer model.
- Decide how `executed` and `degraded` should become explicit reusable result sections with successful fail-closed degraded outcomes instead of adapter-specific or ad hoc metadata.
- Decide how the decision section should represent one unified resolved-location record for both plain text and structured JSON without leaking raw values.
- Decide how policy-block outcomes and analyzer errors remain distinct across evaluate, LLM, MCP, and evaluation consumers.
- Decide how to prove consumer compatibility using the current test surface without inventing a new public wire schema.
- Decide how effective language and optional recognizer/entity-scope inputs enter the shared contract while preserving current adapter compatibility through deterministic adapter defaults.

## Phase 1: Design Output

- `data-model.md`: analyzer request/result entities, unified location model, transform section, execution/degradation summaries, effective-language handling, and analyzer error model.
- `contracts/analyzer-surface.md`: internal analyzer request/result surface, top-level section rules, fail-closed degradation semantics, and metadata-safe boundaries.
- `contracts/consumer-compatibility.md`: evaluate/LLM/MCP/structured/evaluation reuse rules, policy-block/error semantics, unified location reuse, and proof requirements.
- `quickstart.md`: validation workflow for shared analyzer contract reuse, deterministic replay identity, effective-language handling, fail-closed degradation behavior, and metadata-only result safety.
- Update agent context via `.specify/scripts/bash/update-agent-context.sh codex`.

## Complexity Tracking

| Deviation | Why Needed | Why Simpler Alternative Was Rejected |
|-----------|------------|--------------------------------------|
| None | The analyzer-contract feature fits the constitution and current v1 scope | N/A |
