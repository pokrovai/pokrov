# Tasks: Operator Semantics Freeze

**Input**: Design artifacts from `/specs/012-operator-semantics/`  
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/operator-semantics-contract.md`

**Tests**: Test tasks are mandatory for this feature because it changes proxy/policy/security hot paths.

**Organization**: Tasks are grouped by user story so each story can be implemented and validated independently.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Task can run in parallel (different files, no direct dependency)
- **[Story]**: User story label (`[US1]`, `[US2]`, `[US3]`)
- Each task includes an explicit file path

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare feature documentation artifacts and shared test scaffolding alignment.

- [X] T001 Align implementation scope and invariants in `specs/012-operator-semantics/spec.md`
- [X] T002 [P] Capture operator-semantics execution notes in `specs/012-operator-semantics/research.md`
- [X] T003 [P] Verify data-model coverage for transform outcomes in `specs/012-operator-semantics/data-model.md`
- [X] T004 Add operator-semantics quick verification flow in `specs/012-operator-semantics/quickstart.md`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Implement shared contracts and core wiring required by all user stories.

**CRITICAL**: No user story work begins before this phase completes.

- [X] T005 Introduce supported-operator enum contract and validation helpers in `crates/pokrov-core/src/types/foundation/transform.rs`
- [X] T006 [P] Extend transform outcome metadata contract in `crates/pokrov-core/src/types/foundation/mod.rs`
- [X] T007 [P] Wire deterministic operator contract exports in `crates/pokrov-core/src/types.rs`
- [X] T008 Add profile-level operator validation hooks in `crates/pokrov-config/src/model/sanitization.rs`
- [X] T009 Enforce unsupported-operator fail-closed config validation in `crates/pokrov-config/src/validate.rs`
- [X] T010 [P] Add config validation regression coverage for operator constraints in `crates/pokrov-config/src/validate_tests.rs`
- [X] T011 Add shared transform-plan construction entrypoint in `crates/pokrov-core/src/transform/mod.rs`
- [X] T012 [P] Add shared test fixtures for operator semantics in `tests/common/sanitization_foundation_test_support.rs`

**Checkpoint**: Foundation complete; user stories can proceed in priority order.

---

## Phase 3: User Story 1 - Deterministic Operator Outcomes (Priority: P1) MVP

**Goal**: Identical resolved-hit input and profile produce identical transform output and metadata with stable overlap behavior.

**Independent Test**: Replay identical resolved-hit sets and verify byte-equivalent payload + metadata with no suppressed-span reapplication.

### Tests for User Story 1

- [X] T013 [P] [US1] Add deterministic operator contract assertions in `tests/contract/sanitization_foundation_contract.rs`
- [X] T014 [P] [US1] Add integration replay determinism scenario in `tests/integration/sanitization_transform_flow.rs`
- [X] T015 [P] [US1] Add unit tests for deterministic operator ordering in `crates/pokrov-core/src/transform/mod.rs`
- [X] T016 [P] [US1] Add overlap suppression non-reapply regression test in `crates/pokrov-core/src/detection/mod.rs`

### Implementation for User Story 1

- [X] T017 [US1] Implement deterministic application ordering for resolved hits in `crates/pokrov-core/src/transform/mod.rs`
- [X] T018 [P] [US1] Implement explicit `replace` and `redact` stable semantics in `crates/pokrov-core/src/transform/mod.rs`
- [X] T019 [P] [US1] Implement explicit `mask` deterministic visibility semantics in `crates/pokrov-core/src/transform/mod.rs`
- [X] T020 [US1] Implement one-way deterministic `hash` semantics in `crates/pokrov-core/src/transform/mod.rs`
- [X] T021 [US1] Emit deterministic operator decision summary metadata in `crates/pokrov-core/src/audit/mod.rs`

**Checkpoint**: US1 is independently functional and verifiable.

---

## Phase 4: User Story 2 - Clear Block vs Transform Semantics (Priority: P2)

**Goal**: Terminal `block` outcomes never forward payload while still emitting safe metadata-only explain/audit evidence.

**Independent Test**: Force unsupported operator and policy block paths; verify downstream payload omission and metadata-only outputs.

### Tests for User Story 2

- [X] T022 [P] [US2] Add block-vs-transform contract coverage in `tests/contract/sanitization_evaluate_contract.rs`
- [X] T023 [P] [US2] Add blocked outcome integration scenario in `tests/integration/sanitization_audit_explain_flow.rs`
- [X] T024 [P] [US2] Add metadata-only leakage assertions for blocked outcomes in `tests/security/sanitization_metadata_leakage.rs`
- [X] T025 [P] [US2] Add unsupported-operator reason-code path test in `tests/integration/sanitization_evaluate_flow.rs`

### Implementation for User Story 2

- [X] T026 [US2] Implement fail-closed `unsupported_operator` -> terminal `block` behavior in `crates/pokrov-core/src/policy/mod.rs`
- [X] T027 [US2] Prevent payload forwarding for blocked outcomes in `crates/pokrov-core/src/lib.rs`
- [X] T028 [P] [US2] Emit metadata-only blocked explain summary in `crates/pokrov-core/src/dry_run/mod.rs`
- [X] T029 [P] [US2] Emit metadata-only blocked audit summary and explicit `keep` markers in `crates/pokrov-core/src/audit/mod.rs`

**Checkpoint**: US2 is independently functional and verifiable.

---

## Phase 5: User Story 3 - JSON-Safe Structured Processing (Priority: P3)

**Goal**: Non-blocking transforms mutate only string leaves while preserving JSON structure and non-string values.

**Independent Test**: Run nested mixed-type JSON payloads through transform and verify valid JSON plus unchanged non-string leaves.

### Tests for User Story 3

- [X] T030 [P] [US3] Add nested JSON leaf-mutation integration test in `tests/integration/sanitization_foundation_shared_contracts.rs`
- [X] T031 [P] [US3] Add JSON-validity regression coverage in `tests/integration/sanitization_foundation_stage_boundaries.rs`
- [X] T032 [P] [US3] Add non-string leaf immutability unit tests in `crates/pokrov-core/src/traversal/mod.rs`
- [X] T033 [P] [US3] Add overhead-budget regression scenario for structured transform in `tests/performance/sanitization_foundation_contract_overhead.rs`

### Implementation for User Story 3

- [X] T034 [US3] Enforce string-leaf-only transform traversal in `crates/pokrov-core/src/traversal/mod.rs`
- [X] T035 [US3] Preserve object/array and non-string leaf invariants in `crates/pokrov-core/src/transform/mod.rs`
- [X] T036 [US3] Propagate structured-path metadata for transformed leaves in `crates/pokrov-core/src/types/foundation/boundaries.rs`

**Checkpoint**: US3 is independently functional and verifiable.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Finalize docs, evidence, and full validation gates across all stories.

- [X] T037 [P] Update operator semantics contract documentation in `specs/012-operator-semantics/contracts/operator-semantics-contract.md`
- [X] T038 [P] Update feature acceptance evidence notes in `specs/012-operator-semantics/quickstart.md`
- [X] T039 Run full verification gates and record output references in `specs/012-operator-semantics/tasks.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: starts immediately
- **Phase 2 (Foundational)**: depends on Phase 1 and blocks all stories
- **Phase 3 (US1)**: starts after Phase 2 and defines MVP
- **Phase 4 (US2)**: starts after Phase 2; may run in parallel with late US1 stabilization tasks if files do not overlap
- **Phase 5 (US3)**: starts after Phase 2; should merge after US1 transform core stabilizes
- **Phase 6 (Polish)**: starts after US1-US3 completion

### User Story Dependencies

- **US1 (P1)**: no user-story dependency; required for MVP
- **US2 (P2)**: depends on foundational contracts; independent from US3
- **US3 (P3)**: depends on foundational contracts; independent from US2

### Within Each User Story

- Write/extend tests first, then implement behavior
- Types/contracts before orchestration logic
- Core transform/policy changes before audit/explain polish
- Story closes only when independent test criteria passes

### Parallel Opportunities

- Phase 2 tasks marked `[P]` can run in parallel (`T006`, `T007`, `T010`, `T012`)
- US1 test tasks (`T013`-`T016`) can run in parallel
- US2 test tasks (`T022`-`T025`) and metadata tasks (`T028`, `T029`) can run in parallel
- US3 tests (`T030`-`T033`) can run in parallel; `T036` can run after traversal contract stabilizes

---

## Parallel Example: User Story 2

```bash
# Parallel tests
Task: "T022 [US2] contract coverage in tests/contract/sanitization_evaluate_contract.rs"
Task: "T023 [US2] integration scenario in tests/integration/sanitization_audit_explain_flow.rs"
Task: "T024 [US2] security leakage assertions in tests/security/sanitization_metadata_leakage.rs"

# Parallel implementation after core block path is wired
Task: "T028 [US2] blocked explain summary in crates/pokrov-core/src/dry_run/mod.rs"
Task: "T029 [US2] blocked audit summary in crates/pokrov-core/src/audit/mod.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 and Phase 2
2. Complete US1 (Phase 3)
3. Verify deterministic replay and overlap behavior evidence
4. Release MVP behavior before starting US2/US3 hardening

### Incremental Delivery

1. Foundation contracts (`T005`-`T012`)
2. Deterministic operator semantics (US1)
3. Terminal block semantics and metadata safety (US2)
4. Structured JSON invariants and performance confirmation (US3)
5. Final polish and full verification gates

### Parallel Team Strategy

1. One engineer owns `pokrov-config` validation tasks (`T008`-`T010`)
2. One engineer owns core transform ordering/operator semantics (`T011`, `T017`-`T021`, `T035`)
3. One engineer owns integration/security/performance suites (`T013`-`T016`, `T022`-`T025`, `T030`-`T033`)

---

## Notes

- `[P]` means task is parallelizable with no unresolved file dependency.
- `[USx]` tags map each task to a single user story for traceability.
- Every user story is independently testable and deliverable.

## Verification Evidence

- 2026-04-05: `cargo test --test contract --test integration --test security --test performance` -> PASS (`contract`: 40 passed, `integration`: 90 passed, `security`: 17 passed, `performance`: 10 passed).
- 2026-04-05: `cargo test` -> PASS (workspace tests green).
- 2026-04-05: `cargo fmt --check` -> FAIL (pre-existing formatting drift in `crates/pokrov-config/src/model/sanitization.rs` and `crates/pokrov-core/src/detection/*`; no formatting changes applied per constraints).
- 2026-04-05: `cargo clippy --all-targets --all-features` -> FAIL (pre-existing rustc artifact mismatch in `target/` compiled with rustc 1.91.1 while current toolchain is rustc 1.94.1; requires clean rebuild before clippy can complete).
