# Задачи: Hardening Release

**Вход**: Артефакты проектирования из `specs/005-hardening-release/`
**Prerequisites**: `plan.md` (обязательно), `spec.md` (обязательно для историй), `research.md`, `data-model.md`, `contracts/`, `quickstart.md`

**Тесты**: Для этой фичи тесты обязательны, так как `spec.md` явно требует Unit, Integration, Performance и Security coverage для runtime/proxy/policy/ops изменений.

**Организация**: Задачи сгруппированы по пользовательским историям, чтобы каждая история была independently testable и deliverable.

## Формат: `[ID] [P?] [Story] Description`

- **[P]**: можно выполнять параллельно, если файлы не пересекаются и нет зависимости от незавершенных задач
- **[Story]**: идентификатор пользовательской истории (`US1`, `US2`, `US3`)
- Каждая задача содержит точные пути к файлам

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Подготовить артефакты и тестовый каркас hardening-release

- [X] T001 Создать runbook hardening в `docs/verification/005-hardening-release.md`
- [X] T002 Обновить базовый пример и описание hardening-конфига в `config/pokrov.example.yaml` и `config/README.md`
- [X] T003 [P] Добавить общий тестовый support для rate-limit/metrics сценариев в `tests/common/hardening_test_support.rs`
- [X] T004 [P] Подключить новые hardening test-модули в `tests/contract.rs`, `tests/integration.rs`, `tests/performance.rs`, `tests/security.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Реализовать общие блокирующие компоненты до старта пользовательских историй

**CRITICAL**: Ни одна пользовательская история не должна стартовать до завершения этой фазы

- [X] T005 Добавить модели rate-limit конфигурации (`RateLimitConfig`, `RateLimitProfile`) в `crates/pokrov-config/src/model.rs`
- [X] T006 [P] Реализовать загрузку/экспорт rate-limit конфигурации в `crates/pokrov-config/src/loader.rs` и `crates/pokrov-config/src/lib.rs`
- [X] T007 Добавить валидацию rate-limit профилей и enforcement-mode в `crates/pokrov-config/src/validate.rs`
- [X] T008 [P] Добавить shared runtime типы лимитера (`RateLimitDecision`, `RateLimitWindowState`) в `crates/pokrov-api/src/app.rs`
- [X] T009 [P] Реализовать sliding-window evaluator с monotonic time source в `crates/pokrov-api/src/middleware/rate_limit.rs`
- [X] T010 Подключить rate-limit middleware в `crates/pokrov-api/src/middleware/mod.rs` и `crates/pokrov-api/src/app.rs`
- [X] T011 [P] Добавить mandatory hardening metrics hooks в `crates/pokrov-metrics/src/hooks.rs` и `crates/pokrov-metrics/src/registry.rs`
- [X] T012 [P] Добавить endpoint `/metrics` в `crates/pokrov-api/src/handlers/metrics.rs`, `crates/pokrov-api/src/handlers/mod.rs`, `crates/pokrov-api/src/app.rs`
- [X] T013 Добавить helper для predictable `429 rate_limit_exceeded` responses в `crates/pokrov-api/src/error.rs`

**Checkpoint**: Foundation готова, пользовательские истории можно реализовывать независимо

---

## Phase 3: User Story 1 - Контроль нагрузки и злоупотреблений (Priority: P1) MVP

**Goal**: Детерминированно ограничивать запросы по API key и token-like budget с predictable `429` ответом

**Independent Test**: Отправить burst запросов выше budget и подтвердить `429` с `request_id`, `retry_after_ms`, `limit`, `remaining`, `reset_at` и rate-limit headers

### Tests for User Story 1

- [X] T014 [P] [US1] Добавить contract checks для `429` body/headers на `/v1/chat/completions` и `/v1/mcp/tools/{toolName}/invoke` в `tests/contract/llm_proxy_api_contract.rs` и `tests/contract/mcp_mediation_api_contract.rs`
- [X] T015 [P] [US1] Добавить integration test превышения request budget в `tests/integration/rate_limit_request_budget_path.rs`
- [X] T016 [P] [US1] Добавить integration test исчерпания token-like budget для LLM path в `tests/integration/rate_limit_token_budget_path.rs`
- [X] T017 [P] [US1] Добавить unit tests sliding-window расчета (`remaining`, `retry_after_ms`, reset) в `crates/pokrov-api/src/middleware/rate_limit.rs`
- [X] T018 [P] [US1] Добавить security test metadata-only ограничений для `429` ответа в `tests/security/rate_limit_metadata_leakage.rs`

### Implementation for User Story 1

- [X] T019 [P] [US1] Интегрировать limiter enforcement в `crates/pokrov-api/src/handlers/chat_completions.rs`, `crates/pokrov-api/src/handlers/mcp_tool_call.rs`, `crates/pokrov-api/src/middleware/rate_limit.rs`
- [X] T020 [P] [US1] Реализовать token-like estimator по санитизированному LLM payload в `crates/pokrov-proxy-llm/src/normalize.rs` и `crates/pokrov-proxy-llm/src/handler.rs`
- [X] T021 [US1] Добавить predictable `429` mapping и `Retry-After`/`X-RateLimit-*` headers в `crates/pokrov-api/src/error.rs` и `crates/pokrov-api/src/middleware/rate_limit.rs`
- [X] T022 [US1] Добавить metadata-only audit полей rate-limit decisions в `crates/pokrov-proxy-llm/src/audit.rs` и `crates/pokrov-proxy-mcp/src/audit.rs`
- [X] T023 [US1] Реализовать profile-aware лимиты по API key binding в `crates/pokrov-api/src/app.rs` и `crates/pokrov-api/src/middleware/rate_limit.rs`

**Checkpoint**: История 1 полностью работоспособна и проверяема независимо

---

## Phase 4: User Story 2 - Наблюдаемость и безопасность логирования (Priority: P2)

**Goal**: Публиковать обязательные Prometheus metrics и гарантировать metadata-only structured logging

**Independent Test**: Прогнать allow/blocked/upstream-error сценарии и подтвердить наличие mandatory series и отсутствие raw payload в логах

### Tests for User Story 2

- [X] T024 [P] [US2] Добавить contract test `/metrics` и mandatory series presence в `tests/contract/runtime_api_contract.rs` и `tests/contract/llm_proxy_metadata_contract.rs`
- [X] T025 [P] [US2] Добавить integration test роста series `requests/blocked/upstream_errors/rate_limit_events/duration` в `tests/integration/hardening_metrics_flow.rs`
- [X] T026 [P] [US2] Расширить security tests log-safety для LLM/MCP allow+block сценариев в `tests/security/logging_safety.rs`, `tests/security/llm_proxy_metadata_leakage.rs`, `tests/security/mcp_metadata_leakage.rs`
- [X] T027 [P] [US2] Добавить unit tests forbidden-label и low-cardinality guardrail в `crates/pokrov-metrics/src/registry.rs`

### Implementation for User Story 2

- [X] T028 [P] [US2] Реализовать mandatory metrics catalog и label allowlist enforcement в `crates/pokrov-metrics/src/registry.rs` и `crates/pokrov-metrics/src/hooks.rs`
- [X] T029 [P] [US2] Реализовать metrics exposition handler и маршрутизацию `/metrics` в `crates/pokrov-api/src/handlers/metrics.rs`, `crates/pokrov-api/src/handlers/mod.rs`, `crates/pokrov-api/src/app.rs`
- [X] T030 [US2] Привести runtime structured logging к allowlisted metadata полям в `crates/pokrov-api/src/middleware/mod.rs` и `crates/pokrov-runtime/src/observability.rs`
- [X] T031 [US2] Добавить metrics labels по decision/provider/status для LLM/MCP path в `crates/pokrov-proxy-llm/src/handler.rs` и `crates/pokrov-proxy-mcp/src/handler.rs`
- [X] T032 [US2] Обновить readiness degradation checks для observability path в `crates/pokrov-api/src/handlers/ready.rs` и `crates/pokrov-runtime/src/lifecycle.rs`

**Checkpoint**: Истории 1 и 2 работают независимо и не регрессируют друг друга

---

## Phase 5: User Story 3 - Подтверждение release readiness (Priority: P3)

**Goal**: Подготовить repeatable release verification и self-hosted deployment package с metadata-only evidence

**Independent Test**: Выполнить performance/security/operational checks, сформировать `release-evidence.json` и проверить deploy по инструкции

### Tests for User Story 3

- [X] T033 [P] [US3] Добавить contract test соответствия `release-evidence.json` schema в `tests/contract/release_evidence_contract.rs`
- [X] T034 [P] [US3] Добавить performance test p95/p99 overhead и throughput baseline в `tests/performance/hardening_release_overhead_budget.rs`
- [X] T035 [P] [US3] Расширить startup-time verification (`<=5s`) в `tests/performance/bootstrap_probes.rs`
- [X] T036 [P] [US3] Добавить security test invalid-auth/rate-limit-abuse/log-safety/secret-handling bundle checks в `tests/security/hardening_release_security_checks.rs`
- [X] T037 [P] [US3] Добавить integration test graceful degradation при upstream error и shutdown под нагрузкой в `tests/integration/hardening_degraded_shutdown_flow.rs`

### Implementation for User Story 3

- [X] T038 [P] [US3] Реализовать сборщик release evidence и schema validation в `crates/pokrov-runtime/src/release_evidence.rs` и `crates/pokrov-runtime/src/lib.rs`
- [X] T039 [P] [US3] Добавить CLI flow генерации evidence/checksums в `crates/pokrov-runtime/src/main.rs` и `crates/pokrov-runtime/src/bootstrap.rs`
- [X] T040 [US3] Добавить release package manifest и verification checklist в `config/release/manifest.yaml` и `config/release/verification-checklist.md`
- [X] T041 [US3] Обновить deployment/runbook документацию в `docs/verification/005-hardening-release.md` и `specs/005-hardening-release/quickstart.md`
- [X] T042 [US3] Реализовать gate aggregation (`pass/fail`) по performance/security/operational evidence в `crates/pokrov-runtime/src/release_evidence.rs` и `tests/contract/release_evidence_contract.rs`

**Checkpoint**: Все пользовательские истории функциональны независимо и покрыты release evidence

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Закрыть кросс-исторные доработки и финальную верификацию

- [X] T043 [P] Синхронизировать feature-контракты с финальной реализацией в `specs/005-hardening-release/contracts/hardening-api.yaml`, `specs/005-hardening-release/contracts/metrics-catalog.yaml`, `specs/005-hardening-release/contracts/release-evidence.schema.yaml`
- [X] T044 [P] Обновить пользовательскую документацию и конфиг-гайд по hardening knobs в `README.md` и `config/README.md`
- [X] T045 Зафиксировать финальный verification matrix и команды в `docs/verification/005-hardening-release.md`
- [X] T046 Добавить сквозной regression test rate-limit+metrics+logging+evidence flow в `tests/integration/hardening_end_to_end_release_flow.rs`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: стартует сразу
- **Phase 2 (Foundational)**: зависит от Setup и блокирует все пользовательские истории
- **Phase 3 (US1)**: стартует после завершения Foundational (MVP)
- **Phase 4 (US2)**: стартует после Foundational; может идти параллельно с поздними задачами US1 при отсутствии конфликтов файлов
- **Phase 5 (US3)**: стартует после Foundational; зависит от готовности runtime/metrics/rate-limit surfaces
- **Phase 6 (Polish)**: выполняется после целевых историй

### User Story Dependencies

- **US1 (P1)**: первая deliverable история, формирует MVP hardening
- **US2 (P2)**: опирается на US1 metrics hooks и runtime behavior, но имеет независимую приемку по observability/logging safety
- **US3 (P3)**: использует результаты US1/US2 для release evidence и packaging, с независимой приемкой по readiness checks

### Within Each User Story

- Сначала создаются тесты (contract/integration/unit/performance/security)
- Затем реализуются core models/handlers/runtime wiring
- История считается завершенной только после выполнения independent test criteria и evidence

### Parallel Opportunities

- Setup: `T003`, `T004`
- Foundational: `T006`, `T008`, `T009`, `T011`, `T012`
- US1: `T014-T020`
- US2: `T024-T029`
- US3: `T033-T039`
- Polish: `T043`, `T044`

---

## Parallel Example: User Story 1

```bash
# Tests in parallel
Task: "T014 [US1] tests/contract/llm_proxy_api_contract.rs + tests/contract/mcp_mediation_api_contract.rs"
Task: "T015 [US1] tests/integration/rate_limit_request_budget_path.rs"
Task: "T016 [US1] tests/integration/rate_limit_token_budget_path.rs"
Task: "T018 [US1] tests/security/rate_limit_metadata_leakage.rs"

# Implementation in parallel
Task: "T019 [US1] crates/pokrov-api/src/handlers/chat_completions.rs + crates/pokrov-api/src/handlers/mcp_tool_call.rs"
Task: "T020 [US1] crates/pokrov-proxy-llm/src/normalize.rs"
```

---

## Parallel Example: User Story 2

```bash
# Tests in parallel
Task: "T024 [US2] tests/contract/runtime_api_contract.rs"
Task: "T025 [US2] tests/integration/hardening_metrics_flow.rs"
Task: "T026 [US2] tests/security/logging_safety.rs"

# Implementation in parallel
Task: "T028 [US2] crates/pokrov-metrics/src/registry.rs"
Task: "T029 [US2] crates/pokrov-api/src/handlers/metrics.rs"
```

---

## Parallel Example: User Story 3

```bash
# Tests in parallel
Task: "T033 [US3] tests/contract/release_evidence_contract.rs"
Task: "T034 [US3] tests/performance/hardening_release_overhead_budget.rs"
Task: "T036 [US3] tests/security/hardening_release_security_checks.rs"

# Implementation in parallel
Task: "T038 [US3] crates/pokrov-runtime/src/release_evidence.rs"
Task: "T040 [US3] config/release/manifest.yaml"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Завершить Phase 1 (Setup)
2. Завершить Phase 2 (Foundational)
3. Завершить Phase 3 (US1)
4. Подтвердить independent acceptance US1 по predictable `429`
5. Только после этого переходить к US2/US3

### Incremental Delivery

1. Setup + Foundational
2. US1 (rate limiting)
3. US2 (metrics + logging safety)
4. US3 (release readiness + packaging)
5. Polish + final verification matrix

### Parallel Team Strategy

1. После Foundation один поток ведет US1 runtime throttling, второй US2 observability, третий US3 release evidence
2. Интеграция выполняется после прохождения обязательных test gates в каждой истории
3. Финальный merge только после T045-T046

---

## Notes

- `[P]` используется только для задач без конфликтов по файлам и незавершенных зависимостей
- Метки `[US1]`, `[US2]`, `[US3]` используются только в фазах пользовательских историй
- Каждая история имеет independent test criterion и обязательные test tasks
- Все задачи сформулированы как исполнимые без дополнительного контекста
