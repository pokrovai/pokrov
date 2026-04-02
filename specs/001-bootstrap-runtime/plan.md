# План реализации: Bootstrap Runtime

**Ветка**: `001-bootstrap-runtime` | **Дата**: 2026-04-03 | **Спецификация**: [spec.md](specs/001-bootstrap-runtime/spec.md)
**Вход**: Спецификация фичи из `/specs/001-bootstrap-runtime/spec.md`

## Сводка

Создать минимальный runtime-каркас Pokrov.AI как self-hosted Rust web-service: валидируемый YAML-конфиг, жизненный цикл старта и остановки, `request_id`, безопасные structured logs, а также служебные endpoint'ы `/health` и `/ready`. Фича не добавляет proxy-логику, но фиксирует контракты и точки расширения для последующих этапов PRD.

## Технический контекст

**Language/Version**: Rust stable (1.85+)  
**Primary Dependencies**: axum, tokio, serde, serde_yaml, tower, tracing  
**Storage**: N/A (только in-memory runtime state)  
**Testing**: `cargo test`, HTTP integration tests, log-safety checks  
**Target Platform**: Linux container runtime  
**Project Type**: web-service  
**Performance Goals**: startup <= 5s; служебные endpoint'ы отвечают без заметной деградации относительно базового HTTP runtime  
**Constraints**: self-hosted only, metadata-only logging, no external control plane, readiness зависит от валидного конфига  
**Scale/Scope**: foundation feature for v1; готовит каркас для LLM и MCP proxy paths

## Проверка конституции

*GATE: Должна быть пройдена до Phase 0 research и повторно после Phase 1 design.*

- Санитизация и policy enforcement описаны до upstream-вызова; raw sensitive payload не используется как часть логирования, аудита или explain outputs. PASS: bootstrap-слой не обрабатывает payload и явно запрещает raw logging.
- Поведение детерминировано: одинаковый payload при одинаковом config дает те же detections, actions и summaries; трансформации сохраняют валидность протокола. PASS: в scope только детерминированная загрузка конфига и lifecycle state machine.
- Scope ограничен одобренными интерфейсами и рамками v1; любые расширения вне PRD перечислены как out-of-scope или вынесены в отдельное обоснование. PASS: фича ограничена runtime, config, health/readiness и observability foundation.
- Наблюдаемость спроектирована: `request_id`, structured logs, metrics, `/health`, `/ready`, predictable upstream errors и graceful shutdown покрыты планом. PASS: `request_id`, logs, `/health`, `/ready` и lifecycle входят в deliverable; metrics hooks зарезервированы.
- Для затронутого поведения определены acceptance evidence и обязательные test gates: unit, integration, performance и security, если изменение влияет на proxy/policy/ops. PASS: все четыре класса проверок определены.
- Любое нарушение конституции вынесено в раздел Complexity Tracking с объяснением, почему более простой вариант был отклонен. PASS: отклонений нет.

## Структура проекта

### Документация фичи

```text
specs/001-bootstrap-runtime/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── runtime-api.yaml
└── checklists/
    └── requirements.md
```

### Исходный код (корень репозитория)

```text
Cargo.toml
crates/
├── pokrov-api/
├── pokrov-config/
├── pokrov-metrics/
└── pokrov-runtime/

config/
└── pokrov.example.yaml

tests/
├── integration/
├── performance/
└── security/
```

**Решение по структуре**: Использовать Rust workspace с отдельными crates для API, конфигурации, runtime и метрик. На этом этапе создается только foundation-структура и публичные служебные контракты, без proxy-specific crates.

## Phase 0 Research

- Зафиксировать выбор базового HTTP/runtime стека для self-hosted сервиса.
- Зафиксировать способ загрузки и валидации YAML-конфига без хранения секретов в открытом виде.
- Зафиксировать модель readiness/liveness и graceful shutdown, совместимую с контейнерным окружением.
- Зафиксировать подход к structured logging и корреляции `request_id`.

## Phase 1 Design & Contracts

- Описать data model для `RuntimeConfig`, `RuntimeState`, `RequestContext` и health/readiness статусов.
- Зафиксировать контракт `runtime-api.yaml` для `/health` и `/ready`.
- Подготовить quickstart для локального и контейнерного запуска.
- Обновить agent context после фиксации технического контекста.

## Post-Design Constitution Check

- Повторная проверка должна подтвердить, что публичные контракты не допускают raw payload leakage и что readiness не объявляет сервис готовым до валидной инициализации.
- Повторная проверка должна подтвердить, что quickstart и acceptance evidence покрывают observability и lifecycle.

## Complexity Tracking

Нарушений конституции или вынужденных усложнений на текущем этапе не выявлено.
