# Data Model: Operator Semantics Freeze

## 1. OperatorPolicyBinding

- **Purpose**: Maps profile/entity/category context to one of the supported operators.
- **Fields**:
  - `profile_id`
  - `entity_category`
  - `operator_kind` (`replace`, `redact`, `mask`, `hash`, `keep`)
  - `operator_params` (operator-specific safe config)
  - `priority_hint` (optional stable tie helper if required by policy)
- **Validation rules**:
  - operator_kind must belong to the supported core set
  - bindings must be deterministic for identical profile+entity input

## 2. ResolvedTransformHit

- **Purpose**: Final transform-eligible hit produced after overlap suppression and policy resolution.
- **Fields**:
  - `hit_id`
  - `location` (path/span metadata)
  - `resolved_action` (`block` or transform action)
  - `operator_kind`
  - `operator_params`
  - `precedence_trace`
  - `suppressed_hit_ids`
- **Lifecycle**:
  - `candidate` -> `overlap_resolved` -> `policy_resolved` -> `transform_ready`
- **Validation rules**:
  - suppressed hits are never reintroduced
  - transform-ready hits include enough metadata for deterministic ordering

## 3. TransformPlan

- **Purpose**: Deterministic execution plan built from resolved hits for one payload.
- **Fields**:
  - `request_id`
  - `profile_id`
  - `plan_items` (ordered `ResolvedTransformHit` list)
  - `blocked` (terminal flag)
  - `block_reason` (metadata-only, e.g., `unsupported_operator`)
- **Rules**:
  - identical resolved-hit sets produce identical `plan_items` ordering
  - when `blocked=true`, no sanitized payload is emitted downstream

## 4. OperatorApplicationResult

- **Purpose**: Per-hit operator execution record used for explain/audit/evaluation summaries.
- **Fields**:
  - `hit_id`
  - `operator_kind`
  - `status` (`applied`, `skipped`, `blocked`)
  - `reason_code`
  - `output_fingerprint` (metadata-safe, optional)
- **Validation rules**:
  - must not include raw source spans or transformed payload fragments
  - `keep` must always remain explicit in result metadata

## 5. TransformOutcome

- **Purpose**: Final per-request outcome consumed by proxy runtime and evaluation paths.
- **Variants**:
  - `BlockedOutcome`
  - `TransformedOutcome`

### 5.1 BlockedOutcome

- **Fields**:
  - `request_id`
  - `final_action` = `block`
  - `reason_code`
  - `operator_summary`
  - `explain_summary`
  - `audit_summary`
- **Rule**:
  - contains no sanitized or raw payload body

### 5.2 TransformedOutcome

- **Fields**:
  - `request_id`
  - `final_action` = `transform`
  - `sanitized_payload`
  - `operator_results`
  - `explain_summary`
  - `audit_summary`
- **Rules**:
  - `sanitized_payload` remains valid JSON for structured input
  - only string leaves are mutated directly

## 6. JsonTraversalInvariant

- **Purpose**: Captures non-negotiable traversal guarantees for structured inputs.
- **Fields**:
  - `path_class` (object key path/array index path)
  - `leaf_kind` (string/non-string)
  - `mutation_allowed` (bool)
- **Invariant**:
  - `mutation_allowed=true` only when `leaf_kind=string`

## 7. Operator Semantics Constraints

- `replace`: output is configured stable replacement value.
- `redact`: output uses fixed safe redaction strategy.
- `mask`: output preserves configured visible prefix/suffix policy deterministically.
- `hash`: output is one-way and deterministic within the same profile.
- `keep`: output preserves content and is explicitly marked in metadata-only explain/audit.

## 8. State Transitions

```text
resolved_hits
  -> transform_plan_built
  -> blocked | applying_operators
  -> transformed
  -> outcome_emitted
  -> explain_audit_summarized
```

State transitions must be deterministic for identical payload, profile, and resolved-hit input.
