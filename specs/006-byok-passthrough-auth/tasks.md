# Задачи: BYOK Passthrough для шлюза

**Вход**: Артефакты проектирования из `/specs/006-byok-passthrough-auth/`
**Prerequisites**: `plan.md` (обязательно), `spec.md` (обязательно для историй), `research.md`, `data-model.md`, `contracts/`, `quickstart.md`

**Тесты**: Для этой фичи тестовые задачи обязательны, потому что `spec.md` явно требует Unit, Integration, Performance и Security coverage для изменений runtime/proxy/policy/auth.

**Организация**: Задачи сгруппированы по пользовательским историям, чтобы каждая история оставалась independently testable и independently deliverable.

## Формат: `[ID] [P?] [Story] Description`

- **[P]**: можно выполнять параллельно, если файлы не пересекаются и нет зависимости от незавершенных задач
- **[Story]**: идентификатор пользовательской истории (`US1`, `US2`, `US3`)
- Каждая задача содержит точные пути к файлам

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Подготовить документацию, контракты и каркас тестов для BYOK фичи

- [X] T001 Обновить feature runbook для BYOK сценариев в `docs/verification/005-hardening-release.md`
- [X] T002 Обновить примеры BYOK-параметров в `config/pokrov.example.yaml` и `config/README.md`
- [X] T003 [P] Обновить обзор gateway/BYOK режима в `README.md`
- [X] T004 [P] Подключить новые byok test-модули в `tests/contract.rs`, `tests/integration.rs`, `tests/security.rs`, `tests/performance.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Реализовать общие auth/identity primitives, блокирующие все пользовательские истории

**CRITICAL**: Ни одна пользовательская история не должна стартовать до завершения этой фазы

- [X] T005 Добавить конфигурацию `upstream_auth_mode` и identity resolution в `crates/pokrov-config/src/model.rs`
- [X] T006 [P] Добавить загрузку/экспорт новых auth/identity полей в `crates/pokrov-config/src/lib.rs` и `crates/pokrov-config/src/loader.rs`
- [X] T007 Добавить валидацию `upstream_auth_mode` и identity binding правил в `crates/pokrov-config/src/validate.rs` и `crates/pokrov-config/src/validate_tests.rs`
- [X] T008 [P] Добавить runtime типы `ClientIdentity`, `GatewayAuthContext`, `UpstreamCredentialSource` в `crates/pokrov-api/src/app.rs`
- [X] T009 [P] Реализовать извлечение client identity и gateway auth context в `crates/pokrov-api/src/middleware/mod.rs` и `crates/pokrov-api/src/auth.rs`
- [X] T010 Добавить единый metadata-only auth error mapping (`401/422`) в `crates/pokrov-api/src/error.rs`
- [X] T011 [P] Добавить hooks метрик по `auth_mode`/`auth_decision` в `crates/pokrov-metrics/src/hooks.rs` и `crates/pokrov-metrics/src/registry.rs`
- [X] T012 [P] Подключить auth/identity состояния в bootstrap/runtime wiring в `crates/pokrov-runtime/src/bootstrap.rs` и `crates/pokrov-runtime/src/lib.rs`

**Checkpoint**: Foundation готова, пользовательские истории можно реализовывать независимо

---

## Phase 3: User Story 1 - Прозрачный BYOK вызов LLM (Priority: P1) MVP

**Goal**: Дать клиенту BYOK passthrough для LLM path при сохранении sanitization/policy behavior

**Independent Test**: В `passthrough` режиме запрос с валидным gateway auth и валидным provider credential успешно проходит на `/v1/chat/completions`; без provider credential возвращается metadata-only `422` без upstream вызова

### Tests for User Story 1

- [X] T013 [P] [US1] Добавить contract test BYOK auth responses для `/v1/chat/completions` в `tests/contract/llm_proxy_api_contract.rs`
- [X] T014 [P] [US1] Добавить integration test happy path passthrough LLM в `tests/integration/llm_proxy_byok_passthrough_happy_path.rs`
- [X] T015 [P] [US1] Добавить integration test missing provider credential block path в `tests/integration/llm_proxy_byok_passthrough_missing_credential_path.rs`
- [X] T016 [P] [US1] Добавить security test metadata-only auth error body/headers в `tests/security/llm_proxy_byok_metadata_leakage.rs`
- [X] T017 [P] [US1] Добавить unit tests upstream credential source selection в `crates/pokrov-proxy-llm/src/routing.rs`

### Implementation for User Story 1

- [X] T018 [P] [US1] Реализовать выбор upstream credential source (`static`/`passthrough`) в `crates/pokrov-proxy-llm/src/routing.rs` и `crates/pokrov-proxy-llm/src/types.rs`
- [X] T019 [US1] Интегрировать passthrough credential propagation в upstream вызов LLM в `crates/pokrov-proxy-llm/src/handler.rs` и `crates/pokrov-proxy-llm/src/upstream.rs`
- [X] T020 [US1] Добавить pre-upstream блокировку при отсутствующем provider credential в `crates/pokrov-api/src/handlers/chat_completions.rs` и `crates/pokrov-api/src/error.rs`
- [X] T021 [US1] Добавить metadata-only audit поля для auth mode/source в `crates/pokrov-proxy-llm/src/audit.rs`
- [X] T022 [US1] Обновить metrics labels/outcomes для LLM BYOK paths в `crates/pokrov-metrics/src/registry.rs` и `crates/pokrov-proxy-llm/src/handler.rs`

**Checkpoint**: История 1 полностью работоспособна и проверяема независимо

---

## Phase 4: User Story 2 - Разделение клиентской и upstream авторизации (Priority: P2)

**Goal**: Жестко разделить gateway auth и upstream auth с предсказуемыми отказами и без leakage

**Independent Test**: Невалидный gateway auth всегда блокируется на границе Pokrov (`401`) даже при валидном provider key; при валидном gateway auth и невалидном provider key возвращается корректная upstream/auth ошибка без утечки секрета

### Tests for User Story 2

- [X] T023 [P] [US2] Добавить contract test раздельных auth failure semantics в `tests/contract/runtime_api_contract.rs` и `tests/contract/llm_proxy_api_contract.rs`
- [X] T024 [P] [US2] Добавить integration test gateway auth failure before upstream call в `tests/integration/llm_proxy_gateway_auth_failure_path.rs`
- [X] T025 [P] [US2] Добавить integration test valid gateway auth + invalid provider credential path в `tests/integration/llm_proxy_byok_invalid_provider_credential_path.rs`
- [X] T026 [P] [US2] Добавить security test no raw `Authorization` leakage in logs/errors в `tests/security/llm_proxy_auth_validation.rs` и `tests/security/logging_safety.rs`
- [X] T027 [P] [US2] Добавить unit tests gateway auth parser/normalization в `crates/pokrov-api/src/auth.rs`

### Implementation for User Story 2

- [X] T028 [P] [US2] Реализовать отдельный gateway auth extractor (включая `X-Pokrov-Api-Key`) в `crates/pokrov-api/src/auth.rs` и `crates/pokrov-api/src/middleware/mod.rs`
- [X] T029 [US2] Обновить маппинг auth error codes/messages (`gateway_unauthorized`, `upstream_credential_*`) в `crates/pokrov-api/src/error.rs`
- [X] T030 [US2] Сохранить и прокинуть разделенный auth context в LLM и MCP handlers в `crates/pokrov-api/src/handlers/chat_completions.rs` и `crates/pokrov-api/src/handlers/mcp_tool_call.rs`
- [X] T031 [US2] Добавить раздельные metadata-only audit события для gateway/upstream auth outcome в `crates/pokrov-proxy-llm/src/audit.rs` и `crates/pokrov-proxy-mcp/src/audit.rs`
- [X] T032 [US2] Обновить observability/runtime logs по auth stages без секретов в `crates/pokrov-runtime/src/observability.rs` и `crates/pokrov-api/src/middleware/mod.rs`

**Checkpoint**: Истории 1 и 2 работают независимо и не регрессируют друг друга

---

## Phase 5: User Story 3 - Изоляция policy и rate-limit по client identity (Priority: P3)

**Goal**: Привязать policy/rate-limit decisions к client identity и обеспечить межклиентскую изоляцию

**Independent Test**: Два клиента с разными identity получают изолированные profile/rate-limit решения; превышение лимита у одного не влияет на второго

### Tests for User Story 3

- [X] T033 [P] [US3] Добавить contract test identity-aware rate-limit/auth semantics в `tests/contract/runtime_api_contract.rs` и `tests/contract/runtime_config_contract.rs`
- [X] T034 [P] [US3] Добавить integration test identity-isolated rate-limit budget в `tests/integration/byok_identity_rate_limit_isolation_path.rs`
- [X] T035 [P] [US3] Добавить integration test identity-bound profile selection для LLM/MCP paths в `tests/integration/byok_identity_policy_binding_path.rs`
- [X] T036 [P] [US3] Добавить security test metadata-only identity audit/log behavior в `tests/security/rate_limit_metadata_leakage.rs` и `tests/security/mcp_metadata_leakage.rs`
- [X] T037 [P] [US3] Добавить performance regression check BYOK overhead в `tests/performance/llm_proxy_overhead_budget.rs` и `tests/performance/mcp_mediation_overhead_budget.rs`

### Implementation for User Story 3

- [X] T038 [P] [US3] Реализовать identity-to-policy binding resolution в `crates/pokrov-api/src/middleware/mod.rs` и `crates/pokrov-api/src/app.rs`
- [X] T039 [P] [US3] Реализовать identity-based rate-limit keying в `crates/pokrov-api/src/middleware/rate_limit.rs`
- [X] T040 [US3] Применить identity-aware profile fallback в LLM/MCP handlers в `crates/pokrov-api/src/handlers/chat_completions.rs` и `crates/pokrov-api/src/handlers/mcp_tool_call.rs`
- [X] T041 [US3] Добавить auth mode + identity fields в runtime metrics/events в `crates/pokrov-metrics/src/registry.rs` и `crates/pokrov-metrics/src/hooks.rs`
- [X] T042 [US3] Обновить readiness/runtime state checks для identity/auth config readiness в `crates/pokrov-api/src/handlers/ready.rs` и `crates/pokrov-runtime/src/bootstrap.rs`

**Checkpoint**: Все пользовательские истории функциональны независимо

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Финальная синхронизация контрактов, документации и verification matrix

- [X] T043 [P] Синхронизировать contracts с фактическим behavior в `specs/006-byok-passthrough-auth/contracts/byok-auth-api.yaml` и `specs/006-byok-passthrough-auth/contracts/byok-auth-config.yaml`
- [X] T044 [P] Обновить quickstart и feature docs по итоговой реализации в `specs/006-byok-passthrough-auth/quickstart.md` и `specs/006-byok-passthrough-auth/plan.md`
- [X] T045 Обновить verification runbook и команды приемки в `docs/verification/005-hardening-release.md`
- [X] T046 [P] Добавить e2e regression test BYOK static+passthrough flow в `tests/integration/byok_end_to_end_flow.rs`
- [X] T047 Выполнить и зафиксировать финальные проверки в `specs/006-byok-passthrough-auth/checklists/requirements.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: стартует сразу
- **Phase 2 (Foundational)**: зависит от Setup и блокирует все пользовательские истории
- **Phase 3 (US1)**: стартует после завершения Foundational (MVP)
- **Phase 4 (US2)**: стартует после Foundational; может идти параллельно с поздними задачами US1 при отсутствии конфликтов по файлам
- **Phase 5 (US3)**: стартует после Foundational; зависит от готовых auth/identity primitives из US1/US2
- **Phase 6 (Polish)**: выполняется после целевых историй

### User Story Dependencies

- **US1 (P1)**: первая deliverable история, формирует MVP BYOK passthrough LLM path
- **US2 (P2)**: опирается на US1 credential flow, но имеет независимую приемку по разделению auth boundaries
- **US3 (P3)**: использует foundations + auth context для identity-bound policy/rate-limit isolation

### Within Each User Story

- Сначала создаются тесты (contract/integration/security/unit/performance)
- Затем реализуются core auth/policy/runtime wiring задачи
- История считается завершенной только после выполнения independent test criterion

### Parallel Opportunities

- Setup: `T003`, `T004`
- Foundational: `T006`, `T008`, `T009`, `T011`, `T012`
- US1: `T013-T018`
- US2: `T023-T028`
- US3: `T033-T039`
- Polish: `T043`, `T044`, `T046`

---

## Parallel Example: User Story 1

```bash
# Tests in parallel
Task: "T013 [US1] tests/contract/llm_proxy_api_contract.rs"
Task: "T014 [US1] tests/integration/llm_proxy_byok_passthrough_happy_path.rs"
Task: "T016 [US1] tests/security/llm_proxy_byok_metadata_leakage.rs"

# Implementation in parallel
Task: "T018 [US1] crates/pokrov-proxy-llm/src/routing.rs + crates/pokrov-proxy-llm/src/types.rs"
Task: "T021 [US1] crates/pokrov-proxy-llm/src/audit.rs"
```

---

## Parallel Example: User Story 2

```bash
# Tests in parallel
Task: "T023 [US2] tests/contract/runtime_api_contract.rs"
Task: "T024 [US2] tests/integration/llm_proxy_gateway_auth_failure_path.rs"
Task: "T026 [US2] tests/security/logging_safety.rs"

# Implementation in parallel
Task: "T028 [US2] crates/pokrov-api/src/auth.rs + crates/pokrov-api/src/middleware/mod.rs"
Task: "T031 [US2] crates/pokrov-proxy-llm/src/audit.rs + crates/pokrov-proxy-mcp/src/audit.rs"
```

---

## Parallel Example: User Story 3

```bash
# Tests in parallel
Task: "T034 [US3] tests/integration/byok_identity_rate_limit_isolation_path.rs"
Task: "T035 [US3] tests/integration/byok_identity_policy_binding_path.rs"
Task: "T037 [US3] tests/performance/llm_proxy_overhead_budget.rs"

# Implementation in parallel
Task: "T038 [US3] crates/pokrov-api/src/middleware/mod.rs + crates/pokrov-api/src/app.rs"
Task: "T041 [US3] crates/pokrov-metrics/src/registry.rs + crates/pokrov-metrics/src/hooks.rs"
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
2. US1 (passthrough credential flow for LLM)
3. US2 (gateway/upstream auth separation)
4. US3 (identity-bound policy/rate-limit isolation)
5. Polish + final verification

### Parallel Team Strategy

1. После Foundation один поток ведет US1 (LLM credential source), второй US2 (auth boundaries), третий US3 (identity isolation)
2. Интеграция выполняется после прохождения обязательных test gates в каждой истории
3. Финальный merge только после Phase 6 verification

---

## Notes

- `[P]` используется только для задач без конфликтов по файлам и незавершенных зависимостей
- Метки `[US1]`, `[US2]`, `[US3]` используются только в фазах пользовательских историй
- Каждая история имеет independent test criterion и обязательные test tasks
- Suggested MVP scope: Phase 1 + Phase 2 + Phase 3 (US1)
