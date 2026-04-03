<!--
Sync Impact Report
Version change: template -> 1.0.0
Modified principles:
- placeholder principle 1 -> I. Санитизация до внешнего доступа
- placeholder principle 2 -> II. Детерминированное применение политики
- placeholder principle 3 -> III. Одобренные интерфейсы и ограниченный scope
- placeholder principle 4 -> IV. Наблюдаемость и объяснимые операции
- placeholder principle 5 -> V. Верификация без исключений
Added sections:
- Продуктовые и архитектурные ограничения
- Процесс разработки и контроль качества
Removed sections:
- none
Templates requiring updates:
- ✅ .specify/templates/plan-template.md
- ✅ .specify/templates/spec-template.md
- ✅ .specify/templates/tasks-template.md
- N/A .specify/templates/commands/ (directory not present)
- ✅ docs/PRD.md
Follow-up TODOs:
- none
-->
# Конституция Pokrov.AI

## Ключевые принципы

### I. Санитизация до внешнего доступа
Любой LLM- или MCP-payload MUST проходить detection и transformation до отправки
upstream. Raw sensitive payload, сырые detections и несанитизированные фрагменты
MUST NOT попадать в логи, аудит или explain-ответы по умолчанию. Для нарушений
policy profile, allowlist или argument validation MUST существовать явный block
path со структурированной ошибкой. Обоснование: Pokrov.AI ценен только тогда,
когда риск устраняется до внешнего исполнения, а не фиксируется постфактум.

### II. Детерминированное применение политики
Одинаковые входные данные при одинаковой конфигурации MUST приводить к одинаковым
detections, transform actions и audit summary. Любая трансформация MUST сохранять
структурную валидность исходного протокола, а каждое policy decision MUST иметь
краткое explain summary без раскрытия чувствительных данных. Обоснование:
без детерминизма невозможно отлаживать false positives, сравнивать dry-run с
enforcement и защищать продуктовые контракты.

### III. Одобренные интерфейсы и ограниченный scope
Версия v1 MUST ограничиваться self-hosted open-source сервисом на Rust с
OpenAI-compatible LLM endpoint, одобренным MCP mediation subset и container-first
поставкой. Всё, что требует внешнего control plane, RBAC/IAM, A2A proxy,
универсального MCP registry или иных явно исключенных из PRD возможностей, MUST
оставаться вне scope до отдельной поправки к PRD и конституции. Обоснование:
Pokrov.AI v1 решает одну узкую задачу и не должен размываться в платформу общего
назначения.

### IV. Наблюдаемость и объяснимые операции
Каждый запрос MUST иметь `request_id`, структурированные JSON-логи, метрики
Prometheus и проверяемые `/health` и `/ready` endpoints. Readiness MUST отражать
состояние конфигурации и routing layer, graceful shutdown MUST корректно завершать
активные запросы, а ошибки upstream MUST возвращаться в предсказуемой форме.
Обоснование: продукт, который изменяет трафик и блокирует инструменты, без
наблюдаемости превращается в непрозрачный источник отказов.

### V. Верификация без исключений
Изменение не считается завершенным, пока его обязательные unit, integration,
performance и security checks не определены и не выполнены для затронутого
поведения. Happy path, block path, audit generation, logging safety и обновление
документации MUST быть частью acceptance evidence для каждого endpoint или policy
flow. Обоснование: Pokrov.AI меняет безопасность и протоколы, поэтому неподтвержденная
корректность равна регрессии.

## Продуктовые и архитектурные ограничения

- Язык реализации MUST быть Rust, если иное не ратифицировано отдельной поправкой.
- Конфигурация MUST использовать YAML и валидироваться на старте; невалидный конфиг
  MUST NOT переводить сервис в состояние ready.
- Секреты MUST поступать через env vars или secret mounts; хранение открытых
  секретов в конфиге запрещено.
- Audit по умолчанию MUST быть metadata-only.
- Сервис MUST укладываться в NFR v1: p95 latency overhead <= 50 мс, p99 <= 100 мс,
  startup <= 5 сек и throughput не ниже 500 RPS на baseline payload.
- Поставка MUST выполняться как self-hosted контейнер без зависимости от внешней
  control plane инфраструктуры.

## Процесс разработки и контроль качества

- `docs/PRD.md` MUST оставаться источником истины для продуктового поведения v1, а
  эта конституция MUST задавать
  инженерные правила, quality gates и требования к доказательствам готовности.
- Каждый новый `spec.md` MUST явно фиксировать scope, out-of-scope, security/privacy
  constraints, operational readiness, acceptance evidence и required test coverage.
- Каждый новый `plan.md` MUST проходить конституционные гейты до начала реализации и
  после проектирования: sanitization-first, deterministic behavior, observability,
  bounded scope и verification evidence.
- Каждый новый `tasks.md` MUST включать задачи для обязательных проверок, логирования,
  метрик, аудита, block path и документации, если история затрагивает проксирование,
  политику, безопасность или операционные аспекты.
- Любое отклонение от этих правил MUST быть явно зафиксировано в документации с
  причиной, более простым отклоненным вариантом и планом компенсации риска.

## Governance

- Эта конституция имеет приоритет над локальными привычками и шаблонными практиками
  проекта в части инженерного процесса, quality gates и критериев приемки.
- Поправка MUST включать: измененные принципы или разделы, причину изменения, список
  синхронизируемых шаблонов и документов, а также влияние на acceptance criteria.
- Версионирование MUST следовать semantic versioning:
  MAJOR для несовместимого изменения принципов или governance,
  MINOR для новых принципов или существенного расширения обязательных правил,
  PATCH для уточнений и редакционных правок без изменения смысла.
- Любое изменение конституции MUST сопровождаться обновлением затронутых шаблонов в
  `.specify/templates/` и связанных guidance-документов в том же наборе изменений.
- Compliance review MUST выполняться при подготовке плана, при ревью изменений и
  перед слиянием: проверяются конституционные гейты, тестовые доказательства и
  отсутствие несогласованных конфликтов с PRD.

**Version**: 1.0.0 | **Ratified**: 2026-03-30 | **Last Amended**: 2026-04-03
