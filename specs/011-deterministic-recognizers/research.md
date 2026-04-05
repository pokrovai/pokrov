# Research: Deterministic Recognizers

## Decision 1: Keep deterministic recognizer execution inside `pokrov-core`

- **Decision**: Implement deterministic recognizer orchestration as an extension of the existing `pokrov-core` detection and analysis stages, not as a separate runtime service or a policy-layer post-processor.
- **Rationale**: The current analyzer already owns detection, overlap resolution, replay identity, explain, and audit contracts. Keeping deterministic recognizers in `pokrov-core` preserves sanitization-before-upstream guarantees, avoids new network or serialization overhead, and keeps the metadata-only invariants enforceable in one place.
- **Alternatives considered**:
  - A remote or sidecar recognizer service: rejected because it expands v1 scope, adds latency, and weakens fail-closed behavior.
  - Policy-only list handling without recognizer integration: rejected because precedence, suppression, and explainability would fragment across stages.

## Decision 2: Compile deterministic families from profile-scoped startup configuration

- **Decision**: Extend `pokrov-config` sanitization profiles so deterministic recognizer definitions, validators, context dictionaries, allowlists, and denylists are validated at startup and compiled into `SanitizationEngine` profile state.
- **Rationale**: Startup compilation matches the current `PolicyProfile` and `compile_custom_rules` model, preserves readiness semantics, and ensures invalid recognizer definitions fail before serving traffic.
- **Alternatives considered**:
  - Runtime hot reload: rejected because it is explicitly outside current v1 scope.
  - Per-request regex or list compilation: rejected because it would add avoidable hot-path allocations and latency.

## Decision 3: Normalize all deterministic family outputs into one shared candidate contract

- **Decision**: Each deterministic recognizer family will emit one common candidate shape before overlap resolution, carrying normalized category, location, score, recognizer provenance, validation status, reason codes, and suppression metadata.
- **Rationale**: The existing foundation contracts already distinguish normalized and resolved hit families. Extending those contracts is the lowest-risk way to make deterministic recognizers reusable across runtime flows, evaluation flows, explain summaries, and future remote recognizer convergence.
- **Alternatives considered**:
  - Family-specific result structs: rejected because they would force policy, transform, and audit consumers to understand recognizer internals.
  - Delaying contract updates until more families exist: rejected because it would create churn across later specs.

## Decision 4: Freeze deterministic precedence defaults before implementation

- **Decision**: The feature adopts the clarified defaults as design-time rules:
  - failed validation rejects a candidate by default
  - same-span winners are chosen by highest final score, then family priority, then stable recognizer ordering
  - allowlist suppression applies only to exact normalized matches in the configured entity scope
  - negative context downscores by default and suppresses only when a family explicitly documents stronger behavior
- **Rationale**: These defaults directly affect test vectors, contract fields, overlap logic, and explain summaries. Locking them in research prevents plan-stage drift and implementation rework.
- **Alternatives considered**:
  - Leaving defaults family-specific: rejected because it would weaken determinism and make acceptance tests ambiguous.
  - Allowing broad substring allowlist suppression: rejected because it increases false-negative risk for security-sensitive content.

## Decision 5: Reuse current verification surfaces and add deterministic-family coverage to them

- **Decision**: Verification will extend existing test suites under `tests/contract`, `tests/integration`, `tests/security`, and `tests/performance` plus focused unit tests in `pokrov-core` and `pokrov-config`.
- **Rationale**: The repository already contains analyzer contract and performance suites. Reusing them keeps acceptance evidence aligned with current release gates and avoids a disconnected test harness.
- **Alternatives considered**:
  - Creating a feature-only standalone test harness: rejected because it duplicates runtime contracts and risks drift from real entrypoints.
  - Deferring performance/security proof until end-to-end release hardening: rejected because deterministic recognizers live in a hot path and must prove safety early.
