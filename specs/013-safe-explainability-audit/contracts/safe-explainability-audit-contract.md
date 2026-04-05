# Contract: Safe Explainability and Audit

## 1. Scope

This contract freezes the safe explain and audit surface for analyzer outputs used by runtime proxy flows and evaluation/parity reporting.

In scope:

- metadata-only explain summary schema
- metadata-only audit summary schema
- reason-code catalog governance
- confidence bucket policy
- mode-based failure handling for explain/audit generation
- retention and access-control constraints for safe metadata

Out of scope:

- raw Presidio-style trace dumps
- raw snippet/context payload logging
- unrestricted internal user access to explain/audit outputs

## 2. Explain Summary Contract

### 2.1 Allowed explain fields

Explain output may include only:

- recognizer/stage provenance identifiers
- evidence class
- reason codes
- confidence bucket
- validation summary
- suppression markers
- overlap resolution markers
- policy escalation markers
- degradation markers

### 2.2 Prohibited explain content

Explain output must not include:

- raw snippets
- raw payload fragments
- nearby context words
- original matched text
- recognizer raw evidence payload

## 3. Audit Summary Contract

### 3.1 Allowed audit fields

Audit output may include only:

- request/correlation identifier
- profile identifier
- execution mode
- final action
- family/category hit counts
- path class
- execution flags
- duration metadata
- degradation metadata

### 3.2 Prohibited audit content

Audit output must not include:

- payload content
- original field values
- recognizer-specific raw evidence

## 4. Reason Code Contract

The reason-code catalog is explicit and versioned. Initial required codes:

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

Any catalog extension requires explicit versioned contract update.

## 5. Confidence Contract

Safe explain output uses stable confidence buckets/score bands. External-safe consumers must not depend on unrestricted raw scoring internals.

## 6. Failure-Mode Contract

When explain/audit generation fails:

- enforcement/runtime paths: fail closed
- explicitly non-enforcing evaluation paths: continue with degradation metadata (`fail-open-with-degradation`)

## 7. Retention and Access Contract

- safe explain/audit metadata retention is fixed at 30 days
- metadata must be deleted after retention window
- read access is limited to security/operations roles under least privilege

## 8. Consumer Guarantees

Runtime and evaluation/parity consumers may rely on:

- stable metadata-only explain and audit output shapes
- explicit reason-code and confidence semantics
- deterministic degraded/failure behavior by mode
- no raw-content leakage in serialized explain/audit artifacts
