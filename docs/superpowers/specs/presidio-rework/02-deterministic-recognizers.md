# Deterministic Recognizers

Date: 2026-04-05
Status: Draft

## Context

Deterministic recognizers are the first native Presidio-like families Pokrov should implement because they are predictable, latency-friendly, and compatible with inline proxy flows.
This specification defines how pattern, checksum, context, denylist, and allowlist mechanisms combine into one deterministic family block.

## Goals

- Define all deterministic recognizer families and their interaction rules.
- Fix the precedence between pattern matching, validation, context scoring, denylist positives, and allowlist suppression.
- Make deterministic behavior reproducible across text and structured JSON leaves.

## Non-Goals

- Deep NLP or ML recognizers.
- Remote recognizer service behavior.
- Final entity catalog for EN and RU packs.

## Families

### Pattern recognizers
Required capabilities:
- multiple patterns per entity
- base score per pattern
- priority per pattern
- optional normalization before validation
- optional validator and invalidator hooks
- config-driven loading

### Checksum and validation recognizers
Required capabilities:
- deterministic post-match validators
- explicit validation status on normalized hits
- score boost or rejection based on validation result

### Context-aware recognizers
Required capabilities:
- positive context windows
- negative context windows
- recognizer-specific context dictionaries
- EN and RU support
- lexical context only in core

### Denylists
Required capabilities:
- explicit high-confidence positives from configured values
- profile, tenant, and language scoping

### Allowlists
Required capabilities:
- suppression of false positives before final overlap resolution
- entity-specific and profile-specific allowlists
- explicit suppression reason codes

## Scoring Pipeline

The deterministic pipeline is frozen as:
`pattern match -> normalization -> validator/checksum -> context scoring -> allowlist suppression -> overlap resolution`

Required effects:
- invalid checksum may reject a candidate completely or downscore it deterministically
- positive context may boost but not override allowlist suppression
- negative context may suppress or downscore based on family rules
- denylist positives enter overlap resolution as normal candidates with explicit provenance

## Precedence Rules

- allowlist suppression takes precedence over context boost for the same entity family
- checksum validation takes precedence over pattern-only confidence for checksum-capable families
- denylist positives do not bypass overlap resolution or policy evaluation
- same-span collisions are broken deterministically by explicit ordering rules

## Shared Output Requirements

Each deterministic recognizer candidate must emit:
- normalized entity or category
- location
- score
- recognizer id
- evidence class
- validation status
- reason codes
- suppression status when applicable

## Structured JSON Compatibility

- Deterministic recognizers must operate on normalized string leaves extracted from JSON.
- Field path information must be preserved so later field-aware policies can act on results.
- Behavior must not change based solely on whether the text came from plain text input or a JSON leaf.

## Acceptance Criteria

- The same input always yields the same candidate scores and suppression outcomes.
- Precedence between pattern, checksum, context, denylist, and allowlist logic is explicit.
- Deterministic recognizers emit only normalized hits.
- Shared behavior works on text leaves and structured JSON leaves.
