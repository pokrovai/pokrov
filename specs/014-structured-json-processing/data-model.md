# Data Model: Structured JSON Processing

## 1. StructuredPayload

- **Purpose**: Represents incoming nested JSON object/array payload processed inline.
- **Core properties**:
  - preserves original object/array topology
  - may contain mixed leaf types
  - source for traversal and transformation
- **Validation rules**:
  - payload must remain valid JSON before and after processing
  - non-string leaves must not be transformed directly

## 2. StringLeafContext

- **Purpose**: Captures processing context for each string leaf.
- **Fields**:
  - `json_pointer` (internal traversal reference)
  - `logical_field_alias` (optional)
  - `path_class` (safe category for reporting)
  - `parent_context` (object/array relation metadata)
  - `profile_id` and `language`
- **Validation rules**:
  - context must be deterministic for identical payload + config
  - reporting outputs must not expose `json_pointer`

## 3. PathBindingRule

- **Purpose**: Defines path-aware detection/transform behavior.
- **Fields**:
  - `binding_target` (exact pointer, alias, or subtree)
  - `recognizer_family_includes`
  - `recognizer_family_excludes`
  - `default_operator`
  - `high_risk_override`
  - `precedence_level`
- **Validation rules**:
  - precedence order is fixed and total
  - conflicting rules resolve deterministically by precedence

## 4. StructuredHit

- **Purpose**: Normalized detection result for a string leaf under path context.
- **Fields**:
  - shared hit metadata (family/category/confidence bucket)
  - bound path context identifiers
  - resolved policy action candidate
- **Validation rules**:
  - contract compatibility with existing plain-text hit model
  - no raw payload snippets in safe outputs

## 5. StructuredTransformResult

- **Purpose**: Holds transformed payload plus metadata-only processing summary.
- **Fields**:
  - `sanitized_payload`
  - `leaf_processed_count`
  - `leaf_changed_count`
  - `decision_counts`
  - `degradation_flags`
- **Validation rules**:
  - shape of `sanitized_payload` must match input topology
  - only string leaves may differ from input values

## 6. StructuredSummaryRecord

- **Purpose**: Safe explain/audit summary output for structured mode.
- **Fields**:
  - `request_id`
  - `profile_id`
  - `path_class_counts`
  - `family_hit_counts`
  - `decision_counts`
  - `duration_ms`
  - `failure_mode` (if any)
- **Validation rules**:
  - MUST NOT include raw values or exact JSON pointer
  - MUST remain metadata-only and deterministic for identical input+config

## 7. SizeAndFailurePolicy

- **Purpose**: Encodes behavior for oversize payload and high-risk failures.
- **Fields**:
  - `sla_size_limit_bytes` (= 1 MB)
  - `sla_mode` (strict within limit)
  - `oversize_mode` (best-effort)
  - `high_risk_failure_action` (`block` / fail-closed)
- **Validation rules**:
  - payload <=1 MB follows SLA performance goals
  - payload >1 MB may exceed SLA but must preserve security/privacy invariants
  - high-risk processing failure must produce block decision

## 8. State Transitions

```text
payload_received
  -> traversal_started
  -> leaf_context_bound
  -> detection_and_policy_resolution
  -> transform_applied | block_decision
  -> summary_built (metadata-only)
  -> response_emitted
```

For payload >1 MB, transition path remains identical, but SLA classification is `best_effort`.
