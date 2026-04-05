# Data Model: Analyzer Core Contract For Presidio Rework

## AnalyzerRequestContract

- **Purpose**: Defines the canonical analyzer input shared by runtime adapters and evaluation consumers.
- **Fields**:
  - `request_id: String`
  - `correlation_id: Option<String>`
  - `profile_id: String`
  - `mode: EvaluationMode`
  - `payload: serde_json::Value`
  - `path_class: PathClass`
  - `effective_language: String`
  - `entity_scope_filters: Vec<String>`
  - `recognizer_family_filters: Vec<String>`
  - `allowlist_additions: Vec<String>`
- **Validation rules**:
  - `request_id` must always be present and non-empty;
  - `correlation_id` is optional and must be metadata-only;
  - `effective_language` is always present in the shared request contract;
  - adapter layers may inject a deterministic configured default for `effective_language` when the caller does not provide one explicitly;
  - optional filters default to empty collections rather than consumer-specific request variants;
  - payload remains raw input only inside the analyzer execution boundary and must not leak into metadata-only outputs.

## AnalyzerResultContract

- **Purpose**: Defines the shared successful analyzer outcome reused by runtime and evaluation consumers.
- **Fields**:
  - `request_id: String`
  - `profile_id: String`
  - `mode: EvaluationMode`
  - `path_class: PathClass`
  - `decision: DecisionSection`
  - `transform: TransformSection`
  - `explain: ExplainSection`
  - `audit: AuditSection`
  - `executed: ExecutedSection`
  - `degraded: DegradedSection`
- **Validation rules**:
  - all successful analyzer outcomes, including `block`, use the same top-level section layout;
  - degraded outcomes remain successful analyzer results when a safe policy outcome exists;
  - the contract must be serializable for adapter reuse;
  - no consumer may need a private top-level result family to interpret block or dry-run outcomes.

## DecisionSection

- **Purpose**: Captures the deterministic policy outcome and resolved location metadata.
- **Fields**:
  - `final_action: PolicyAction`
  - `total_hit_count: u32`
  - `counts_by_category: BTreeMap<String, u32>`
  - `counts_by_family: BTreeMap<String, u32>`
  - `resolved_locations: Vec<ResolvedLocationRecord>`
  - `replay_identity: String`
- **Validation rules**:
  - identical input, profile, mode, effective language, and recognizer set must yield the same `replay_identity`;
  - resolved locations must be ordered deterministically;
  - raw matched values, excerpts, and debug-only objects are forbidden.

## ResolvedLocationRecord

- **Purpose**: Represents the metadata-safe location of one surviving decision outcome.
- **Fields**:
  - `location_kind: TextSpan | JsonField | LogicalField`
  - `json_pointer: Option<String>`
  - `logical_field_path: Option<String>`
  - `start: Option<usize>`
  - `end: Option<usize>`
  - `category: String`
  - `effective_action: PolicyAction`
- **Validation rules**:
  - text inputs may use span offsets;
  - structured inputs may use `json_pointer` and logical-field metadata;
  - one record family must represent both text and structured locations without consumer-local variants;
  - location records must stay metadata-only and never embed raw leaf values.

## TransformSection

- **Purpose**: Carries the analyzer mutation outcome for successful execution.
- **Fields**:
  - `final_action: PolicyAction`
  - `blocked: bool`
  - `sanitized_payload: Option<serde_json::Value>`
  - `transformed_fields_count: u32`
  - `transform_metadata: Vec<String>`
- **Validation rules**:
  - `blocked=true` implies `sanitized_payload=None`;
  - non-block outcomes preserve JSON validity;
  - sanitized payload may exist only in the transform section and must not be duplicated into explain, audit, executed, or degraded metadata.

## ExplainSection

- **Purpose**: Metadata-only explanation of why the analyzer outcome occurred.
- **Fields**:
  - `profile_id: String`
  - `mode: EvaluationMode`
  - `final_action: PolicyAction`
  - `category_counts: Vec<String>`
  - `family_counts: BTreeMap<String, u32>`
  - `entity_counts: BTreeMap<String, u32>`
  - `reason_codes: Vec<String>`
  - `confidence_buckets: Vec<String>`
  - `provenance_summary: Vec<String>`
  - `degradation_markers: Vec<String>`
- **Validation rules**:
  - must remain metadata-only;
  - must not include raw payload text, matched substrings, or nearby source snippets.

## AuditSection

- **Purpose**: Metadata-only audit record safe for logs, metrics, and verification artifacts.
- **Fields**:
  - `request_id: String`
  - `profile_id: String`
  - `mode: EvaluationMode`
  - `final_action: PolicyAction`
  - `total_hit_count: u32`
  - `counts_by_category: BTreeMap<String, u32>`
  - `counts_by_family: BTreeMap<String, u32>`
  - `path_class: PathClass`
  - `duration_ms: u64`
  - `degradation_metadata: Vec<String>`
- **Validation rules**:
  - audit remains metadata-only by construction;
  - it must stay sufficient for runtime observability and consumer-safe evidence.

## ExecutedSection

- **Purpose**: States what analyzer work actually ran for the request.
- **Fields**:
  - `execution_enabled: bool`
  - `stages_completed: Vec<String>`
  - `recognizer_families_executed: Vec<String>`
  - `transform_applied: bool`
- **Validation rules**:
  - dry-run and enforce modes must be distinguishable without inspecting transport-specific behavior;
  - execution details remain metadata-only.

## DegradedSection

- **Purpose**: States whether analyzer behavior degraded and how consumers should interpret that state.
- **Fields**:
  - `is_degraded: bool`
  - `reasons: Vec<String>`
  - `fail_closed_applied: bool`
  - `missing_execution_paths: Vec<String>`
- **Validation rules**:
  - non-degraded results use explicit empty/default metadata rather than omit the section;
  - degraded outcomes remain successful analyzer results rather than analyzer errors when a safe policy outcome exists;
  - `fail_closed_applied=true` is required when mandatory analyzer evidence or execution paths are unavailable;
  - degraded state must remain safe for audit, explain, routing, and evaluation reuse.

## AnalyzerError

- **Purpose**: Represents true analyzer failures that are not policy outcomes.
- **Fields**:
  - `kind: InvalidInput | InvalidProfile | RuntimeFailure`
  - `request_id: String`
  - `profile_id: Option<String>`
  - `stage: Option<String>`
  - `safe_message: String`
- **Validation rules**:
  - policy blocks must never use this model;
  - degraded safe outcomes must never use this model;
  - error metadata must be sufficient for operational handling without exposing raw payload content.

## Relationships

- `AnalyzerRequestContract` feeds one analyzer invocation and always carries one effective language value.
- `AnalyzerResultContract` is emitted for every successful invocation, including policy-block and degraded fail-closed outcomes.
- `DecisionSection` owns the replay identity and unified resolved locations used by `TransformSection`, `ExplainSection`, and `AuditSection`.
- `ExecutedSection` and `DegradedSection` describe runtime behavior orthogonally to policy outcome.
- `AnalyzerError` is the only failure-path contract and is mutually exclusive with `AnalyzerResultContract`.
