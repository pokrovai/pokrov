# Quickstart: Analyzer Core Contract For Presidio Rework

## Preconditions

- Current branch: `010-analyzer-core-contract`
- The feature specification, plan, research, data model, and contract artifacts exist under `specs/010-analyzer-core-contract/`
- The implementation remains within current Pokrov crate boundaries
- `009-architecture-foundation` remains the frozen upstream basis for stage ownership and foundation trace behavior

## Review Order

1. Read `spec.md` for scope, clarifications, and acceptance expectations.
2. Read `plan.md` for technical context, constitution gates, and design outputs.
3. Read `research.md` for key contract decisions and rejected alternatives.
4. Read `data-model.md` for analyzer request/result entities and validation rules.
5. Read `contracts/analyzer-surface.md` and `contracts/consumer-compatibility.md` for downstream reuse rules.

## Validation Scenarios

### 1. Shared Request Contract Review

Confirm that all analyzer consumers can use one request shape.

Expected:
- request identity is always present
- profile, mode, path class, and payload are shared across consumers
- effective language is always present, even when injected through a deterministic adapter default
- optional entity-scope, recognizer-family, and allowlist additions do not require adapter-specific request variants

### 2. Shared Result Section Review

Confirm that every successful analyzer completion uses the same top-level result sections.

Expected:
- `decision`, `transform`, `explain`, `audit`, `executed`, and `degraded` are always present
- block outcomes remain successful results
- degraded outcomes remain successful results when a safe policy outcome exists and record fail-closed handling when required evidence is missing
- sanitized payload exists only in the transform section when non-blocking

### 3. Policy Block Versus Error Review

Confirm that consumers treat enforcement outcomes and analyzer failures differently.

Expected:
- `block` is not mapped to an analyzer error
- invalid input and invalid profile stay true errors
- degraded fail-closed outcomes are not mapped to analyzer errors when a safe result exists
- runtime adapters do not add local policy-block special casing

### 4. Consumer Compatibility Proof Review

Confirm that runtime and evaluation flows reuse the same analyzer contract family.

Expected:
- evaluate-path tests and at least one runtime-adapter test exercise the shared contract
- foundation trace or equivalent shared proof surface remains aligned with analyzer semantics
- no private evaluation-only result family appears

### 5. Replay And Metadata-Safety Review

Confirm that deterministic replay and metadata-only boundaries are preserved.

Expected:
- repeated identical inputs with the same effective language produce the same replay identity
- explain, audit, executed, and degraded outputs remain metadata-only
- unified structured/text location metadata works without exposing leaf values

## Minimal Validation Command Set

```bash
cargo test --test contract sanitization_evaluate_contract
cargo test --test contract sanitization_foundation_contract
cargo test --test integration sanitization_evaluate_flow
cargo test --test integration sanitization_foundation_shared_contracts
cargo test --test integration llm_proxy_happy_path
cargo test --test integration mcp_allowed_tool_path
cargo test --test security sanitization_metadata_leakage
cargo test --test security sanitization_foundation_metadata_leakage
cargo test --test performance sanitization_evaluate_latency
cargo test --test performance sanitization_foundation_contract_overhead
cargo test --workspace
cargo clippy --all-targets --all-features
```

## Acceptance Evidence To Collect

- One implementation note showing where the canonical analyzer request/result contracts live in `pokrov-core`.
- Contract evidence that all required result sections are present, degraded fail-closed handling is explicit, and policy-block/error semantics are distinct.
- One executable compatibility proof spanning a runtime-oriented consumer and an evaluation-oriented consumer.
- Deterministic replay evidence for repeated identical inputs with the same effective language.
- Metadata-safety evidence confirming no raw payload leakage outside the transform section.
