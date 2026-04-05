# Tasks: Safe Explainability and Audit

**Input**: Design artifacts from `/specs/013-safe-explainability-audit/`  
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/safe-explainability-audit-contract.md`

**Tests**: Test tasks are mandatory for this feature because it changes proxy/policy/security/ops behavior and explicitly requires unit, integration, performance, and security coverage.

**Organization**: Tasks are grouped by user story so each story can be implemented and validated independently.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Task can run in parallel (different files, no direct dependency)
- **[Story]**: User story label (`[US1]`, `[US2]`, `[US3]`)
- Each task includes an explicit file path

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Align feature artifacts and shared verification scaffolding before contract implementation.

- [X] T001 Reconcile approved explain/audit field inventory in `specs/013-safe-explainability-audit/contracts/safe-explainability-audit-contract.md`
- [X] T002 [P] Record reason-code governance decisions in `specs/013-safe-explainability-audit/research.md`
- [X] T003 [P] Confirm entity-to-story mapping and invariants in `specs/013-safe-explainability-audit/data-model.md`
- [X] T004 Define acceptance execution checklist and evidence placeholders in `specs/013-safe-explainability-audit/quickstart.md`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Implement shared contracts and mode policy wiring required by all user stories.

**CRITICAL**: No user story work begins before this phase completes.

- [X] T005 Introduce safe explain/audit shared contract structs in `crates/pokrov-core/src/types/foundation/mod.rs`
- [X] T006 [P] Add reason-code catalog enum/constants and catalog version metadata in `crates/pokrov-core/src/types/foundation/boundaries.rs`
- [X] T007 [P] Add confidence bucket contract and deterministic mapping surface in `crates/pokrov-core/src/types/foundation/transform.rs`
- [X] T008 Export explain/audit foundation contracts from crate root in `crates/pokrov-core/src/types.rs`
- [X] T009 Implement explain/audit failure-policy mode mapping (`fail_closed` vs `fail_open_with_degradation`) in `crates/pokrov-core/src/policy/mod.rs`
- [X] T010 [P] Add profile schema fields and defaults for explain/audit policy controls in `crates/pokrov-config/src/model/sanitization.rs`
- [X] T011 Enforce config validation for reason-code catalog, confidence bucket policy, and failure-mode matrix in `crates/pokrov-config/src/validate.rs`
- [X] T012 [P] Add validation regression tests for explain/audit policy configuration in `crates/pokrov-config/src/validate_tests.rs`
- [X] T013 Add shared explain/audit fixture builders for tests in `tests/common/sanitization_analyzer_contract_test_support.rs`

**Checkpoint**: Foundation complete; all stories can proceed.

---

## Phase 3: User Story 1 - Verify Safe Explain Output (Priority: P1) MVP

**Goal**: Security engineers can inspect deterministic analyzer decisions via metadata-only explain output with no raw-content leakage.

**Independent Test**: Run deterministic recognizer flows and verify explain output contains only approved metadata fields, reason-code catalog entries, and confidence buckets without raw snippets/context.

### Tests for User Story 1

- [X] T014 [P] [US1] Add explain-schema contract regression assertions in `tests/contract/sanitization_foundation_contract.rs`
- [X] T015 [P] [US1] Add explain metadata-only integration scenario across allow/transform/block/degraded outcomes in `tests/integration/sanitization_audit_explain_flow.rs`
- [X] T016 [P] [US1] Add explain leakage-prevention security tests (snippet/context/original-text forbidden) in `tests/security/sanitization_metadata_leakage.rs`
- [X] T017 [P] [US1] Add explain-builder and reason-code fallback unit tests in `crates/pokrov-core/src/dry_run/mod.rs`

### Implementation for User Story 1

- [X] T018 [US1] Implement safe explain summary builder with allowlisted metadata-only fields in `crates/pokrov-core/src/dry_run/mod.rs`
- [X] T019 [P] [US1] Emit deterministic explain reason-code entries from deterministic recognizer stages in `crates/pokrov-core/src/detection/mod.rs`
- [X] T020 [P] [US1] Apply confidence bucket policy and prohibit raw score export in explain output in `crates/pokrov-core/src/transform/mod.rs`
- [X] T021 [US1] Wire explain summary generation into analyzer evaluation result surface in `crates/pokrov-core/src/lib.rs`

**Checkpoint**: US1 is independently functional and verifiable.

---

## Phase 4: User Story 2 - Verify Metadata-Only Audit (Priority: P2)

**Goal**: Operators can trace runtime behavior through metadata-only audit summaries with deterministic counters and no payload leakage.

**Independent Test**: Execute allow/transform/block/degraded flows and verify audit records contain only approved metadata fields and fail according to mode-based policy on explain/audit generation errors.

### Tests for User Story 2

- [X] T022 [P] [US2] Add audit-schema contract regression assertions in `tests/contract/sanitization_evaluate_contract.rs`
- [X] T023 [P] [US2] Add runtime-mode failure handling integration tests for fail-closed vs degraded continue in `tests/integration/sanitization_evaluate_flow.rs`
- [X] T024 [P] [US2] Add audit metadata leakage security assertions across payload-rich inputs in `tests/security/sanitization_foundation_metadata_leakage.rs`
- [X] T025 [P] [US2] Add audit-builder deterministic counters and duration-metadata unit tests in `crates/pokrov-core/src/audit/mod.rs`

### Implementation for User Story 2

- [X] T026 [US2] Implement metadata-only audit summary builder with strict allowlisted fields in `crates/pokrov-core/src/audit/mod.rs`
- [X] T027 [US2] Wire final action/profile/mode counters into audit summaries in analyzer orchestration in `crates/pokrov-core/src/lib.rs`
- [X] T028 [P] [US2] Enforce runtime fail-closed path for explain/audit generation failures in `crates/pokrov-core/src/policy/mod.rs`
- [X] T029 [P] [US2] Enforce evaluation fail-open-with-degradation path and degradation reason propagation in `crates/pokrov-core/src/dry_run/mod.rs`

**Checkpoint**: US2 is independently functional and verifiable.

---

## Phase 5: User Story 3 - Reuse Safe Outputs Across Reports (Priority: P3)

**Goal**: Evaluation/parity reporting reuses the same safe explain/audit contracts without introducing unsafe side channels.

**Independent Test**: Generate evaluation/parity artifacts from analyzer runs and verify reports consume shared safe explain/audit fields only, with no contract expansion.

### Tests for User Story 3

- [X] T030 [P] [US3] Add contract test to enforce shared runtime/evaluation explain-audit shape parity in `tests/contract/sanitization_foundation_contract.rs`
- [X] T031 [P] [US3] Add integration test for evaluation/parity reuse of safe explain/audit outputs in `tests/integration/sanitization_foundation_shared_contracts.rs`
- [X] T032 [P] [US3] Add security test to block raw-content fields in serialized evaluation artifacts in `tests/security/sanitization_metadata_leakage.rs`
- [X] T033 [P] [US3] Add performance test for explain+audit overhead budget (`<=10ms p95`) in `tests/performance/sanitization_foundation_contract_overhead.rs`

### Implementation for User Story 3

- [X] T034 [US3] Expose shared explain/audit contract outputs for runtime and evaluation consumers in `crates/pokrov-core/src/lib.rs`
- [X] T035 [P] [US3] Align evaluation/parity serialization with safe metadata-only explain/audit contracts in `crates/pokrov-core/src/types/foundation/mod.rs`
- [X] T036 [US3] Update proxy integration surfaces to consume shared safe explain/audit metadata contracts in `crates/pokrov-proxy-llm/src/lib.rs`
- [X] T037 [P] [US3] Update MCP mediation integration surface to consume shared safe explain/audit metadata contracts in `crates/pokrov-proxy-mcp/src/lib.rs`

**Checkpoint**: US3 is independently functional and verifiable.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Finalize operational controls, docs, and verification evidence across all stories.

- [X] T038 [P] Add retention-window and deletion-path verification scenario docs in `specs/013-safe-explainability-audit/quickstart.md`
- [X] T039 [P] Add least-privilege access verification notes for security/ops-only readers in `specs/013-safe-explainability-audit/contracts/safe-explainability-audit-contract.md`
- [X] T040 [P] Add end-to-end acceptance evidence checklist for SC-001..SC-007 in `specs/013-safe-explainability-audit/spec.md`
- [X] T041 Run `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features` and record outputs in `specs/013-safe-explainability-audit/tasks.md`
  - 2026-04-05 execution evidence:
    - `cargo test` -> PASS (`contract`: 40 passed, `integration`: 90 passed, `performance`: 10 passed, `security`: 17 passed)
    - `cargo fmt --check` -> PASS
    - `cargo clippy --all-targets --all-features` -> PASS

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: starts immediately
- **Phase 2 (Foundational)**: depends on Phase 1 and blocks all user stories
- **Phase 3 (US1)**: depends on Phase 2 and defines MVP scope
- **Phase 4 (US2)**: depends on Phase 2; can proceed after US1 core contracts are available
- **Phase 5 (US3)**: depends on Phase 2 and requires US1/US2 contract surfaces to be stable
- **Phase 6 (Polish)**: depends on completion of US1-US3

### User Story Dependencies

- **US1 (P1)**: no user-story dependency; first deliverable and MVP
- **US2 (P2)**: depends on foundational contracts; independent acceptance from US3
- **US3 (P3)**: depends on foundational contracts plus shared explain/audit outputs implemented in US1/US2

### Within Each User Story

- Implement tests before feature code changes
- Build/lock contracts before orchestration wiring
- Enforce failure-mode semantics before final integration
- Story is complete only after independent test criteria passes

### Parallel Opportunities

- Foundational tasks `T006`, `T007`, `T010`, `T012` can run in parallel
- US1 tests `T014`-`T017` can run in parallel; implementation tasks `T019` and `T020` can run in parallel after `T018`
- US2 tests `T022`-`T025` can run in parallel; tasks `T028` and `T029` can run in parallel after `T026`
- US3 tests `T030`-`T033` can run in parallel; tasks `T035` and `T037` can run in parallel after `T034`

---

## Parallel Example: User Story 1

```bash
# Parallel test work
Task: "T014 [US1] explain-schema contract regression in tests/contract/sanitization_foundation_contract.rs"
Task: "T015 [US1] explain metadata-only integration in tests/integration/sanitization_audit_explain_flow.rs"
Task: "T016 [US1] leakage-prevention assertions in tests/security/sanitization_metadata_leakage.rs"

# Parallel implementation after explain builder skeleton exists
Task: "T019 [US1] deterministic reason-code emission in crates/pokrov-core/src/detection/mod.rs"
Task: "T020 [US1] confidence bucket mapping in crates/pokrov-core/src/transform/mod.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 and Phase 2
2. Complete US1 (Phase 3)
3. Verify US1 independent test criteria and evidence
4. Freeze MVP explain contract before moving to US2/US3

### Incremental Delivery

1. Foundation contracts and validation (`T005`-`T013`)
2. Metadata-only explain output (US1)
3. Metadata-only audit output and failure policy (US2)
4. Runtime/evaluation contract reuse and performance proof (US3)
5. Polish + final verification gates (`T038`-`T041`)

### Parallel Team Strategy

1. Engineer A owns config and validation (`T010`-`T012`)
2. Engineer B owns explain/audit core builders (`T018`, `T026`, `T034`)
3. Engineer C owns test suites and acceptance evidence (`T014`-`T017`, `T022`-`T025`, `T030`-`T033`, `T041`)

---

## Notes

- `[P]` means task is parallelizable with no unresolved dependency on the same file set.
- `[USx]` links tasks directly to a single user story.
- Every user story is independently completable and testable.
