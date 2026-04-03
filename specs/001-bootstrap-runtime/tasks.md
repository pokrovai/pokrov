# Задачи: Bootstrap Runtime

**Вход**: Артефакты проектирования из `specs/001-bootstrap-runtime/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md`

**Тесты**: Для этой runtime-фичи обязательны unit, integration, performance и
security tasks согласно `spec.md`.

**Организация**: Задачи сгруппированы по пользовательским историям так, чтобы
каждую историю можно было реализовать и проверить независимо после завершения
Setup и Foundational phases.

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Инициализация Rust workspace и базовой структуры проекта

- [X] T001 Создать workspace manifests в `Cargo.toml`, `crates/pokrov-api/Cargo.toml`, `crates/pokrov-config/Cargo.toml`, `crates/pokrov-metrics/Cargo.toml` и `crates/pokrov-runtime/Cargo.toml`
- [X] T002 [P] Создать базовые entrypoints и модульные заглушки в `crates/pokrov-api/src/lib.rs`, `crates/pokrov-config/src/lib.rs`, `crates/pokrov-metrics/src/lib.rs` и `crates/pokrov-runtime/src/lib.rs`
- [X] T003 [P] Настроить workspace-level formatting, linting и ignore rules в `Cargo.toml`, `rustfmt.toml` и `.gitignore`
- [X] T004 [P] Подготовить пример bootstrap-конфига и операторские заметки в `config/pokrov.example.yaml` и `config/README.md`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Общие компоненты runtime, без которых пользовательские истории не стартуют

**CRITICAL**: Ни одна пользовательская история не начинается до завершения этой фазы

- [X] T005 Реализовать typed config/domain models и ошибки bootstrap-конфига в `crates/pokrov-config/src/model.rs` и `crates/pokrov-config/src/error.rs`
- [X] T006 [P] Реализовать lifecycle state holder и shared runtime state в `crates/pokrov-runtime/src/lifecycle.rs`
- [X] T007 [P] Реализовать metrics hook boundary и registry scaffolding в `crates/pokrov-metrics/src/hooks.rs` и `crates/pokrov-metrics/src/registry.rs`
- [X] T008 [P] Создать базовый router, app state и общие probe response types в `crates/pokrov-api/src/app.rs` и `crates/pokrov-api/src/handlers/mod.rs`
- [X] T009 Реализовать bootstrap entrypoint и binary shell в `crates/pokrov-runtime/src/bootstrap.rs`, `crates/pokrov-runtime/src/main.rs` и `crates/pokrov-runtime/src/lib.rs`
- [X] T010 [P] Создать общую middleware/error scaffolding для HTTP слоя в `crates/pokrov-api/src/middleware/mod.rs` и `crates/pokrov-api/src/error.rs`

**Checkpoint**: Workspace, lifecycle shell и app wiring готовы; можно реализовывать истории

---

## Phase 3: User Story 1 - Запуск сервиса с валидным конфигом (Priority: P1) MVP

**Goal**: Сервис стартует только с валидным YAML-конфигом и становится ready после завершения инициализации

**Independent Test**: Запустить сервис с валидным конфигом и убедиться, что `/health` отвечает успешно, `/ready` становится ready после инициализации, а невалидный конфиг завершает startup с понятной ошибкой

### Tests for User Story 1

- [X] T011 [P] [US1] Добавить contract test для YAML config schema в `tests/contract/runtime_config_contract.rs`
- [X] T012 [P] [US1] Добавить integration test для валидного старта и отказа на невалидном конфиге в `tests/integration/startup_config_flow.rs`
- [X] T013 [P] [US1] Добавить unit tests для parsing и semantic validation конфига в `crates/pokrov-config/src/validate.rs`

### Implementation for User Story 1

- [X] T014 [P] [US1] Реализовать YAML loader и file-based config ingestion в `crates/pokrov-config/src/loader.rs`
- [X] T015 [US1] Реализовать semantic config validation, secret reference rules и startup error mapping в `crates/pokrov-config/src/validate.rs` и `crates/pokrov-config/src/error.rs`
- [X] T016 [P] [US1] Реализовать probe handlers для happy-path startup в `crates/pokrov-api/src/handlers/health.rs` и `crates/pokrov-api/src/handlers/ready.rs`
- [X] T017 [US1] Подключить config loading и переход `starting -> ready` в `crates/pokrov-runtime/src/bootstrap.rs`, `crates/pokrov-runtime/src/main.rs` и `crates/pokrov-api/src/app.rs`
- [X] T018 [US1] Зафиксировать рабочий локальный сценарий запуска и обязательные поля конфига в `config/README.md` и `specs/001-bootstrap-runtime/quickstart.md`

**Checkpoint**: История 1 независимо проверяется локальным запуском с валидным и невалидным YAML

---

## Phase 4: User Story 2 - Проверка состояния сервиса (Priority: P2)

**Goal**: Оператор различает liveness и readiness во время старта и graceful shutdown

**Independent Test**: Во время старта `/ready` возвращает not-ready, а при `SIGTERM` сервис сначала переходит в draining/not-ready и только потом завершает процесс

### Tests for User Story 2

- [X] T019 [P] [US2] Добавить contract test для состояний `/health` и `/ready` по `runtime-api.yaml` в `tests/contract/runtime_api_contract.rs`
- [X] T020 [P] [US2] Добавить integration test для startup-pending и graceful shutdown с активным запросом в `tests/integration/readiness_shutdown_flow.rs`
- [X] T021 [P] [US2] Добавить unit tests для lifecycle transitions и drain timeout logic в `crates/pokrov-runtime/src/lifecycle.rs`

### Implementation for User Story 2

- [X] T022 [P] [US2] Реализовать lifecycle-driven readiness checks и active request tracking в `crates/pokrov-runtime/src/lifecycle.rs`
- [X] T023 [US2] Реализовать readiness semantics и state-aware probe responses в `crates/pokrov-api/src/handlers/ready.rs` и `crates/pokrov-api/src/app.rs`
- [X] T024 [US2] Реализовать signal handling и graceful shutdown orchestration в `crates/pokrov-runtime/src/bootstrap.rs` и `crates/pokrov-runtime/src/main.rs`
- [X] T025 [US2] Подключить lifecycle events к metrics hooks без изменения публичного probe behavior в `crates/pokrov-metrics/src/hooks.rs` и `crates/pokrov-metrics/src/registry.rs`

**Checkpoint**: История 2 независимо подтверждает корректное поведение probes на startup и shutdown

---

## Phase 5: User Story 3 - Трассировка запросов и событий (Priority: P3)

**Goal**: Каждый запрос и lifecycle event коррелируются через `request_id`, а structured logs остаются metadata-only

**Independent Test**: Отправить запрос к runtime и убедиться, что ответ и JSON-лог содержат одинаковый `request_id`, а логи не включают raw payload или открытые секреты

### Tests for User Story 3

- [X] T026 [P] [US3] Добавить integration test для `x-request-id` propagation и response correlation в `tests/integration/request_id_logging_flow.rs`
- [X] T027 [P] [US3] Добавить security test для metadata-only logging и запрета raw secret leakage в `tests/security/logging_safety.rs`
- [X] T028 [P] [US3] Добавить unit tests для генерации и нормализации `request_id` middleware в `crates/pokrov-api/src/middleware/request_id.rs`

### Implementation for User Story 3

- [X] T029 [P] [US3] Реализовать request context и `request_id` middleware в `crates/pokrov-api/src/middleware/request_id.rs` и `crates/pokrov-api/src/middleware/mod.rs`
- [X] T030 [P] [US3] Реализовать JSON structured logging subscriber и lifecycle event logging в `crates/pokrov-runtime/src/observability.rs` и `crates/pokrov-runtime/src/bootstrap.rs`
- [X] T031 [US3] Прокинуть `request_id` в headers, response body и probe handlers в `crates/pokrov-api/src/app.rs`, `crates/pokrov-api/src/handlers/health.rs` и `crates/pokrov-api/src/handlers/ready.rs`
- [X] T032 [US3] Интегрировать request correlation с app state и metrics hooks в `crates/pokrov-api/src/app.rs` и `crates/pokrov-metrics/src/hooks.rs`

**Checkpoint**: История 3 независимо подтверждает observability и logging safety

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Довести runtime до release-ready состояния по verification и поставке

- [X] T033 [P] Добавить performance smoke tests для `/health` и `/ready` в `tests/performance/bootstrap_probes.rs`
- [X] T034 [P] Подготовить container runtime assets в `Dockerfile` и `.dockerignore`
- [X] T035 Обновить пользовательскую и операторскую документацию по запуску и shutdown в `README.md`, `config/README.md` и `specs/001-bootstrap-runtime/quickstart.md`
- [X] T036 Зафиксировать acceptance evidence и финальную verification checklist в `specs/001-bootstrap-runtime/checklists/requirements.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1: Setup** стартует сразу
- **Phase 2: Foundational** зависит от Setup и блокирует все истории
- **Phase 3: US1** зависит от Foundational и формирует MVP
- **Phase 4: US2** зависит от US1, потому что расширяет probe/lifecycle behavior уже работающего startup flow
- **Phase 5: US3** зависит от US1 и может идти после US2, так как затрагивает те же probe handlers и app wiring
- **Phase 6: Polish** зависит от завершения нужных историй

### User Story Dependencies

- **US1 (P1)**: первая поставляемая история; задает валидный startup contract
- **US2 (P2)**: расширяет lifecycle semantics поверх US1
- **US3 (P3)**: добавляет request correlation и logging safety поверх работающих probes

### Within Each User Story

- Тестовые задачи идут раньше реализации
- Loader/model/contract changes идут раньше wiring и handlers
- История считается завершенной только после прохождения ее independent test

### Parallel Opportunities

- `T002`, `T003`, `T004` можно выполнять параллельно после `T001`
- `T006`, `T007`, `T008`, `T010` можно выполнять параллельно после `T005`
- В US1 параллелятся `T011`, `T012`, `T013`, а также `T014` и `T016`
- В US2 параллелятся `T019`, `T020`, `T021`, а также `T022` и `T025`
- В US3 параллелятся `T026`, `T027`, `T028`, а также `T029` и `T030`
- В Polish параллелятся `T033` и `T034`

---

## Parallel Example: User Story 1

```bash
Task: "Добавить contract test для YAML config schema в tests/contract/runtime_config_contract.rs"
Task: "Добавить integration test для валидного старта и отказа на невалидном конфиге в tests/integration/startup_config_flow.rs"
Task: "Добавить unit tests для parsing и semantic validation конфига в crates/pokrov-config/src/validate.rs"

Task: "Реализовать YAML loader и file-based config ingestion в crates/pokrov-config/src/loader.rs"
Task: "Реализовать probe handlers для happy-path startup в crates/pokrov-api/src/handlers/health.rs и crates/pokrov-api/src/handlers/ready.rs"
```

## Parallel Example: User Story 2

```bash
Task: "Добавить contract test для состояний /health и /ready в tests/contract/runtime_api_contract.rs"
Task: "Добавить integration test для startup-pending и graceful shutdown в tests/integration/readiness_shutdown_flow.rs"
Task: "Добавить unit tests для lifecycle transitions в crates/pokrov-runtime/src/lifecycle.rs"

Task: "Реализовать lifecycle-driven readiness checks в crates/pokrov-runtime/src/lifecycle.rs"
Task: "Подключить lifecycle events к metrics hooks в crates/pokrov-metrics/src/hooks.rs и crates/pokrov-metrics/src/registry.rs"
```

## Parallel Example: User Story 3

```bash
Task: "Добавить integration test для x-request-id propagation в tests/integration/request_id_logging_flow.rs"
Task: "Добавить security test для metadata-only logging в tests/security/logging_safety.rs"
Task: "Добавить unit tests для request_id middleware в crates/pokrov-api/src/middleware/request_id.rs"

Task: "Реализовать request_id middleware в crates/pokrov-api/src/middleware/request_id.rs и crates/pokrov-api/src/middleware/mod.rs"
Task: "Реализовать JSON structured logging subscriber в crates/pokrov-runtime/src/observability.rs и crates/pokrov-runtime/src/bootstrap.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Завершить Phase 1: Setup
2. Завершить Phase 2: Foundational
3. Завершить Phase 3: US1
4. Проверить startup behavior с валидным и невалидным конфигом
5. Только после этого переходить к lifecycle hardening и observability

### Incremental Delivery

1. Setup + Foundational создают общий runtime foundation
2. US1 добавляет рабочий startup path и ready transition
3. US2 добавляет startup/draining lifecycle semantics
4. US3 добавляет request correlation и safe logging
5. Polish закрывает performance, container delivery и acceptance evidence

### Parallel Team Strategy

1. Один исполнитель завершает workspace/bootstrap foundation
2. После T018 можно развести US2 и часть US3 по разным владельцам с учетом конфликтов в `crates/pokrov-api/src/app.rs`
3. Polish выполняется после интеграции всех историй

---

## Notes

- `[P]` означает отсутствие прямой зависимости по незавершенным задачам и раздельные файлы
- Все задачи используют строгий checklist format `- [ ] T### [P] [US#] Description with file path`
- Suggested MVP scope: **Phase 1 + Phase 2 + Phase 3 (US1)**
