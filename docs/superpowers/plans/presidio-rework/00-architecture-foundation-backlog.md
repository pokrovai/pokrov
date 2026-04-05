# Backlog: Architecture Foundation

Date: 2026-04-05
Spec source: `docs/superpowers/specs/presidio-rework/00-architecture-foundation.md`
Status: Implemented by `specs/009-architecture-foundation`

## Summary

This backlog establishes the minimum shared architecture for the Presidio rework.
Its job is not feature delivery. Its job is to freeze the internal contracts that later recognizer, operator, structured, evaluation, and remote-adapter work will depend on.

## Scope

In scope:
- pipeline stage boundaries
- shared internal types
- extension-point traits and contracts
- repository layout for evaluation-safe and restricted artifacts
- foundation-level verification

Out of scope:
- recognizer-family behavior
- operator-family implementation details
- entity coverage
- remote adapter implementations
- evaluation corpora population

## Deliverables

- frozen stage boundaries inside `pokrov-core`
- shared internal types for normalized hits, resolved hits, transform planning, explain, and audit
- explicit extension-point interfaces for native recognizers, remote recognizers, structured processors, evaluation runners, and baseline runners
- repository layout rules documented where future work will consume them

## Implementation Notes

- `crates/pokrov-core/src/types/foundation.rs` now holds the compile-visible stage boundaries, extension-point contracts, shared hit families, transform plan, and evaluation artifact boundaries.
- `crates/pokrov-core/src/lib.rs` now exposes `SanitizationEngine::trace_foundation_flow` for runtime and evaluation proof reuse.
- `docs/verification/009-architecture-foundation.md` is the evidence sink for contract, integration, security, performance, and workspace verification output.

## Tasks

### Phase 1: Contract scaffolding
- `F0001` Define the pipeline stage boundary map in code-facing documentation and ensure the crate/module structure can host `normalization`, `recognizer`, `analysis`, `policy`, `transform`, `explain`, and `audit` responsibilities without ambiguity.
- `F0002` Introduce or refactor shared internal type definitions so `NormalizedHit`, `ResolvedHit`, `TransformPlan`, `TransformResult`, `ExplainSummary`, and `AuditSummary` have stable placeholders or final fields aligned with the spec.
- `F0003` Add explicit extension-point interfaces for `NativeRecognizer`, `RemoteRecognizer`, `StructuredProcessor`, `EvaluationRunner`, and `BaselineRunner` in a way that does not yet require concrete implementations.

### Phase 2: Dependency and invariants wiring
- `F0004` Enforce the stage dependency direction so analysis does not depend on transform internals, transform does not own policy logic, and explain/audit consume only safe post-analysis and post-policy data.
- `F0005` Encode the metadata-only invariant into shared explain/audit-facing types so raw fragments cannot be carried by accident.
- `F0006` Document or codify repository placement rules for repo-safe fixtures versus restricted evaluation data references.

### Phase 3: Verification baseline
- `F0007` Add unit or compile-level verification that shared contracts can be referenced from multiple future subsystems without circular ownership.
- `F0008` Add a foundation test or fixture proving a runtime flow and an evaluation-style flow can target the same top-level contract families.
- `F0009` Record foundation acceptance evidence in a dedicated verification note once implementation lands.

## Dependencies

- No upstream implementation dependency beyond the existing workspace.
- Blocks all downstream Presidio rework implementation specs.

## Acceptance Evidence

Implementation is complete when:
- all shared contract types exist in a stable location
- extension-point interfaces exist and compile
- stage ownership is unambiguous
- no-raw-data constraints are encoded in explain/audit-facing contracts
- downstream work can reference these contracts without redefining them

## Suggested Verification

- targeted unit tests for contract construction
- compile-time checks for trait visibility and cross-crate references
- one documented walkthrough showing how a payload would move through each frozen stage without family-specific logic
