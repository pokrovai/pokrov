# Research: Analyzer Core Contract For Presidio Rework

## Decision 1: Evolve the existing `EvaluateRequest` and `EvaluateResult` path into the canonical analyzer contract

- **Decision**: The feature will extend the current `pokrov-core` request/result path instead of introducing a second parallel analyzer-only model.
- **Rationale**: `pokrov-api`, `pokrov-proxy-llm`, `pokrov-proxy-mcp`, and the existing test suites already depend on the current core surface. Extending the existing contract keeps the blast radius small and prevents adapter-local duplication.
- **Alternatives considered**:
  - Introduce a brand-new analyzer request/result family beside `EvaluateRequest` and `EvaluateResult`: clearer naming, but it would force duplicate mapping code and temporary contract drift.
  - Keep the existing surface untouched and document analyzer semantics only: too weak to freeze consumer expectations.

## Decision 2: `executed` and `degraded` must become explicit top-level analyzer result sections with fail-closed degradation semantics

- **Decision**: The shared analyzer result should expose structured `executed` and `degraded` sections rather than relying on one boolean plus scattered metadata in explain or audit, and degraded outcomes should remain successful analyzer results that default to fail-closed when required evidence is missing.
- **Rationale**: The feature spec requires these to be stable top-level sections that all consumers can reuse. A structured section makes consumer behavior explicit for dry-run, partial execution, and degraded recognizer paths without collapsing safe enforcement outcomes into engine failures.
- **Alternatives considered**:
  - Keep a single `executed: bool` and store degradation only in explain or audit markers: too implicit and adapter-specific.
  - Add only a `degraded: bool`: insufficient for safe consumer reasoning about what degraded and whether fail-closed behavior was applied.
  - Turn any degraded outcome into an analyzer error: conflates degraded enforcement with true engine failure.

## Decision 3: Reuse the current resolved-span path but generalize it into one unified metadata-safe resolved-location model

- **Decision**: The decision section should evolve the current resolved-span metadata into one unified location model that can represent both text spans and structured field locations without carrying raw values.
- **Rationale**: The existing core pipeline already tracks `json_pointer`, span boundaries, and effective action. Generalizing this path is safer than replacing it and is required for structured JSON compatibility.
- **Alternatives considered**:
  - Keep only text-span start/end positions: insufficient for structured JSON consumers.
  - Add raw field excerpts for easier debugging: violates metadata-only safety.
  - Split text and structured decisions into separate location record families: creates consumer-specific branching and contract drift.

## Decision 4: Policy blocks remain successful analyzer results; analyzer failures remain true errors

- **Decision**: `block` stays a successful analyzer outcome with decision/transform/explain/audit/executed/degraded sections, while invalid input, invalid profile, and runtime failures remain analyzer errors.
- **Rationale**: This preserves clear consumer semantics for retries, observability, and upstream suppression. A block is a valid policy outcome, not an engine failure.
- **Alternatives considered**:
  - Represent policy blocks as errors for transport convenience: conflates enforcement with failure.
  - Return nested transport-specific wrappers that reinterpret block semantics per adapter: creates local drift.

## Decision 5: The contract remains an internal shared surface, not a new public wire API

- **Decision**: `contracts/` will document the internal analyzer contract and consumer rules rather than define a new OpenAPI or provider-facing schema.
- **Rationale**: This feature freezes the shared typed contract inside the workspace. Serialized reuse is allowed, but the feature does not create a new public external API surface by itself.
- **Alternatives considered**:
  - Model the feature as a new HTTP API schema: inaccurate for the requested scope.
  - Skip contract documents because the feature is internal: loses a stable artifact for downstream workstreams.

## Decision 6: Consumer compatibility proof should extend the current shared-contract and adapter test surface

- **Decision**: Compatibility evidence should build on the existing `trace_foundation_flow`, evaluate-path tests, and LLM/MCP adapter tests rather than introduce a separate proof harness.
- **Rationale**: The current test layout already proves cross-crate reuse points. Extending that evidence is cheaper and more representative than inventing a second verification layer.
- **Alternatives considered**:
  - Add a standalone analyzer proof harness disconnected from runtime adapters: weaker confidence and duplicated setup.
  - Rely on documentation review alone: insufficient for the spec requirement of executable proof.

## Decision 7: Effective language is mandatory in the shared request contract even if adapters initially default it

- **Decision**: Effective language, entity-scope filters, recognizer-family gates, and policy-allowed allowlist additions should be represented in the core analyzer request contract, with adapter layers allowed to inject a deterministic configured default language when the caller does not provide one.
- **Rationale**: The contract must be stable for structured, remote, and evaluation consumers before those workstreams land. Making effective language mandatory avoids ambiguous replay and recognizer selection while preserving current adapter feasibility.
- **Alternatives considered**:
  - Defer all optional analyzer inputs until later workstreams: pushes contract churn downstream.
  - Keep language optional in the shared contract: weakens replay determinism and recognizer selection guarantees.
  - Add the fields only to individual adapters first: creates consumer-specific request variants.
