---

description: "Шаблон списка задач для реализации фичи"
---

# Задачи: [FEATURE NAME]

**Вход**: Артефакты проектирования из `/specs/[###-feature-name]/`
**Prerequisites**: `plan.md` (обязательно), `spec.md` (обязательно для историй),
`research.md`, `data-model.md`, `contracts/`

**Тесты**: Тестовые задачи НЕ являются опциональными, если история затрагивает
proxy/policy/security/ops или если это явно требуется в спецификации. Для таких
изменений обязательно включайте unit, integration, performance и security coverage
в объеме, определенном `spec.md` и `plan.md`.

**Организация**: Задачи группируются по пользовательским историям, чтобы каждая
история могла быть реализована и проверена независимо.

## Формат: `[ID] [P?] [Story] Description`

- **[P]**: можно выполнять параллельно, если файлы не пересекаются
- **[Story]**: идентификатор истории, например `US1`, `US2`, `US3`
- В описании MUST быть указаны точные пути к файлам

## Соглашения по путям

- **Single project**: `src/`, `tests/` в корне репозитория
- **Web app**: `backend/src/`, `frontend/src/`
- **Mobile**: `api/src/`, `ios/src/` или `android/src/`
- Примеры ниже исходят из single project; адаптируйте их к фактической структуре из `plan.md`

<!-- 
  ============================================================================
  IMPORTANT: Задачи ниже являются ПРИМЕРАМИ и должны быть заменены реальными.
  
  Команда /speckit.tasks MUST заменить их задачами на основе:
  - пользовательских историй из spec.md
  - требований и constitutional gates из plan.md
  - сущностей из data-model.md
  - контрактов из contracts/
  
  Задачи MUST быть организованы по историям так, чтобы каждая история могла:
  - реализовываться независимо
  - тестироваться независимо
  - поставляться как отдельный инкремент
  
  Не оставляйте примерные задачи в финальном tasks.md.
  ============================================================================
-->

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Инициализация проекта и базовой структуры

- [ ] T001 Создать структуру проекта по `plan.md`
- [ ] T002 Инициализировать [language] проект и зависимости [framework]
- [ ] T003 [P] Настроить linting, formatting и базовые CI/check команды
- [ ] T004 [P] Подготовить конфиг, secrets handling и образцы env/config файлов

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Базовая инфраструктура, без которой истории нельзя реализовывать

**CRITICAL**: Ни одна пользовательская история не начинается до завершения этой фазы

Примеры базовых задач:

- [ ] T005 Настроить базовые модели/сущности и общие типы
- [ ] T006 [P] Поднять маршрутизацию/API handlers и middleware
- [ ] T007 [P] Реализовать auth, allowlist или policy selection framework
- [ ] T008 [P] Настроить structured logging, audit hooks и `request_id`
- [ ] T009 [P] Настроить Prometheus metrics, health и readiness
- [ ] T010 Настроить единый error model и predictable error responses

**Checkpoint**: Foundation ready - теперь пользовательские истории можно вести параллельно

---

## Phase 3: User Story 1 - [Title] (Priority: P1) MVP

**Goal**: [Коротко опишите, что дает история]

**Independent Test**: [Как проверить историю отдельно]

### Tests for User Story 1

> Если история меняет поведение runtime/proxy/policy, тесты MUST появиться до реализации.

- [ ] T011 [P] [US1] Contract test для [endpoint/interface] в [path]
- [ ] T012 [P] [US1] Integration test для happy path в [path]
- [ ] T013 [P] [US1] Integration/security test для block path или logging safety в [path]
- [ ] T014 [P] [US1] Unit tests для policy/detection/validation logic в [path]

### Implementation for User Story 1

- [ ] T015 [P] [US1] Создать или обновить [Entity1/component] в [path]
- [ ] T016 [P] [US1] Создать или обновить [Entity2/component] в [path]
- [ ] T017 [US1] Реализовать [service/handler] в [path]
- [ ] T018 [US1] Добавить validation, sanitization и error handling в [path]
- [ ] T019 [US1] Добавить audit, logs, metrics и docs updates в [path]

**Checkpoint**: История 1 полностью работоспособна и проверяема независимо

---

## Phase 4: User Story 2 - [Title] (Priority: P2)

**Goal**: [Коротко опишите, что дает история]

**Independent Test**: [Как проверить историю отдельно]

### Tests for User Story 2

- [ ] T020 [P] [US2] Contract test для [endpoint/interface] в [path]
- [ ] T021 [P] [US2] Integration test для happy/block path в [path]
- [ ] T022 [P] [US2] Unit/security/perf checks, если история влияет на policy или latency, в [path]

### Implementation for User Story 2

- [ ] T023 [P] [US2] Создать или обновить [Entity/component] в [path]
- [ ] T024 [US2] Реализовать [service/handler] в [path]
- [ ] T025 [US2] Реализовать [endpoint/feature] в [path]
- [ ] T026 [US2] Интегрировать историю с общими компонентами и observability hooks

**Checkpoint**: Истории 1 и 2 работают независимо

---

## Phase 5: User Story 3 - [Title] (Priority: P3)

**Goal**: [Коротко опишите, что дает история]

**Independent Test**: [Как проверить историю отдельно]

### Tests for User Story 3

- [ ] T027 [P] [US3] Contract test для [endpoint/interface] в [path]
- [ ] T028 [P] [US3] Integration test для happy/block path в [path]
- [ ] T029 [P] [US3] Performance/security checks, если история влияет на latency, auth или logging safety

### Implementation for User Story 3

- [ ] T030 [P] [US3] Создать или обновить [Entity/component] в [path]
- [ ] T031 [US3] Реализовать [service/handler] в [path]
- [ ] T032 [US3] Реализовать [endpoint/feature] в [path]

**Checkpoint**: Все пользовательские истории функциональны независимо

---

[Добавляйте дополнительные фазы по тому же шаблону]

---

## Phase N: Polish & Cross-Cutting Concerns

**Purpose**: Улучшения, затрагивающие несколько историй

- [ ] TXXX [P] Обновить документацию в `docs/`, `quickstart.md`, примеры конфигов
- [ ] TXXX Упростить код и убрать временные обходы
- [ ] TXXX Проверить производительность на всех затронутых путях
- [ ] TXXX [P] Добавить недостающие unit/integration/security/perf tests
- [ ] TXXX Проверить logging safety, audit completeness и readiness semantics
- [ ] TXXX Запустить и зафиксировать финальную verification-команду

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: стартует сразу
- **Foundational (Phase 2)**: зависит от Setup и блокирует все истории
- **User Stories (Phase 3+)**: стартуют после Foundational
- **Polish (Final Phase)**: начинается после завершения нужных историй

### User Story Dependencies

- **User Story 1 (P1)**: стартует первой после Foundational
- **User Story 2 (P2)**: может интегрироваться с US1, но обязана оставаться independently testable
- **User Story 3 (P3)**: может зависеть от общих компонентов, но не должна ломать независимую приемку

### Within Each User Story

- Обязательные тесты MUST быть описаны до реализации
- Контракты и модели идут раньше сервисов и handlers
- Core behavior идет раньше интеграции и polish
- История считается завершенной только после acceptance evidence и verification

### Parallel Opportunities

- Все задачи Setup и Foundational с меткой `[P]` можно выполнять параллельно
- После Foundational разные истории могут идти параллельно при отсутствии конфликтов по файлам
- Тесты, модели и изолированные компоненты внутри одной истории можно распараллеливать

---

## Parallel Example: User Story 1

```bash
# Запустить тесты истории 1 параллельно:
Task: "Contract test для [endpoint/interface] в [path]"
Task: "Integration test для happy/block path в [path]"
Task: "Unit test для policy or validation logic в [path]"

# Параллельно сделать независимые компоненты:
Task: "Создать [Entity1/component] в [path]"
Task: "Создать [Entity2/component] в [path]"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Завершить Phase 1: Setup
2. Завершить Phase 2: Foundational
3. Завершить Phase 3: User Story 1
4. Остановиться и проверить историю 1 независимо
5. Переходить к следующим историям только после verification evidence

### Incremental Delivery

1. Setup + Foundational -> foundation ready
2. Добавить User Story 1 -> independent verification
3. Добавить User Story 2 -> independent verification
4. Добавить User Story 3 -> independent verification
5. Каждая история должна добавлять ценность без регрессии предыдущих

### Parallel Team Strategy

При нескольких разработчиках:

1. Команда завершает Setup + Foundational
2. После этого истории распределяются по владельцам
3. Интеграция разрешена только после прохождения обязательных test gates

---

## Notes

- `[P]` = разные файлы и отсутствие прямых зависимостей
- Метка `[Story]` нужна для трассировки до истории
- Каждая история должна быть independently completable and testable
- Избегайте расплывчатых задач, конфликтов по одним и тем же файлам и скрытых cross-story зависимостей
