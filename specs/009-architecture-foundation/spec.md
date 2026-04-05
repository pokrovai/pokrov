# Feature Specification: Architecture Foundation For Presidio Rework

**Feature Branch**: `009-architecture-foundation`  
**Created**: 2026-04-05  
**Status**: Draft  
**Input**: User-provided source documents: `docs/superpowers/specs/presidio-rework/00-architecture-foundation.md docs/superpowers/plans/presidio-rework/00-architecture-foundation-backlog.md`

## Clarifications

### Session 2026-04-05

- Q: Should the foundation be documentation-only, or must it also include compile-visible shared contract scaffolding? → A: Documentation + minimal compile-visible shared contracts and extension points
- Q: Is a documented walkthrough enough, or must foundation include one executable proof that runtime and evaluation flows share the same contract families? → A: One executable proof is required for shared runtime and evaluation contracts
- Q: Should the architecture foundation freeze only repository placement and contract-safe handling boundaries for evaluation data, or also define retention and governance policy? → A: Freeze only repository placement and contract-safe handling boundaries
- Q: If downstream work needs to change frozen contracts, can it adapt locally, or must it go through an explicit foundation revision first? → A: Any change requires an explicit foundation revision before affected downstream implementation proceeds

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Freeze Core Boundaries Before Family Work (Priority: P1)

As a Pokrov maintainer, I want one approved architecture foundation for the Presidio rework so that later work on recognizers, operators, structured processing, evaluation, and remote adapters builds on the same stage boundaries and shared contracts.

**Why this priority**: Without a frozen foundation, each later workstream can redefine the analyzer pipeline differently, causing contract churn, rework, and security drift.

**Independent Test**: Review the architecture foundation and confirm that later Presidio rework workstreams can reference shared contracts and stage ownership without redefining them.

**Acceptance Scenarios**:

1. **Given** the Presidio rework has multiple downstream workstreams, **When** maintainers review the foundation specification, **Then** they can identify one explicit boundary for detection, analysis, policy, transform, explain, and audit.
2. **Given** a downstream workstream needs shared result contracts, **When** it references the foundation, **Then** it can reuse the approved contract names and responsibilities instead of defining new competing versions.

---

### User Story 2 - Reuse One Shared Contract Model Across Runtime And Evaluation (Priority: P2)

As an implementation engineer, I want runtime flows and evaluation flows to depend on the same core result shapes so that feature delivery, parity testing, and evidence generation do not require separate incompatible models.

**Why this priority**: If runtime and evaluation evolve different contract families, parity testing becomes expensive, ambiguous, and fragile.

**Independent Test**: Trace one runtime-oriented flow and one evaluation-oriented flow and confirm that both can be expressed using the same top-level hit, transform, explain, and audit contracts.

**Acceptance Scenarios**:

1. **Given** a runtime sanitization flow and an evaluation replay flow, **When** both are mapped to foundation contracts, **Then** both flows can consume the same top-level result families.
2. **Given** future native and remote recognizers produce findings, **When** they are integrated into the architecture, **Then** their outputs can converge into one normalized hit model.

---

### User Story 3 - Encode Safety Invariants Into Shared Contracts (Priority: P3)

As a security reviewer, I want metadata-only and no-raw-data rules encoded into the architecture foundation so that later features cannot accidentally leak payload fragments through explain, audit, or evaluation-safe artifacts.

**Why this priority**: Safety boundaries are hardest to restore after multiple downstream features start depending on unsafe or ambiguous contracts.

**Independent Test**: Inspect the foundation requirements and verify that explain and audit outputs are restricted to metadata-safe fields and that restricted evaluation data remains outside the repository.

**Acceptance Scenarios**:

1. **Given** explain and audit outputs are defined by the foundation, **When** reviewers inspect allowed fields, **Then** raw payload snippets, matched fragments, and nearby source text are excluded.
2. **Given** future evaluation datasets include restricted material, **When** repository placement rules are applied, **Then** only repo-safe fixtures are commit-eligible and restricted sources remain external.

### Edge Cases

- What happens if a downstream workstream tries to combine policy ownership with recognizer execution in one stage?
- What happens if runtime flows and evaluation flows require different result fields for the same decision path?
- What happens if remote recognizers return findings that do not align with native recognizer result semantics?
- What happens if repository-safe fixtures and restricted benchmark references are mixed in one storage path?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The architecture foundation MUST define one explicit stage model covering input normalization, recognizer execution, analysis and suppression, policy resolution, transform application, safe explain, and audit summary.
- **FR-002**: The architecture foundation MUST define the responsibility boundary of each stage and state what each stage must not do.
- **FR-002a**: The architecture foundation MUST include minimal compile-visible shared contracts and extension points so downstream work can depend on stable result shapes without waiting for full family implementations.
- **FR-003**: The architecture foundation MUST define one normalized detection result shape that is shared by native and remote recognizer outputs.
- **FR-004**: The architecture foundation MUST define one resolved-hit shape for post-analysis decisions that can be consumed by policy, explain, and transformation work.
- **FR-005**: The architecture foundation MUST define one transform-planning contract and one transform-result contract for block and non-block outcomes.
- **FR-006**: The architecture foundation MUST define one safe explain summary contract and one audit summary contract for metadata-only outputs.
- **FR-007**: The architecture foundation MUST encode that final policy action remains owned by Pokrov core rather than by recognizers or remote adapters.
- **FR-008**: The architecture foundation MUST encode that explain and audit contracts cannot carry raw payload fragments, matched substrings, or nearby source text.
- **FR-009**: The architecture foundation MUST define extension points for native recognizers, remote recognizers, structured processors, evaluation runners, and baseline runners.
- **FR-010**: The architecture foundation MUST define dependency direction rules so downstream work cannot couple analysis to transform internals or move policy logic into transformation.
- **FR-011**: The architecture foundation MUST define repository placement rules that distinguish repo-safe fixtures from restricted evaluation datasets and external benchmark references.
- **FR-011a**: The architecture foundation MUST limit evaluation-data scope to repository placement and contract-safe handling boundaries and MUST NOT define retention or benchmark-governance policy at this stage.
- **FR-012**: The architecture foundation MUST support later workstreams reusing the same contract families for deterministic text handling, structured processing, evaluation, and remote integrations.
- **FR-013**: The architecture foundation MUST require an explicit foundation revision whenever a downstream spec needs to change frozen stage ownership or shared contract semantics before affected downstream implementation proceeds.

### Key Entities *(include if feature involves data)*

- **Pipeline Stage Boundary**: The approved ownership line between normalization, recognition, analysis, policy, transformation, explain, and audit.
- **Normalized Hit**: The common candidate-detection record shared by native and remote recognizers.
- **Resolved Hit**: The post-analysis detection record that survives suppression and overlap handling and is ready for policy use.
- **Transform Plan**: The approved action map that connects policy outcome to transformation order and behavior.
- **Transform Result**: The final outcome record for blocked or transformed payload handling.
- **Explain Summary**: The metadata-only explanation record used for safe diagnostics.
- **Audit Summary**: The metadata-only audit record used for request-level evidence.
- **Extension Point**: A bounded integration contract for native recognizers, remote recognizers, structured processors, evaluation runners, or baseline runners.

## Security & Privacy Constraints *(mandatory)*

- The foundation MUST preserve the rule that raw payload content never leaves the proxy boundary through explain, audit, or evaluation-safe artifacts.
- The foundation MUST preserve Pokrov ownership of final policy action, even when future remote recognizers participate in detection.
- The foundation MUST preserve JSON-safe, non-blocking transformation expectations for downstream work that mutates payload content.
- The foundation MUST require restricted datasets and non-redistributable benchmarks to remain outside the repository.
- The foundation MUST treat retention, access-governance, and benchmark onboarding policy for restricted datasets as later evaluation-planning scope rather than foundation scope.

## Operational Readiness *(mandatory for runtime changes)*

- **Logs**: This foundation MUST define safe audit and explain output boundaries so later runtime changes can emit metadata-only structured events consistently.
- **Metrics**: This foundation MUST leave room for later timing, count, and degradation metrics without requiring new unsafe result fields.
- **Health/Readiness**: This foundation MUST not introduce a new readiness contract by itself, but it MUST make later readiness checks for contract safety and extension validity possible.
- **Documentation/Config**: The foundation MUST document stage boundaries, shared contract families, extension points, and repository placement rules for future implementation work.

## Required Test Coverage *(mandatory)*

- **Unit**: Shared contract construction and field-safety checks for hit, transform, explain, and audit records.
- **Integration**: At least one executable proof MUST show that a runtime-oriented flow and an evaluation-oriented flow can target the same contract families.
- **Performance**: Verification that the foundation does not require extra hot-path result families or duplicate contract conversions for later runtime work.
- **Security**: Verification that explain and audit contracts cannot represent raw payload fragments and that restricted evaluation sources are clearly segregated from repo-safe fixtures.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All downstream Presidio rework specs for deterministic handling, structured processing, evaluation, and remote integration reference the shared foundation contract families without redefining stage ownership.
- **SC-002**: At least one runtime flow and one evaluation flow are verified through one executable proof using the same top-level contract families with no family-specific adapter-only result model.
- **SC-003**: Zero approved foundation contracts for explain and audit include fields intended to hold raw payload text, matched substrings, or nearby source fragments.
- **SC-004**: Downstream implementation planning for deterministic, structured, evaluation, and remote work can begin without opening a foundation revision for unresolved contract ambiguity, and any later frozen-contract change is routed through an explicit foundation revision.

## Acceptance Evidence *(mandatory)*

- An approved specification that defines the stage model, shared contract families, extension points, and repository placement rules.
- Evidence that the approved foundation includes minimal compile-visible shared contracts and extension points for downstream consumers.
- A verification note or walkthrough showing how a payload decision can move through all frozen stages without redefining responsibilities.
- One executable proof showing that runtime-compatible and evaluation-compatible flows reuse the same contract families.
- Evidence that explain and audit boundaries remain metadata-only and exclude raw sensitive payload fragments.

## Assumptions

- This feature is a foundation feature for the Presidio rework and is intentionally delivered before recognizer-family or operator-family implementation work.
- Existing Pokrov crate boundaries remain in place; this feature defines shared contracts and responsibilities rather than broad product refactoring.
- Later deterministic, structured, evaluation, and remote-adapter workstreams will consume this foundation instead of redefining it locally.
- Restricted benchmark datasets may be referenced by metadata, but they are not committed into the repository as part of this feature.
