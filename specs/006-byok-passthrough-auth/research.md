# Research: BYOK Passthrough Auth

## Decision 1: Поддерживать dual-mode upstream auth (`static` и `passthrough`)

- **Decision**: Ввести явный `upstream_auth_mode` с двумя допустимыми режимами: `static` (текущее поведение) и `passthrough` (клиентский provider credential).
- **Rationale**: Это закрывает multi-client gateway use case и сохраняет обратную совместимость для текущих инсталляций.
- **Alternatives considered**:
  - Только `passthrough`: ломает существующие single-key deployments.
  - Только `static`: не решает BYOK-модель для множества клиентов.

## Decision 2: Разделить контур gateway auth и контур upstream auth

- **Decision**: Gateway auth и upstream auth валидируются независимо и дают независимые причины отказа.
- **Rationale**: Разделение trust boundaries исключает сценарий, где валидный provider key дает доступ к Pokrov без gateway authorization.
- **Alternatives considered**:
  - Использовать один и тот же заголовок/секрет для обеих задач: повышает риск privilege confusion.
  - Полностью передавать auth в ingress и не проверять внутри Pokrov: ухудшает детерминизм и diagnosability.

## Decision 3: Canonical client identity должен быть обязательным входом для policy/rate-limit binding

- **Decision**: Каждому запросу назначается canonical `client_identity`, используемый для profile resolution, rate-limit bucket selection и audit metadata.
- **Rationale**: Изоляция клиентов невозможна без стабильного ключа идентичности на уровне runtime decision path.
- **Alternatives considered**:
  - Использовать только runtime API key profile без identity: не поддерживает multi-tenant изоляцию.
  - Использовать volatile request attributes (например, raw IP): низкая стабильность и плохая воспроизводимость.

## Decision 4: Metadata-only observability распространяется на credential-bearing flows

- **Decision**: Логи/аудит/метрики фиксируют auth mode, identity, decision и status, но не содержат raw credentials и их фрагменты.
- **Rationale**: BYOK увеличивает количество credential-bearing запросов, поэтому запрет на leakage должен быть явным и проверяемым.
- **Alternatives considered**:
  - Redaction в логах по blacklist: риск пропуска новых credential форматов.
  - Удалить подробные observability события: теряется операционная объяснимость.

## Decision 5: Ошибки auth должны быть предсказуемыми и раздельными

- **Decision**: Ошибки gateway auth и upstream credential errors документируются как отдельные error classes с metadata-only envelope.
- **Rationale**: Это обеспечивает повторяемую диагностику клиентов и позволяет тестировать block paths независимо.
- **Alternatives considered**:
  - Единая generic ошибка для всех auth failures: снижает пригодность для поддержки и автоматических проверок.
  - Проксировать raw upstream auth error без нормализации: риск leakage и нестабильный контракт.

## Decision 6: Passthrough режим должен применяться согласованно для LLM и MCP путей

- **Decision**: Дизайн контракта и data model едины для LLM и MCP path с учетом их endpoint-специфики.
- **Rationale**: Консистентность auth/policy модели снижает риски drift между двумя proxy flows.
- **Alternatives considered**:
  - Реализовать только LLM сначала и оставить MCP отдельно без общего контракта: повышает риск несовместимых behavior contracts.

## Decision 7: Performance budget не пересматривается

- **Decision**: Дополнительная auth/identity логика должна оставаться в рамках текущего v1 overhead budget без нового внешнего state store.
- **Rationale**: PRD и конституция задают фиксированный operational budget для runtime.
- **Alternatives considered**:
  - Ввести внешний distributed state для identity/rate-limit на этапе v1: out-of-scope по сложности и операционным зависимостям.
