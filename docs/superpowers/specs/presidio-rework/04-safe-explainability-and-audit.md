# Safe Explainability And Audit

Date: 2026-04-05
Status: Draft

## Context

Presidio exposes a rich decision process, but Pokrov cannot copy that behavior directly because metadata-only safety is a core product invariant.
This specification defines the maximum explainability and audit surface that Pokrov can expose without leaking sensitive content.

## Goals

- Freeze metadata-only explain and audit schemas.
- Define the allowed provenance and reason-code surface.
- Keep explainability useful for debugging and evaluation.
- Enforce no-raw-data constraints structurally rather than by convention.

## Explain Model

Explain output may include:
- recognizer provenance ids
- evidence class
- reason codes
- confidence buckets
- validation result summary
- suppression markers
- overlap resolution markers
- policy escalation markers
- degradation markers

Explain output must not include:
- raw snippets
- raw payload fragments
- nearby context words
- original matched text
- direct regex pattern dumps if they would expose sensitive structure beyond safe reason metadata

## Audit Model

Audit output may include:
- request id or correlation id
- profile id
- mode
- final action
- family and category hit counts
- path class
- execution flags
- duration
- degradation metadata

Audit output must not include:
- payload content
- original field values
- recognizer-specific raw evidence

## Reason Code Catalog

The initial reason-code catalog must include at least:
- `pattern_match`
- `validator_accept`
- `validator_reject`
- `checksum_valid`
- `checksum_invalid`
- `context_boost`
- `context_suppress`
- `allowlist_suppressed`
- `denylist_positive`
- `overlap_won`
- `overlap_lost`
- `policy_escalated`
- `remote_degraded`

New reason codes must be added explicitly and versioned.

## Confidence Rules

- Explain output should use confidence buckets or stable score bands rather than leaking raw model internals everywhere.
- Raw numeric scores may remain in internal result contracts when needed for evaluation, but only if still metadata-safe.

## Reuse Rules

- Explain and audit outputs must be reusable by runtime flows and evaluation reports.
- Evaluation reports may aggregate explain and audit fields, but must not expand them with raw payload content.

## Acceptance Criteria

- Explain and audit schemas are metadata-only by construction.
- The reason-code catalog is explicit and regression-testable.
- Runtime, evaluation, and parity reports can reuse the same safe outputs.
- No later spec may introduce raw fragments into explain or audit without revising this contract.
