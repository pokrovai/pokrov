# Backlog: Safe Explainability And Audit

Date: 2026-04-05
Spec source: `docs/superpowers/specs/presidio-rework/04-safe-explainability-and-audit.md`
Status: Draft

## Summary

This backlog delivers the metadata-only explain and audit layer for the Presidio rework.
Its purpose is to preserve observability, parity reporting, and debugging value without allowing raw content or matched fragments to leak through logs, API outputs, or evaluation artifacts.

## Scope

In scope:
- safe explain schema
- safe audit schema
- reason-code catalog
- confidence-bucket policy
- provenance and degradation markers
- regression tests for no-raw-data behavior

Out of scope:
- unrestricted Presidio-style decision trace dumps
- raw snippet logging
- vendor-specific logging transports

## Deliverables

- explain-summary builders aligned with the frozen analyzer contract
- metadata-only audit summary builders
- initial reason-code catalog wired into deterministic recognizers and policy outcomes
- confidence-bucket strategy usable by runtime and evaluation outputs
- leakage-prevention tests

## Tasks

### Phase 1: Schema and builder alignment
- `E0401` Implement or refine explain-summary structures so they expose provenance, reason codes, confidence buckets, suppression markers, and degradation markers only.
- `E0402` Implement or refine audit-summary structures so they expose request identity, profile, action, counts, timing, path class, and degradation facts only.
- `E0403` Ensure explain and audit sections plug directly into the top-level analyzer result without family-specific side channels.

### Phase 2: Reason codes and confidence policy
- `E0404` Encode the initial reason-code catalog, including validation, checksum, context, allowlist, denylist, overlap, policy, and remote degradation markers.
- `E0405` Define the confidence-bucket or score-band policy used in safe explain outputs.
- `E0406` Ensure recognizer and policy stages can emit safe provenance without exposing raw evidence.

### Phase 3: Leakage prevention
- `E0407` Add regression tests proving raw snippets, matched fragments, and nearby context strings cannot appear in explain or audit outputs.
- `E0408` Add integration tests proving block, allow, transform, and degraded flows all remain metadata-only.
- `E0409` Verify that evaluation and parity reports can reuse explain and audit fields without introducing raw data.

### Phase 4: Evidence and operational readiness
- `E0410` Add structured-output or serialization tests for explain and audit sections.
- `E0411` Add at least one end-to-end runtime test covering recognizer provenance, safe explain, and safe audit together.
- `E0412` Record explain/audit verification evidence and explicitly note any remaining observability gaps.

## Dependencies

- Depends on `00-architecture-foundation-backlog.md` and `01-analyzer-core-contract-backlog.md`.
- Depends functionally on `02-deterministic-recognizers-backlog.md` because reason codes and provenance originate there.
- Supports later evaluation and remote-adapter work.

## Acceptance Evidence

Implementation is complete when:
- explain and audit outputs are metadata-only by construction
- the reason-code catalog is wired and testable
- confidence-bucket behavior is explicit
- no-raw-data leakage tests pass across allow, transform, block, and degraded flows
- evaluation and parity layers can reuse safe outputs directly

## Suggested Verification

- unit tests for explain and audit builders
- serialization tests
- security-style leakage tests
- end-to-end tests with deterministic recognizer outputs and policy outcomes
