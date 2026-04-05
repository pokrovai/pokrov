# Backlog: Operator Semantics

Date: 2026-04-05
Spec source: `docs/superpowers/specs/presidio-rework/03-operator-semantics.md`
Status: Draft

## Summary

This backlog delivers the supported Pokrov-core anonymization operators and freezes how they apply after policy resolution.
Its purpose is to make transform behavior deterministic, JSON-safe, and auditable across plain text and structured JSON paths.

## Scope

In scope:
- `replace`
- `redact`
- `mask`
- `hash`
- `keep`
- deterministic operator application order
- blocking versus transformed outcomes
- JSON validity guarantees

Out of scope:
- runtime custom lambda operators
- reversible encryption or decryption in core
- deanonymization workflows

## Deliverables

- stable operator implementations for the five supported operators
- deterministic transform-plan application logic
- explicit blocked-result semantics
- transform result metadata usable by explain, audit, and evaluation

## Tasks

### Phase 1: Core operator implementations
- `O0301` Implement or normalize `replace`, `redact`, `mask`, `hash`, and `keep` in one shared transform layer.
- `O0302` Ensure operator configuration can be selected by entity, category, and profile without requiring family-specific transform paths.
- `O0303` Define and implement the safe one-way hashing behavior allowed in core.

### Phase 2: Application-order and blocking semantics
- `O0304` Encode transform-plan application order so resolved hits are applied deterministically after policy resolution.
- `O0305` Implement blocked-result handling so `block` never leaks sanitized payload to downstream execution but still returns safe explain and audit sections.
- `O0306` Ensure `keep` remains explicit in safe outputs and is not treated as silent passthrough.

### Phase 3: JSON-safe transform behavior
- `O0307` Guarantee only string leaves are transformed directly.
- `O0308` Ensure object and array structure remains intact for non-blocking outcomes.
- `O0309` Add overlap-aware transform tests proving suppressed or losing spans are not re-applied.

### Phase 4: Verification and evidence
- `O0310` Add unit tests for each operator family and configuration edge cases.
- `O0311` Add integration tests on nested JSON payloads and blocked/non-blocked flows.
- `O0312` Record transform verification evidence and any remaining limitations.

## Dependencies

- Depends on `00-architecture-foundation-backlog.md` and `01-analyzer-core-contract-backlog.md`.
- Strongly coupled with `02-deterministic-recognizers-backlog.md` because recognizer outputs feed transform plans.
- Blocks later structured-processing and evaluation work.

## Acceptance Evidence

Implementation is complete when:
- all five operators exist with stable semantics
- blocked versus transformed outcomes are explicit
- transform results preserve valid JSON for non-blocking flows
- operator application order is deterministic and overlap-aware
- transform result metadata can be reused by explain, audit, and evaluation paths

## Suggested Verification

- unit tests per operator
- nested JSON integration tests
- block-path tests
- deterministic replay of identical resolved-hit sets
