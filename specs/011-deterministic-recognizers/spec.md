# Feature Specification: Deterministic Recognizers

**Feature Branch**: `011-deterministic-recognizers`  
**Created**: 2026-04-05  
**Status**: Draft  
**Input**: User description: "`docs/superpowers/specs/presidio-rework/02-deterministic-recognizers.md docs/superpowers/plans/presidio-rework/02-deterministic-recognizers-backlog.md`"

## Clarifications

### Session 2026-04-05

- Q: What should be the default behavior when deterministic validation fails? → A: Failed validation rejects the candidate by default; only explicitly defined families may use downgraded retention.
- Q: How should same-span deterministic collisions be resolved by default? → A: Highest final score wins first; ties break by explicit family priority, then stable recognizer ordering.
- Q: What is the default allowlist suppression scope? → A: Allowlist suppression applies only to exact normalized matches for the configured entity scope.
- Q: What is the default effect of negative context? → A: Negative context downscores the candidate by default; only explicitly defined families may suppress.

## User Scenarios and Testing *(mandatory)*

### User Story 1 - Detect high-confidence structured secrets consistently (Priority: P1)

A security operator defines deterministic recognition rules so the proxy can identify predictable sensitive values, such as account-like numbers or configured forbidden terms, before those values leave the trust boundary.

**Why this priority**: Deterministic detection is the first usable layer for enforcement because it provides predictable outcomes for blocking, masking, or redaction decisions.

**Independent test**: Submit the same approved test inputs multiple times under the same profile and confirm that the same candidates, scores, and suppression outcomes are produced every time.

**Acceptance scenarios**:

1. **Given** a profile with deterministic recognition rules for a supported sensitive value, **When** the proxy analyzes matching content, **Then** it returns a candidate with a normalized category, location, score, provenance, and reason codes.
2. **Given** the same content is analyzed repeatedly under the same profile, **When** the evaluation completes, **Then** the candidate set and final statuses are identical across runs.

---

### User Story 2 - Tune confidence without losing safety controls (Priority: P2)

A policy author uses contextual hints, allowlists, and denylists to reduce false positives and strengthen high-confidence detections without making outcomes unpredictable.

**Why this priority**: Deterministic recognizers are only useful in production if operators can tune them safely while preserving clear precedence between true positives and suppression rules.

**Independent test**: Evaluate curated examples that include contextual hints, configured allowed values, and configured denied values, then confirm the expected precedence and reason codes for each case.

**Acceptance scenarios**:

1. **Given** a matched value with positive contextual evidence, **When** it is evaluated, **Then** the candidate confidence increases only within the configured deterministic rules.
2. **Given** a matched value that is explicitly allowlisted for the active profile, **When** it is evaluated, **Then** the candidate is suppressed even if contextual evidence is positive.
3. **Given** a configured denied value overlaps with another deterministic candidate, **When** evaluation completes, **Then** both candidates enter the same final decision stage and the winning outcome follows explicit precedence rules.

---

### User Story 3 - Review outcomes without exposing raw payloads (Priority: P3)

A security reviewer inspects why a deterministic detection was accepted, rejected, or suppressed using metadata-only evidence that is safe to retain in logs and audit workflows.

**Why this priority**: Operators need explainable outcomes to trust enforcement decisions, but the review path must not leak the sensitive content being protected.

**Independent test**: Review generated logs and audit evidence for accepted, rejected, and suppressed cases and confirm that explanation metadata is present while raw payload content is absent.

**Acceptance scenarios**:

1. **Given** a deterministic candidate that fails validation or is suppressed, **When** metadata is recorded for review, **Then** the record includes status and reason codes without exposing the raw matched value.
2. **Given** the same sensitive value appears in plain text and in a structured payload field, **When** both are analyzed, **Then** reviewers can distinguish the field context while seeing the same normalized detection outcome.

### Edge Cases

- What happens when a value matches multiple deterministic families on the same span? The candidate with the highest final score wins, with ties resolved by explicit family priority and then stable recognizer ordering.
- How does the system behave when validation rejects a pattern hit after an initial match is found? The candidate is removed by default unless the recognizer family explicitly documents downgraded retention.
- What happens when an allowlisted term is embedded in longer surrounding text that would otherwise trigger a detection? Embedded text is not suppressed by default unless it forms an exact normalized match for the configured entity scope.
- How does the system behave when contextual hints are missing, mixed-language, or contradictory? Negative context reduces confidence by default, and only explicitly defined families may suppress a candidate outright.
- What happens when the same sensitive value appears in both free text and structured fields within one request?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST support deterministic recognizer families for pattern matches, validation-based matches, contextual scoring, configured denied values, and configured allowed values.
- **FR-002**: The system MUST evaluate deterministic candidates in one documented and repeatable order from initial match creation through final suppression or promotion.
- **FR-003**: The system MUST normalize recognized values and categories before producing the final candidate record.
- **FR-004**: The system MUST reject a candidate by default when deterministic validation fails, and it MAY allow explicitly defined recognizer families to retain the candidate with downgraded confidence when that exception is documented in the active rules.
- **FR-005**: The system MUST allow positive contextual evidence to adjust candidate confidence within deterministic rules defined for the active profile, and it MUST treat negative context as a confidence reduction by default unless an explicitly defined recognizer family documents stronger suppression behavior.
- **FR-006**: The system MUST apply allowlist suppression before final overlap resolution only for exact normalized matches within the configured entity scope so approved false positives do not become enforceable detections without suppressing embedded non-exact matches by default.
- **FR-007**: The system MUST treat denylist positives as first-class candidates that still pass through the same final decision stage and policy evaluation as other deterministic candidates.
- **FR-008**: The system MUST resolve same-span and same-family collisions by selecting the candidate with the highest final score first, then breaking ties by explicit family priority and finally by stable recognizer ordering so identical inputs never produce different winners.
- **FR-009**: The system MUST emit, for every deterministic candidate, a normalized category, location, score, recognizer identifier, evidence class, validation status, reason codes, and suppression status when applicable.
- **FR-010**: The system MUST preserve field-path context for detections originating from structured payloads so downstream policy decisions can distinguish where a match came from.
- **FR-011**: The system MUST produce the same detection and suppression outcomes for equivalent content regardless of whether the content originated from plain text or an extracted structured field, except for field-context metadata.
- **FR-012**: The system MUST support profile, tenant, and language scoping for deterministic list-based controls where such scope is defined.

### Key Entities *(include if feature involves data)*

- **Deterministic Recognizer Family**: A configurable detection rule group that produces repeatable candidates from patterns, validation checks, contextual evidence, denied values, or allowed values.
- **Deterministic Candidate**: A normalized detection result with score, location, provenance, validation state, and suppression metadata used for downstream policy decisions.
- **Context Signal**: Positive or negative surrounding evidence that can change candidate confidence within deterministic limits.
- **List Control Entry**: A configured allowed or denied value bound to one or more profiles, tenants, languages, or entity categories.
- **Field Context**: Metadata that identifies where a candidate was found inside structured content without altering the candidate’s detection semantics.

## Security and Privacy Constraints *(mandatory)*

- Sensitive request content, matched values, normalized values, and contextual source text MUST NOT be written to logs, audit stores, or operator-facing evidence.
- Deterministic recognizers MUST produce only metadata-safe explanation artifacts, including status, scope, reason codes, counts, and routing context.
- Suppression, rejection, masking, redaction, and blocking decisions MUST remain policy-driven and MUST NOT be bypassed by contextual boosts or denylist provenance.
- Allowed and denied values MUST be scoped to approved profiles or tenants so one tenant’s configuration cannot silently alter another tenant’s outcomes.

## Operational Readiness *(mandatory for runtime changes)*

- **Logs**: Metadata-only events must record request identity, active profile, recognizer family counts, suppression counts, validation outcomes, and final decision summaries.
- **Metrics**: Operations must be able to track deterministic evaluation volume, candidate counts by family, suppression counts, validation rejection counts, and end-to-end evaluation latency.
- **Health/Readiness**: Service readiness must reflect whether required deterministic recognizer definitions and scoped list controls are loaded for active traffic paths.
- **Documentation/config**: Operator guidance and configuration examples must explain recognizer precedence, supported scopes, safe reason codes, and the difference between allowed and denied values.

## Required Test Coverage *(mandatory)*

- **Unit**: Candidate creation, normalization, validation outcomes, contextual scoring, allowlist suppression, denylist positives, and same-span precedence logic.
- **Integration**: End-to-end deterministic analysis for plain text and structured payloads, including accepted, rejected, and suppressed outcomes under realistic profiles.
- **Performance**: Repeatable workloads that confirm deterministic evaluation stays within the existing proxy overhead budget for supported inputs.
- **Security**: Verification that logs and audit evidence remain metadata-only and that scoped list controls do not leak or cross tenant boundaries.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In replay verification, 100% of repeated runs on the same approved input set produce identical deterministic candidates, scores, and suppression statuses.
- **SC-002**: For the phase-one deterministic entity set, at least 95% of seeded positive cases are detected and at least 95% of seeded approved false-positive cases are suppressed in the acceptance dataset.
- **SC-003**: Structured payload verification shows zero behavioral drift between plain-text inputs and equivalent extracted field values other than field-context metadata.
- **SC-004**: Deterministic recognition stays within the existing release target of no more than 50 ms additional overhead at P95 for the approved verification workload.

## Acceptance Evidence *(mandatory)*

- A replay report showing identical outcomes across repeated runs of the approved deterministic test corpus.
- Acceptance test results covering validation success, validation rejection, contextual boosts, contextual suppression, allowlist suppression, denylist positives, and overlap resolution.
- Structured payload evidence showing equivalent outcomes between free-text inputs and extracted field values with preserved field context.
- Audit and log inspection evidence proving that no raw sensitive values or raw payload fragments are retained in operator-visible artifacts.

## Assumptions

- The first release covers only the initial high-priority deterministic entity families needed for phase-one security enforcement.
- English and Russian are the only languages that require lexical context behavior in this feature.
- Profile and tenant binding already exist as external configuration concepts and can scope deterministic controls without redefining those concepts here.
- Remote recognizers, machine-learning recognizers, and broader entity-pack expansion remain out of scope for this feature.
