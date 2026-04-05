# Backlog: Analyzer Core Contract

Date: 2026-04-05
Spec source: `docs/superpowers/specs/presidio-rework/01-analyzer-core-contract.md`
Status: Draft

## Summary

This backlog turns the frozen architecture into a concrete runtime analyzer contract.
It defines what every consumer of the analyzer can rely on: inputs, outputs, result sections, deterministic behavior, and the split between policy outcomes and internal failures.

## Scope

In scope:
- analyzer input contract
- analyzer result shape
- decision section contract
- transform/explain/audit section boundaries
- policy-block vs error semantics
- deterministic replay contract

Out of scope:
- deterministic recognizer family logic
- operator-specific algorithms
- structured field-binding logic beyond result compatibility
- remote transport details

## Deliverables

- stable analyzer request contract
- stable analyzer result contract with `decision`, `transform`, `explain`, `audit`, `executed`, and `degraded`
- explicit policy-block semantics distinct from runtime errors
- deterministic signature or equivalent replay identity
- compatibility proof for LLM, MCP, structured, and evaluation consumers

## Tasks

### Phase 1: Input and top-level result contracts
- `A0101` Introduce or refine the analyzer request type so it captures payload, profile id, execution mode, language, path class, and request identity consistently.
- `A0102` Introduce or refine the analyzer top-level result shape so it includes `decision`, `transform`, `explain`, `audit`, `executed`, and `degraded` sections.
- `A0103` Ensure the result shape is serializable and reusable by HTTP-facing adapters, structured processors, and evaluation runners.

### Phase 2: Decision and error semantics
- `A0104` Define the decision section fields for final action, hit counts, resolved spans or field locations, and deterministic replay identity.
- `A0105` Implement or normalize the distinction between policy-block outcomes and analyzer runtime failures.
- `A0106` Ensure invalid input and invalid profile cases remain analyzer errors and are never represented as policy blocks.

### Phase 3: Compatibility and determinism
- `A0107` Ensure native and future remote recognizers feed the same normalized-hit path into the analyzer result.
- `A0108` Ensure structured JSON uses the same top-level result shape as plain text after normalization.
- `A0109` Add deterministic replay coverage for identical input, profile, language, mode, and recognizer set.

### Phase 4: Consumer verification
- `A0110` Add analyzer-contract tests that prove LLM and MCP adapters can consume the same result shape without special casing policy-block outcomes.
- `A0111` Add at least one structured/evaluation-oriented compatibility test using the same result sections.
- `A0112` Record analyzer-contract verification evidence after implementation lands.

## Dependencies

- Depends on `00-architecture-foundation-backlog.md` being implemented first.
- Blocks downstream implementation work for deterministic recognizers, operators, and structured processing.

## Acceptance Evidence

Implementation is complete when:
- analyzer inputs are explicit and stable
- analyzer outputs expose the required top-level sections
- policy-block and runtime-failure semantics are unambiguous
- deterministic replay identity is present and testable
- the same result shape is consumable by LLM, MCP, structured, and evaluation paths

## Suggested Verification

- unit tests for request/result construction
- integration-style tests for policy-block vs runtime-failure behavior
- replay test for deterministic identity
- compatibility tests for at least two downstream consumers

## Progress update 2026-04-05

- `A0101-A0104`: Implemented via canonical `EvaluateRequest`, `EvaluateResult`, and `EvaluateDecision` updates in `pokrov-core`.
- `A0105-A0106`: Implemented by preserving policy-block as successful result and keeping analyzer invalid input/profile/runtime as analyzer errors.
- `A0109-A0112`: Implemented with replay-identity logic, shared consumer wiring, and verification notes in `docs/verification/010-analyzer-core-contract.md`.
