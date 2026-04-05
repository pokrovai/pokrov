# Backlog: Deterministic Recognizers

Date: 2026-04-05
Spec source: `docs/superpowers/specs/presidio-rework/02-deterministic-recognizers.md`
Status: Draft

## Summary

This backlog delivers the first native Presidio-style recognizer block in Pokrov: pattern, checksum, context, denylist, and allowlist mechanisms with deterministic precedence.
Its purpose is to make the analyzer contract useful for real security decisions without pulling in ML or remote dependencies.

## Scope

In scope:
- pattern recognizers
- checksum and validation recognizers
- context-aware lexical scoring
- denylist positives
- allowlist suppression
- deterministic precedence and scoring pipeline

Out of scope:
- ML recognizers
- remote recognizer integrations
- final EN/RU entity-pack breadth beyond the first priority entities
- operator implementation details beyond operator hints emitted by recognizers

## Deliverables

- native deterministic recognizer interfaces and concrete recognizer families
- validator and checksum hooks in the candidate-hit pipeline
- explicit context boost and suppression behavior
- allowlist and denylist handling with safe reason codes
- deterministic overlap input into the analyzer decision stage

## Tasks

### Phase 1: Pattern and validation baseline
- `D0201` Implement pattern-recognizer support for multiple patterns per entity with explicit base score and priority.
- `D0202` Introduce validator and invalidator hooks so pattern-based families can normalize and validate candidates before final candidate scoring.
- `D0203` Implement checksum-capable recognizer helpers for early families such as card-like numbers and IBAN.

### Phase 2: Context and list mechanisms
- `D0204` Add lexical context boost and suppression support with recognizer-specific EN/RU dictionaries.
- `D0205` Implement denylist recognizers that emit explicit high-confidence positives with provenance.
- `D0206` Implement allowlist suppression before final overlap resolution with explicit suppression reason codes.

### Phase 3: Deterministic precedence and output alignment
- `D0207` Encode the deterministic scoring pipeline `pattern -> normalize -> validate/checksum -> context -> allowlist suppression -> overlap input` in shared recognizer orchestration.
- `D0208` Ensure each deterministic candidate emits the normalized fields required by the frozen hit contract.
- `D0209` Add deterministic ordering and precedence tests for same-span and same-family collisions.

### Phase 4: Coverage and verification
- `D0210` Implement the first concrete deterministic families needed by the EN/RU phase-one entity set.
- `D0211` Add structured-JSON compatibility tests proving behavior is consistent on normalized string leaves.
- `D0212` Record deterministic-recognizer verification evidence and known gaps.

## Dependencies

- Depends on `00-architecture-foundation-backlog.md` and `01-analyzer-core-contract-backlog.md`.
- Blocks `06-en-ru-entity-packs` implementation and contributes directly to evaluation baselines.

## Acceptance Evidence

Implementation is complete when:
- pattern, checksum, context, denylist, and allowlist mechanisms all exist in one deterministic pipeline
- precedence rules are encoded and testable
- candidates emit the frozen normalized-hit contract
- identical inputs produce identical candidate outcomes
- structured JSON leaves behave the same as plain text leaves modulo field metadata

## Suggested Verification

- unit tests for pattern scoring, validation, checksum, context, denylist, and allowlist behavior
- precedence tests for conflicting candidates
- replay tests for determinism
- initial parity comparison against `Vanilla Presidio` for the supported deterministic families
