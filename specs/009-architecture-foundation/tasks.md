# Tasks: Architecture Foundation For Presidio Rework

**Input**: Design artifacts from `specs/009-architecture-foundation/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md`

**Tests**: Test tasks are mandatory for this feature because it affects shared proxy/policy/security contracts and must provide unit, integration, performance, and security evidence.

**Organization**: Tasks are grouped by user story so each story can be implemented and validated independently.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel if files do not overlap and no incomplete dependency is required
- **[Story]**: Maps the task to a user story from `spec.md` (`US1`, `US2`, `US3`)
- Every task includes exact file paths

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare feature-specific verification files and test harness entry points before shared-contract changes land.

- [X] T001 Create foundation verification note scaffold in `docs/verification/009-architecture-foundation.md`
- [X] T002 [P] Create shared test support helpers for foundation scenarios in `tests/common/sanitization_foundation_test_support.rs`
- [X] T003 [P] Wire foundation-specific suites into `tests/contract.rs`, `tests/integration.rs`, `tests/security.rs`, and `tests/performance.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Establish the shared code-facing contract surface that blocks all user stories.

**CRITICAL**: No user story work starts before this phase is complete.

- [X] T004 Define the foundation module layout and exports in `crates/pokrov-core/src/lib.rs` and `crates/pokrov-core/src/types.rs`
- [X] T005 [P] Create stage-boundary and extension-point scaffolding in `crates/pokrov-core/src/types/foundation.rs`
- [X] T006 [P] Create `NormalizedHit`, `ResolvedHit`, and `TransformPlan` scaffolding in `crates/pokrov-core/src/types/foundation.rs`
- [X] T007 [P] Extend `ExplainSummary`, `AuditSummary`, and related safe placeholder fields in `crates/pokrov-core/src/types.rs`
- [X] T008 [P] Update metadata-only explain and audit builders for foundation-safe fields in `crates/pokrov-core/src/audit/mod.rs`
- [X] T009 Document foundational verification checkpoints in `docs/verification/009-architecture-foundation.md`

**Checkpoint**: Shared contract scaffolding is available and downstream story work can proceed without redefining core contract families.

---

## Phase 3: User Story 1 - Freeze Core Boundaries Before Family Work (Priority: P1) MVP

**Goal**: Freeze explicit stage ownership so downstream Presidio workstreams cannot redefine detection, analysis, policy, transform, explain, and audit boundaries.

**Independent Test**: Review the resulting exports and tests to confirm one approved stage boundary exists for each pipeline stage and that downstream consumers use those exports instead of local reinvention.

### Tests for User Story 1

- [X] T010 [P] [US1] Add unit tests for stage ownership and forbidden-responsibility invariants in `crates/pokrov-core/src/types/foundation.rs`
- [X] T011 [P] [US1] Add contract coverage for foundation symbol exports in `tests/contract/sanitization_foundation_contract.rs`
- [X] T012 [P] [US1] Add integration coverage for the stage-boundary walkthrough in `tests/integration/sanitization_foundation_stage_boundaries.rs`
- [X] T013 [P] [US1] Add security coverage for policy-ownership separation in `tests/security/sanitization_foundation_stage_ownership.rs`

### Implementation for User Story 1

- [X] T014 [P] [US1] Implement stage-boundary enums and ownership metadata in `crates/pokrov-core/src/types/foundation.rs`
- [X] T015 [P] [US1] Refactor `SanitizationEngine` flow comments and exports around the approved stage model in `crates/pokrov-core/src/lib.rs`
- [X] T016 [US1] Update API-facing evaluate contract usage to consume foundation-safe exports in `crates/pokrov-api/src/handlers/evaluate.rs`
- [X] T017 [US1] Record accepted stage-boundary evidence in `docs/verification/009-architecture-foundation.md`

**Checkpoint**: User Story 1 is independently verifiable: core stage ownership is explicit, exported, and protected by tests.

---

## Phase 4: User Story 2 - Reuse One Shared Contract Model Across Runtime And Evaluation (Priority: P2)

**Goal**: Ensure runtime flows and evaluation-oriented flows use the same top-level contract families, backed by one executable proof.

**Independent Test**: Run the executable proof and verify that runtime-oriented and evaluation-oriented flows share the same contract families without a private evaluation-only result model.

### Tests for User Story 2

- [X] T018 [P] [US2] Add unit coverage for `NormalizedHit`, `ResolvedHit`, and `TransformPlan` construction in `crates/pokrov-core/src/types/foundation.rs`
- [X] T019 [P] [US2] Add integration proof for shared runtime/evaluation contract reuse in `tests/integration/sanitization_foundation_shared_contracts.rs`
- [X] T020 [P] [US2] Add performance regression coverage for shared contract reuse in `tests/performance/sanitization_foundation_contract_overhead.rs`

### Implementation for User Story 2

- [X] T021 [P] [US2] Add runtime/evaluation-compatible helper fields and conversions in `crates/pokrov-core/src/types/foundation.rs` and `crates/pokrov-core/src/types.rs`
- [X] T022 [P] [US2] Implement shared proof support helpers in `tests/common/sanitization_foundation_test_support.rs`
- [X] T023 [US2] Integrate shared contract proof expectations into `crates/pokrov-core/src/lib.rs` and `crates/pokrov-core/src/audit/mod.rs`
- [X] T024 [US2] Record executable runtime/evaluation proof evidence in `docs/verification/009-architecture-foundation.md`

**Checkpoint**: User Story 2 is independently verifiable: one executable proof demonstrates shared runtime/evaluation contract reuse.

---

## Phase 5: User Story 3 - Encode Safety Invariants Into Shared Contracts (Priority: P3)

**Goal**: Encode metadata-only and no-raw-data rules into shared contracts and define safe repository boundaries for evaluation artifacts.

**Independent Test**: Verify that explain and audit contracts cannot serialize raw payload fragments and that repo-safe evaluation guidance is explicitly separated from restricted external references.

### Tests for User Story 3

- [X] T025 [P] [US3] Add security coverage for metadata-only explain and audit serialization in `tests/security/sanitization_foundation_metadata_leakage.rs`
- [X] T026 [P] [US3] Add integration coverage for evaluation artifact boundary handling in `tests/integration/sanitization_foundation_evaluation_boundary.rs`
- [X] T027 [P] [US3] Add unit coverage for safe explain and audit placeholders in `crates/pokrov-core/src/audit/mod.rs`

### Implementation for User Story 3

- [X] T028 [P] [US3] Enforce metadata-only explain and audit placeholders in `crates/pokrov-core/src/types.rs` and `crates/pokrov-core/src/audit/mod.rs`
- [X] T029 [P] [US3] Create repo-safe evaluation boundary guidance in `tests/fixtures/eval/README.md`
- [X] T030 [US3] Record security and evaluation-boundary evidence in `docs/verification/009-architecture-foundation.md`

**Checkpoint**: User Story 3 is independently verifiable: metadata-only safety and evaluation-boundary rules are encoded and tested.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final documentation alignment, roadmap sync, and verification capture across all stories.

- [X] T031 [P] Update feature guidance in `specs/009-architecture-foundation/quickstart.md`, `specs/009-architecture-foundation/contracts/shared-contracts.md`, and `specs/009-architecture-foundation/contracts/revision-policy.md`
- [X] T032 [P] Sync roadmap references in `docs/superpowers/plans/presidio-rework/master-roadmap.md` and `docs/superpowers/plans/presidio-rework/00-architecture-foundation-backlog.md`
- [X] T033 Run final verification commands and record outputs in `docs/verification/009-architecture-foundation.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1: Setup** starts immediately.
- **Phase 2: Foundational** depends on Phase 1 and blocks all user stories.
- **Phase 3: US1** depends on Phase 2.
- **Phase 4: US2** depends on Phase 2 and should build on US1 exports to avoid duplicate contract work.
- **Phase 5: US3** depends on Phase 2 and should reuse the shared contract families introduced by US1 and US2.
- **Phase 6: Polish** depends on completion of the intended user stories.

### User Story Dependencies

- **US1 (P1)** is the MVP and should land first.
- **US2 (P2)** depends on the shared contract families and stage ownership introduced by US1.
- **US3 (P3)** depends on the shared contract families from US1 and benefits from the executable proof added in US2.

### Within Each User Story

- Tests precede implementation.
- Shared contract definitions precede API or verification-note updates.
- Evidence capture is the final task inside each story.

### Parallel Opportunities

- `T002` and `T003` can run in parallel after `T001`.
- `T005`, `T006`, `T007`, and `T008` can run in parallel after `T004`.
- In **US1**, `T010-T013` can run in parallel; `T014` and `T015` can run in parallel once the tests are in place.
- In **US2**, `T018-T020` can run in parallel; `T021` and `T022` can run in parallel.
- In **US3**, `T025-T027` can run in parallel; `T028` and `T029` can run in parallel.
- `T031` and `T032` can run in parallel before `T033`.

---

## Parallel Example: User Story 2

```bash
# Parallel test work for US2:
Task: "T018 [US2] Add unit coverage for NormalizedHit/ResolvedHit/TransformPlan in crates/pokrov-core/src/types/foundation.rs"
Task: "T019 [US2] Add integration proof in tests/integration/sanitization_foundation_shared_contracts.rs"
Task: "T020 [US2] Add performance regression coverage in tests/performance/sanitization_foundation_contract_overhead.rs"

# Parallel implementation work for US2:
Task: "T021 [US2] Add runtime/evaluation-compatible helper fields in crates/pokrov-core/src/types/foundation.rs and crates/pokrov-core/src/types.rs"
Task: "T022 [US2] Implement shared proof support helpers in tests/common/sanitization_foundation_test_support.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup.
2. Complete Phase 2: Foundational.
3. Complete Phase 3: User Story 1.
4. Stop and verify that stage ownership and shared contract exports are stable.

### Incremental Delivery

1. Setup + Foundational -> compile-visible shared contract scaffolding ready.
2. Add US1 -> verify frozen stage ownership.
3. Add US2 -> verify shared runtime/evaluation proof.
4. Add US3 -> verify metadata-only safety and evaluation-boundary rules.
5. Finish with Polish -> capture final evidence and roadmap sync.

### Parallel Team Strategy

1. One engineer owns Phase 2 core contract scaffolding in `crates/pokrov-core`.
2. After Phase 2, a second engineer can own US2 proof/test harness work while the first finishes US1 API/export alignment.
3. US3 can proceed once shared contract families are stable enough to encode metadata-only checks without reworking the core model.

---

## Notes

- `[P]` means the task can run in parallel only if file ownership does not conflict.
- `docs/verification/009-architecture-foundation.md` is the single evidence sink for this feature.
- This feature must not implement recognizer families, remote transports, or dataset governance policy; those belong to later Presidio rework features.
