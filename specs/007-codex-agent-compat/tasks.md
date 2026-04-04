# Задачи: Codex Agent Compatibility

**Вход**: Артефакты проектирования из `/specs/007-codex-agent-compat/`
**Prerequisites**: `plan.md` (обязательно), `spec.md` (обязательно для историй), `research.md`, `data-model.md`, `contracts/`, `quickstart.md`

**Тесты**: Для этой фичи тестовые задачи обязательны, потому что `spec.md` явно требует Unit, Integration, Performance и Security coverage для runtime/proxy/auth/policy изменений.

**Организация**: Задачи сгруппированы по пользовательским историям, чтобы каждая история оставалась independently testable и independently deliverable.

## Формат: `[ID] [P?] [Story] Description`

- **[P]**: можно выполнять параллельно, если файлы не пересекаются и нет зависимости от незавершенных задач
- **[Story]**: идентификатор пользовательской истории (`US1`, `US2`, `US3`)
- Каждая задача содержит точные пути к файлам

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Подготовить каркас feature-документации, контрактных тестов и verification entrypoints для `responses` compatibility

- [X] T001 Обновить feature execution notes и acceptance checklist links в `specs/007-codex-agent-compat/plan.md` и `specs/007-codex-agent-compat/quickstart.md`
- [X] T002 [P] Добавить baseline route coverage заметки для `/v1/responses` в `docs/verification/003-llm-proxy.md`
- [X] T003 [P] Подключить новые test-модули для responses compatibility в `tests/contract.rs`, `tests/integration.rs`, `tests/security.rs`, `tests/performance.rs`
- [X] T004 [P] Обновить runtime usage guidance для Codex compatibility в `README.md`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Реализовать общие primitives для нового endpoint, mapping и observability, блокирующие все пользовательские истории

**CRITICAL**: Ни одна пользовательская история не должна стартовать до завершения этой фазы

- [X] T005 Добавить endpoint wiring `POST /v1/responses` в `crates/pokrov-api/src/app.rs`
- [X] T006 [P] Создать handler каркас для responses compatibility в `crates/pokrov-api/src/handlers/responses.rs` и экспорт в `crates/pokrov-api/src/handlers/mod.rs`
- [X] T007 [P] Добавить базовые типы request/response compatibility в `crates/pokrov-proxy-llm/src/types.rs`
- [X] T008 Реализовать deterministic mapping `responses -> chat/completions` в `crates/pokrov-proxy-llm/src/normalize.rs`
- [X] T009 [P] Добавить shared error mapping для `unsupported_request_subset`/`invalid_request` в `crates/pokrov-api/src/error.rs`
- [X] T010 [P] Расширить route/path normalization для `/v1/responses` в `crates/pokrov-api/src/middleware/mod.rs` и `crates/pokrov-metrics/src/registry.rs`
- [X] T011 [P] Добавить audit event schema для responses endpoint в `crates/pokrov-proxy-llm/src/audit.rs`
- [X] T012 [P] Добавить hooks для auth-stage и upstream metrics на `/v1/responses` в `crates/pokrov-metrics/src/hooks.rs` и `crates/pokrov-metrics/src/registry.rs`
- [X] T013 Обновить contract validation scaffolding для нового endpoint в `tests/contract/llm_proxy_api_contract.rs`

**Checkpoint**: Foundation готова, пользовательские истории можно реализовывать независимо

---

## Phase 3: User Story 1 - Codex request compatibility (Priority: P1) MVP

**Goal**: Дать Codex-compatible sync path через `POST /v1/responses` с сохранением sanitization-first и metadata-only guarantees

**Independent Test**: Валидный нестриминговый Codex-style запрос на `/v1/responses` проходит policy/sanitization pipeline и возвращает успешный структурированный ответ без регрессии `/v1/chat/completions`

### Tests for User Story 1

- [X] T014 [P] [US1] Добавить contract test для non-stream `POST /v1/responses` success/error envelopes в `tests/contract/responses_api_contract.rs`
- [X] T015 [P] [US1] Добавить integration test non-stream happy path для responses compatibility в `tests/integration/responses_compat_happy_path.rs`
- [X] T016 [P] [US1] Добавить integration test policy block short-circuit до upstream в `tests/integration/responses_policy_block_path.rs`
- [X] T017 [P] [US1] Добавить unit tests mapping `responses -> chat/completions` в `crates/pokrov-proxy-llm/src/normalize.rs`
- [X] T018 [P] [US1] Добавить security test metadata-only error payload на non-stream path в `tests/security/responses_metadata_leakage.rs`

### Implementation for User Story 1

- [X] T019 [P] [US1] Реализовать parsing/validation минимального non-stream subset в `crates/pokrov-api/src/handlers/responses.rs`
- [X] T020 [P] [US1] Реализовать non-stream conversion во внутренний chat flow в `crates/pokrov-proxy-llm/src/handler.rs` и `crates/pokrov-proxy-llm/src/types.rs`
- [X] T021 [US1] Подключить pre-upstream sanitization/policy evaluation для responses path в `crates/pokrov-proxy-llm/src/handler.rs`
- [X] T022 [US1] Реализовать JSON response envelope с `pokrov` metadata в `crates/pokrov-api/src/handlers/responses.rs`
- [X] T023 [US1] Добавить mapping upstream/provider errors в predictable responses errors в `crates/pokrov-api/src/error.rs` и `crates/pokrov-api/src/handlers/responses.rs`
- [X] T024 [US1] Добавить regression guard для неизменного `chat/completions` поведения в `tests/integration/llm_proxy_chat_completions_regression.rs`

**Checkpoint**: История 1 полностью работоспособна и проверяема независимо

---

## Phase 4: User Story 2 - Streaming parity for Codex workflows (Priority: P2)

**Goal**: Обеспечить stream-совместимость для Codex через `/v1/responses` с сохранением output sanitization и SSE lifecycle

**Independent Test**: Stream запрос к `/v1/responses` возвращает SSE-compatible поток с корректным завершением `data: [DONE]`, policy/output sanitization применяются детерминированно

### Tests for User Story 2

- [X] T025 [P] [US2] Добавить contract test stream mode для `/v1/responses` в `tests/contract/responses_api_contract.rs`
- [X] T026 [P] [US2] Добавить integration test stream happy path в `tests/integration/responses_stream_happy_path.rs`
- [X] T027 [P] [US2] Добавить integration test malformed upstream stream chunk handling в `tests/integration/responses_stream_malformed_chunk_path.rs`
- [X] T028 [P] [US2] Добавить security test stream output metadata-only safety в `tests/security/responses_stream_metadata_leakage.rs`
- [X] T029 [P] [US2] Добавить unit tests stream conversion/sanitization boundary в `crates/pokrov-proxy-llm/src/stream.rs`

### Implementation for User Story 2

- [X] T030 [P] [US2] Реализовать stream=true branch для `/v1/responses` handler в `crates/pokrov-api/src/handlers/responses.rs`
- [X] T031 [P] [US2] Реализовать conversion internal stream response -> responses-compatible SSE events в `crates/pokrov-proxy-llm/src/stream.rs`
- [X] T032 [US2] Добавить output sanitization orchestration для stream chunks на responses path в `crates/pokrov-proxy-llm/src/handler.rs`
- [X] T033 [US2] Добавить predictable stream error termination semantics в `crates/pokrov-api/src/handlers/responses.rs` и `crates/pokrov-api/src/error.rs`
- [X] T034 [US2] Добавить stream observability (route metrics, action/rule_hits summaries) в `crates/pokrov-metrics/src/registry.rs` и `crates/pokrov-proxy-llm/src/audit.rs`

**Checkpoint**: Истории 1 и 2 работают независимо и не регрессируют друг друга

---

## Phase 5: User Story 3 - Secure dual-auth passthrough boundary (Priority: P3)

**Goal**: Гарантировать раздельную валидацию gateway/upstream credential для responses path и предсказуемые metadata-only auth failures

**Independent Test**: При валидном `X-Pokrov-Api-Key` и отсутствующем upstream bearer запрос к `/v1/responses` блокируется `422` до upstream; при невалидном gateway auth возвращается `401` независимо от provider token

### Tests for User Story 3

- [X] T035 [P] [US3] Добавить contract test auth-stage failures для `/v1/responses` в `tests/contract/responses_api_contract.rs`
- [X] T036 [P] [US3] Добавить integration test missing upstream credential passthrough block path в `tests/integration/responses_auth_missing_upstream_credential.rs`
- [X] T037 [P] [US3] Добавить integration test gateway auth failure precedence в `tests/integration/responses_gateway_auth_failure.rs`
- [X] T038 [P] [US3] Добавить security test no credential leakage in errors/logs/audit в `tests/security/responses_auth_metadata_leakage.rs`
- [X] T039 [P] [US3] Добавить performance test auth overhead budget на `/v1/responses` в `tests/performance/responses_proxy_overhead_budget.rs`

### Implementation for User Story 3

- [X] T040 [P] [US3] Реализовать split-auth extraction и passthrough guard для responses handler в `crates/pokrov-api/src/handlers/responses.rs` и `crates/pokrov-api/src/auth.rs`
- [X] T041 [P] [US3] Интегрировать identity-bound profile/rate-limit resolution для `/v1/responses` в `crates/pokrov-api/src/handlers/responses.rs` и `crates/pokrov-api/src/handlers/rate_limit.rs`
- [X] T042 [US3] Добавить auth-stage audit events (`gateway_auth`, `upstream_credentials`) для responses path в `crates/pokrov-proxy-llm/src/audit.rs`
- [X] T043 [US3] Добавить auth decision metrics labels/outcomes для `/v1/responses` в `crates/pokrov-metrics/src/registry.rs` и `crates/pokrov-metrics/src/hooks.rs`
- [X] T044 [US3] Добавить structured error mappings (`gateway_unauthorized`, `upstream_credential_*`) для responses path в `crates/pokrov-api/src/error.rs`

**Checkpoint**: Все пользовательские истории функциональны независимо

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Финальная синхронизация contracts/docs и обязательных verification gates

- [X] T045 [P] Синхронизировать реализацию с contracts в `specs/007-codex-agent-compat/contracts/codex-responses-api.yaml` и `specs/007-codex-agent-compat/contracts/codex-compat-config.yaml`
- [X] T046 [P] Обновить quickstart/runtime docs по итоговому behavior в `specs/007-codex-agent-compat/quickstart.md` и `README.md`
- [X] T047 Обновить verification evidence для responses compatibility в `docs/verification/003-llm-proxy.md`
- [X] T048 [P] Добавить финальные regression/security/perf assertions в `tests/integration/llm_proxy_chat_completions_regression.rs`, `tests/security/responses_metadata_leakage.rs`, `tests/performance/responses_proxy_overhead_budget.rs`
- [X] T049 Выполнить финальный verification matrix и зафиксировать статус в `specs/007-codex-agent-compat/checklists/requirements.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: стартует сразу
- **Phase 2 (Foundational)**: зависит от Setup и блокирует все пользовательские истории
- **Phase 3 (US1)**: стартует после завершения Foundational (MVP)
- **Phase 4 (US2)**: стартует после Foundational; зависит от базового mapping из US1
- **Phase 5 (US3)**: стартует после Foundational; зависит от готового responses handler skeleton
- **Phase 6 (Polish)**: выполняется после целевых историй

### User Story Dependencies

- **US1 (P1)**: первая deliverable история, формирует MVP для non-stream Codex compatibility
- **US2 (P2)**: зависит от базового responses mapping, добавляет stream parity
- **US3 (P3)**: использует foundations + responses handler для split-auth и identity-bound решений

### Within Each User Story

- Сначала создаются тесты (contract/integration/security/unit/performance)
- Затем реализуются core handler/mapping/auth/observability задачи
- История считается завершенной только после выполнения independent test criterion

### Parallel Opportunities

- Setup: `T002`, `T003`, `T004`
- Foundational: `T006`, `T007`, `T009`, `T010`, `T011`, `T012`
- US1: `T014`, `T015`, `T016`, `T017`, `T018`, `T019`, `T020`
- US2: `T025`, `T026`, `T027`, `T028`, `T029`, `T030`, `T031`
- US3: `T035`, `T036`, `T037`, `T038`, `T039`, `T040`, `T041`
- Polish: `T045`, `T046`, `T048`

---

## Parallel Example: User Story 1

```bash
# Tests in parallel
Task: "T014 [US1] tests/contract/responses_api_contract.rs"
Task: "T015 [US1] tests/integration/responses_compat_happy_path.rs"
Task: "T018 [US1] tests/security/responses_metadata_leakage.rs"

# Implementation in parallel
Task: "T019 [US1] crates/pokrov-api/src/handlers/responses.rs"
Task: "T020 [US1] crates/pokrov-proxy-llm/src/handler.rs + crates/pokrov-proxy-llm/src/types.rs"
```

---

## Parallel Example: User Story 2

```bash
# Tests in parallel
Task: "T025 [US2] tests/contract/responses_api_contract.rs"
Task: "T026 [US2] tests/integration/responses_stream_happy_path.rs"
Task: "T028 [US2] tests/security/responses_stream_metadata_leakage.rs"

# Implementation in parallel
Task: "T030 [US2] crates/pokrov-api/src/handlers/responses.rs"
Task: "T031 [US2] crates/pokrov-proxy-llm/src/stream.rs"
```

---

## Parallel Example: User Story 3

```bash
# Tests in parallel
Task: "T035 [US3] tests/contract/responses_api_contract.rs"
Task: "T036 [US3] tests/integration/responses_auth_missing_upstream_credential.rs"
Task: "T039 [US3] tests/performance/responses_proxy_overhead_budget.rs"

# Implementation in parallel
Task: "T040 [US3] crates/pokrov-api/src/handlers/responses.rs + crates/pokrov-api/src/auth.rs"
Task: "T043 [US3] crates/pokrov-metrics/src/registry.rs + crates/pokrov-metrics/src/hooks.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Завершить Phase 1 (Setup)
2. Завершить Phase 2 (Foundational)
3. Завершить Phase 3 (US1)
4. Подтвердить independent acceptance US1
5. Только после этого переходить к US2/US3

### Incremental Delivery

1. Setup + Foundational
2. US1 (non-stream compatibility)
3. US2 (stream parity)
4. US3 (secure split-auth boundary)
5. Polish + final verification

### Parallel Team Strategy

1. После Foundation один поток ведет US1 handler/mapping, второй US2 stream path, третий US3 auth boundary
2. Интеграция выполняется после прохождения обязательных test gates в каждой истории
3. Финальный merge только после Phase 6 verification

---

## Notes

- `[P]` используется только для задач без конфликтов по файлам и незавершенных зависимостей
- Метки `[US1]`, `[US2]`, `[US3]` используются только в фазах пользовательских историй
- Каждая история имеет independent test criterion и обязательные test tasks
- Suggested MVP scope: Phase 1 + Phase 2 + Phase 3 (US1)
