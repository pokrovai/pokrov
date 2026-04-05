# Research: Architecture Foundation For Presidio Rework

## Decision 1: The foundation includes compile-visible scaffolding, not just documentation

- **Decision**: The feature will deliver documentation plus minimal compile-visible shared contracts and extension points.
- **Rationale**: Downstream workstreams need stable symbols and result families to depend on. Pure documentation would not prevent contract drift during implementation.
- **Alternatives considered**:
  - Documentation-only foundation: too weak to prevent local reinvention.
  - Full stage skeleton with placeholder implementations everywhere: too much blast radius for a foundation feature.

## Decision 2: Shared runtime/evaluation contract reuse requires one executable proof

- **Decision**: The foundation must include one executable proof that runtime-oriented and evaluation-oriented flows can use the same top-level contract families.
- **Rationale**: This turns contract reuse into a verifiable guarantee instead of a narrative claim.
- **Alternatives considered**:
  - Documentation walkthrough only: easier, but does not block drift.
  - Executable proof for every extension point: unnecessary scope increase for the first foundation milestone.

## Decision 3: Extend current `pokrov-core` result types instead of replacing the whole pipeline at once

- **Decision**: Foundation work should evolve current `pokrov-core` types, especially the existing explain and audit summaries, toward the new shared model rather than replacing the current evaluation pipeline in one step.
- **Rationale**: Existing `ExplainSummary`, `AuditSummary`, and related request/result types already embody metadata-only constraints and should anchor the transition.
- **Alternatives considered**:
  - Greenfield result model with parallel pipeline: creates duplicate concepts and migration overhead.
  - Delay shared model design until recognizer-family work begins: increases downstream ambiguity.

## Decision 4: Evaluation-data scope in foundation is limited to placement and safe-handling boundaries

- **Decision**: The foundation will define only repository placement and safe-handling rules for evaluation artifacts.
- **Rationale**: Retention policy, access governance, and benchmark onboarding belong to later evaluation planning where dataset specifics exist.
- **Alternatives considered**:
  - Freeze retention and governance now: premature without concrete dataset operations.
  - Ignore evaluation data boundaries entirely: increases risk of restricted data leakage into the repository.

## Decision 5: Contract artifacts should be internal contract documents, not external API schemas

- **Decision**: `contracts/` will contain internal contract documents for shared result families and revision policy instead of external API schemas.
- **Rationale**: This feature defines cross-workstream architectural contracts inside the workspace rather than new user-facing HTTP or provider APIs.
- **Alternatives considered**:
  - Skip `contracts/` because the feature is internal: loses a stable artifact for downstream consumers.
  - Model the feature as OpenAPI/config schema changes: inaccurate because the feature is about internal foundation contracts.

## Decision 6: Frozen contracts change only through explicit foundation revision

- **Decision**: Any downstream need to change frozen stage ownership or shared contract semantics must go through an explicit foundation revision before affected implementation continues.
- **Rationale**: This keeps contract churn visible and controlled across the Presidio rework roadmap.
- **Alternatives considered**:
  - Allow local downstream adaptation with documentation: encourages divergence.
  - Forbid all future changes until the whole rework completes: too rigid for an incremental roadmap.

## Decision 7: Extension points stay interface-level only in this feature

- **Decision**: Native recognizers, remote recognizers, structured processors, evaluation runners, and baseline runners are frozen as interface-level extension points only.
- **Rationale**: The foundation must define who plugs into the shared model without dragging family behavior or transport details into scope.
- **Alternatives considered**:
  - Add concrete remote or evaluation implementations now: exceeds the foundation goal.
  - Omit extension points until later: forces downstream specs to invent their own integration boundaries.
