# Implementation Plan: Architecture Foundation For Presidio Rework

**Branch**: `009-architecture-foundation` | **Date**: 2026-04-05 | **Spec**: [specs/009-architecture-foundation/spec.md](specs/009-architecture-foundation/spec.md)
**Input**: Feature specification from `specs/009-architecture-foundation/spec.md`

## Summary

Freeze the minimum shared architecture for the Presidio rework by adding documentation-backed, compile-visible contract scaffolding for stage boundaries, shared hit and transform families, safe explain and audit summaries, and extension points. The plan is intentionally narrow: it prepares `pokrov-core` and related consumers for later deterministic, structured, evaluation, and remote-recognizer work without implementing recognizer families or changing policy behavior.

## Technical Context

**Language/Version**: Rust stable 1.85+  
**Primary Dependencies**: Rust workspace crates with existing `serde`, `serde_json`, `thiserror`, `tracing`-compatible types; no new external dependency required for the planning baseline  
**Storage**: N/A for persistent storage; compile-visible shared contracts plus existing in-memory runtime metadata structures  
**Testing**: `cargo test`, targeted `pokrov-core` unit tests, integration proof for shared runtime/evaluation contracts, metadata-safety checks, workspace regression checks  
**Target Platform**: Linux x86_64/aarch64, Docker-compatible self-hosted runtime  
**Project Type**: Rust workspace, multi-crate security proxy service (`pokrov-core`, `pokrov-api`, `pokrov-proxy-llm`, `pokrov-proxy-mcp`, `pokrov-config`, `pokrov-runtime`, `pokrov-metrics`)  
**Performance Goals**: Preserve v1 constraints: p95 overhead <= 50 ms, p99 <= 100 ms, no duplicate hot-path contract conversions  
**Constraints**: Sanitization-first, deterministic behavior, metadata-only audit, no raw payload in explain/audit, self-hosted only, no external control plane, no family-level behavior changes in this feature  
**Scale/Scope**: Foundation-only feature for the Presidio rework; affects shared contracts and stage ownership for all later workstreams

## Constitution Check

*GATE: Passed before Phase 0 research and re-checked after Phase 1 design.*

### Pre-Design Gate

- **Sanitization Before External Access**: PASS. The feature does not alter upstream call order and explicitly preserves metadata-only explain and audit boundaries.
- **Deterministic Policy Application**: PASS. The feature freezes stage ownership and contract families without introducing new policy heuristics or transform logic.
- **Approved Interfaces And Bounded Scope**: PASS. The scope is limited to existing Rust workspace boundaries and shared internal contracts for v1.
- **Observability And Explainable Operations**: PASS. The plan preserves `request_id`, structured metadata-only outputs, and future observability compatibility without adding payload-bearing diagnostics.
- **Verification Without Exceptions**: PASS. The feature requires unit, integration, performance-neutrality, and security evidence for shared contracts and runtime/evaluation reuse.
- **Constitution Deviations**: PASS. No deviation is required.

### Post-Design Re-Check

- `research.md` fixes the key design decisions for scaffolding level, executable proof requirement, data-boundary scope, and contract revision control.
- `data-model.md` defines the shared contract families and their validation boundaries.
- `contracts/` documents the internal shared-contract surface and the revision-control rules for future workstreams.
- `quickstart.md` describes how to validate stage ownership, shared-contract reuse, and metadata-only boundaries after implementation.

## Project Structure

### Feature Documentation

```text
specs/009-architecture-foundation/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── shared-contracts.md
│   └── revision-policy.md
└── tasks.md
```

### Source Code

```text
Cargo.toml
config/
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
```

**Structure Decision**: The implementation remains within current crate boundaries. The shared foundation contracts are expected to live in `crates/pokrov-core` and be consumed by `pokrov-api`, proxy crates, and later evaluation tooling. Feature documentation stays under `specs/009-architecture-foundation/`.

## Phase 0: Research Output

- Decide whether the foundation is documentation-only or must include compile-visible shared scaffolding.
- Decide whether runtime/evaluation contract reuse requires executable proof or only narrative documentation.
- Decide whether evaluation-data scope in foundation includes only repository placement boundaries or also lifecycle/governance policy.
- Decide how to represent internal contracts for downstream workstreams inside `contracts/`.
- Decide how frozen-contract revisions are triggered and governed.
- Decide how to evolve existing `ExplainSummary` and `AuditSummary` safely instead of rewriting the entire core result model.

## Phase 1: Design Output

- `data-model.md`: shared entities for stage boundaries, hit families, transform families, explain/audit summaries, extension points, and evaluation artifact boundaries.
- `contracts/shared-contracts.md`: internal contract surface for stage ownership, shared result families, and downstream consumer expectations.
- `contracts/revision-policy.md`: explicit rules for when a downstream change must become a foundation revision.
- `quickstart.md`: validation workflow for runtime/evaluation contract reuse, metadata-only guarantees, and repository-safe evaluation boundaries.
- Update agent context via `.specify/scripts/bash/update-agent-context.sh codex`.

## Complexity Tracking

| Deviation | Why Needed | Why Simpler Alternative Was Rejected |
|-----------|------------|--------------------------------------|
| None | The foundation fits the constitution and current v1 scope | N/A |
