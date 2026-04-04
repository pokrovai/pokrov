# Research: Codex Compatibility via Responses Endpoint

## Decision 1: Добавить новый endpoint `POST /v1/responses` как compatibility layer

- **Decision**: Реализовать отдельный `responses` endpoint для Codex, не меняя контракт `POST /v1/chat/completions`.
- **Rationale**: README уже фиксирует mismatch между Codex wire protocol (`responses`) и текущим Pokrov API (`chat/completions`). Отдельный endpoint закрывает gap без breaking changes.
- **Alternatives considered**:
  - Использовать внешний adapter `responses -> chat/completions`: добавляет операционную зависимость и снижает предсказуемость self-hosted rollout.
  - Оставить только docs-warning: не решает интеграцию и не выполняет целевой feature scope.

## Decision 2: Ограничить scope minimal Codex subset (sync + stream)

- **Decision**: Поддержать только минимально необходимый subset `responses` для coding-agent workflows: не-stream и stream, без broad parity по всем optional API families.
- **Rationale**: Соответствует v1 scope-control и принципу minimal surface area; позволяет быстро закрыть основной сценарий без архитектурного дрейфа.
- **Alternatives considered**:
  - Полная Responses API parity: высокий риск scope creep и регрессий в v1.
  - Только sync без stream: ухудшает UX и ломает ожидаемое поведение Codex workflows.

## Decision 3: Сохранить passthrough split-auth boundary в compatibility path

- **Decision**: Для passthrough режима оставить раздельные контуры: gateway auth через `X-Pokrov-Api-Key`, upstream credential через `Authorization: Bearer`.
- **Rationale**: Это уже закреплено в текущем runtime и критично для security boundary: upstream token не должен давать доступ к Pokrov boundary.
- **Alternatives considered**:
  - Single bearer multiplex: создаёт privilege confusion между gateway и upstream auth.
  - Static-only для Codex path: противоречит согласованному BYOK/passthrough default.

## Decision 4: Mapping `responses -> internal chat flow` должен быть детерминированным

- **Decision**: Нормализация `responses` запроса выполняется в стабильный internal LLM payload и использует существующий sanitization/policy pipeline без изменения semantic meaning.
- **Rationale**: Детерминизм нужен для повторяемых решений policy engine, security диагностики и стабильности contract tests.
- **Alternatives considered**:
  - Специальный отдельный policy pipeline для `responses`: повышает риск расхождения поведения между endpoint'ами.
  - Частичный bypass sanitization для stream chunks: нарушает constitition principle I.

## Decision 5: Для нового endpoint сохраняется metadata-only observability

- **Decision**: Логи, аудит и метрики для `POST /v1/responses` должны использовать те же metadata-only инварианты, что и существующий LLM path.
- **Rationale**: Новый compatibility endpoint несёт credential-bearing и sensitive payload риск; observability must stay safe and explainable.
- **Alternatives considered**:
  - Более детализированные логи с частями payload: недопустимый риск leakage.
  - Урезанная observability: затрудняет эксплуатацию и incident triage.

## Decision 6: Error semantics должны быть предсказуемыми и совместимыми с текущей моделью

- **Decision**: Использовать deterministic structured errors для invalid gateway auth, missing upstream credential, policy block и upstream failures.
- **Rationale**: Единая predictable error taxonomy упрощает contract testing и эксплуатацию без раскрытия секретов.
- **Alternatives considered**:
  - Проксирование raw upstream errors без нормализации: нестабильный клиентский контракт и риск leakage.
  - Generic single error: теряется diagnosability для block path и auth stage.

## Decision 7: Out-of-scope явно фиксируется в design artifacts

- **Decision**: Явно исключить MCP parity expansion, A2A/RBAC/SIEM/UI и broad Responses family.
- **Rationale**: Соответствует PRD/Constitution принципу ограниченного v1 scope.
- **Alternatives considered**:
  - Параллельное расширение MCP compatibility: не требуется текущим feature input и увеличивает blast radius.
