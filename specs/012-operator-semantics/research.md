# Research: Operator Semantics Freeze

## Decision 1: Support exactly five core operators and fail closed on unsupported values

- **Decision**: Freeze support to `replace`, `redact`, `mask`, `hash`, and `keep`; any other operator reference resolves to terminal `block` with metadata-only reason `unsupported_operator`.
- **Rationale**: A fixed operator surface reduces ambiguity, keeps policy behavior auditable, and prevents silent fail-open bypasses.
- **Alternatives considered**:
  - Implicit fallback to `redact`: rejected because it hides configuration errors and can alter intended policy outcomes.
  - Runtime extension operators (lambda/custom): rejected as out of v1 scope and high risk for deterministic guarantees.

## Decision 2: Apply operators only after overlap resolution in deterministic order

- **Decision**: Transform application consumes only resolved hits after suppression and uses a stable deterministic order for application.
- **Rationale**: This guarantees replayability and prevents suppressed spans from being re-applied by downstream transform logic.
- **Alternatives considered**:
  - Applying during candidate stage: rejected because overlap winners are not final yet.
  - Per-family transform paths: rejected due to duplicated logic and inconsistent ordering.

## Decision 3: Keep `hash` one-way and deterministic within the same profile

- **Decision**: `hash` outputs are one-way and deterministic for identical input under the same profile context.
- **Rationale**: Deterministic hashing is required for replay-equivalent outputs and stable explain/audit summaries, while one-way semantics keep deanonymization out of core.
- **Alternatives considered**:
  - Randomized hash/salting per request: rejected because it breaks deterministic replay.
  - Reversible tokenization/encryption: rejected as explicitly out of scope.

## Decision 4: Keep `keep` explicit and observable in metadata-only outputs

- **Decision**: `keep` remains allowed only as an explicit policy action and must be marked in metadata-only explain/audit outputs.
- **Rationale**: This preserves policy flexibility while making intentional non-masking decisions visible for security review.
- **Alternatives considered**:
  - Forcing `block` for all sensitive `keep`: rejected because policy owners may intentionally allow specific scopes.
  - Silent passthrough: rejected because it weakens auditability.

## Decision 5: Preserve JSON validity by mutating only string leaves in non-blocking paths

- **Decision**: Non-blocking transforms mutate only string leaves; object/array shape and non-string leaves remain unchanged.
- **Rationale**: This maintains protocol validity for upstream/downstream consumers and avoids structural regressions.
- **Alternatives considered**:
  - Broad value coercion to strings: rejected due to schema breakage risk.
  - Whole-document text replacement: rejected because it is not JSON-safe.

## Decision 6: Reuse existing verification surfaces for acceptance evidence

- **Decision**: Add operator-semantics coverage in existing unit/contract/integration/security/performance test surfaces.
- **Rationale**: Existing suites already enforce runtime and policy contracts; extension avoids drift and duplicate harnesses.
- **Alternatives considered**:
  - Feature-only ad-hoc harness: rejected because it diverges from production entrypoints.
