# Задачи: Sanitization Core

**Вход**: Артефакты проектирования из `specs/002-sanitization-core/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md`

**Тесты**: Для этой security/policy-фичи обязательны unit, integration,
performance и security tasks согласно `spec.md`.

**Организация**: Задачи сгруппированы по пользовательским историям так, чтобы
каждую историю можно было реализовать и проверить независимо после завершения
Setup и Foundational phases.

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Подготовить workspace и каркас sanitization core для последующей реализации

- [X] T001 Добавить crate `pokrov-core` в workspace manifests в `Cargo.toml` и `crates/pokrov-core/Cargo.toml`
- [X] T002 [P] Создать модульный каркас core crate в `crates/pokrov-core/src/lib.rs`, `crates/pokrov-core/src/types.rs`, `crates/pokrov-core/src/detection/mod.rs`, `crates/pokrov-core/src/policy/mod.rs`, `crates/pokrov-core/src/transform/mod.rs`, `crates/pokrov-core/src/traversal/mod.rs`, `crates/pokrov-core/src/audit/mod.rs` и `crates/pokrov-core/src/dry_run/mod.rs`
- [X] T003 [P] Подключить зависимости `pokrov-core`/`regex`/`serde_json` в `crates/pokrov-api/Cargo.toml`, `crates/pokrov-runtime/Cargo.toml` и `crates/pokrov-config/Cargo.toml`
- [X] T004 [P] Подготовить test-suite entrypoints для новых test files в `tests/contract.rs`, `tests/integration.rs`, `tests/security.rs` и `tests/performance.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Общие компоненты evaluate pipeline, без которых истории не стартуют

**CRITICAL**: Ни одна пользовательская история не начинается до завершения этой фазы

- [X] T005 Добавить domain-модели sanitization policy profiles и custom rules в `crates/pokrov-config/src/model.rs`
- [X] T006 [P] Реализовать загрузку секции `sanitization` из YAML config в `crates/pokrov-config/src/loader.rs`
- [X] T007 [P] Реализовать валидацию profile/actions/custom-rule constraints в `crates/pokrov-config/src/validate.rs` и `crates/pokrov-config/src/error.rs`
- [X] T008 [P] Добавить общие типы evaluate request/response/error для API и core в `crates/pokrov-core/src/types.rs` и `crates/pokrov-api/src/error.rs`
- [X] T009 Реализовать shared evaluator state/wiring в `crates/pokrov-api/src/app.rs` и `crates/pokrov-runtime/src/bootstrap.rs`
- [X] T010 [P] Добавить metrics hooks для `rule_hits`, `transformed_payloads`, `blocked_evaluations` в `crates/pokrov-metrics/src/hooks.rs` и `crates/pokrov-metrics/src/registry.rs`
- [X] T011 Обновить readiness-check с учетом загрузки policy configuration в `crates/pokrov-runtime/src/lifecycle.rs` и `crates/pokrov-api/src/handlers/ready.rs`

**Checkpoint**: Config + evaluator wiring + readiness/metrics готовы; можно реализовывать истории

---

## Phase 3: User Story 1 - Оценка payload по политике (Priority: P1) MVP

**Goal**: Детерминированно вычислять detections и финальное policy action в `enforce`/`dry_run`

**Independent Test**: Один и тот же payload + config повторно возвращает идентичные detections/action/explain

### Tests for User Story 1

- [X] T012 [P] [US1] Добавить contract test для `POST /v1/sanitize/evaluate` по `sanitization-evaluate-api.yaml` в `tests/contract/sanitization_evaluate_contract.rs`
- [X] T013 [P] [US1] Добавить integration test для strict-profile evaluate happy path и deterministic replay в `tests/integration/sanitization_evaluate_flow.rs`
- [X] T014 [P] [US1] Добавить unit tests для rule matching и deterministic overlap ordering в `crates/pokrov-core/src/detection/mod.rs`
- [X] T015 [P] [US1] Добавить unit tests для policy profile selection и action precedence в `crates/pokrov-core/src/policy/mod.rs`

### Implementation for User Story 1

- [X] T016 [P] [US1] Реализовать detection engine для built-in (`secrets`, `pii`, `corporate_markers`) и custom rules в `crates/pokrov-core/src/detection/mod.rs`
- [X] T017 [P] [US1] Реализовать deterministic overlap resolver и final action selection в `crates/pokrov-core/src/policy/mod.rs`
- [X] T018 [US1] Реализовать evaluate pipeline orchestration (`detect -> resolve -> decide`) в `crates/pokrov-core/src/lib.rs` и `crates/pokrov-core/src/types.rs`
- [X] T019 [US1] Реализовать HTTP evaluate handler для `enforce|dry_run` в `crates/pokrov-api/src/handlers/evaluate.rs` и `crates/pokrov-api/src/handlers/mod.rs`
- [X] T020 [US1] Подключить route `/v1/sanitize/evaluate` и evaluator wiring в `crates/pokrov-api/src/app.rs` и `crates/pokrov-runtime/src/bootstrap.rs`

**Checkpoint**: История 1 независимо подтверждает deterministic policy evaluation

---

## Phase 4: User Story 2 - Применение трансформаций без поломки формата (Priority: P2)

**Goal**: Применять `mask`/`replace`/`redact`/`block` с сохранением JSON-валидности для неблокирующих исходов

**Independent Test**: Transform-путь корректно изменяет только string leaves; `block` не допускает partial passthrough

### Tests for User Story 2

- [X] T021 [P] [US2] Добавить integration test для `mask`/`replace`/`redact` на nested JSON payload в `tests/integration/sanitization_transform_flow.rs`
- [X] T022 [P] [US2] Добавить integration test для block outcome без `sanitized_payload` в `tests/integration/sanitization_transform_flow.rs`
- [X] T023 [P] [US2] Добавить unit tests для recursive JSON traversal (mutate only string leaves) в `crates/pokrov-core/src/traversal/mod.rs`
- [X] T024 [P] [US2] Добавить unit tests для transform action applier и block short-circuit в `crates/pokrov-core/src/transform/mod.rs`

### Implementation for User Story 2

- [X] T025 [P] [US2] Реализовать JSON-safe traversal по `serde_json::Value` в `crates/pokrov-core/src/traversal/mod.rs`
- [X] T026 [P] [US2] Реализовать mask/replace/redact transformers и span applier в `crates/pokrov-core/src/transform/mod.rs`
- [X] T027 [US2] Интегрировать transform stage и terminal `block` behavior в `crates/pokrov-core/src/lib.rs` и `crates/pokrov-core/src/policy/mod.rs`
- [X] T028 [US2] Обновить evaluate response contract (наличие `sanitized_payload` только для non-block) в `crates/pokrov-api/src/handlers/evaluate.rs` и `specs/002-sanitization-core/contracts/sanitization-evaluate-api.yaml`

**Checkpoint**: История 2 независимо подтверждает корректные трансформации и block semantics

---

## Phase 5: User Story 3 - Metadata-only аудит и explainability (Priority: P3)

**Goal**: Возвращать explain/audit metadata без raw sensitive fragments

**Independent Test**: Explain/audit outputs содержат только category/count/action metadata и не включают raw payload fragments

### Tests for User Story 3

- [X] T029 [P] [US3] Добавить integration test для explain summary и dry-run parity в `tests/integration/sanitization_audit_explain_flow.rs`
- [X] T030 [P] [US3] Добавить security test на отсутствие raw payload/fragments в API/log outputs в `tests/security/sanitization_metadata_leakage.rs`
- [X] T031 [P] [US3] Добавить unit tests для metadata-only audit aggregation в `crates/pokrov-core/src/audit/mod.rs`

### Implementation for User Story 3

- [X] T032 [P] [US3] Реализовать explain summary builders и category-hit aggregation в `crates/pokrov-core/src/audit/mod.rs` и `crates/pokrov-core/src/types.rs`
- [X] T033 [US3] Реализовать metadata-only audit event generation в evaluate pipeline в `crates/pokrov-core/src/lib.rs` и `crates/pokrov-runtime/src/observability.rs`
- [X] T034 [US3] Добавить evaluate observability поля (`request_id`, `profile_id`, `final_action`, `rule_hits_total`) в `crates/pokrov-api/src/handlers/evaluate.rs` и `crates/pokrov-metrics/src/registry.rs`
- [X] T035 [US3] Обновить policy-profile documentation и custom-rule ограничения в `config/pokrov.example.yaml` и `config/README.md`

**Checkpoint**: История 3 независимо подтверждает explainability и metadata-only audit safety

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Закрыть performance/security verification и acceptance evidence

- [X] T036 [P] Добавить performance test для evaluate overhead (p95/p99 + replay determinism) в `tests/performance/sanitization_evaluate_latency.rs` и `tests/performance.rs`
- [X] T037 [P] Подключить новые contract/integration/security test modules в `tests/contract.rs`, `tests/integration.rs` и `tests/security.rs`
- [X] T038 Обновить сценарии в `specs/002-sanitization-core/quickstart.md` для финального verify path
- [X] T039 Зафиксировать acceptance evidence и verification результаты в `specs/002-sanitization-core/checklists/requirements.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1: Setup** стартует сразу
- **Phase 2: Foundational** зависит от Setup и блокирует все истории
- **Phase 3: US1** зависит от Foundational и формирует MVP evaluate flow
- **Phase 4: US2** зависит от US1, так как расширяет уже реализованный evaluate pipeline
- **Phase 5: US3** зависит от US1 и может выполняться параллельно с US2 при аккуратной синхронизации изменений в `crates/pokrov-api/src/handlers/evaluate.rs`
- **Phase 6: Polish** зависит от завершения нужных историй

### User Story Dependencies

- **US1 (P1)**: первая поставляемая история, задает deterministic evaluation core
- **US2 (P2)**: строится поверх US1 для transform semantics
- **US3 (P3)**: строится поверх US1 для explain/audit safety и observability

### Within Each User Story

- Тестовые задачи идут раньше реализации
- Контракты и unit checks идут раньше API wiring
- История считается завершенной только после прохождения ее independent test

### Parallel Opportunities

- В Setup параллелятся `T002`, `T003`, `T004` после `T001`
- В Foundational параллелятся `T006`, `T007`, `T008`, `T010` после `T005`
- В US1 параллелятся `T012`, `T013`, `T014`, `T015`, а также `T016` и `T017`
- В US2 параллелятся `T021`, `T022`, `T023`, `T024`, а также `T025` и `T026`
- В US3 параллелятся `T029`, `T030`, `T031`, а также `T032` и `T035`
- В Polish параллелятся `T036` и `T037`

---

## Parallel Example: User Story 1

```bash
Task: "Добавить contract test для POST /v1/sanitize/evaluate в tests/contract/sanitization_evaluate_contract.rs"
Task: "Добавить integration test deterministic replay в tests/integration/sanitization_evaluate_flow.rs"
Task: "Добавить unit tests overlap ordering в crates/pokrov-core/src/detection/mod.rs"

Task: "Реализовать detection engine в crates/pokrov-core/src/detection/mod.rs"
Task: "Реализовать overlap resolver в crates/pokrov-core/src/policy/mod.rs"
```

## Parallel Example: User Story 2

```bash
Task: "Добавить integration test transform path в tests/integration/sanitization_transform_flow.rs"
Task: "Добавить unit tests traversal в crates/pokrov-core/src/traversal/mod.rs"
Task: "Добавить unit tests transform applier в crates/pokrov-core/src/transform/mod.rs"

Task: "Реализовать JSON-safe traversal в crates/pokrov-core/src/traversal/mod.rs"
Task: "Реализовать mask/replace/redact transformer в crates/pokrov-core/src/transform/mod.rs"
```

## Parallel Example: User Story 3

```bash
Task: "Добавить integration test explain/dry-run parity в tests/integration/sanitization_audit_explain_flow.rs"
Task: "Добавить security test metadata leakage в tests/security/sanitization_metadata_leakage.rs"
Task: "Добавить unit tests audit aggregation в crates/pokrov-core/src/audit/mod.rs"

Task: "Реализовать explain summary builders в crates/pokrov-core/src/audit/mod.rs"
Task: "Обновить policy-profile docs в config/pokrov.example.yaml и config/README.md"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Завершить Phase 1: Setup
2. Завершить Phase 2: Foundational
3. Завершить Phase 3: US1
4. Проверить deterministic evaluate flow как самостоятельный инкремент
5. Переходить к трансформациям и audit safety только после evidence по US1

### Incremental Delivery

1. Setup + Foundational создают общий sanitization foundation
2. US1 добавляет deterministic evaluate decision core
3. US2 добавляет JSON-safe transform и block semantics
4. US3 добавляет explainability и metadata-only audit guarantees
5. Polish закрывает performance/security verification и acceptance evidence

### Parallel Team Strategy

1. Один исполнитель закрывает Setup + Foundational
2. После US1 команда может разделиться: один исполнитель ведет US2, второй US3
3. Интеграция выполняется после прохождения обязательных test gates по каждой истории

---

## Notes

- `[P]` означает отсутствие прямой зависимости по незавершенным задачам и раздельные файлы
- Все задачи используют строгий checklist format `- [ ] T### [P] [US#] Description with file path`
- Suggested MVP scope: **Phase 1 + Phase 2 + Phase 3 (US1)**
