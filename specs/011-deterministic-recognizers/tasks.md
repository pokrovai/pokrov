# Tasks: Deterministic Recognizers

**Input**: Design artifacts from `/specs/011-deterministic-recognizers/`  
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`

**Tests**: Test tasks are mandatory for this feature because it changes analyzer, policy, security, observability, and performance-sensitive runtime behavior.

**Organization**: Tasks are grouped by user story so each story can be implemented and verified independently once foundational work is complete.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: task can run in parallel with other tasks if files do not overlap and prerequisites are complete
- **[Story]**: user story label for story-specific phases only
- Every task includes exact file paths

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare the deterministic recognizer module, configuration, and test scaffolding used by all stories.

- [X] T001 Create deterministic recognizer module scaffolding in `crates/pokrov-core/src/detection/mod.rs`, `crates/pokrov-core/src/detection/deterministic/mod.rs`, `crates/pokrov-core/src/detection/deterministic/pattern.rs`, `crates/pokrov-core/src/detection/deterministic/validation.rs`, `crates/pokrov-core/src/detection/deterministic/context.rs`, and `crates/pokrov-core/src/detection/deterministic/lists.rs`
- [X] T002 [P] Add deterministic recognizer configuration scaffolding to `crates/pokrov-config/src/model.rs` and `config/pokrov.example.yaml`
- [X] T003 [P] Create shared deterministic analyzer fixtures in `tests/common/sanitization_deterministic_test_support.rs` and wire imports from `tests/common/sanitization_analyzer_contract_test_support.rs` and `tests/common/sanitization_foundation_test_support.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Extend core contracts and startup plumbing before story-specific behavior is built.

**CRITICAL**: No user story work should start before this phase is complete.

- [X] T004 Extend analyzer hit and decision contracts in `crates/pokrov-core/src/types.rs` and `crates/pokrov-core/src/types/foundation/hit_families.rs`
- [X] T005 Extend engine execution flow and metadata-only trace plumbing in `crates/pokrov-core/src/lib.rs`, `crates/pokrov-core/src/audit/mod.rs`, and `crates/pokrov-core/src/types/foundation/mod.rs`
- [X] T006 [P] Validate and translate deterministic profile configuration in `crates/pokrov-config/src/model.rs` and `crates/pokrov-config/src/validate.rs`
- [X] T007 [P] Add foundation contract coverage for deterministic candidate fields in `tests/contract/sanitization_foundation_contract.rs` and `tests/integration/sanitization_foundation_shared_contracts.rs`
- [X] T008 [P] Add startup and readiness coverage for deterministic recognizer configuration in `tests/contract/runtime_config_contract.rs` and `tests/integration/startup_config_flow.rs`

**Checkpoint**: Foundation ready. User story work can now proceed in dependency order.

---

## Phase 3: User Story 1 - Detect high-confidence structured secrets consistently (Priority: P1) MVP

**Goal**: Enable deterministic pattern and validator-based detection that emits stable candidates across plain text and structured JSON leaves.

**Independent Test**: Submit the same approved input corpus multiple times under the same profile and confirm identical deterministic candidates, scores, and suppression outcomes for plain text and JSON-leaf inputs.

### Tests for User Story 1

- [X] T009 [P] [US1] Add unit tests for pattern compilation, normalization, and stable candidate ordering in `crates/pokrov-core/src/detection/mod.rs` and `crates/pokrov-core/src/detection/deterministic/pattern.rs`
- [X] T010 [P] [US1] Add analyzer contract tests for deterministic candidate output and replay identity in `tests/contract/sanitization_evaluate_contract.rs`
- [X] T011 [P] [US1] Add integration tests for plain-text and JSON-leaf parity in `tests/integration/sanitization_evaluate_flow.rs` and `tests/integration/sanitization_transform_flow.rs`
- [X] T012 [P] [US1] Add performance coverage for deterministic pattern evaluation in `tests/performance/sanitization_evaluate_latency.rs`

### Implementation for User Story 1

- [X] T013 [P] [US1] Implement compiled pattern recognizers and normalization flow in `crates/pokrov-core/src/detection/deterministic/pattern.rs` and `crates/pokrov-core/src/detection/deterministic/mod.rs`
- [X] T014 [P] [US1] Implement validator and checksum candidate handling in `crates/pokrov-core/src/detection/deterministic/validation.rs`
- [X] T015 [US1] Wire deterministic candidate execution into `crates/pokrov-core/src/detection/mod.rs` and `crates/pokrov-core/src/lib.rs`
- [X] T016 [US1] Update deterministic overlap inputs and resolved location mapping in `crates/pokrov-core/src/policy/mod.rs` and `crates/pokrov-core/src/types.rs`

**Checkpoint**: User Story 1 is independently functional and verifiable.

---

## Phase 4: User Story 2 - Tune confidence without losing safety controls (Priority: P2)

**Goal**: Add lexical context, exact-match allowlists, and denylist positives with deterministic precedence and scoped configuration.

**Independent Test**: Evaluate curated examples containing positive context, negative context, exact allowlist entries, and denylist overlaps, then confirm the expected precedence, scores, and suppression metadata under one active profile.

### Tests for User Story 2

- [X] T017 [P] [US2] Add unit tests for context scoring, exact-match allowlists, and denylist provenance in `crates/pokrov-core/src/detection/deterministic/context.rs` and `crates/pokrov-core/src/detection/deterministic/lists.rs`
- [X] T018 [P] [US2] Add integration tests for profile-scoped context and list precedence in `tests/integration/sanitization_evaluate_flow.rs` and `tests/integration/sanitization_audit_explain_flow.rs`
- [X] T019 [P] [US2] Add security tests for profile and tenant list isolation in `tests/security/sanitization_metadata_leakage.rs` and `tests/security/sanitization_foundation_metadata_leakage.rs`

### Implementation for User Story 2

- [X] T020 [P] [US2] Implement EN/RU lexical context dictionaries and negative-context defaults in `crates/pokrov-core/src/detection/deterministic/context.rs`
- [X] T021 [P] [US2] Implement exact-match allowlist suppression and first-class denylist candidate generation in `crates/pokrov-core/src/detection/deterministic/lists.rs`
- [X] T022 [US2] Extend deterministic recognizer profile schema for context and list controls in `crates/pokrov-config/src/model.rs` and `crates/pokrov-config/src/validate.rs`
- [X] T023 [US2] Apply context and list precedence rules in `crates/pokrov-core/src/detection/deterministic/mod.rs` and `crates/pokrov-core/src/policy/mod.rs`

**Checkpoint**: User Stories 1 and 2 remain independently testable, with US2 adding deterministic tuning controls.

---

## Phase 5: User Story 3 - Review outcomes without exposing raw payloads (Priority: P3)

**Goal**: Expose metadata-only validation, suppression, and precedence explanations through shared analyzer contracts, explain summaries, and audit summaries.

**Independent Test**: Review explain and audit outputs for accepted, rejected, and suppressed deterministic candidates and confirm that reason codes, statuses, and family counts are present without raw payload content.

### Tests for User Story 3

- [X] T024 [P] [US3] Add contract tests for validation status, suppression status, and reason-code metadata in `tests/contract/sanitization_foundation_contract.rs` and `tests/contract/sanitization_evaluate_contract.rs`
- [X] T025 [P] [US3] Add integration tests for metadata-only explain and audit outcomes in `tests/integration/sanitization_audit_explain_flow.rs` and `tests/integration/sanitization_foundation_evaluation_boundary.rs`
- [X] T026 [P] [US3] Add security tests proving no raw deterministic evidence leaks in `tests/security/sanitization_metadata_leakage.rs` and `tests/security/sanitization_foundation_metadata_leakage.rs`

### Implementation for User Story 3

- [X] T027 [P] [US3] Extend normalized and resolved hit metadata for validation and suppression traces in `crates/pokrov-core/src/types.rs` and `crates/pokrov-core/src/types/foundation/hit_families.rs`
- [X] T028 [P] [US3] Update explain and audit summaries with deterministic family counts and safe reason codes in `crates/pokrov-core/src/audit/mod.rs` and `crates/pokrov-core/src/lib.rs`
- [X] T029 [US3] Propagate deterministic execution metadata through runtime-facing traces in `crates/pokrov-core/src/types/foundation/mod.rs` and `tests/common/sanitization_foundation_test_support.rs`

**Checkpoint**: All user stories are independently functional, metadata-safe, and covered by explicit tests.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Finalize documentation, release evidence, and full-workspace verification.

- [X] T030 [P] Update deterministic recognizer documentation and example configuration in `config/pokrov.example.yaml` and `config/README.md`
- [X] T031 Update verification guidance and acceptance evidence notes in `specs/011-deterministic-recognizers/quickstart.md` and `config/release/verification-checklist.md`
- [X] T032 Run final workspace verification from `Cargo.toml` and record deterministic recognizer evidence in `config/release/verification-checklist.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1: Setup** starts immediately.
- **Phase 2: Foundational** depends on Phase 1 and blocks all story phases.
- **Phase 3: US1** depends on Phase 2 and is the MVP increment.
- **Phase 4: US2** depends on Phase 2 and the US1 candidate pipeline being available.
- **Phase 5: US3** depends on Phase 2 and the deterministic metadata produced by US1 and US2.
- **Phase 6: Polish** starts after the target stories are complete.

### User Story Dependencies

- **US1 (P1)** has no user-story dependency beyond Foundation.
- **US2 (P2)** builds on the deterministic candidate pipeline delivered in US1.
- **US3 (P3)** builds on deterministic statuses and precedence metadata delivered by US1 and US2.

### Within Each User Story

- Write or update mandatory tests before finishing implementation tasks.
- Candidate and contract changes come before policy/explain integration.
- A story is complete only when its independent test criteria pass.

### Parallel Opportunities

- `T002` and `T003` can run in parallel after `T001`.
- `T006`, `T007`, and `T008` can run in parallel after `T004` and `T005`.
- In US1, `T009` through `T012` can run in parallel, and `T013` and `T014` can run in parallel.
- In US2, `T017` through `T019` can run in parallel, and `T020` and `T021` can run in parallel.
- In US3, `T024` through `T026` can run in parallel, and `T027` and `T028` can run in parallel.
- `T030` can run in parallel with late-stage verification work once implementation is stable.

---

## Parallel Example: User Story 1

```bash
# Parallel test work for US1
Task: "T009 Add unit tests in crates/pokrov-core/src/detection/mod.rs and crates/pokrov-core/src/detection/deterministic/pattern.rs"
Task: "T010 Add contract tests in tests/contract/sanitization_evaluate_contract.rs"
Task: "T011 Add integration tests in tests/integration/sanitization_evaluate_flow.rs and tests/integration/sanitization_transform_flow.rs"
Task: "T012 Add performance coverage in tests/performance/sanitization_evaluate_latency.rs"

# Parallel implementation work for US1
Task: "T013 Implement pattern recognizers in crates/pokrov-core/src/detection/deterministic/pattern.rs and crates/pokrov-core/src/detection/deterministic/mod.rs"
Task: "T014 Implement validator handling in crates/pokrov-core/src/detection/deterministic/validation.rs"
```

---

## Parallel Example: User Story 2

```bash
# Parallel test work for US2
Task: "T017 Add unit tests in crates/pokrov-core/src/detection/deterministic/context.rs and crates/pokrov-core/src/detection/deterministic/lists.rs"
Task: "T018 Add integration tests in tests/integration/sanitization_evaluate_flow.rs and tests/integration/sanitization_audit_explain_flow.rs"
Task: "T019 Add security tests in tests/security/sanitization_metadata_leakage.rs and tests/security/sanitization_foundation_metadata_leakage.rs"

# Parallel implementation work for US2
Task: "T020 Implement context scoring in crates/pokrov-core/src/detection/deterministic/context.rs"
Task: "T021 Implement list controls in crates/pokrov-core/src/detection/deterministic/lists.rs"
```

---

## Parallel Example: User Story 3

```bash
# Parallel test work for US3
Task: "T024 Add contract tests in tests/contract/sanitization_foundation_contract.rs and tests/contract/sanitization_evaluate_contract.rs"
Task: "T025 Add integration tests in tests/integration/sanitization_audit_explain_flow.rs and tests/integration/sanitization_foundation_evaluation_boundary.rs"
Task: "T026 Add security tests in tests/security/sanitization_metadata_leakage.rs and tests/security/sanitization_foundation_metadata_leakage.rs"

# Parallel implementation work for US3
Task: "T027 Extend hit metadata in crates/pokrov-core/src/types.rs and crates/pokrov-core/src/types/foundation/hit_families.rs"
Task: "T028 Update explain and audit summaries in crates/pokrov-core/src/audit/mod.rs and crates/pokrov-core/src/lib.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup.
2. Complete Phase 2: Foundational.
3. Complete Phase 3: User Story 1.
4. Run the independent US1 verification path before expanding scope.

### Incremental Delivery

1. Setup and Foundation establish the shared deterministic recognizer surface.
2. Deliver US1 for baseline deterministic detection and replay stability.
3. Deliver US2 for context and list-based tuning.
4. Deliver US3 for metadata-only review and audit completeness.
5. Finish with cross-cutting documentation and workspace verification.

### Parallel Team Strategy

1. One owner completes Setup and Foundational work.
2. After Foundation, one owner can advance US1 while another prepares US2 test assets.
3. US3 starts after deterministic statuses and precedence traces are available.

---

## Notes

- Total tasks: 32
- User story task counts: US1 = 8, US2 = 7, US3 = 6
- Suggested MVP scope: Phase 1, Phase 2, and Phase 3 (US1 only)
- All tasks follow the required checklist format with checkbox, ID, story label where required, and exact file paths.
