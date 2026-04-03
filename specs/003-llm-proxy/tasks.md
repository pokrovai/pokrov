# Tasks: LLM Proxy

**Input**: Design artifacts from `/specs/003-llm-proxy/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md`

**Tests**: Test tasks are mandatory for this feature because `spec.md` explicitly requires unit, integration, performance, and security coverage.

**Organization**: Tasks are grouped by user story so each story can be implemented and verified independently.

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare workspace and crate skeleton for the LLM proxy path.

- [ ] T001 Add `crates/pokrov-proxy-llm` to workspace members and shared dependencies in `Cargo.toml`
- [ ] T002 Create crate manifest and base module exports in `crates/pokrov-proxy-llm/Cargo.toml` and `crates/pokrov-proxy-llm/src/lib.rs`
- [ ] T003 [P] Create LLM proxy module skeleton in `crates/pokrov-proxy-llm/src/types.rs`, `crates/pokrov-proxy-llm/src/normalize.rs`, `crates/pokrov-proxy-llm/src/handler.rs`, `crates/pokrov-proxy-llm/src/routing.rs`, `crates/pokrov-proxy-llm/src/upstream.rs`, `crates/pokrov-proxy-llm/src/stream.rs`, `crates/pokrov-proxy-llm/src/audit.rs`, and `crates/pokrov-proxy-llm/src/errors.rs`
- [ ] T004 [P] Add baseline LLM provider/route example section in `config/pokrov.example.yaml`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Build shared config/runtime/API foundations required by all user stories.

**CRITICAL**: No user story work starts before this phase is complete.

- [ ] T005 Introduce typed LLM config models (`providers`, `routes`, `defaults`) in `crates/pokrov-config/src/model.rs`
- [ ] T006 Implement LLM config validation rules from `llm-routing.schema.yaml` in `crates/pokrov-config/src/validate.rs` and `crates/pokrov-config/src/error.rs`
- [ ] T007 [P] Add config loader coverage for valid/invalid route bindings in `crates/pokrov-config/src/loader.rs` and `tests/contract/runtime_config_contract.rs`
- [ ] T008 Extend application state with LLM proxy dependencies in `crates/pokrov-api/src/app.rs` and `crates/pokrov-api/src/lib.rs`
- [ ] T009 Define shared LLM proxy error mapping primitives in `crates/pokrov-api/src/error.rs` and `crates/pokrov-proxy-llm/src/errors.rs`
- [ ] T010 [P] Add LLM lifecycle metric hooks and counters in `crates/pokrov-metrics/src/hooks.rs` and `crates/pokrov-metrics/src/registry.rs`
- [ ] T011 Initialize route table and upstream client bootstrap wiring in `crates/pokrov-runtime/src/bootstrap.rs`
- [ ] T012 [P] Extend readiness checks for LLM routing configuration status in `crates/pokrov-api/src/handlers/ready.rs` and `crates/pokrov-runtime/src/lifecycle.rs`

**Checkpoint**: Foundation complete; user stories can proceed.

---

## Phase 3: User Story 1 - Secure LLM Request Proxying (Priority: P1) MVP

**Goal**: Accept OpenAI-compatible chat requests, sanitize input before upstream, and block deterministically when policy requires.

**Independent Test**: Send a non-stream chat completion request and verify allowed flow reaches upstream only after sanitization, while blocked flow returns structured `403` without upstream call.

### Tests for User Story 1

- [ ] T013 [P] [US1] Add contract test for non-stream success and `policy_blocked` error in `tests/contract/llm_proxy_api_contract.rs`
- [ ] T014 [P] [US1] Add integration happy-path test for sanitized upstream request body in `tests/integration/llm_proxy_happy_path.rs`
- [ ] T015 [P] [US1] Add integration block-path short-circuit test (no upstream call) in `tests/integration/llm_proxy_block_path.rs`
- [ ] T016 [P] [US1] Add security test for invalid API key unauthorized handling in `tests/security/llm_proxy_auth_validation.rs`
- [ ] T017 [P] [US1] Add unit tests for request normalization and profile precedence in `crates/pokrov-proxy-llm/src/normalize.rs`

### Implementation for User Story 1

- [ ] T018 [P] [US1] Implement `LLMRequestEnvelope`, `LLMMessage`, and content structures in `crates/pokrov-proxy-llm/src/types.rs`
- [ ] T019 [US1] Implement OpenAI request normalization and profile resolution order in `crates/pokrov-proxy-llm/src/normalize.rs`
- [ ] T020 [US1] Implement input policy evaluation and sanitization orchestration in `crates/pokrov-proxy-llm/src/handler.rs`
- [ ] T021 [US1] Implement deterministic block short-circuit with metadata-only structured error in `crates/pokrov-proxy-llm/src/errors.rs` and `crates/pokrov-proxy-llm/src/handler.rs`
- [ ] T022 [US1] Implement non-stream response shaping with `request_id` and `pokrov` metadata in `crates/pokrov-proxy-llm/src/handler.rs`
- [ ] T023 [US1] Add `/v1/chat/completions` route and HTTP adapter wiring in `crates/pokrov-api/src/handlers/chat_completions.rs`, `crates/pokrov-api/src/handlers/mod.rs`, and `crates/pokrov-api/src/app.rs`

**Checkpoint**: US1 is independently functional and testable.

---

## Phase 4: User Story 2 - Streaming and Model-Based Routing (Priority: P2)

**Goal**: Route requests deterministically by `model` and preserve OpenAI-style streaming compatibility.

**Independent Test**: Execute non-stream and stream requests for different models and verify deterministic provider routing and SSE-compatible stream behavior.

### Tests for User Story 2

- [ ] T024 [P] [US2] Add contract test for `model_not_routed` and stream response headers in `tests/contract/llm_proxy_stream_contract.rs`
- [ ] T025 [P] [US2] Add integration test for deterministic model-to-provider routing in `tests/integration/llm_proxy_routing_path.rs`
- [ ] T026 [P] [US2] Add integration test for SSE framing and `[DONE]` termination in `tests/integration/llm_proxy_streaming_path.rs`
- [ ] T027 [P] [US2] Add integration test for upstream timeout/unavailable structured errors in `tests/integration/llm_proxy_upstream_error_path.rs`
- [ ] T028 [P] [US2] Add unit tests for route resolution determinism and fallback behavior in `crates/pokrov-proxy-llm/src/routing.rs`

### Implementation for User Story 2

- [ ] T029 [P] [US2] Implement provider route table and deterministic route resolution in `crates/pokrov-proxy-llm/src/routing.rs`
- [ ] T030 [US2] Implement unmapped-model error path with metadata-only payload in `crates/pokrov-proxy-llm/src/routing.rs` and `crates/pokrov-proxy-llm/src/errors.rs`
- [ ] T031 [US2] Implement upstream request execution with provider timeout/retry settings in `crates/pokrov-proxy-llm/src/upstream.rs`
- [ ] T032 [US2] Implement OpenAI-style SSE stream parsing and pass-through lifecycle in `crates/pokrov-proxy-llm/src/stream.rs`
- [ ] T033 [US2] Integrate routing + stream/non-stream upstream flow in `crates/pokrov-proxy-llm/src/handler.rs`
- [ ] T034 [US2] Wire route defaults and provider auth resolution from runtime config into LLM handler setup in `crates/pokrov-runtime/src/bootstrap.rs`

**Checkpoint**: US1 and US2 both work independently.

---

## Phase 5: User Story 3 - Output Sanitization and Metadata-Only Audit (Priority: P3)

**Goal**: Apply optional output sanitization and emit metadata-only audit records without leaking raw prompt/response content.

**Independent Test**: Enable output sanitization profile and verify sanitized client output plus metadata-only audit/log artifacts across allow/block/error flows.

### Tests for User Story 3

- [ ] T035 [P] [US3] Add contract test for required `pokrov` metadata fields and forbidden raw fields in `tests/contract/llm_proxy_metadata_contract.rs`
- [ ] T036 [P] [US3] Add integration test for non-stream output sanitization before response return in `tests/integration/llm_proxy_output_sanitization_path.rs`
- [ ] T037 [P] [US3] Add integration test for stream chunk output sanitization in `tests/integration/llm_proxy_stream_output_sanitization_path.rs`
- [ ] T038 [P] [US3] Add security test for metadata-only logs/audit leakage prevention in `tests/security/llm_proxy_metadata_leakage.rs`
- [ ] T039 [P] [US3] Add performance regression test for LLM proxy overhead budget in `tests/performance/llm_proxy_overhead_budget.rs`

### Implementation for User Story 3

- [ ] T040 [P] [US3] Implement non-stream output sanitization pipeline in `crates/pokrov-proxy-llm/src/handler.rs`
- [ ] T041 [P] [US3] Implement stream chunk output sanitization for delta/content events in `crates/pokrov-proxy-llm/src/stream.rs`
- [ ] T042 [US3] Implement `LLMAuditEvent` metadata-only builder and schema-safe serialization in `crates/pokrov-proxy-llm/src/audit.rs`
- [ ] T043 [US3] Emit metadata-only audit events on allow, block, and upstream-error terminal states in `crates/pokrov-proxy-llm/src/handler.rs`
- [ ] T044 [US3] Extend runtime metrics for final action, blocked requests, upstream status, and duration in `crates/pokrov-metrics/src/registry.rs` and `crates/pokrov-metrics/src/hooks.rs`
- [ ] T045 [US3] Add `X-Request-Id` propagation and metadata summary attachment in `crates/pokrov-api/src/handlers/chat_completions.rs`

**Checkpoint**: All user stories are independently verifiable.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final hardening, documentation, and release-readiness evidence.

- [ ] T046 [P] Update quickstart verification commands and expected evidence in `specs/003-llm-proxy/quickstart.md`
- [ ] T047 [P] Update operator documentation for LLM routes/providers and auth expectations in `config/README.md` and `README.md`
- [ ] T048 Record final verification evidence (contract/integration/security/performance results) in `docs/verification/003-llm-proxy.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- Setup (Phase 1): starts immediately.
- Foundational (Phase 2): depends on Setup and blocks all user stories.
- User Stories (Phase 3-5): start after Foundational.
- Polish (Phase 6): starts after required user stories are complete.

### User Story Dependencies

- US1 (P1): first delivery target (MVP) after Foundational.
- US2 (P2): depends on Foundational and US1 handler surface for route integration.
- US3 (P3): depends on US1 request/response flow and US2 stream flow.

### Within Each User Story

- Required tests are defined before implementation tasks.
- Types/contracts are implemented before handlers and integration wiring.
- Story is complete only after independent test criteria and evidence pass.

### Parallel Opportunities

- Phase 1: T003-T004 can run in parallel after T001-T002.
- Phase 2: T007, T010, and T012 can run in parallel once core config models exist.
- US1: T013-T017 can run in parallel; T018 and T023 can run in parallel with test authoring.
- US2: T024-T028 can run in parallel; T029 and T031 can run in parallel before T033.
- US3: T035-T039 can run in parallel; T040 and T041 can run in parallel before T043.

---

## Parallel Example: User Story 1

```bash
Task: "T013 [US1] Contract test in tests/contract/llm_proxy_api_contract.rs"
Task: "T014 [US1] Integration happy path in tests/integration/llm_proxy_happy_path.rs"
Task: "T016 [US1] Security auth validation in tests/security/llm_proxy_auth_validation.rs"
Task: "T018 [US1] Envelope types in crates/pokrov-proxy-llm/src/types.rs"
```

## Parallel Example: User Story 2

```bash
Task: "T025 [US2] Routing integration test in tests/integration/llm_proxy_routing_path.rs"
Task: "T026 [US2] Streaming integration test in tests/integration/llm_proxy_streaming_path.rs"
Task: "T029 [US2] Route resolver in crates/pokrov-proxy-llm/src/routing.rs"
Task: "T031 [US2] Upstream execution in crates/pokrov-proxy-llm/src/upstream.rs"
```

## Parallel Example: User Story 3

```bash
Task: "T036 [US3] Non-stream output sanitization test in tests/integration/llm_proxy_output_sanitization_path.rs"
Task: "T037 [US3] Stream output sanitization test in tests/integration/llm_proxy_stream_output_sanitization_path.rs"
Task: "T040 [US3] Non-stream output sanitization in crates/pokrov-proxy-llm/src/handler.rs"
Task: "T041 [US3] Stream output sanitization in crates/pokrov-proxy-llm/src/stream.rs"
```

---

## Implementation Strategy

### MVP First (US1 Only)

1. Complete Phase 1 and Phase 2.
2. Complete Phase 3 (US1).
3. Validate US1 independently before moving to US2/US3.

### Incremental Delivery

1. Deliver US1 secure non-stream path.
2. Add US2 deterministic routing and streaming compatibility.
3. Add US3 output sanitization and metadata-only auditability.
4. Complete Phase 6 documentation and verification artifacts.

### Parallel Team Strategy

1. Team completes Setup + Foundational phases together.
2. After checkpoint, engineers split by story or test/implementation tracks using `[P]` tasks.
3. Merge only after independent story acceptance criteria and coverage gates pass.

---

## Notes

- `[P]` marks tasks that can run in parallel with disjoint files and no unresolved dependency.
- `[US1]`, `[US2]`, `[US3]` map directly to user stories in `spec.md`.
- Every task includes explicit file path targets and follows the required checklist format.
