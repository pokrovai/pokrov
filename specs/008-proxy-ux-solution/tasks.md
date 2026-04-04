# Задачи: Proxy UX P0-P2 Improvements

**Вход**: Артефакты проектирования из `specs/008-proxy-ux-solution/`  
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`

**Тесты**: Для этой фичи обязательны unit, integration, performance и security проверки, так как изменения затрагивают proxy/routing/policy/ops behavior.

**Организация**: Задачи сгруппированы по пользовательским историям (`US1`, `US2`, `US3`) и могут поставляться инкрементально.

## Формат: `[ID] [P?] [Story] Description`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Синхронизировать артефакты и тестовые входы под scope P0+P1+P2

- [x] T001 Обновить verification-команды и сценарии в `specs/008-proxy-ux-solution/quickstart.md`
- [x] T002 Обновить пример runtime-конфига для wildcard/fallback/transformers/limits в `config/pokrov.example.yaml`
- [x] T003 [P] Обновить API контракт P0+P1+P2 в `specs/008-proxy-ux-solution/contracts/proxy-ux-api.yaml`
- [x] T004 [P] Обновить config contract P0+P1+P2 в `specs/008-proxy-ux-solution/contracts/proxy-ux-routing-config.yaml`
- [x] T005 Обновить секцию roadmap/scope в `README.md`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Реализовать общий routing/config foundation для всех историй

**CRITICAL**: Ни одна пользовательская история не начинается до завершения этой фазы

- [x] T006 Добавить модели wildcard/fallback/transform профилей в `crates/pokrov-config/src/model.rs`
- [x] T007 Реализовать loader defaults/normalization для новых полей в `crates/pokrov-config/src/loader.rs`
- [x] T008 Реализовать валидацию conflicts/cycles/ambiguity для full routing graph в `crates/pokrov-config/src/validate.rs`
- [x] T009 [P] Добавить unit tests валидации wildcard/fallback/transform/limits в `crates/pokrov-config/src/validate_tests.rs`
- [x] T010 Реализовать unified resolution index (exact->alias->wildcard) в `crates/pokrov-proxy-llm/src/routing.rs`
- [x] T011 [P] Расширить routing types для `resolved_via`, fallback outcome и catalog kind в `crates/pokrov-proxy-llm/src/types.rs`
- [x] T012 [P] Расширить error taxonomy (`wildcard_conflict`, `fallback_exhausted`, transform errors) в `crates/pokrov-proxy-llm/src/errors.rs`
- [x] T013 Реализовать provider-specific upstream endpoint selection в `crates/pokrov-proxy-llm/src/upstream.rs`
- [x] T014 [P] Добавить базовые observability hooks для fallback/transform/provider-model limits в `crates/pokrov-metrics/src/hooks.rs`
- [x] T015 Реализовать counters/labels для fallback/transform/provider-model limits в `crates/pokrov-metrics/src/registry.rs`
- [x] T016 Интегрировать startup/readiness fail-fast для new config validation failures в `crates/pokrov-runtime/src/bootstrap.rs`

**Checkpoint**: Foundation готов, пользовательские истории можно реализовывать независимо

---

## Phase 3: User Story 1 - Автообнаружение и совместимость имени модели (Priority: P1) MVP

**Goal**: Дать стабильный model discovery + deterministic exact/alias routing UX

**Independent Test**: `GET /v1/models` возвращает canonical/alias entries; запросы с canonical/alias маршрутизируются одинаково и детерминированно

### Tests for User Story 1

- [x] T017 [P] [US1] Обновить contract coverage для `GET /v1/models` kinds в `tests/contract/llm_proxy_api_contract.rs`
- [x] T018 [P] [US1] Добавить integration happy-path catalog test для canonical+alias в `tests/integration/llm_proxy_routing_path.rs`
- [x] T019 [P] [US1] Добавить integration test исключения disabled entries из каталога в `tests/integration/startup_config_flow.rs`
- [x] T020 [P] [US1] Добавить unit tests deterministic exact/alias resolution в `crates/pokrov-proxy-llm/src/routing.rs`
- [x] T021 [P] [US1] Добавить security test metadata-only error для unknown model в `tests/security/llm_proxy_metadata_leakage.rs`
- [x] T022 [P] [US1] Добавить performance check alias-resolution overhead в `tests/performance/llm_proxy_overhead_budget.rs`

### Implementation for User Story 1

- [x] T023 [US1] Реализовать/обновить `GET /v1/models` handler в `crates/pokrov-api/src/handlers/models.rs`
- [x] T024 [US1] Подключить route и handler wiring в `crates/pokrov-api/src/handlers/mod.rs` и `crates/pokrov-api/src/app.rs`
- [x] T025 [US1] Реализовать catalog builder (canonical+alias entries) в `crates/pokrov-proxy-llm/src/routing.rs`
- [x] T026 [US1] Интегрировать exact/alias resolution logging fields в `crates/pokrov-proxy-llm/src/handler/mod.rs`
- [x] T027 [US1] Обновить user-facing docs по discovery/aliases в `README.md` и `specs/008-proxy-ux-solution/quickstart.md`

**Checkpoint**: US1 полностью работоспособна и проверяема независимо

---

## Phase 4: User Story 2 - Полноценный multi-provider routing (Priority: P2)

**Goal**: Реализовать wildcard/prefix routing, fallback routing и provider transformers для unified UX

**Independent Test**: Запросы через wildcard/prefix идут в ожидаемые routes; retriable upstream failure переключает на fallback; provider transforms сохраняют клиентский контракт

### Tests for User Story 2

- [x] T028 [P] [US2] Добавить contract coverage кодов `wildcard_conflict`/`fallback_exhausted` в `tests/contract/llm_proxy_stream_contract.rs`
- [x] T029 [P] [US2] Добавить contract coverage transform errors в `tests/contract/llm_proxy_api_contract.rs`
- [x] T030 [P] [US2] Добавить integration test wildcard routing precedence в `tests/integration/llm_proxy_routing_path.rs`
- [x] T031 [P] [US2] Добавить integration test fallback success path в `tests/integration/llm_proxy_upstream_error_path.rs`
- [x] T032 [P] [US2] Добавить integration test fallback exhausted path в `tests/integration/llm_proxy_upstream_error_path.rs`
- [x] T033 [P] [US2] Добавить integration test Anthropic transform happy/error path в `tests/integration/llm_proxy_happy_path.rs`
- [x] T034 [P] [US2] Добавить integration test Gemini transform happy/error path в `tests/integration/llm_proxy_happy_path.rs`
- [x] T035 [P] [US2] Добавить unit tests wildcard specificity ordering в `crates/pokrov-proxy-llm/src/routing.rs`
- [x] T036 [P] [US2] Добавить unit tests fallback trigger matrix в `crates/pokrov-proxy-llm/src/handler/mod.rs`
- [x] T037 [P] [US2] Добавить security tests metadata-only fallback/transform failures в `tests/security/llm_proxy_metadata_leakage.rs`
- [x] T038 [P] [US2] Добавить performance checks для wildcard+fallback overhead в `tests/performance/llm_proxy_overhead_budget.rs`

### Implementation for User Story 2

- [x] T039 [US2] Реализовать wildcard rule model и deterministic precedence в `crates/pokrov-proxy-llm/src/routing.rs`
- [x] T040 [US2] Реализовать fallback chain execution policy в `crates/pokrov-proxy-llm/src/handler/mod.rs`
- [x] T041 [US2] Реализовать upstream retry/fallback orchestration hooks в `crates/pokrov-proxy-llm/src/upstream.rs`
- [x] T042 [US2] Добавить Anthropic request/response transformer в `crates/pokrov-proxy-llm/src/transform/anthropic.rs`
- [x] T043 [US2] Добавить Gemini request/response transformer в `crates/pokrov-proxy-llm/src/transform/gemini.rs`
- [x] T044 [US2] Подключить transformer dispatch в `crates/pokrov-proxy-llm/src/handler/mod.rs`
- [x] T045 [US2] Обновить API error mapping для wildcard/fallback/transform failures в `crates/pokrov-api/src/error.rs`
- [x] T046 [US2] Обновить structured observability (resolution_source, fallback_step, transform_profile) в `crates/pokrov-proxy-llm/src/handler/mod.rs`
- [x] T047 [US2] Обновить docs/examples для wildcard/fallback/multi-provider routing в `README.md` и `specs/008-proxy-ux-solution/quickstart.md`

**Checkpoint**: US1 и US2 работают независимо

---

## Phase 5: User Story 3 - Production-polish и strict drop-in совместимость (Priority: P3)

**Goal**: Довести `/v1/responses` passthrough, provider/model limits и strict metadata mode для production

**Independent Test**: `/v1/responses` работает в native passthrough; provider/model budgets дают предсказуемые 429; suppress-mode стабильно убирает `pokrov` поля

### Tests for User Story 3

- [x] T048 [P] [US3] Обновить contract coverage `/v1/responses` passthrough semantics в `tests/contract/responses_api_contract.rs`
- [x] T049 [P] [US3] Добавить integration test `/v1/responses` passthrough happy-path в `tests/integration/responses_compat_happy_path.rs`
- [x] T050 [P] [US3] Добавить integration test `/v1/responses` unsupported subset error в `tests/integration/responses_stream_malformed_chunk_path.rs`
- [x] T051 [P] [US3] Добавить integration test provider/model rate limit exceeded в `tests/integration/rate_limit_token_budget_path.rs`
- [x] T052 [P] [US3] Добавить unit tests provider/model budget keying в `crates/pokrov-api/src/handlers/rate_limit.rs`
- [x] T053 [P] [US3] Добавить security tests metadata suppression for success+error in responses path в `tests/security/responses_metadata_leakage.rs`
- [x] T054 [P] [US3] Добавить performance checks responses passthrough overhead в `tests/performance/responses_proxy_overhead_budget.rs`

### Implementation for User Story 3

- [x] T055 [US3] Реализовать native `/v1/responses` passthrough path в `crates/pokrov-proxy-llm/src/handler/mod.rs`
- [x] T056 [US3] Обновить responses handler для passthrough + metadata mode consistency в `crates/pokrov-api/src/handlers/responses.rs`
- [x] T057 [US3] Добавить provider/model rate-limit config plumbing в `crates/pokrov-config/src/model.rs` и `crates/pokrov-config/src/loader.rs`
- [x] T058 [US3] Интегрировать provider/model budget evaluation в `crates/pokrov-api/src/handlers/chat_completions.rs` и `crates/pokrov-api/src/handlers/responses.rs`
- [x] T059 [US3] Добавить provider/model rate-limit metrics events в `crates/pokrov-metrics/src/registry.rs`
- [x] T060 [US3] Обновить strict-client compatibility docs для responses passthrough в `README.md` и `specs/008-proxy-ux-solution/quickstart.md`

**Checkpoint**: Все пользовательские истории функциональны независимо

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Завершить интеграцию P0+P1+P2 и зафиксировать evidence

- [x] T061 [P] Синхронизировать `spec.md`, `plan.md`, `contracts/`, `tasks.md` в `specs/008-proxy-ux-solution/`
- [x] T062 [P] Обновить docs verification-отчет в `docs/verification/008-proxy-ux-solution.md`
- [x] T063 Проверить readiness/shutdown semantics с новыми routing failure modes в `tests/integration/readiness_shutdown_flow.rs`
- [x] T064 Проверить logging safety для fallback/transform/rate-limit paths в `tests/security/logging_safety.rs`
- [x] T065 Выполнить full contract suite `cargo test --test contract` и зафиксировать результат в `docs/verification/008-proxy-ux-solution.md`
- [x] T066 Выполнить full integration suite `cargo test --test integration` и зафиксировать результат в `docs/verification/008-proxy-ux-solution.md`
- [x] T067 Выполнить full security/performance suites `cargo test --test security` и `cargo test --test performance` и зафиксировать результат в `docs/verification/008-proxy-ux-solution.md`
- [x] T068 Выполнить `cargo test --workspace` и зафиксировать итоговое evidence в `docs/verification/008-proxy-ux-solution.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: стартует сразу
- **Phase 2 (Foundational)**: зависит от Setup и блокирует все истории
- **Phase 3 (US1)**: зависит от Foundational
- **Phase 4 (US2)**: зависит от Foundational, может идти параллельно с US1 после стабилизации routing core
- **Phase 5 (US3)**: зависит от Foundational и частично от US2 (transform/passthrough hooks)
- **Phase 6 (Polish)**: после завершения целевых историй

### User Story Dependencies

- **US1 (P1)**: MVP increment
- **US2 (P2)**: использует routing foundation и расширяет его wildcard/fallback/transform behavior
- **US3 (P3)**: использует foundation + routing/transform contracts для production hardening

### Within Each User Story

- Сначала тесты (contract/integration/unit/security/perf), затем реализация
- Контракт/модели раньше handler orchestration
- История завершается только после independent acceptance checks

### Parallel Opportunities

- Setup задачи `T003`, `T004` можно делать параллельно
- Foundation задачи `T009`, `T011`, `T012`, `T014` могут идти параллельно
- В US2 тесты `T028-T038` частично параллелятся с реализацией отдельных модулей `T042-T043`
- В US3 тестовые и docs-задачи можно выполнять параллельно с budget plumbing

---

## Parallel Example: User Story 2

```bash
# Параллельные тесты US2
Task: "T030 [US2] wildcard routing precedence в tests/integration/llm_proxy_routing_path.rs"
Task: "T031 [US2] fallback success path в tests/integration/llm_proxy_upstream_error_path.rs"
Task: "T033 [US2] Anthropic transform path в tests/integration/llm_proxy_happy_path.rs"

# Параллельная реализация на разных файлах
Task: "T042 [US2] Anthropic transformer в crates/pokrov-proxy-llm/src/transform/anthropic.rs"
Task: "T043 [US2] Gemini transformer в crates/pokrov-proxy-llm/src/transform/gemini.rs"
Task: "T046 [US2] observability fields в crates/pokrov-proxy-llm/src/handler/mod.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Завершить Phase 1 и Phase 2
2. Реализовать Phase 3 (US1)
3. Подтвердить discovery + alias deterministic routing
4. Затем переходить к US2/US3

### Incremental Delivery

1. Foundation (`T001-T016`)
2. MVP discovery+alias (`T017-T027`)
3. Multi-provider routing (`T028-T047`)
4. Production polish (`T048-T060`)
5. Final verification (`T061-T068`)

### Parallel Team Strategy

1. Один инженер ведет `pokrov-config` + readiness foundation (`T006-T009`, `T016`)
2. Второй инженер ведет routing/transform engine (`T010-T014`, `T039-T044`)
3. Третий инженер ведет API contracts/tests/docs (`T017-T038`, `T048-T068`)

---

## Notes

- `[P]` означает отсутствие незавершенной прямой зависимости и/или непересекающиеся файлы
- Все задачи содержат точные пути к файлам
- Каждая история остается independently testable и independently releasable
