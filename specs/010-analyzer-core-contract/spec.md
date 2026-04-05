# Feature Specification: Analyzer Core Contract For Presidio Rework

**Feature Branch**: `010-analyzer-core-contract`  
**Created**: 2026-04-05  
**Status**: Draft  
**Input**: User-provided source documents: `docs/superpowers/specs/presidio-rework/01-analyzer-core-contract.md docs/superpowers/plans/presidio-rework/01-analyzer-core-contract-backlog.md`

## Clarifications

### Session 2026-04-05

- Q: Should this feature freeze only a written analyzer contract, or also require one shared typed contract that downstream consumers can depend on directly? -> A: It must require one shared typed analyzer contract for downstream runtime and evaluation consumers.
- Q: Is documentation-level compatibility enough, or must the feature require executable proof that consumers reuse one result shape without policy-block special casing? -> A: At least one executable compatibility proof is required.
- Q: Does this feature define a new external wire API contract for adapters? -> A: No. The feature freezes the shared analyzer contract and allows serialized reuse, but it does not promise a new external public wire surface by itself.
- Q: How should degraded analyzer outcomes be represented and enforced? -> A: Degradation remains a successful analyzer result with explicit degraded metadata, and missing required evidence defaults to fail-closed policy behavior.
- Q: How should resolved locations be represented in the shared decision contract? -> A: Use one unified resolved-location record that may carry span offsets, `json_pointer`, and logical field metadata as applicable.
- Q: How should language be handled in the shared analyzer request contract? -> A: Language is mandatory in the shared analyzer request contract, but adapters may inject a deterministic default when the caller does not provide one.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Freeze One Analyzer Contract For All Consumers (Priority: P1)

As a Pokrov maintainer, I want one stable analyzer request and result contract so that LLM, MCP, structured JSON, and evaluation flows can reuse the same analyzer outcome model without redefining their own result families.

**Why this priority**: Without one frozen analyzer contract, later Presidio rework workstreams can drift into incompatible request and result models, forcing rework and increasing safety risk.

**Independent Test**: Review the analyzer contract specification and verify that a runtime-oriented consumer and an evaluation-oriented consumer can both depend on the same top-level analyzer request and result families.

**Acceptance Scenarios**:

1. **Given** downstream runtime and evaluation flows need analyzer outcomes, **When** maintainers inspect the approved contract, **Then** they find one shared analyzer request contract and one shared analyzer result shape instead of separate consumer-specific models.
2. **Given** a consumer needs analyzer outcome data, **When** it depends on the contract, **Then** it can reuse `decision`, `transform`, `explain`, `audit`, `executed`, and `degraded` without redefining those sections locally.

---

### User Story 2 - Distinguish Policy Blocks From Analyzer Failures (Priority: P2)

As an integration engineer, I want policy-block outcomes to remain successful analyzer results while invalid input, invalid profile, and runtime failures remain analyzer errors so that consumers handle enforcement outcomes and internal failures differently and safely.

**Why this priority**: Conflating policy decisions with analyzer failures makes consumer behavior ambiguous and creates incorrect retry, fallback, and audit behavior.

**Independent Test**: Trace one block scenario and one analyzer-failure scenario and verify that the contract requires different handling paths without ambiguous overlap.

**Acceptance Scenarios**:

1. **Given** analyzer processing finds a policy outcome of `block`, **When** the final result is returned, **Then** the contract represents it as a successful analyzer result with safe explain and audit sections.
2. **Given** analyzer processing receives invalid input, an invalid profile, or an internal runtime failure, **When** the outcome is returned, **Then** the contract represents it as an analyzer error and not as a policy block.

---

### User Story 3 - Preserve Deterministic And Metadata-Only Outcomes Across Modes (Priority: P3)

As a security reviewer, I want analyzer outcomes to be deterministic and metadata-only across plain text, structured JSON, and future remote-recognizer participation so that replay, evaluation, and audit remain safe and comparable.

**Why this priority**: Determinism and metadata-only safety are core Pokrov invariants and must remain stable before recognizer, operator, structured, and remote work proceeds.

**Independent Test**: Review repeated-run and cross-consumer evidence to confirm that identical analyzer inputs produce the same resolved outcome identity and that explain and audit sections never require raw payload fragments.

**Acceptance Scenarios**:

1. **Given** identical analyzer input, profile, effective language, mode, and recognizer set, **When** the analyzer is run repeatedly, **Then** the resolved hits, final action, and replay identity remain the same.
2. **Given** text, structured JSON, and normalized remote-recognizer evidence all feed the analyzer, **When** consumers inspect explain and audit sections, **Then** they receive metadata-only outcomes without raw payload snippets or matched fragments.

### Edge Cases

- What happens when analyzer input is valid but produces zero resolved hits?
- What happens when a structured JSON leaf and a plain-text payload contain equivalent content but differ in field location metadata?
- What happens when a policy block is returned and no non-blocking transformed payload should be surfaced to consumers?
- What happens when remote-recognizer degradation or partial analyzer execution occurs but explain and audit outputs must remain metadata-only and the analyzer must default to fail-closed when required evidence is missing?
- What happens when invalid input or invalid profile errors are detected before policy resolution can complete?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The feature MUST define one shared analyzer request contract that captures payload, profile identity, execution mode, language, path class, and request or correlation identity for every analyzer invocation.
- **FR-001a**: The shared analyzer request contract MUST always carry one language value, and adapter layers MAY inject a deterministic configured default when the caller does not provide language explicitly.
- **FR-002**: The analyzer request contract MUST support optional entity scope filters, evaluation or dry-run flags, recognizer-family include or exclude gates, and policy-allowed explicit allowlist additions without requiring consumer-specific request variants.
- **FR-003**: The feature MUST define one shared typed analyzer result contract that downstream runtime and evaluation consumers can depend on directly.
- **FR-004**: The analyzer result contract MUST expose the top-level sections `decision`, `transform`, `explain`, `audit`, `executed`, and `degraded`.
- **FR-005**: The `decision` section MUST include the final action, total hit count, counts by entity family or category, one unified resolved-location model for text spans and structured field locations, and one stable replay identity for deterministic re-execution.
- **FR-005a**: Each resolved-location record in the shared `decision` section MUST carry span offsets, `json_pointer`, and logical field metadata when applicable without requiring consumer-specific location variants.
- **FR-006**: The `decision` section MUST NOT include raw matched values, raw excerpts, or debug-only internal objects.
- **FR-007**: A final policy action of `block` MUST be represented as a successful analyzer result rather than as an analyzer failure.
- **FR-008**: Invalid input, invalid profile, and internal runtime failures MUST remain analyzer errors and MUST NOT be represented as policy-block outcomes.
- **FR-009**: Identical analyzer input, profile, effective language, execution mode, and recognizer set MUST produce the same resolved-hit outcome and the same final action.
- **FR-010**: The contract MUST define deterministic ordering rules for same-span and same-score collisions so replay behavior is explicit rather than implied.
- **FR-011**: Plain-text and structured JSON analyzer flows MUST reuse the same top-level analyzer result shape after normalization.
- **FR-012**: Native recognizers and future remote recognizers MUST converge into the same normalized-hit path used by the shared analyzer result contract.
- **FR-013**: Runtime and evaluation consumers MUST be able to reuse the same top-level analyzer result sections without local policy-block special casing.
- **FR-014**: The `explain` and `audit` sections MUST remain metadata-only and MUST NOT carry raw payload text, matched substrings, or nearby source snippets.
- **FR-015**: The `executed` and `degraded` sections MUST describe execution-path and degradation facts in a consumer-safe way so downstream flows can reason about what ran and what degraded without unsafe payload exposure.
- **FR-015a**: Degraded analyzer outcomes MUST remain successful analyzer results rather than analyzer errors when a safe policy outcome can still be produced.
- **FR-015b**: When required evidence or execution paths are missing under degraded analyzer execution, the shared contract MUST default to fail-closed policy behavior and record that decision in metadata-safe degraded output.
- **FR-016**: The shared analyzer contract MUST support serialized reuse by downstream adapters and evaluation artifacts without creating a separate competing contract family.
- **FR-017**: This feature MUST freeze analyzer contract semantics for downstream Presidio rework workstreams and MUST require later work to reuse these semantics instead of redefining them locally.

### Key Entities *(include if feature involves data)*

- **Analyzer Request**: The stable input record that identifies the payload, execution context, profile context, and effective language for one analyzer invocation.
- **Analyzer Result**: The shared top-level outcome record that groups `decision`, `transform`, `explain`, `audit`, `executed`, and `degraded` for every successful analyzer completion.
- **Decision Summary**: The deterministic policy outcome record containing final action, counts, unified resolved locations, and replay identity.
- **Transform Result**: The safe record describing whether the outcome was blocked or transformed and what non-sensitive transformation facts remain visible to consumers.
- **Explain Summary**: The metadata-only explanation record that helps consumers understand why an outcome occurred without exposing payload fragments.
- **Audit Summary**: The metadata-only request-level evidence record for safe observability and compliance review.
- **Execution Summary**: The safe record of which analyzer stages or recognizer paths executed for a given request.
- **Degradation Summary**: The safe record of degraded analyzer behavior or missing execution paths that consumers may need for routing, replay, or evaluation interpretation.
- **Analyzer Error**: The failure outcome reserved for invalid input, invalid profile, and internal runtime failure states.

## Security & Privacy Constraints *(mandatory)*

- Raw payload content, matched substrings, and nearby source text MUST NOT appear in `decision`, `explain`, `audit`, `executed`, or `degraded` outputs.
- Policy-block outcomes MUST preserve the same metadata-only safety rules as non-block outcomes.
- The shared analyzer contract MUST preserve Pokrov ownership of final policy action even when future remote recognizers contribute evidence.
- Degraded analyzer handling MUST remain metadata-only and MUST default to fail-closed behavior when required evidence is unavailable.
- The contract MUST remain safe for reuse by runtime and evaluation consumers without requiring consumer-specific unsafe debug fields.

## Operational Readiness *(mandatory for runtime changes)*

- **Logs**: The contract MUST let later runtime flows emit structured events with request identity, profile identity, final action, path class, counts, and degradation metadata without adding unsafe payload fields.
- **Metrics**: The contract MUST support later count, replay, and degradation metrics without requiring raw-content parsing or consumer-local contract forks.
- **Health/Readiness**: This feature MUST not define a new health or readiness endpoint by itself, but it MUST provide contract clarity sufficient for later runtime verification and readiness checks.
- **Documentation/Config**: Documentation MUST describe analyzer request fields, top-level result sections, policy-block versus error semantics, deterministic replay identity, and metadata-only boundaries.

## Required Test Coverage *(mandatory)*

- **Unit**: Analyzer request construction, analyzer result construction, decision field safety, and error-versus-block distinction.
- **Integration**: At least one block path and one analyzer-error path, plus one compatibility proof showing that runtime-oriented and evaluation-oriented consumers reuse the same result sections.
- **Performance**: Verification that the frozen contract does not require duplicate consumer-specific result families or replay-identity ambiguity that would force extra conversion work in later hot paths.
- **Security**: Verification that decision, explain, audit, execution, and degradation outputs remain metadata-only and do not require raw payload fragments.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of downstream analyzer consumers in scope for this milestone can rely on the same top-level analyzer result sections without defining separate consumer-specific result families.
- **SC-002**: At least one executable proof shows that one runtime-oriented consumer path and one evaluation-oriented consumer path use the same analyzer result family and treat policy-block outcomes differently from analyzer failures.
- **SC-003**: 100% of reviewed analyzer contract fields in `decision`, `explain`, `audit`, `executed`, and `degraded` are metadata-safe and exclude raw payload text, matched substrings, and nearby source fragments.
- **SC-004**: In all covered replay tests, identical analyzer input, profile, effective language, mode, and recognizer set produce the same final action and the same replay identity.
- **SC-005**: In all covered degradation scenarios where required analyzer evidence is unavailable, the analyzer returns a successful degraded result, records fail-closed handling in metadata-safe output, and does not downgrade the event into an analyzer error.

## Acceptance Evidence *(mandatory)*

- An approved specification defining the analyzer request contract, analyzer result sections, decision semantics, and error semantics.
- A verification artifact showing one block outcome and one analyzer-error outcome under the frozen contract.
- At least one executable compatibility proof showing that runtime-oriented and evaluation-oriented consumers reuse the same top-level analyzer result family without local policy-block special casing.
- Deterministic replay evidence confirming stable final action and replay identity for repeated identical inputs.
- A field-safety review showing that the shared analyzer contract keeps explain, audit, execution, and degradation outputs metadata-only.

## Assumptions

- `009-architecture-foundation` remains the frozen upstream basis for pipeline stages, shared hit families, and extension-point direction.
- This feature freezes the analyzer-facing request and result contract, not recognizer-family behavior, operator-family behavior, or remote transport details.
- The shared analyzer contract may be serialized by downstream consumers, but this feature does not create a new public external wire API contract by itself.
- Adapters may supply a deterministic configured default language, but they must not omit effective language from the shared analyzer request contract.
- Downstream deterministic, structured, explain-and-audit, evaluation, and remote-recognizer workstreams will reuse this analyzer contract instead of redefining consumer-local variants.
