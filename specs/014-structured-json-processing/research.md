# Research: Structured JSON Processing

## Decision 1: Keep deterministic structured traversal as a first-class contract

- **Decision**: Traverse objects and arrays with stable ordering and process only string leaves as recognizer input.
- **Rationale**: Deterministic traversal is required for repeatable policy outcomes, explainability, and acceptance tests.
- **Alternatives considered**:
  - Non-deterministic map iteration: rejected due to unstable policy and test results.
  - Full-value processing (all leaf types): rejected because only strings are valid recognizer input in current analyzer model.

## Decision 2: Encode explicit path binding precedence

- **Decision**: Apply binding precedence in fixed order: exact pointer -> logical alias -> subtree default -> profile default -> global default.
- **Rationale**: Prevents ambiguous overrides and ensures predictable behavior for path-specific policies.
- **Alternatives considered**:
  - Last-match-wins approach: rejected because it is config-order dependent and hard to reason about.
  - Priority by specificity without explicit levels: rejected due to hidden edge-case conflicts.

## Decision 3: Split behavior by payload size policy

- **Decision**: For payload <=1 MB, enforce p95 overhead target <=50 ms; for payload >1 MB, continue best-effort processing without latency SLA while preserving security/privacy invariants.
- **Rationale**: Keeps v1 performance guarantees realistic for normal traffic while avoiding unsafe bypass behavior for larger payloads.
- **Alternatives considered**:
  - Hard reject oversize payloads: rejected for this feature after clarification (chosen behavior is best-effort).
  - Keep same SLA for all sizes: rejected due to high risk of unstable latency.

## Decision 4: Use fail-closed behavior for high-risk processing failures

- **Decision**: If structured analyzer/transform processing fails in high-risk context, block request (fail-closed).
- **Rationale**: Security-first boundary requires blocking rather than forwarding potentially unsanitized data.
- **Alternatives considered**:
  - Fail-open always: rejected due to leakage risk.
  - Hybrid by low-risk/high-risk path: rejected for this iteration; high-risk fail-closed explicitly required.

## Decision 5: Restrict explain/audit summaries to path-safe metadata

- **Decision**: Expose only safe categories/counts/path classes; never include raw values or exact JSON pointer in explain/audit summaries.
- **Rationale**: Prevents structural and value leakage while preserving operational diagnostics.
- **Alternatives considered**:
  - Include exact pointers in default summaries: rejected due to privacy and data-minimization risk.
  - Debug-only exact pointers: rejected for v1 baseline to keep safety contract simple and strict.

## Decision 6: Reuse shared core contracts across plain-text and structured flows

- **Decision**: Structured mode reuses existing normalized hit/decision/transform contract family rather than introducing a separate result model.
- **Rationale**: Reduces drift between modes and keeps audit/explain semantics consistent.
- **Alternatives considered**:
  - Structured-only result contract: rejected due to higher maintenance and parity risk.
  - Adapter layer at proxy-only level: rejected because core analyzer invariants must remain centralized.
