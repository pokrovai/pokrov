# Data Model: Safe Explainability and Audit

## 1. ExplainSummary

- **Purpose**: Metadata-only explanation of analyzer decisions for runtime and evaluation consumers.
- **Fields**:
  - `request_id`
  - `provenance_ids` (recognizer and stage provenance)
  - `evidence_class`
  - `reason_codes` (catalog references only)
  - `confidence_bucket`
  - `validation_summary`
  - `suppression_markers`
  - `overlap_markers`
  - `policy_escalation_markers`
  - `degradation_markers`
- **Validation rules**:
  - must not include raw payload fragments, matched snippets, or nearby context words
  - all reason codes must exist in the catalog version in use
  - confidence output must use stable buckets/score bands only

## 2. AuditSummary

- **Purpose**: Metadata-only execution/audit record for policy and operational tracing.
- **Fields**:
  - `request_id` (or correlation identifier)
  - `profile_id`
  - `mode`
  - `final_action`
  - `family_hit_counts`
  - `category_hit_counts`
  - `path_class`
  - `execution_flags`
  - `duration_ms`
  - `degradation_metadata`
- **Validation rules**:
  - must not include payload content or recognizer raw evidence
  - counts must remain deterministic for identical input+config
  - duration and flags remain metadata-only

## 3. ReasonCodeCatalog

- **Purpose**: Versioned dictionary of allowable reason-code signals used in explain/audit outputs.
- **Fields**:
  - `catalog_version`
  - `entries` (code, meaning, emitting stage)
- **Required initial entries**:
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
- **Validation rules**:
  - new codes require explicit catalog update/versioning
  - removed or renamed codes require coordinated consumer updates

## 4. ConfidenceBucketPolicy

- **Purpose**: Stable mapping from internal confidence signals to safe external buckets.
- **Fields**:
  - `policy_version`
  - `bucket_definitions`
  - `mapping_rules`
- **Validation rules**:
  - bucket mapping must be deterministic for identical inputs
  - bucket labels must be stable across runtime and evaluation consumers

## 5. ExplainAuditFailurePolicy

- **Purpose**: Encodes mode-based behavior when explain/audit generation fails.
- **Fields**:
  - `mode`
  - `failure_action` (`fail_closed` or `fail_open_with_degradation`)
  - `degradation_reason_code`
- **Rules**:
  - enforcement/runtime modes: `fail_closed`
  - explicitly non-enforcing evaluation modes: `fail_open_with_degradation`

## 6. ExplainAuditRetentionPolicy

- **Purpose**: Defines operational retention boundary for metadata artifacts.
- **Fields**:
  - `retention_days` (=30)
  - `deletion_schedule`
  - `policy_scope`
- **Rules**:
  - metadata is retained for 30 days
  - metadata is deleted after policy window

## 7. ExplainAuditAccessPolicy

- **Purpose**: Defines least-privilege read access for safe explain/audit outputs.
- **Fields**:
  - `allowed_roles`
  - `access_scope`
  - `enforcement_point`
- **Rules**:
  - only security/operations roles can retrieve explain/audit outputs
  - unauthorized access attempts are denied and auditable

## 8. State Transitions

```text
analysis_completed
  -> explain_audit_building
  -> explain_audit_ready | explain_audit_failure
  -> runtime_enforcement_fail_closed | evaluation_degraded_continue
  -> metadata_published
  -> retention_window_active
  -> retention_expired_deleted
```

State transitions must remain deterministic for identical input payload, policy profile, and analyzer mode.
