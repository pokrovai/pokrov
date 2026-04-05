# Data Model: Architecture Foundation For Presidio Rework

## PipelineStageBoundary

- **Purpose**: Defines the approved ownership line between normalization, recognition, analysis, policy, transformation, explain, and audit.
- **Fields**:
  - `stage_id: StageId`
  - `allowed_inputs: Vec<StageArtifact>`
  - `allowed_outputs: Vec<StageArtifact>`
  - `owns_policy_decision: bool`
  - `may_mutate_payload: bool`
  - `forbidden_responsibilities: Vec<String>`
- **Validation rules**:
  - only the policy stage may own final action selection;
  - only the transform stage may mutate payload content;
  - explain and audit stages must never accept raw payload fragments as inputs.

## NormalizedHitRecord

- **Purpose**: Common candidate-detection record produced by native and remote recognizers.
- **Fields**:
  - `entity_type_or_category: String`
  - `location_kind: String`
  - `json_pointer_or_logical_path: String`
  - `start: Option<usize>`
  - `end: Option<usize>`
  - `score: String`
  - `recognizer_id: String`
  - `evidence_class: String`
  - `reason_codes: Vec<String>`
  - `validation_status: String`
  - `suppressed: bool`
  - `language: Option<String>`
- **Validation rules**:
  - native and remote recognizers must emit the same top-level shape;
  - location data must be stable enough for downstream policy and evaluation reuse;
  - this record must not contain raw matched text.

## ResolvedHitRecord

- **Purpose**: Post-analysis record after validation, suppression, and overlap handling.
- **Fields**:
  - `winning_identity: String`
  - `surviving_location: String`
  - `effective_score: String`
  - `effective_action_hint: Option<String>`
  - `suppressed_competitors: Vec<String>`
  - `precedence_trace: Vec<String>`
- **Validation rules**:
  - one surviving result exists per approved span or field outcome;
  - precedence trace is metadata-only and deterministic;
  - local suppression must not bypass policy ownership.

## TransformPlanRecord

- **Purpose**: Connects policy outcome to ordered transformation behavior.
- **Fields**:
  - `final_action: String`
  - `per_hit_operator_mapping: Vec<String>`
  - `transform_order: Vec<String>`
  - `mode: String`
- **Validation rules**:
  - block versus transform mode must be explicit;
  - order must be deterministic for the same resolved-hit input;
  - transform planning does not own detection or policy recomputation.

## TransformResultRecord

- **Purpose**: Final blocked or transformed outcome after plan application.
- **Fields**:
  - `final_action: String`
  - `blocked: bool`
  - `sanitized_payload_present: bool`
  - `transformed_fields_count: u32`
  - `safe_transform_metadata: Vec<String>`
- **Validation rules**:
  - non-blocking results preserve valid JSON structure;
  - safe metadata must remain reusable by evaluation without carrying payload fragments.

## ExplainSummaryRecord

- **Purpose**: Metadata-only explanation payload for consumers and verification.
- **Fields**:
  - `final_action: String`
  - `family_counts: Vec<String>`
  - `entity_counts: Vec<String>`
  - `reason_codes: Vec<String>`
  - `confidence_buckets: Vec<String>`
  - `provenance_summary: Vec<String>`
  - `degradation_markers: Vec<String>`
- **Validation rules**:
  - no raw payload, snippets, matched values, or nearby source text;
  - must remain serializable for API and evaluation-safe outputs.

## AuditSummaryRecord

- **Purpose**: Metadata-only audit record safe for logs and operational evidence.
- **Fields**:
  - `request_id: String`
  - `profile_id: String`
  - `mode: String`
  - `final_action: String`
  - `category_counts: Vec<String>`
  - `family_counts: Vec<String>`
  - `path_class: String`
  - `duration_ms: u64`
  - `degradation_metadata: Vec<String>`
- **Validation rules**:
  - audit output must stay metadata-only by construction;
  - count and timing fields must be sufficient for metrics and later evidence generation.

## ExtensionPointContract

- **Purpose**: Bounded integration point for later family or tooling work.
- **Fields**:
  - `kind: NativeRecognizer | RemoteRecognizer | StructuredProcessor | EvaluationRunner | BaselineRunner`
  - `accepted_inputs: Vec<StageArtifact>`
  - `produced_outputs: Vec<StageArtifact>`
  - `policy_ownership_allowed: bool`
  - `payload_mutation_allowed: bool`
- **Validation rules**:
  - recognizers never own final policy action;
  - evaluation and baseline runners consume shared result families rather than custom private models;
  - structured processors extend field semantics without replacing the shared hit model.

## EvaluationArtifactBoundary

- **Purpose**: Distinguishes repo-safe evaluation fixtures from restricted external references.
- **Fields**:
  - `artifact_class: RepoSafeFixture | RestrictedExternalReference`
  - `commit_allowed: bool`
  - `access_metadata_required: bool`
  - `redistribution_allowed: bool`
- **Validation rules**:
  - restricted datasets are never committed;
  - external references must carry access and redistribution metadata;
  - foundation scope ends at placement and safe-handling boundaries.

## Relationships

- `PipelineStageBoundary` governs all transitions between shared contract families.
- `NormalizedHitRecord` is produced by recognizer extension points and consumed by analysis.
- `ResolvedHitRecord` is produced by analysis and consumed by policy, explain, and transform planning.
- `TransformPlanRecord` is produced by policy and consumed by transformation.
- `TransformResultRecord`, `ExplainSummaryRecord`, and `AuditSummaryRecord` are sibling outputs of the post-policy flow.
- `EvaluationArtifactBoundary` constrains where verification data may live, but it does not define dataset governance or retention policy.
