# Backlog: Structured JSON Processing

Date: 2026-04-05
Spec source: `docs/superpowers/specs/presidio-rework/05-structured-json-processing.md`
Status: Draft

## Summary

This backlog delivers inline structured and semi-structured JSON processing on top of the shared analyzer and transform contracts.
Its purpose is to preserve JSON-safe traversal while adding field-aware semantics, path-sensitive policies, and structured summaries.

## Scope

In scope:
- deterministic traversal of nested objects and arrays
- field and path metadata preservation
- path-aware recognizer and operator binding
- inline structured detection and transformation
- structured summaries without value leakage

Out of scope:
- CSV and SQL-style batch processors
- document-layout understanding
- OCR and image extraction

## Deliverables

- stable traversal behavior for structured JSON
- field-aware binding layer for recognizer-family and operator overrides
- path-precedence rules enforced in runtime behavior
- structured explain and audit summaries built on metadata-only outputs

## Tasks

### Phase 1: Traversal and path semantics
- `S0501` Implement or normalize deterministic traversal for objects and arrays with stable path metadata on every string leaf.
- `S0502` Ensure non-string leaves are preserved and never transformed directly.
- `S0503` Add support for empty-string leaves and mixed nested structures without breaking traversal.

### Phase 2: Field-aware binding
- `S0504` Introduce path-aware recognizer-family inclusion and exclusion rules.
- `S0505` Introduce path-aware operator defaults and high-risk field overrides.
- `S0506` Encode precedence between exact pointer overrides, logical field aliases, subtree defaults, profile defaults, and global defaults.

### Phase 3: Structured runtime behavior
- `S0507` Ensure inline structured detection retains field path metadata through normalized and resolved hits.
- `S0508` Ensure inline structured transformation preserves object and array shape exactly.
- `S0509` Add structured summaries that expose safe counts and categories without exposing field values.

### Phase 4: Verification and evidence
- `S0510` Add unit tests for traversal order and path binding precedence.
- `S0511` Add integration tests on nested JSON with path-specific overrides and mixed leaf types.
- `S0512` Record structured-processing verification evidence and any deferred batch-mode gaps.

## Dependencies

- Depends on `00-architecture-foundation-backlog.md` and `01-analyzer-core-contract-backlog.md`.
- Depends on `02-deterministic-recognizers-backlog.md`, `03-operator-semantics-backlog.md`, and `04-safe-explainability-and-audit-backlog.md`.
- Provides the runtime base for later structured evaluation work.

## Acceptance Evidence

Implementation is complete when:
- traversal is deterministic
- path binding precedence is encoded and testable
- only string leaves are transformed directly
- field-aware summaries remain metadata-only
- structured JSON uses the same shared contracts as plain text

## Suggested Verification

- unit tests for traversal and binding precedence
- nested JSON integration tests
- block/non-block structured transform tests
- safe structured explain and audit coverage
