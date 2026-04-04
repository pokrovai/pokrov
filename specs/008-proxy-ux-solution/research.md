# Research: Proxy UX P0-P2 Improvements

## Decision 1: Единый детерминированный порядок model resolution

- **Decision**: Ввести фиксированный порядок сопоставления `model` ключа: `exact canonical` -> `alias` -> `wildcard/prefix`.
- **Rationale**: Гарантирует предсказуемость поведения и исключает скрытую зависимость от порядка конфигурации.
- **Alternatives considered**:
  - `wildcard` раньше `alias`: повышает риск неожиданной маршрутизации и ухудшает UX для явно заданных aliases.
  - Использовать только exact+alias без wildcard: не покрывает P1 multi-provider UX требования.

## Decision 2: Fail-fast валидация конфликтов для full routing graph

- **Decision**: Startup/readiness блокируются при конфликте canonical/alias/wildcard правил или невалидной fallback-цепочке.
- **Rationale**: Явный not-ready безопаснее, чем недетерминированный runtime routing в production.
- **Alternatives considered**:
  - Разрешать конфликты через "first match": недетерминированно и плохо диагностируется.
  - Игнорировать конфликтные маршруты: приводит к частичной недоступности и скрытым ошибкам.

## Decision 3: Fallback routing только для retriable классов отказов

- **Decision**: Fallback активируется только для transport errors и заранее определенных `upstream_5xx`/`timeout` классов; policy/validation/auth errors не должны триггерить fallback.
- **Rationale**: Сохраняет безопасность и корректность contract semantics; исключает обход policy решений через резервный провайдер.
- **Alternatives considered**:
  - Fallback на любой non-2xx: может маскировать клиентские ошибки и policy block decisions.
  - Без fallback: не закрывает ключевой P1 resilience requirement.

## Decision 4: Transformer strategy для Anthropic и Gemini

- **Decision**: Реализовать provider-specific request/response transformers на границе `pokrov-proxy-llm`, сохраняя unified клиентский контракт на входе/выходе.
- **Rationale**: Закрывает multi-provider UX, не ломая публичный API и существующие integration paths.
- **Alternatives considered**:
  - Прокидывать provider-native contracts наружу: ломает drop-in совместимость и увеличивает сложность клиентов.
  - Универсальный "best-effort" transformer без provider профилей: риск semantic drift и нестабильных ответов.

## Decision 5: Native `/v1/responses` passthrough как P2 baseline

- **Decision**: Для `/v1/responses` добавить нативный passthrough режим без обязательной нормализации в chat-completions subset.
- **Rationale**: Убирает функциональные ограничения Codex-like клиентов и снижает контрактные потери на конвертации.
- **Alternatives considered**:
  - Сохранить текущий mapping через chat-completions: не покрывает P2 требование full responses UX.
  - Полный rewrite вокруг Responses-only API: слишком большой blast radius для v1 scope.

## Decision 6: Provider/model-aware rate limits как дополнительный контур

- **Decision**: Ввести отдельные лимиты по provider/model, не заменяя существующие identity/policy budgets.
- **Rationale**: Улучшает защиту mixed-provider workload и предотвращает перекос потребления в одном route.
- **Alternatives considered**:
  - Один глобальный лимит: не различает стоимость и риск разных providers/models.
  - Заменить текущие лимиты новыми: нарушает backward compatibility.

## Decision 7: Observability contract для P0-P2 UX

- **Decision**: Добавить структурированные события и метрики для resolution/fallback/transform/rate-limit, сохраняя metadata-only ограничения.
- **Rationale**: Без этих сигналов сложно диагностировать деградации multi-provider routing.
- **Alternatives considered**:
  - Логировать только итоговый статус: недостаточно для анализа ошибок в fallback/transform цепочке.
  - Добавить raw payload в диагностику: нарушает конституцию и security constraints.

## Decision 8: Scope и конституционные ограничения

- **Decision**: P0+P1+P2 реализуются в рамках существующих crate boundaries и self-hosted v1 без внешнего control plane.
- **Rationale**: Соответствует `constitution.md` и снижает архитектурный риск.
- **Alternatives considered**:
  - Вынести routing/transform rules во внешний orchestrator: противоречит v1 ограничениям.
