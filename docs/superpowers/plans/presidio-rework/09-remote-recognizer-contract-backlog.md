# Backlog: Remote Recognizer Contract

Date: 2026-04-05
Spec source: `docs/superpowers/specs/presidio-rework/09-remote-recognizer-contract.md`
Status: Draft

## Summary

This backlog defines the adapter path for future ML, PHI, and cloud recognizers without letting them redefine Pokrov core behavior.
Its purpose is to make remote recognizers pluggable, fail-safe, and evaluation-compatible while preserving Pokrov ownership of policy and metadata-only safety.

## Scope

In scope:
- remote recognizer adapter boundary
- normalized remote-hit contract
- timeout and degradation semantics
- fail-closed default behavior
- safe explain/audit compatibility

Out of scope:
- vendor-specific transport protocols
- concrete cloud or ML model integrations
- remote surrogate operator behavior

## Deliverables

- remote recognizer adapter interface
- normalized output mapping into the shared hit model
- explicit degradation-state handling
- safe provenance rules for remote recognizers
- evaluation compatibility rules for remote runs

## Tasks

### Phase 1: Adapter contract
- `R0901` Define the remote recognizer adapter input contract for text or JSON leaf payload, language, entity scope, timeout budget, and safe observability metadata.
- `R0902` Define the adapter output contract so remote responses can populate normalized hits without changing the shared analyzer result model.
- `R0903` Ensure remote recognizers can emit safe provenance and degradation markers without leaking raw data.

### Phase 2: Degradation and fail-safe behavior
- `R0904` Encode the degradation-state model for success, empty-valid response, timeout or transport failure, and malformed response.
- `R0905` Implement or freeze fail-closed as the default remote degradation mode.
- `R0906` Ensure remote degradation is visible in safe explain and audit outputs and can influence evaluation reports.

### Phase 3: Runtime and evaluation compatibility
- `R0907` Ensure remote hits normalize into the same hit model used by native recognizers.
- `R0908` Ensure remote recognizers never own final policy decisions.
- `R0909` Ensure evaluation and parity tooling can distinguish quality issues from degradation issues in remote runs.

### Phase 4: Verification and evidence
- `R0910` Add contract tests for remote adapter request and normalized-response handling.
- `R0911` Add degradation tests for timeout, malformed response, and fail-closed behavior.
- `R0912` Record remote-adapter verification evidence and note any deferred transport details.

## Dependencies

- Depends on `00-architecture-foundation-backlog.md`, `01-analyzer-core-contract-backlog.md`, `04-safe-explainability-and-audit-backlog.md`, and `07-evaluation-lab-foundation-backlog.md`.
- Uses evaluation and parity infrastructure defined by `07-08`.
- Should not block native deterministic-family implementation.

## Acceptance Evidence

Implementation is complete when:
- remote adapters can normalize into the shared hit model
- fail-closed degradation is explicit and testable
- safe explain and audit can represent remote degradation without leaking raw data
- evaluation reports can distinguish remote quality failures from remote availability failures

## Suggested Verification

- adapter-contract tests
- degradation and timeout tests
- parity and evaluation dry-runs using synthetic remote responses
