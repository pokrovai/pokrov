# Backlog: EN RU Entity Packs

Date: 2026-04-05
Spec source: `docs/superpowers/specs/presidio-rework/06-en-ru-entity-packs.md`
Status: Draft

## Summary

This backlog turns the first EN and RU entity-pack decisions into implementation scope.
Its purpose is to make deterministic-family work concrete by tying recognizer coverage, language-specific context, and default operator expectations to an explicit phase-one entity set.

## Scope

In scope:
- phase-one supported entity list
- unsupported and deferred entity list
- entity-to-recognizer-family mapping
- entity-to-language mapping
- default operator expectations by risk class and profile direction

Out of scope:
- broad country-specific long-tail coverage
- ML-first person, location, and organization parity
- full PHI and medical entity families

## Deliverables

- explicit EN and RU phase-one entity matrix
- language-specific context and list requirements for supported entities
- published mapping from entity to recognizer family
- documented exclusions and deferred entities

## Tasks

### Phase 1: Supported and unsupported matrix
- `L0601` Freeze the phase-one supported entity list for EN and RU.
- `L0602` Freeze the unsupported or deferred entity list with rationale.
- `L0603` Define the minimal entity groups required for secrets, PII, and corporate markers in phase one.

### Phase 2: Family and language mapping
- `L0604` Map each supported entity to one or more deterministic recognizer families.
- `L0605` Define EN-specific context, validation, and list requirements where needed.
- `L0606` Define RU-specific context, validation, and list requirements where needed.

### Phase 3: Operator expectations and reporting
- `L0607` Document the default operator direction per entity or risk class, aligned with the operator semantics spec.
- `L0608` Produce the coverage-reporting shape: supported list, unsupported list, entity-to-family map, entity-to-language map, and entity-to-risk-class map.
- `L0609` Ensure the entity-pack definitions are suitable inputs for evaluation-case generation and parity reporting.

### Phase 4: Verification and evidence
- `L0610` Add coverage tests or fixtures proving each supported entity maps to at least one recognizer family.
- `L0611` Add EN/RU-specific tests for language-sensitive context or list behavior in the supported families.
- `L0612` Record entity-pack verification evidence and deferred-coverage notes.

## Dependencies

- Depends on `02-deterministic-recognizers-backlog.md` and `03-operator-semantics-backlog.md`.
- Supports `07-evaluation-lab-foundation` and `08-baseline-and-dataset-inventory` by defining what must be measured in phase one.

## Acceptance Evidence

Implementation is complete when:
- the phase-one entity set is explicit
- every supported entity maps to at least one recognizer family
- EN and RU differences are documented and testable
- unsupported entities are listed rather than implied
- evaluation and parity work can consume the entity-pack definitions directly

## Suggested Verification

- coverage fixtures or mapping tests
- EN/RU context-behavior tests
- reviewable entity matrix artifact checked into documentation or config-facing references
