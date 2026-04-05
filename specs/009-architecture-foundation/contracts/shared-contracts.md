# Internal Contract Surface: Architecture Foundation

## Purpose

This document defines the internal shared-contract surface that later Presidio rework workstreams must consume.
It is not a user-facing API specification. It is the contract boundary between foundation work and downstream implementation work.

## Stage Ownership Contract

### Normalization

- Accepts raw text or structured payload input plus execution context.
- Produces traversal-ready views and context metadata.
- Must not mutate payloads.
- Must not choose policy actions.

### Recognizer Execution

- Consumes normalization outputs.
- Produces `NormalizedHit` families only.
- May invoke native recognizers and later remote recognizer adapters.
- Must not resolve overlap, choose operators, or emit audit payloads.

### Analysis And Suppression

- Consumes normalized hits.
- Produces `ResolvedHit` families.
- Owns validation, suppression, and precedence outcomes.
- Must not mutate payloads or own final policy action.

### Policy Resolution

- Consumes resolved hits plus profile context.
- Produces `TransformPlan`.
- Owns final action selection.
- Must not mutate payloads directly.

### Transformation

- Consumes transform plan plus original payload.
- Produces `TransformResult`.
- Owns payload mutation only.
- Must not re-run recognition or policy.

### Safe Explain

- Consumes resolved-hit and policy outcomes.
- Produces `ExplainSummary`.
- Must remain metadata-only.

### Audit Summary

- Consumes request context, policy outcome, and safe timing/count metadata.
- Produces `AuditSummary`.
- Must remain metadata-only.

## Shared Contract Families

Downstream workstreams must align with these families:

- `NormalizedHit`
- `ResolvedHit`
- `TransformPlan`
- `TransformResult`
- `ExplainSummary`
- `AuditSummary`
- `ExtensionPointContract`
- `EvaluationArtifactBoundary`

Current compile-visible exports live in:

- `crates/pokrov-core/src/types/foundation.rs`
- `crates/pokrov-core/src/types.rs`
- `crates/pokrov-core/src/lib.rs` via `SanitizationEngine::trace_foundation_flow`

## Downstream Consumer Rules

- Deterministic recognizer work may extend fields inside the approved hit families but must not create a competing top-level hit model.
- Structured JSON work may add field-aware semantics but must not bypass stage ownership or payload-safety rules.
- Evaluation work must reuse runtime-compatible result families rather than inventing a private evaluation-only result model.
- Remote recognizer work must normalize external outputs into the approved shared hit family.

## Metadata-Only Safety Contract

The following data classes must not appear in explain or audit families:

- raw payload text
- matched substrings
- nearby source fragments
- full leaf values copied from input payloads

Safe metadata includes:

- counts
- identifiers
- enums and reason codes
- profile and path metadata
- timing and degradation markers

## Executable Proof Requirement

The foundation is not complete until at least one executable proof demonstrates that:

- a runtime-oriented flow can emit the approved contract families; and
- an evaluation-oriented flow can consume or verify those same families without a separate adapter-only result model.

The current executable proof lives in `tests/integration/sanitization_foundation_shared_contracts.rs`.
