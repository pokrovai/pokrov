# EN RU Entity Packs

Date: 2026-04-05
Status: Draft

## Context

Pokrov should not start with unlimited Presidio coverage.
It needs an explicit EN and RU entity-pack scope for early phases so recognizer work, operator defaults, evaluation cases, and parity reports all target the same entity set.

## Goals

- Define the priority entity families for EN and RU.
- Map each planned entity to recognizer families.
- Define default operator expectations by risk class and profile.
- Record explicit exclusions rather than leaving them implicit.

## Non-Goals

- Global country-specific long-tail coverage.
- Full medical and PHI ontology.
- Deep ML-only entity families in the first native phases.

## Phase 1 Priority Entities

### Shared high-priority entities
- email
- phone number
- card-like number
- IBAN
- IP address
- URL or domain
- secret token and API key shapes
- corporate markers

### EN-first entities
- common person-name adjacency contexts where deterministic support is realistic
- address-like high-risk patterns only if false-positive rate is acceptable

### RU-first entities
- localized phone and identifier context dictionaries
- localized corporate marker vocabularies
- RU allowlist and denylist support

## Entity To Recognizer Mapping

- email -> pattern + validation + allowlist suppression
- phone -> pattern + context
- card-like number -> pattern + checksum + context
- IBAN -> pattern + checksum + context
- IP -> pattern + validation
- URL/domain -> pattern + validation + allowlist suppression
- secret token -> pattern + secret-family validators where available
- corporate marker -> pattern + denylist + context

## Default Operator Expectations

The initial default direction should be:
- secrets -> `block`, `redact`, or strict-safe profile behavior
- high-confidence PII -> `redact` or `mask` by profile
- allowlisted or low-risk corporate markers -> `keep` or profile-specific safe behavior
- custom and unresolved risk classes -> conservative profile-controlled defaults

This document records the direction only. Final operator semantics are defined in the operator spec.

## Explicit Exclusions For Early Phases

The following remain out of first-phase native scope unless separately approved:
- broad person-name detection parity through ML NER
- location and organization NER families requiring heavy NLP
- medical disorders, medications, and full PHI families
- global national identifier coverage beyond selected deterministic validators

## Coverage Reporting Requirements

The entity pack must be published as:
- supported entities list
- unsupported entities list
- entity-to-family mapping
- entity-to-language mapping
- entity-to-default-risk-class mapping

## Acceptance Criteria

- The Phase 1 entity set is explicit.
- Every supported entity maps to at least one recognizer family.
- EN and RU differences are documented rather than hidden in code.
- Unsupported entities are listed with rationale.
