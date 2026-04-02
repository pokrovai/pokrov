# Research: Bootstrap Runtime

## Decision 1: Строить foundation как Rust workspace из четырех bootstrap crates

- **Decision**: Использовать workspace с crates `pokrov-api`,
  `pokrov-config`, `pokrov-runtime` и `pokrov-metrics`.
- **Rationale**: Это совпадает с PRD stage 1, изолирует HTTP wiring,
  конфигурацию, lifecycle и observability hooks, а также не смешивает bootstrap
  concerns с будущими proxy flows.
- **Alternatives considered**:
  - Один crate для всего сервиса: быстрее на старте, но скрывает границы
    ответственности и усложняет последующее добавление `pokrov-core` и proxy
    crates.
  - Сразу заводить все v1 crates из PRD: преждевременно раздувает bootstrap
    feature и нарушает bounded scope.

## Decision 2: YAML-конфиг валидировать в два этапа: schema parse, затем runtime validation

- **Decision**: Сначала десериализовать YAML через `serde`/`serde_yaml` в
  typed config model, затем выполнять отдельный semantic validation pass для
  обязательных полей, диапазонов портов, shutdown timeouts и secret reference
  правил.
- **Rationale**: Разделение структурной и semantic validation дает ясные ошибки
  старта, упрощает unit tests и не позволяет перевести сервис в ready при
  частично валидной конфигурации.
- **Alternatives considered**:
  - Валидировать только на уровне `serde` типов: недостаточно для бизнес-правил
    вроде запрета raw secret values.
  - Принимать конфиг как untyped map и проверять вручную: хуже для читаемости и
    будущей расширяемости.

## Decision 3: Разрешить только reference-style секреты `env:` и `file:`

- **Decision**: Любое секретное значение в bootstrap-конфиге описывается только
  ссылкой формата `env:VAR_NAME` или `file:/path/to/secret`.
- **Rationale**: Это напрямую соответствует PRD и конституции, покрывает
  локальный запуск и Kubernetes/Docker secret mounts, а также позволяет
  проверять безопасный формат без логирования содержимого секрета.
- **Alternatives considered**:
  - Разрешить plain-text секреты для dev режима: противоречит security rules и
    создает риск утечки.
  - Поддержать только `env:`: упрощает реализацию, но хуже ложится на
    container-first secret mounts.

## Decision 4: Lifecycle координировать через явное состояние и счетчик активных запросов

- **Decision**: Runtime хранит состояние `starting -> ready -> draining -> stopped`
  в наблюдаемом lifecycle объекте, а middleware ведет счетчик активных запросов
  для bounded graceful shutdown.
- **Rationale**: Такой подход делает readiness детерминированной, позволяет
  немедленно перевести сервис в not-ready на shutdown signal и корректно ждать
  завершения in-flight запросов без внешнего coordination store.
- **Alternatives considered**:
  - Считать процесс ready сразу после bind сокета: дает ложноположительную
    готовность до завершения инициализации.
  - Завершать процесс сразу по сигналу без drain: нарушает NFR по надежности и
    acceptance scenario для graceful shutdown.

## Decision 5: `request_id` вести через middleware, возвращать в JSON body и `x-request-id`

- **Decision**: Для каждого HTTP-запроса middleware принимает входящий
  `x-request-id`, если он непустой и корректный, иначе генерирует новый UUID;
  идентификатор прокидывается в tracing span, structured logs, response body и
  response header.
- **Rationale**: Это сохраняет корреляцию с внешней инфраструктурой, при этом не
  делает сервис зависимым от внешнего генератора идентификаторов.
- **Alternatives considered**:
  - Всегда генерировать новый `request_id`: проще, но ломает внешнюю
    корреляцию.
  - Возвращать `request_id` только в body: достаточно для JSON API, но хуже для
    ingress/probe tooling.

## Decision 6: `/ready` отражает только bootstrap-зависимости текущей фичи, но остается расширяемым

- **Decision**: В bootstrap feature readiness зависит от успешной загрузки
  конфига, завершения runtime initialization и отсутствия draining state; ответ
  строится как список checks, который потом можно расширять policy/upstream
  зависимостями без изменения базового контракта.
- **Rationale**: Это позволяет уже сейчас реализовать корректный startup/shutdown
  behavior и не фиксирует контракт в слишком узкой форме.
- **Alternatives considered**:
  - Возвращать только boolean-ready: слишком мало данных для диагностики.
  - Сразу завязать readiness на policy/upstream routing: преждевременно для
    bootstrap scope.

## Decision 7: Metrics в этой фазе оформить как стабильные hooks и registry boundary

- **Decision**: Выделить `pokrov-metrics` с именованными runtime counters/hooks,
  чтобы startup, readiness changes, shutdown signals и request lifecycle сразу
  инструментировались через единый интерфейс, даже если полный Prometheus export
  будет расширяться следующими фичами.
- **Rationale**: Bootstrap runtime получает observability boundary уже на первом
  этапе и не потребует ломать публичный lifecycle contract при подключении
  дополнительных метрик.
- **Alternatives considered**:
  - Игнорировать metrics до следующих фич: нарушает конституционное требование
    про observability planning.
  - Реализовать полный metrics surface прямо сейчас: расширяет scope beyond
    bootstrap foundation.
