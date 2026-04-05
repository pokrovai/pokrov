# Tasks: Structured JSON Processing

**Input**: Design artifacts from `specs/014-structured-json-processing/`  
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/structured-json-processing-contract.md`

**Tests**: Test tasks are mandatory for this feature because it changes proxy/policy/security/ops behavior and explicitly requires unit, integration, performance, and security coverage.

**Organization**: Tasks are grouped by user story so each story can be implemented and validated independently.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Task can run in parallel (different files, no direct dependency)
- **[Story]**: User story label (`[US1]`, `[US2]`, `[US3]`)
- Each task includes an explicit file path

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Align feature documentation, fixtures, and execution scaffolding before shared code changes.

- [X] T001 Finalize structured JSON behavior scope and acceptance mappings in `specs/014-structured-json-processing/spec.md`
- [X] T002 [P] Freeze traversal/precedence/safety rules in `specs/014-structured-json-processing/contracts/structured-json-processing-contract.md`
- [X] T003 [P] Confirm entity and invariant mapping for stories in `specs/014-structured-json-processing/data-model.md`
- [X] T004 Prepare feature verification workflow and evidence checklist in `specs/014-structured-json-processing/quickstart.md`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Introduce shared structured-processing contracts and policy/config plumbing required by all stories.

**CRITICAL**: No user story work begins before this phase completes.

- [X] T005 Add shared structured traversal context types in `crates/pokrov-core/src/types/foundation/mod.rs`
- [X] T006 [P] Add path-safe path-class and structured summary boundary types in `crates/pokrov-core/src/types/foundation/boundaries.rs`
- [X] T007 [P] Add structured transform contract extensions for leaf-level accounting in `crates/pokrov-core/src/types/foundation/transform.rs`
- [X] T008 Export structured processing foundation contracts in `crates/pokrov-core/src/types.rs`
- [X] T009 Add structured path binding configuration model (pointer/alias/subtree/precedence) in `crates/pokrov-config/src/model/sanitization.rs`
- [X] T010 [P] Add size and failure policy config fields (`<=1MB` SLA mode, `>1MB` best-effort, high-risk fail-closed) in `crates/pokrov-config/src/model/sanitization.rs`
- [X] T011 Enforce validation rules for path-binding precedence and structured policy semantics in `crates/pokrov-config/src/validate.rs`
- [X] T012 [P] Add config validation regression coverage for structured bindings and size/failure policy in `crates/pokrov-config/src/validate_tests.rs`
- [X] T013 Add structured analyzer fixture builders for nested payload cases in `tests/common/sanitization_analyzer_contract_test_support.rs`

**Checkpoint**: Foundation complete; all stories can proceed.

---

## Phase 3: User Story 1 - Deterministic Structured Detection (Priority: P1) MVP

**Goal**: Security operators can run deterministic nested JSON detection over string leaves while preserving payload structure.

**Independent Test**: Run nested JSON fixtures repeatedly and verify deterministic traversal order, string-leaf-only analysis, and unchanged non-string leaves.

### Tests for User Story 1

- [X] T014 [P] [US1] Add contract tests for deterministic traversal and string-leaf-only detection in `tests/contract/sanitization_foundation_contract.rs`
- [X] T015 [P] [US1] Add integration tests for nested traversal happy path and mixed leaf types in `tests/integration/sanitization_transform_flow.rs`
- [X] T016 [P] [US1] Add security tests ensuring non-string values and raw payload fragments are never leaked during structured detection in `tests/security/sanitization_metadata_leakage.rs`
- [X] T017 [P] [US1] Add unit tests for traversal order and empty-string handling in `crates/pokrov-core/src/traversal/mod.rs`

### Implementation for User Story 1

- [X] T018 [US1] Implement deterministic structured traversal over objects/arrays with stable leaf visitation in `crates/pokrov-core/src/traversal/mod.rs`
- [X] T019 [P] [US1] Implement per-leaf string detection entrypoint and context propagation in `crates/pokrov-core/src/detection/mod.rs`
- [X] T020 [P] [US1] Ensure transform pipeline mutates only string leaves and preserves JSON shape in `crates/pokrov-core/src/transform/mod.rs`
- [X] T021 [US1] Wire structured traversal + detection flow into analyzer orchestration in `crates/pokrov-core/src/lib.rs`
- [X] T022 [US1] Integrate structured detection path into LLM proxy request flow in `crates/pokrov-proxy-llm/src/handler/mod.rs`
- [X] T023 [P] [US1] Integrate structured detection path into MCP mediation flow in `crates/pokrov-proxy-mcp/src/handler.rs`

**Checkpoint**: US1 is independently functional and verifiable.

---

## Phase 4: User Story 2 - Path-Aware Policy Semantics (Priority: P2)

**Goal**: Policy operators can configure path-aware includes/excludes/operator defaults and get deterministic precedence resolution.

**Independent Test**: Submit payloads with identical strings in different paths and verify precedence-driven action differences and fail-closed behavior on high-risk failures.

### Tests for User Story 2

- [X] T024 [P] [US2] Add contract tests for pointer/alias/subtree precedence order enforcement in `tests/contract/sanitization_evaluate_contract.rs`
- [X] T025 [P] [US2] Add integration tests for path-specific overrides and conflicting rule resolution in `tests/integration/sanitization_evaluate_flow.rs`
- [X] T026 [P] [US2] Add integration test for payload-size mode split (`<=1MB` SLA mode, `>1MB` best-effort) in `tests/integration/llm_proxy_body_limit_path.rs`
- [X] T027 [P] [US2] Add security test for fail-closed behavior on high-risk structured processing errors in `tests/security/sanitization_foundation_stage_ownership.rs`
- [X] T028 [P] [US2] Add unit tests for precedence resolver and rule conflict handling in `crates/pokrov-core/src/policy/mod.rs`

### Implementation for User Story 2

- [X] T029 [US2] Implement path-aware recognizer include/exclude binding resolution in `crates/pokrov-core/src/policy/mod.rs`
- [X] T030 [P] [US2] Implement deterministic precedence engine (pointer -> alias -> subtree -> profile -> global) in `crates/pokrov-core/src/policy/mod.rs`
- [X] T031 [P] [US2] Implement size-policy execution mode handling and SLA classification in `crates/pokrov-core/src/lib.rs`
- [X] T032 [US2] Implement high-risk structured processing fail-closed decision path in `crates/pokrov-core/src/policy/mod.rs`
- [X] T033 [US2] Propagate resolved path policy context into transform operator selection in `crates/pokrov-core/src/transform/mod.rs`

**Checkpoint**: US2 is independently functional and verifiable.

---

## Phase 5: User Story 3 - Metadata-Only Structured Summaries (Priority: P3)

**Goal**: Operations engineers can inspect structured explain/audit summaries using only path-safe metadata with zero raw value or exact-pointer leakage.

**Independent Test**: Generate explain/audit outputs for allow/mask/redact/block and degraded flows, then verify only safe counters/categories/path classes are present.

### Tests for User Story 3

- [X] T034 [P] [US3] Add contract tests to forbid exact JSON pointer and raw values in structured explain/audit outputs in `tests/contract/sanitization_foundation_contract.rs`
- [X] T035 [P] [US3] Add integration tests for structured summary generation across allow/mask/redact/block outcomes in `tests/integration/sanitization_audit_explain_flow.rs`
- [X] T036 [P] [US3] Add security tests for metadata-only summary leakage prevention in `tests/security/sanitization_foundation_metadata_leakage.rs`
- [X] T037 [P] [US3] Add performance test for structured mode overhead budget on `<=1MB` payloads in `tests/performance/sanitization_evaluate_latency.rs`
- [X] T038 [P] [US3] Add unit tests for path-safe category aggregation and summary determinism in `crates/pokrov-core/src/audit/mod.rs`

### Implementation for User Story 3

- [X] T039 [US3] Implement structured explain summary builder with path-safe categories only in `crates/pokrov-core/src/dry_run/mod.rs`
- [X] T040 [US3] Implement structured audit summary builder with metadata-only counters and safe path classes in `crates/pokrov-core/src/audit/mod.rs`
- [X] T041 [P] [US3] Enforce exact-pointer stripping/redaction from explain/audit serialization in `crates/pokrov-core/src/types/foundation/boundaries.rs`
- [X] T042 [US3] Wire structured summary outputs into shared analyzer result contracts in `crates/pokrov-core/src/lib.rs`
- [X] T043 [P] [US3] Consume structured safe summaries in LLM proxy audit integration in `crates/pokrov-proxy-llm/src/audit.rs`
- [X] T044 [P] [US3] Consume structured safe summaries in MCP proxy audit integration in `crates/pokrov-proxy-mcp/src/audit.rs`

**Checkpoint**: US3 is independently functional and verifiable.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Finalize docs, evidence, and full verification across all stories.

- [X] T045 [P] Update structured processing operational guidance and examples in `docs/superpowers/specs/presidio-rework/05-structured-json-processing.md`
- [X] T046 [P] Update structured processing backlog traceability after implementation planning in `docs/superpowers/plans/presidio-rework/05-structured-json-processing-backlog.md`
- [X] T047 [P] Add final acceptance evidence checklist for SC-001..SC-007 in `specs/014-structured-json-processing/spec.md`
- [X] T048 Run `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets --all-features` and record outcomes in `specs/014-structured-json-processing/tasks.md`

### T048 Verification Evidence (2026-04-05)

- `cargo test` -> PASS (contract/integration/performance/security suites passed)
- `cargo fmt --check` -> PASS
- `cargo clippy --all-targets --all-features` -> PASS

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: starts immediately
- **Phase 2 (Foundational)**: depends on Phase 1 and blocks all user stories
- **Phase 3 (US1)**: depends on Phase 2 and defines MVP scope
- **Phase 4 (US2)**: depends on Phase 2 and reuses US1 structured traversal outputs
- **Phase 5 (US3)**: depends on Phase 2 and consumes US1/US2 policy/transform outputs
- **Phase 6 (Polish)**: depends on completion of US1-US3

### User Story Dependencies

- **US1 (P1)**: no user-story dependency; first deliverable and MVP
- **US2 (P2)**: depends on foundational contracts; independent acceptance from US3
- **US3 (P3)**: depends on foundational contracts and stabilized structured traversal/policy outputs from US1 and US2

### Within Each User Story

- Tests are defined before implementation tasks
- Contract and policy semantics are implemented before proxy integration wiring
- Security-sensitive behavior (fail-closed, leakage prevention) is implemented before polish
- Story completion requires independent test pass and acceptance evidence

### Parallel Opportunities

- Foundational tasks `T006`, `T007`, `T010`, and `T012` can run in parallel
- US1 tests `T014`-`T017` can run in parallel; implementation tasks `T019`, `T020`, and `T023` can run in parallel after `T018`
- US2 tests `T024`-`T028` can run in parallel; implementation tasks `T030` and `T031` can run in parallel after `T029`
- US3 tests `T034`-`T038` can run in parallel; implementation tasks `T041`, `T043`, and `T044` can run in parallel after `T039` and `T040`

---

## Parallel Example: User Story 2

```bash
# Parallel tests for path-aware policy behavior
Task: "T024 [US2] precedence contract tests in tests/contract/sanitization_evaluate_contract.rs"
Task: "T025 [US2] path override integration tests in tests/integration/sanitization_evaluate_flow.rs"
Task: "T027 [US2] fail-closed security tests in tests/security/sanitization_foundation_stage_ownership.rs"

# Parallel implementation after policy skeleton is in place
Task: "T030 [US2] precedence engine in crates/pokrov-core/src/policy/mod.rs"
Task: "T031 [US2] size-mode policy in crates/pokrov-core/src/lib.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 and Phase 2
2. Complete US1 (Phase 3)
3. Verify US1 independent test criteria and evidence
4. Freeze traversal and shape-preservation behavior before US2/US3

### Incremental Delivery

1. Foundation contracts and config validation (`T005`-`T013`)
2. Deterministic structured detection and transformation (US1)
3. Path-aware policy precedence and high-risk failure handling (US2)
4. Metadata-only structured explain/audit summaries (US3)
5. Polish and final verification gates (`T045`-`T048`)

### Parallel Team Strategy

1. Engineer A owns config and validation (`T009`-`T012`)
2. Engineer B owns core traversal/policy/transform (`T018`-`T033`)
3. Engineer C owns explain/audit summaries and security/performance tests (`T034`-`T044`, `T048`)

---

## Notes

- `[P]` means task is parallelizable with no unresolved dependency on the same file set.
- `[USx]` links tasks directly to a single user story.
- Every user story is independently completable and testable.
