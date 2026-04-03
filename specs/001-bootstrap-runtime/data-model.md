# Data Model: Bootstrap Runtime

## RuntimeConfig

- **Purpose**: Корневая валидированная модель bootstrap-конфига, из которой
  строится runtime.
- **Fields**:
  - `server: ServerConfig`
  - `logging: LoggingConfig`
  - `shutdown: ShutdownConfig`
  - `security: SecurityConfig`
  - `reserved: ReservedFeatureConfig`
- **Relationships**:
  - агрегирует все bootstrap-подсекции конфигурации;
  - после валидации передается в `RuntimeLifecycle` и HTTP app wiring.
- **Validation rules**:
  - `server` и `logging` обязательны;
  - отсутствие обязательной подсекции блокирует startup;
  - значения, похожие на секреты, допускаются только через `SecretRef`;
  - неподдерживаемые bootstrap-поля не должны ломать forward-compatible reserved
    blocks.

## ServerConfig

- **Purpose**: Сетевые параметры HTTP runtime.
- **Fields**:
  - `host: String`
  - `port: u16`
- **Validation rules**:
  - `host` обязателен и не может быть пустым;
  - `port` должен находиться в диапазоне `1..=65535`.

## LoggingConfig

- **Purpose**: Настройки structured logging bootstrap-слоя.
- **Fields**:
  - `level: String`
  - `format: String` со значением `json`
  - `component: String` со значением по умолчанию `runtime`
- **Validation rules**:
  - bootstrap runtime принимает только JSON-формат;
  - уровень логирования должен быть одним из `trace|debug|info|warn|error`;
  - логирование не должно включать raw request/response payload.

## ShutdownConfig

- **Purpose**: Ограничения для graceful shutdown.
- **Fields**:
  - `drain_timeout_ms: u64`
  - `grace_period_ms: u64`
- **Validation rules**:
  - оба значения положительные;
  - `grace_period_ms >= drain_timeout_ms`;
  - переход в `draining` происходит до ожидания активных запросов.

## SecurityConfig

- **Purpose**: Минимальная security-конфигурация bootstrap runtime, связанная с
  secret reference rules и будущей привязкой policy profile.
- **Fields**:
  - `api_keys: Vec<ApiKeyBinding>`
- **Validation rules**:
  - элементы списка обязаны использовать `SecretRef`;
  - `profile` обязателен и не пустой;
  - raw secret values считаются невалидными.

## ApiKeyBinding

- **Purpose**: Привязка API key к policy profile для последующих фич.
- **Fields**:
  - `key: SecretRef`
  - `profile: String`
- **Validation rules**:
  - `profile` соответствует slug-like формату;
  - повторяющиеся bindings должны быть явно отклонены на этапе валидации.

## SecretRef

- **Purpose**: Безопасная ссылка на секрет без хранения значения в YAML.
- **Variants**:
  - `env:<VAR_NAME>`
  - `file:<ABSOLUTE_OR_MOUNTED_PATH>`
- **Validation rules**:
  - plain-text значения запрещены;
  - `env:` требует непустое имя переменной;
  - `file:` требует непустой путь.

## ReservedFeatureConfig

- **Purpose**: Зарезервированное место под секции `policies`, `llm` и `mcp`,
  которые появятся в следующих фичах без перелома bootstrap layout.
- **Fields**:
  - `policies: Option<Map>`
  - `llm: Option<Map>`
  - `mcp: Option<Map>`
- **Validation rules**:
  - bootstrap feature не делает readiness зависимой от этих секций;
  - presence of reserved blocks не считается ошибкой, если bootstrap-часть
    валидна.

## RuntimeState

- **Purpose**: Явное состояние жизненного цикла процесса.
- **States**:
  - `starting`
  - `ready`
  - `draining`
  - `stopped`
- **Transitions**:
  - `starting -> ready` после успешной загрузки и валидации конфига и
    завершения инициализации HTTP runtime;
  - `starting -> stopped` при фатальной ошибке bootstrap;
  - `ready -> draining` при получении сигнала остановки;
  - `draining -> stopped` после drain active requests или по истечении timeout.

## RuntimeLifecycle

- **Purpose**: Runtime aggregate, который хранит текущее состояние, readiness
  checks и количество активных запросов.
- **Fields**:
  - `state: RuntimeState`
  - `config_loaded: bool`
  - `active_requests: usize`
  - `shutdown_started_at: Option<DateTime>`
- **Validation rules**:
  - `active_requests` не может быть отрицательным;
  - `state=ready` допустим только при `config_loaded=true`;
  - `state=draining` должен сразу делать `/ready` not-ready.

## RequestContext

- **Purpose**: Контекст отдельного HTTP-запроса для корреляции логов и ответа.
- **Fields**:
  - `request_id: String`
  - `method: String`
  - `path: String`
  - `started_at: DateTime`
- **Validation rules**:
  - `request_id` обязателен для каждого запроса;
  - контекст не хранит raw body и payload-derived sensitive fields.

## HealthResponse

- **Purpose**: Машинно-читаемый liveness response.
- **Fields**:
  - `status: "ok"`
  - `request_id: String`

## ReadyResponse

- **Purpose**: Машинно-читаемый readiness response.
- **Fields**:
  - `status: "ready" | "starting" | "draining"`
  - `request_id: String`
  - `checks: ReadyChecks`

## ReadyChecks

- **Purpose**: Детализация bootstrap readiness без раскрытия чувствительных
  данных.
- **Fields**:
  - `config: "ok" | "pending" | "failed"`
  - `runtime: "ok" | "pending" | "draining"`
  - `active_requests: u64`
- **Validation rules**:
  - значения checks должны отражать только metadata-only состояние;
  - секция не должна включать значения конфига или секретов.
