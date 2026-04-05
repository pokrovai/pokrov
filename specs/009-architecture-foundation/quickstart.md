# Quickstart: Architecture Foundation For Presidio Rework

## Preconditions

- Current branch: `009-architecture-foundation`
- The feature specification, plan, research, data model, and contract artifacts exist under `specs/009-architecture-foundation/`
- The implementation remains within current Pokrov crate boundaries
- No downstream recognizer-family or structured-processing feature is allowed to redefine the foundation locally

## Review Order

1. Read `spec.md` for scope and acceptance expectations.
2. Read `research.md` for design decisions and rejected alternatives.
3. Read `data-model.md` for the shared contract families.
4. Read `contracts/shared-contracts.md` for stage ownership and downstream consumer rules.
5. Read `contracts/revision-policy.md` for frozen-contract change control.

## Validation Scenarios

### 1. Stage Ownership Review

Confirm that each stage has one clear owner and one clear forbidden-responsibility set.

Expected:
- normalization does not mutate payloads
- recognizer execution does not own policy
- transformation does not re-run recognition
- explain and audit remain metadata-only
- `foundation_stage_boundaries()` exports the frozen sequence from `crates/pokrov-core/src/types/foundation.rs`

### 2. Shared Contract Family Review

Confirm that downstream deterministic, structured, evaluation, and remote work can reuse the approved contract families.

Expected:
- one normalized hit family
- one resolved-hit family
- one transform-plan family
- one transform-result family
- one explain summary family
- one audit summary family
- `SanitizationEngine::trace_foundation_flow` emits the same family set for runtime and evaluation paths

### 3. Runtime/Evaluation Proof Requirement Review

Confirm that implementation work for this feature includes one executable proof of shared runtime/evaluation contract reuse.

Expected:
- the proof uses the same top-level contract families
- the proof does not rely on a private evaluation-only result model
- the proof is exercised by `tests/integration/sanitization_foundation_shared_contracts.rs`

### 4. Evaluation Artifact Boundary Review

Confirm that repository-safe fixtures and restricted external references are separated conceptually and operationally.

Expected:
- repo-safe fixtures are eligible for repository storage
- restricted datasets remain external and carry access metadata
- retention and governance policy stay out of foundation scope
- `tests/fixtures/eval/README.md` remains the repo-safe guidance source for this boundary

### 5. Revision Control Review

Confirm that any downstream change to frozen contracts requires an explicit foundation revision.

Expected:
- no local downstream fork of stage ownership
- no silent expansion of explain/audit payload content
- no private contract family introduced for convenience

## Minimal Validation Command Set

```bash
cargo test --test contract sanitization_foundation
cargo test --test integration sanitization_foundation
cargo test --test security sanitization_foundation
cargo test --test performance sanitization_foundation
cargo test -p pokrov-core
cargo test --workspace
cargo clippy --all-targets --all-features
```

## Acceptance Evidence To Collect

- One accepted implementation note showing where compile-visible shared contracts live.
- One executable proof for shared runtime/evaluation contract reuse.
- Evidence that explain and audit contract families remain metadata-only.
- Evidence that downstream work can reference the approved contract families without redefining them locally.
- One verification note in `docs/verification/009-architecture-foundation.md` recording the command outputs.
