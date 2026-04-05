# Tasks: Analyzer Core Contract For Presidio Rework

**Input**: Design artifacts from `specs/010-analyzer-core-contract/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md`

**Tests**: Test tasks are mandatory for this feature because it freezes shared analyzer/policy/security contracts and requires unit, integration, performance, and security evidence.

**Organization**: Tasks are grouped by user story so each story can be implemented and validated independently.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel if files do not overlap and no incomplete dependency is required
- **[Story]**: Maps the task to a user story from `spec.md` (`US1`, `US2`, `US3`)
- Every task includes exact file paths

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare verification artifacts and shared test scaffolding for the analyzer-contract milestone.

- [X] T001 Create analyzer-contract verification note scaffold in `docs/verification/010-analyzer-core-contract.md`
- [X] T002 [P] Add shared analyzer contract test helpers in `tests/common/sanitization_analyzer_contract_test_support.rs`
- [X] T003 [P] Wire analyzer-contract suites into `tests/contract.rs`, `tests/integration.rs`, `tests/security.rs`, and `tests/performance.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Establish canonical shared analyzer request/result contracts in `pokrov-core` before story-level adapter work.

**CRITICAL**: No user story work starts before this phase is complete.

- [X] T004 Define analyzer-core contract module layout and exports in `crates/pokrov-core/src/lib.rs` and `crates/pokrov-core/src/types.rs`
- [X] T005 [P] Add canonical request fields (including mandatory `effective_language` and optional filters) in `crates/pokrov-core/src/types.rs`
- [X] T006 [P] Add canonical result sections (`decision`, `transform`, `explain`, `audit`, `executed`, `degraded`) in `crates/pokrov-core/src/types.rs`
- [X] T007 [P] Implement unified resolved-location record shape for text and structured paths in `crates/pokrov-core/src/types/foundation/mod.rs`
- [X] T008 [P] Extend metadata-only explain/audit builders for new executed/degraded fields in `crates/pokrov-core/src/audit/mod.rs`
- [X] T009 Document foundational analyzer-contract checkpoints in `docs/verification/010-analyzer-core-contract.md`

**Checkpoint**: Canonical analyzer contract is compile-visible in `pokrov-core`, and downstream stories can reuse it without contract forks.

---

## Phase 3: User Story 1 - Freeze One Analyzer Contract For All Consumers (Priority: P1) MVP

**Goal**: Provide one shared analyzer request/result family that runtime and evaluation consumers can reuse directly.

**Independent Test**: Verify one runtime-oriented and one evaluation-oriented flow consume the same top-level analyzer sections without redefining local result families.

### Tests for User Story 1

- [X] T010 [P] [US1] Add unit tests for analyzer request/result construction invariants in `crates/pokrov-core/src/types.rs`
- [X] T011 [P] [US1] Add contract coverage for required top-level analyzer sections in `tests/contract/sanitization_evaluate_contract.rs`
- [X] T012 [P] [US1] Add integration compatibility proof for shared runtime/evaluation contract reuse in `tests/integration/sanitization_foundation_shared_contracts.rs`
- [X] T013 [P] [US1] Add performance check for shared-contract reuse without duplicate conversion layers in `tests/performance/sanitization_foundation_contract_overhead.rs`
- [X] T014 [P] [US1] Add security assertions that shared sections remain metadata-only outside transform payload in `tests/security/sanitization_foundation_metadata_leakage.rs`

### Implementation for User Story 1

- [X] T015 [P] [US1] Implement canonical analyzer request/result data structures in `crates/pokrov-core/src/types.rs`
- [X] T016 [P] [US1] Implement serialization-safe shared contract helpers for runtime/evaluation consumers in `crates/pokrov-core/src/types/foundation/mod.rs`
- [X] T017 [US1] Update evaluate handler to consume canonical analyzer contract without local result forks in `crates/pokrov-api/src/handlers/evaluate.rs`
- [X] T018 [US1] Record runtime/evaluation shared-contract evidence in `docs/verification/010-analyzer-core-contract.md`

**Checkpoint**: User Story 1 is independently verifiable with one shared analyzer contract reused by runtime and evaluation paths.

---

## Phase 4: User Story 2 - Distinguish Policy Blocks From Analyzer Failures (Priority: P2)

**Goal**: Preserve clear contract semantics where policy `block` is a successful analyzer result and true analyzer failures remain errors.

**Independent Test**: Execute one block scenario and one analyzer-error scenario and verify consumers use different handling paths without local ambiguity.

### Tests for User Story 2

- [X] T019 [P] [US2] Add unit tests for block-vs-error and degraded-fail-closed invariants in `crates/pokrov-core/src/lib.rs`
- [X] T020 [P] [US2] Add integration coverage for evaluate block and analyzer-error separation in `tests/integration/sanitization_evaluate_flow.rs`
- [X] T021 [P] [US2] Add integration coverage proving runtime adapter block handling does not map to analyzer error in `tests/integration/llm_proxy_block_path.rs`
- [X] T022 [P] [US2] Add security test for metadata-safe block/error/degraded outputs in `tests/security/sanitization_metadata_leakage.rs`

### Implementation for User Story 2

- [X] T023 [P] [US2] Implement explicit analyzer error classification (`InvalidInput`, `InvalidProfile`, `RuntimeFailure`) in `crates/pokrov-core/src/types.rs`
- [X] T024 [P] [US2] Implement policy block as successful result with full shared sections in `crates/pokrov-core/src/lib.rs`
- [X] T025 [US2] Update LLM proxy adapter mapping to preserve shared block-vs-error semantics in `crates/pokrov-proxy-llm/src/lib.rs` and `crates/pokrov-proxy-llm/src/audit.rs`
- [X] T026 [US2] Update MCP proxy adapter mapping to preserve shared block-vs-error semantics in `crates/pokrov-proxy-mcp/src/lib.rs` and `crates/pokrov-proxy-mcp/src/audit.rs`
- [X] T027 [US2] Record block/error contract evidence in `docs/verification/010-analyzer-core-contract.md`

**Checkpoint**: User Story 2 is independently verifiable: policy enforcement outcomes are distinct from analyzer failures across consumers.

---

## Phase 5: User Story 3 - Preserve Deterministic And Metadata-Only Outcomes Across Modes (Priority: P3)

**Goal**: Guarantee deterministic replay identity and metadata-only explain/audit/executed/degraded outputs across text, structured JSON, and degraded paths.

**Independent Test**: Verify repeated identical inputs produce the same replay identity and that non-transform sections never expose raw payload fragments.

### Tests for User Story 3

- [X] T028 [P] [US3] Add unit tests for deterministic ordering and replay identity stability in `crates/pokrov-core/src/types/foundation/mod.rs`
- [X] T029 [P] [US3] Add integration test for structured JSON unified location records in `tests/integration/sanitization_foundation_evaluation_boundary.rs`
- [X] T030 [P] [US3] Add performance regression test for replay-identity stability without extra hot-path recomputation in `tests/performance/sanitization_evaluate_latency.rs`
- [X] T031 [P] [US3] Add security test for metadata-only executed/degraded outputs across degradation paths in `tests/security/sanitization_foundation_metadata_leakage.rs`

### Implementation for User Story 3

- [X] T032 [P] [US3] Implement deterministic same-span/same-score ordering and replay identity generation in `crates/pokrov-core/src/lib.rs` and `crates/pokrov-core/src/types/foundation/mod.rs`
- [X] T033 [P] [US3] Implement unified location population for plain text and structured JSON flows in `crates/pokrov-core/src/traversal/mod.rs` and `crates/pokrov-core/src/types/foundation/mod.rs`
- [X] T034 [P] [US3] Implement executed/degraded metadata sections with fail-closed markers for missing required evidence in `crates/pokrov-core/src/audit/mod.rs` and `crates/pokrov-core/src/types.rs`
- [X] T035 [US3] Record deterministic replay and metadata-safety evidence in `docs/verification/010-analyzer-core-contract.md`

**Checkpoint**: User Story 3 is independently verifiable: deterministic replay and metadata-only guarantees hold across supported modes.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Align docs/contracts and capture final verification evidence across all stories.

- [X] T036 [P] Update feature docs alignment in `specs/010-analyzer-core-contract/quickstart.md`, `specs/010-analyzer-core-contract/contracts/analyzer-surface.md`, and `specs/010-analyzer-core-contract/contracts/consumer-compatibility.md`
- [X] T037 [P] Sync rework roadmap references in `docs/superpowers/plans/presidio-rework/master-roadmap.md` and `docs/superpowers/plans/presidio-rework/01-analyzer-core-contract-backlog.md`
- [X] T038 Run final verification commands and record command outputs in `docs/verification/010-analyzer-core-contract.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1: Setup** starts immediately.
- **Phase 2: Foundational** depends on Phase 1 and blocks all user stories.
- **Phase 3: US1** depends on Phase 2.
- **Phase 4: US2** depends on Phase 2 and should reuse US1 canonical contract outputs.
- **Phase 5: US3** depends on Phase 2 and should build on US1/US2 semantics.
- **Phase 6: Polish** depends on completion of intended user stories.

### User Story Dependencies

- **US1 (P1)** is the MVP and should land first.
- **US2 (P2)** depends on the shared request/result contracts introduced in US1.
- **US3 (P3)** depends on shared contracts from US1 and semantic separation from US2.

### Within Each User Story

- Tests precede implementation tasks.
- Core contract/types changes precede adapter mapping changes.
- Verification evidence capture is the final task in each story.

### Parallel Opportunities

- `T002` and `T003` can run in parallel after `T001`.
- `T005`, `T006`, `T007`, and `T008` can run in parallel after `T004`.
- In **US1**, `T010-T014` can run in parallel; `T015` and `T016` can run in parallel.
- In **US2**, `T019-T022` can run in parallel; `T023` and `T024` can run in parallel.
- In **US3**, `T028-T031` can run in parallel; `T032`, `T033`, and `T034` can run in parallel where file ownership is coordinated.
- `T036` and `T037` can run in parallel before `T038`.

---

## Parallel Example: User Story 1

```bash
# Parallel test work for US1:
Task: "T010 [US1] Add unit tests in crates/pokrov-core/src/types.rs"
Task: "T011 [US1] Add contract coverage in tests/contract/sanitization_evaluate_contract.rs"
Task: "T012 [US1] Add integration compatibility proof in tests/integration/sanitization_foundation_shared_contracts.rs"
Task: "T014 [US1] Add security assertions in tests/security/sanitization_foundation_metadata_leakage.rs"

# Parallel implementation work for US1:
Task: "T015 [US1] Implement canonical analyzer request/result structures in crates/pokrov-core/src/types.rs"
Task: "T016 [US1] Implement serialization-safe shared contract helpers in crates/pokrov-core/src/types/foundation/mod.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup.
2. Complete Phase 2: Foundational.
3. Complete Phase 3: User Story 1.
4. Stop and verify one shared runtime/evaluation analyzer contract family.

### Incremental Delivery

1. Setup + Foundational -> canonical analyzer contract is compile-visible.
2. Add US1 -> verify shared contract reuse.
3. Add US2 -> verify policy block versus analyzer error semantics.
4. Add US3 -> verify deterministic replay and metadata-only guarantees.
5. Finish with Polish -> capture final verification evidence.

### Parallel Team Strategy

1. One engineer owns Phase 2 `pokrov-core` contract scaffolding (`types`, `foundation`, `audit`).
2. After Phase 2, one engineer can own US2 adapter semantics (`pokrov-proxy-llm`, `pokrov-proxy-mcp`) while another delivers US3 deterministic and metadata-only guarantees in `pokrov-core`.
3. Final polish is coordinated after all mandatory test gates pass.

---

## Notes

- `[P]` means task-level parallelism is safe only with non-overlapping file ownership.
- `docs/verification/010-analyzer-core-contract.md` is the single evidence sink for this feature.
- This feature freezes analyzer contract semantics only; recognizer-family expansion, remote transport behavior, and external wire APIs remain out of scope.
