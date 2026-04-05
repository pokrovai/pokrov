# Data Model: Deterministic Recognizers

## 1. DeterministicRecognizerProfile

- **Purpose**: Compiled recognizer state bound to one active sanitization profile and consumed by `SanitizationEngine`.
- **Source**: Derived from validated `pokrov-config` sanitization profile configuration at startup.
- **Fields**:
  - `profile_id`: canonical profile identifier
  - `families`: ordered collection of compiled recognizer families
  - `allowlists`: exact normalized suppression entries by entity scope
  - `denylists`: explicit positive-match entries by entity scope
  - `language_context`: EN/RU lexical dictionaries or family-specific overrides
  - `default_context_policy`: default negative/positive context behavior for the profile
- **Validation rules**:
  - profile must compile fully before readiness is reported
  - recognizer identifiers must be unique within the profile
  - allowlist and denylist entries must declare a valid entity scope

## 2. DeterministicRecognizerDefinition

- **Purpose**: Configured family definition for one deterministic recognizer.
- **Fields**:
  - `recognizer_id`: stable identifier used in candidates and explain metadata
  - `entity_category`: normalized category emitted by the family
  - `family_kind`: `pattern`, `validation`, `contextual`, `denylist`, or `allowlist`
  - `family_priority`: deterministic tie-break priority applied after final score
  - `enabled`: startup activation flag
  - `patterns`: zero or more ordered pattern definitions
  - `validator`: optional validator definition
  - `context_policy`: optional positive/negative context policy
  - `scope`: optional language, tenant, or profile restrictions
- **Relationships**:
  - belongs to one `DeterministicRecognizerProfile`
  - may reference one `ValidatorDefinition`
  - may reference one `ContextPolicy`

## 3. PatternDefinition

- **Purpose**: One normalized matching rule inside a pattern-capable recognizer.
- **Fields**:
  - `pattern_id`: stable local identifier
  - `expression`: compiled deterministic matcher source
  - `base_score`: initial confidence before validation and context
  - `pattern_priority`: ordering within the recognizer
  - `normalization_mode`: optional normalization strategy applied before validation
- **Validation rules**:
  - pattern must compile at startup
  - base score must fit configured deterministic scoring bounds
  - normalization mode must not change location semantics

## 4. ValidatorDefinition

- **Purpose**: Post-match deterministic validator or invalidator applied to normalized matches.
- **Fields**:
  - `validator_id`: stable identifier
  - `validator_kind`: checksum, structural, or deterministic post-match rule
  - `success_adjustment`: score delta or status used when validation succeeds
  - `failure_behavior`: default reject, or family-documented downgraded retention
  - `reason_codes`: metadata-safe explanation codes for pass/fail
- **Validation rules**:
  - failure behavior defaults to reject unless the recognizer family explicitly overrides it
  - validator output must remain metadata-only

## 5. ContextPolicy

- **Purpose**: Lexical evidence model used to boost or downscore a candidate.
- **Fields**:
  - `positive_terms`: positive lexical hints
  - `negative_terms`: negative lexical hints
  - `window`: bounded context distance around a candidate
  - `language_scope`: EN, RU, or both
  - `negative_default`: downscore by default
  - `suppression_opt_in`: explicit flag for families allowed to suppress on negative context
- **Validation rules**:
  - context terms must be scoped to supported languages
  - context processing must not leak source text into explanation or audit records

## 6. ListControlEntry

- **Purpose**: Exact-match allowlist or denylist configuration.
- **Fields**:
  - `entry_id`: stable identifier
  - `list_kind`: allowlist or denylist
  - `normalized_value`: exact normalized comparison value
  - `entity_scope`: allowed entity category or family scope
  - `profile_scope`: profile binding
  - `tenant_scope`: optional tenant binding
  - `language_scope`: optional language binding
  - `reason_code`: metadata-safe explanation code
- **Validation rules**:
  - allowlist entries suppress only exact normalized matches
  - denylist entries emit first-class candidates and do not bypass overlap resolution
  - cross-tenant leakage is forbidden

## 7. DeterministicCandidate

- **Purpose**: Shared candidate output emitted by any deterministic family before final overlap resolution.
- **Fields**:
  - `recognizer_id`
  - `entity_category`
  - `location_kind`
  - `json_pointer` or logical field path metadata
  - `start` and `end`
  - `normalized_score`
  - `evidence_class`
  - `validation_status`
  - `reason_codes`
  - `suppression_status`
  - `suppression_reason`
  - `family_priority`
  - `language`
- **Lifecycle**:
  - `matched` -> `normalized` -> `validated` -> `context_adjusted` -> `suppressed` or `eligible_for_overlap`
- **Validation rules**:
  - candidate must never contain raw matched substrings
  - equivalent plain-text and structured-field inputs must produce identical candidate semantics except for field-context metadata

## 8. ResolvedDeterministicHit

- **Purpose**: Post-analysis hit that survives deterministic precedence and feeds policy, explain, and transform planning.
- **Fields**:
  - `winning_candidate_id`
  - `resolved_location`
  - `effective_score`
  - `effective_action_hint`
  - `suppressed_candidate_ids`
  - `precedence_trace`
  - `final_validation_status`
- **Resolution rules**:
  - highest final score wins first
  - ties are broken by family priority
  - remaining ties use stable recognizer ordering
- **Relationships**:
  - derived from one or more `DeterministicCandidate` records
  - consumed by policy resolution, explain, audit, and transform planning

## 9. State Transitions

```text
configured
  -> compiled
  -> matched
  -> normalized
  -> validated
  -> context_adjusted
  -> suppressed | eligible_for_overlap
  -> resolved
  -> policy_mapped
  -> transformed_or_blocked
```

State transitions must remain deterministic for identical input, profile, language, and path class.
