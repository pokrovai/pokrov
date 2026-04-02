# Data Model: Bootstrap Runtime

## RuntimeConfig

- **Purpose**: Представляет валидированную конфигурацию запуска сервиса.
- **Fields**:
  - `server.host`: сетевой адрес bind
  - `server.port`: порт bind
  - `security.api_keys[]`: набор reference-only секретов с привязкой профиля
  - `logging.level`: минимальный уровень журналирования
- **Validation rules**:
  - `server.host` и `server.port` обязательны
  - `security.api_keys` не должны содержать открытые секреты
  - отсутствующие обязательные секции блокируют переход в ready

## RuntimeState

- **Purpose**: Отражает жизненный цикл процесса.
- **States**:
  - `starting`
  - `ready`
  - `draining`
  - `stopped`
- **Transitions**:
  - `starting -> ready` после успешной загрузки и валидации конфига и завершения инициализации
  - `starting -> stopped` при фатальной ошибке старта
  - `ready -> draining` при начале graceful shutdown
  - `draining -> stopped` после завершения активных запросов

## RequestContext

- **Purpose**: Контекст отдельного HTTP-запроса для корреляции логов и ответа.
- **Fields**:
  - `request_id`
  - `path`
  - `method`
  - `started_at`
- **Validation rules**:
  - `request_id` обязателен для каждого запроса
  - поля контекста не должны включать raw body

## HealthStatus

- **Purpose**: Машинно-читаемый ответ о liveness/readiness сервиса.
- **Fields**:
  - `status`
  - `request_id`
  - `checks` для readiness-ответа
- **Validation rules**:
  - `status` принимает только значения, соответствующие endpoint semantics
  - `checks` не раскрывают чувствительные значения конфигурации
