# Research: Safe Explainability and Audit

## Decision 1: Freeze strict metadata-only explain and audit schemas

- **Decision**: Define explicit allowed-field contracts for explain and audit summaries and treat any raw snippet/context/payload field as a regression.
- **Rationale**: Metadata-only safety is a non-negotiable product invariant; explicit field allowlists are more reliable than convention-based safeguards.
- **Alternatives considered**:
  - Best-effort filtering at serialization time: rejected because bypasses can still leak raw fragments.
  - Family-specific explain payloads: rejected because they increase drift and leakage surface.

## Decision 2: Encode and version reason-code catalog as a governed contract

- **Decision**: Maintain a versioned reason-code catalog with explicit initial entries (`pattern_match`, `validator_accept`, `validator_reject`, `checksum_valid`, `checksum_invalid`, `context_boost`, `context_suppress`, `allowlist_suppressed`, `denylist_positive`, `overlap_won`, `overlap_lost`, `policy_escalated`, `remote_degraded`).
- **Rationale**: Reason codes are consumed by runtime/evaluation diagnostics; versioned governance avoids hidden semantics changes.
- **Alternatives considered**:
  - Unversioned free-form reason text: rejected because it breaks deterministic consumers and testability.
  - Per-recognizer private reason lists: rejected because it fragments observability.

## Decision 3: Use confidence buckets for safe external explainability

- **Decision**: Expose stable confidence buckets/score bands in safe outputs, and keep any internal raw scoring constrained to metadata-safe internal contracts.
- **Rationale**: Buckets reduce leakage risk and provide stable semantics for operators and reports.
- **Alternatives considered**:
  - Expose raw numeric model scoring everywhere: rejected because it can leak internals and destabilize consumers.
  - Omit confidence entirely: rejected because it weakens debugging and parity analysis.

## Decision 4: Enforce mode-based failure policy for explain/audit generation errors

- **Decision**: Fail closed in enforcement/runtime paths; allow fail open only in explicitly non-enforcing evaluation flows, with degradation metadata.
- **Rationale**: This preserves security posture for production decisions while keeping non-enforcing analysis available.
- **Alternatives considered**:
  - Always fail closed: rejected because it unnecessarily blocks non-enforcing evaluation workflows.
  - Always fail open: rejected because it weakens enforcement safety.

## Decision 5: Reuse one safe explain/audit contract across runtime and evaluation consumers

- **Decision**: Runtime flows and evaluation/parity reporting consume the same safe explain/audit output structures.
- **Rationale**: Shared contracts prevent drift and avoid creating unsafe side channels for observability.
- **Alternatives considered**:
  - Separate evaluation-only explain payload: rejected because duplicate models increase maintenance and leakage risk.
  - Runtime-only explain/audit with evaluation re-derivation: rejected because it weakens parity consistency.

## Decision 6: Fix retention and access policy as first-class acceptance constraints

- **Decision**: Retain safe explain/audit metadata for 30 days and enforce read access for security/operations roles only under least privilege.
- **Rationale**: Defines clear operational boundaries for investigations without expanding sensitive-data risk.
- **Alternatives considered**:
  - Short retention (7 days): rejected because it may be insufficient for incident timelines.
  - Long retention (90+ days): rejected due to higher accumulated risk and limited extra value for v1.
  - Broad internal access: rejected due to least-privilege violations.
