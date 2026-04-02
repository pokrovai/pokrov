# Research: Bootstrap Runtime

## Decision 1: Использовать Rust workspace с отдельными crates для runtime foundation

- **Decision**: Организовать проект как Rust workspace с отдельными crates `pokrov-api`, `pokrov-config`, `pokrov-metrics` и `pokrov-runtime`.
- **Rationale**: Это напрямую следует из PRD section 11, упрощает последующее добавление `pokrov-core`, `pokrov-proxy-llm` и `pokrov-proxy-mcp`, а также держит границы ответственности явными с первого этапа.
- **Alternatives considered**:
  - Один crate для всего сервиса: быстрее для старта, но хуже масштабируется под требования PRD и усложняет дальнейшее разделение ответственности.
  - Workspace с полным набором всех будущих crates сразу: создает избыточный объем до появления соответствующей функциональности.

## Decision 2: Использовать async HTTP runtime с явной lifecycle state machine

- **Decision**: Строить сервис на асинхронном HTTP runtime с отдельным состоянием `starting -> ready -> draining -> stopped`.
- **Rationale**: PRD требует health/readiness, graceful shutdown и предсказуемое поведение в контейнерной среде. Явная state machine делает условия готовности и остановки тестируемыми и детерминированными.
- **Alternatives considered**:
  - Полагаться только на факт живого процесса: не покрывает readiness и нарушает требования к операционной готовности.
  - Считать сервис ready сразу после bind сокета: создает ложноположительную готовность до завершения конфигурационной инициализации.

## Decision 3: Конфигурацию хранить в YAML, секреты передавать только как ссылки на env/secret mounts

- **Decision**: Принять YAML как внешний формат конфига, а для секретов разрешить только reference-style значения.
- **Rationale**: Это требование прямо закреплено в PRD section 12 и конституции. Такая модель позволяет валидировать конфиг при старте и предотвращать попадание открытых секретов в репозиторий и логи.
- **Alternatives considered**:
  - Разрешить открытые секреты в YAML для локальной разработки: противоречит продуктовым ограничениям и создает риск утечки.
  - Полностью отказаться от файлового конфига: противоречит целевому self-hosted сценарию v1.

## Decision 4: Structured logs должны быть metadata-only и обязательно содержать request correlation

- **Decision**: Все runtime-логи формировать как JSON с обязательными полями `timestamp`, `level`, `component`, `action`, `request_id` и служебными метаданными, без сырых тел запросов.
- **Rationale**: Это соответствует PRD section 14 и конституционному требованию observability without leakage.
- **Alternatives considered**:
  - Текстовые логи: хуже для интеграции с self-hosted observability tooling.
  - Логи с частичным выводом payload для отладки: нарушают требования безопасности.

## Decision 5: Health/readiness возвращают JSON и общий correlation field

- **Decision**: Даже служебные endpoint'ы возвращают JSON-ответы с `request_id` и машинно-читаемым статусом.
- **Rationale**: PRD section 9 задает общий принцип JSON-ответов и обязательного `request_id` во всех ответах. Это упрощает проверку и операционную автоматизацию.
- **Alternatives considered**:
  - Plain-text `OK`/`READY`: минималистично, но не соответствует общему ответному контракту и хуже для автоматизированной диагностики.
