# Задачи: MCP Mediation

**Вход**: Артефакты проектирования из `/specs/004-mcp-mediation/`
**Prerequisites**: `plan.md` (обязательно), `spec.md` (обязательно для историй), `research.md`, `data-model.md`, `contracts/`, `quickstart.md`

**Тесты**: Для этой фичи тесты обязательны, так как затрагиваются proxy/policy/security/ops path и в `spec.md` явно задано Required Test Coverage (unit, integration, performance, security).

**Организация**: Задачи сгруппированы по пользовательским историям, чтобы каждую историю можно было реализовать и проверить независимо.

## Формат: `[ID] [P?] [Story] Description`

- **[P]**: можно выполнять параллельно, если файлы не пересекаются и нет зависимости от незавершенных задач
- **[Story]**: идентификатор пользовательской истории (`US1`, `US2`, `US3`)
- Каждая задача содержит точные пути к файлам

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Подготовить workspace и базовые артефакты для MCP mediation

- [X] T001 Добавить crate `crates/pokrov-proxy-mcp` в workspace в `Cargo.toml` и создать `crates/pokrov-proxy-mcp/Cargo.toml`
- [X] T002 Создать каркас crate и модули `crates/pokrov-proxy-mcp/src/lib.rs`, `crates/pokrov-proxy-mcp/src/handler.rs`, `crates/pokrov-proxy-mcp/src/policy.rs`, `crates/pokrov-proxy-mcp/src/validate.rs`, `crates/pokrov-proxy-mcp/src/upstream.rs`, `crates/pokrov-proxy-mcp/src/audit.rs`, `crates/pokrov-proxy-mcp/src/types.rs`, `crates/pokrov-proxy-mcp/src/errors.rs`
- [X] T003 [P] Подготовить тестовый support для MCP path в `tests/common/mcp_test_support.rs`
- [X] T004 [P] Добавить MCP пример конфигурации в `config/pokrov.example.yaml` и обновить описание параметров в `config/README.md`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Реализовать базовые контракты, wiring и observability до начала пользовательских историй

**CRITICAL**: Ни одна пользовательская история не должна стартовать до завершения этой фазы

- [X] T005 Добавить MCP config-модели (`Mcp`, `McpServerDefinition`, `McpToolPolicy`, `ToolArgumentConstraints`) в `crates/pokrov-config/src/model.rs`
- [X] T006 [P] Реализовать загрузку MCP config-секции в `crates/pokrov-config/src/loader.rs` и экспорт через `crates/pokrov-config/src/lib.rs`
- [X] T007 Реализовать семантическую валидацию MCP config (unique server id, endpoint uniqueness, blocked>allowed precedence checks) в `crates/pokrov-config/src/validate.rs`
- [X] T008 [P] Реализовать request/response DTO и internal flow types из data-model в `crates/pokrov-proxy-mcp/src/types.rs`
- [X] T009 Реализовать MCP error type и mapping на HTTP-safe коды в `crates/pokrov-proxy-mcp/src/errors.rs` и `crates/pokrov-api/src/error.rs`
- [X] T010 [P] Добавить handler wiring для `POST /v1/mcp/tool-call` в `crates/pokrov-api/src/handlers/mod.rs`, `crates/pokrov-api/src/handlers/mcp_tool_call.rs`, `crates/pokrov-api/src/app.rs`
- [X] T011 [P] Добавить MCP metrics (`mcp_tool_calls_total`, `mcp_tool_calls_blocked_total`, `mcp_tool_call_duration_ms`) в `crates/pokrov-metrics/src/registry.rs` и hooks в `crates/pokrov-metrics/src/hooks.rs`
- [X] T012 Добавить runtime bootstrap/readiness wiring для MCP mediation dependencies в `crates/pokrov-runtime/src/bootstrap.rs`, `crates/pokrov-api/src/handlers/ready.rs`

**Checkpoint**: Foundation готова - пользовательские истории можно реализовывать независимо

---

## Phase 3: User Story 1 - Разрешенный вызов approved tool (Priority: P1) MVP

**Goal**: Обеспечить allow-путь MCP tool call с upstream execution и output sanitization

**Independent Test**: Вызвать allowlisted `server/tool` с корректными аргументами через `POST /v1/mcp/tool-call` и подтвердить `200`, `allowed=true`, metadata-only `pokrov.*` и sanitization результата

### Tests for User Story 1

- [X] T013 [P] [US1] Добавить contract test success-response для `/v1/mcp/tool-call` в `tests/contract/mcp_mediation_api_contract.rs`
- [X] T014 [P] [US1] Добавить integration test allowed tool path в `tests/integration/mcp_allowed_tool_path.rs`
- [X] T015 [P] [US1] Добавить integration test sanitized output path в `tests/integration/mcp_output_sanitization_path.rs`
- [X] T016 [P] [US1] Добавить unit tests JSON-safe string-leaf sanitization для MCP result в `crates/pokrov-proxy-mcp/src/handler.rs`

### Implementation for User Story 1

- [X] T017 [P] [US1] Реализовать upstream tool-call клиент с timeout handling в `crates/pokrov-proxy-mcp/src/upstream.rs`
- [X] T018 [P] [US1] Реализовать allow decision для разрешенных server/tool в `crates/pokrov-proxy-mcp/src/policy.rs`
- [X] T019 [US1] Реализовать happy-path orchestration в `crates/pokrov-proxy-mcp/src/handler.rs`
- [X] T020 [US1] Интегрировать output sanitization через `pokrov-core` и success metadata (`profile/action/rule_hits`) в `crates/pokrov-proxy-mcp/src/handler.rs` и `crates/pokrov-proxy-mcp/src/types.rs`
- [X] T021 [US1] Реализовать metadata-only audit события allow-path в `crates/pokrov-proxy-mcp/src/audit.rs`

**Checkpoint**: История 1 полностью работоспособна и проверяема независимо

---

## Phase 4: User Story 2 - Блокировка запрещенного вызова (Priority: P2)

**Goal**: Блокировать неallowlisted server/tool и unsafe arguments до upstream execution с безопасным explain summary

**Independent Test**: Передать запросы с неразрешенным `server/tool` и невалидными аргументами, подтвердить `403/422`, `allowed=false`, `error.code` и отсутствие upstream execution

### Tests for User Story 2

- [X] T022 [P] [US2] Добавить contract tests для blocked/validation error responses в `tests/contract/mcp_mediation_api_contract.rs`
- [X] T023 [P] [US2] Добавить integration test blocked server/tool path в `tests/integration/mcp_blocked_tool_path.rs`
- [X] T024 [P] [US2] Добавить integration test invalid arguments path в `tests/integration/mcp_argument_validation_path.rs`
- [X] T025 [P] [US2] Добавить security test block-before-upstream и metadata-only error details в `tests/security/mcp_block_before_execution.rs`
- [X] T026 [P] [US2] Добавить unit tests blocklist precedence и formatter block-response в `crates/pokrov-proxy-mcp/src/policy.rs` и `crates/pokrov-proxy-mcp/src/errors.rs`

### Implementation for User Story 2

- [X] T027 [P] [US2] Реализовать deterministic policy precedence (`server allowlist -> tool allowlist -> blocklist precedence`) в `crates/pokrov-proxy-mcp/src/policy.rs`
- [X] T028 [P] [US2] Реализовать двухстадийную validation pipeline (schema + constraints) в `crates/pokrov-proxy-mcp/src/validate.rs`
- [X] T029 [US2] Реализовать mapping `tool_call_blocked`/`argument_validation_failed` на `403/422` в `crates/pokrov-proxy-mcp/src/handler.rs` и `crates/pokrov-proxy-mcp/src/errors.rs`
- [X] T030 [US2] Обеспечить short-circuit block path без upstream вызова в `crates/pokrov-proxy-mcp/src/handler.rs` и `crates/pokrov-proxy-mcp/src/upstream.rs`
- [X] T031 [US2] Добавить metadata-only explain details (`server`, `tool`, `reason`, `violation_count`) и audit reason mapping в `crates/pokrov-proxy-mcp/src/types.rs` и `crates/pokrov-proxy-mcp/src/audit.rs`

**Checkpoint**: Истории 1 и 2 работают независимо и не регрессируют друг друга

---

## Phase 5: User Story 3 - Минимальный pilot subset MCP transport (Priority: P3)

**Goal**: Зафиксировать pilot subset MCP mediation surface с предсказуемым отказом для out-of-scope variants и операционной готовностью

**Independent Test**: Прогнать pilot-compatible MCP сценарии через endpoint и убедиться, что unsupported variant отклоняется предсказуемо, upstream unavailable даёт `503`, а endpoint защищен API key

### Tests for User Story 3

- [X] T032 [P] [US3] Добавить contract test predictable unsupported-variant rejection в `tests/contract/mcp_mediation_api_contract.rs`
- [X] T033 [P] [US3] Добавить integration test pilot subset end-to-end flow в `tests/integration/mcp_pilot_subset_path.rs`
- [X] T034 [P] [US3] Добавить integration test upstream unavailable mapping (`503 upstream_unavailable`) в `tests/integration/mcp_upstream_unavailable_path.rs`
- [X] T035 [P] [US3] Добавить security test invalid API key для MCP endpoint в `tests/security/mcp_auth_validation.rs`
- [X] T036 [P] [US3] Добавить performance test MCP overhead budget (`p95<=50ms`, `p99<=100ms`) в `tests/performance/mcp_mediation_overhead_budget.rs`

### Implementation for User Story 3

- [X] T037 [P] [US3] Реализовать pilot subset guard и deterministic unsupported-variant error в `crates/pokrov-proxy-mcp/src/handler.rs` и `crates/pokrov-proxy-mcp/src/errors.rs`
- [X] T038 [US3] Реализовать upstream failure mapping (`502 upstream_error`, `503 upstream_unavailable`) в `crates/pokrov-proxy-mcp/src/upstream.rs` и `crates/pokrov-proxy-mcp/src/handler.rs`
- [X] T039 [US3] Расширить readiness check валидностью MCP allowlist config в `crates/pokrov-api/src/handlers/ready.rs` и `crates/pokrov-runtime/src/bootstrap.rs`
- [X] T040 [US3] Обновить MCP endpoint contract и policy schema по pilot subset ограничениям в `specs/004-mcp-mediation/contracts/mcp-mediation-api.yaml` и `specs/004-mcp-mediation/contracts/mcp-policy.schema.yaml`

**Checkpoint**: Все пользовательские истории функциональны независимо и покрыты required test evidence

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Финальные сквозные проверки и эксплуатационная документация

- [X] T041 [P] Обновить quickstart и feature docs для allowlist semantics, validation rules и scope boundaries в `specs/004-mcp-mediation/quickstart.md` и `docs/verification/004-mcp-mediation.md`
- [X] T042 Проверить metadata-only logging/audit safety в `tests/security/mcp_metadata_leakage.rs` и `tests/security/logging_safety.rs`
- [X] T043 [P] Добавить/обновить runtime API contract coverage для MCP route в `tests/contract/runtime_api_contract.rs`
- [X] T044 Зафиксировать финальный verification runbook и команды в `docs/verification/004-mcp-mediation.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: стартует сразу
- **Phase 2 (Foundational)**: зависит от Setup и блокирует все пользовательские истории
- **Phase 3 (US1)**: стартует после Phase 2 (MVP)
- **Phase 4 (US2)**: стартует после Phase 2; рекомендуется после базового завершения US1 для reuse handler/policy surfaces
- **Phase 5 (US3)**: стартует после Phase 2; зависит от существующего MCP endpoint behavior из US1/US2
- **Phase 6 (Polish)**: после завершения целевых пользовательских историй

### User Story Dependencies

- **US1 (P1)**: не зависит от US2/US3, формирует MVP allow-path
- **US2 (P2)**: использует те же policy/handler surfaces, но имеет независимую приемку через block/validation сценарии
- **US3 (P3)**: опирается на endpoint и error model из US1/US2, добавляет pilot subset guard и operability evidence

### Within Each User Story

- Тесты создаются до основных implementation задач
- Policy/validation logic реализуется до handler orchestration
- Story закрывается только после выполнения independent test criteria

### Parallel Opportunities

- Setup: `T003`, `T004` можно делать параллельно после `T001-T002`
- Foundational: `T006`, `T008`, `T010`, `T011` можно вести параллельно при готовом `T005`
- US1: `T013-T016` и `T017-T018` можно распараллелить; `T019-T021` последовательно
- US2: `T022-T026` и `T027-T028` можно распараллелить; `T029-T031` последовательно
- US3: `T032-T036` и `T037` можно распараллелить; `T038-T040` после базовой готовности handler/upstream
- Polish: `T041` и `T043` параллельно; `T042` и `T044` после стабилизации test coverage

---

## Parallel Example: User Story 2

```bash
# Параллельные тестовые треки:
Task: "T023 [US2] tests/integration/mcp_blocked_tool_path.rs"
Task: "T024 [US2] tests/integration/mcp_argument_validation_path.rs"
Task: "T025 [US2] tests/security/mcp_block_before_execution.rs"

# Параллельные implementation треки:
Task: "T027 [US2] crates/pokrov-proxy-mcp/src/policy.rs"
Task: "T028 [US2] crates/pokrov-proxy-mcp/src/validate.rs"
```

---

## Implementation Strategy

### MVP First (US1)

1. Завершить Phase 1 и Phase 2
2. Реализовать и проверить US1 (Phase 3)
3. Подтвердить independent acceptance US1 (`200 allowed`, sanitized output, metadata-only audit)
4. Переходить к US2/US3 после фиксации MVP evidence

### Incremental Delivery

1. Foundation (Setup + Foundational)
2. US1 (allow-path MVP)
3. US2 (block/validation hardening)
4. US3 (pilot subset scope + operability)
5. Polish (cross-cutting verification and docs)

### Parallel Team Strategy

1. Один инженер ведет config/runtime/route foundation (`T005-T012`)
2. После foundation команда делится: allow-path (US1), block/validation (US2), operability/perf/security (US3 tests)
3. Интеграция только после прохождения обязательных test gates и metadata-only safety checks

---

## Notes

- `[P]` используется только для задач без конфликтов по файлам и прямых зависимостей
- Метки `[US1]`, `[US2]`, `[US3]` применяются только к задачам пользовательских историй
- Каждая история имеет свой independent test criterion и набор acceptance-oriented тестов
- Все задачи сформулированы так, чтобы быть исполнимыми без дополнительного контекста
